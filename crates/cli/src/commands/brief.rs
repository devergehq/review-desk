//! `brief put` and `brief confirm`.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{Brief, NewBrief, Store};
use serde_json::json;

use crate::error::Result;
use crate::input::read_json;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum BriefCommand {
    /// Store the review's brief, replacing any brief already held.
    ///
    /// Replacing clears the confirmation: the developer confirmed the text that was
    /// there, not whatever replaces it. The document is `{"problem": "...",
    /// "diagram": "...", "option_map": "...", "approach_verdict": "..."}`; only
    /// `problem` is required, and a field that is not one of those four is refused.
    Put {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Mark the review's brief confirmed by the developer.
    ///
    /// Confirming twice keeps the first confirmation time.
    Confirm {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
    },
}

pub fn run(command: &BriefCommand) -> Result<Report> {
    match command {
        BriefCommand::Put { review, file } => {
            // Read the document before opening the database: a brief that cannot be
            // parsed should not create a database file as a side effect.
            // A brief holds no closed set, so there is nothing to name.
            let new: NewBrief = read_json(file, None)?;
            let store = Store::open_default()?;
            let brief = store.put_brief(*review, new)?;

            Ok(Report::new(
                json!({ "brief": brief }),
                format!("brief stored for review {}", brief.review_id),
            ))
        }

        BriefCommand::Confirm { review } => {
            let store = Store::open_default()?;
            let brief = store.confirm_brief(*review)?;

            Ok(Report::new(
                json!({ "brief": brief }),
                confirmed_line(&brief),
            ))
        }
    }
}

fn confirmed_line(brief: &Brief) -> String {
    match brief.confirmed_at {
        Some(at) => format!("brief confirmed for review {} at {at}", brief.review_id),
        // confirm_brief always sets a time, so this is unreachable in practice. Saying
        // so beats an unwrap that would turn a surprise into a panic.
        None => format!("brief for review {} is not confirmed", brief.review_id),
    }
}
