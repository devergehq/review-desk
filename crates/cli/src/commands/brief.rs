//! `brief put`, and the refusal that is all `brief confirm` is now.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{NewBrief, Store};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::input::read_json;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum BriefCommand {
    /// Store the review's brief, replacing any brief already held.
    ///
    /// Replacing clears the confirmation: the developer confirmed the text that was
    /// there, not whatever replaces it.
    ///
    /// The document is `{"problem": "...", "diagram": "...", "options": [...],
    /// "approach_verdict": "...", "provenance": "...", "confirmation_required": true,
    /// "confirmation_not_required_because": "..."}`. Only `problem` is required, and a
    /// field that is not one of those is refused.
    ///
    /// `confirmation_required` defaults to true. Set false — with
    /// `confirmation_not_required_because` saying why, which is then required — it records
    /// that the caller decided the developer's act at layer 2 was unnecessary, typically a
    /// change under the caller's own size threshold. Layer 2 is `done` on write and the
    /// review does not appear in the developer's inbox.
    ///
    /// **A brief already written as requiring confirmation cannot be rewritten as needing
    /// none**, and that refusal is the whole of what makes the field safe to have: it can
    /// only ever be set before the developer was asked. The other direction is allowed.
    ///
    /// `options` is the alternatives the review considered before it read the diff, and is
    /// what the One review screen's "Ways to solve it" shows — one object per alternative,
    /// `{"key": "A", "title": "...", "argument_for": "...", "argument_against": "...",
    /// "chosen": false}`, with `chosen` true on the one the author built.
    ///
    /// `option_map` is the same fact as one opaque string and is superseded; giving both is
    /// refused rather than merged, because nothing could say which the review meant. This
    /// text listed only `option_map` for longer than it was true, which is how a session
    /// came to seed a brief with no options at all.
    Put {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Removed: confirming a brief happens in the dashboard.
    ///
    /// Kept as a command so that running it says so. Deleting the subcommand outright
    /// would exit 1 as well — clap's usage error is remapped to 1 — but with clap's
    /// "unrecognized subcommand" text, which tells a caller nothing about where
    /// confirmation went. Hidden from `--help` because it is not something to run.
    #[command(hide = true)]
    Confirm {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: Option<i64>,
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

        // **Only the developer signs off**, decided on 9 October 2026. An agent could
        // otherwise confirm its own understanding of a problem and release every later
        // layer of its own review, which is the one thing the brief exists to stop.
        //
        // The database is not opened and nothing is read: the answer does not depend on
        // the record, and a removed command should not create a database as a side
        // effect.
        BriefCommand::Confirm { .. } => Err(CliError::bad_input(
            "brief confirm has been removed",
            "confirming a brief is the developer's sign-off and is done in the \
             dashboard; an agent session reads the confirmation instead, through \
             `review show`",
        )),
    }
}
