//! `finding add`, `finding set` and `finding link`.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{Disposition, HunkLineMarker, NewFinding, Severity, Store, Verification};
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
    /// "disposition_reason": "...", "rule_id": 9}`. `lens`, `severity` and `claim` are
    /// required, the disposition defaults to `in_draft`, and a field that is not one of
    /// the names here is refused rather than quietly dropped.
    ///
    /// A finding may also carry what it is about, which is what the dashboard shows:
    ///
    /// ```json
    /// {
    ///   "hunk": {
    ///     "file": "jobs/DeliverWebhook.php",
    ///     "start_line": 56,
    ///     "lines": [
    ///       {"marker": "unchanged", "text": "{"},
    ///       {"marker": "removed", "text": "    $client->post($url, $payload);"},
    ///       {"marker": "added", "text": "    $r = $client->post($url, $payload);"}
    ///     ]
    ///   },
    ///   "flagged_line": 57,
    ///   "inline_comment": "This retry re-sends the payload with no idempotency key.",
    ///   "why_it_matters": "A slow endpoint is charged twice for one event.",
    ///   "proof": "deliveries for evt_1: 2, expected 1",
    ///   "proof_note": "A throwaway test, not committed.",
    ///   "verification": "reproduced",
    ///   "suggested_fix": "Send the delivery id as the key.",
    ///   "fix_references": ["SendInvoiceEmail.php:44"]
    /// }
    /// ```
    ///
    /// All of it is optional and none of it depends on the rest; a finding with none of
    /// it reads as it always did. **Review Desk does not fetch code**, so the hunk is
    /// stored exactly as it is given here — no trimming, no re-wrapping, no normalising —
    /// and `start_line` is the number the first added or unchanged line carries, a removed
    /// line carrying none. A line's `text` is one file line and may not contain an
    /// end-of-line character, or the numbering beside it is wrong. `flagged_line` has to
    /// be a line the hunk draws, when a hunk is given. `proof_note` is the plain caption
    /// drawn beneath the proof, which is drawn as a monospace block. `verification` is one of `reproduced`, `read_not_run` or `unknown`, and it
    /// defaults to `unknown`: saying nothing about the method is not the same as saying
    /// the code was read and not run.
    Add {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The JSON document, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },

    /// Record the GitHub review comment a finding was posted as.
    ///
    /// The harvester needs it to find the finding's thread; a finding without one is
    /// skipped for replies and resolution. Linking the same comment to the same finding
    /// again changes nothing, so a posting step can be re-run.
    Link {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The finding's sequence number within the review.
        #[arg(long, value_name = "FID")]
        finding: i64,
        /// The id of the GitHub review comment.
        #[arg(long, value_name = "ID")]
        github_comment: String,
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
                    "severity is one of: {}; disposition is one of: {}; \
                     verification is one of: {}; a hunk line's marker is one of: {}",
                    Severity::valid_values(),
                    Disposition::valid_values(),
                    Verification::valid_values(),
                    HunkLineMarker::valid_values()
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

        FindingCommand::Link {
            review,
            finding,
            github_comment,
        } => {
            let store = Store::open_default()?;
            let linked = store.set_finding_github_comment(*review, *finding, github_comment)?;

            Ok(Report::new(
                json!({ "finding": linked }),
                format!(
                    "finding {} of review {} was posted as GitHub comment {}",
                    linked.seq,
                    linked.review_id,
                    linked.github_comment_id.as_deref().unwrap_or_default()
                ),
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
