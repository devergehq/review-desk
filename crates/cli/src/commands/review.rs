//! `review open`, `review link`, `review find` and `review show`.

use clap::Subcommand;
use review_desk_store::{
    Draft, Finding, FullReview, Outcome, Review, ReviewFacts, ReviewLayer, ReviewLinks, Store,
};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Open the review of a head commit, or hand back the one already on the record.
    ///
    /// A head commit already reviewed returns that review. A new head commit on a pull
    /// request that has been reviewed before starts the next round. Prints the id.
    ///
    /// Any fact given is written to the review, and a fact left out is left as it was.
    /// Running this again on a head commit already reviewed hands back the same review
    /// without spending a round, so it is also how a finished session records what it
    /// did not check.
    ///
    /// **Opening a later round hands back the previous round's findings**, ready to be
    /// carried with `finding carry`. A second-round review that cannot see round 1's
    /// findings cannot say "two fixed, two still open, one new", which is the whole of a
    /// second-round review — so they come back with the review rather than needing a
    /// command of their own. A first round has none, and `carryable` is then null.
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
        /// The pull request's title.
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,
        /// The Linear ticket the change is for.
        #[arg(long = "ticket", value_name = "ID")]
        ticket: Option<String>,
        /// Additions plus deletions in the change.
        #[arg(long = "changed-lines", value_name = "N")]
        changed_lines: Option<i64>,
        /// What the review did not look at, in the reviewer's own words.
        #[arg(long = "not-checked", value_name = "TEXT")]
        not_checked: Option<String>,
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

    /// List the reviews of a pull request, newest round first.
    ///
    /// For an agent that has lost a review id: the rounds are printed with their head
    /// commits, so the one being worked on can be picked out.
    Find {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
    },

    /// Print the whole review: the layers, the brief, the findings, the outcomes and
    /// the draft.
    Show {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
    },
}

pub fn run(command: &ReviewCommand) -> Result<Report> {
    match command {
        ReviewCommand::Open {
            repo,
            pr,
            head,
            title,
            ticket,
            changed_lines,
            not_checked,
        } => {
            let store = Store::open_default()?;
            let opened = store.open_review_with(
                repo,
                *pr,
                head,
                &ReviewFacts {
                    title: title.clone(),
                    ticket_id: ticket.clone(),
                    changed_lines: *changed_lines,
                    not_checked: not_checked.clone(),
                },
            )?;

            let carryable = store.carryable(opened.review.id)?;

            Ok(Report::new(
                json!({
                    "review": opened.review,
                    "created": opened.created,
                    "carryable": carryable,
                }),
                // The id alone, so `ID=$(review-desk review open ...)` is the whole of
                // what a shell caller has to do. Whether it was created is in the
                // document for a caller that needs to know, and so are the findings
                // waiting to be carried: a line of prose about them here would break
                // every caller that reads this output as an id.
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

        ReviewCommand::Find { repo, pr } => {
            let store = Store::open_default()?;
            // The store lists rounds oldest first, which is right for reading a pull
            // request's history. An agent recovering an id wants the round it is on,
            // so the newest comes first here.
            let mut reviews = store.list_reviews(repo, *pr)?;
            reviews.reverse();

            let plain = if reviews.is_empty() {
                format!("no reviews of {repo}#{pr}")
            } else {
                reviews
                    .iter()
                    .map(found_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            Ok(Report::new(
                json!({ "repo": repo, "pr": pr, "reviews": reviews }),
                plain,
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

/// `3 round 2 head 3f9c2ab — Retry failed webhook deliveries`
fn found_line(review: &Review) -> String {
    let mut line = format!(
        "{} round {} head {}",
        review.id, review.round, review.head_sha
    );
    if let Some(title) = &review.title {
        line.push_str(&format!(" — {title}"));
    }
    line
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

    if let Some(title) = &review.title {
        lines.push(format!("  title: {title}"));
    }
    for (label, value) in [
        ("ticket", review.ticket_id.clone()),
        ("changed lines", review.changed_lines.map(|n| n.to_string())),
        ("not checked", review.not_checked.clone()),
    ] {
        if let Some(value) = value {
            lines.push(format!("  {label}: {value}"));
        }
    }

    lines.push(format!("  layers: {}", layers_line(&full.layers)));

    match &full.brief {
        Some(brief) => {
            // Three states, not two. A brief recorded as needing no confirmation is not
            // outstanding, and this is the surface a session reads: reporting it as "not
            // confirmed" would have a session wait for an act nobody is going to perform.
            lines.push(format!(
                "  brief ({}) {}",
                match (brief.confirmed_at, brief.confirmation_required) {
                    (Some(at), _) => format!("confirmed at {at}"),
                    (None, false) => match &brief.confirmation_not_required_because {
                        Some(because) => format!("confirmation not required: {because}"),
                        None => "confirmation not required".to_string(),
                    },
                    (None, true) => "not confirmed".to_string(),
                },
                brief.problem
            ));
            for (label, value) in [
                ("diagram", &brief.diagram),
                ("option map", &brief.option_map),
                ("approach", &brief.approach_verdict),
                ("provenance", &brief.provenance),
            ] {
                if let Some(value) = value {
                    lines.push(format!("    {label}: {value}"));
                }
            }
            for option in &brief.options {
                lines.push(format!(
                    "    option {}{}: {} (for: {} · against: {})",
                    option.key,
                    if option.chosen { " — built" } else { "" },
                    option.title,
                    option.argument_for,
                    option.argument_against,
                ));
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

    match &full.draft {
        Some(draft) => lines.push(format!("  draft: {}", draft_line(draft))),
        None => lines.push("  draft: none".to_string()),
    }

    lines.join("\n")
}

/// `1 lint done, 2 understand waiting_on_you, …`, every layer, in order.
///
/// All seven, always: the store completes the set, so a layer nothing has written reads
/// as `pending` rather than being missing from the line.
fn layers_line(layers: &[ReviewLayer]) -> String {
    layers
        .iter()
        .map(|layer| format!("{} {} {}", layer.number, layer.name, layer.state))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `ready, 1240 characters` or `approved as request_changes, 1240 characters`.
fn draft_line(draft: &Draft) -> String {
    let mut line = draft.status.to_string();
    if let Some(event) = &draft.event {
        line.push_str(&format!(" as {event}"));
    }
    line.push_str(&format!(", {} characters", draft.body.len()));
    line
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
    // The rule it was raised under, in the display key a person quotes. Driving a
    // whole review through the binary by hand is what showed this missing: the
    // citation is the most load-bearing field this change adds, and the only view a
    // person reads was dropping it.
    if let Some(rule_id) = finding.rule_id {
        line.push_str(&format!(" · same ruling as R-{rule_id:02}"));
    }
    // What became of it in this round, who said so, and where it came from. The same
    // argument as the rule citation above: this is the only view a person reads, and a
    // round-two review whose findings all read as `new` would hide the one fact DEV-873
    // added.
    line.push_str(&format!(" · {}", finding.round_status));
    if let Some(by) = finding.round_status_by {
        line.push_str(&format!(" by {by}"));
    }
    if let Some(continues) = finding.continues_review_id {
        line.push_str(&format!(" · continues review {continues}"));
    }
    // The two are independent fields and are checked independently, so each is printed on
    // its own. Nesting the number inside the commit hid a finding attributed to an earlier
    // finding and no commit from the only view a person reads.
    match (&finding.introduced_by_commit, finding.introduced_by_seq) {
        (Some(commit), Some(seq)) => {
            line.push_str(&format!(" · introduced by {commit}, which fixed F{seq}"))
        }
        (Some(commit), None) => line.push_str(&format!(" · introduced by {commit}")),
        (None, Some(seq)) => line.push_str(&format!(" · introduced by the fix for F{seq}")),
        (None, None) => {}
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
