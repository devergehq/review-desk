//! A review GitHub has asked for, before Review Desk holds a round of it.
//!
//! **The one entity here that is not about a review**, and the reason it exists is a
//! silence. Review Desk learns of a pull request when a session opens a round on it; the
//! Locus dispatcher learns of the review request first and queues it until a slot frees,
//! which can be hours and is forever if the dispatcher is down. On 15 October 2026 the
//! queue drew 7 pull requests against 9 review requests on GitHub, and the whole of the gap
//! was requests nobody had started. DEV-996.
//!
//! **Review Desk reads nothing from GitHub for this.** The dispatcher is the only caller —
//! the same arrangement as `review out-of-play`, and for the same reason: one process
//! watches GitHub and the record holds what it says it saw.
//!
//! # Why this is not `review open`
//!
//! It nearly is. `review open` is already documented as how a review request is recorded
//! before any session starts, the dispatcher already calls it on sight, and a review with
//! no brief is already a `review_requested` item in [`Store::work_list`] — so a bare
//! `review open` buys a round number, a review id, a page to link, dismissal, deletion and
//! membership of [`Store::newest_rounds_in_play`], all of which this table has to do
//! without.
//!
//! **What it cannot do is be called without a head commit.** `review open` is keyed on the
//! commit being reviewed and refuses an empty one, and opening a round spends a round
//! number on a pull request nobody has looked at — a review of a commit no session chose,
//! which is the stray review DEV-945 was raised about. The dispatcher that is holding a
//! request in its backlog for hours has not chosen a commit and should not have to invent
//! one: the head will have moved by the time a session starts, and the record would carry
//! a round opened against a commit nobody reviewed.
//!
//! So the two records say different things, and both are true at once: a request is what
//! GitHub asked for, and a round is what a session opened. DEV-996 decides it that way and
//! this module follows; the alternative — a review that can exist with no head commit — is
//! a larger change to the record than a table keyed on a pull request.
//!
//! # What this module will not answer
//!
//! **Whether a request has been satisfied is not stored, and nothing here writes it.** A
//! request is answered once a round of its pull request opens at or after `requested_at`,
//! and [`Store::review_request_satisfied_by`] is that comparison, made every time the
//! question is asked. A column would have to be written by whatever opens a round — one
//! more write to miss — and would then need unwriting whenever a request was recorded after
//! a round had already opened. The comparison cannot be missed and cannot go stale.
//!
//! **Nor whether the request should be drawn on the queue**, beyond the part that is a fact
//! about the record. [`Store::open_review_requests`] is the *candidates*, like
//! [`Store::newest_rounds_in_play`] before it, and for the argument that query carries: the
//! subtraction against what the answer is actually serving belongs where the answer is
//! assembled, in `api.rs` and in `web/app/data/sample.ts`, never in a sixth reading of the
//! record.

use rusqlite::{params, Row, Transaction, TransactionBehavior};

use crate::entities::inbox::{FINISHED, NEWEST_ROUND};
use crate::error::{Result, StoreError};
use crate::model::{ReviewRequest, Timestamp};
use crate::{normalise, Store};

/// The first second the server could not spell: midnight on 1 January 10000, UTC.
///
/// **The format's own limit, which catches the unit error as a side effect.** A
/// `requested_at` at or past this is served by `wire::rfc3339` as a five-digit year, and
/// the contract declares the field `format: date-time` — so the answer would carry a
/// timestamp nothing can parse, and the queue would sort it by text against four-digit
/// years, which puts the furthest-future row at the bottom of a newest-first list. Every
/// value a caller could mean is below it; `Date.now()` in place of `date +%s` is about
/// 1.8e12 and is well above it, which is the mistake this argument invites and the one it
/// therefore refuses.
///
/// **A bound on what can be represented, not on what is plausible.** A request dated next
/// week is a clock out of step and is the caller's business; one the server cannot write
/// down is this record's. An earlier version of this stopped at `1e12` and called itself a
/// unit guard, which left a window where the record accepted a time the answer could not
/// serve — found by an independent review of this change, which ran it.
const BEYOND_RFC3339: Timestamp = 253_402_300_800;

/// The columns a [`ReviewRequest`] is read from, in the order [`row_to_request`] takes them.
const REQUEST_COLUMNS: &str = "q.repo, q.pr, q.requested_at, q.withdrawn_at,
                               q.created_at, q.updated_at";

/// Read one row of [`REQUEST_COLUMNS`].
fn row_to_request(row: &Row) -> rusqlite::Result<ReviewRequest> {
    Ok(ReviewRequest {
        repo: row.get(0)?,
        pr: row.get(1)?,
        requested_at: row.get(2)?,
        withdrawn_at: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// **The record's own answer about this pull request is that it is not to be worked on**,
/// as a clause over `review_requests` aliased `q`.
///
/// Three states, and between them they are [`crate::entities::inbox::IN_PLAY`] read from
/// the outside: the pull request merged or closed, the pull request out of play, or the
/// newest round dismissed. A request recorded against any of them is a row the queue would
/// draw for a pull request nobody is going to review — and this group is a complement,
/// where a wrong clause *draws* a row rather than hiding one.
///
/// **The dismissal is the one that was nearly left out, and it is the one that matters
/// most.** A round the developer dismissed draws nothing, which DEV-994 settled as the
/// answer rather than a gap: "a dismissal is the developer saying they will not act on this
/// round, so a group that put the pull request back in front of them under a different
/// heading would undo their own act." A request is such a group. The shape that reaches it
/// is ordinary: the author pushes, GitHub re-requests a review, DEV-962 moves the unposted
/// round's head instead of opening a round — so nothing will ever open to satisfy the
/// request — and the developer then dismisses the round. Without this clause the pull
/// request comes straight back under "Requested, not started", with no round to link, no
/// ticket, and no way for the developer to dismiss it again, because the row has no review
/// id. Found by a red-team pass over this change's design, before it was written.
///
/// **It is the newest round that is asked about, not any round.** An older dismissed round
/// under a live newest one says nothing: that pull request is drawn by its newest round,
/// and the subtraction in `api.rs` removes the request. [`NEWEST_ROUND`] is the same
/// expression the queue's own groups use, for the same reason.
///
/// **Unconditional, rather than compared against `requested_at` like satisfaction is.** A
/// request recorded after a dismissal is GitHub asking again, and an earlier draft of this
/// drew it for that reason. Two things argue the other way, and together they settle it.
/// The developer's act is about the pull request as they last saw it, and they have a verb
/// for changing their mind — `POST /reviews/{id}/restore` — which is the developer's own
/// and not a dispatcher's. And a clause that compares two times is a clause a caller can
/// walk past by moving one of them: `requested_at` is last-write-wins by the command's
/// pinned contract, so a dispatcher re-asserting the request with its own clock would push
/// the ask past the dismissal on every tick and draw the row for ever. The exclusion that
/// cannot be moved is the one to have.
///
/// **The two settled states are named by [`FINISHED`] rather than spelled again**, so
/// `the_words_in_the_finished_clause_are_the_settled_states` guards this clause too, and a
/// second literal `IN ('merged', 'closed')` is exactly the drift that test exists to catch.
/// It is also not "any round that holds a settled state", which both backends reject by
/// name — `pr_state!()` reads the highest round that holds one, because a pull request
/// closed, reopened and reviewed again holds `closed` on an early round for ever.
/// `out_of_play` is the other way round and is written to *every* round — migration 19 says
/// why — so `EXISTS` is right for it whichever round it reaches first.
fn pull_request_left_the_desk() -> String {
    format!(
        "EXISTS (SELECT 1 FROM reviews v
                  WHERE v.repo = q.repo AND v.pr = q.pr
                    AND (v.out_of_play IS NOT NULL
                         OR {FINISHED}
                         OR (v.dismissed_at IS NOT NULL AND {NEWEST_ROUND})))"
    )
}

/// **A round of this pull request opened at or after the request**, as a clause over
/// `review_requests` aliased `q`.
///
/// The whole of what "satisfied" means. DEV-996: "A request is satisfied once a round of
/// that pull request opened after `requested_at`; then that round's row is the row."
///
/// **At or after, not strictly after.** Both times are whole seconds, and a round opened in
/// the same second as the request was opened *for* it — a dispatcher that records the
/// request and starts a session in one tick is the ordinary case, not a contrived one.
/// Strictly after would leave that request reading "unsatisfied" for as long as it existed.
/// It cannot draw a second row either way, because a pull request with a round is a pull
/// request the queue has already drawn, and the request is subtracted against what was
/// drawn — but a record that says a satisfied request is open is a record somebody will
/// eventually believe.
///
/// **Every round counts, whatever became of it.** A round that opened and was then
/// dismissed, merged or taken out of play still answered the request: GitHub asked, a
/// session came. Filtering those out would leave the request open and draw the pull request
/// under "Requested, not started" the moment its round left the desk, which is the queue
/// contradicting itself about one pull request.
const SATISFIED: &str = "EXISTS (SELECT 1 FROM reviews v
              WHERE v.repo = q.repo AND v.pr = q.pr
                AND v.created_at >= q.requested_at)";

impl Store {
    /// Record what GitHub has asked for, or update what it asked for before.
    ///
    /// **An upsert, because the caller is a poller.** The dispatcher sees the same request
    /// on every tick for as long as GitHub lists it, so "recording the same request twice
    /// leaves one record" is an acceptance criterion rather than a nicety, and
    /// `UNIQUE (repo, pr)` is what makes it true rather than hoped for.
    ///
    /// **The call asserts the whole state, and the last one wins.** `requested_at` replaces
    /// whatever was there and `withdrawn` is set from this call alone — so GitHub taking a
    /// request away and asking again is two calls, and the second reopens the request with
    /// the new time. Accumulating a history here would be a second record of something
    /// GitHub already holds, and the one question the queue asks is what is outstanding
    /// now.
    ///
    /// **So a caller that passes its own clock instead of GitHub's request time breaks the
    /// age on the row**, which would then read "requested just now" on every tick for ever.
    /// [`Store::set_pr_out_of_play`] has the same hazard and answers it by refusing to move
    /// a time the caller did not change; that answer is not available here, because there
    /// is nothing but the time to compare — the same request re-asserted and a fresh
    /// request on the same pull request are the same three arguments. The caller is the one
    /// watching GitHub, so the record takes the time it is given and `--json` prints it
    /// back; `created_at` keeps when Review Desk first heard, which is the one thing a
    /// caller's clock cannot overwrite.
    ///
    /// Refuses a repository that is not `owner/repo`, a pull request below 1, and a time
    /// that is not whole seconds since the epoch — see
    /// [`super::pull_request_on_the_record`] for the first two and [`BEYOND_RFC3339`] for
    /// the far end of the third.
    pub fn record_review_request(
        &self,
        repo: &str,
        pr: i64,
        requested_at: Timestamp,
        withdrawn: bool,
    ) -> Result<ReviewRequest> {
        let repo = super::pull_request_on_the_record(repo, pr)?;
        // **Seconds, and a time this record can serve.** Zero is the epoch and a negative
        // one is before it; `BEYOND_RFC3339` is the far end, and it is where
        // `Date.now()` in place of `date +%s` lands. None of the three is a moment GitHub
        // asked for a review, and each would draw a row whose age is nonsense in a group
        // nothing else corrects. Refused for the reason a pull request of 0 is: a value no
        // caller can mean is a caller that has gone wrong, and saying so is cheaper than a
        // row somebody has to find.
        if !(1..BEYOND_RFC3339).contains(&requested_at) {
            return Err(StoreError::InvalidValue {
                field: "requested_at",
                value: requested_at.to_string(),
                expected: "when GitHub asked, in whole seconds since the epoch".to_string(),
            });
        }

        let now = self.now();
        // IMMEDIATE, so the insert and the update that follows it cannot be interleaved
        // with another tick of the same dispatcher. Two processes recording one request at
        // once is not hypothetical — the dispatcher is restarted by hand while the old one
        // is still finishing a tick.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.conn.execute(
            "INSERT INTO review_requests
                 (repo, pr, requested_at, withdrawn_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT (repo, pr) DO UPDATE SET
                 requested_at = excluded.requested_at,
                 withdrawn_at = excluded.withdrawn_at,
                 updated_at   = excluded.updated_at",
            params![repo, pr, requested_at, withdrawn.then_some(now), now,],
        )?;
        // **Read back inside the transaction**, so the row handed to the caller is the
        // one this call wrote rather than whatever the next tick of another dispatcher
        // left a moment later. It is also what the IMMEDIATE lock above is for: a single
        // upsert needs no transaction of its own, and the write and the read of it do.
        let written = self.review_request(&repo, pr)?;
        tx.commit()?;

        written.ok_or(StoreError::NotFound {
            entity: "review request",
            id: format!("{repo}#{pr}"),
        })
    }

    /// The request recorded for this pull request, open or withdrawn, if there is one.
    pub fn review_request(&self, repo: &str, pr: i64) -> Result<Option<ReviewRequest>> {
        let repo = normalise::repo(repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REQUEST_COLUMNS} FROM review_requests q
              WHERE q.repo = ?1 AND q.pr = ?2"
        ))?;
        let mut rows = statement.query_map(params![repo, pr], row_to_request)?;
        rows.next().transpose().map_err(Into::into)
    }

    /// The round that answered this request, if one has opened at or after it.
    ///
    /// **The earliest such round, which is the one that answered it.** A pull request can
    /// reach round four with three of them opened after the request; the round that
    /// satisfied it is the first. That is not the same as the row the queue draws for the
    /// pull request, which is its *newest* round — DEV-983's rule — and the two are
    /// different questions: this one is about the request's own history.
    ///
    /// [`SATISFIED`] carries why "at or after", and why a round that was dismissed or
    /// merged still counts.
    pub fn review_request_satisfied_by(&self, repo: &str, pr: i64) -> Result<Option<i64>> {
        let Some(request) = self.review_request(repo, pr)? else {
            return Ok(None);
        };
        let mut statement = self.conn.prepare(
            "SELECT v.id FROM reviews v
              WHERE v.repo = ?1 AND v.pr = ?2 AND v.created_at >= ?3
              ORDER BY v.created_at ASC, v.round ASC, v.id ASC
              LIMIT 1",
        )?;
        let mut rows = statement.query_map(
            params![request.repo, request.pr, request.requested_at],
            |row| row.get::<_, i64>(0),
        )?;
        rows.next().transpose().map_err(Into::into)
    }

    /// Every request still outstanding, newest request first.
    ///
    /// **The candidates for the queue's "Requested, not started", not the group.** The same
    /// division of labour as [`Store::newest_rounds_in_play`], which DEV-994 wrote the
    /// argument for: what the record knows is answered here, and the subtraction against
    /// the items the answer has just served is made where the answer is assembled — in
    /// `api.rs` and in `web/app/data/sample.ts`. A pull request already drawn under another
    /// heading is drawn once, and this query is deliberately unable to tell which those
    /// are.
    ///
    /// Three exclusions, each argued where it is written: a withdrawn request is GitHub
    /// saying it no longer wants one; [`SATISFIED`] is a round having opened for it; and
    /// [`pull_request_left_the_desk`] is the record's own answer that the pull request is
    /// not to be worked on — merged, closed, out of play, or its newest round dismissed.
    ///
    /// **What is deliberately not excluded is a pull request the record knows nothing
    /// about**, which is the whole case this exists for: no round, no title, nothing but
    /// what the dispatcher was told.
    pub fn open_review_requests(&self) -> Result<Vec<ReviewRequest>> {
        let left_the_desk = pull_request_left_the_desk();
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REQUEST_COLUMNS} FROM review_requests q
              WHERE q.withdrawn_at IS NULL
                AND NOT {SATISFIED}
                AND NOT {left_the_desk}
              ORDER BY q.requested_at DESC, q.repo ASC, q.pr DESC"
        ))?;
        let rows = statement.query_map([], row_to_request)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
