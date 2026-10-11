//! `provenance sources`, `provenance criteria`, `provenance changes`,
//! `provenance lint` and `provenance show`.
//!
//! Where a review's understanding came from. **All four are writes an agent makes and the
//! developer does not**: nothing here confirms, approves or decides anything, and there is
//! no reading of a ticket — what gets recorded is what the session says it read.
//!
//! Each write **replaces the whole list**. That is one command per record rather than one
//! per row, it needs no ordering argument, and it means the change map and the criteria
//! change in one step — a half-written coverage record is not a state any reader has to
//! cope with.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{
    CriterionStatus, DescriptionLintStatus, FileCheck, NewAcceptanceCriterion,
    NewDescriptionLintResult, NewFileChange, NewReviewSource, ReviewProvenance, SourceKind, Store,
};
use serde::Deserialize;
use serde_json::json;

use crate::error::Result;
use crate::input::read_json;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum ProvenanceCommand {
    /// Record what the review read the problem from, replacing the whole list.
    ///
    /// The document is `{"sources": [{"kind": "...", "label": "...", "link": "...",
    /// "status": "...", "note": "..."}]}`. `kind` is one of `ticket`, `description` or
    /// `comment`; `label` is what to call it on screen; `link` is optional and opaque —
    /// Review Desk never fetches it.
    ///
    /// `status` and `note` are optional and are what the sources table draws beside the
    /// reference: the ticket's state **as the session read it** — free text, because every
    /// tracker names its own columns — and one line saying what the source tells us. A
    /// source with no `note` shows its reference alone.
    ///
    /// **This is not Review Desk reading the ticket.** It is the session saying which
    /// ticket, description and comments it read, in its own words.
    ///
    /// Sending `{"sources": []}` clears the list: the list is the statement, so taking an
    /// entry off the record is restating it.
    Sources {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Record what the ticket asks for, replacing the whole list.
    ///
    /// The document is `{"criteria": [{"text": "...", "status": "...", ...}]}`. `status`
    /// is one of `met`, `not_met` or `not_in_this_pr`, and each criterion carries at most
    /// one pointer: `finding_seq`, or `file` with an optional `line`, or `note`.
    ///
    /// **A criterion recorded as `not_met` has to name a finding on this review.** A
    /// review that says the change fails a criterion and raises nothing about it posts a
    /// review that never mentions the failure. So the finding is raised first and the
    /// criteria are written — or rewritten — at layer 5 or later, which is also when a
    /// review is entitled to an opinion about whether the diff meets the ticket.
    Criteria {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Record the per-file change map, replacing the whole table.
    ///
    /// The document is `{"files": [{"path": "...", "summary": "...", "lines_added": 0,
    /// "lines_removed": 0, "checked": "...", "how_checked": "..."}]}`. `checked` is one of
    /// `read_in_full`, `read_in_part`, `run` or `not_reviewed`; `how_checked` is the
    /// sentence the screen shows beside it.
    ///
    /// **An area that was not reviewed is a row like any other, and it must say why.**
    /// `path` may be a pattern for an area — `resources/admin/**` — and
    /// `checked: "not_reviewed"` with no `how_checked` is refused.
    ///
    /// The findings on a file are not given here. They are read from the review's own
    /// findings, by path, so the table and the findings cannot disagree.
    Changes {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Record the description lint's individual results, replacing the whole list.
    ///
    /// The document is `{"results": [{"label": "...", "status": "...", "detail": "..."}]}`.
    /// `status` is one of `pass`, `warn` or `fail`.
    ///
    /// This is layer 1 on the **pull request's description**, which is a different thing
    /// from the lint over the review body that `draft put` carries. The overall verdict is
    /// not given: it is "no result failed", derived on the way out, so a stored verdict
    /// can never sit above a list that contradicts it.
    Lint {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Print all four lists, and the findings on each file.
    Show {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
    },
}

/// `{"sources": [...]}`. Unknown fields are refused: a misspelled key would otherwise be
/// an empty list, which clears the record.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesDocument {
    sources: Vec<NewReviewSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CriteriaDocument {
    criteria: Vec<NewAcceptanceCriterion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangesDocument {
    files: Vec<NewFileChange>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintDocument {
    results: Vec<NewDescriptionLintResult>,
}

pub fn run(command: &ProvenanceCommand) -> Result<Report> {
    match command {
        ProvenanceCommand::Sources { review, file } => {
            // Read before the database is opened, so a document that cannot be parsed
            // does not create a database file as a side effect.
            let document: SourcesDocument =
                read_json(file, Some(&format!("kind: {}", SourceKind::valid_values())))?;
            let store = Store::open_default()?;
            let written = store.put_review_sources(*review, document.sources)?;

            Ok(Report::new(
                json!({ "sources": written }),
                format!(
                    "review {review}: {}",
                    counted(written.len(), "source", "sources")
                ),
            ))
        }

        ProvenanceCommand::Criteria { review, file } => {
            let document: CriteriaDocument = read_json(
                file,
                Some(&format!("status: {}", CriterionStatus::valid_values())),
            )?;
            let store = Store::open_default()?;
            let written = store.put_acceptance_criteria(*review, document.criteria)?;

            let unmet = written
                .iter()
                .filter(|criterion| criterion.status == CriterionStatus::NotMet)
                .count();
            Ok(Report::new(
                json!({ "criteria": written }),
                format!(
                    "review {review}: {}, {unmet} not met",
                    counted(written.len(), "criterion", "criteria")
                ),
            ))
        }

        ProvenanceCommand::Changes { review, file } => {
            let document: ChangesDocument = read_json(
                file,
                Some(&format!("checked: {}", FileCheck::valid_values())),
            )?;
            let store = Store::open_default()?;
            let written = store.put_file_changes(*review, document.files)?;

            let not_reviewed = written
                .iter()
                .filter(|file| file.checked == FileCheck::NotReviewed)
                .count();
            Ok(Report::new(
                json!({ "files": written }),
                format!(
                    "review {review}: {}, {not_reviewed} not reviewed",
                    counted(written.len(), "file", "files")
                ),
            ))
        }

        ProvenanceCommand::Lint { review, file } => {
            let document: LintDocument = read_json(
                file,
                Some(&format!(
                    "status: {}",
                    DescriptionLintStatus::valid_values()
                )),
            )?;
            let store = Store::open_default()?;
            let written = store.put_description_lint(*review, document.results)?;
            let provenance = store.review_provenance(*review)?;

            Ok(Report::new(
                json!({
                    "results": written,
                    // Derived from the results above, and served beside them so a caller
                    // never has to work out what "passes" means from the list.
                    "passes": provenance.description_lint_passes(),
                }),
                format!(
                    "review {review}: {}, description lint {}",
                    counted(written.len(), "result", "results"),
                    match provenance.description_lint_passes() {
                        Some(true) => "passes",
                        Some(false) => "fails",
                        None => "has not run",
                    }
                ),
            ))
        }

        ProvenanceCommand::Show { review } => {
            let store = Store::open_default()?;
            let provenance = store.review_provenance(*review)?;
            let findings = findings_by_path(&store, &provenance)?;

            let plain = show_lines(&provenance, &findings);
            Ok(Report::new(
                json!({
                    "provenance": provenance,
                    "findings_by_path": findings,
                    "description_lint_passes": provenance.description_lint_passes(),
                }),
                plain,
            ))
        }
    }
}

/// The findings on each file in the change map, by path.
///
/// **Read from the findings, never stored against the entry.** That is what makes
/// DEV-875's "a finding listed on a file exists and points at that file" true by
/// construction: there is no second copy of the join to go stale.
pub(super) fn findings_by_path(
    store: &Store,
    provenance: &ReviewProvenance,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut findings = serde_json::Map::new();
    for file in &provenance.files {
        findings.insert(
            file.path.clone(),
            json!(store.findings_on_path(provenance.review_id, &file.path)?),
        );
    }
    Ok(findings)
}

/// `1 source` or `3 sources`.
fn counted(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// All four lists as a person reads them. Shared with `review show`, which prints the
/// whole record and must not grow a second spelling of this.
pub(super) fn show_lines(
    provenance: &ReviewProvenance,
    findings: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut lines = vec![format!("review {} provenance", provenance.review_id)];

    if provenance.is_empty() {
        lines.push("  nothing recorded".to_string());
        return lines.join("\n");
    }

    lines.push(format!("  read from: {}", provenance.sources.len()));
    for source in &provenance.sources {
        let mut line = format!("    {} {} — {}", source.position, source.kind, source.label);
        if let Some(link) = &source.link {
            line.push_str(&format!(" ({link})"));
        }
        lines.push(line);
    }

    lines.push(format!(
        "  description lint: {}",
        match provenance.description_lint_passes() {
            Some(true) => "passes",
            Some(false) => "fails",
            None => "has not run",
        }
    ));
    for result in &provenance.description_lint {
        let mut line = format!("    {} {}", result.status, result.label);
        if let Some(detail) = &result.detail {
            line.push_str(&format!(": {detail}"));
        }
        lines.push(line);
    }

    lines.push(format!(
        "  what the ticket asks for: {}",
        provenance.criteria.len()
    ));
    for criterion in &provenance.criteria {
        lines.push(format!(
            "    {} {} — {}{}",
            criterion.position,
            criterion.status,
            criterion.text,
            pointer(criterion),
        ));
    }

    lines.push(format!("  what they built: {}", provenance.files.len()));
    for file in &provenance.files {
        let on_file = findings
            .get(&file.path)
            .and_then(|seqs| seqs.as_array())
            .map(|seqs| {
                if seqs.is_empty() {
                    "none".to_string()
                } else {
                    seqs.iter()
                        .map(|seq| format!("F{seq}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            })
            .unwrap_or_else(|| "none".to_string());

        let mut line = format!(
            "    {} +{} -{} {} — {} · findings: {on_file}",
            file.path, file.lines_added, file.lines_removed, file.checked, file.summary,
        );
        if let Some(how) = &file.how_checked {
            line.push_str(&format!(" · {how}"));
        }
        lines.push(line);
    }

    lines.join("\n")
}

/// ` → jobs/DeliverWebhook.php:61`, ` → F1`, ` → a note`, or nothing.
fn pointer(criterion: &review_desk_store::AcceptanceCriterion) -> String {
    if let Some(seq) = criterion.finding_seq {
        return format!(" → F{seq}");
    }
    if let Some(file) = &criterion.file {
        return match criterion.line {
            Some(line) => format!(" → {file}:{line}"),
            None => format!(" → {file}"),
        };
    }
    match &criterion.note {
        Some(note) => format!(" → {note}"),
        None => String::new(),
    }
}
