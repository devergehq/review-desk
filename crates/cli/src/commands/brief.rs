//! `brief put`, `brief problem-statement`, and the refusal that is all `brief confirm`
//! is now.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{NewBrief, ProblemStatement, Store};
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
    ///
    /// ## The structure the three approved boards draw
    ///
    /// Everything below is optional, and a brief that sends none of it reads back exactly
    /// as it did before. **Every field is plain text**; nothing renders markup.
    ///
    /// * `headline` — the one sentence the Understand page opens with.
    /// * `how_it_works_today` — the context a newcomer needs, in plain words.
    /// * `flow_before`, `flow_after` — the two flows, each a list of rows and each row a
    ///   list of one to three boxes: `[[{"title": "...", "note": "...", "problem": 1}]]`.
    ///   Two boxes in one row are the split row the board draws side by side.
    ///   `flow_before_caption` and `flow_after_caption` are the sub-captions above them.
    /// * `problems` — the numbered problems:
    ///   `{"number": 1, "was_wrong": "...", "now_fixed": "...", "detail_title": "...",
    ///   "detail_before": ["..."], "detail_after": ["..."]}`. **The number is the brief's
    ///   one internal identity**, and a flow box, an option verdict or a solution part
    ///   naming a number no problem carries is refused, with the field and the number.
    /// * `support_table` — `{"title": "...", "columns": ["..."], "rows": [["..."]]}`. Every
    ///   row holds one cell per column.
    /// * `options[].proposed_by` — `ticket`, `blind_pass` or `both`. Defaults to `ticket`.
    /// * `options[].verdicts` — `{"problem": 1, "verdict": "fixed | partly | stays |
    ///   not_assessed", "why": "..."}`. `why` is required for all but `not_assessed`.
    /// * `blind_pass` — what a session reported a blind pass doing:
    ///   `{"given": "...", "pick_key": "C", "pick_why": "...", "looked_up": "nothing",
    ///   "questions": ["..."]}`. **Review Desk runs no blind pass**; this records one.
    ///   Leaving it out, sending it with no pick, and sending it with options whose
    ///   `proposed_by` names it are three distinguishable states.
    /// * `parts` — the change in parts: `{"title": "...", "summary": "...", "fixes": [1],
    ///   "files": [{"path": "...", "lines_added": 0, "lines_removed": 0,
    ///   "in_diff": true}]}`. `in_diff: false` is how a part claims a file the pull
    ///   request did not change, and such a file has no lines added or removed.
    /// * `open_choices` — `{"left_open": "...", "chosen": "...", "why": "...",
    ///   "departs_from_ticket": false, "finding_seq": 3}`.
    /// * `disagreements` — `{"description_says": "...", "code_does": "...",
    ///   "finding_seq": 3}`. **The finding is required**: a disagreement nothing raises
    ///   posts a review that never mentions it.
    Put {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Print the problem half of the brief, and nothing from the change half.
    ///
    /// **This is what a blind pass may be shown.** It carries how the system works today,
    /// the before flow, and what was wrong with each numbered problem — and that is the
    /// whole of it. The headline, the after flow, every problem's `now_fixed`, the named
    /// detail, the supporting table, the options, the blind pass, the parts, the choices
    /// and the disagreements are all absent, because an independent view of a problem
    /// stops being independent the moment it is shown the answer.
    ///
    /// The superseded `diagram` is absent too. It held the before and the after at once
    /// and nothing can separate them, which is why the flows are rows now.
    ///
    /// **It decides nothing.** A read is a read: it does not confirm the brief, does not
    /// touch layer 2, and writes nothing at all.
    #[command(name = "problem-statement")]
    ProblemStatement {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
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

        BriefCommand::ProblemStatement { review } => {
            let store = Store::open_default()?;
            let statement = store.problem_statement(*review)?;
            let plain = problem_statement_lines(&statement);

            Ok(Report::new(
                json!({ "problem_statement": statement }),
                plain,
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

/// The problem half as a person reads it.
///
/// Built from the same five fields the JSON is, in the same order. A second spelling of
/// what the problem half contains is a second place it can grow a leak.
fn problem_statement_lines(statement: &ProblemStatement) -> String {
    let mut lines = vec![format!(
        "review {} — the problem, and nothing of the change",
        statement.review_id
    )];

    lines.push(format!("  problem: {}", statement.problem));
    if let Some(today) = &statement.how_it_works_today {
        lines.push(format!("  how it works today: {today}"));
    }

    if !statement.flow_before.is_empty() {
        let caption = statement
            .flow_before_caption
            .as_deref()
            .unwrap_or("how it works today");
        lines.push(format!(
            "  before: {caption} ({} rows)",
            statement.flow_before.len()
        ));
        for (index, row) in statement.flow_before.iter().enumerate() {
            for flow_box in row {
                let mark = flow_box
                    .problem
                    .map(|problem| format!("[{problem}] "))
                    .unwrap_or_default();
                let note = flow_box
                    .note
                    .as_deref()
                    .map(|note| format!(" — {note}"))
                    .unwrap_or_default();
                lines.push(format!("    {}. {mark}{}{note}", index + 1, flow_box.title));
            }
        }
    }

    if !statement.problems.is_empty() {
        lines.push(format!("  what is wrong: {}", statement.problems.len()));
        for problem in &statement.problems {
            lines.push(format!("    {} {}", problem.number, problem.was_wrong));
        }
    }

    lines.join("\n")
}
