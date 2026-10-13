//! Reviews: one per pull request head commit per round.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    DiffAtHead, Disposition, DraftStatus, HeadUpdate, HeadUpdateKind, HeadUpdates, NewReview,
    NitTicket, NitTicketStatus, NotForSigning, OpenedAction, OpenedReview, OutOfPlay, PrState,
    Review, ReviewFacts, ReviewLayerState, ReviewLinks, ReviewWork, Timestamp,
};
use crate::{is_unique_violation, normalise, Store};

pub(super) const COLUMNS: &str = "id, repo, pr, head_sha, round, title, ticket_id, \
                       ticket_url, changed_lines, \
                       not_checked, session_id, ledger_id, github_review_id, pr_state, \
                       pr_state_at, harvested_at, nit_ticket_status, nit_ticket_id, \
                       nit_ticket_requested_at, nit_ticket_filed_at, dismissed_at, \
                       dismissed_reason, out_of_play, out_of_play_at, last_activity_at, \
                       created_at";

impl Store {
    /// Create a review.
    ///
    /// A review is unique on repository, pull request and round; a second one fails
    /// with [`StoreError::DuplicateReview`] rather than an opaque SQLite error.
    pub fn create_review(&self, new: NewReview) -> Result<Review> {
        let new = validate(new)?;

        let created_at = self.now();
        self.conn
            .execute(
                // `last_activity_at` is the creation itself: opening a review is a write, and
                // the first one a session makes. Set here rather than by a second statement so
                // that a review never exists with its activity unset.
                "INSERT INTO reviews
                     (repo, pr, head_sha, round, title, ticket_id, ticket_url,
                      changed_lines, session_id, ledger_id, github_review_id,
                      last_activity_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                params![
                    new.repo,
                    new.pr,
                    new.head_sha,
                    new.round,
                    new.title,
                    new.ticket_id,
                    new.ticket_url,
                    new.changed_lines,
                    new.session_id,
                    new.ledger_id,
                    new.github_review_id,
                    created_at,
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error) {
                    StoreError::DuplicateReview {
                        repo: new.repo.clone(),
                        pr: new.pr,
                        round: new.round,
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        self.get_review(self.conn.last_insert_rowid())
    }

    /// Open the review of this head commit: the one already on the record, the unposted
    /// round whose head moves to it, or a new round.
    ///
    /// A head commit already reviewed is handed back — the newest round for that commit,
    /// even when later rounds exist for other commits.
    ///
    /// **A round is a posted review, so a new commit opens a round only when the newest
    /// round has posted.** Patrick, 13 October 2026, on DEV-962: "it shouldn't create a
    /// whole new record because that doesn't make any sense… If changes happen after our
    /// post, that's a new review. That's another round." Before this, his queue held one
    /// pull request twice — round 1 and round 2, both offering "changes since your last
    /// review" — over a round 1 he had never posted. So an unposted newest round *moves*,
    /// whoever calls it, and the move is recorded: [`Store::head_updates`] is what lets the
    /// page say "Updated for 3 commits since `4d9464f`" and the code review show what
    /// changed since the draft was written.
    ///
    /// **Whoever calls it, including over a session at work — and that is the reversal
    /// DEV-962 asks for.** DEV-945 refused to move a round anybody had *started*, on the
    /// ground that a brief computed from one commit must not end up on a review the record
    /// says is of another. That protection bought a second unposted round instead, which is
    /// the fault this issue was raised from, and it bought it for every push: the dispatcher
    /// records a request before any session runs, and the author pushes while the session
    /// reads. What replaces it is the record of the move. The session re-reads the delta and
    /// updates its findings and its draft; the review page says the head moved and for
    /// which commits; and a sign-off that was given before the move is withdrawn, because
    /// the developer approved different code.
    ///
    /// **`--session` still matters and no longer protects.** A session passing its id on
    /// its first call is how the record knows whose review it is — which is what
    /// [`Store::review_started`] answers for a dispatcher's look-before-write guard, and
    /// what DEV-954 added the argument for. What it no longer does is pin the head, because
    /// pinning the head is the behaviour this issue removes.
    ///
    /// **Commits that only bring the base branch in change nothing**, which is
    /// [`DiffAtHead::Unchanged`] and the third row of DEV-962's table. The head does not
    /// move, no round is opened, the line count is left alone, nothing is marked active and
    /// no sign-off is withdrawn; a `base_only` row records that they arrived. The caller
    /// says so, because the store reads neither git nor GitHub.
    ///
    /// **A dismissed round's head moves too, and it stays dismissed.** It is still one
    /// request, and the developer has said they do not want it reviewed; starting a fresh
    /// round instead would put the request they declined back in front of a session on the
    /// author's next push. DEV-945 puts automatic dismissal out of scope, and automatic
    /// *un*-dismissal is the same decision read backwards.
    ///
    /// **The cost of that, said out loud:** a dismissed request stays dismissed however
    /// many commits arrive, because each one moves its head and no round is ever opened
    /// that would be in play. For "I am not reviewing this pull request" that is the
    /// point. For a dismissal whose reason was about one commit — "superseded, waiting
    /// for the rewrite" — the rewrite lands and nothing happens, and the way back is the
    /// Reviews list's dismissed view. Read as a fault by a review of DEV-945 and kept
    /// deliberately: the alternative is Review Desk deciding that a push means the
    /// developer changed their mind.
    ///
    /// **A pull request out of play stays out of play, and a round opened on one inherits
    /// it.** The author pulling the pull request back is not something a push undoes, and a
    /// round that did not inherit the state would put the request back in front of a session
    /// the moment a post was followed by a commit. `review in-play` is the way back, and it
    /// clears every round at once.
    ///
    /// The round that moved comes back with `created: false`,
    /// [`OpenedReview::head_moved_from`] set and [`OpenedAction::HeadMoved`], so a caller
    /// can say which of the four things happened without comparing commits itself.
    ///
    /// The lookup, the round arithmetic and the insert are one step, for the reason
    /// [`Store::add_finding`] gives: two sessions opening the same pull request at the
    /// same time on two different new commits would otherwise both read the same latest
    /// round, and the second would be refused as a duplicate of a round it never asked
    /// for.
    pub fn open_review(&self, repo: &str, pr: i64, head_sha: &str) -> Result<OpenedReview> {
        self.open_round(repo, pr, head_sha, DiffAtHead::Unsaid)
    }

    /// [`Store::open_review`], with what the caller knows about the diff.
    ///
    /// Private because the two public doors are [`Store::open_review`], which says nothing
    /// about the diff, and [`Store::open_review_with`], which is the command line's and the
    /// API's and carries both the facts and what was said about the diff. A third public
    /// name taking only the diff would be a door onto the same room.
    fn open_round(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
        diff: DiffAtHead,
    ) -> Result<OpenedReview> {
        // Normalised before the lookup, through the same helper `create_review` uses
        // before the insert. A lookup keyed differently from the insert would miss and
        // then start a round for a commit already reviewed.
        let (repo, head_sha) = (normalise::repo(repo), normalise::head_sha(head_sha));

        // **Refused here, because two of the paths below do not reach `validate`.**
        // Creating a round goes through `create_review`, which has refused a blank head
        // since the first slice; *moving* one is an `UPDATE` of this column and went
        // nowhere near it, and recording a base-only update is an `INSERT` into another
        // table. So `review open --head "$HEAD"` from a shell whose variable is unset wiped
        // the commit the review was of — on a request nobody had started, which is exactly
        // the row this feature is built for — and left a `head_sha` the contract's own
        // pattern refuses, so `GET /reviews` then served a body failing its schema. Driven
        // against the binary by a review of DEV-945. The same class as `--ticket ""`, which
        // that branch repaired twice elsewhere.
        if head_sha.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "head_sha",
                value: String::new(),
                expected: "the head commit being reviewed".to_string(),
            });
        }

        // Looked for before any lock is taken. Reopening a commit already reviewed is
        // the common call — every session's first one — and putting it behind the write
        // lock would make every reopen queue behind every writer. Worse, losing that
        // queue for the whole busy timeout arrives as `StoreError::Sqlite`, which
        // `is_refusal` calls false, so an ordinary reopen under contention would exit 2:
        // the code the review worker treats as a loud warning.
        if let Some(review) = self.find_review_by_head(&repo, pr, &head_sha)? {
            return Ok(OpenedReview {
                review,
                created: false,
                head_moved_from: None,
                action: OpenedAction::Unchanged,
            });
        }

        // Only a commit with no review needs the write lock, and then the round
        // arithmetic and the insert have to be one step: two sessions opening the same
        // pull request on two different new commits would otherwise both read the same
        // latest round, and the loser would be refused as a duplicate of a round it
        // never asked for. IMMEDIATE for the reason `add_finding` gives — SQLite will
        // not upgrade a read lock, so a DEFERRED transaction fails outright instead of
        // waiting out the busy timeout.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // Checked again now the lock is held. The miss above was unsynchronised, so
        // another caller may have created this very review in between.
        if let Some(review) = self.find_review_by_head(&repo, pr, &head_sha)? {
            tx.commit()?;
            return Ok(OpenedReview {
                review,
                created: false,
                head_moved_from: None,
                action: OpenedAction::Unchanged,
            });
        }

        let newest = self.newest_round(&repo, pr)?;
        // **Kept before the branches consume `newest`.** Out of play is a fact about the
        // pull request, so a round opened while the request is out of play is out of play
        // too — see the inherit below.
        let out_of_play = newest
            .as_ref()
            .and_then(|newest| newest.out_of_play.map(|why| (why, newest.out_of_play_at)));

        // **The base-only case first, because it is the one that does nothing.** It is
        // recorded against the newest round whether that round has posted or not: after a
        // post, DEV-962's table says the same word — nothing — and a row saying the base
        // branch arrived is true either way. With no round at all there is nothing for the
        // commits to leave unchanged, so the call falls through and opens the first round:
        // a first review has to exist before anything can be said to be unchanged about it.
        if let (DiffAtHead::Unchanged, Some(newest)) = (diff, newest.as_ref()) {
            // **Said twice is said once, because the caller is a poller.** Every other path
            // through this function dedupes on the head commit — a commit already reviewed
            // is found and handed back — and this one cannot, because it deliberately does
            // not move `head_sha` onto the commit it is about. So a dispatcher that keeps
            // asserting "the base branch came in and changed nothing" every tick, which is
            // exactly the caller this flag is for, would add a row each time and the page
            // would read "12 pushes brought the base branch in" for one push. Found by a
            // review of this branch. `set_pr_out_of_play` is idempotent for the same caller
            // and for the same reason; this is that rule applied to the other half.
            self.record_base_only_once(newest.id, &newest.head_sha, &head_sha)?;
            let review = self.get_review(newest.id)?;
            tx.commit()?;
            return Ok(OpenedReview {
                review,
                created: false,
                head_moved_from: None,
                action: OpenedAction::DiffUnchanged,
            });
        }

        // The newest round, when it has not posted: this commit is where that request is
        // now, so the round moves to it rather than a second one opening beside it. Inside
        // the write lock, which is what makes two callers arriving on two different commits
        // end with one review rather than two — the second finds the round the first moved
        // and moves it again.
        // **A failed read is not an answer of "no".** `.unwrap_or(false)` was here, and it
        // would have moved the head of a round that *had* posted whenever the read behind
        // this question failed — which is the one case this rule must not get wrong, because
        // the move cannot be undone and the round it moved was a review that happened. The
        // question is asked explicitly, and the error is returned.
        let unposted = match &newest {
            Some(newest) if !self.review_is_posted(newest.id)? => Some(newest.clone()),
            _ => None,
        };
        if let Some(unposted) = unposted {
            let moved_from = unposted.head_sha.clone();
            // **The line count goes with the commit it was counted on.** It is additions
            // plus deletions of the change as it stood, and `POST /reviews` says in its
            // own comment that it is what the draft's word budget is computed from — so a
            // figure measured on the commit this review has just stopped being of is a
            // wrong answer rather than an old one. Cleared rather than recomputed,
            // because the store reads no GitHub: the next caller that knows writes it,
            // and `POST /reviews` fills it from the read it makes anyway.
            //
            // The title is left alone. It is a fact about the pull request rather than
            // about the commit, and a caller that has a newer one passes it.
            //
            // **And nothing else is cleared, which is a decision rather than an
            // oversight.** The brief, the findings, the per-file change map and this
            // round's commits were all written against the commit the review has just
            // stopped being of, and every one of them is work a session will rewrite when
            // it re-reads the delta — so clearing them would destroy the review in order
            // to protect it from being out of date. The line count is the one exception
            // because it is a bare number nothing can tell is stale and the next ordinary
            // call refills it; the rest are measurements whose staleness the page states,
            // in the sentence `head_updates` is for.
            self.conn.execute(
                "UPDATE reviews SET head_sha = ?2, changed_lines = NULL WHERE id = ?1",
                params![unposted.id, head_sha],
            )?;
            let moved = self.record_head_update(
                unposted.id,
                HeadUpdateKind::Moved,
                &moved_from,
                &head_sha,
            )?;
            // **A sign-off given before the move is withdrawn.** DEV-962's criterion: the
            // developer approved different code, so the record stops saying they approved
            // this. Inside the same transaction as the move, because a head that moved over
            // a draft still marked `approved` is a draft a session may post.
            //
            // **Stamped with the move's own time, not a second reading of the clock.** The
            // code review shows the moves since the draft was last written, by comparing the
            // draft's `updated_at` with each move's `at` — and the withdrawal *is* a write to
            // the draft, so two clock reads straddling a second boundary would date the draft
            // one second after the move that caused it and the page would go silent about it.
            // Microseconds of window per second, and free to close: the row this just wrote
            // carries the time. Found by a review of the fix that made that comparison
            // inclusive.
            self.withdraw_signoff(unposted.id, &moved_from, moved.at)?;
            self.mark_review_active(unposted.id)?;
            let review = self.get_review(unposted.id)?;
            tx.commit()?;
            return Ok(OpenedReview {
                review,
                created: false,
                head_moved_from: Some(moved_from),
                action: OpenedAction::HeadMoved,
            });
        }

        let round = self.latest_round(&repo, pr)?.unwrap_or(0) + 1;
        let review = self.create_review(NewReview {
            repo,
            pr,
            head_sha,
            round,
            ..Default::default()
        })?;

        // **A round of a pull request that is out of play is out of play.** Migration 19's
        // argument, applied to the one case that could break it: the state is the *pull
        // request's*, and a round opened without it would make "no round holds one" false
        // the moment a post was followed by a push — the queue would then offer a session a
        // round of a pull request the author has pulled back, which is the thing the column
        // exists to stop. The time comes with it rather than being taken now: this round is
        // new, but the request has been out of play since the caller said so.
        //
        // The caller that polls the pull request need not know the rule. It keeps saying
        // what GitHub shows it, and `review in-play` clears every round at once.
        let review = match out_of_play {
            Some((why, at)) => {
                self.conn.execute(
                    "UPDATE reviews SET out_of_play = ?2, out_of_play_at = ?3 WHERE id = ?1",
                    params![review.id, why, at],
                )?;
                self.get_review(review.id)?
            }
            None => review,
        };

        tx.commit()?;
        Ok(OpenedReview {
            review,
            created: true,
            head_moved_from: None,
            action: OpenedAction::Created,
        })
    }

    /// The newest round of this pull request, whatever became of it.
    ///
    /// `None` only when the pull request has no review at all. A dismissed round counts, for
    /// the reason [`Store::open_review`] gives — it is still the request — and so does a
    /// finished one: a merged pull request that somehow gains a commit moves the round it
    /// has rather than opening one nothing can post.
    ///
    /// **It counts every round rather than the newest in play**, which is the rule
    /// `NEWEST_ROUND` states for the queue and it holds for the same reason: a round number
    /// that fell back to an earlier round the moment the newest one was dismissed would make
    /// the dismissed round's own head the one that moves next.
    pub(crate) fn newest_round(&self, repo: &str, pr: i64) -> Result<Option<Review>> {
        // Its callers normalise already; the two lookups below do it again for themselves,
        // which is the rule this crate keeps rather than an argument about who called whom.
        let Some(round) = self.latest_round(repo, pr)? else {
            return Ok(None);
        };
        self.find_review(repo, pr, round)
    }

    /// Whether this round has posted: its draft was reported posted, or a review was linked
    /// against it with no draft at all.
    ///
    /// **One definition, read by both rules that turn on it.** [`Store::open_review`] asks
    /// it to decide whether a new commit is this round or the next one, and
    /// [`Store::not_for_signing`] asks it to decide whether a newer round supersedes this
    /// one. Two spellings of "posted" would let the two disagree, and the disagreement would
    /// be a round that can neither move nor be signed off.
    pub fn review_is_posted(&self, review_id: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                     SELECT 1 FROM drafts
                     WHERE review_id = ?1 AND posted_at IS NOT NULL
                 )
                 OR EXISTS (
                     SELECT 1 FROM reviews
                     WHERE id = ?1 AND github_review_id IS NOT NULL
                 )",
            params![review_id],
            |row| row.get(0),
        )?)
    }

    /// Record that the base branch came in, unless this round already says so about these
    /// two commits.
    ///
    /// **Idempotent, because what writes it is a poll.** The counts this feeds are what the
    /// page says out loud — "2 pushes brought the base branch in and changed nothing" — so a
    /// repeated assertion of one fact has to stay one row. The pair is the whole identity:
    /// the commit the round is of and the commit the caller is reporting, which is what one
    /// base-branch merge is. A *second* base merge reports a different `to_sha` and is a
    /// second row, which is right.
    pub(crate) fn record_base_only_once(
        &self,
        review_id: i64,
        from_sha: &str,
        to_sha: &str,
    ) -> Result<()> {
        let (from_sha, to_sha) = (normalise::head_sha(from_sha), normalise::head_sha(to_sha));
        let said: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM round_head_updates
                  WHERE review_id = ?1 AND kind = ?2 AND from_sha = ?3 AND to_sha = ?4
             )",
            params![review_id, HeadUpdateKind::BaseOnly, from_sha, to_sha],
            |row| row.get(0),
        )?;
        if said {
            return Ok(());
        }

        self.record_head_update(review_id, HeadUpdateKind::BaseOnly, &from_sha, &to_sha)?;
        Ok(())
    }

    /// Record one thing that happened to a round's head.
    ///
    /// Append-only, and the caller is inside a transaction with whatever the row is about —
    /// a move and the row saying it moved are one fact, and a record holding one without the
    /// other is the state this table was added to stop.
    pub(crate) fn record_head_update(
        &self,
        review_id: i64,
        kind: HeadUpdateKind,
        from_sha: &str,
        to_sha: &str,
    ) -> Result<HeadUpdate> {
        // Normalised through the same helper every head goes through, so the CHECK that
        // refuses a move from a commit to itself refuses a real mistake rather than two
        // spellings of one commit.
        let (from_sha, to_sha) = (normalise::head_sha(from_sha), normalise::head_sha(to_sha));
        let at = self.now();
        self.conn.execute(
            "INSERT INTO round_head_updates (review_id, kind, from_sha, to_sha, at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![review_id, kind, from_sha, to_sha, at],
        )?;
        Ok(HeadUpdate {
            id: self.conn.last_insert_rowid(),
            review_id,
            kind,
            from_sha,
            to_sha,
            at,
        })
    }

    /// Whether anybody has begun on this review: a session that said so, or anything only a
    /// session writes.
    ///
    /// **Anything a session writes about the commit counts, and the point is the ones
    /// that are not work a delete would lose.** A session linked by
    /// `review link --session` or `review open --session`; a layer moved off `pending`;
    /// what the review did not look at; and the things `provenance` and `round commits`
    /// record — the per-file change map, the sources and criteria its understanding came
    /// from, the description lint's results, the commits since the previous round. Every one
    /// of them is a session at work, and none of them makes [`ReviewWork::started`] true.
    ///
    /// They are *claimed* rather than *held*: a delete still removes a review carrying
    /// only these, because none of them is work the developer would lose — which is what
    /// `deleting_a_review_nobody_started_takes_every_row_under_it` seeds and asserts.
    ///
    /// A request recorded by the dispatcher has none of them: `review open` writes no
    /// layer and no provenance, and the ledger id it may carry is the dispatcher's own
    /// entry rather than a session.
    ///
    /// **What this no longer decides is whether a head may move.** It was DEV-945's guard
    /// against moving a round out from under a running session; DEV-962 moves an unposted
    /// round's head whoever is working on it and records the move instead, for the reason
    /// [`Store::open_review`] gives. What the question is still for is telling a caller
    /// outside the record whether anybody has begun — [`Store::review_started`], which
    /// DEV-954 asks for so that a dispatcher's guard does not have to infer it from
    /// `session_id`.
    fn review_is_claimed(&self, review: &Review) -> Result<bool> {
        if review.session_id.is_some() || review.not_checked.is_some() {
            return Ok(true);
        }
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                     SELECT 1 FROM review_layers WHERE review_id = ?1 AND state <> ?2
                 )
                 OR EXISTS (SELECT 1 FROM review_files WHERE review_id = ?1)
                 OR EXISTS (SELECT 1 FROM review_sources WHERE review_id = ?1)
                 OR EXISTS (SELECT 1 FROM review_criteria WHERE review_id = ?1)
                 OR EXISTS (
                     SELECT 1 FROM description_lint_results WHERE review_id = ?1
                 )
                 OR EXISTS (SELECT 1 FROM round_commits WHERE review_id = ?1)",
            params![review.id, ReviewLayerState::Pending],
            |row| row.get(0),
        )?)
    }

    /// What this review holds of the three things only a session writes.
    ///
    /// One read rather than four, because both callers need all of them: `open_review` to
    /// decide whether a round may move, and `delete_review` to decide whether it may go
    /// and to say what it is protecting if it may not.
    pub fn review_work(&self, id: i64) -> Result<ReviewWork> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM briefs WHERE review_id = ?1) AS has_brief,
                    (SELECT COUNT(*) FROM findings WHERE review_id = ?1) AS findings,
                    EXISTS (SELECT 1 FROM drafts WHERE review_id = ?1) AS has_draft,
                    EXISTS (
                        SELECT 1 FROM reviews
                        WHERE id = ?1 AND github_review_id IS NOT NULL
                    ) AS is_posted",
            params![id],
            |row| {
                Ok(ReviewWork {
                    has_brief: row.get("has_brief")?,
                    findings: row.get("findings")?,
                    has_draft: row.get("has_draft")?,
                    is_posted: row.get("is_posted")?,
                })
            },
        )?)
    }

    /// Whether anybody has begun on this round.
    ///
    /// **DEV-954 asks for this on the wire so that a caller does not have to infer it from
    /// `session_id`.** A worker links itself only once it has begun, and in the window
    /// between a push and that link a dispatcher could not tell a round somebody was
    /// reading from a request nobody had picked up — so it guessed from the one field it
    /// could see, and a round whose session id was not yet written read as free.
    ///
    /// True when the round holds a brief, a finding, a draft or a posted review — which is
    /// [`ReviewWork::started`], the three things a delete would lose — **or** when anything
    /// only a session writes is there: a linked session, a layer past `pending`, what was
    /// not checked, the provenance, the description lint, this round's commits. The second
    /// half is [`Store::review_is_claimed`] and its list is the longer one on purpose: the
    /// question a guard needs answered is "has anybody begun", and a session that has
    /// written a layer and no brief has begun.
    ///
    /// **It decides nothing here.** Since DEV-962 an unposted round's head moves whoever is
    /// at work on it, so this is a fact a caller reads rather than a gate anything passes.
    pub fn review_started(&self, review_id: i64) -> Result<bool> {
        let review = self.get_review(review_id)?;
        Ok(self.review_work(review.id)?.started() || self.review_is_claimed(&review)?)
    }

    /// Withdraw a sign-off because the commit under it moved, and say which commit it was
    /// given on.
    ///
    /// **Only an approved draft, and only ever downwards.** A draft nobody has signed off
    /// has nothing to withdraw, and a posted one cannot be here at all — a posted round
    /// does not move its head. The status goes to `drafting` rather than `ready` for
    /// migration 18's reason: the code it was written against has changed, so the next step
    /// is the session's, and `ready` would leave "waiting on you to sign off" over a draft
    /// written for a commit nobody is looking at.
    ///
    /// `event` is left where it is, which migration 4's CHECK permits once `approved_at`
    /// and `approved_by` are null: it records what the review is *to be* posted as, which
    /// is still true.
    ///
    /// Answers whether anything was withdrawn, which is what a caller needs in order to say
    /// so and nothing more.
    ///
    /// `at` is the move's own time rather than a reading of the clock here: the draft's
    /// `updated_at` and the move are one event, and the code review compares the two.
    fn withdraw_signoff(&self, review_id: i64, approved_head: &str, at: Timestamp) -> Result<bool> {
        let withdrawn = self.conn.execute(
            "UPDATE drafts
                SET status = ?2,
                    approved_at = NULL,
                    approved_by = NULL,
                    signoff_withdrawn_at = ?3,
                    signoff_withdrawn_head = ?4,
                    updated_at = ?3
              WHERE review_id = ?1 AND status = ?5",
            params![
                review_id,
                DraftStatus::Drafting,
                at,
                normalise::head_sha(approved_head),
                DraftStatus::Approved,
            ],
        )?;
        Ok(withdrawn > 0)
    }

    /// Everything that has happened to this round's head, with the counts a page reads.
    ///
    /// Oldest first, which is the order they happened in and the order the code review
    /// reads them to say what changed since the draft was written.
    ///
    /// **The commit count is derived from `round_commits` and is null rather than zero when
    /// it cannot be.** "Updated for 3 commits since `4d9464f`" is a sentence about the pull
    /// request's commits, and the only record of those is what a session wrote when it last
    /// read the pull request. So the count is the commits recorded *after* the one the round
    /// was opened on — and when that commit is not among them, because nothing recorded them
    /// or because they were recorded before the move and not since, there is no count to
    /// give. Zero would read as "nothing changed", which is the one thing it is not.
    pub fn head_updates(&self, review_id: i64) -> Result<HeadUpdates> {
        self.get_review(review_id)?;

        let mut statement = self.conn.prepare(
            "SELECT id, review_id, kind, from_sha, to_sha, at
               FROM round_head_updates
              WHERE review_id = ?1
              ORDER BY id",
        )?;
        let updates = statement
            .query_map(params![review_id], |row| {
                Ok(HeadUpdate {
                    id: row.get("id")?,
                    review_id: row.get("review_id")?,
                    kind: row.get("kind")?,
                    from_sha: row.get("from_sha")?,
                    to_sha: row.get("to_sha")?,
                    at: row.get("at")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut summary = HeadUpdates {
            last_at: updates.last().map(|update| update.at),
            ..HeadUpdates::default()
        };
        for update in &updates {
            match update.kind {
                HeadUpdateKind::Moved => {
                    // The first move's `from_sha`: the commit this round was opened on, and
                    // the one the page measures from. A later move's is a commit this round
                    // was already of, which would make the sentence say "since" a commit
                    // the developer has already been shown.
                    summary.since_sha.get_or_insert(update.from_sha.clone());
                    summary.moved += 1;
                }
                HeadUpdateKind::BaseOnly => summary.base_only += 1,
                HeadUpdateKind::Folded => summary.folded += 1,
            }
        }

        if let Some(since) = &summary.since_sha {
            summary.commits = self.commits_after(review_id, since)?;
        }

        summary.updates = updates;
        Ok(summary)
    }

    /// How many of this round's recorded commits come after this one, or `None` when the
    /// commit is not among them.
    ///
    /// One query rather than reading the list: the position is what orders them, and
    /// `round_commits` holds it for the reason its own migration gives — a rebased commit
    /// can carry a committer date out of order.
    fn commits_after(&self, review_id: i64, sha: &str) -> Result<Option<i64>> {
        let sha = normalise::head_sha(sha);
        Ok(self
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM round_commits
                          WHERE review_id = ?1 AND position > at_sha.position)
                   FROM round_commits at_sha
                  WHERE at_sha.review_id = ?1 AND at_sha.sha = ?2",
                params![review_id, sha],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Open the review of this head commit, and record what the caller knows about the
    /// pull request.
    ///
    /// [`Store::open_review`] and then the facts: a fact given is written, a fact left
    /// `None` is left as it was — with the one exception [`Store::set_review_facts`]
    /// states, that a new `ticket_id` clears `ticket_url` unless the same call gives a new
    /// address. That holds whether the review was created by this call
    /// or found already on the record — which is what lets a session that has finished
    /// a review report what it did not check by opening the same head commit again,
    /// rather than needing a command of its own.
    ///
    /// **What the caller knows about the diff goes through to the same place.**
    /// [`DiffAtHead::Unchanged`] is "these commits only brought the base branch in", and
    /// then nothing moves and the facts given are still recorded against the round that is
    /// there — a newer title is a newer title whatever the commits did.
    ///
    /// **Two steps, not one.** `open_review` takes its own write lock, and SQLite has
    /// no nested transaction to put around both, so a reader between them sees a review
    /// whose facts are not yet written. That is the state every review opened by
    /// `review open` has been in since DEV-851, and it resolves itself; the alternative
    /// is unpicking `open_review`'s round arithmetic, which is load-bearing for a
    /// reason its own comment gives.
    pub fn open_review_with(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
        facts: &ReviewFacts,
        diff: DiffAtHead,
    ) -> Result<OpenedReview> {
        // **The facts are checked before a round is spent.** `open_review` creates the
        // review, so a fact refused on the second step would leave behind a review the
        // refused call appears not to have made — and on a head commit nobody had reviewed
        // yet it would have spent a round number doing it. Checking here is what makes
        // `review open --ticket-url` all-or-nothing in the ordinary case.
        //
        // **It is not a guarantee under contention, and this used to claim it was.** The
        // check runs outside `open_review`'s write lock, so between it and the insert
        // another caller can claim the unstarted round this one read a ticket id from;
        // the insert then opens a fresh round with no id, `set_review_facts` refuses the
        // unpaired address, and the call fails having spent a round. Two callers racing
        // on one pull request while one of them passes an address is narrow enough to
        // state rather than close — closing it means moving the whole of
        // `open_review`'s round arithmetic behind this check, and that arithmetic is
        // load-bearing for the reason its own comment gives.
        //
        // The pairing check needs the review the call is about, which may or may not exist:
        // a session can record the id on one call and the address on a later one, so an
        // address with no id in *this* call is fine when the record already holds one.
        // `find_review_by_head` is the same lookup `open_review` does first, and writes
        // nothing.
        //
        // **Normalised before the lookup, through the same helpers `open_review` uses.**
        // `find_review_by_head` compares under SQLite's binary collation and takes its
        // arguments as given, so a lookup keyed differently from the insert misses — and a
        // miss here is not a slow path, it is a refusal. Measured: without this,
        // `--repo DevergeHQ/Review-Desk --head C0FFEE1 --ticket-url …` was refused for
        // having no ticket id while the review it names held one, and the same call spelled
        // in lower case succeeded. One repository spelled two ways is one repository, which
        // this file's own tests say in as many words.
        //
        // **And only when there is an address to check**, which is the common case not
        // paying for the rare one: every `review open` reaches here, and without the guard
        // each one ran this lookup and then `open_review`'s own identical lookup on the
        // next line. The authoritative check is in `set_review_facts` either way.
        if facts.ticket_url.is_some() {
            let (looked_up_repo, looked_up_head) =
                (normalise::repo(repo), normalise::head_sha(head_sha));
            // **The review this call is about is not always the one at this commit.**
            // Since DEV-945 a head that names no review may be about to be *moved* onto
            // an earlier round, and that round is where `set_review_facts` will write —
            // so looking only by head handed the check `None` and refused an address for
            // a review that already holds the id. Reproduced by a review of that branch:
            // record the id on one call, the address on the next after a push, and the
            // second was refused. Asked in the same order `open_review` resolves it, so
            // the pre-check and the write cannot disagree about which review they mean.
            //
            // **Since DEV-962 that round is the newest one, posted or not**, which is a
            // wider net than the unstarted round this used to ask for and is right for both
            // paths: an unposted newest round is where a move will write, and a *posted*
            // one is where a base-only report will write. Either way the id this call is
            // judged against is the one on the review the call is about.
            let held = match self.find_review_by_head(&looked_up_repo, pr, &looked_up_head)? {
                Some(review) => review.ticket_id,
                None => self
                    .newest_round(&looked_up_repo, pr)?
                    .and_then(|review| review.ticket_id),
            };
            check_ticket(facts, held)?;
        }

        let opened = self.open_round(repo, pr, head_sha, diff)?;
        if facts.is_empty() {
            return Ok(opened);
        }

        let review = self.set_review_facts(opened.review.id, facts)?;
        Ok(OpenedReview { review, ..opened })
    }

    /// Record what is known about the pull request: its title, its ticket and that
    /// ticket's address, how many lines it changed and what the review did not look at.
    ///
    /// A fact left `None` is left as it was, so one call can set the title without
    /// clearing what was not checked.
    ///
    /// **One exception, and it is the only one:** a `ticket_id` that differs from the one
    /// held clears `ticket_url` unless the same call gives a new address. An address belongs
    /// to one ticket, and keeping it against a different id made the dashboard draw a link
    /// whose text and destination named two different things. See the block in the body.
    ///
    /// **A `ticket_url` that is not `https` is refused here**, before anything is
    /// written, with [`StoreError::InvalidValue`]. This is the one gate every caller
    /// passes through — the command line, the local API and the dashboard behind it — and
    /// the only place the refusal can be made once. The reason is in migration 16: the one
    /// thing a stored address is for is being drawn as a link, so a `javascript:` or
    /// `data:` value is the whole of the risk and it never reaches the column.
    pub fn set_review_facts(&self, id: i64, facts: &ReviewFacts) -> Result<Review> {
        // Checked first, so a bad id is NotFound rather than a silent update of
        // nothing.
        let held = self.get_review(id)?;
        let ticket_url = check_ticket(facts, held.ticket_id.clone())?;
        let ticket_id = normalise::optional_fact(facts.ticket_id.as_deref());

        // **A new id drops the old address, because an address belongs to one ticket.**
        // The two columns are written through independent `COALESCE`s, so a session
        // correcting a mistyped ticket — `--ticket DEV-860` on a review opened with
        // `--ticket ABC-231 --ticket-url …/ABC-231` — replaced the id and left the address.
        // The header then drew `DEV-860` as a link to `ABC-231`: the link's text and its
        // destination naming two different tickets, silently, which is the exact failure
        // `prUrl` is *built* rather than stored to avoid.
        //
        // **Both ids are read through `optional_fact`, which is not belt and braces.** The
        // left side is normalised and the right is whatever the column holds, and a released
        // build stored ids unnormalised — `--ticket "$(…)"` kept the newline. So comparing
        // the two raw found `DEV-860` unequal to `DEV-860 ` and dropped the address for a
        // caller re-stating the *same* ticket. Migration 16 repairs those rows; this makes
        // the comparison right whatever a row holds, which is the rule `normalise`'s own
        // header states: a value normalised differently on two sides of a comparison is the
        // same bug wearing a different hat.
        //
        // **This does not give a session a way to clear an address and keep the id.**
        // `--ticket-url ""` reads as absent and leaves the column alone, by design, and the
        // drop below happens only as a side effect of the id changing. Stated because an
        // earlier version of this comment implied otherwise.
        //
        // **Dropped rather than refused.** Correcting a ticket id is a plausible thing for a
        // session to do and should not need the address to hand; a review with an id and no
        // address draws the id as text, which is a state the whole feature already handles.
        // Keeping an address nobody said belonged to the new ticket is the one option that
        // tells a reader something untrue.
        let stale_url = ticket_url.is_none()
            && ticket_id.is_some()
            && ticket_id != normalise::optional_fact(held.ticket_id.as_deref())
            && held.ticket_url.is_some();

        // **A call with nothing to write writes nothing, and says so by not marking the
        // review active.** The guard was in `open_review_with` alone, which is one level
        // away from the method that can break the invariant: the `UPDATE` below is a no-op
        // when every fact is absent, but `mark_review_active` after it is not, and
        // `last_activity_at` is what the queue reads for "last written to 4 min ago" and
        // for whether a session looks alive. The one other caller guards itself today; a
        // future one would have got the old behaviour back for free.
        //
        // **After the checks, not before.** A refused `ticket_url` is still refused even
        // though it would have written nothing — a caller that passed a `javascript:`
        // address wants to be told, not quietly told nothing happened.
        if facts.is_empty() {
            return Ok(held);
        }

        self.conn.execute(
            "UPDATE reviews SET
                 title         = COALESCE(?2, title),
                 ticket_id     = COALESCE(?3, ticket_id),
                 ticket_url    = CASE WHEN ?7 THEN NULL ELSE COALESCE(?4, ticket_url) END,
                 changed_lines = COALESCE(?5, changed_lines),
                 not_checked   = COALESCE(?6, not_checked)
             WHERE id = ?1",
            params![
                id,
                // Every optional string fact through the one rule, so that a blank one is
                // "say nothing about this" rather than "set it to the empty string" — which
                // `COALESCE` cannot be told apart from a value. `--title "$T"` with `$T`
                // unset destroyed a recorded title until this line read it this way.
                normalise::optional_fact(facts.title.as_deref()),
                ticket_id,
                ticket_url,
                facts.changed_lines,
                normalise::optional_fact(facts.not_checked.as_deref()),
                stale_url,
            ],
        )?;

        self.mark_review_active(id)?;
        self.get_review(id)
    }

    /// Ask for, or record, the one ticket covering every nit held back from this
    /// review's draft.
    ///
    /// With no ticket id this is the developer asking for one; with a ticket id it is
    /// an agent reporting the one it filed. Asking twice keeps the first time asked,
    /// for the reason confirming a brief twice does: the request was made once.
    ///
    /// **Review Desk does not file the ticket.** It records that one was asked for and
    /// which one answered, and the findings it covers are read from the dispositions
    /// rather than stored against it, so the set cannot drift.
    pub fn request_nit_ticket(&self, id: i64, ticket_id: Option<&str>) -> Result<NitTicket> {
        let ticket_id = ticket_id.map(str::trim);
        if ticket_id.is_some_and(str::is_empty) {
            return Err(StoreError::InvalidValue {
                field: "nit_ticket_id",
                value: String::new(),
                expected: "the id of the ticket the nits were filed as".to_string(),
            });
        }

        // **The ask, not the report.** With no ticket id this is the developer asking for
        // one, which is an item in the work list a dismissed review is not in — so it
        // would be asking for something nothing will do. With a ticket id it is a session
        // saying which ticket it *already filed*, and refusing that would lose the id:
        // the ticket exists in the tracker and the record would never learn which one,
        // with the status stuck at `requested` for ever. Caught by a review of this
        // branch; the guard had covered both roles of one function.
        if ticket_id.is_none() {
            self.refuse_if_dismissed(id, "asking for its held-back findings to be ticketed")?;
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let review = self.get_review(id)?;

        let status = match ticket_id {
            Some(_) => NitTicketStatus::Filed,
            None => NitTicketStatus::Requested,
        };
        write_nit_ticket(&self.conn, review.id, status, ticket_id, self.now())?;

        self.mark_review_active(id)?;
        tx.commit()?;
        self.nit_ticket(id)
    }

    /// The one ticket covering this review's held-back nits, and which findings those
    /// are.
    pub fn nit_ticket(&self, id: i64) -> Result<NitTicket> {
        let review = self.get_review(id)?;

        Ok(NitTicket {
            review_id: review.id,
            status: review.nit_ticket_status,
            ticket_id: review.nit_ticket_id,
            finding_seqs: self.held_back_findings(id)?,
            requested_at: review.nit_ticket_requested_at,
            filed_at: review.nit_ticket_filed_at,
        })
    }

    /// The sequence numbers of this review's findings held back from the draft.
    pub fn held_back_findings(&self, id: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT seq FROM findings
             WHERE review_id = ?1 AND disposition = ?2 ORDER BY seq ASC",
        )?;
        let rows = statement.query_map(params![id, Disposition::HeldBack], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Move this review's last-activity time to now.
    ///
    /// **Called by every write that belongs to a review, and by no read.** It is the one
    /// answer Review Desk can give to "is the session still alive": Allele owns session
    /// liveness and tells this store nothing, so how long the record has been silent is
    /// what the queue has to say instead.
    ///
    /// A plain statement rather than a transaction of its own, so it joins whatever
    /// transaction the write that called it opened. A sign-off whose layer row cannot be
    /// written writes neither, and its activity must not survive either.
    ///
    /// `review_writes_move_the_last_activity_time` walks every write the command line can
    /// make and fails when one of them does not move it, which is the only way a caller
    /// added later gets caught.
    pub(crate) fn mark_review_active(&self, review_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE reviews SET last_activity_at = ?2 WHERE id = ?1",
            params![review_id, self.now()],
        )?;
        Ok(())
    }

    /// Take a pull request out of play, or put it back.
    ///
    /// **One call for both directions, because they are one fact.** `Some(why)` records that
    /// the pull request is a draft or that its review request was withdrawn; `None` records
    /// that it is back. A second function for the way back would be a second place the
    /// column's meaning is written down.
    ///
    /// **It names a pull request, not a round, and writes every round of it.** This is the
    /// state of the *request* — the author pulled it back, or somebody took the reviewer off
    /// it — and nothing about it belongs to one round. Writing it to every round is what
    /// makes "no round holds one" mean exactly "the request is back": the alternative, a
    /// value on the round that happened to be newest, leaves an older round still saying
    /// `draft` after the pull request is ready, and `IN_PLAY` would then be answering from
    /// whichever round a query reached first. Migration 19 carries the whole argument.
    ///
    /// **Review Desk read none of this.** It writes nothing to GitHub and polls nothing: the
    /// dispatcher watching the pull request says so, through `review out-of-play` and
    /// `review in-play`. That is also why there is no actor here — no developer act is
    /// involved, and a column holding `agent` in every row cannot disagree with itself.
    ///
    /// **A pull request with no review at all is [`StoreError::NotFound`]**, rather than a
    /// write of nothing reported as success. A dispatcher that has not recorded the request
    /// yet is a dispatcher whose next `review open` records it, and answering 0 rounds
    /// written as though something had happened is how a caller comes to believe a state it
    /// never set.
    ///
    /// Answers the rounds as they now stand, oldest round first, so a caller can say how
    /// many it wrote and a reader can see that every one of them agrees.
    pub fn set_pr_out_of_play(
        &self,
        repo: &str,
        pr: i64,
        why: Option<OutOfPlay>,
    ) -> Result<Vec<Review>> {
        let repo = normalise::repo(repo);

        // IMMEDIATE, so the read that proves the pull request exists and the write that
        // changes every one of its rounds cannot have a round opened between them. A round
        // opened a moment later is a round `review open` gives the state to, which is the
        // next paragraph's job rather than this lock's.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        if self.latest_round(&repo, pr)?.is_none() {
            return Err(StoreError::NotFound {
                entity: "pull request",
                id: format!("{repo}#{pr}"),
            });
        }

        // **`out_of_play_at` is written with the reason and cleared with it.** A time
        // without a reason would be a review that went out of play for nothing, which is
        // migration 17's rule for a dismissal's reason read the other way round.
        //
        // **And it does not move when the same reason is said again**, which matters
        // because the caller that says it is a poller: a dispatcher that re-asserts `draft`
        // every tick would otherwise reset "out of play since" for ever, and a page saying
        // "a draft since 2 minutes ago" about a pull request pulled back last Tuesday is
        // the fault `set_review_layer`'s idempotence argument describes. A *different*
        // reason is a different fact and takes the new time: a pull request that went draft
        // and then had its request withdrawn is out of play for the newer reason, and the
        // two can arrive in either order.
        //
        // `IS` rather than `=`, which is SQLite's null-safe comparison: `out_of_play = NULL`
        // is null, so the clearing case would fall to the `ELSE` and write a time against no
        // reason.
        self.conn.execute(
            "UPDATE reviews
                SET out_of_play = ?3,
                    out_of_play_at = CASE
                        WHEN ?3 IS NULL         THEN NULL
                        WHEN out_of_play IS ?3  THEN out_of_play_at
                        ELSE ?4
                    END
              WHERE repo = ?1 AND pr = ?2",
            params![repo, pr, why, self.now()],
        )?;
        tx.commit()?;

        self.list_reviews(&repo, pr)
    }

    /// Dismiss a review, with the developer's reason.
    ///
    /// **Hidden and kept.** The review leaves the queue, the work list and the Reviews
    /// list's default view, and every row under it stays exactly as it was: its findings,
    /// the dispositions the developer made one at a time, its sign-offs, and the evidence
    /// the rules in force were built from. Patrick decided this against deleting outright
    /// on 12 October 2026, for that reason — "a review with findings carries your
    /// sign-offs and the evidence rules are built from, so deleting it loses something
    /// real".
    ///
    /// **A blank reason is refused**, which is the rules screen's rule for a dismissed
    /// rule and holds here for the same reason: a review hidden silently is one nobody can
    /// afterwards say why they hid.
    ///
    /// **Dismissing a dismissed review is refused rather than quietly rewritten.** The
    /// first dismissal is the one the developer made, and a second call would move its
    /// time and replace its words. Restore it and dismiss it again to change the reason;
    /// both are the developer's acts and both are on the record.
    ///
    /// The refusal says that it is dismissed and not *when*: times are integers in this
    /// crate and the server converts them at its own boundary, so a timestamp in the
    /// message would reach the developer as `1791780818`. When it happened is on the
    /// review, which is what the page re-reads — [`StoreError::AlreadyDismissed`] carries
    /// that argument, and this paragraph used to contradict it.
    ///
    /// **There is no actor argument.** Only the developer dismisses a review: the command
    /// line has no path to this, and the dashboard's API is the only caller.
    pub fn dismiss_review(&self, id: i64, reason: &str) -> Result<Review> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "dismissed_reason",
                value: String::new(),
                expected: "why this review is being dismissed".to_string(),
            });
        }

        // The read and the write are one step, so two clicks on one button cannot both
        // pass the already-dismissed check and the second overwrite the first's reason.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let review = self.get_review(id)?;
        if review.is_dismissed() {
            return Err(StoreError::AlreadyDismissed {
                review_id: review.id,
            });
        }

        self.conn.execute(
            "UPDATE reviews SET dismissed_at = ?2, dismissed_reason = ?3 WHERE id = ?1",
            params![id, self.now(), reason],
        )?;
        // **It moves the activity time, as every write that belongs to a review does.**
        //
        // Read twice as a fault and left twice, so the reasoning is here. The queue draws
        // this column as how long a *session* has been silent, and a dismissal is the
        // developer's act — so a review abandoned three days ago, dismissed and later
        // restored, reads "just now". That is true, and it is true of confirming a brief
        // and of approving a draft in exactly the same way: every developer act in this
        // crate marks the review active. Exempting this one would make the dismissal the
        // single write that does not, which is a worse record than the imprecision it
        // would fix. What the column needs is to stop standing in for two questions, and
        // that is a change to what the queue reads rather than to this line.
        self.mark_review_active(id)?;
        tx.commit()?;

        self.get_review(id)
    }

    /// Bring a dismissed review back.
    ///
    /// Both columns are cleared together — a review holding a reason and no time would be
    /// a review the queries read as in play while the page drew it as dismissed.
    ///
    /// **Restoring a review that is not dismissed is refused.** Nothing would change, and
    /// a success for a request that did nothing is how a screen comes to show a button
    /// that appears to work.
    pub fn restore_review(&self, id: i64) -> Result<Review> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let review = self.get_review(id)?;
        if !review.is_dismissed() {
            return Err(StoreError::NotDismissed {
                review_id: review.id,
            });
        }

        self.conn.execute(
            "UPDATE reviews SET dismissed_at = NULL, dismissed_reason = NULL WHERE id = ?1",
            params![id],
        )?;
        self.mark_review_active(id)?;
        tx.commit()?;

        self.get_review(id)
    }

    /// Delete a review that never got past being requested, and everything under it.
    ///
    /// **Refused unless the review holds nothing** — no brief, no finding, no draft and
    /// no review posted on GitHub, asked through [`ReviewWork`], which `open_review` also
    /// asks before it moves a round's head. The first three are the ones DEV-945 names;
    /// the fourth is a review whose process ran outside Review Desk, which holds none of
    /// them and is plainly not a row that should never have existed. The refusal says what
    /// the review is holding, so the developer reads "a brief and 4 findings" rather than
    /// "not allowed", and names dismissal as the thing to do instead.
    ///
    /// **What goes with it is everything that referenced it.** The row is deleted and
    /// SQLite's own foreign keys take its layers, its sources, its ticket criteria, its
    /// per-file change map and its round commits with it — `PRAGMA foreign_keys` is on for
    /// every connection this crate opens, and `deleting_a_review_nobody_started_takes_every_row_under_it`
    /// asserts the result rather than trusting the declaration. A rule whose origin was
    /// this review keeps its statement and loses the origin, which is what
    /// `ON DELETE SET NULL` is there for; a never-started review has no findings for a rule
    /// to have come from, so that path is reachable only through a review that was deleted
    /// after being restored to nothing, and the schema answers it either way.
    ///
    /// **It hands back the review it deleted.** A caller that has just removed a row
    /// cannot read it again, and the dashboard has a line to draw about what went.
    pub fn delete_review(&self, id: i64) -> Result<Review> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let review = self.get_review(id)?;
        self.refuse_if_started(&review)?;
        self.remove_review(id)?;
        tx.commit()?;

        Ok(review)
    }

    /// Refuse a removal of a review that holds something the developer would lose.
    ///
    /// **One refusal, two callers**, which is why it is a function: [`Store::delete_review`]
    /// and [`Store::fold_round`] remove a review row, and a fold that judged this for itself
    /// could offer to fold a round the delete would refuse. The words are the ones the
    /// refusal is read in — "review 7 holds a brief and 4 findings" — and
    /// [`ReviewWork::in_words`] composes them.
    pub(crate) fn refuse_if_started(&self, review: &Review) -> Result<()> {
        let work = self.review_work(review.id)?;
        if work.started() {
            return Err(StoreError::ReviewStarted {
                review_id: review.id,
                holds: work.in_words(),
                lost: work.lost(),
            });
        }
        Ok(())
    }

    /// Remove the review row, with everything under it cascading.
    ///
    /// Private and unguarded: the two callers check first, inside their own transaction.
    pub(crate) fn remove_review(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM reviews WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Refuse an act that needs an agent afterwards, on a review the developer dismissed.
    ///
    /// **The four acts this guards are the ones whose completion is somebody else's.**
    /// Confirming a brief asks an agent to carry on past layer 2; approving a draft asks
    /// one to post it; sending it back asks one to revise it; asking for the held-back
    /// nits to be ticketed asks one to file the ticket. Each is an item in `work list`,
    /// and `work list` is the list a dismissed review is not in — so performing one would
    /// be the developer signing something off that nothing will ever act on, with the
    /// dashboard telling them "the review session posts it".
    ///
    /// Found by a review of this branch, which drove it: dismiss a review, approve its
    /// draft, and the draft is approved for ever while `work list` says nothing is
    /// waiting. The record is not wrong there — it is the developer who has been told
    /// something untrue — and the honest fix is to refuse the act rather than to let it
    /// succeed into nothing.
    ///
    /// **It is not a lock on the record.** Everything that changes a review *without*
    /// needing an agent still works on a dismissed one: a finding's disposition, a
    /// decision on a finding, writing the draft, the harvest's own writes. Dismissal
    /// takes a review out of the lists; it does not freeze what is on it, which is what
    /// "every record under it is kept" means.
    /// **It comes after the actor check, in all three files that make both.** This one
    /// reads the review, so putting it first turned "an agent may not approve a draft"
    /// into a 404 for a review that is not there and into a dismissal refusal for one
    /// that is — answering a question about the record to a caller who was not allowed to
    /// ask it. Who you are is settled before what you asked about.
    pub(crate) fn refuse_if_dismissed(&self, review_id: i64, act: &'static str) -> Result<()> {
        match self.not_for_signing(review_id)? {
            None => Ok(()),
            Some(NotForSigning::Dismissed) => Err(StoreError::DismissedReview { review_id, act }),
            // **Both added by DEV-957, through the function that already held the rule.**
            // The four acts are the same four — the ones whose completion is somebody
            // else's — and the reason generalises exactly: approving the draft of a merged
            // pull request, or of a round the author has pushed past, is the developer
            // signing something off that nothing will ever act on, with the dashboard
            // telling them "the review session posts it". The name is now narrower than
            // what it does and is left alone deliberately: renaming it touches every
            // caller and the thing worth having is one guard rather than three.
            Some(NotForSigning::Finished { how, .. }) => Err(StoreError::FinishedReview {
                review_id,
                how: how.as_str(),
                act,
            }),
            Some(NotForSigning::Superseded { by_round }) => Err(StoreError::SupersededRound {
                review_id,
                round: self.get_review(review_id)?.round,
                by_round,
                act,
            }),
        }
    }

    /// Why nothing on this review may be signed off, or `None` when it may.
    ///
    /// **The guard above and the draft page read this same answer**, which is what stops
    /// the page offering a sign-off the API refuses. It is the whole of DEV-957's
    /// "an unposted draft on a finished review says so and cannot be signed off": the
    /// cannot and the says-so are one value.
    ///
    /// **A posted round is never superseded**, and that is the condition rather than a
    /// consequence. A round that posted *did* post — its review is on GitHub — so a newer
    /// round does not make it a thing that never happened. What a newer round supersedes
    /// is a round whose draft is still waiting, which can no longer be the review that
    /// posts. Posted is asked the way `posted_reviews_awaiting_author` asks it: the
    /// draft's own `posted_at`, or a `github_review_id` on the review for a review posted
    /// by `review link` with no draft at all.
    ///
    /// **An *approved* round that a newer round supersedes is refused too, and that is
    /// deliberate rather than an oversight.** It means a draft the developer signed off and
    /// no session posted before the author pushed can no longer be sent back: it is stuck
    /// at `approved`, which is the shape DEV-929 removed elsewhere on the ground that "a
    /// stuck review is worse than the thing the refusal prevents".
    ///
    /// The difference is what the screen says. DEV-929's stuck draft was announcing that it
    /// was *waiting to be posted* while nothing could post it; this one says "Not posted —
    /// superseded by round N" and offers nothing, so the record and the page agree and no
    /// developer is left waiting for an event that will not come. `approved` is also still
    /// true: they did sign it off, and the author moving past that round does not unmake
    /// it. Sending it back would ask a session to rewrite a round the work list no longer
    /// offers, which is the thing this guard exists to refuse.
    ///
    /// Raised as "confirm intent" by a review of this branch, and confirmed rather than
    /// changed.
    ///
    /// The three questions in one read, because every caller needs all of them and the
    /// order between them is a decision [`NotForSigning`] states rather than one each
    /// caller makes.
    pub fn not_for_signing(&self, review_id: i64) -> Result<Option<NotForSigning>> {
        let review = self.get_review(review_id)?;
        if review.is_dismissed() {
            return Ok(Some(NotForSigning::Dismissed));
        }

        let (state, state_at, newest_round) = self.conn.query_row(
            "SELECT
                 (SELECT settled.pr_state FROM reviews settled
                  WHERE settled.repo = ?2 AND settled.pr = ?3
                    AND settled.pr_state IS NOT NULL
                  ORDER BY settled.round DESC LIMIT 1)       AS pr_state,
                 (SELECT settled.pr_state_at FROM reviews settled
                  WHERE settled.repo = ?2 AND settled.pr = ?3
                    AND settled.pr_state IS NOT NULL
                  ORDER BY settled.round DESC LIMIT 1)       AS pr_state_at,
                 (SELECT MAX(round) FROM reviews
                  WHERE repo = ?2 AND pr = ?3)               AS newest_round",
            params![review.id, review.repo, review.pr],
            |row| {
                Ok((
                    row.get::<_, Option<PrState>>("pr_state")?,
                    row.get::<_, Option<Timestamp>>("pr_state_at")?,
                    row.get::<_, Option<i64>>("newest_round")?,
                ))
            },
        )?;

        // **Asked through the one definition of posted**, which since DEV-962 is
        // [`Store::review_is_posted`] because the round rule turns on the same word: a
        // round that counts as posted here has to be a round `open_review` will not move,
        // or a pull request could hold a round that can neither post nor be superseded.
        // It was a fourth column of the query above until that rule arrived; one read more
        // is the price of the two rules being unable to disagree.
        let is_posted = self.review_is_posted(review.id)?;

        if let Some(how) = state.filter(PrState::is_settled) {
            return Ok(Some(NotForSigning::Finished { how, at: state_at }));
        }

        // `newest_round` cannot be null — this review is one of the rows it maxes over —
        // and falling back to this round's own number says "nothing newer" rather than
        // turning a database this build does not understand into a panic.
        let by_round = newest_round.unwrap_or(review.round);
        if by_round > review.round && !is_posted {
            return Ok(Some(NotForSigning::Superseded { by_round }));
        }

        Ok(None)
    }

    /// Read a review by id. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_review(&self, id: i64) -> Result<Review> {
        self.find_review_by_id(id)?.ok_or(StoreError::NotFound {
            entity: "review",
            id: id.to_string(),
        })
    }

    /// Read a review by id, or `None`.
    pub fn find_review_by_id(&self, id: i64) -> Result<Option<Review>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM reviews WHERE id = ?1"),
                params![id],
                row_to_review,
            )
            .optional()?)
    }

    /// The review of this repository, pull request and round, if there is one.
    ///
    /// Normalised here, as [`Store::open_review`]'s own comment argues for the lookup it
    /// makes: the column holds the canonical spelling, so a lookup keyed differently from
    /// the insert misses — and a miss is "there is no such review" rather than a slow
    /// path. Doing it in each lookup rather than in each caller is what stops the next
    /// caller forgetting.
    pub fn find_review(&self, repo: &str, pr: i64, round: i64) -> Result<Option<Review>> {
        let repo = normalise::repo(repo);
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM reviews
                     WHERE repo = ?1 AND pr = ?2 AND round = ?3"
                ),
                params![repo, pr, round],
                row_to_review,
            )
            .optional()?)
    }

    /// The newest review of this repository, pull request and head commit, if there is
    /// one. A caller opening a review for a head commit it has already reviewed should
    /// be handed that review back rather than starting a round.
    pub fn find_review_by_head(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
    ) -> Result<Option<Review>> {
        // Both halves of the identity, through the helpers the insert uses. `open_review`
        // normalises before it calls this and says why; a caller that does not is the one
        // this line is for.
        let (repo, head_sha) = (normalise::repo(repo), normalise::head_sha(head_sha));
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM reviews
                     WHERE repo = ?1 AND pr = ?2 AND head_sha = ?3
                     ORDER BY round DESC LIMIT 1"
                ),
                params![repo, pr, head_sha],
                row_to_review,
            )
            .optional()?)
    }

    /// The highest round reached on this pull request, or `None` if it has never been
    /// reviewed. The next round is this plus one.
    pub fn latest_round(&self, repo: &str, pr: i64) -> Result<Option<i64>> {
        let repo = normalise::repo(repo);
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(round) FROM reviews WHERE repo = ?1 AND pr = ?2",
                params![repo, pr],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Every review of a pull request, oldest round first.
    ///
    /// **A dismissed round is listed, and that is a decision.** This is one pull request's
    /// own history — which commit was reviewed when, and what became of each round — and a
    /// round the developer dismissed is part of it. `review find` is an agent recovering an
    /// id, and handing it a list with a hole where round 2 used to be would make the round
    /// numbers read as a gap in the record. The lists that leave dismissed reviews out are
    /// the ones asking what to do next.
    pub fn list_reviews(&self, repo: &str, pr: i64) -> Result<Vec<Review>> {
        // **The one that was wrong, and the Locus half found it on a real repository.**
        // `review find --repo Globex/portal` answered "no reviews" while
        // `review open` and `work list` found them, because those two normalise and this
        // did not — and the answer echoed the query back, so the two spellings never
        // appeared side by side for anybody to notice.
        let repo = normalise::repo(repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reviews
             WHERE repo = ?1 AND pr = ?2 ORDER BY round ASC"
        ))?;
        let rows = statement.query_map(params![repo, pr], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Store the other systems' ids against a review.
    ///
    /// A field left `None` is left as it is, so one call can set the session id without
    /// clearing the GitHub review id.
    ///
    /// **All three are read through [`normalise::optional_fact`]**, which is the rule
    /// every other optional string this record holds is written under and is here for the
    /// reason that function's own doc gives: `--session "$SESSION_ID"` from a shell whose
    /// variable is unset is `Some("")`, and `COALESCE` is only told to leave a column
    /// alone by a `NULL`, so the empty string *replaced* what was there.
    ///
    /// Since DEV-945 that is worse than a wiped id. A `session_id` of `""` is
    /// `is_some()`, so the review reads as one a session has picked up and its head never
    /// moves again — one request becomes two unstarted reviews, which is the thing the
    /// issue exists to prevent — and a `github_review_id` of `""` makes
    /// [`ReviewWork::started`] true, so the review can no longer be deleted. Reproduced
    /// by a review of this branch, through `review open --session ""`.
    pub fn set_review_links(&self, id: i64, links: ReviewLinks) -> Result<Review> {
        // Make sure the review is there, so a bad id is NotFound rather than a silent
        // update of nothing.
        self.get_review(id)?;

        let (session_id, ledger_id, github_review_id) = (
            normalise::optional_fact(links.session_id.as_deref()),
            normalise::optional_fact(links.ledger_id.as_deref()),
            normalise::optional_fact(links.github_review_id.as_deref()),
        );

        // **A call with nothing to write writes nothing, and says so by not marking the
        // review active**, which is `set_review_facts`' own rule and is here for the
        // same reason read through this branch's new caller: `review open --session
        // "$SESSION_ID"` from a shell whose variable is unset normalises to three
        // `None`s, and marking the review active anyway moves `last_activity_at` — the
        // one thing the queue has to say whether a session is still alive. A dead
        // session would read as freshly alive. Measured by a review of this branch.
        if session_id.is_none() && ledger_id.is_none() && github_review_id.is_none() {
            return self.get_review(id);
        }

        self.conn.execute(
            "UPDATE reviews SET
                 session_id       = COALESCE(?2, session_id),
                 ledger_id        = COALESCE(?3, ledger_id),
                 github_review_id = COALESCE(?4, github_review_id)
             WHERE id = ?1",
            params![id, session_id, ledger_id, github_review_id],
        )?;

        self.mark_review_active(id)?;
        self.get_review(id)
    }

    /// Record what the pull request was last seen to be, and when it reached that
    /// state.
    ///
    /// Writing the same state twice is the same as writing it once, which is what lets
    /// the harvester run again without special-casing a review it has already seen.
    pub fn set_review_pr_state(
        &self,
        id: i64,
        state: PrState,
        state_at: Option<Timestamp>,
    ) -> Result<Review> {
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET pr_state = ?2, pr_state_at = ?3 WHERE id = ?1",
            params![id, state, state_at],
        )?;

        self.mark_review_active(id)?;
        self.get_review(id)
    }

    /// Record that the harvester has just read this review.
    pub fn mark_review_harvested(&self, id: i64, at: Timestamp) -> Result<Review> {
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET harvested_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;

        self.mark_review_active(id)?;
        self.get_review(id)
    }

    /// The reviews a sweep is to read, oldest first.
    ///
    /// A review never harvested qualifies: its state is unknown, so it has to be
    /// looked at. A review whose pull request was **merged** does not — there is nothing
    /// further to learn, and fetching it again would cost a GitHub call per run for ever.
    ///
    /// **A closed pull request is read again, and only its newest round.** DEV-957 asks
    /// that a reopened pull request return its latest round to play at the next harvest,
    /// and nothing can notice a reopen without reading it: a state nobody reads is a state
    /// that never changes. The set is bounded by a fact rather than by a window — **GitHub
    /// cannot reopen a merged pull request**, so `merged` is genuinely terminal and only
    /// `closed` can come back. The newest round only, because that is the round a reopen
    /// would put back in play and the only one whose reading the derived state is taken from.
    ///
    /// **The cost, said plainly: one GitHub call per sweep per closed-and-unmerged pull
    /// request, where before it was none.** One, and that is a property of
    /// `harvest::run::harvest_one` rather than of this list: it stops after reading the pull
    /// request when the review was already settled and still is. Without that it would be
    /// three calls and more — the review threads and a commit comparison follow the state
    /// read — on a pull request nobody is going to touch again, for ever. Found by a review
    /// of this branch, which priced the first version of this sentence and found it wrong.
    ///
    /// **Nor does a dismissed review**, for the same reason read a different way: the
    /// developer has said they will not act on it again, so a quarter-hourly read of
    /// GitHub on its behalf is a call nobody is going to use the answer to. This is wider
    /// than the two places DEV-945 names — the queue and the Reviews list's default view —
    /// and it is the same sentence: Review Desk stops working on a dismissed review. A
    /// review restored is read again by the next sweep, because this list is decided from
    /// the record each time rather than from what the last sweep saw.
    pub fn reviews_to_harvest(&self, repo: Option<&str>) -> Result<Vec<Review>> {
        // Normalised, because the column is. A caller who typed `DevergeHQ/Locus` is
        // naming the repository the record holds as `devergehq/locus`, and an
        // unnormalised filter here would answer "nothing to harvest" rather than
        // "no such repository" — the lookup-normalised-differently-from-the-insert bug
        // `crate::normalise` exists to prevent.
        let repo = repo.map(normalise::repo);

        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reviews v
             WHERE (
                     v.pr_state IS NULL
                     OR v.pr_state = 'open'
                     OR (
                       v.pr_state = 'closed'
                       AND v.round = (SELECT MAX(round) FROM reviews
                                      WHERE repo = v.repo AND pr = v.pr)
                     )
                   )
               AND v.dismissed_at IS NULL
               AND (?1 IS NULL OR v.repo = ?1)
             ORDER BY v.id ASC"
        ))?;
        let rows = statement.query_map(params![repo], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Check and normalise the identity the rest of the store keys on.
///
/// The store is the only way into the record, so it is the only place that can hold
/// this line. A review whose repository is empty or malformed would silently partition
/// the record: rules are scoped by the same string, and `rules_for_repo` would never
/// match it.
///
/// The repository and head commit come back canonical, through [`crate::normalise`],
/// which is where the argument for that lives. Checking one form and storing another is
/// the whole bug: it would make `abc ` — or `ABC` — a different commit from `abc`.
fn validate(new: NewReview) -> Result<NewReview> {
    let repo = normalise::repo(&new.repo);
    let repo = repo.as_str();
    // **The characters and the segments, through the one copy of that rule.** This checked
    // only that both halves were non-empty, so `acme/portal#x` was stored — and a header
    // then built `https://github.com/acme/portal#x/pull/412`, which GitHub serves as the
    // repository page rather than the pull request: a link whose text and destination name
    // two different things, which is the defect DEV-936 exists to remove, reached through
    // the one field nobody thought was free text.
    //
    // The first fix for that wrote the character class here and missed `..`, which
    // `wire::is_repo_name` had refused all along and said why. `normalise::is_repo_name` is
    // now the only copy, and it carries both the rule and the reason.
    if !normalise::is_repo_name(repo) {
        return Err(StoreError::InvalidValue {
            field: "repo",
            value: new.repo.clone(),
            expected: concat!(
                "a repository as owner/repo, in letters, digits, dot, underscore and ",
                "hyphen, and neither half `.` nor `..`",
            )
            .to_string(),
        });
    }

    if new.pr < 1 {
        return Err(StoreError::InvalidValue {
            field: "pr",
            value: new.pr.to_string(),
            expected: "a pull request number of 1 or more".to_string(),
        });
    }

    if new.round < 1 {
        return Err(StoreError::InvalidValue {
            field: "round",
            value: new.round.to_string(),
            expected: "a round of 1 or more".to_string(),
        });
    }

    if new.head_sha.trim().is_empty() {
        return Err(StoreError::InvalidValue {
            field: "head_sha",
            value: new.head_sha.clone(),
            expected: "the head commit being reviewed".to_string(),
        });
    }

    // The same pairing rule, through the same function, so the two writers cannot drift
    // apart in what they refuse or in what they say about it. A review being created holds
    // nothing yet, which is what the `None` is.
    let url = paired(
        ticket_url(new.ticket_url.as_deref())?,
        new.ticket_id.as_deref(),
        None,
    )?;
    let id = normalise::optional_fact(new.ticket_id.as_deref());

    Ok(NewReview {
        repo: repo.to_string(),
        head_sha: normalise::head_sha(&new.head_sha),
        ticket_id: id,
        ticket_url: url,
        ..new
    })
}

/// The two ticket facts, checked together, and the address as it will be stored.
///
/// `held` is the id already on the record — `None` for a review that does not exist yet —
/// because a session may record the id on one call and the address on a later one.
fn check_ticket(facts: &ReviewFacts, held: Option<String>) -> Result<Option<String>> {
    paired(
        ticket_url(facts.ticket_url.as_deref())?,
        facts.ticket_id.as_deref(),
        held.as_deref(),
    )
}

/// An address is refused unless a ticket id goes with it, from this call or the record.
///
/// **Nothing would ever draw an address with no id.** Both renderers key on the id, which
/// is the link's *text* — an address drawn as its own text is a link a reader has to parse
/// rather than read — so a `ticket_url` stored beside a null `ticket_id` is a value that
/// reaches no screen and exists only in `review show`. A silent no-op, and the kind of
/// silent that looks like success.
///
/// **All three ids are read through [`normalise::optional_fact`], and that is the whole
/// care this function needs.** Reading any one of them more loosely than the others is
/// what this check has twice been let through by: first the *passed* id, tested with
/// `is_none()` while the address was normalised, so `--ticket "" --ticket-url https://…`
/// passed; then the *held* id, tested the same way, so a review whose stored id was already
/// `""` — which `--ticket ""` used to write — satisfied the "the record holds one" branch.
/// One function, one rule, three fields.
///
/// **Shared by both writers**, so the two cannot drift apart in what they refuse or in what
/// they say about it. `create_review` and `set_review_facts` are two ways to the same two
/// columns.
fn paired(url: Option<String>, passed: Option<&str>, held: Option<&str>) -> Result<Option<String>> {
    if url.is_none()
        || normalise::optional_fact(passed).is_some()
        || normalise::optional_fact(held).is_some()
    {
        return Ok(url);
    }

    // **`expected` says what would have been accepted, and nothing else.** The contract
    // defines it that way, and three attempts to smuggle *what happened* through this
    // refusal all misreported it: a hard-coded `""` in the value slot told a caller who
    // passed `--ticket "  "` that the field held nothing; a `"<none>"` sentinel asserts the
    // field holds a value, and `<none>` is a legal opaque `ForeignId`, so it cannot be told
    // from a caller who passed it; and a trailing "no id was passed" fought the opener
    // `ticket_id holds ""` in the same sentence.
    //
    // `InvalidValue` formats as `{field} holds {value:?}`, which cannot say "this field is
    // missing" — the right fix is a variant of its own, and that is a contract change
    // (`ErrorCode`) rather than something to bolt on here. Until then the value is what
    // arrived and the clause describes only the accepted shape, which is true in every
    // case: the id absent, the id blank, and a review that does not exist yet.
    //
    // **"or already on the review"** is the part that must not be dropped. It was, for one
    // commit, and it is the only place this refusal tells a caller the id may come from the
    // record — which is the two-call pattern `review open`'s own help documents: the id when
    // the review is opened, the address on a later call. A review is one round, so round 2
    // of a pull request does not inherit round 1's ticket, any more than it inherits its
    // title.
    Err(StoreError::InvalidValue {
        field: "ticket_id",
        value: passed.unwrap_or_default().to_string(),
        expected: concat!(
            "a ticket id to go with the address — passed in this call, or already on the ",
            "review the address is being set against; an address is drawn as its id, so ",
            "one with no id reaches no screen",
        )
        .to_string(),
    })
}

/// A ticket's address on its way into the record: trimmed, and `https` or refused.
///
/// **Both writers call this, which is the point.** `create_review` and `set_review_facts`
/// are two ways to the same column, and a check on one of them is a check on neither —
/// `review open --ticket-url` reaches the second and `POST /reviews` can reach either.
///
/// A value that is nothing but whitespace is `None` rather than an error — that is an
/// argument nobody filled in, and the field is optional — which is
/// [`normalise::optional_fact`]'s rule and is read from there rather than repeated. A value
/// that is *something* and not `https` is a mistake worth naming, because the caller plainly
/// meant to pass an address and the one they passed cannot be drawn as a link.
///
/// **`https://` and nothing else**, lowercase scheme as every tracker emits it. `http` is
/// refused with the rest: a ticket read over plain HTTP is a ticket read by anyone on the
/// path, and Review Desk's own rule is that it never fetches a tracker, so there is no
/// case where the insecure form is the only one available.
fn ticket_url(given: Option<&str>) -> Result<Option<String>> {
    let Some(given) = normalise::optional_fact(given) else {
        return Ok(None);
    };
    if !given.starts_with("https://") {
        return Err(StoreError::InvalidValue {
            field: "ticket_url",
            value: given,
            expected: "the ticket's address, starting https://".to_string(),
        });
    }
    Ok(Some(given))
}

/// Build a review from a row selecting [`COLUMNS`].
pub(super) fn row_to_review(row: &Row<'_>) -> rusqlite::Result<Review> {
    Ok(Review {
        id: row.get("id")?,
        repo: row.get("repo")?,
        pr: row.get("pr")?,
        head_sha: row.get("head_sha")?,
        round: row.get("round")?,
        title: row.get("title")?,
        ticket_id: row.get("ticket_id")?,
        ticket_url: row.get("ticket_url")?,
        changed_lines: row.get("changed_lines")?,
        not_checked: row.get("not_checked")?,
        session_id: row.get("session_id")?,
        ledger_id: row.get("ledger_id")?,
        github_review_id: row.get("github_review_id")?,
        pr_state: row.get("pr_state")?,
        pr_state_at: row.get("pr_state_at")?,
        harvested_at: row.get("harvested_at")?,
        nit_ticket_status: row.get("nit_ticket_status")?,
        nit_ticket_id: row.get("nit_ticket_id")?,
        nit_ticket_requested_at: row.get("nit_ticket_requested_at")?,
        nit_ticket_filed_at: row.get("nit_ticket_filed_at")?,
        dismissed_at: row.get("dismissed_at")?,
        dismissed_reason: row.get("dismissed_reason")?,
        out_of_play: row.get("out_of_play")?,
        out_of_play_at: row.get("out_of_play_at")?,
        last_activity_at: row.get("last_activity_at")?,
        created_at: row.get("created_at")?,
    })
}

/// Move the one ticket covering a review's held-back nits to this status.
///
/// **One statement, two callers.** `request_nit_ticket` is the dashboard asking for the
/// ticket or recording the one that was filed; `ticket_rest_and_approve` asks for it in the
/// same transaction as a sign-off. Written out twice they could come to mean different
/// things, and `requested_at` is the kind of column that gets overwritten by the second
/// copy of a statement.
///
/// `requested_at` is kept once set, so asking twice does not move the clock, and
/// `filed_at` is only written by a filing.
pub(crate) fn write_nit_ticket(
    conn: &Connection,
    review_id: i64,
    status: NitTicketStatus,
    ticket_id: Option<&str>,
    at: Timestamp,
) -> Result<()> {
    conn.execute(
        "UPDATE reviews SET
             nit_ticket_status       = ?2,
             nit_ticket_id           = COALESCE(?3, nit_ticket_id),
             nit_ticket_requested_at = COALESCE(nit_ticket_requested_at, ?4),
             nit_ticket_filed_at     = CASE WHEN ?2 = 'filed'
                                            THEN COALESCE(nit_ticket_filed_at, ?4)
                                            ELSE nit_ticket_filed_at END
         WHERE id = ?1",
        params![review_id, status, ticket_id, at],
    )?;

    Ok(())
}
