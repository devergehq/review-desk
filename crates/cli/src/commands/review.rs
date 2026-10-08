//! `review open`, `review link` and `review show`.

use clap::Subcommand;
use review_desk_store::{Finding, FullReview, Outcome, Review, ReviewLinks, Store};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Open the review of a head commit, or hand back the one already on the record.
    ///
    /// A head commit already reviewed returns that review. A new head commit on a pull
    /// request that has been reviewed before starts the next round. Prints the id.
    Open {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
        /// The head commit being reviewed.
        #[arg(long, value_name = "SHA")]
        head: String,
    },

    /// Store the other systems' ids against a review.
    ///
    /// An id left out is left as it was, so one call can set the session without
    /// clearing the posted GitHub review.
    Link {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The Allele session that ran the review.
        #[arg(long, value_name = "ID")]
        session: Option<String>,
        /// The Locus dispatcher ledger entry.
        #[arg(long, value_name = "ID")]
        ledger: Option<String>,
        /// The review posted on GitHub.
        #[arg(long = "github-review", value_name = "ID")]
        github_review: Option<String>,
    },

    /// Print the whole review: the brief, the findings and the outcomes.
    Show {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
    },
}

pub fn run(command: &ReviewCommand) -> Result<Report> {
    match command {
        ReviewCommand::Open { repo, pr, head } => {
            let store = Store::open_default()?;
            let opened = store.open_review(repo, *pr, head)?;

            Ok(Report::new(
                json!({ "review": opened.review, "created": opened.created }),
                // The id alone, so `ID=$(review-desk review open ...)` is the whole of
                // what a shell caller has to do. Whether it was created is in the
                // document for a caller that needs to know.
                opened.review.id.to_string(),
            ))
        }

        ReviewCommand::Link {
            review,
            session,
            ledger,
            github_review,
        } => {
            if session.is_none() && ledger.is_none() && github_review.is_none() {
                // Every id left out means "leave this one alone", so a call with none of
                // them would silently do nothing. That is a scripting mistake worth
                // naming rather than reporting success for.
                return Err(CliError::bad_input(
                    "review link changes nothing",
                    "give at least one of --session, --ledger or --github-review",
                ));
            }

            let store = Store::open_default()?;
            let linked = store.set_review_links(
                *review,
                ReviewLinks {
                    session_id: session.clone(),
                    ledger_id: ledger.clone(),
                    github_review_id: github_review.clone(),
                },
            )?;

            Ok(Report::new(
                json!({ "review": linked }),
                links_line(&linked),
            ))
        }

        ReviewCommand::Show { review } => {
            let store = Store::open_default()?;
            let full = store.full_review(*review)?;
            let plain = show_lines(&full);

            Ok(Report::new(serde_json::to_value(&full)?, plain))
        }
    }
}

/// `review 3: session=s-1 ledger=- github-review=gh-44`, with `-` for an id not held.
fn links_line(review: &Review) -> String {
    format!(
        "review {}: session={} ledger={} github-review={}",
        review.id,
        or_dash(&review.session_id),
        or_dash(&review.ledger_id),
        or_dash(&review.github_review_id),
    )
}

/// A whole review as a person reads it.
///
/// Times are the integers the store holds — seconds since the epoch. The store's
/// position is that formatting belongs to whatever displays a record, and a date
/// library earns its place in the dashboard rather than here.
fn show_lines(full: &FullReview) -> String {
    let review = &full.review;
    let mut lines = vec![
        format!(
            "review {} {}#{} round {} head {}",
            review.id, review.repo, review.pr, review.round, review.head_sha
        ),
        format!("  {}", links_line(review)),
    ];

    match &full.brief {
        Some(brief) => {
            lines.push(format!(
                "  brief ({}) {}",
                match brief.confirmed_at {
                    Some(at) => format!("confirmed at {at}"),
                    None => "not confirmed".to_string(),
                },
                brief.problem
            ));
            for (label, value) in [
                ("diagram", &brief.diagram),
                ("option map", &brief.option_map),
                ("approach", &brief.approach_verdict),
            ] {
                if let Some(value) = value {
                    lines.push(format!("    {label}: {value}"));
                }
            }
        }
        None => lines.push("  brief: none".to_string()),
    }

    lines.push(format!("  findings: {}", full.findings.len()));
    for finding in &full.findings {
        lines.push(format!("    {}", finding_line(finding)));
    }

    lines.push(format!("  outcomes: {}", full.outcomes.len()));
    for outcome in &full.outcomes {
        lines.push(format!("    {}", outcome_line(outcome)));
    }

    lines.join("\n")
}

/// `2 should clarity src/x.rs:12 held_back — the name says nothing`
fn finding_line(finding: &Finding) -> String {
    let mut line = format!(
        "{} {} {} {} {}",
        finding.seq,
        finding.severity,
        finding.lens,
        location(finding),
        finding.disposition,
    );
    line.push_str(&format!(" — {}", finding.claim));
    if let Some(reason) = &finding.disposition_reason {
        line.push_str(&format!(" ({reason})"));
    }
    line
}

/// `src/x.rs:12`, `src/x.rs`, `line 12` or `-`.
///
/// A finding can carry a line without a file — the store makes the two independently
/// optional — and a line is worth saying even when nothing says where. Printing `-` for
/// that case threw it away in the only view a person reads.
fn location(finding: &Finding) -> String {
    match (&finding.file, finding.line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.clone(),
        (None, Some(line)) => format!("line {line}"),
        (None, None) => "-".to_string(),
    }
}

/// `outcome 1 on finding 2: cut by developer — it is a nit`
fn outcome_line(outcome: &Outcome) -> String {
    let mut line = format!(
        "outcome {} on finding {}: {} by {}",
        outcome.id, outcome.finding_seq, outcome.kind, outcome.actor
    );
    if let Some(detail) = &outcome.detail {
        line.push_str(&format!(" — {detail}"));
    }
    line
}

fn or_dash(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}
