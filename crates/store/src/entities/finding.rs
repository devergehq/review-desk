//! Findings: what a review found, each numbered once for the whole pull request.
//!
//! A finding's number is its identity across rounds — see [`Finding::seq`] — so the
//! carrying, the attribution and the per-round status live next door in
//! [`super::round`], and this module holds raising one and dispositioning it.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::entities::{composition, finding_evidence};
use crate::error::{Result, StoreError};
use crate::model::{Disposition, DraftStatus, Finding, NewFinding};
use crate::{is_foreign_key_violation, Store};

pub(super) const COLUMNS: &str = "review_id, seq, lens, severity, file, line, claim, title, \
                       evidence, \
                       disposition, disposition_reason, rule_id, github_comment_id, \
                       flagged_line, inline_comment, why_it_matters, proof, proof_note, \
                       verification, suggested_fix, continues_review_id, round_status, \
                       round_status_by, round_status_checked, round_status_at, \
                       round_status_commit, introduced_by_commit, introduced_by_seq, \
                       created_at";

impl Store {
    /// Add a finding to a review, assigning the next sequence number **unused on the pull
    /// request**.
    ///
    /// Not within the review, which is what it used to be. A finding's number is its
    /// identity across rounds — F2 in round 1 is F2 in round 3 — so a number has to mean
    /// one finding on the pull request and not one finding per round. Restarting at 1 each
    /// round was what made "two fixed, two still open, one new" unanswerable.
    ///
    /// For a first round the two are the same number, which is why no review already on
    /// the record is renumbered: the column is untouched and only the *next* allocation
    /// changes.
    ///
    /// Fails with [`StoreError::UnknownParent`] if the review does not exist; the
    /// database enforces that, not this function.
    ///
    /// A finding that names the rule it was raised under also moves that rule's
    /// `last_cited_at`. **That column is stored rather than scanned for** — the choice
    /// S12 asks for — and it cannot drift, because a finding is never deleted, only
    /// dispositioned, so the newest citation only ever moves forward.
    pub fn add_finding(&self, review_id: i64, new: NewFinding) -> Result<Finding> {
        // Checked before the transaction opens, so a finding is never half written: the
        // hunk, its lines, the fix references and the finding itself are one record.
        finding_evidence::validate(&new)?;

        // The read that picks the sequence number and the insert that claims it must
        // be one step. A DEFERRED transaction would take a read lock and then try to
        // upgrade it on the insert, and SQLite refuses that upgrade immediately with
        // "database is locked" rather than waiting out the busy timeout — it cannot
        // wait without risking deadlock. IMMEDIATE takes the write lock up front, so a
        // second caller queues on the busy handler instead of failing.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // Checked inside the transaction, so a rule that is not there is named as the
        // missing parent it is. The foreign key would refuse the row too, but it
        // arrives as one violation among several and the message would blame the
        // review.
        if let Some(rule_id) = new.rule_id {
            if self.find_rule(rule_id)?.is_none() {
                return Err(StoreError::UnknownParent {
                    entity: "finding",
                    parent: "rule",
                    id: rule_id.to_string(),
                });
            }
        }

        // Checked before anything is written, so a commit that is a word rather than an
        // object name is a refusal naming it. The API serves this column as
        // `^[0-9a-f]{7,40}$`, and `--commit HEAD` is a plausible thing to type.
        let introduced_by = super::round::commit_sha_for(
            "introduced_by_commit",
            new.introduced_by_commit.as_deref(),
        )?;

        // Checked inside the transaction, like the rule above, so an attribution naming a
        // finding nobody raised is a refusal that says which number rather than a row
        // pointing at nothing. Scoped to the pull request, because that is the scope a
        // number is an identity in.
        if let Some(introduced_by) = new.introduced_by_seq {
            if !self.pull_request_holds_finding(review_id, introduced_by)? {
                return Err(StoreError::UnknownParent {
                    entity: "finding",
                    parent: "finding it was introduced by",
                    id: introduced_by.to_string(),
                });
            }
        }

        // The highest number used anywhere on this pull request, not on this review. The
        // subquery is the join the identity rests on: `reviews_by_head` indexes
        // `(repo, pr, head_sha)`, so the `(repo, pr)` prefix serves it.
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(f.seq), 0) + 1
             FROM findings f
             JOIN reviews r    ON r.id = f.review_id
             JOIN reviews this ON this.id = ?1
             WHERE r.repo = this.repo AND r.pr = this.pr",
            params![review_id],
            |row| row.get(0),
        )?;

        let created_at = self.now();
        tx.execute(
            "INSERT INTO findings
                 (review_id, seq, lens, severity, file, line, claim, title, evidence,
                  disposition, disposition_reason, rule_id, flagged_line, inline_comment,
                  why_it_matters, proof, proof_note, verification, suggested_fix,
                  introduced_by_commit, introduced_by_seq, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                review_id,
                seq,
                new.lens,
                new.severity,
                new.file,
                new.line,
                new.claim,
                new.title,
                new.evidence,
                new.disposition,
                new.disposition_reason,
                new.rule_id,
                new.flagged_line,
                // Bound, not formatted, and never trimmed. The comment, the consequence,
                // the proof and the fix are stored exactly as the agent wrote them: this
                // is the only copy of them there is.
                new.inline_comment,
                new.why_it_matters,
                new.proof,
                new.proof_note,
                new.verification,
                new.suggested_fix,
                introduced_by,
                new.introduced_by_seq,
                created_at,
            ],
        )
        .map_err(|error| {
            if is_foreign_key_violation(&error) {
                StoreError::UnknownParent {
                    entity: "finding",
                    parent: "review",
                    id: review_id.to_string(),
                }
            } else {
                StoreError::Sqlite(error)
            }
        })?;

        // In the same transaction as the finding, for the reason the module says: a
        // finding stored with half a hunk attached is a worse record than one with none.
        finding_evidence::write(&tx, review_id, seq, &new)?;

        if let Some(rule_id) = new.rule_id {
            // MAX, not an assignment: findings can be added in any order, and a rule's
            // last citation is the newest one it has, not the newest one written.
            tx.execute(
                "UPDATE rules
                 SET last_cited_at = MAX(COALESCE(last_cited_at, 0), ?2),
                     updated_at    = ?2
                 WHERE id = ?1",
                params![rule_id, created_at],
            )?;
        }

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_finding(review_id, seq)
    }

    /// Read one finding. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_finding(&self, review_id: i64, seq: i64) -> Result<Finding> {
        self.find_finding(review_id, seq)?
            .ok_or(StoreError::NotFound {
                entity: "finding",
                id: format!("{review_id}/{seq}"),
            })
    }

    /// Read one finding, with its hunk and fix references, or `None`.
    pub fn find_finding(&self, review_id: i64, seq: i64) -> Result<Option<Finding>> {
        let held = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM findings WHERE review_id = ?1 AND seq = ?2"),
                params![review_id, seq],
                row_to_finding,
            )
            .optional()?;

        match held {
            Some(finding) => Ok(Some(self.with_evidence(finding)?)),
            None => Ok(None),
        }
    }

    /// Every finding of a review, in sequence order, each with its hunk and fix
    /// references.
    pub fn list_findings(&self, review_id: i64) -> Result<Vec<Finding>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM findings WHERE review_id = ?1 ORDER BY seq ASC"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_finding)?;
        let findings = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        self.with_evidence_all(findings)
    }

    /// Change a finding's disposition, with the reason it changed.
    ///
    /// **The body is recomposed with it.** Moving a finding between the draft, the held-back
    /// list and the suppressed one changes what the review posts — the finding's block stops
    /// composing, and since DEV-929 its index row and its thread go with it. Until then this
    /// wrote the column and left `drafts.body` as it was, so the stored body raised a finding
    /// the record said was not in the draft; the threads are composed from the record at read
    /// time, so the two would now disagree with each other as well.
    ///
    /// **A move that changes what posts is refused once the draft is signed off, and a move
    /// that cannot is not.** The refusal is the one a decision earns, and `decision.rs` has
    /// the argument: an approved draft is what the review session posts, and it does not move
    /// underneath it. But a held-back nit moved to suppressed after the sign-off changes
    /// nothing about the body — neither disposition posts — and refusing it would stop the
    /// developer tidying the list they are not posting. So what is checked is the *effect*:
    /// the finding posts now, or it would after the move.
    ///
    /// A review with no draft is the ordinary case of a session holding a finding back before
    /// it has written one, and nothing is recomposed there.
    pub fn set_finding_disposition(
        &self,
        review_id: i64,
        seq: i64,
        disposition: Disposition,
        reason: Option<String>,
    ) -> Result<Finding> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // Checked first, so an unknown finding is NotFound rather than a no-op update.
        let finding = self.get_finding(review_id, seq)?;

        // What posts is the disposition **and** the round status, which is `is_posted`. The
        // move changes the first of those, so the question is whether either side of it is a
        // finding the body carries.
        let changes_what_posts = finding.is_posted()
            || (disposition == Disposition::InDraft && finding.round_status.is_open());

        let draft = self.find_draft(review_id)?;
        if let Some(draft) = &draft {
            // **Approved, and not posted.** What the refusal protects is a body the session
            // is about to post: once it is on GitHub the body is history, the record is
            // allowed to move on, and the harvest does exactly that.
            if changes_what_posts && draft.status == DraftStatus::Approved {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found: draft.status,
                    wanted: "a draft that is drafting, ready or sent back. This move would \
                             change what the review posts, and an approved draft is what the \
                             session posts under the developer's name",
                });
            }
        }

        tx.execute(
            "UPDATE findings SET disposition = ?3, disposition_reason = ?4
             WHERE review_id = ?1 AND seq = ?2",
            params![review_id, seq, disposition, reason],
        )?;

        // **Recomposed when the move changes what posts, and not otherwise.** Two reasons,
        // and the second is the one that is easy to miss. On an approved draft the move that
        // reaches here changes nothing about the body, and recomposing would rewrite the
        // characters the developer signed off for no reason at all. On a draft still being
        // written, a recompose *clears the lint* — which is right when the body changed and
        // wrong when it did not: a held-back finding moved to suppressed composes the same
        // body, and clearing the result would make the record say nobody had checked words
        // that had just been checked.
        if changes_what_posts && draft.is_some_and(|draft| draft.status.is_writable()) {
            composition::recompose(&tx, review_id, composition::Lint::Clear, self.now())?;
        }

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_finding(review_id, seq)
    }

    /// Record the GitHub review comment a finding was posted as.
    ///
    /// Linking the same finding to the same comment again changes nothing, so the
    /// command can be re-run. Linking a comment that another finding already holds is
    /// refused: a comment belongs to one finding, and the harvester would otherwise
    /// record the same reply against two of them.
    pub fn set_finding_github_comment(
        &self,
        review_id: i64,
        seq: i64,
        comment_id: &str,
    ) -> Result<Finding> {
        let comment_id = comment_id.trim();
        if comment_id.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "github_comment_id",
                value: comment_id.to_string(),
                expected: "the id of the GitHub review comment the finding was posted as"
                    .to_string(),
            });
        }

        // Checked first, so an unknown finding is NotFound rather than a no-op update.
        self.get_finding(review_id, seq)?;

        if let Some(holder) = self.find_finding_by_github_comment(comment_id)? {
            if (holder.review_id, holder.seq) != (review_id, seq) {
                return Err(StoreError::DuplicateGithubComment {
                    comment_id: comment_id.to_string(),
                    review_id: holder.review_id,
                    seq: holder.seq,
                });
            }
        }

        self.conn.execute(
            "UPDATE findings SET github_comment_id = ?3 WHERE review_id = ?1 AND seq = ?2",
            params![review_id, seq, comment_id],
        )?;

        self.mark_review_active(review_id)?;
        self.get_finding(review_id, seq)
    }

    /// The finding posted as this GitHub review comment, if any.
    pub fn find_finding_by_github_comment(&self, comment_id: &str) -> Result<Option<Finding>> {
        let held = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM findings WHERE github_comment_id = ?1"),
                params![comment_id],
                row_to_finding,
            )
            .optional()?;

        match held {
            Some(finding) => Ok(Some(self.with_evidence(finding)?)),
            None => Ok(None),
        }
    }
}

/// Build a finding from a row selecting [`COLUMNS`].
pub(super) fn row_to_finding(row: &Row<'_>) -> rusqlite::Result<Finding> {
    Ok(Finding {
        review_id: row.get("review_id")?,
        seq: row.get("seq")?,
        lens: row.get("lens")?,
        severity: row.get("severity")?,
        file: row.get("file")?,
        line: row.get("line")?,
        claim: row.get("claim")?,
        title: row.get("title")?,
        evidence: row.get("evidence")?,
        disposition: row.get("disposition")?,
        disposition_reason: row.get("disposition_reason")?,
        rule_id: row.get("rule_id")?,
        github_comment_id: row.get("github_comment_id")?,
        // Read separately, by the caller that builds the finding: a row builder has no
        // connection to read them through. The same arrangement as a brief's options.
        hunk: None,
        flagged_line: row.get("flagged_line")?,
        inline_comment: row.get("inline_comment")?,
        why_it_matters: row.get("why_it_matters")?,
        proof: row.get("proof")?,
        proof_note: row.get("proof_note")?,
        verification: row.get("verification")?,
        suggested_fix: row.get("suggested_fix")?,
        fix_references: Vec::new(),
        continues_review_id: row.get("continues_review_id")?,
        round_status: row.get("round_status")?,
        round_status_by: row.get("round_status_by")?,
        round_status_checked: row.get("round_status_checked")?,
        round_status_at: row.get("round_status_at")?,
        round_status_commit: row.get("round_status_commit")?,
        introduced_by_commit: row.get("introduced_by_commit")?,
        introduced_by_seq: row.get("introduced_by_seq")?,
        created_at: row.get("created_at")?,
    })
}
