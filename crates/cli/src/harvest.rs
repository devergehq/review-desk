//! The two subcommands DEV-859 adds: `harvest` and `finding link`.
//!
//! Everything harvesting does lives in `review-desk-harvest`. This module is the thin
//! layer that turns arguments into a [`Selection`], prints the result, and chooses an
//! exit code.
//!
//! **The `--json` envelope and the exit mapping here are local on purpose.** DEV-851 is
//! building the same two things across nine other subcommands in this crate at the same
//! time. Rather than guess at its shape and conflict with it, this module implements the
//! behaviour DEV-851's clarification of 8 October 2026 documents — a single JSON
//! document on standard output in `--json` mode, including on a refusal; text on
//! standard error otherwise; 1 for refused, 2 for could not run — and keeps it in one
//! place, so that whichever branch merges second can delete [`fail`] and the `--json`
//! flags in favour of the shared ones.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use review_desk_harvest::{gh::Gh, harvest, HarvestError, Report, Selection};
use review_desk_store::{Finding, Store};
use serde_json::json;

/// Exit code for a request that was understood and refused.
const EXIT_REFUSED: u8 = 1;

/// Exit code for Review Desk being unable to run.
const EXIT_BROKEN: u8 = 2;

/// `review-desk harvest`.
#[derive(Debug, Args)]
pub struct HarvestArgs {
    /// Only this repository, as OWNER/REPO.
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,

    /// Only this review. It is read whatever state its pull request is in.
    #[arg(long, value_name = "ID")]
    pub review: Option<i64>,

    /// Print one JSON document to standard output and nothing else.
    #[arg(long)]
    pub json: bool,
}

/// `review-desk finding …`.
#[derive(Debug, Subcommand)]
pub enum FindingCommand {
    /// Record the GitHub review comment a finding was posted as.
    Link {
        /// The review the finding belongs to.
        #[arg(long, value_name = "ID")]
        review: i64,

        /// The finding's sequence number within the review.
        #[arg(long, value_name = "FID")]
        finding: i64,

        /// The id of the GitHub review comment.
        #[arg(long, value_name = "ID")]
        github_comment: String,

        /// Print one JSON document to standard output and nothing else.
        #[arg(long)]
        json: bool,
    },
}

/// Read the outcomes of posted reviews back from GitHub.
pub fn run_harvest(args: HarvestArgs) -> ExitCode {
    match run(&args) {
        Ok(report) => {
            if args.json {
                println!(
                    "{}",
                    json!({ "ok": true, "summary": report.summary(), "harvest": report })
                );
            } else {
                println!("{}", report.summary());
                for line in report.lines() {
                    println!("{line}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => fail(args.json, &error),
    }
}

fn run(args: &HarvestArgs) -> Result<Report, HarvestError> {
    let store = Store::open_default()?;
    harvest(
        &store,
        &Gh::new(),
        Selection {
            repo: args.repo.clone(),
            review: args.review,
        },
    )
}

/// Work with one finding.
pub fn run_finding(command: FindingCommand) -> ExitCode {
    let FindingCommand::Link {
        review,
        finding,
        github_comment,
        json,
    } = command;

    match link(review, finding, &github_comment) {
        Ok(finding) => {
            if json {
                println!(
                    "{}",
                    json!({
                        "ok": true,
                        "review": finding.review_id,
                        "finding": finding.seq,
                        "github_comment_id": finding.github_comment_id,
                    })
                );
            } else {
                println!(
                    "linked finding {}/{} to GitHub comment {}",
                    finding.review_id,
                    finding.seq,
                    finding.github_comment_id.as_deref().unwrap_or_default()
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => fail(json, &error),
    }
}

fn link(review: i64, finding: i64, comment: &str) -> Result<Finding, HarvestError> {
    let store = Store::open_default()?;
    Ok(store.set_finding_github_comment(review, finding, comment)?)
}

/// Report a failure the way DEV-851's conventions require, and choose its exit code.
///
/// A refusal and a breakage have to be distinguishable without reading the message, so
/// the code is the answer and the text is only for a person.
fn fail(as_json: bool, error: &HarvestError) -> ExitCode {
    if as_json {
        println!(
            "{}",
            json!({ "ok": false, "code": error.code(), "message": error.to_string() })
        );
    } else {
        eprintln!("review-desk: {error}");
    }

    if error.is_refusal() {
        ExitCode::from(EXIT_REFUSED)
    } else {
        ExitCode::from(EXIT_BROKEN)
    }
}
