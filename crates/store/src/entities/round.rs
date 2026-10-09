//! Rounds: a finding's identity across them, and what changed between them.
//!
//! A finding raised in round 1 and the same finding looked at again in round 2 used to be
//! unrelated rows, because each round numbered its findings afresh. Three things fix that,
//! and they are all here:
//!
//! | Fact | Where it lives |
//! | -- | -- |
//! | A finding keeps its number across rounds | [`Store::carry_finding`], and the pull-request-wide allocation in [`Store::add_finding`] |
//! | What became of a carried finding in this round | `findings.round_status`, with who decided and how they checked |
//! | What changed between two rounds | `round_commits` and `round_commit_files` |
//!
//! **Three things are derived rather than stored**, and that is deliberate: the round a
//! finding was carried from, how many consecutive rounds it has been open, and whether a
//! new finding is late. Each is answerable from the carry chain and the round's commits,
//! and a stored copy of any of them could disagree with the chain it came from.
//!
//! **Nothing here decides a status.** DEV-873 puts that out of scope: an agent or the
//! developer says what became of a finding and how they checked, and this records it. That
//! is also why no function here signs anything off — saying a finding is fixed is not
//! approving a review, and `DecidedBy::Agent` exists so the record can tell them apart.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    now, Carryable, Finding, FindingCarry, FindingInRound, NewRoundCommit, Review, RoundCommit,
    RoundStatus, RoundStatusCounts, RoundSummary,
};
use crate::{normalise, Store};

impl Store {
    // -- carrying a finding forward -----------------------------------------------

    /// Carry a finding from an earlier round into this one, with what became of it.
    ///
    /// **The number is not an argument, and that is the point.** `seq` names the finding on
    /// the earlier round and the carried row is written with the same number, so "F2 in
    /// round 1 is F2 in round 3" holds by construction. Nothing can carry a finding and
    /// renumber it.
    ///
    /// Carrying the same finding again rewrites the status rather than failing, which is
    /// how a status is changed later — the developer accepting the author's reason on a
    /// finding an agent left `still_open`. The link is rewritten too, so a re-carry from a
    /// different round moves the chain rather than leaving it pointing at the old one.
    ///
    /// Refused when the two reviews are not two rounds of one pull request, and when the
    /// round being carried into is not later than the round carried from. A chain that
    /// could run sideways or backwards would make [`Store::rounds_open`] a loop.
    ///
    /// **The column list below is deliberately explicit, and a test guards it.** The row
    /// this writes differs from the row it reads in exactly the round columns, so there is
    /// no `SELECT *` that would do — which means a column added to `findings` by a later
    /// migration is silently not carried, and a carried finding would quietly lose what it
    /// held. `every_column_of_a_finding_is_decided_about_when_it_is_carried`, in
    /// `crates/store/tests/rounds.rs`, fails when a column has no stated answer. Anything
    /// added to `findings` has to be carried here or named there with its reason.
    ///
    /// `github_comment_id` is the one column deliberately not carried: a round posts its
    /// own comments, and the unique index on that column would refuse this insert if it
    /// were copied.
    pub fn carry_finding(
        &self,
        from_review_id: i64,
        into_review_id: i64,
        seq: i64,
        carry: &FindingCarry,
    ) -> Result<Finding> {
        // The whole thing is one transaction: the checks below are about rows another
        // writer could change, and a carry written against a pull request that has since
        // moved is a link to the wrong round.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        let from = self.get_review(from_review_id)?;
        let into = self.get_review(into_review_id)?;

        if (from.repo.as_str(), from.pr) != (into.repo.as_str(), into.pr) {
            return Err(StoreError::InvalidValue {
                field: "from_review_id",
                value: from_review_id.to_string(),
                expected: format!(
                    "a round of {}#{}; review {} is a round of {}#{}",
                    into.repo, into.pr, from.id, from.repo, from.pr
                ),
            });
        }

        if from.round >= into.round {
            return Err(StoreError::InvalidValue {
                field: "from_review_id",
                value: from_review_id.to_string(),
                expected: format!(
                    "an earlier round than {}; review {} is round {}",
                    into.round, from.id, from.round
                ),
            });
        }

        // Read whole, not just checked: the carried row is the earlier finding's claim and
        // evidence, so a round 2 page can show what was said without reading round 1.
        let earlier = self.get_finding(from_review_id, seq)?;

        let at = now();
        let commit = commit_sha_for("commit", carry.commit.as_deref())?;

        // One statement for "insert the carried finding, or update the one already here".
        // Two statements with a read between them would let a second session carrying the
        // same finding insert between them; the primary key makes the conflict and this
        // resolves it.
        tx.execute(
            "INSERT INTO findings
                 (review_id, seq, lens, severity, file, line, claim, evidence,
                  disposition, disposition_reason, rule_id, introduced_by_commit,
                  introduced_by_seq, continues_review_id, round_status, round_status_by,
                  round_status_checked, round_status_at, round_status_commit, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20)
             ON CONFLICT (review_id, seq) DO UPDATE SET
                 continues_review_id  = ?14,
                 round_status         = ?15,
                 round_status_by      = ?16,
                 round_status_checked = ?17,
                 round_status_at      = ?18,
                 round_status_commit  = ?19",
            params![
                into_review_id,
                seq,
                earlier.lens,
                earlier.severity,
                earlier.file,
                earlier.line,
                earlier.claim,
                earlier.evidence,
                earlier.disposition,
                earlier.disposition_reason,
                earlier.rule_id,
                // Where the finding came from, carried: "introduced by a41c9e2, the
                // commit that fixed F1" stays true in every later round, and the count of
                // findings a fix introduced is read across the whole pull request.
                earlier.introduced_by_commit,
                earlier.introduced_by_seq,
                from_review_id,
                carry.status,
                carry.by,
                trimmed(carry.checked.as_deref()),
                at,
                commit,
                at,
            ],
        )?;

        tx.commit()?;
        self.get_finding(into_review_id, seq)
    }

    /// Change what became of a finding in the round it is already on.
    ///
    /// For a finding raised in this round — which has nothing to carry it from — and for
    /// changing a carried one without naming its earlier round again.
    pub fn set_finding_round_status(
        &self,
        review_id: i64,
        seq: i64,
        carry: &FindingCarry,
    ) -> Result<Finding> {
        // Checked first, so an unknown finding is NotFound rather than a no-op update.
        self.get_finding(review_id, seq)?;

        let at = now();
        self.conn.execute(
            "UPDATE findings SET
                 round_status         = ?3,
                 round_status_by      = ?4,
                 round_status_checked = ?5,
                 round_status_at      = ?6,
                 round_status_commit  = ?7
             WHERE review_id = ?1 AND seq = ?2",
            params![
                review_id,
                seq,
                carry.status,
                carry.by,
                trimmed(carry.checked.as_deref()),
                at,
                commit_sha_for("commit", carry.commit.as_deref())?,
            ],
        )?;

        self.get_finding(review_id, seq)
    }

    /// The previous round's findings, ready to be carried into this one.
    ///
    /// `None` when there is no earlier round — a first round, or a review whose pull
    /// request holds nothing before it.
    ///
    /// Every finding that round held, settled ones included. A finding ticketed in round 1
    /// is not part of round 2, and the page says so in those words; filtering it out here
    /// would take away the fact it needs to.
    pub fn carryable(&self, review_id: i64) -> Result<Option<Carryable>> {
        let Some(previous) = self.previous_round(review_id)? else {
            return Ok(None);
        };

        Ok(Some(Carryable {
            from_review_id: previous.id,
            from_round: previous.round,
            findings: self.list_findings(previous.id)?,
        }))
    }

    /// The round before this one on the same pull request, if there is one.
    ///
    /// The highest round below this review's, not this review's round minus one: a round
    /// can be missing from the record, and "the round before" is still answerable.
    pub fn previous_round(&self, review_id: i64) -> Result<Option<Review>> {
        let review = self.get_review(review_id)?;

        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {columns} FROM reviews
                     WHERE repo = ?1 AND pr = ?2 AND round < ?3
                     ORDER BY round DESC LIMIT 1",
                    columns = super::review::COLUMNS
                ),
                params![review.repo, review.pr, review.round],
                super::review::row_to_review,
            )
            .optional()?)
    }

    // -- what changed between two rounds ------------------------------------------

    /// Record the commits between the previous round's head commit and this round's,
    /// replacing whatever was recorded before.
    ///
    /// Replaced rather than merged, for the reason [`Store::put_draft`] gives: these are
    /// one answer to one question, read from GitHub in one go, and a second read that
    /// merged would leave a commit behind that the pull request no longer has after a
    /// force push.
    ///
    /// **Review Desk reads GitHub and never writes to it.** The reading is the harvest
    /// crate's; this stores what it read.
    pub fn put_round_commits(
        &self,
        review_id: i64,
        commits: &[NewRoundCommit],
    ) -> Result<Vec<RoundCommit>> {
        // Validated in full before anything is written, so a bad commit halfway down the
        // list is a refusal that left the previous answer intact rather than half of a new
        // one.
        let prepared = commits
            .iter()
            .map(prepare_commit)
            .collect::<Result<Vec<_>>>()?;

        let mut seen = BTreeSet::new();
        for commit in &prepared {
            if !seen.insert(commit.sha.clone()) {
                return Err(StoreError::InvalidValue {
                    field: "sha",
                    value: commit.sha.clone(),
                    expected: "each commit once; the same commit was given twice".to_string(),
                });
            }
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        // Checked inside the transaction, so a review deleted under us is a refusal rather
        // than rows that cascade away a moment later.
        self.get_review(review_id)?;

        // The files cascade with their commit, so one delete clears both tables.
        tx.execute(
            "DELETE FROM round_commits WHERE review_id = ?1",
            params![review_id],
        )?;

        for (position, commit) in prepared.iter().enumerate() {
            tx.execute(
                "INSERT INTO round_commits (review_id, sha, position, summary, committed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    review_id,
                    commit.sha,
                    position as i64,
                    commit.summary,
                    commit.committed_at,
                ],
            )?;

            for path in &commit.files {
                tx.execute(
                    "INSERT INTO round_commit_files (review_id, sha, path)
                     VALUES (?1, ?2, ?3)",
                    params![review_id, commit.sha, path],
                )?;
            }
        }

        tx.commit()?;
        self.list_round_commits(review_id)
    }

    /// The commits recorded between the previous round and this one, oldest first.
    ///
    /// An empty list means none have been recorded, which is not the same as "nothing
    /// changed" — [`FindingInRound::late`] is null rather than false for exactly that
    /// reason.
    pub fn list_round_commits(&self, review_id: i64) -> Result<Vec<RoundCommit>> {
        let mut commits = {
            let mut statement = self.conn.prepare(
                "SELECT review_id, sha, position, summary, committed_at
                 FROM round_commits WHERE review_id = ?1 ORDER BY position ASC",
            )?;
            let rows = statement.query_map(params![review_id], |row| {
                Ok(RoundCommit {
                    review_id: row.get("review_id")?,
                    sha: row.get("sha")?,
                    position: row.get("position")?,
                    summary: row.get("summary")?,
                    committed_at: row.get("committed_at")?,
                    files: Vec::new(),
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        // One query for every commit's files rather than one per commit: a round with
        // twenty commits is a round with twenty-one queries otherwise, and this is read on
        // every load of the page.
        let mut files: HashMap<String, Vec<String>> = HashMap::new();
        {
            let mut statement = self.conn.prepare(
                "SELECT sha, path FROM round_commit_files
                 WHERE review_id = ?1 ORDER BY sha ASC, path ASC",
            )?;
            let rows = statement.query_map(params![review_id], |row| {
                Ok((row.get::<_, String>("sha")?, row.get::<_, String>("path")?))
            })?;
            for row in rows {
                let (sha, path) = row?;
                files.entry(sha).or_default().push(path);
            }
        }

        for commit in &mut commits {
            commit.files = files.remove(&commit.sha).unwrap_or_default();
        }

        Ok(commits)
    }

    // -- the rounds timeline -------------------------------------------------------

    /// Every round of a pull request, oldest first, with what each one posted and what came
    /// back.
    ///
    /// The timeline across the top of the round-two page. Five counts per round —
    /// findings, commits, replies, threads resolved, and the findings by status — each
    /// gathered in one grouped query rather than one query per round, so a pull request on
    /// its fifth round costs the same number of reads as one on its first.
    pub fn rounds(&self, repo: &str, pr: i64) -> Result<Vec<RoundSummary>> {
        let reviews = self.list_reviews(&normalise::repo(repo), pr)?;
        if reviews.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<i64> = reviews.iter().map(|review| review.id).collect();

        let commits = self.count_by_review("round_commits", None, &ids)?;
        let replies = self.count_by_review("outcomes", Some("replied"), &ids)?;
        let resolved = self.count_by_review("outcomes", Some("thread_resolved"), &ids)?;

        // The statuses and the finding count come from one pass over the findings, because
        // the count is the sum of the statuses and reading it twice could disagree with
        // itself.
        let mut statuses: HashMap<i64, RoundStatusCounts> = HashMap::new();
        let mut findings: HashMap<i64, i64> = HashMap::new();
        {
            let list = ids
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let mut statement = self.conn.prepare(&format!(
                "SELECT review_id, round_status FROM findings WHERE review_id IN ({list})"
            ))?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>("review_id")?,
                    row.get::<_, RoundStatus>("round_status")?,
                ))
            })?;
            for row in rows {
                let (review_id, status) = row?;
                statuses.entry(review_id).or_default().add(status);
                *findings.entry(review_id).or_default() += 1;
            }
        }

        let mut summaries = Vec::with_capacity(reviews.len());
        for review in reviews {
            // The draft is read one review at a time. It is one row per review by primary
            // key, and a draft is the one thing here that a round may not have at all.
            let draft = self.find_draft(review.id)?;

            summaries.push(RoundSummary {
                review_id: review.id,
                round: review.round,
                head_sha: review.head_sha,
                created_at: review.created_at,
                posted_at: draft.as_ref().and_then(|draft| draft.posted_at),
                event: draft.as_ref().and_then(|draft| draft.event),
                findings: findings.get(&review.id).copied().unwrap_or(0),
                commits: commits.get(&review.id).copied().unwrap_or(0),
                replies: replies.get(&review.id).copied().unwrap_or(0),
                threads_resolved: resolved.get(&review.id).copied().unwrap_or(0),
                statuses: statuses.get(&review.id).copied().unwrap_or_default(),
            });
        }

        Ok(summaries)
    }

    /// Count rows per review, for the reviews named.
    ///
    /// **The id list is formatted into the statement rather than bound**, because SQLite
    /// takes one parameter per element of an `IN` list and the length here is a count of
    /// rounds. The ids come from `list_reviews` — rows this database just handed back, not
    /// anything a caller typed — so there is nothing to inject.
    fn count_by_review(
        &self,
        table: &str,
        kind: Option<&str>,
        ids: &[i64],
    ) -> Result<HashMap<i64, i64>> {
        let list = ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let filter = kind.map_or(String::new(), |_| " AND kind = ?1".to_string());

        let mut statement = self.conn.prepare(&format!(
            "SELECT review_id, COUNT(*) FROM {table}
             WHERE review_id IN ({list}){filter}
             GROUP BY review_id"
        ))?;
        let rows = match kind {
            Some(kind) => statement.query(params![kind])?,
            None => statement.query([])?,
        };

        let mut counts = HashMap::new();
        let mut rows = rows;
        while let Some(row) = rows.next()? {
            counts.insert(row.get(0)?, row.get(1)?);
        }
        Ok(counts)
    }

    // -- the three derived facts ---------------------------------------------------

    /// Every finding of a round, with the round it was carried from, how many consecutive
    /// rounds it has been open, and whether it is late.
    ///
    /// One call rather than three per finding, because this is what a page load reads.
    pub fn findings_in_round(&self, review_id: i64) -> Result<Vec<FindingInRound>> {
        let review = self.get_review(review_id)?;
        let findings = self.list_findings(review_id)?;
        let commits = self.list_round_commits(review_id)?;

        // The files this round's commits touched, gathered once. The `late` question is
        // asked of every new finding, and asking the database each time would be a query
        // per finding for one set that does not change.
        let changed: BTreeSet<&str> = commits
            .iter()
            .flat_map(|commit| commit.files.iter().map(String::as_str))
            .collect();

        findings
            .into_iter()
            .map(|finding| {
                let continues_round = match finding.continues_review_id {
                    Some(id) => Some(self.get_review(id)?.round),
                    None => None,
                };

                Ok(FindingInRound {
                    late: late(&review, &finding, &changed),
                    rounds_open: self.rounds_open(review_id, finding.seq)?,
                    continues_round,
                    finding,
                })
            })
            .collect()
    }

    /// How many consecutive rounds a finding has been open, counting this one.
    ///
    /// 0 when it is not open now: "open for 2 rounds" is a sentence about a finding that
    /// still is, and a fixed finding counting the rounds it used to be open for would read
    /// as a complaint.
    ///
    /// The walk follows `continues_review_id` back and stops at the first round where the
    /// finding was not open, or where the chain ends. It is bounded by the round number —
    /// each step moves to a strictly earlier round, which [`Store::carry_finding`] refuses
    /// to let a chain break — so a cycle written by hand into the database cannot spin it.
    pub fn rounds_open(&self, review_id: i64, seq: i64) -> Result<i64> {
        let mut finding = self.get_finding(review_id, seq)?;
        if !finding.round_status.is_open() {
            return Ok(0);
        }

        let mut rounds = 1;
        let mut round = self.get_review(review_id)?.round;

        while let Some(earlier_id) = finding.continues_review_id {
            let earlier_round = self.get_review(earlier_id)?.round;
            if earlier_round >= round {
                // Not reachable through `carry_finding`, which refuses a chain that does
                // not run backwards. A database edited by hand is the case this is for.
                break;
            }
            round = earlier_round;

            let Some(earlier) = self.find_finding(earlier_id, seq)? else {
                break;
            };
            if !earlier.round_status.is_open() {
                break;
            }

            rounds += 1;
            finding = earlier;
        }

        Ok(rounds)
    }

    /// Whether this pull request holds a finding with this number, in any round.
    ///
    /// The scope a number is an identity in, which is what lets an attribution name a
    /// finding by number alone.
    pub(super) fn pull_request_holds_finding(&self, review_id: i64, seq: i64) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT f.seq FROM findings f
                 JOIN reviews r    ON r.id = f.review_id
                 JOIN reviews this ON this.id = ?1
                 WHERE r.repo = this.repo AND r.pr = this.pr AND f.seq = ?2
                 LIMIT 1",
                params![review_id, seq],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }
}

/// Whether a new finding sits on code the round's commits did not touch.
///
/// Four reasons the answer is `None`, and they are all "the question cannot be answered"
/// rather than "no":
///
/// - a first round has nothing to be late against;
/// - a finding carried from an earlier round was not raised in this one;
/// - a finding with no file names no code to compare;
/// - a round where nothing is known about what the commits touched.
///
/// Reporting `false` for any of them would read as "this was not missed", which is a claim
/// nothing here is in a position to make.
///
/// **Two of those four were wrong first time, and both were found by reviewing this
/// change.**
///
/// The second used to read `round_status != New`. A status is a separate, mutable axis:
/// accepting the author's reason on a finding raised in round 2 flipped its lateness to
/// `None` and lost the one fact DEV-873 added about it. What says "raised in this round" is
/// `continues_review_id` being absent, and nothing else does.
///
/// The fourth used to be "no commits recorded". A round's commits can be recorded with no
/// file lists — a GitHub read that failed partway — and `changed` is then empty, so every
/// new finding read as late. The question is whether anything is *known* about what
/// changed, not whether a commit list exists.
fn late(review: &Review, finding: &Finding, changed: &BTreeSet<&str>) -> Option<bool> {
    if review.round < 2 || finding.continues_review_id.is_some() || changed.is_empty() {
        return None;
    }

    let file = finding.file.as_deref()?;
    Some(!changed.contains(file))
}

/// A commit checked and put in the form the record keys on.
fn prepare_commit(commit: &NewRoundCommit) -> Result<NewRoundCommit> {
    let sha = commit_sha_for("sha", Some(&commit.sha))?.ok_or(StoreError::InvalidValue {
        field: "sha",
        value: commit.sha.clone(),
        expected: "the commit, as GitHub gave it".to_string(),
    })?;

    // The first line, and only the first. GitHub sends the whole message; "a one-line
    // summary" is what the page shows, and storing the body would put a paragraph in a
    // table cell.
    let summary = commit
        .summary
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if summary.is_empty() {
        return Err(StoreError::InvalidValue {
            field: "summary",
            value: commit.summary.clone(),
            expected: "the commit message's first line".to_string(),
        });
    }

    let mut files: Vec<String> = commit
        .files
        .iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect();
    files.sort();
    files.dedup();

    Ok(NewRoundCommit {
        sha,
        summary,
        committed_at: commit.committed_at,
        files,
    })
}

/// A note as the record holds it: trimmed, and nothing rather than blank.
///
/// A `--checked ""` is a caller who gave no note, and storing the empty string would make
/// "how it was checked" a field that is present and says nothing.
fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A commit a record points at, canonical and checked, or nothing.
///
/// **Checked rather than merely tidied**, because these are typed: `--commit HEAD` and a
/// four-character abbreviation are both plausible, and the API serves the column as
/// `^[0-9a-f]{7,40}$` — so an unchecked value is a response that fails its own contract.
/// Found by reviewing DEV-873 rather than by a test, which is why there is one now.
pub(super) fn commit_sha_for(field: &'static str, sha: Option<&str>) -> Result<Option<String>> {
    let Some(sha) = normalise::optional_commit(sha) else {
        return Ok(None);
    };

    if !normalise::is_commit_sha(&sha) {
        return Err(StoreError::InvalidValue {
            field,
            value: sha,
            expected: "a commit as 7 to 40 hexadecimal characters, as git abbreviates one"
                .to_string(),
        });
    }

    Ok(Some(sha))
}
