//! `outcome add`.

use clap::Subcommand;
use review_desk_store::{Actor, FindingRef, NewOutcome, OutcomeKind, Store};
use serde_json::json;

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum OutcomeCommand {
    /// Record what became of a finding.
    Add {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The finding's sequence number within the review.
        #[arg(long, value_name = "FID")]
        finding: i64,
        /// One of: cut, regraded, rewritten, kept, rejected, raised_again, replied,
        /// thread_resolved, code_changed_after.
        #[arg(long, value_name = "VALUE")]
        kind: String,
        /// Who brought it about. One of: developer, author, harvester.
        #[arg(long = "by", value_name = "VALUE")]
        by: String,
        /// Anything worth saying beyond the kind.
        #[arg(long, value_name = "TEXT")]
        detail: Option<String>,
        /// When it happened, in seconds since the epoch. Defaults to now.
        #[arg(long = "at", value_name = "SECONDS")]
        at: Option<i64>,
    },
}

pub fn run(command: &OutcomeCommand) -> Result<Report> {
    match command {
        OutcomeCommand::Add {
            review,
            finding,
            kind,
            by,
            detail,
            at,
        } => {
            // Both parsed before the database is opened, so an unknown kind is refused
            // with the valid values named rather than reported as a usage error.
            let kind: OutcomeKind = kind.parse()?;
            let actor: Actor = by.parse()?;

            let store = Store::open_default()?;
            let outcome = store.add_outcome(
                FindingRef {
                    review_id: *review,
                    seq: *finding,
                },
                NewOutcome {
                    kind,
                    actor,
                    detail: detail.clone(),
                    occurred_at: *at,
                },
            )?;

            let mut plain = format!(
                "outcome {} on finding {} of review {}: {} by {}",
                outcome.id, outcome.finding_seq, outcome.review_id, outcome.kind, outcome.actor
            );
            if let Some(detail) = &outcome.detail {
                plain.push_str(&format!(" — {detail}"));
            }

            Ok(Report::new(json!({ "outcome": outcome }), plain))
        }
    }
}
