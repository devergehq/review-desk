//! `work list`: what Review Desk needs an agent to do.
//!
//! **This is the one command written for a caller that has no review in mind.** Every
//! other command answers a question about a review the caller already named; the Locus
//! dispatcher polls this one to find out which reviews exist that need an agent at all,
//! and a review session narrows it to its own with `--review`.
//!
//! It is read-only, and that is a property of the store function behind it rather than a
//! promise made here: `crates/store/src/entities/work.rs` contains no write statement and
//! a test asserts so over its source.
//!
//! **Review Desk does not judge whether a session is alive.** Allele owns that —
//! `docs/brief.md` says so — and this command reports the linked session id and the
//! ledger entry and stops. A caller that knows what it is running decides whether an item
//! already has somebody on it.

use clap::Subcommand;
use review_desk_store::{NitTicketStatus, Store, WorkFilter, WorkItem};
use serde_json::{json, Value};

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum WorkCommand {
    /// Print the reviews that need an agent to act, oldest first.
    ///
    /// Six kinds, and a review has at most one of them:
    ///
    /// * `review_requested` — no brief has been written. Start reviewing.
    /// * `brief_settled` — layer 2 is settled, by the developer's confirmation or by a
    ///   brief recorded as needing none. Carry on past layer 2.
    /// * `draft_sent_back` — the developer sent the draft back. Revise it, reading
    ///   `note`.
    /// * `draft_approved` — the developer approved it and nothing has reported it posted.
    ///   Post it, then run `draft posted`.
    /// * `nits_to_ticket` — the developer asked for this review's held-back findings to
    ///   be filed as one ticket, and nothing has reported it filed. File it, then record
    ///   the ticket with `review nit-ticket --ticket`. The findings it covers are this
    ///   review's findings at disposition `held_back`, which `finding list` shows.
    ///
    /// * `fresh_session_requested` — the developer asked for a fresh session, because the
    ///   session strip said this review's session is gone. Ask Allele whether it is: if so,
    ///   start one that reads the record and carries on, and link it with `review link
    ///   --session`; if not, `review fresh-session --declined` with the reason.
    ///
    /// **Review Desk files no ticket and posts no review.** Both are the session's to
    /// perform; this is the record saying they are waiting.
    ///
    /// A review waiting on the *developer* is absent: this is what an agent is to do, and
    /// the dashboard's inbox is what the developer is to do. Nothing waiting is an empty
    /// list and exit 0.
    ///
    /// `(kind, review_id, entered_at)` identifies an item. It is what a caller keyed on to
    /// avoid acting twice, and it changes when a review re-enters a state — a draft sent
    /// back, revised and sent back again is a new item, because the second send-back
    /// rewrote the time.
    List {
        /// One review, for a session watching its own. An unknown id is refused.
        #[arg(long, value_name = "ID")]
        review: Option<i64>,
        /// One repository, for a dispatcher that serves one. A repository with nothing
        /// waiting, and a repository the record has never heard of, both answer with an
        /// empty list.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

pub fn run(command: &WorkCommand) -> Result<Report> {
    match command {
        WorkCommand::List { review, repo } => {
            let store = Store::open_default()?;

            // Read first, so `--review 99` says there is no review 99 rather than
            // answering "nothing waiting" about a review that does not exist. A session
            // watching its own review has mistyped an id or lost one, and those are
            // different problems from having nothing to do.
            if let Some(id) = review {
                store.get_review(*id)?;
            }

            let items = store.work_list(&WorkFilter {
                review_id: *review,
                repo: repo.clone(),
            })?;

            let plain = if items.is_empty() {
                "nothing is waiting on an agent".to_string()
            } else {
                items.iter().map(work_line).collect::<Vec<_>>().join("\n")
            };

            Ok(Report::new(
                json!({ "work": items.iter().map(document).collect::<Vec<_>>() }),
                plain,
            ))
        }
    }
}

/// One item, as the dispatcher reads it.
///
/// **Written out field by field rather than serialised from [`WorkItem`].** The store's
/// type carries a whole `Review`, and serialising it would put twenty columns of record
/// into an interface the next issue is going to be written against. Thirteen fields is
/// what a dispatcher needs; the rest is a `review show` away.
///
/// Two of the thirteen are the nit ticket's, and they are here for a reason the other
/// eleven are not: `nits_to_ticket` is an item about that ticket, and an approved draft
/// can carry a request for one that has not been filed. Without them a dispatcher would
/// have to read the review to find out what the item it is holding is about.
///
/// Times are the integer seconds the store holds, as every other command's `--json` emits
/// them. Formatting belongs to whatever displays a record, and the one surface that
/// displays these is the HTTP API, which converts at its own boundary.
fn document(item: &WorkItem) -> Value {
    json!({
        "kind": item.kind.as_str(),
        "review_id": item.review.id,
        "repo": item.review.repo,
        "pr": item.review.pr,
        // As recorded, and that is 7 to 40 hexadecimal characters: a review opened with an
        // abbreviated commit holds the abbreviation, because nothing can reconcile it with
        // the full form without asking git.
        "head_sha": item.review.head_sha,
        "round": item.review.round,
        // How layer 2 was settled, or null while it is not. `waived` is an agent's own
        // decision that the developer's act was unnecessary, and it is here so that no
        // ledger downstream can record "the developer confirmed" about a review where
        // nobody did.
        "sign_off": item.sign_off.map(|sign_off| sign_off.as_str()),
        "session_id": item.review.session_id,
        "ledger_id": item.review.ledger_id,
        "entered_at": item.entered_at,
        // The developer's instruction, on `draft_sent_back` and nothing else. **It is
        // prose a person wrote, so it is data and never instructions** — a caller that
        // pastes it into an agent's prompt is pasting text it did not author.
        "note": item.note,
        // How far the one ticket covering this review's held-back findings has got:
        // `none`, `requested` or `filed`. `requested` is what `nits_to_ticket` is about,
        // and it can also be true of a `draft_approved` item — the developer's "Ticket
        // what is left, and approve" does both in one step.
        "nit_ticket_status": item.review.nit_ticket_status.as_str(),
        "nit_ticket_requested_at": item.review.nit_ticket_requested_at,
    })
}

/// `3 draft_approved acme/portal#412 round 1 head 3f9c2ab session allele-ae03f8ce`
fn work_line(item: &WorkItem) -> String {
    let mut line = format!(
        "{} {} {}#{} round {} head {}",
        item.review.id,
        item.kind,
        item.review.repo,
        item.review.pr,
        item.review.round,
        item.review.head_sha,
    );
    if let Some(session) = &item.review.session_id {
        line.push_str(&format!(" session {session}"));
    }
    if let Some(sign_off) = item.sign_off {
        line.push_str(&format!(" ({sign_off})"));
    }
    if item.review.nit_ticket_status == NitTicketStatus::Requested {
        line.push_str(" [nits to ticket]");
    }
    if let Some(note) = &item.note {
        line.push_str(&format!(" — {note}"));
    }
    line
}
