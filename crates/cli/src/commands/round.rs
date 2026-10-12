//! `round commits` and `round list`: what changed between two rounds, and the rounds so far.
//!
//! **Neither command decides anything.** `round commits` reads GitHub and writes what it
//! read; `round list` reads the record back. What became of a finding is said by
//! `finding carry`, and signing a review off happens in the dashboard and nowhere else.

use clap::Subcommand;
use review_desk_harvest::{commits_between, gh::Gh};
use review_desk_store::{RoundCommit, RoundSummary, Store};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum RoundCommand {
    /// Record the commits between the previous round's head commit and this round's.
    ///
    /// Read from GitHub by running `gh`, under whatever login the developer already has.
    /// **Nothing is written to GitHub.** Running it again replaces what was recorded, so a
    /// round re-read after a force push holds the commits the pull request has now rather
    /// than the union of both reads.
    ///
    /// A first round has no previous round to compare against, so it needs `--base`.
    Commits {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// Compare from this commit instead of the previous round's head.
        #[arg(long, value_name = "SHA")]
        base: Option<String>,
    },

    /// Print every round of a pull request: what each posted, and what came back.
    List {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
    },
}

pub fn run(command: &RoundCommand) -> Result<Report> {
    match command {
        RoundCommand::Commits { review, base } => {
            let store = Store::open_default()?;
            let this = store.get_review(*review)?;

            // Resolved before `gh` is run, so a round with nothing to compare against is a
            // refusal that made no GitHub call rather than one that made four.
            let base = match base {
                Some(base) => base.clone(),
                None => match store.previous_round(this.id)? {
                    Some(previous) => previous.head_sha,
                    None => {
                        return Err(CliError::bad_input(
                            format!(
                                "review {} is round {} and has no previous round",
                                this.id, this.round
                            ),
                            "give --base to say which commit to compare from",
                        ))
                    }
                },
            };

            let read = commits_between(&Gh::new(), &this.repo, &base, &this.head_sha)?;
            let recorded = store.put_round_commits(this.id, &read)?;

            let plain = if recorded.is_empty() {
                format!(
                    "nothing changed between {base} and {head} on {repo}#{pr}",
                    head = this.head_sha,
                    repo = this.repo,
                    pr = this.pr
                )
            } else {
                let mut lines = vec![format!(
                    "{} since {base}",
                    count(recorded.len() as i64, "commit", "commits")
                )];
                lines.extend(recorded.iter().map(commit_line));
                lines.join("\n")
            };

            Ok(Report::new(
                json!({ "review": this.id, "base": base, "commits": recorded }),
                plain,
            ))
        }

        RoundCommand::List { repo, pr } => {
            let store = Store::open_default()?;
            let rounds = store.rounds(repo, *pr)?;

            // The repository the store searched, as `review find` and `rules list` carry
            // it. This was the one read the sweep for that missed, which a review of the
            // sweep found: the rows were right and the heading named a repository the
            // rows did not.
            let found = review_desk_store::normalised_repo(repo);

            let plain = if rounds.is_empty() {
                format!("no reviews of {found}#{pr}")
            } else {
                rounds.iter().map(round_line).collect::<Vec<_>>().join("\n")
            };

            Ok(Report::new(
                json!({ "repo": found, "pr": pr, "rounds": rounds }),
                plain,
            ))
        }
    }
}

/// `a41c9e2 Sends an idempotency key with each delivery. — jobs/DeliverWebhook.php`
fn commit_line(commit: &RoundCommit) -> String {
    let mut line = format!("  {} {}", commit.sha, commit.summary);
    if !commit.files.is_empty() {
        line.push_str(&format!(" — {}", commit.files.join(", ")));
    }
    line
}

/// `round 1 review 3 head 3f9c2ab posted as request_changes · 5 findings · 3 replies, 2 resolved`
///
/// The status counts come last and only when something has been decided: a first round
/// where every finding is new would otherwise read as five separate figures saying one
/// thing.
fn round_line(round: &RoundSummary) -> String {
    let mut line = format!(
        "round {} review {} head {}",
        round.round, round.review_id, round.head_sha
    );

    match (round.posted_at, round.event) {
        (Some(at), Some(event)) => line.push_str(&format!(" posted {at} as {event}")),
        (Some(at), None) => line.push_str(&format!(" posted {at}")),
        (None, Some(event)) => line.push_str(&format!(" to post as {event}")),
        (None, None) => line.push_str(" not posted"),
    }

    line.push_str(&format!(
        " · {} · {} · {} replies, {} resolved",
        count(round.findings, "finding", "findings"),
        count(round.commits, "commit", "commits"),
        round.replies,
        round.threads_resolved
    ));

    let counts = &round.statuses;
    let settled =
        counts.fixed + counts.still_open + counts.accepted + counts.withdrawn + counts.ticketed;
    if settled > 0 {
        line.push_str(&format!(
            " · {} fixed, {} still open, {} new, {} accepted, {} withdrawn, {} ticketed",
            counts.fixed,
            counts.still_open,
            counts.new,
            counts.accepted,
            counts.withdrawn,
            counts.ticketed
        ));
    }

    line
}

/// `1 commit`, `3 commits`. One place, so no line here counts by hand.
fn count(many: i64, one: &str, more: &str) -> String {
    format!("{many} {}", if many == 1 { one } else { more })
}
