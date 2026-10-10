//! The draft review: one per review, replaced in place.
//!
//! **This module is where the sign-off decision is enforced, not merely described.**
//! Whatever writes a draft goes through [`Store::put_draft`], which accepts only
//! `drafting` and `ready`. `approved` is reachable through [`Store::approve_draft`] and
//! `posted` through [`Store::post_draft`], and nothing else can set either. An agent
//! can therefore write a draft and read its status, and cannot sign it off, without
//! that resting on the agent's good manners.
//!
//! **Since DEV-877 a draft body is composed rather than written.** What a caller stores
//! is the blocks it is made of — see [`super::composition`] — and the body is what those
//! blocks compose to. A caller that sends no blocks is writing the draft as one block of
//! prose, which is honest and has one visible consequence: the developer's per-finding
//! decisions refuse on such a draft, because nothing can say which characters are F1's.
//!
//! **Each of the four transitions below also writes its layer row, in the same
//! transaction.** Writing a draft as ready puts layer 6 at `waiting_on_you`, approving
//! finishes it, sending back puts it in progress carrying the developer's note, and
//! marking a draft posted leaves layer 7 `pending` — waiting for a harvest that has not
//! happened — rather than `done`. Until DEV-864 none of them wrote a layer at all, so the
//! trail went stale the moment the developer acted; the first of the four was still
//! missing until DEV-890, which is where when a draft became ready has to come from.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::entities::{composition, only_the_developer};
use crate::error::{Result, StoreError};
use crate::model::{
    Actor, Draft, DraftLint, DraftStatus, NewDraft, NewDraftSection, ReviewEvent, ReviewLayerName,
    ReviewLayerState, Timestamp,
};
use crate::{is_foreign_key_violation, Store};

/// What layer 6 says when a session takes a ready draft back to work on.
///
/// Not the developer's own note — that is what `send_back_draft` writes, and the
/// difference is who asked for the change.
pub(crate) const TAKEN_BACK_DETAIL: &str = "the session is rewriting it";

/// What layer 6 says while the draft is waiting to be signed off.
///
/// The same words the Inbox and the trail already use for a layer waiting on the
/// developer, so a draft written as ready reads the same whether the session reported the
/// layer itself or this module wrote it.
pub(crate) const READY_DETAIL: &str = "waiting on you";

/// What layer 6 says once the developer has approved the draft.
///
/// The event chosen is on the draft, not here. A layer's detail is the short phrase after
/// its name, and "who settled this layer" is what the trail is short of.
pub(crate) const APPROVED_DETAIL: &str = "approved by the developer";

/// What layer 7 says once the session has posted the review.
///
/// Layer 7 is the harvest, and posting is its precondition rather than its completion.
pub(crate) const AWAITING_HARVEST_DETAIL: &str = "posted, waiting for the harvest";

const COLUMNS: &str = "review_id, body, status, event, approved_at, approved_by, \
                       sent_back_note, sent_back_at, posted_at, lint_passes, \
                       lint_visible_words, lint_word_budget, lint_findings_in_body, \
                       lint_findings_expected, lint_raised_before_on_pr, lint_errors, \
                       lint_checked_at, updated_at";

impl Store {
    /// Store the review's draft, replacing the body already held.
    ///
    /// Refused once the draft is approved or posted: an approved draft is what the
    /// review session reads, and editing it underneath would change what gets posted.
    /// A draft the developer sent back may be rewritten, which is how it gets back to
    /// `ready`.
    ///
    /// The status may be `drafting` or `ready` and nothing else. See the module
    /// documentation for why that is the whole of the sign-off boundary.
    pub fn put_draft(&self, review_id: i64, new: NewDraft) -> Result<Draft> {
        if !matches!(new.status, DraftStatus::Drafting | DraftStatus::Ready) {
            return Err(StoreError::InvalidValue {
                field: "status",
                value: new.status.to_string(),
                expected: "drafting or ready; approving a draft is the developer's act \
                           and is done in the dashboard"
                    .to_string(),
            });
        }

        // Checked before the transaction opens, which is this crate's rule and was this
        // call's comment before the rule was true of it: the check sat below the
        // `BEGIN IMMEDIATE` and the rollback was what covered it. Found by the review of
        // DEV-877. Nothing here reads the database, so there is no reason for it to be
        // inside a transaction and one good reason for it not to be.
        if let Some(sections) = &new.sections {
            composition::validate(sections)?;
        }

        // The status check and the write are one step, for the reason `add_finding`
        // gives: a draft approved between the two would otherwise be overwritten by a
        // caller that read it as `ready`. IMMEDIATE because SQLite will not upgrade a
        // read lock.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        if let Some(held) = self.find_draft(review_id)? {
            if !held.status.is_writable() {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found: held.status,
                    wanted: "only a draft that is drafting, ready or sent back can be \
                             rewritten",
                });
            }
        }

        let lint = new.lint.unwrap_or_default();
        // A lint that reported something is stamped with when it ran. A caller that
        // supplied no time gets now, because a lint result with no time cannot be told
        // from one that is a week old.
        let checked_at = if lint.is_empty() {
            None
        } else {
            Some(lint.checked_at.unwrap_or_else(|| self.now()))
        };

        let updated_at = self.now();
        tx.execute(
            "INSERT INTO drafts
                 (review_id, body, status, updated_at, lint_passes, lint_visible_words,
                  lint_word_budget, lint_findings_in_body, lint_findings_expected,
                  lint_raised_before_on_pr, lint_errors, lint_checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT (review_id) DO UPDATE SET
                 body                     = excluded.body,
                 status                   = excluded.status,
                 updated_at               = excluded.updated_at,
                 lint_passes              = excluded.lint_passes,
                 lint_visible_words       = excluded.lint_visible_words,
                 lint_word_budget         = excluded.lint_word_budget,
                 lint_findings_in_body    = excluded.lint_findings_in_body,
                 lint_findings_expected   = excluded.lint_findings_expected,
                 lint_raised_before_on_pr = excluded.lint_raised_before_on_pr,
                 lint_errors              = excluded.lint_errors,
                 lint_checked_at          = excluded.lint_checked_at",
            params![
                review_id,
                new.body,
                new.status,
                updated_at,
                lint.passes,
                lint.visible_words,
                lint.word_budget,
                lint.findings_in_body,
                lint.findings_expected,
                lint.raised_before_on_pr,
                lint.errors,
                checked_at,
            ],
        )
        .map_err(|error| unknown_review(error, review_id))?;

        // The blocks, then the body they compose to — in that order, because the draft row
        // has to exist before a block can refer to it.
        //
        // **A body given alongside the blocks is checked, not stored.** The one difference
        // forgiven is trailing newlines, which markdown does not distinguish; anything
        // else means the caller and the record would disagree about what gets posted, and
        // this is the check that keeps "the assembled draft of a review with no decisions
        // is what the agent wrote" true of every write rather than of the happy path.
        match &new.sections {
            Some(sections) => {
                composition::write(&tx, review_id, sections)?;
                let composed =
                    composition::recompose(&tx, review_id, composition::Lint::Keep, self.now())?;
                let given = new.body.trim_end_matches('\n');
                if !given.is_empty() && given != composed.trim_end_matches('\n') {
                    return Err(StoreError::InvalidValue {
                        field: "body",
                        value: "a body the blocks do not compose to".to_string(),
                        expected: "the body these blocks compose to, or no body at all. \
                                   The composition is what gets posted, and two answers to \
                                   what that is would be one too many"
                            .to_string(),
                    });
                }
            }
            // One prose block holding the whole body. Trimmed of the newlines around it
            // and composed back with exactly one, so that the column and the blocks agree
            // from the first write rather than from the second.
            None => {
                let whole = new.body.trim_matches('\n');
                let sections = if whole.is_empty() {
                    Vec::new()
                } else {
                    vec![NewDraftSection::prose(whole)]
                };
                composition::write(&tx, review_id, &sections)?;
                composition::recompose(&tx, review_id, composition::Lint::Keep, self.now())?;
            }
        }

        // **A draft that is ready is layer 6 waiting on the developer, and the trail has
        // to say so.** Nothing wrote that before: `approve_draft`, `send_back_draft` and
        // `post_draft` each move layer 6, and the one transition that left it alone was
        // the draft becoming ready in the first place — so the record could hold a draft
        // in the Inbox's "Sign off a draft" group under a trail still reading `pending`.
        // That is the disagreement `layer_trail.rs` exists to remove, and DEV-890 needs
        // the state change for a second reason: when the draft became ready is where "how
        // long was the review with the agent" stops, and this is the only record of it.
        //
        // **And the reverse: a ready draft taken back is no longer waiting on anybody.**
        // `put_draft` accepts `drafting` over a draft that is `ready`, which is a session
        // taking it back to work on — and leaving layer 6 at `waiting_on_you` would say
        // the developer was being waited on while the agent typed, keep a wait run open
        // that no one is in, and freeze "when the draft was ready" at an instant that is
        // no longer true. That is the stale trail this change exists to remove, on the one
        // transition that still had it.
        //
        // **Only from `waiting_on_you`.** A draft that was already `drafting` is the
        // session's business and the session reports it with `review-desk layer set`;
        // moving layer 6 to `running` on every part-written draft would put it in the
        // queue's "With the agent" group on the store's say-so rather than the session's.
        //
        // Inside the transaction, like the other three, so a draft and its trail are
        // written together or not at all. A state is recorded only when it changes, so the
        // developer editing a draft that is already ready does not restart the clock on
        // their own wait.
        let waiting = self
            .find_review_layer(review_id, ReviewLayerName::Draft.number())?
            .is_some_and(|layer| layer.state == ReviewLayerState::WaitingOnYou);
        match new.status {
            DraftStatus::Ready => {
                self.set_review_layer(
                    review_id,
                    ReviewLayerName::Draft.number(),
                    ReviewLayerState::WaitingOnYou,
                    Some(READY_DETAIL),
                )?;
            }
            DraftStatus::Drafting if waiting => {
                self.set_review_layer(
                    review_id,
                    ReviewLayerName::Draft.number(),
                    ReviewLayerState::Running,
                    Some(TAKEN_BACK_DETAIL),
                )?;
            }
            _ => {}
        }

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_draft(review_id)
    }

    /// Read the review's draft. Fails with [`StoreError::NotFound`] if there is none.
    pub fn get_draft(&self, review_id: i64) -> Result<Draft> {
        self.find_draft(review_id)?.ok_or(StoreError::NotFound {
            entity: "draft",
            id: review_id.to_string(),
        })
    }

    /// Read the review's draft, or `None`.
    pub fn find_draft(&self, review_id: i64) -> Result<Option<Draft>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM drafts WHERE review_id = ?1"),
                params![review_id],
                row_to_draft,
            )
            .optional()?)
    }

    /// Approve the review's draft as the developer, choosing the GitHub review event it
    /// is to be posted as.
    ///
    /// **This does not post anything.** It sets the status, the event and the time; the
    /// review session finds the approved draft and posts it, then reports the GitHub
    /// review id through [`Store::post_draft`].
    ///
    /// `by` is required and must be [`Actor::Developer`]. Only the developer signs off,
    /// and the store is the only way into the record, so this is the one place that can
    /// hold that line. Approving again with the same event changes nothing, so a
    /// retried call is safe; approving at any other status is refused.
    pub fn approve_draft(&self, review_id: i64, event: ReviewEvent, by: Actor) -> Result<Draft> {
        only_the_developer(by, "approve a draft")?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let draft = self.get_draft(review_id)?;

        // The same approval twice is one approval, so the second one writes no draft
        // columns — but it still writes layer 6, for the reason `confirm_brief` gives:
        // a draft approved before this behaviour existed has a trail that says layer 6
        // is waiting, and approving again is the only call that can put it right. A
        // *different* event is not the same approval: it would change what gets posted,
        // and the first choice has already been reported as made.
        let already_approved = match draft.status {
            DraftStatus::Ready => false,
            DraftStatus::Approved if draft.event == Some(event) => true,
            found => {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found,
                    wanted: "only a draft that is ready can be approved",
                })
            }
        };

        if !already_approved {
            write_approval(&self.conn, review_id, event, by, self.now())?;
        }

        self.set_review_layer(
            review_id,
            ReviewLayerName::Draft.number(),
            ReviewLayerState::Done,
            Some(APPROVED_DETAIL),
        )?;

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_draft(review_id)
    }

    /// Send the review's draft back with the instruction to change it.
    ///
    /// The note is required: a bare rejection teaches the session nothing, and the
    /// whole point of the record is that it can be learned from.
    pub fn send_back_draft(&self, review_id: i64, note: &str) -> Result<Draft> {
        let note = note.trim();
        if note.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "sent_back_note",
                value: note.to_string(),
                expected: "what the session is to change".to_string(),
            });
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let draft = self.get_draft(review_id)?;

        let already_sent_back = match draft.status {
            DraftStatus::Ready => false,
            DraftStatus::SentBack if draft.sent_back_note.as_deref() == Some(note) => true,
            found => {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found,
                    wanted: "only a draft that is ready can be sent back",
                })
            }
        };

        if !already_sent_back {
            let at = self.now();
            self.conn.execute(
                "UPDATE drafts SET
                     status         = ?2,
                     sent_back_note = ?3,
                     sent_back_at   = ?4,
                     updated_at     = ?4
                 WHERE review_id = ?1",
                params![review_id, DraftStatus::SentBack, note, at],
            )?;
        }

        // **Back in progress, carrying the note.** Not `waiting_on_you`: the developer
        // has acted and it is the session's turn again. The note is the detail verbatim
        // rather than a count or a phrase, because what the developer asked for is the
        // only thing that makes "layer 6, in progress" mean anything to whoever reads
        // the trail next.
        self.set_review_layer(
            review_id,
            ReviewLayerName::Draft.number(),
            ReviewLayerState::Running,
            Some(note),
        )?;

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_draft(review_id)
    }

    /// Record that the review session has posted the approved draft, as this GitHub
    /// review.
    ///
    /// **Refused unless the draft is approved.** This is the only transition that
    /// involves GitHub, and the session makes it: Review Desk never posts. The draft's
    /// `posted_at` and the review's `github_review_id` are one fact, so they are
    /// written in one transaction — a posted draft nobody can find the review for would
    /// be worse than either write failing.
    ///
    /// Reporting the same GitHub review twice changes nothing.
    pub fn post_draft(&self, review_id: i64, github_review_id: &str) -> Result<Draft> {
        let github_review_id = github_review_id.trim();
        if github_review_id.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "github_review_id",
                value: github_review_id.to_string(),
                expected: "the id of the review posted on GitHub".to_string(),
            });
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let draft = self.get_draft(review_id)?;
        let review = self.get_review(review_id)?;

        let already_posted = match draft.status {
            DraftStatus::Approved => false,
            DraftStatus::Posted if review.github_review_id.as_deref() == Some(github_review_id) => {
                true
            }
            found => {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found,
                    wanted: "only an approved draft can be marked posted",
                })
            }
        };

        if !already_posted {
            let at = self.now();
            self.conn.execute(
                "UPDATE drafts SET status = ?2, posted_at = ?3, updated_at = ?3
                 WHERE review_id = ?1",
                params![review_id, DraftStatus::Posted, at],
            )?;
            self.conn.execute(
                "UPDATE reviews SET github_review_id = ?2 WHERE id = ?1",
                params![review_id, github_review_id],
            )?;
        }

        // **Layer 7 is `pending`, not `done`.** Posting is what makes a harvest possible;
        // it is not the harvest. `done` here would say the outcomes had been read back
        // from GitHub, which is the one thing nobody has done yet — and layer 7 is the
        // whole feedback loop, so a layer trail that claimed it was finished would make
        // the record lie about the thing the project exists for.
        //
        // **Unless the harvest has already run.** Reporting the same GitHub review twice
        // is a supported call — the `Posted` arm above exists so that a session retrying
        // after a crash succeeds — and layer 7 is the one layer whose true state can be
        // *past* what this writes. Written unconditionally, a retry a week later would
        // move a harvested layer 7 back to `pending` and throw away the harvester's own
        // detail. Nothing else here can regress, because no state is past `done`.
        let harvested = self
            .find_review_layer(review_id, ReviewLayerName::Harvest.number())?
            .is_some_and(|layer| layer.state == ReviewLayerState::Done);
        if !harvested {
            self.set_review_layer(
                review_id,
                ReviewLayerName::Harvest.number(),
                ReviewLayerState::Pending,
                Some(AWAITING_HARVEST_DETAIL),
            )?;
        }

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_draft(review_id)
    }
}

/// Write the approval: the status, the event, the time and whose assent it was.
///
/// **One statement, two callers.** [`Store::approve_draft`] is "Sign off and post", and
/// `Store::ticket_rest_and_approve` is "Ticket what is left, and approve" — which approves
/// in the same transaction as it moves the open findings into a ticket. A second copy of
/// this statement is a second place for `approved_by` to be left out.
pub(crate) fn write_approval(
    conn: &Connection,
    review_id: i64,
    event: ReviewEvent,
    by: Actor,
    at: Timestamp,
) -> Result<()> {
    conn.execute(
        "UPDATE drafts SET
             status      = ?2,
             event       = ?3,
             approved_at = ?4,
             approved_by = ?5,
             updated_at  = ?4
         WHERE review_id = ?1",
        params![review_id, DraftStatus::Approved, event, at, by],
    )?;

    Ok(())
}

/// A draft whose review is absent is a named failure, not a raw SQLite one.
fn unknown_review(error: rusqlite::Error, review_id: i64) -> StoreError {
    if is_foreign_key_violation(&error) {
        StoreError::UnknownParent {
            entity: "draft",
            parent: "review",
            id: review_id.to_string(),
        }
    } else {
        StoreError::Sqlite(error)
    }
}

/// Build a draft from a row selecting [`COLUMNS`].
fn row_to_draft(row: &Row<'_>) -> rusqlite::Result<Draft> {
    Ok(Draft {
        review_id: row.get("review_id")?,
        body: row.get("body")?,
        status: row.get("status")?,
        event: row.get("event")?,
        approved_at: row.get("approved_at")?,
        approved_by: row.get("approved_by")?,
        sent_back_note: row.get("sent_back_note")?,
        sent_back_at: row.get("sent_back_at")?,
        posted_at: row.get("posted_at")?,
        lint: DraftLint {
            passes: row.get::<_, Option<i64>>("lint_passes")?.map(|on| on != 0),
            visible_words: row.get("lint_visible_words")?,
            word_budget: row.get("lint_word_budget")?,
            findings_in_body: row.get("lint_findings_in_body")?,
            findings_expected: row.get("lint_findings_expected")?,
            raised_before_on_pr: row.get("lint_raised_before_on_pr")?,
            errors: row.get("lint_errors")?,
            checked_at: row.get::<_, Option<Timestamp>>("lint_checked_at")?,
        },
        updated_at: row.get("updated_at")?,
    })
}
