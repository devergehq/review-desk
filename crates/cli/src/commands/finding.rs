//! `finding add`, `finding carry`, `finding became`, `finding set` and `finding link`.
//!
//! **None of these signs anything off.** Only the developer does that, in the dashboard.
//! `carry` and `became` record what became of a finding and who said so, which is a
//! statement about the code rather than an approval of the review.

use std::path::PathBuf;

use clap::Subcommand;
use review_desk_store::{
    DecidedBy, Disposition, Finding, FindingCarry, HunkLineMarker, NewFinding, RoundStatus,
    Severity, Store, Verification,
};
use serde_json::json;

use crate::error::Result;
use crate::input::read_json;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum FindingCommand {
    /// Add one finding to a review, and print its number.
    ///
    /// The document is `{"lens": "...", "severity": "...", "claim": "...", "title":
    /// "...", "file": "...", "line": 12, "line_sha": "a1b2c3d", "evidence": "...",
    /// "disposition": "...", "disposition_reason": "...", "rule_id": 9,
    /// "introduced_by_commit": "a41c9e2", "introduced_by_seq": 1}`. `lens`, `severity`
    /// and `claim` are required, the disposition defaults to `in_draft`, and a field that
    /// is not one of the names here is refused rather than quietly dropped.
    ///
    /// **`line_sha` is the commit `line` was measured on — say it.** It is what the
    /// finding's permalink is built from, and without it the link falls back to the
    /// review's head, which is where the review *is* and not where the line was counted.
    /// The two come apart: a carried finding keeps the earlier round's numbering, and a
    /// new commit on an unposted round moves its head under findings nobody has re-read.
    /// Nothing defaults it, because a session reads the diff for some minutes while the
    /// author pushes, and a commit derived from the head afterwards would be a confident
    /// wrong answer where the record can simply have none. It covers `flagged_line` and
    /// the hunk as well; all three are measured together.
    ///
    /// `title` is the one line the code review page's row carries — "A retried delivery
    /// carries no idempotency key". It is optional and has no maximum length; a
    /// finding with none shows its claim clamped to two lines, which is the honest
    /// rendering of a finding nobody titled. The empty string is refused, because it would
    /// draw a blank row where saying nothing draws the claim.
    ///
    /// **The number is the next one unused on the pull request, not on this review.** A
    /// finding's number is its identity across rounds, so round 2's first new finding on a
    /// pull request that raised five in round 1 is F6.
    ///
    /// `introduced_by_commit` and `introduced_by_seq` are a new finding's attribution —
    /// "introduced by a41c9e2, the commit that fixed F1". The number is checked against
    /// the pull request's findings, so a typo is a refusal rather than an attribution
    /// pointing at nothing.
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

    /// Carry a finding from an earlier round into this one, with what became of it.
    ///
    /// **The carried finding keeps the number it had** — F2 in round 1 is F2 in round 3 —
    /// which is why no number is assigned here and `--finding` names the one it already
    /// has. Its claim, evidence, severity and lens are copied from the earlier round, so
    /// this round can be read without reading that one.
    ///
    /// Carrying the same finding again rewrites the status, which is how a status changes
    /// later: an agent leaves it `still_open` and the developer accepts the author's
    /// reason.
    ///
    /// **This is not a sign-off.** Saying a finding is fixed is not approving a review;
    /// `--by` records who decided, and `agent` is a value so that the record never says
    /// the developer checked something nobody showed them.
    Carry {
        /// The review of the round being carried into.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The review of the earlier round being carried from.
        #[arg(long = "from", value_name = "ID")]
        from: i64,
        /// The finding's number, which it keeps.
        #[arg(long, value_name = "FID")]
        finding: i64,
        /// One of: new, fixed, still_open, accepted, withdrawn, ticketed.
        #[arg(long, value_name = "VALUE")]
        status: String,
        /// Who decided: developer, agent or author.
        #[arg(long, value_name = "WHO")]
        by: String,
        /// How it was checked, in your own words.
        #[arg(long, value_name = "TEXT")]
        checked: Option<String>,
        /// The commit the status rests on — for a fixed finding, the one that fixed it.
        #[arg(long, value_name = "SHA")]
        commit: Option<String>,
    },

    /// Change what became of a finding in the round it is already on.
    ///
    /// For a finding raised in this round, which has no earlier round to be carried from.
    /// Carrying one again does the same thing for a finding that has.
    Became {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The finding's number.
        #[arg(long, value_name = "FID")]
        finding: i64,
        /// One of: new, fixed, still_open, accepted, withdrawn, ticketed.
        #[arg(long, value_name = "VALUE")]
        status: String,
        /// Who decided: developer, agent or author.
        #[arg(long, value_name = "WHO")]
        by: String,
        /// How it was checked, in your own words.
        #[arg(long, value_name = "TEXT")]
        checked: Option<String>,
        /// The commit the status rests on.
        #[arg(long, value_name = "SHA")]
        commit: Option<String>,
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

        FindingCommand::Carry {
            review,
            from,
            finding,
            status,
            by,
            checked,
            commit,
        } => {
            let carry = read_carry(status, by, checked, commit)?;
            let store = Store::open_default()?;
            let carried = store.carry_finding(*from, *review, *finding, &carry)?;
            let rounds_open = store.rounds_open(*review, *finding)?;

            Ok(Report::new(
                json!({ "finding": carried, "rounds_open": rounds_open }),
                carried_line(&carried, *from, rounds_open),
            ))
        }

        FindingCommand::Became {
            review,
            finding,
            status,
            by,
            checked,
            commit,
        } => {
            let carry = read_carry(status, by, checked, commit)?;
            let store = Store::open_default()?;
            let changed = store.set_finding_round_status(*review, *finding, &carry)?;
            let rounds_open = store.rounds_open(*review, *finding)?;

            Ok(Report::new(
                json!({ "finding": changed, "rounds_open": rounds_open }),
                became_line(&changed, rounds_open),
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

/// The two closed sets and the two notes a carry records.
///
/// Parsed here rather than by clap, for the reason `finding set` gives: an unknown value
/// should be refused as an invalid value — with the valid ones named — rather than as a
/// usage error that names the flag and not the vocabulary.
fn read_carry(
    status: &str,
    by: &str,
    checked: &Option<String>,
    commit: &Option<String>,
) -> Result<FindingCarry> {
    Ok(FindingCarry {
        status: status.parse::<RoundStatus>()?,
        by: by.parse::<DecidedBy>()?,
        checked: checked.clone(),
        commit: commit.clone(),
    })
}

/// `finding 2 of review 7 continues review 3: still_open, decided by agent, open for 2 rounds`
fn carried_line(finding: &Finding, from: i64, rounds_open: i64) -> String {
    let mut line = format!(
        "finding {} of review {} continues review {from}: {}",
        finding.seq, finding.review_id, finding.round_status
    );
    line.push_str(&decided(finding, rounds_open));
    line
}

/// `finding 6 of review 7 is new, decided by agent`
fn became_line(finding: &Finding, rounds_open: i64) -> String {
    let mut line = format!(
        "finding {} of review {} is {}",
        finding.seq, finding.review_id, finding.round_status
    );
    line.push_str(&decided(finding, rounds_open));
    line
}

/// Who decided, how they checked, the commit it rests on, and the rounds it has been open.
///
/// "open for 3 rounds" is printed only while it is open, which is the only state the
/// sentence is true in.
fn decided(finding: &Finding, rounds_open: i64) -> String {
    let mut parts = Vec::new();
    if let Some(by) = finding.round_status_by {
        parts.push(format!("decided by {by}"));
    }
    if let Some(checked) = &finding.round_status_checked {
        parts.push(format!("checked: {checked}"));
    }
    if let Some(commit) = &finding.round_status_commit {
        parts.push(format!("commit {commit}"));
    }
    if rounds_open > 1 {
        parts.push(format!("open for {rounds_open} rounds"));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(", {}", parts.join(", "))
    }
}
