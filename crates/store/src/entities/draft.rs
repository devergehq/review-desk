//! The draft review: one per review, replaced in place.
//!
//! **This module is where the sign-off decision is enforced, not merely described.**
//! Whatever writes a draft goes through [`Store::put_draft`], which accepts only
//! `drafting` and `ready`. `approved` is reachable through [`Store::approve_draft`] and
//! `posted` through [`Store::post_draft`], and nothing else can set either. An agent
//! can therefore write a draft and read its status, and cannot sign it off, without
//! that resting on the agent's good manners.
//!
//! **Each of the three transitions below also writes its layer row, in the same
//! transaction.** Approving finishes layer 6, sending back puts it in progress carrying
//! the developer's note, and marking a draft posted leaves layer 7 `pending` — waiting
//! for a harvest that has not happened — rather than `done`. Until DEV-864 none of them
//! wrote a layer at all, so the trail went stale the moment the developer acted.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::entities::only_the_developer;
use crate::error::{Result, StoreError};
use crate::model::{
    now, Actor, Draft, DraftLint, DraftStatus, NewDraft, ReviewEvent, ReviewLayerName,
    ReviewLayerState, Timestamp,
};
use crate::{is_foreign_key_violation, Store};

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
            Some(lint.checked_at.unwrap_or_else(now))
        };

        let updated_at = now();
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
            self.conn.execute(
                "UPDATE drafts SET
                     status      = ?2,
                     event       = ?3,
                     approved_at = ?4,
                     approved_by = ?5,
                     updated_at  = ?4
                 WHERE review_id = ?1",
                params![review_id, DraftStatus::Approved, event, now(), by],
            )?;
        }

        self.set_review_layer(
            review_id,
            ReviewLayerName::Draft.number(),
            ReviewLayerState::Done,
            Some(APPROVED_DETAIL),
        )?;

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
            let at = now();
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
            let at = now();
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
        self.set_review_layer(
            review_id,
            ReviewLayerName::Harvest.number(),
            ReviewLayerState::Pending,
            Some(AWAITING_HARVEST_DETAIL),
        )?;

        tx.commit()?;
        self.get_draft(review_id)
    }
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
