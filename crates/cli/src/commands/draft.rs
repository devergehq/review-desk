//! `draft put`, `draft show` and `draft posted`.
//!
//! **A draft body is composed, not written as one block.** Since DEV-877 the developer
//! decides on each finding in the dashboard — keep, cut, regrade, rewrite — and the body
//! is assembled from the blocks the session wrote. So `draft put --sections` is how a
//! session writes a draft it wants those decisions to work on; `--file` alone still works
//! and stores the body as one block, which leaves the decisions with nothing to act on and
//! a refusal that says so.
//!
//! **None of this lets the command line decide anything.** Writing the blocks is writing
//! the draft; the decisions are the developer's, through the dashboard's API, and
//! `crates/cli/tests/signoff.rs` scans this crate's source for their names.
//!
//! **There is no `draft approve`, and there will not be one.** Approving a draft is the
//! developer's sign-off and happens in the dashboard — decided on 9 October 2026. The
//! store enforces it rather than this module promising it: `put_draft` accepts only
//! `drafting` and `ready`, so nothing here could approve a draft even by mistake.
//!
//! What a review session does instead is write the draft, then read `draft show` until
//! the status is `approved`, and then post it and report the GitHub review id with
//! `draft posted`.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{
    Draft, DraftLint, DraftSectionKind, DraftStatus, NewDraft, NewDraftSection, Store,
};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::input::{read_json, read_text};
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum DraftCommand {
    /// Store the review's draft body and mark it ready for the developer.
    ///
    /// The file is the review body as GitHub-flavoured markdown, taken verbatim, or `-`
    /// for standard input. It is not JSON: a review body carries code fences and
    /// quotation marks, and escaping them to get them through a JSON string would be
    /// work for every caller and a chance to mangle what gets posted.
    ///
    /// Refused once the draft is approved or posted. A draft the developer sent back
    /// may be rewritten, which is how it gets back to ready.
    Put {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The markdown body, or `-` for standard input.
        ///
        /// Optional when `--sections` is given, because the body is then what the blocks
        /// compose to. Given as well, it is checked against the composition and the write
        /// is refused when the two differ by anything but trailing newlines.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
        /// The blocks the body is composed of, as a JSON array:
        /// `[{"kind":"prose","text":"The approach fits the ticket."},
        /// {"kind":"finding","finding_seq":1,"title":"A retry carries no key",
        /// "text":"The customer receives the event twice."}]`
        ///
        /// A `prose` block is markdown carried through as it is. A `finding` block is a
        /// heading the store composes — `### Blocker · F1 · <title>`, from the finding's
        /// own severity — followed by its text. The store numbers the blocks, refuses a
        /// second block about one finding, and refuses a block about a finding that is
        /// not posted: a fixed, accepted, withdrawn or ticketed finding is not raised as
        /// an open finding, and what the round found about it is prose.
        #[arg(long, value_name = "PATH")]
        sections: Option<PathBuf>,
        /// What the review-craft lint said about this body, as a JSON document:
        /// `{"passes": true, "visible_words": 118, "word_budget": 150,
        /// "findings_in_body": 3, "findings_expected": 3, "raised_before_on_pr": 0,
        /// "errors": "...", "checked_at": 1760000000}`. Every field is optional.
        #[arg(long, value_name = "PATH")]
        lint: Option<PathBuf>,
        /// Keep the draft at `drafting` instead of marking it ready, for a session
        /// saving work it has not finished. A drafting review does not appear in the
        /// developer's inbox.
        #[arg(long)]
        drafting: bool,
    },

    /// Print the draft, its status, and the event chosen once it is approved.
    ///
    /// **This is how a review session learns it may post.** The status is `approved`
    /// and the event says which GitHub review event to post it as. Nothing on the
    /// command line can move a draft to approved; the developer does that in the
    /// dashboard.
    Show {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
    },

    /// Mark an approved draft as posted, naming the GitHub review it became.
    ///
    /// Refused unless the draft is approved. This is the only transition that involves
    /// GitHub, and the session makes it: Review Desk never posts. The review's
    /// `github_review_id` is recorded with it.
    Posted {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The id of the review posted on GitHub.
        #[arg(long = "github-review", value_name = "ID")]
        github_review: String,
    },
}

pub fn run(command: &DraftCommand) -> Result<Report> {
    match command {
        DraftCommand::Put {
            review,
            file,
            sections,
            lint,
            drafting,
        } => {
            if file.is_none() && sections.is_none() {
                return Err(CliError::bad_input(
                    "draft put needs something to write",
                    "a draft is written as the blocks it is composed of, with --sections, \
                     or as one block of markdown, with --file; this had neither",
                ));
            }

            // Every document is read before the database is opened, so a body that cannot
            // be read does not create a database file as a side effect.
            let body = match file {
                Some(path) => read_text(path)?,
                None => String::new(),
            };
            let kinds = DraftSectionKind::valid_values();
            let sections: Option<Vec<NewDraftSection>> = match sections {
                Some(path) => Some(read_json(path, Some(&kinds))?),
                None => None,
            };
            let lint: Option<DraftLint> = match lint {
                // A lint holds no closed set, so there is nothing to name.
                Some(path) => Some(read_json(path, None)?),
                None => None,
            };

            let store = Store::open_default()?;
            let blocks = sections.is_some();
            let stored = store.put_draft(
                *review,
                NewDraft {
                    body,
                    status: if *drafting {
                        DraftStatus::Drafting
                    } else {
                        DraftStatus::Ready
                    },
                    lint,
                    sections,
                },
            )?;
            let held = store.draft_sections(*review)?;

            Ok(Report::new(
                json!({ "draft": stored, "sections": held }),
                format!(
                    "draft of review {} is {} ({} characters, {} {})",
                    stored.review_id,
                    stored.status,
                    stored.body.len(),
                    held.len(),
                    if blocks { "blocks" } else { "block of prose" }
                ),
            ))
        }

        DraftCommand::Show { review } => {
            let store = Store::open_default()?;
            let draft = store.get_draft(*review)?;
            let plain = show_lines(&draft);

            Ok(Report::new(json!({ "draft": draft }), plain))
        }

        DraftCommand::Posted {
            review,
            github_review,
        } => {
            let store = Store::open_default()?;
            let posted = store.post_draft(*review, github_review)?;

            Ok(Report::new(
                json!({ "draft": posted }),
                format!(
                    "draft of review {} is posted as GitHub review {}",
                    posted.review_id, github_review
                ),
            ))
        }
    }
}

/// The draft as a person, or a session polling for an approval, reads it.
///
/// The status line comes first and the body after a blank line, so that a caller can
/// read the first line and stop. Times are the integers the store holds, for the reason
/// `review show` gives: formatting belongs to whatever displays a record.
fn show_lines(draft: &Draft) -> String {
    let mut lines = vec![format!(
        "draft of review {}: {}",
        draft.review_id, draft.status
    )];

    match (&draft.event, draft.approved_at) {
        (Some(event), Some(at)) => lines.push(format!("  post as: {event} (approved at {at})")),
        // The database refuses an approved draft with no event, so this says what is
        // true rather than unwrapping something that cannot be absent.
        _ => lines.push("  post as: not approved yet".to_string()),
    }

    if let Some(note) = &draft.sent_back_note {
        lines.push(format!("  sent back: {note}"));
    }
    if let Some(at) = draft.posted_at {
        lines.push(format!("  posted at: {at}"));
    }
    if !draft.lint.is_empty() {
        lines.push(format!("  lint: {}", lint_line(&draft.lint)));
    }

    lines.push(String::new());
    lines.push(draft.body.clone());
    lines.join("\n")
}

/// `passes, 118 of 150 words, 3 of 3 findings, 0 raised before`, leaving out whatever
/// the lint did not report.
fn lint_line(lint: &DraftLint) -> String {
    let mut parts = Vec::new();
    if let Some(passes) = lint.passes {
        parts.push(if passes { "passes" } else { "fails" }.to_string());
    }
    if let (Some(words), Some(budget)) = (lint.visible_words, lint.word_budget) {
        parts.push(format!("{words} of {budget} words"));
    }
    if let (Some(in_body), Some(expected)) = (lint.findings_in_body, lint.findings_expected) {
        parts.push(format!("{in_body} of {expected} findings"));
    }
    if let Some(before) = lint.raised_before_on_pr {
        parts.push(format!("{before} raised before on this pull request"));
    }
    if let Some(errors) = &lint.errors {
        parts.push(format!("errors: {errors}"));
    }
    parts.join(", ")
}
