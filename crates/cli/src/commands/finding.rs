//! `finding add` and `finding set`.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{Disposition, NewFinding, Severity, Store};
use serde_json::json;

use crate::error::Result;
use crate::input::read_json;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum FindingCommand {
    /// Add one finding to a review, and print its sequence number within it.
    ///
    /// The document is `{"lens": "...", "severity": "...", "claim": "...", "file":
    /// "...", "line": 12, "evidence": "...", "disposition": "...",
    /// "disposition_reason": "..."}`. `lens`, `severity` and `claim` are required, the
    /// disposition defaults to `in_draft`, and a field that is not one of those eight is
    /// refused rather than quietly dropped.
    Add {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Change what is to become of a finding in this round's draft.
    Set {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The finding's sequence number within the review.
        #[arg(long, value_name = "FID")]
        finding: i64,
        /// One of: in_draft, held_back, suppressed.
        #[arg(long, value_name = "VALUE")]
        disposition: String,
        /// Why it was held back or suppressed.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
    },
}

pub fn run(command: &FindingCommand) -> Result<Report> {
    match command {
        FindingCommand::Add { review, file } => {
            let new: NewFinding = read_json(
                file,
                Some(&format!(
                    "severity is one of: {}; disposition is one of: {}",
                    Severity::valid_values(),
                    Disposition::valid_values()
                )),
            )?;
            let store = Store::open_default()?;
            let finding = store.add_finding(*review, new)?;

            Ok(Report::new(
                json!({ "finding": finding }),
                // The sequence number alone: it is what the next command needs and what
                // a person quotes.
                finding.seq.to_string(),
            ))
        }

        FindingCommand::Set {
            review,
            finding,
            disposition,
            reason,
        } => {
            // Parsed here rather than by clap, so an unknown value is refused as an
            // invalid value — with the valid ones named — instead of as a usage error.
            let disposition: Disposition = disposition.parse()?;

            let store = Store::open_default()?;
            let changed =
                store.set_finding_disposition(*review, *finding, disposition, reason.clone())?;

            let mut plain = format!(
                "finding {} of review {} is {}",
                changed.seq, changed.review_id, changed.disposition
            );
            if let Some(reason) = &changed.disposition_reason {
                plain.push_str(&format!(": {reason}"));
            }

            Ok(Report::new(json!({ "finding": changed }), plain))
        }
    }
}
