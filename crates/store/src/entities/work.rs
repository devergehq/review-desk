//! What a review needs an agent to do, for an agent that is asking.
//!
//! Everything else in this crate answers a question about a review somebody already
//! named. This answers the opposite one — *which* reviews need an agent at all — for the
//! Locus dispatcher, which has no review in mind, and for a session watching its own.
//!
//! **This module only reads.** `work_list_contains_no_write_statement` asserts that over
//! this file's own source, because "it is a read" is a claim about code that can stop
//! being true one convenience at a time, and a command the dispatcher polls every minute
//! is the last place a write belongs.
//!
//! # Why the kind is decided in Rust and not in SQL
//!
//! [`decide`] is a total function over three small values, which is what makes "a review
//! has at most one kind" checkable rather than asserted. Four `WHERE` clauses in four
//! queries would be four chances for two of them to match the same review and hand an
//! agent two contradictory instructions — and that exact fault was found in this design
//! before it was written, by an adversarial pass over the earlier version.
//!
//! # Why it is level-triggered
//!
//! An earlier draft asked whether anything had *started*: `review_requested` wanted no
//! layer past `pending`, `brief_settled` wanted layers 3 to 7 untouched. That reads
//! naturally and is wrong, because the agent's own first write falsifies it. A session
//! dispatched for `review_requested` that wrote layer 1 and then died matched no kind
//! afterwards and was invisible for ever.
//!
//! So nothing here asks what has started. It asks what is *outstanding*, from the draft's
//! status and layer 2 — both of which a dead session leaves exactly as it found them. A
//! dispatcher that restarts, or loses its ledger, converges on the same list.
//!
//! **Review Desk still does not judge whether a session is alive.** Allele owns that, and
//! `docs/brief.md` says so. An item carries `session_id` and the caller decides; being
//! level-triggered is what makes that decision possible at all, because a review nothing
//! lists is one no liveness check can be applied to.

use rusqlite::{params, Row};

use crate::entities::inbox::{row_to_review, REVIEW_COLUMNS};
use crate::error::Result;
use crate::model::{DraftStatus, Review, SignOff, Timestamp, WorkFilter, WorkItem, WorkKind};
use crate::{normalise, Store};

impl Store {
    /// Every review that needs an agent to act, oldest first.
    ///
    /// A review waiting on the *developer* is absent, which is the whole purpose: this is
    /// the list of what an agent is to do, and the Inbox is the list of what the developer
    /// is to do. A review with nothing outstanding yields no item, and no item is a valid
    /// answer.
    ///
    /// **Only the newest round of a pull request is considered.** A round superseded by a
    /// new head commit would otherwise keep asking for an agent for ever — nothing moves
    /// its state again, so nothing could retract it — and oldest-first would pin that
    /// dead round at the head of the queue in front of live work.
    pub fn work_list(&self, filter: &WorkFilter) -> Result<Vec<WorkItem>> {
        // Normalised through the same helper the insert uses. A caller who typed
        // `Acme/Portal` is naming the repository the record holds as `acme/portal`, and an
        // unnormalised filter here would answer "nothing waiting" rather than "no such
        // repository".
        let repo = filter.repo.as_deref().map(normalise::repo);

        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS},
                    b.confirmed_at          AS brief_confirmed_at,
                    b.confirmation_required AS brief_confirmation_required,
                    b.updated_at            AS brief_updated_at,
                    d.status                AS draft_status,
                    d.sent_back_at          AS draft_sent_back_at,
                    d.sent_back_note        AS draft_sent_back_note,
                    d.approved_at           AS draft_approved_at,
                    d.updated_at            AS draft_updated_at
             FROM reviews v
             LEFT JOIN briefs b ON b.review_id = v.id
             LEFT JOIN drafts d ON d.review_id = v.id
             WHERE v.round = (
                     SELECT MAX(round) FROM reviews
                     WHERE repo = v.repo AND pr = v.pr
                   )
               AND (?1 IS NULL OR v.id = ?1)
               AND (?2 IS NULL OR v.repo = ?2)"
        ))?;

        let candidates = statement
            .query_map(params![filter.review_id, repo], row_to_candidate)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut items: Vec<WorkItem> = candidates.into_iter().filter_map(decide).collect();

        // Oldest first, so a dispatcher serving one item at a time serves the thing that
        // has been waiting longest. Tie-broken by review id, which is stable: two reviews
        // can enter a state in the same second, and a list whose order changed between
        // two polls of an unchanged record would make a caller's progress look like
        // churn.
        items.sort_by_key(|item| (item.entered_at, item.review.id));
        Ok(items)
    }
}

/// A review's own facts plus the two rows that decide what it needs.
#[derive(Debug)]
struct Candidate {
    review: Review,
    brief: Option<BriefState>,
    draft: Option<DraftState>,
}

/// What layer 2 holds. Nothing about the brief's text: only whether it is finished with.
#[derive(Debug)]
struct BriefState {
    confirmed_at: Option<Timestamp>,
    confirmation_required: bool,
    /// When the brief was last written, which is when a waived one settled layer 2.
    updated_at: Timestamp,
}

impl BriefState {
    /// How layer 2 was settled, or `None` while it is not.
    ///
    /// A confirmation outranks a waiver: if the developer confirmed a brief that needed no
    /// confirming, they did confirm it, and the record should say the stronger true thing.
    fn sign_off(&self) -> Option<SignOff> {
        match (self.confirmed_at, self.confirmation_required) {
            (Some(_), _) => Some(SignOff::Developer),
            (None, false) => Some(SignOff::Waived),
            (None, true) => None,
        }
    }

    /// When layer 2 was settled, if it is.
    fn settled_at(&self) -> Option<Timestamp> {
        match (self.confirmed_at, self.confirmation_required) {
            (Some(at), _) => Some(at),
            // A waived brief settled layer 2 the moment it was written, so its own
            // `updated_at` is the honest answer and the only one there is.
            (None, false) => Some(self.updated_at),
            (None, true) => None,
        }
    }
}

/// Where the draft has got to, and when it got there.
#[derive(Debug)]
struct DraftState {
    status: DraftStatus,
    sent_back_at: Option<Timestamp>,
    sent_back_note: Option<String>,
    approved_at: Option<Timestamp>,
    /// The fallback time. The schema makes `approved_at` and `sent_back_at` non-null at
    /// those statuses, so this is never reached in practice — but a row builder that
    /// unwrapped them would turn a database this build does not understand into a panic.
    updated_at: Timestamp,
}

/// What this review needs an agent to do, or nothing.
///
/// **One function, one order, at most one answer.** The draft is asked first because an
/// approved or returned draft is the most advanced thing outstanding: a review whose draft
/// the developer has just approved does not need an agent to go back and reconsider
/// layer 2, whatever the layers say.
///
/// `ready` and `posted` answer `None` for opposite reasons — the first is waiting on the
/// developer, the second is finished with — and `drafting` falls through to layer 2,
/// because a draft nobody finished is not something the developer has been handed.
fn decide(candidate: Candidate) -> Option<WorkItem> {
    let Candidate {
        review,
        brief,
        draft,
    } = candidate;

    let sign_off = brief.as_ref().and_then(BriefState::sign_off);
    let item = |kind: WorkKind, entered_at: Timestamp, note: Option<String>| {
        Some(WorkItem {
            kind,
            review: review.clone(),
            sign_off,
            entered_at,
            note,
        })
    };

    if let Some(draft) = &draft {
        match draft.status {
            DraftStatus::Approved => {
                return item(
                    WorkKind::DraftApproved,
                    draft.approved_at.unwrap_or(draft.updated_at),
                    None,
                )
            }
            DraftStatus::SentBack => {
                return item(
                    WorkKind::DraftSentBack,
                    draft.sent_back_at.unwrap_or(draft.updated_at),
                    draft.sent_back_note.clone(),
                )
            }
            // The developer's turn, and nobody's turn.
            DraftStatus::Ready | DraftStatus::Posted => return None,
            // Unfinished, so what is outstanding is decided by layer 2 below.
            DraftStatus::Drafting => {}
        }
    }

    match &brief {
        // No brief, so layer 2 has not been written and an agent has to start. This is
        // what `POST /reviews` leaves behind when the developer pastes a link, and it is
        // also what a session that died before writing a brief leaves behind — which is
        // the point of asking what is outstanding rather than what has begun.
        None => item(WorkKind::ReviewRequested, review.created_at, None),
        // Layer 2 is finished with, by a confirmation or by a waiver, and no draft is
        // waiting on the developer. An agent carries on.
        Some(brief) => match brief.settled_at() {
            Some(settled_at) => item(WorkKind::BriefSettled, settled_at, None),
            // Written and not settled: the developer is being asked, and nothing an agent
            // does should pre-empt that.
            None => None,
        },
    }
}

/// Build a candidate from a row of [`Store::work_list`]'s query.
fn row_to_candidate(row: &Row<'_>) -> rusqlite::Result<Candidate> {
    // A LEFT JOIN that matched nothing leaves every column of that side null, so the
    // brief's `updated_at` — which the table makes NOT NULL — is what says whether there
    // is a brief at all. Keying off `confirmed_at` would read an unconfirmed brief as no
    // brief, which is the difference between "start reviewing" and "wait for the
    // developer".
    let brief = row
        .get::<_, Option<Timestamp>>("brief_updated_at")?
        .map(|updated_at| -> rusqlite::Result<BriefState> {
            Ok(BriefState {
                confirmed_at: row.get("brief_confirmed_at")?,
                confirmation_required: row.get("brief_confirmation_required")?,
                updated_at,
            })
        })
        .transpose()?;

    let draft = row
        .get::<_, Option<DraftStatus>>("draft_status")?
        .map(|status| -> rusqlite::Result<DraftState> {
            Ok(DraftState {
                status,
                sent_back_at: row.get("draft_sent_back_at")?,
                sent_back_note: row.get("draft_sent_back_note")?,
                approved_at: row.get("draft_approved_at")?,
                updated_at: row.get("draft_updated_at")?,
            })
        })
        .transpose()?;

    Ok(Candidate {
        review: row_to_review(row)?,
        brief,
        draft,
    })
}
