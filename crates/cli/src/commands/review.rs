//! `review open`, `review link`, `review find` and `review show`.

use clap::Subcommand;
use review_desk_store::{
    DiffAtHead, Draft, Finding, FullReview, OutOfPlay, Outcome, Review, ReviewFacts, ReviewLayer,
    ReviewLinks, ReviewRequest, Store,
};
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Open the review of a head commit, or hand back the one already on the record.
    ///
    /// A head commit already reviewed returns that review. A new head commit moves the
    /// newest round's head, or starts the next round when that round has posted — the rule
    /// is below, under `action`. Prints the id.
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
    /// **A round is a posted review, so a new commit opens a round only when the newest
    /// round has posted.** Otherwise it moves that round's head, whoever is calling and
    /// whatever they have written: DEV-962, Patrick — "it shouldn't create a whole new
    /// record because that doesn't make any sense… If changes happen after our post, that's
    /// a new review. That's another round." The move is recorded, so the review page can
    /// say "Updated for 3 commits since `4d9464f`" and the code review can show what
    /// changed since the draft was written. A sign-off given before the move is withdrawn,
    /// because the developer approved different code.
    ///
    /// **`action` in the `--json` answer says which of four things happened** —
    /// `created`, `head_moved`, `unchanged` or `diff_unchanged` — and it is the field to
    /// branch on. `created` and `head_moved_from` still mean exactly what they meant, for
    /// callers written before it existed. `started` is beside it, so a caller deciding
    /// whether to wake the session already at work or dispatch one does not need a second
    /// call to find out.
    ///
    /// **`--session` is how a session says on its first call that it is the one working.**
    /// It no longer pins the head — that is the behaviour DEV-962 removes — and it is what
    /// makes `started` true for a dispatcher's look-before-write guard.
    ///
    /// **`--diff-unchanged` is for commits that only brought the base branch in.** Nothing
    /// moves: the record notes that they arrived and nobody is woken. See the flag.
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
        /// The same shape as `--ledger`: a fact about who is calling, recorded in the same
        /// call rather than by a `review link` afterwards. A review with a session linked
        /// is one somebody is at work on, which is what `review find`'s `started` reports
        /// to a caller that cannot see inside the record.
        ///
        /// **It does not pin the head.** It did until DEV-962, which moves an unposted
        /// round's head whoever is working on it and records the move instead — so what
        /// this buys is the record knowing whose review it is from the first call, rather
        /// than from whenever the session gets to `review link`.
        #[arg(long = "session", value_name = "ID")]
        session: Option<String>,
        /// The pull request's own diff against its base is unchanged: these commits only
        /// brought the base branch in.
        ///
        /// **Then nothing happens, which is the point.** No round is opened, no head
        /// moves, the line count is left alone, no sign-off is withdrawn and no session is
        /// woken; the record notes that the commits arrived, so the review page can say
        /// they changed nothing. DEV-962: Patrick's queue showed a pull request twice over
        /// a round 1 he had never posted, and the only new commits were the author merging
        /// the base branch in.
        ///
        /// **The caller says so because Review Desk cannot.** It reads no git and no
        /// GitHub, so the only thing that can compare the pull request's diff against its
        /// base is whatever is already watching GitHub. Compare the two diffs — `git
        /// patch-id` over the pull request's patch, or GitHub's own comparison — and pass
        /// this when they match. Without it, a new commit moves the head, which is the
        /// right default: "nobody said" and "the diff changed" lead to the same act.
        ///
        /// The answer carries `action: "diff_unchanged"`, and the review handed back is
        /// the newest round with its head where it was.
        #[arg(long = "diff-unchanged")]
        diff_unchanged: bool,
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
    ///
    /// **Each row in the `--json` answer carries `started`**, which is true when anybody
    /// has begun on that round: it holds a brief, a finding, a draft or a posted review, or
    /// anything only a session writes — a linked session, a layer past `pending`, what was
    /// not checked, the provenance, the round's commits. DEV-954 asks for it so that a
    /// caller does not have to infer it from `session_id`, which a worker writes only once
    /// it has begun: in the window between a push and that link, a round somebody was
    /// reading looked exactly like a request nobody had picked up.
    Find {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
    },

    /// Record a review GitHub has asked for, before any session opens a round of it.
    ///
    /// **The one command here that records no review.** DEV-996: Review Desk learns of a
    /// pull request when a session opens a round on it, and the Locus dispatcher learns of
    /// the review request first — then queues it until a slot frees, which can be hours and
    /// is forever if the dispatcher is down. For that whole time the request is on GitHub,
    /// in the dispatcher's backlog, and nowhere on the queue. On 15 October 2026 the queue
    /// drew 7 pull requests against 9 requests on GitHub, and the gap was exactly this.
    ///
    /// The request is drawn on the queue under "Requested, not started", with no round to
    /// link to and the GitHub link as the way out. It stops being drawn the moment a round
    /// of that pull request opens, because that round's row is then the row.
    ///
    /// **Idempotent on the repository and the pull request, because the caller is a
    /// poller.** The dispatcher calls this on every tick for as long as GitHub lists the
    /// request, and recording the same request twice leaves one record. Each call asserts
    /// the whole state: `--requested-at` replaces the time held before it, and a call
    /// without `--withdrawn` reopens a request that was withdrawn — which is GitHub
    /// dropping a request and asking again, the shape the dispatcher is watching for.
    ///
    /// **`--requested-at` is when *GitHub* asked, not when this command ran.** It is the
    /// one fact Review Desk cannot work out for itself: it reads nothing from GitHub, and
    /// the dispatcher can be minutes or hours behind. A caller that passes its own clock
    /// every tick leaves the row reading "requested just now" for ever. It is also the time
    /// a round is compared against, so a value in the future is a request no round can
    /// answer.
    ///
    /// **Nothing is written when the request is answered.** `satisfied_by_review_id` in the
    /// `--json` answer is read at the moment of asking — the first round of that pull
    /// request opened at or after `requested_at` — so a request recorded after a round
    /// already existed is outstanding, which is a review GitHub has asked for again.
    ///
    /// **Review Desk reads nothing from GitHub for this**, as it reads nothing for
    /// `review out-of-play`. Whatever is watching the pull request says so here.
    Requested {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
        /// When GitHub asked, in seconds since the epoch.
        #[arg(long = "requested-at", value_name = "SECONDS")]
        requested_at: i64,
        /// GitHub no longer lists the request. The way back is this command without it.
        #[arg(long)]
        withdrawn: bool,
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

    /// Decline the developer's request for a fresh session, saying why.
    ///
    /// **Declining, never asking.** The developer asks on the review page when the session
    /// strip says the session is gone, and `work list` then reports the review as
    /// `fresh_session_requested`. A dispatcher that asked Allele and found the old session
    /// still there says so here, and the page shows the reason. One that starts a session
    /// needs no command: linking the new session with `review link --session` answers the
    /// request. There is no form of this that asks for a session.
    FreshSession {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// Why no session was started, for the developer to read.
        #[arg(long, value_name = "REASON")]
        declined: String,
    },

    /// Take a pull request out of play: it was converted to a draft, or its review request
    /// was withdrawn.
    ///
    /// **Every round of it leaves the queue and the work list, with the reason, and nothing
    /// else changes.** DEV-962: a pull request converted to draft is the author saying "not
    /// yet" and a withdrawn request is somebody saying "not you", so neither is a dismissal
    /// — which is the developer's own act on a review they decline — and neither is
    /// finished, which merged and closed are. `review in-play` is the way back, and the
    /// review returns on the round it was on.
    ///
    /// **A sign-off is still offered.** The pull request is coming back, so a draft the
    /// developer wants to approve is a draft that will post when it does; refusing it would
    /// leave a stuck review rather than protect anybody.
    ///
    /// **Review Desk reads none of this.** It polls nothing and writes nothing to GitHub,
    /// so whatever is watching the pull request says so here.
    OutOfPlay {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// The pull request number.
        #[arg(long, value_name = "N")]
        pr: i64,
        /// Why: `draft` or `withdrawn`.
        #[arg(long, value_name = "WHY")]
        reason: OutOfPlayReason,
    },

    /// Put a pull request back in play: it is ready again, or the review was re-requested.
    ///
    /// The undo half of `review out-of-play`, on the round the review was already on. A
    /// pull request that is already in play is not an error: the state is the same
    /// afterwards either way, and a dispatcher that cannot tell whether it has already said
    /// so would otherwise have to ask first.
    InPlay {
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
            ticket_url,
            changed_lines,
            not_checked,
            ledger,
            session,
            diff_unchanged,
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
                // The flag's absence is `Unsaid` rather than "the diff changed": nobody
                // said, and the two lead to the same act. See the flag's own help.
                if *diff_unchanged {
                    DiffAtHead::Unchanged
                } else {
                    DiffAtHead::Unsaid
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
                    // **Which of the four things happened, in one word.** `created` and
                    // `head_moved_from` are kept exactly as they were, and together they
                    // could not tell "this commit was already reviewed" from "these
                    // commits changed nothing" — two different instructions for what a
                    // dispatcher does next. DEV-962 asks for the answer to be explicit
                    // enough to branch on, and this is it.
                    "action": opened.action,
                    // **Whether anybody has begun on the round this call hands back**, the
                    // same question `review find`'s rows answer. It is here because of what
                    // a caller does next: on `head_moved` a dispatcher either wakes the
                    // session already at work or dispatches one, and that is this field —
                    // without it the branch needs a second call, and a dispatcher that
                    // skipped it would dispatch a second worker for a round somebody is
                    // reading.
                    "started": store.review_started(review.id)?,
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
            // the argument back would show `Globex/portal` beside rows that are
            // all `globex/portal`, which is the shape of the fault this answer
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

            // **`started` goes on each row rather than into a wrapper**, so the rows are
            // the same objects they have always been with one key more: a caller reading
            // `reviews[0].round` keeps working, which is the rule every addition to this
            // output has followed. Merged the way `review show` merges the provenance into
            // its document, which is this file's own idiom for exactly this.
            let rows = reviews
                .iter()
                .map(|review| -> Result<serde_json::Value> {
                    let mut row = serde_json::to_value(review)?;
                    if let Some(fields) = row.as_object_mut() {
                        fields.insert(
                            "started".to_string(),
                            json!(store.review_started(review.id)?),
                        );
                    }
                    Ok(row)
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(Report::new(
                json!({ "repo": found, "pr": pr, "reviews": rows }),
                plain,
            ))
        }

        ReviewCommand::Requested {
            repo,
            pr,
            requested_at,
            withdrawn,
        } => {
            let store = Store::open_default()?;
            let request = store.record_review_request(repo, *pr, *requested_at, *withdrawn)?;
            // **Read back rather than reasoned about.** Whether a round has already opened
            // for this request is a comparison the store makes against the record, and a
            // caller deciding whether to dispatch needs the answer in the same call that
            // records the request — a second command to ask is a second thing that can fail
            // between them, which is `review open`'s own argument for `--ledger`.
            let satisfied = store.review_request_satisfied_by(&request.repo, request.pr)?;

            Ok(Report::new(
                json!({
                    // **The stored spelling, which is `review find`'s rule and its reason**:
                    // the record keys on the canonical form, and an answer echoing the
                    // caller's own words is how two spellings of one repository stop being
                    // visible.
                    "repo": request.repo,
                    "pr": request.pr,
                    "requested_at": request.requested_at,
                    "withdrawn": request.is_withdrawn(),
                    "satisfied_by_review_id": satisfied,
                }),
                requested_line(&request, satisfied),
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

        ReviewCommand::FreshSession { review, declined } => {
            let store = Store::open_default()?;
            let request = store.decline_fresh_session(*review, declined)?;
            Ok(Report::new(
                json!({ "fresh_session": request }),
                format!("the fresh session asked for on review {review} was declined"),
            ))
        }

        ReviewCommand::OutOfPlay { repo, pr, reason } => {
            let store = Store::open_default()?;
            let rounds = store.set_pr_out_of_play(repo, *pr, Some(reason.0))?;
            Ok(out_of_play_report(repo, *pr, Some(reason.0), rounds))
        }

        ReviewCommand::InPlay { repo, pr } => {
            let store = Store::open_default()?;
            let rounds = store.set_pr_out_of_play(repo, *pr, None)?;
            Ok(out_of_play_report(repo, *pr, None, rounds))
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

/// The one answer both `review out-of-play` and `review in-play` give.
///
/// **One function, because they are one fact read two ways**: a pull request is out of play
/// for a reason or it is not, and two handlers wording the same answer is two places for it
/// to be worded differently.
///
/// **The repository is the stored spelling, not the one that was typed**, which is
/// `review find`'s rule and its reason: the record holds it canonically, and echoing the
/// argument is how two spellings of one repository come to look like two repositories. Taken
/// from the rows where there are any, and from the same helper the store keys on where there
/// are not — `set_pr_out_of_play` refuses a pull request with no review, so the fallback is
/// unreachable and is here rather than a `null` the contract would have to allow.
fn out_of_play_report(repo: &str, pr: i64, why: Option<OutOfPlay>, rounds: Vec<Review>) -> Report {
    let found = match rounds.first() {
        Some(first) => first.repo.clone(),
        None => review_desk_store::normalised_repo(repo),
    };
    let what = match why {
        Some(why) => format!("is out of play ({why})"),
        None => "is back in play".to_string(),
    };
    let line = format!(
        "{found}#{pr} {what}, {}",
        match rounds.len() {
            1 => "1 round".to_string(),
            count => format!("{count} rounds"),
        }
    );

    Report::new(
        json!({
            "repo": found,
            "pr": pr,
            "out_of_play": why,
            "rounds": rounds,
        }),
        line,
    )
}

/// `draft` or `withdrawn`, as clap reads it off the command line.
///
/// A wrapper rather than `#[arg(value_enum)]` on [`OutOfPlay`] itself: the store's enum is
/// spelled by `string_enum!`, which gives it `FromStr` and `Display` from the same strings
/// the column holds — so this takes those and the command line cannot drift from the
/// record. The error names both values, because a caller who typed `--reason ready` has to
/// be told that the way back is `review in-play`.
#[derive(Debug, Clone, Copy)]
pub struct OutOfPlayReason(OutOfPlay);

impl std::str::FromStr for OutOfPlayReason {
    type Err = String;

    fn from_str(given: &str) -> std::result::Result<Self, Self::Err> {
        given
            .parse::<OutOfPlay>()
            .map(OutOfPlayReason)
            .map_err(|_| {
                format!(
                    "expected one of {}; the way back in is `review in-play`",
                    OutOfPlay::ALL
                        .iter()
                        .map(|why| why.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}

/// `acme/portal#444 requested at 1760000000` — and what has become of the request.
///
/// **Two independent facts, both said.** Whether GitHub has withdrawn the request and
/// whether a round has answered it are different questions with four answers between them,
/// and the line carries each where it applies. The time is the integer the record holds,
/// which is this file's rule for every time it prints — formatting belongs to whatever
/// displays a record, and a date library earns its place in the dashboard rather than
/// here.
fn requested_line(request: &ReviewRequest, satisfied_by: Option<i64>) -> String {
    let mut line = format!(
        "{}#{} requested at {}",
        request.repo, request.pr, request.requested_at
    );
    if request.is_withdrawn() {
        line.push_str(", withdrawn");
    }
    // **Said even on a withdrawn request**, because the two are independent facts: a
    // request can be answered by a round and then dropped from GitHub's list in either
    // order, and a line that reported only the withdrawal would hide the round.
    match satisfied_by {
        Some(review) => line.push_str(&format!(", answered by review {review}")),
        None => line.push_str(", no round opened since"),
    }
    line
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

    // **Said before anything else about the review, because it changes what everything
    // else means.** A dismissed review is in no queue, yields no `work list` item, is not
    // in the Reviews list's default view, and refuses the four acts that need an agent —
    // so a session or a developer handed its id, reading the only view a person reads,
    // has to be told. It was the one fact this printed nothing about: a commit of mine
    // said it had been fixed here and the edit never landed, which `review show` says
    // plainly and the commit message did not.
    if let Some(at) = review.dismissed_at {
        lines.push(match &review.dismissed_reason {
            Some(why) => format!("  dismissed at {at}: {why}"),
            None => format!("  dismissed at {at}"),
        });
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
