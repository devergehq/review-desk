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
    /// **One exception: a `--ticket` that differs from the one on the review clears the
    /// address**, unless the same call gives a new `--ticket-url`. An address belongs to one
    /// ticket, and keeping it against a different id made the dashboard draw a link whose
    /// text and destination named two different things. So correct a ticket id and the
    /// address goes with it; pass both to move both.
    ///
    /// **`--ticket-url` is the ticket's address, beside the id `--ticket` records.**
    /// Review Desk cannot work one out from the other — the tracker and its workspace are
    /// the session's knowledge, not the record's — so the dashboard draws the id as a link
    /// when this is given and as text when it is not. It must start `https://`, and a
    /// value that does not is refused rather than stored: the one thing a stored address
    /// is for is being drawn as a link. Nothing is ever fetched from it.
    ///
    /// **An address with no ticket is refused too**, because the dashboard draws the id as
    /// the link's text — so an address beside no id reaches no screen. The id can come from
    /// this call or already be on the review, so learning the address afterwards is one
    /// more `review open` on the same head commit. A later round is its own review and
    /// inherits nothing, so it needs its own `--ticket`.
    ///
    /// **Opening a later round hands back the previous round's findings**, ready to be
    /// carried with `finding carry`. A second-round review that cannot see round 1's
    /// findings cannot say "two fixed, two still open, one new", which is the whole of a
    /// second-round review — so they come back with the review rather than needing a
    /// command of their own. A first round has none, and `carryable` is then null.
    ///
    /// **This is also how a review request is recorded before any session starts.** The
    /// Locus dispatcher calls it the moment GitHub shows it a review request, with the
    /// repository, the pull request, the head commit, the title and the ticket if it knows
    /// one — and a review with no brief is what `work list` reports as
    /// `review_requested`, so the request is in the dispatcher's own queue the moment it
    /// is recorded. Recording the same request twice creates one review: the record is
    /// keyed on the head commit, and `created` says which of the two happened.
    ///
    /// **A new commit on a request nobody has picked up moves that request's head**
    /// rather than starting a second round beside it, and `head_moved_from` in the
    /// `--json` answer carries the commit it was of before. DEV-945, Patrick: "two
    /// unstarted reviews for one request is wrong." A round somebody is working on is
    /// untouched by that rule — a new commit on it is the next round, as it has always
    /// been — and `--session` is how a session says on its first call that it is the one
    /// working, before it has written anything a reader could tell that from.
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
        /// Where that ticket is. Must start `https://`, and needs a ticket on this
        /// review; nothing is fetched from it.
        #[arg(long = "ticket-url", value_name = "URL")]
        ticket_url: Option<String>,
        /// Additions plus deletions in the change.
        #[arg(long = "changed-lines", value_name = "N")]
        changed_lines: Option<i64>,
        /// What the review did not look at, in the reviewer's own words.
        #[arg(long = "not-checked", value_name = "TEXT")]
        not_checked: Option<String>,
        /// The Locus dispatcher's ledger entry for this request.
        ///
        /// Here as well as on `review link` so that recording a review request is one
        /// call rather than two: the dispatcher knows its own ledger id at the moment it
        /// sees the request, and a second command to attach it is a second thing that can
        /// fail between them.
        #[arg(long = "ledger", value_name = "ID")]
        ledger: Option<String>,
        /// The Allele session opening it. **A session should pass this on its first
        /// call.**
        ///
        /// The same shape as `--ledger` — a fact about who is calling, recorded in the
        /// same call rather than by a `review link` afterwards — and it does one thing
        /// more. A review with a session linked is one somebody is at work on, so no
        /// later `review open` moves its head: the window between a session opening a
        /// review and claiming it is the window in which a dispatcher recording the same
        /// request on a newer commit could move the head out from under it.
        #[arg(long = "session", value_name = "ID")]
        session: Option<String>,
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

    /// Record the one ticket this review's held-back findings were filed as.
    ///
    /// **Reporting a filing, never asking for one.** The developer asks, in the
    /// dashboard — "File as one ticket for an agent", or "Ticket what is left, and
    /// approve" — and `work list` then reports the review as `nits_to_ticket`. This is how
    /// the session that filed it says which ticket it became, which is what clears the
    /// item. `--ticket` is required for that reason: there is no form of this command that
    /// asks for a ticket nobody has filed.
    ///
    /// Review Desk files no ticket itself, for the same reason it posts no review: it owns
    /// the record and the session performs the outward act.
    NitTicket {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The ticket the held-back findings were filed as.
        #[arg(long = "ticket", value_name = "ID")]
        ticket: String,
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
            ticket_url,
            changed_lines,
            not_checked,
            ledger,
            session,
        } => {
            let store = Store::open_default()?;
            let opened = store.open_review_with(
                repo,
                *pr,
                head,
                &ReviewFacts {
                    title: title.clone(),
                    ticket_id: ticket.clone(),
                    ticket_url: ticket_url.clone(),
                    changed_lines: *changed_lines,
                    not_checked: not_checked.clone(),
                },
            )?;

            // The ids go on after the facts, through the same call `review link` makes.
            // A value given is written and an absent one leaves what is there, so
            // recording the same request twice does not clear an id the first call set.
            let review = if ledger.is_some() || session.is_some() {
                store.set_review_links(
                    opened.review.id,
                    ReviewLinks {
                        ledger_id: ledger.clone(),
                        session_id: session.clone(),
                        ..Default::default()
                    },
                )?
            } else {
                opened.review
            };

            let carryable = store.carryable(review.id)?;

            Ok(Report::new(
                json!({
                    "review": review,
                    "created": opened.created,
                    // The commit this review was of before this call moved it, or null.
                    // A dispatcher recording a request that the author has pushed to
                    // reads this to learn that its request moved rather than that a
                    // round was spent.
                    "head_moved_from": opened.head_moved_from,
                    "carryable": carryable,
                }),
                // The id alone, so `ID=$(review-desk review open ...)` is the whole of
                // what a shell caller has to do. Whether it was created is in the
                // document for a caller that needs to know, and so are the findings
                // waiting to be carried: a line of prose about them here would break
                // every caller that reads this output as an id.
                review.id.to_string(),
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

            // **The stored spelling, not the one that was typed.** The record holds a
            // repository canonically and the lookup now normalises to match — so echoing
            // the argument back would show `Trilogy-Care/tc-portal` beside rows that are
            // all `trilogy-care/tc-portal`, which is the shape of the fault this answer
            // hid: when the lookup missed, the empty answer repeated the caller's own
            // words and the two spellings never appeared together. Taken from the rows
            // where there are any, and from the same helper the store keys on where
            // there are not.
            let found = match reviews.first() {
                Some(review) => review.repo.clone(),
                None => review_desk_store::normalised_repo(repo),
            };

            let plain = if reviews.is_empty() {
                format!("no reviews of {found}#{pr}")
            } else {
                reviews
                    .iter()
                    .map(found_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            Ok(Report::new(
                json!({ "repo": found, "pr": pr, "reviews": reviews }),
                plain,
            ))
        }

        ReviewCommand::NitTicket { review, ticket } => {
            let store = Store::open_default()?;
            let filed = store.request_nit_ticket(*review, Some(ticket))?;

            Ok(Report::new(
                json!({ "nit_ticket": filed }),
                format!(
                    "the held-back findings of review {} are {} as {} ({})",
                    filed.review_id,
                    filed.status,
                    ticket,
                    filed
                        .finding_seqs
                        .iter()
                        .map(|seq| format!("F{seq}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
        }

        ReviewCommand::Show { review } => {
            let store = Store::open_default()?;
            let full = store.full_review(*review)?;
            let provenance = store.review_provenance(*review)?;

            // Merged into the document rather than added to `FullReview`. The record of
            // where a review's understanding came from is its own module in the store, and
            // `review show` is the one surface that prints everything — so this is the
            // place the two meet, and `FullReview` stays the shape three other changes in
            // flight are also editing.
            let mut document = serde_json::to_value(&full)?;
            if let Some(fields) = document.as_object_mut() {
                fields.insert("provenance".to_string(), serde_json::to_value(&provenance)?);
                fields.insert(
                    "description_lint_passes".to_string(),
                    json!(provenance.description_lint_passes()),
                );
            }

            let findings_by_path = super::provenance::findings_by_path(&store, &provenance)?;
            let plain = format!(
                "{}\n{}",
                show_lines(&full),
                super::provenance::show_lines(&provenance, &findings_by_path)
            );

            Ok(Report::new(document, plain))
        }
    }
}

/// `3 round 2 head 3f9c2ab — Retry failed webhook deliveries`
fn found_line(review: &Review) -> String {
    let mut line = format!(
        "{} round {} head {}",
        review.id, review.round, review.head_sha
    );
    // **A dismissed round is marked rather than left out.** This list is a pull request's
    // own history, and a hole where round 2 was would make the round numbers read as a
    // gap in the record — `Store::list_reviews` carries that argument. So the word is on
    // the line instead, because an agent recovering an id has to know that the round it
    // finds is one the developer has taken out of play.
    if review.is_dismissed() {
        line.push_str(" (dismissed)");
    }
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
        // The address beside the id, on its own line. Printed because this is the only
        // view a person reads, and an address the record holds and nothing shows is one
        // nobody can tell from an address nobody passed.
        ("ticket url", review.ticket_url.clone()),
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
