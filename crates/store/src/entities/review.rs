//! Reviews: one per pull request head commit per round.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    Disposition, NewReview, NitTicket, NitTicketStatus, OpenedReview, PrState, Review, ReviewFacts,
    ReviewLayerState, ReviewLinks, ReviewWork, Timestamp,
};
use crate::{is_unique_violation, normalise, Store};

pub(super) const COLUMNS: &str = "id, repo, pr, head_sha, round, title, ticket_id, \
                       ticket_url, changed_lines, \
                       not_checked, session_id, ledger_id, github_review_id, pr_state, \
                       pr_state_at, harvested_at, nit_ticket_status, nit_ticket_id, \
                       nit_ticket_requested_at, nit_ticket_filed_at, dismissed_at, \
                       dismissed_reason, last_activity_at, created_at";

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

    /// Open the review of this head commit: the one already on the record, the unstarted
    /// round whose head moves to it, or a new round.
    ///
    /// A head commit already reviewed is handed back — the newest round for that commit,
    /// even when later rounds exist for other commits.
    ///
    /// **A new commit on a pull request whose newest round nobody has started moves that
    /// round's head; it does not start a round.** Patrick, 12 October 2026, on DEV-945:
    /// "two unstarted reviews for one request is wrong". The dispatcher records a review
    /// request the moment GitHub shows it one, before any session runs, and an author who
    /// pushes between the request and the session would otherwise turn one request into
    /// two reviews — both unstarted, both offered to the dispatcher by `work list`, and
    /// each able to have a session dispatched for it. Unstarted is [`ReviewWork`]'s
    /// question, so it is the same three things `delete_review` protects: no brief, no
    /// findings, no draft.
    ///
    /// The round that moved comes back with `created: false` and
    /// [`OpenedReview::head_moved_from`] set, so a caller can say which of the three
    /// things happened without comparing commits itself.
    ///
    /// **A round a session has picked up does not move either**, whatever it has written
    /// yet: a linked session or a layer past `pending` is somebody at work, and moving the
    /// head under them would put a brief computed from one commit on a review the record
    /// says is of another. [`Store::latest_unstarted_round`] carries that argument.
    ///
    /// **A dismissed round's head moves too, and it stays dismissed.** It is still one
    /// request, and the developer has said they do not want it reviewed; starting a fresh
    /// round instead would put the request they declined back in front of a session on
    /// the author's next push. DEV-945 puts automatic dismissal out of scope, and
    /// automatic *un*-dismissal is the same decision read backwards.
    ///
    /// A round that has been started keeps the old behaviour exactly: a new commit on it
    /// is the next round, which is what rounds are for.
    ///
    /// The lookup, the round arithmetic and the insert are one step, for the reason
    /// [`Store::add_finding`] gives: two sessions opening the same pull request at the
    /// same time on two different new commits would otherwise both read the same latest
    /// round, and the second would be refused as a duplicate of a round it never asked
    /// for.
    pub fn open_review(&self, repo: &str, pr: i64, head_sha: &str) -> Result<OpenedReview> {
        // Normalised before the lookup, through the same helper `create_review` uses
        // before the insert. A lookup keyed differently from the insert would miss and
        // then start a round for a commit already reviewed.
        let (repo, head_sha) = (normalise::repo(repo), normalise::head_sha(head_sha));

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
            });
        }

        // The newest round, when nobody has started it: this commit is where that
        // request is now, so the round moves to it rather than a second one opening
        // beside it. Inside the write lock, which is what makes two callers arriving on
        // two different commits end with one review rather than two — the second finds
        // the round the first moved and moves it again.
        if let Some(unstarted) = self.latest_unstarted_round(&repo, pr)? {
            let moved_from = unstarted.head_sha.clone();
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
            self.conn.execute(
                "UPDATE reviews SET head_sha = ?2, changed_lines = NULL WHERE id = ?1",
                params![unstarted.id, head_sha],
            )?;
            self.mark_review_active(unstarted.id)?;
            let review = self.get_review(unstarted.id)?;
            tx.commit()?;
            return Ok(OpenedReview {
                review,
                created: false,
                head_moved_from: Some(moved_from),
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

        tx.commit()?;
        Ok(OpenedReview {
            review,
            created: true,
            head_moved_from: None,
        })
    }

    /// The newest round of this pull request, when it is a request nobody has picked up.
    ///
    /// `None` when the pull request has no review at all, and `None` when its newest round
    /// is being worked on. Only the newest round is asked about: an older unstarted round
    /// was superseded by a commit somebody did review, and moving *that* one would move a
    /// round out from under the history it sits in.
    ///
    /// **Two questions, and they are deliberately not the same one.** [`ReviewWork`] asks
    /// what a review *holds* — a brief, a finding, a draft — and that is the question
    /// `delete_review` asks, because those are the three things DEV-945 says a delete
    /// would lose. This asks the narrower thing: has anybody *begun*? A session that has
    /// been dispatched links itself with `review link --session` and moves a layer off
    /// `pending` long before it writes a brief, and between those two moments a review
    /// holds none of the three.
    ///
    /// Without the second half, a dispatcher polling while that session worked would move
    /// the head under it: the session would then write a brief, findings and anchors all
    /// computed from the commit it read into a review the record says is of a newer one.
    /// Before DEV-945 that push opened a round of its own and left the running round
    /// alone, so the narrower question is what keeps that true.
    ///
    /// The case the move is for is untouched: a request the dispatcher recorded and
    /// nothing has been dispatched for has no session, no layer past `pending` and none
    /// of the three.
    ///
    /// **A commit the pull request has moved past moves the head too, and that is right
    /// rather than a gap.** A session dispatched for commit A opens the review with A
    /// while the dispatcher has already recorded B; the review becomes one of A, which is
    /// what that session is actually reviewing, and the dispatcher's next poll finds a
    /// review somebody has claimed and opens round 2 for B. The head is the commit the
    /// review is *of*, not the pull request's newest — which is true of every review ever
    /// posted. What the rule refuses to do is let two unstarted reviews exist; it does
    /// not try to decide which of two commits is newer, and it could not: the store reads
    /// no git.
    ///
    /// **What is left is the window before a session claims the review**, and it is
    /// narrow rather than closed. A session opens the review and then links itself; a
    /// caller moving the head between those two moments would leave the session holding
    /// a commit the record no longer names. `review open --session` is the answer and is
    /// why that argument exists: a session passing its id on its first call claims the
    /// review in the same call, which leaves a window of one statement rather than of
    /// however long the session takes to get to `review link`. A session that does not
    /// pass it is protected from its first layer write onwards.
    fn latest_unstarted_round(&self, repo: &str, pr: i64) -> Result<Option<Review>> {
        let Some(round) = self.latest_round(repo, pr)? else {
            return Ok(None);
        };
        let Some(review) = self.find_review(repo, pr, round)? else {
            return Ok(None);
        };
        if self.review_work(review.id)?.started() || self.review_is_claimed(&review)? {
            return Ok(None);
        }
        Ok(Some(review))
    }

    /// Whether a session has picked this review up, whatever it has written yet.
    ///
    /// Two signs, and either is enough. A session linked to the review — written by
    /// `review link --session`, which is how a session says it is the one working — and a
    /// layer moved off `pending`, which is how it says what it is doing. A request
    /// recorded by the dispatcher has neither: `review open` writes no layer, and the
    /// ledger id it may carry is the dispatcher's own entry rather than a session.
    fn review_is_claimed(&self, review: &Review) -> Result<bool> {
        if review.session_id.is_some() {
            return Ok(true);
        }
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM review_layers WHERE review_id = ?1 AND state <> ?2
             )",
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
    ) -> Result<OpenedReview> {
        // **The facts are checked before a round is spent.** `open_review` creates the
        // review, so a fact refused on the second step would leave behind a review the
        // refused call appears not to have made — and on a head commit nobody had reviewed
        // yet it would have spent a round number doing it. Checking here keeps
        // `review open --ticket-url` all-or-nothing.
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
            check_ticket(
                facts,
                self.find_review_by_head(&looked_up_repo, pr, &looked_up_head)?
                    .and_then(|review| review.ticket_id),
            )?;
        }

        let opened = self.open_review(repo, pr, head_sha)?;
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
    /// first dismissal is the one the developer made, and a second call would move its time
    /// and replace its words — so the refusal says when it was dismissed, which is what a
    /// caller that has lost track needs. Restore it and dismiss it again to change the
    /// reason; both are the developer's acts and both are on the record.
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
    /// every connection this crate opens, and `delete_review_takes_every_row_under_it`
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

        let work = self.review_work(id)?;
        if work.started() {
            return Err(StoreError::ReviewStarted {
                review_id: review.id,
                holds: work.in_words(),
            });
        }

        self.conn
            .execute("DELETE FROM reviews WHERE id = ?1", params![id])?;
        tx.commit()?;

        Ok(review)
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
    pub fn find_review(&self, repo: &str, pr: i64, round: i64) -> Result<Option<Review>> {
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
    pub fn set_review_links(&self, id: i64, links: ReviewLinks) -> Result<Review> {
        // Make sure the review is there, so a bad id is NotFound rather than a silent
        // update of nothing.
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET
                 session_id       = COALESCE(?2, session_id),
                 ledger_id        = COALESCE(?3, ledger_id),
                 github_review_id = COALESCE(?4, github_review_id)
             WHERE id = ?1",
            params![
                id,
                links.session_id,
                links.ledger_id,
                links.github_review_id
            ],
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

    /// The reviews whose pull request was open at their last harvest, oldest first.
    ///
    /// A review never harvested qualifies: its state is unknown, so it has to be
    /// looked at. A review whose pull request was merged or closed does not — there is
    /// nothing further to learn, and fetching it again would cost a GitHub call per
    /// run for ever.
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
            "SELECT {COLUMNS} FROM reviews
             WHERE (pr_state IS NULL OR pr_state = 'open')
               AND dismissed_at IS NULL
               AND (?1 IS NULL OR repo = ?1)
             ORDER BY id ASC"
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
