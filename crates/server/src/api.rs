//! The JSON API: one function from a request to a status and a body.
//!
//! [`Api::answer`] is deliberately a pure function of the request and the two things the
//! server owns — the store and the GitHub reader. It does no HTTP, which is what lets the
//! conformance test call it with a request and validate what comes back against
//! `contract/openapi.yaml`, and lets `lib.rs` be the only place that knows about sockets.
//!
//! Two rules hold in every handler.
//!
//! **Every refusal is computed before any write.** A validation that runs after the store
//! has been changed gives the caller an error for a request that succeeded, which is the
//! one failure a record-keeping API cannot have.
//!
//! **Nothing here writes to GitHub.** One operation *reads* it — `POST /reviews`, to
//! resolve a head commit the caller did not supply — through a trait with four read-only
//! methods and no write in it.

use review_desk_store::{
    Actor, Decision, Disposition, DraftSectionKind, DraftStatus, FindingDecision, NewDraft,
    NewDraftSection, NewOutcome, OutcomeKind, ReviewEvent, ReviewFacts, ReviewsShown, RuleStage,
    Severity, DEVELOPER_DECISIONS,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{ApiError, Code};
use crate::github::Github;
use crate::route::{match_route, Operation, Target};
use crate::store::{now, Records};
use crate::{assets, wire};

/// The path every operation in the contract sits under, from the contract's `servers`.
pub const API_PREFIX: &str = "/api/v1";

/// The window the Inbox's and the Rules screen's figures cover.
///
/// One number for both, because they answer the same question — "lately" — and two
/// windows that drifted apart would make the two screens disagree about the same week.
const WINDOW_DAYS: i64 = 7;

/// The window the Rules screen's kept-findings figures cover.
const RULES_WINDOW_DAYS: i64 = 30;

/// The store and the GitHub reader, answering the contract's operations.
#[derive(Debug, Clone)]
pub struct Api {
    records: Records,
    github: Github,
}

impl Api {
    /// Serve from this store, reading GitHub through this reader.
    pub fn new(records: Records, github: Github) -> Self {
        Api { records, github }
    }

    /// Answer one request. `path` is the part after [`API_PREFIX`], `query` the part
    /// after the `?`.
    pub fn answer(&self, method: &str, path: &str, query: &str, body: &[u8]) -> (u16, Value) {
        let answered = match match_route(method, path) {
            Target::Health => self.health(),
            Target::Operation(operation) => self.operation(operation, query, body),
            Target::WrongMethod => Err(ApiError::bad_request(format!(
                "{path} does not answer {method}"
            ))),
            Target::Unknown => Err(ApiError::not_found(format!(
                "no operation at {method} {API_PREFIX}{path}"
            ))),
        };

        match answered {
            Ok((status, body)) => (status, body),
            // The body is the contract's `Error` object itself, not a document wrapping
            // one. Every error response in `openapi.yaml` is `$ref: Error`, and the
            // examples under `contract/examples/errors/` are bare objects. The command
            // line wraps its failures in `{"error": ...}` because its own `--json`
            // contract says so; this is a different contract and it says otherwise.
            Err(error) => (error.status(), error.document()),
        }
    }

    /// `GET /health`: Review Desk is up and its database is open and migrated.
    ///
    /// Not in the contract. It is the server saying it is running, which a person
    /// starting it wants and no screen needs.
    fn health(&self) -> Result<(u16, Value), ApiError> {
        Ok((
            200,
            json!({
                "status": "ok",
                "schema_version": self.records.schema_version()?,
                "front_end_embedded": assets::embedded(),
            }),
        ))
    }

    fn operation(
        &self,
        operation: Operation,
        query: &str,
        body: &[u8],
    ) -> Result<(u16, Value), ApiError> {
        match operation {
            Operation::GetInbox => self.inbox(),
            Operation::ListRepos => self.repos(),
            Operation::ListReviews => self.reviews(query),
            Operation::StartReview => self.start_review(body),
            Operation::GetReview { review_id } => self.review(review_id),
            Operation::DismissReview { review_id } => self.dismiss_review(review_id, body),
            Operation::RestoreReview { review_id } => self.restore_review(review_id),
            Operation::DeleteReview { review_id } => self.delete_review(review_id),
            Operation::ListDuplicateRounds => self.duplicate_rounds(query),
            Operation::FoldRound { review_id } => self.fold_round(review_id),
            Operation::ConfirmBrief { review_id } => self.confirm_brief(review_id),
            Operation::GetProblemStatement { review_id } => self.problem_statement(review_id),
            Operation::SetFindingDisposition { review_id, seq } => {
                self.set_finding_disposition(review_id, seq, body)
            }
            Operation::DecideFinding { review_id, seq } => {
                self.decide_finding(review_id, seq, body)
            }
            Operation::GetDraft { review_id } => self.draft(review_id),
            Operation::SaveDraft { review_id } => self.save_draft(review_id, body),
            Operation::ApproveDraft { review_id } => self.approve_draft(review_id, body),
            Operation::SendBackDraft { review_id } => self.send_back_draft(review_id, body),
            Operation::RequestNitTicket { review_id } => self.request_nit_ticket(review_id, body),
            Operation::TicketRestAndApprove { review_id } => {
                self.ticket_rest_and_approve(review_id, body)
            }
            Operation::ListReviewTimings => self.review_timings(query),
            Operation::ListRules => self.rules(query),
            Operation::PromoteRule { rule_id } => self.promote_rule(rule_id, body),
            Operation::DismissRule { rule_id } => self.dismiss_rule(rule_id, body),
            Operation::RetireRule { rule_id } => self.retire_rule(rule_id, body),
            Operation::DeferRule { rule_id } => self.defer_rule(rule_id, body),
        }
    }

    // -- inbox --------------------------------------------------------------------

    /// `GET /inbox`: everything waiting on the developer, in one call.
    fn inbox(&self) -> Result<(u16, Value), ApiError> {
        let briefs = self.records.reviews_with_unconfirmed_brief()?;
        let drafts = self.records.reviews_with_draft_status(DraftStatus::Ready)?;
        let replies = self.records.reviews_with_author_activity()?;
        let in_progress = self.records.reviews_in_progress()?;
        let posted = self.records.posted_reviews_awaiting_author()?;
        let proposed = self.records.rules_at_stages(None, &[RuleStage::Proposed])?;

        let briefs_to_confirm = briefs
            .iter()
            .map(|review| -> Result<Value, ApiError> {
                let layers = self.records.review_layers(review.id)?;
                let at_two = layers.iter().find(|layer| layer.number == 2);
                // **The brief's own time when the step has none.** A session that writes a
                // brief and nothing else leaves layer 2 unwritten — DEV-864's rule, that
                // what a session's steps say about its progress is the session's to write
                // — and the queue showed "no time recorded" over a row it could date from
                // the brief in front of it. The step first, because a step that *has* been
                // written is the one that says whose turn it is and when it became theirs.
                // **When step 2 began waiting on the developer, not when its row was
                // last written.** `set_review_layer` is idempotent, so a session
                // re-reporting the same state moves `updated_at` and would reset "brief
                // ready 6 min ago" — and its place in the oldest-first sort — without
                // anything having changed. The pass's own start is the stable answer, and
                // it is what `draft_ready_at` uses.
                //
                // **The waiting pass, not whatever pass is open.** A session that reports
                // step 2 `running` and then writes the brief without reporting
                // `waiting_on_you` is in this group all the same — the group is keyed on
                // the brief being unconfirmed — and dating it from the open `running` pass
                // would say the brief was ready when the agent *started* reading. That
                // review falls to the brief's own time below, which is what it has.
                //
                // **Falling back to the brief's own time**, for a session that wrote a
                // brief and never reported the step (DEV-864 leaves that step unwritten on
                // a first write). `briefs.updated_at` is written by the write that made
                // the brief ready, which is the thing being dated. The draft has no such
                // fallback on purpose: `drafts.updated_at` moves when the *developer*
                // edits the draft, so it would date their wait from their own last
                // keystroke.
                let ready_at = match at_two.and_then(|layer| layer.waiting_on_you_since()) {
                    Some(entered) => Some(entered),
                    None => self
                        .records
                        .find_brief(review.id)?
                        .map(|brief| brief.updated_at),
                };
                Ok(json!({
                    "review_id": review.id,
                    "pull_request": wire::inbox_pr(review),
                    // Which round this brief belongs to. Every item that makes a queue row
                    // carries one since DEV-957: the row is one pull request, assembled from
                    // items that may be about different rounds, and one with no round cannot
                    // be told from one about the row's own.
                    "round": review.round,
                    // The ticket and its address, so the queue's row can link the ticket
                    // without the review being opened first. On all three arrays that make
                    // a row — this one, `drafts_to_approve` and `author_replies` — because
                    // the queue draws **one row per pull request** and reads the ticket
                    // from the newest round that offered one. (Both halves of that sentence
                    // changed in DEV-957: the key was the review, and the ticket came from
                    // whichever array touched the row last.) See `Inbox`'s own description.
                    "ticket_id": review.ticket_id,
                    "ticket_url": review.ticket_url,
                    "brief_ready_at": wire::rfc3339_or_null(ready_at),
                    // True when the session is holding at layer 2 for this confirmation,
                    // which is what justifies "nothing else runs until you confirm" on
                    // the screen. Read from the layer rather than assumed: a review whose
                    // later layers have already run is not blocked on this.
                    "blocks_later_layers": layers
                        .iter()
                        .filter(|layer| layer.number > 2)
                        .all(|layer| layer.state == review_desk_store::ReviewLayerState::Pending),
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let drafts_to_approve = drafts
            .iter()
            .map(|review| -> Result<Value, ApiError> {
                let findings = self.records.findings(review.id)?;
                let held_back = findings
                    .iter()
                    .filter(|finding| finding.disposition == Disposition::HeldBack)
                    .count();
                let in_draft: Vec<_> = findings
                    .iter()
                    .filter(|finding| finding.disposition == Disposition::InDraft)
                    .cloned()
                    .collect();
                Ok(json!({
                    "review_id": review.id,
                    "pull_request": wire::inbox_pr(review),
                    // Which round this brief belongs to. Every item that makes a queue row
                    // carries one since DEV-957: the row is one pull request, assembled from
                    // items that may be about different rounds, and one with no round cannot
                    // be told from one about the row's own.
                    "round": review.round,
                    // The same pair the brief item carries, and for the same reason: a
                    // review whose brief is confirmed and whose draft is waiting reaches
                    // the queue through this array and no other.
                    "ticket_id": review.ticket_id,
                    "ticket_url": review.ticket_url,
                    "round": review.round,
                    "severities": wire::severity_counts(&in_draft),
                    "held_back": held_back,
                    // "delta since your last review" is what a round above one means.
                    "is_delta": review.round > 1,
                    // When the draft became ready: layer 6 entering `waiting_on_you`. The
                    // queue's age cell said "no time recorded" here until DEV-890.
                    "draft_ready_at": wire::rfc3339_or_null(
                        self.records.draft_ready_at(review.id)?,
                    ),
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let author_replies = replies
            .iter()
            .map(|review| -> Result<Option<Value>, ApiError> {
                // One read, two answers. The author's acts are read once and both the
                // list and its newest time come off that: a second read here would be a
                // second read of the same review per row of the queue.
                //
                // **The store decides which acts those are, since DEV-983.** This filtered
                // a whole `full_review` by actor, which is a question about who — and the
                // one it could not ask is whether a later posted round has already reviewed
                // the change an outcome reports. That is a clause over the review's row and
                // its pull request's other rounds, so it is asked where the group's own
                // membership is asked, in one place. The read is narrower as well: the
                // outcomes rather than the review, its layers, its brief, its findings and
                // its draft.
                let acted = self.records.author_activity(review.id)?;
                // **Which round the pull request has reached, and which review that is.**
                // DEV-983: the queue's single row for a pull request is always its newest
                // round, and this is the one array that can be about an older one. The other
                // two that make a row are served for the newest round only; the two that
                // join onto a row carry their own round. So a pull request whose round 1 was
                // replied to and whose round 2 has been opened and not yet written to is in
                // the answer through this item alone — and without these two fields the page
                // cannot know round 2 exists at all, so the row says "round 1" and opens
                // round 1.
                //
                // **Read per item rather than joined into the query**, because the query
                // answers which reviews belong in the group and this is a fact about the
                // pull request each one is on. One read per row of a queue that already
                // makes several, against a statement every other group would then carry a
                // column for.
                let newest = self.records.newest_round(&review.repo, review.pr)?;

                let events: Vec<Value> = acted
                    .iter()
                    .map(|outcome| {
                        json!({
                            "finding_seq": outcome.finding_seq,
                            "finding_key": wire::finding_key(outcome.finding_seq),
                            "kind": outcome.kind.as_str(),
                            "detail": outcome.detail,
                        })
                    })
                    .collect();

                // The newest of those acts' own times. The outcomes have carried one all
                // along; nothing served it, so the queue's age cell read "no time
                // recorded" over a reply it could have dated.
                let replied_at = acted.iter().map(|outcome| outcome.occurred_at).max();

                // `events` has minItems 1, so a review with none is not an item. The
                // store's query should not return one; serving an invalid item if it did
                // would be worse than serving a shorter list.
                Ok((!events.is_empty()).then(|| {
                    json!({
                        "review_id": review.id,
                        "pull_request": wire::inbox_pr(review),
                        // The third array that makes a row, so it carries the pair too.
                        "ticket_id": review.ticket_id,
                        "ticket_url": review.ticket_url,
                        // **Which round was replied to**, since DEV-957: the queue draws
                        // one row per pull request, and a reply is on the round that was
                        // posted — which can be older than the round the row is numbered
                        // for. Without it the row cannot say which review the reply is on.
                        "round": review.round,
                        // And which round the row itself is, with the review to link. Both
                        // fall back to this review when the record somehow holds no newest
                        // round for its own pull request — which is this review, so the row
                        // is then exactly what it was before DEV-983 rather than a row with
                        // no number at all.
                        "rounds_on_pr": newest
                            .as_ref()
                            .map_or(review.round, |newest| newest.round),
                        "newest_round_review_id": newest
                            .as_ref()
                            .map_or(review.id, |newest| newest.id),
                        "events": events,
                        "replied_at": wire::rfc3339_or_null(replied_at),
                    })
                }))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let proposed_rules: Vec<Value> = proposed
            .iter()
            .map(|rule| {
                json!({
                    "rule_id": rule.id,
                    "rule_key": wire::rule_key(rule.id),
                    "statement": rule.statement,
                    "why_proposed": rule.why_proposed,
                    "scope": wire::rule_scope(&rule.scope),
                })
            })
            .collect();

        let in_progress: Vec<Value> = in_progress
            .iter()
            .map(|at| {
                json!({
                    "review_id": at.review.id,
                    "pull_request": wire::inbox_pr(&at.review),
                    // The ticket and its address, like the three arrays that wait on the
                    // developer. A review here *may* be in no other array — a session is
                    // working and nothing is waiting on anyone — and it then draws its own
                    // row, which is why this array has to carry the pair. It may equally be
                    // joined onto a needs-you row, which is what the `Inbox` schema
                    // describes; "can be in no other array" would say the opposite of that.
                    "ticket_id": at.review.ticket_id,
                    "ticket_url": at.review.ticket_url,
                    // Which round the session is working on. The fourth and last array to
                    // carry one: this query says nothing about rounds, so two rounds of one
                    // pull request can both be here and the queue draws one row for them.
                    "round": at.review.round,
                    "layer_number": at.layer.number,
                    "layer_name": at.layer.name.as_str(),
                    "layers_total": 7,
                    "session_id": at.review.session_id,
                    // Allele owns session liveness and nothing tells this server about
                    // it. Null is the honest answer, and the contract requires the screen
                    // to render it rather than showing "session alive" by default.
                    "session_alive": Value::Null,
                    // **What the server can say instead, since DEV-890**: when this step's
                    // current pass began, and when anything last wrote to the review. The
                    // queue shows "last activity 4 min ago" where it showed "session state
                    // unknown" — a fact the record holds, rather than a guess at Allele's.
                    "in_state_since": wire::rfc3339_or_null(at.layer.in_state_since()),
                    "last_activity_at": wire::rfc3339_or_null(at.review.last_activity_at),
                })
            })
            .collect();

        // Posted, and with the author. **Not a group that is waiting on the developer**,
        // which is why it is absent from `totals_items` below — and why the screen has to
        // count it separately rather than read the headline.
        //
        let posted_awaiting_author: Vec<Value> = posted.iter().map(wire::inbox_posted).collect();

        let since = now() - WINDOW_DAYS * wire::DAY;
        let filed = self.records.filed_nit_tickets(since)?;
        let nits_filed: i64 = filed
            .iter()
            .map(|ticket| ticket.finding_seqs.len() as i64)
            .sum();

        // `own_prs_without_reviewer` is empty, and that is the contract's own position:
        // it is GitHub state, nothing lists the developer's own pull requests, and no
        // issue covers it. Empty reads as absent, which is true.
        let own_prs: Vec<Value> = Vec::new();
        // **The five waiting groups, and deliberately not the other two.** `in_progress`
        // is with the agent and `posted_awaiting_author` is with the author; neither asks
        // the developer for anything, and the screen's headline is a count of what does.
        // The contract's `InboxTotals.items` names the five rather than counting them,
        // because "the five groups" stopped being a safe way to say it the moment there
        // were seven arrays.
        let totals_items = briefs_to_confirm.len()
            + drafts_to_approve.len()
            + author_replies.len()
            // Always zero today, and written as a term rather than left out. It was left
            // out, and the sum was right only because the group two lines below is a
            // hardcoded empty array — which also made the matching term in
            // `conformance.rs`'s assertion vacuous. The day something supplies the
            // developer's own pull requests, this sum is already right and that test
            // already checks it.
            + own_prs.len()
            + proposed_rules.len();
        // **The repositories the three waiting groups actually hold items for.**
        //
        // `author_replies` is read from the items rather than from `replies`, which is the
        // set of reviews the store returned: an item whose every event turned out to be a
        // code change a posted round had already reviewed is dropped above, and the two
        // reads are not one snapshot — so a post landing between them can empty an item
        // this count had already credited its repository for. Counting what was served
        // cannot disagree with what was served.
        let mut repos: std::collections::BTreeSet<&str> = briefs
            .iter()
            .chain(drafts.iter())
            .map(|review| review.repo.as_str())
            .collect();
        for item in &author_replies {
            if let Some(repo) = item["pull_request"]["repo"].as_str() {
                repos.insert(repo);
            }
        }

        Ok((
            200,
            json!({
                "totals": { "items": totals_items, "repos": repos.len() },
                "briefs_to_confirm": briefs_to_confirm,
                "drafts_to_approve": drafts_to_approve,
                "author_replies": author_replies,
                "own_prs_without_reviewer": own_prs,
                "proposed_rules": proposed_rules,
                "in_progress": in_progress,
                "posted_awaiting_author": posted_awaiting_author,
                "handled_without_you": {
                    "window_days": WINDOW_DAYS,
                    // Needs a CI source Review Desk has not got.
                    "description_problems_caught": Value::Null,
                    "nits_filed": nits_filed,
                    "nit_tickets": filed.len(),
                },
            }),
        ))
    }

    /// `GET /reviews`: every review on the record, newest first.
    ///
    /// **The one page where a review that is not waiting on anybody can be found.**
    /// `GET /inbox` is what is waiting and `GET /timings` is what has finished, so a
    /// review posted and not yet replied to appeared in neither and was reachable only by
    /// typing its id.
    ///
    /// The repository is checked rather than merely filtered on, as `listRules` and
    /// `listReviewTimings` check the same parameter: an unchecked filter answers a
    /// mistyped repository with an empty list, which reads as "no reviews there" rather
    /// than "there is no such repository".
    fn reviews(&self, query: &str) -> Result<(u16, Value), ApiError> {
        // **Normalised once, here, so the rows and the echo are about one repository.**
        // The store keys on the canonical spelling and every list below normalises for
        // itself, so the rows were always right — but the answer echoed the query, and
        // the contract says the page's filter reads that field rather than trusting its
        // own state. `ACME/Portal` came back beside rows all spelled `acme/portal`, which
        // is the shape of the fault the Locus half found in `review find`: when a lookup
        // misses, an answer repeating the caller's words shows them nothing.
        let repo = repo_filter(query)?;

        // **A review off the desk is left out unless it is asked for.** The default is
        // the narrow view rather than everything, for the reason `ReviewsShown`'s own doc
        // gives: a caller that has never heard of a dismissal or of a finish must not be
        // handed a review that has left the desk. A value that is not one of the four is
        // refused by name rather than falling back to the default — a mistyped filter
        // answering with the default view would read as "there are no dismissed reviews".
        //
        // **The parameter is `show`, and it was `dismissed` until DEV-957.** That issue
        // adds a fourth position, Finished, and `dismissed=finished` would have been a
        // parameter whose name said the opposite of its value. Renamed rather than
        // extended, and nothing outside this dashboard reads it.
        //
        // **An empty value is a value.** `query_value` drops one, so `?show=` arrived
        // as "nothing was asked" and answered with the default view — which is what this
        // paragraph says must not happen, for a client that serialises its filters
        // whether or not they are set: the page reads its view line off this field, so it
        // would have said "Every review in play" under a picker set to Dismissed.
        // **The name this parameter had until DEV-957 is refused by name.** An unknown
        // parameter is ignored by this server, so `?dismissed=only` from a bookmark or a
        // saved client would have answered with the in-play view — which is exactly the
        // failure the paragraph above forbids, reached through the rename rather than
        // through a typo. Found by a review of this branch.
        if query
            .split('&')
            .any(|pair| pair == "dismissed" || pair.starts_with("dismissed="))
        {
            return Err(ApiError::invalid_value(
                "dismissed",
                "`dismissed` was renamed to `show` in DEV-957, when it gained a value \
                 called `finished`"
                    .to_string(),
                format!("show, one of: {}", ReviewsShown::valid_values()),
            ));
        }

        let asked_blank = query
            .split('&')
            .any(|pair| pair == "show=" || pair == "show");
        let shown = match query_value(query, "show") {
            None if asked_blank => {
                return Err(ApiError::invalid_value(
                    "show",
                    "\"\" is not a choice about which reviews to show".to_string(),
                    ReviewsShown::valid_values(),
                ))
            }
            None => ReviewsShown::default(),
            Some(asked) => asked.parse::<ReviewsShown>().map_err(|_| {
                ApiError::invalid_value(
                    "show",
                    format!("{asked:?} is not a choice about which reviews to show"),
                    ReviewsShown::valid_values(),
                )
            })?,
        };

        let reviews: Vec<Value> = self
            .records
            .reviews_at_step(repo.as_deref(), shown)?
            .iter()
            .map(wire::review_list_item)
            .collect();

        Ok((
            200,
            json!({
                "repo": repo,
                "show": shown.as_str(),
                "reviews": reviews,
            }),
        ))
    }

    /// `GET /repos`: the repositories Review Desk holds a record for.
    fn repos(&self) -> Result<(u16, Value), ApiError> {
        let repos: Vec<Value> = self
            .records
            .list_repos()?
            .iter()
            .map(wire::repo_summary)
            .collect();
        Ok((200, json!({ "repos": repos })))
    }

    // -- one review ---------------------------------------------------------------

    /// `POST /reviews`: start a review from a pull request link.
    ///
    /// The order of the checks is the point of this handler, and DEV-861 states it: the
    /// request is checked in full before anything reaches for GitHub, so a machine with
    /// no `gh` reports the real problem. A malformed link is a `400`, not a report that
    /// `gh` is missing.
    fn start_review(&self, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: StartReviewRequest = read_body(body)?;
        let link = wire::parse_pr_link(&request.pr_url)?;

        let (head_sha, resolved) = match request.head_sha {
            // Supplied, so no read is made at all: the command line and a review session
            // both know the head already, and the contract says they save the server a
            // read by passing it.
            Some(head_sha) => {
                if !wire::is_head_sha(&head_sha) {
                    return Err(ApiError::invalid_value(
                        "head_sha",
                        format!("{head_sha:?} is not a commit hash"),
                        "7 to 40 lower-case hexadecimal characters",
                    ));
                }
                (head_sha, None)
            }
            // Omitted, which is what the Inbox screen does: it has only a pasted link.
            // The head is resolved before the review is created, because the store
            // deduplicates on it and a headless review could not be matched on the next
            // click. If the read fails, nothing is created.
            None => {
                let read = self.github.pull_request(&link.repo, link.pr)?;
                (read.head_sha.clone(), Some(read))
            }
        };

        // **One read, three facts.** `contract/README.md` item S22 always said this read
        // supplied the title and the changed-line count; until DEV-862 the `GitHub`
        // trait's `PullRequest` carried neither, so this handler wrote `title: null` for
        // every review the Inbox started and a comment here said the item was wrong. The
        // read was widened instead: a review with no title has nothing for the Inbox and
        // the One review screen to list it under, which is every screen it appears on.
        //
        // A caller that passed a head commit gets no read and therefore no resolution,
        // which is the bargain the contract offers: pass what you know and save the read.
        //
        // What the caller sent is written whatever is already there — stating a value is
        // asking for it to be recorded. What was *resolved* is not, and the next call is
        // where the difference shows.
        let stated = ReviewFacts {
            title: request.title,
            ticket_id: request.ticket_id,
            ticket_url: request.ticket_url,
            changed_lines: request.changed_lines,
            not_checked: None,
        };

        let opened = self
            .records
            .open_review(&link.repo, link.pr, &head_sha, &stated)?;

        // **A resolved value fills a gap; it never replaces an answer.**
        //
        // `POST /reviews` is deduplicating — the same head commit hands back the review
        // already on the record — so pasting a link twice reaches this a second time. With
        // the resolved facts written unconditionally, the second call overwrote the first's
        // with GitHub's: a session that opened round 2 with the delta it actually reviewed
        // (`review open --changed-lines 180`) had that replaced by the whole pull request's
        // additions plus deletions the moment the developer pasted the link. That figure is
        // what the draft's word budget is computed from, so the overwrite was not cosmetic —
        // and it contradicted this contract's own sentence, that a title you pass is one
        // nothing then overwrites.
        //
        // Only a field the record has no answer for is filled, which is also the only case
        // the read was widened for.
        //
        // **And a line count is filled only on a first round, because that is the round the
        // figure is about.** What this read resolves is the pull request's own additions plus
        // deletions; round 1 reviews the whole pull request, so the figure is its subject,
        // and every later round is about the delta since the round before it — for which a
        // whole-pull-request count is a wrong answer rather than a missing one. It is the
        // figure the draft's word budget is computed from, so the difference is not cosmetic.
        //
        // **Two wrong answers, one condition.** This gated on "did the head move" first,
        // which DEV-962 made newly reachable: the Inbox's own paste resolves the head, so a
        // pull request the author had pushed to moved the round and the cleared count was
        // refilled with the whole pull request's. But the move was never what made the figure
        // wrong — a *round 2* opened on the pull request's current commit takes no move at
        // all, and was filled with the same wrong number; and a *round 1* whose head moved
        // had a correct figure thrown away. Both measured by a review of the fix, in probes
        // against this server. The round is the question, and asking it answers both.
        //
        // The title is filled on any round, which is right: it is a fact about the pull
        // request rather than about a commit or a round, which is also why a move leaves it
        // alone.
        //
        // **One cell where the round number and the history diverge, and it fails safe.**
        // `round == 1` stands in for "nothing has been posted to the author yet", and the
        // duplicate-round tidy can part the two: folding a legacy round 1 into round 2 leaves
        // one round, numbered 2, on a pull request nothing ever posted — so a paste declines a
        // whole-pull-request count that would have been right. It needs records written before
        // DEV-962 to reach, and it leaves a null rather than a wrong figure, which is the
        // direction this whole paragraph argues for. Noticed by a review of the fix.
        let gaps = ReviewFacts {
            title: resolved
                .as_ref()
                .and_then(|read| read.title.clone())
                .filter(|_| opened.review.title.is_none()),
            ticket_id: None,
            // Nothing resolves a ticket's address. Review Desk fetches nothing from a
            // tracker, and GitHub does not know which ticket a pull request is for — so
            // this is given by the caller or it stays empty, which is migration 16's rule.
            ticket_url: None,
            changed_lines: resolved
                .as_ref()
                .and_then(|read| read.changed_lines)
                .filter(|_| opened.review.round == 1 && opened.review.changed_lines.is_none()),
            not_checked: None,
        };

        let review = if gaps.is_empty() {
            opened.review
        } else {
            self.records.set_review_facts(opened.review.id, &gaps)?
        };

        Ok((201, wire::review(&review)))
    }

    /// `POST /reviews/{reviewId}/dismiss`: the developer takes a review out of play.
    ///
    /// **Hidden and kept.** The review leaves the queue, the work list and the Reviews
    /// list's default view, and every row under it stays — which is why this is the answer
    /// for every review and a delete is the answer only for one that never started.
    /// Patrick decided both on 12 October 2026.
    ///
    /// **A reason is required**, as it is for a dismissed rule and for the same reason: a
    /// review hidden silently is one nobody can afterwards say why they hid. The refusal
    /// names the field, so the page can put the message under the box the developer typed
    /// in.
    ///
    /// **This is the only way to dismiss a review.** There is no command for it, the way
    /// there is no command to confirm a brief or approve a draft: it is the developer's
    /// act, and a session that could reach it could take its own review out of the
    /// developer's sight.
    fn dismiss_review(&self, review_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        // **The review first, then the body**, which is the order every sibling handler
        // uses and the one this file's own rule implies: a review that is not there is a
        // 404, and judging the reason first answered `invalid_value` about a field on a
        // review nobody has. Found by a review of this branch — the unknown-review test
        // sends a reason that is valid, so it could not have caught it.
        self.records.review(review_id)?;

        let request: DismissReviewRequest = read_body(body)?;
        let reason = trimmed(Some(request.reason)).ok_or_else(|| {
            ApiError::invalid_value(
                "reason",
                "a review dismissed with no reason is one nobody can say why they hid".to_string(),
                "why this review is being dismissed",
            )
        })?;

        let review = self.records.dismiss_review(review_id, &reason)?;
        Ok((200, wire::review(&review)))
    }

    /// `POST /reviews/{reviewId}/restore`: bring a dismissed review back.
    ///
    /// The undo half of a dismissal, and the reason a dismissal is the recommended act for
    /// everything: nothing about it is final. A review that is not dismissed is a `409` —
    /// nothing would change, and answering 200 for a request that did nothing is how a
    /// screen comes to show a button that appears to work.
    fn restore_review(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        self.records.review(review_id)?;
        let review = self.records.restore_review(review_id)?;
        Ok((200, wire::review(&review)))
    }

    /// `DELETE /reviews/{reviewId}`: remove a review that never got past being requested.
    ///
    /// **Offered only for a review that holds nothing** — no brief, no finding, no
    /// draft, and no review posted on GitHub — and the store is the one that judges it.
    /// A second opinion here is how the page comes to offer a button the API refuses.
    /// The refusal is a `409` naming what the review holds, so the developer reads
    /// "review 7 holds a brief and 4 findings" and is pointed at dismissal instead.
    ///
    /// **It answers with the review that went.** Nothing can read it again, and the page
    /// has a line to draw about what was removed.
    fn delete_review(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        self.records.review(review_id)?;
        let review = self.records.delete_review(review_id)?;
        Ok((200, wire::review(&review)))
    }

    /// `GET /duplicate-rounds`: the one-off tidy's proposal, per pull request.
    ///
    /// **It shows and decides nothing.** DEV-962 asks for a tidy that "shows each pull
    /// request's extra unposted rounds and folds them into the newest (or dismisses them);
    /// nothing is deleted without confirmation" — so this is a read, and the two acts it
    /// proposes are `POST /reviews/{reviewId}/fold` and the dismissal that already exists.
    /// There is no command line path to any of the three.
    ///
    /// **Nothing new can appear here.** Since DEV-962 a round opens only when the newest one
    /// has posted, so a pull request cannot grow a second unposted round; what this answers
    /// about is the record as it stands.
    fn duplicate_rounds(&self, query: &str) -> Result<(u16, Value), ApiError> {
        let repo = repo_filter(query)?;
        let found = self.records.duplicate_rounds(repo.as_deref())?;

        Ok((
            200,
            json!({
                "pull_requests": found
                    .iter()
                    .map(wire::duplicate_rounds)
                    .collect::<Vec<_>>(),
                // The two counts the screen's heading needs, so it does not add them up
                // itself and disagree with the rows.
                "pull_requests_affected": found.len(),
                "extra_rounds": found
                    .iter()
                    .map(|found| found.extras.len())
                    .sum::<usize>(),
            }),
        ))
    }

    /// `POST /reviews/{reviewId}/fold`: fold an extra unposted round into the newest one.
    ///
    /// **The developer's act, and this is the only surface it happens on**, as a dismissal
    /// and a delete are: a session that could reach it could fold away a round of its own
    /// review. The store judges whether the round may be folded — it refuses the newest
    /// round, a round that posted, and a round holding a brief, a finding or a draft — and a
    /// second opinion here is how a screen comes to offer a button the API refuses.
    ///
    /// **It answers with both rounds**: the one that went, because nothing can read it
    /// again and the page has a line to draw about what was removed, and the one it folded
    /// into, because that is the row the screen keeps.
    fn fold_round(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        self.records.review(review_id)?;
        let folded = self.records.fold_round(review_id)?;

        Ok((
            200,
            json!({
                "folded": wire::review(&folded.folded),
                "into": wire::review(&folded.into),
            }),
        ))
    }

    /// `GET /reviews/{reviewId}`: one review whole, as the One review screen shows it.
    ///
    /// **Where the coverage answer is decided.** A review can record two things about what
    /// it did not look at: one sentence (`reviews.not_checked`, since DEV-854) and a
    /// per-file change map with a row per unreviewed area (since DEV-875). Two records a
    /// screen could render side by side are two records that will eventually say different
    /// things, so this handler picks one — `coverage.source` — and serves the other as
    /// null. See [`wire::coverage_source`] and [`wire::not_checked`].
    fn review(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        let full = self.records.full_review(review_id)?;
        let ticket = self.records.nit_ticket(review_id)?;
        let provenance = self.records.review_provenance(review_id)?;

        // The findings on each file, read from the findings by path. The change map stores
        // no finding column, so this is the only copy of the join and it cannot go stale.
        let mut rows = Vec::with_capacity(provenance.files.len());
        for file in &provenance.files {
            let seqs = self.records.findings_on_path(review_id, &file.path)?;
            rows.push(wire::file_change(file, &seqs));
        }

        let source = wire::coverage_source(&provenance.files, full.review.not_checked.as_deref());

        // The round-two page's three questions: which rounds there have been, what changed
        // since the last one, and where each finding stands in this one. All three in this
        // one response, because the contract's rule is that each screen is one GET.
        let rounds = self.records.rounds(&full.review.repo, full.review.pr)?;
        let commits = self.records.round_commits(review_id)?;
        let findings = self.records.findings_in_round(review_id)?;

        Ok((
            200,
            json!({
                "review": wire::review(&full.review),
                // **Why nothing on it may be signed off, or null.** The same value the draft
                // page reads, one step earlier: confirming a brief is one of the four acts
                // the store refuses on a review that has left the desk, so the screen that
                // offers it needs the answer or it offers a button the API refuses.
                "not_for_signing": wire::not_for_signing(
                    self.records.not_for_signing(review_id)?,
                ),
                "layers": full.layers.iter().map(wire::review_layer).collect::<Vec<_>>(),
                "brief": full.brief.as_ref().map_or(Value::Null, wire::brief),
                "findings": findings.iter().map(wire::finding_in_round).collect::<Vec<_>>(),
                "outcomes": full.outcomes.iter().map(wire::outcome).collect::<Vec<_>>(),
                "finding_severities": wire::severity_counts(&full.findings),
                "finding_dispositions": wire::disposition_counts(&full.findings),
                "not_checked": wire::not_checked(source, full.review.not_checked.as_deref()),
                "nit_ticket": wire::nit_ticket(&ticket),
                "draft_status": full
                    .draft
                    .as_ref()
                    .map_or(Value::Null, |draft| json!(draft.status.as_str())),
                "sources": provenance
                    .sources
                    .iter()
                    .map(wire::review_source)
                    .collect::<Vec<_>>(),
                "criteria": provenance
                    .criteria
                    .iter()
                    .map(wire::acceptance_criterion)
                    .collect::<Vec<_>>(),
                "description_lint": wire::description_lint(&provenance),
                "coverage": wire::coverage(source, rows),
                // The totals. Each step's own figures are inside `layers` above, with the
                // passes they were computed from.
                "timings": wire::review_timings(&self.records.review_timings(review_id)?),
                "rounds": rounds.iter().map(wire::round_summary).collect::<Vec<_>>(),
                "commits_since_previous_round": commits
                    .iter()
                    .map(wire::round_commit)
                    .collect::<Vec<_>>(),
                // **What has happened to this round's head since it was opened.** The
                // fourth question the round-two page asks, and the one DEV-962 added: the
                // page says "Updated for 3 commits since `4d9464f`", and the code review
                // shows what changed since the draft was written. Served with the round
                // rather than fetched on its own, which is this contract's rule that each
                // screen is one GET.
                "head_updates": wire::head_updates(
                    &self.records.head_updates(review_id)?,
                ),
            }),
        ))
    }

    /// `POST /reviews/{reviewId}/brief/confirm`: the developer's act at layer 2.
    ///
    /// **A sign-off endpoint.** Only the developer can sign off — Patrick, 9 October 2026
    /// — and this API is the only surface where it happens: `review-desk brief confirm`
    /// no longer exists. The actor is `store::SIGN_OFF_ACTOR`, a constant, and the store
    /// refuses any other.
    fn confirm_brief(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        // Checked first so an unknown review is 404 and a review with no brief is 409.
        // The contract gives the operation both, and they are different things to say.
        self.records.review(review_id)?;
        self.records.brief_to_confirm(review_id)?;

        // Confirming an already confirmed brief returns it unchanged rather than failing,
        // which is what the store does and what the contract asks for.
        let brief = self.records.confirm_brief(review_id)?;
        Ok((200, wire::brief(&brief)))
    }

    /// `GET /reviews/{reviewId}/brief/problem-statement`: the problem, with nothing of
    /// the change in it.
    ///
    /// **The one read a blind pass may be given.** It serves how the system works today,
    /// the before flow and what was wrong with each numbered problem. The headline, the
    /// after flow, every problem's `now_fixed`, the named detail, the supporting table,
    /// the options, the blind pass, the parts, the choices and the disagreements are all
    /// absent — an independent view of a problem stops being independent the moment it is
    /// shown the answer.
    ///
    /// It is a read and it decides nothing: no confirmation, no layer, no write.
    fn problem_statement(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        // Checked first so an unknown review is 404 and a review with no brief is 409 —
        // the two things `confirmBrief` distinguishes, for the same reason.
        self.records.review(review_id)?;
        let statement = self.records.problem_statement(review_id)?;
        Ok((200, wire::problem_statement(&statement)))
    }

    /// `PATCH /reviews/{reviewId}/findings/{seq}`: move a finding between the draft, held
    /// back and suppressed.
    ///
    /// A finding is never deleted. Holding it back or suppressing it records a reason,
    /// which is what lets a later rule proposal cite it — so the reason is required here
    /// rather than merely accepted, and it is checked before the store is touched.
    fn set_finding_disposition(
        &self,
        review_id: i64,
        seq: i64,
        body: &[u8],
    ) -> Result<(u16, Value), ApiError> {
        let request: SetFindingDispositionRequest = read_body(body)?;
        let disposition: Disposition = request.disposition.parse().map_err(ApiError::from)?;
        let reason = trimmed(request.reason);

        if reason.is_none() && disposition != Disposition::InDraft {
            return Err(ApiError::invalid_value(
                "reason",
                format!(
                    "moving a finding to {disposition} needs a reason: \
                     a finding removed from the draft without one teaches the harvester nothing"
                ),
                "a sentence saying why",
            ));
        }

        // Read before write, so an unknown finding is a 404 that changed nothing.
        self.records.finding(review_id, seq)?;

        let finding = self
            .records
            .set_finding_disposition(review_id, seq, disposition, reason)?;
        Ok((200, wire::finding(&finding)))
    }

    // -- the draft ----------------------------------------------------------------

    /// Everything the Draft preview screen shows, for a draft already read.
    fn draft_detail(&self, draft: &review_desk_store::Draft) -> Result<Value, ApiError> {
        let review_id = draft.review_id;
        let review = self.records.review(review_id)?;
        let findings = self.records.findings(review_id)?;
        let ticket = self.records.nit_ticket(review_id)?;

        // The developer's edits are not stored on the draft. They are outcomes with actor
        // `developer`, which is where the record already puts them and which is why
        // `PUT .../draft` takes them in its body: "regraded F3 from Blocker to Should" is
        // not recoverable from the characters that changed.
        let edits: Vec<Value> = self
            .records
            .full_review(review_id)?
            .outcomes
            .iter()
            .filter(|outcome| outcome.actor == Actor::Developer)
            // Every kind a decision leaves behind, named once in the store. Before
            // DEV-877 this was the three the developer could make by editing markdown;
            // the list is now the eight decisions, and a ninth added to the store without
            // this list being revisited is a compile error rather than a row quietly
            // missing from the screen.
            .filter(|outcome| DEVELOPER_DECISIONS.contains(&outcome.kind))
            .map(wire::draft_edit)
            .collect();

        let held_back: Vec<Value> = findings
            .iter()
            .filter(|finding| finding.disposition == Disposition::HeldBack)
            .map(wire::held_back_finding)
            .collect();

        let sections: Vec<Value> = self
            .records
            .draft_sections(review_id)?
            .iter()
            .map(wire::draft_section)
            .collect();

        // **The threads, beside the body they are not in.** A finding GitHub can anchor is
        // a row in the body's index and a comment on the code, so the page cannot show what
        // posts from the body alone — DEV-929. They are composed from the same blocks, so
        // cutting a finding takes its row and its thread together.
        let threads: Vec<Value> = self
            .records
            .draft_threads(review_id)?
            .iter()
            .map(wire::draft_thread)
            .collect();

        Ok(json!({
            "draft": wire::draft(draft),
            "review": wire::review(&review),
            "lint": wire::draft_lint(&draft.lint),
            "sections": sections,
            "threads": threads,
            // **The one state the hand-over cannot serve**, so the screen can say why and
            // offer the way out rather than leaving the developer to find that the session
            // is stuck. False on every draft this build wrote; true of one approved before
            // the composition changed shape.
            "body_is_stale": self.records.draft_body_is_stale(review_id)?,
            // **Why this draft may not be signed off, or null.** The screen draws its
            // sentence from this and the store refuses from the same value, so the page
            // cannot offer a sign-off the API would refuse — DEV-957's "an unposted draft
            // on a finished review says so and cannot be signed off" is one field.
            "not_for_signing": wire::not_for_signing(self.records.not_for_signing(review_id)?),
            "edits": edits,
            "held_back": held_back,
            "nit_ticket": wire::nit_ticket(&ticket),
        }))
    }

    /// `GET /reviews/{reviewId}/draft`.
    fn draft(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        self.records.review(review_id)?;
        let draft = self.records.draft(review_id)?;
        Ok((200, self.draft_detail(&draft)?))
    }

    /// `PUT /reviews/{reviewId}/draft`: save an edited body and record the edits.
    ///
    /// Refused once the draft is `approved` or `posted`: an approved draft is what the
    /// review session reads, and editing it underneath would change what gets posted.
    fn save_draft(&self, review_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: SaveDraftRequest = read_body(body)?;
        self.records.review(review_id)?;
        let existing = self.records.draft(review_id)?;

        if matches!(existing.status, DraftStatus::Approved | DraftStatus::Posted) {
            return Err(ApiError::conflict(format!(
                "this draft is {} and cannot be edited: \
                 an approved draft is what the review session reads and posts",
                existing.status
            )));
        }

        if request.body.is_none() && request.sections.is_none() {
            return Err(ApiError::invalid_value(
                "body",
                "a save with neither a body nor the blocks to compose one",
                "the blocks the body is composed of, or a body to store as one block",
            ));
        }

        // Every edit is checked against a finding that exists before anything is written,
        // so a typo in one edit cannot leave the body saved and the outcomes half
        // recorded.
        for edit in &request.edits {
            self.records.finding(review_id, edit.finding_seq)?;
            if edit.summary.trim().is_empty() {
                return Err(ApiError::invalid_value(
                    "edits[].summary",
                    format!("the edit to F{} has an empty summary", edit.finding_seq),
                    "what changed, in a few words",
                ));
            }
        }

        // The status is kept, not taken from the request: `PUT` saves a body. Moving a
        // draft to `ready` is the session saying it has finished, and approving it is the
        // developer's own endpoint — neither is something a save should be able to do.
        let saved = self.records.put_draft(
            review_id,
            NewDraft {
                body: request.body.clone().unwrap_or_default(),
                status: match existing.status {
                    DraftStatus::Ready => DraftStatus::Ready,
                    _ => DraftStatus::Drafting,
                },
                // The body changed, so whatever the lint said about the old one is no
                // longer about this draft. Cleared rather than kept: a stale lint result
                // shown against a new body is worse than no result.
                lint: None,
                sections: request
                    .sections
                    .as_ref()
                    .map(|sections| sections.iter().map(SaveDraftSection::stored).collect()),
            },
        )?;

        for edit in &request.edits {
            self.records.add_outcome(
                review_id,
                edit.finding_seq,
                NewOutcome {
                    kind: edit.kind,
                    actor: Actor::Developer,
                    detail: Some(edit.summary.clone()),
                    occurred_at: None,
                },
            )?;
        }

        Ok((200, self.draft_detail(&saved)?))
    }

    /// Refuse a draft that is not in a state this act can be performed from.
    ///
    /// **Checked here rather than left to the store**, for two reasons. The store refuses
    /// it too, but as a generic refusal that maps to 400, and the contract declares 409
    /// for every one of these — "the draft is not in a state that can be approved" is a
    /// statement about the record, not about the request. And checking first keeps the
    /// handler's own rule: every refusal is computed before anything is written.
    fn draft_must_be(
        &self,
        review_id: i64,
        wanted: DraftStatus,
        act: &str,
    ) -> Result<(), ApiError> {
        let draft = self.records.draft(review_id)?;
        if draft.status != wanted {
            return Err(ApiError::conflict(format!(
                "this draft is {} and only a draft that is {wanted} can be {act}",
                draft.status
            )));
        }
        Ok(())
    }

    /// `POST /reviews/{reviewId}/draft/approve`.
    ///
    /// **A sign-off endpoint, and it does not post.** It sets the status to `approved`,
    /// stores the chosen event and the time, and stops. The review session finds the
    /// approved draft and posts it under the developer's name.
    fn approve_draft(&self, review_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: ApproveDraftRequest = read_body(body)?;
        let event: ReviewEvent = request.event.parse().map_err(ApiError::from)?;

        self.records.review(review_id)?;
        self.draft_must_be(review_id, DraftStatus::Ready, "approved")?;

        let approved = self.records.approve_draft(review_id, event)?;
        Ok((200, self.draft_detail(&approved)?))
    }

    /// `POST /reviews/{reviewId}/draft/send-back`.
    fn send_back_draft(&self, review_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: SendBackDraftRequest = read_body(body)?;

        let note = trimmed(Some(request.note)).ok_or_else(|| {
            ApiError::invalid_value(
                "note",
                "a draft sent back with no note teaches the session nothing",
                "what the session is to change",
            )
        })?;

        self.records.review(review_id)?;

        // **Ready, or approved and not yet posted.** Taking a sign-off back is the one
        // transition out of `approved`, and without it a draft the hand-over refuses has
        // nowhere to go — measured by the DEV-929 worker half, which found such a draft
        // could not be sent back, rewritten or posted. Still the developer's act, through
        // the same route and with the same note: nothing here approves or posts anything.
        let draft = self.records.draft(review_id)?;
        if !matches!(draft.status, DraftStatus::Ready | DraftStatus::Approved) {
            return Err(ApiError::conflict(format!(
                "this draft is {} and only a draft that is ready, or one approved and not \
                 yet posted, can be sent back",
                draft.status
            )));
        }

        let sent_back = self.records.send_back_draft(review_id, &note)?;
        Ok((200, self.draft_detail(&sent_back)?))
    }

    /// `POST /reviews/{reviewId}/findings/{seq}/decide`: the developer's decision about
    /// one finding, and the draft recomposed from it.
    ///
    /// **Eight decisions behind the designs' eleven buttons**, with the mapping in the
    /// store's `FindingDecision`. Each is one transaction there: the finding, its block of
    /// the body, the body itself and the outcome recording who decided are written
    /// together, and the draft's lint reads as not checked afterwards.
    ///
    /// Only the developer decides. The actor is not a parameter — see
    /// `store::DECIDING_ACTOR` — and nothing in the command line reaches the store
    /// function behind this.
    fn decide_finding(
        &self,
        review_id: i64,
        seq: i64,
        body: &[u8],
    ) -> Result<(u16, Value), ApiError> {
        let request: DecideFindingRequest = read_body(body)?;
        let decision: FindingDecision = request.decision.parse().map_err(ApiError::from)?;
        let severity: Option<Severity> = request
            .severity
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(ApiError::from)?;

        // Read before written, so an unknown review or finding is a 404 that changed
        // nothing. The store checks both again inside its transaction.
        self.records.review(review_id)?;
        self.records.finding(review_id, seq)?;

        let made = self.records.decide_finding(
            review_id,
            seq,
            &Decision {
                decision,
                severity,
                title: trimmed(request.title),
                // Not trimmed. The paragraph is what gets posted, and a trailing space in
                // markdown is a line break; the store refuses the one shape that would
                // compose a body nobody wrote, which is a newline at either end — in
                // `composition::validate_rewrite`, which the review of DEV-877 found this
                // comment had been describing before it existed.
                text: request.text,
                note: trimmed(request.note),
            },
        )?;

        // The finding, the outcome and the whole draft, because the review page draws all
        // three and a decision changes all three. One call, one re-render.
        Ok((
            200,
            json!({
                "finding": wire::finding(&made.finding),
                "outcome": wire::outcome(&made.outcome),
                "draft": self.draft_detail(&made.draft)?,
            }),
        ))
    }

    /// `POST /reviews/{reviewId}/draft/ticket-and-approve`: file what is left as one
    /// ticket, and approve.
    ///
    /// Behind "Ticket what is left, and approve". **A sign-off, and it neither posts nor
    /// files anything.** Every finding the draft would have posted is held back for the
    /// ticket and recorded as ticketed this round, the review's one nit ticket is asked
    /// for, and the draft is approved to be posted as `approve`. The review session finds
    /// the approved draft and the ticket in `work list`, posts the one and files the
    /// other.
    ///
    /// **The approval cannot link to the ticket**, which is a gap in the design rather
    /// than an omission here: Review Desk files no ticket, so no ticket id exists at the
    /// moment of approval. Only the session that files it can add that line.
    fn ticket_rest_and_approve(
        &self,
        review_id: i64,
        body: &[u8],
    ) -> Result<(u16, Value), ApiError> {
        let request: TicketRestAndApproveRequest = if body.is_empty() {
            TicketRestAndApproveRequest::default()
        } else {
            read_body(body)?
        };

        self.records.review(review_id)?;
        self.draft_must_be(review_id, DraftStatus::Ready, "approved")?;

        let note = trimmed(request.note);
        let settled = self
            .records
            .ticket_rest_and_approve(review_id, note.as_deref())?;

        Ok((
            200,
            json!({
                "draft": self.draft_detail(&settled.draft)?,
                "nit_ticket": wire::nit_ticket(&settled.ticket),
                "ticketed": settled.ticketed,
                "ticketed_keys": settled
                    .ticketed
                    .iter()
                    .map(|seq| wire::finding_key(*seq))
                    .collect::<Vec<_>>(),
            }),
        ))
    }

    /// `POST /reviews/{reviewId}/held-back/ticket`: ask for one ticket covering the nits
    /// held back, or record the one that was filed.
    ///
    /// **Review Desk does not file Linear tickets**, for the same reason it does not post
    /// to GitHub: it owns the record, not the outward action.
    fn request_nit_ticket(&self, review_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: RequestNitTicketRequest = if body.is_empty() {
            RequestNitTicketRequest::default()
        } else {
            read_body(body)?
        };

        self.records.review(review_id)?;

        // Checked before the write: the contract gives this operation a 409 for a review
        // that holds back no nits, and asking for a ticket covering nothing would record
        // a request nobody can fill.
        if self.records.held_back_findings(review_id)?.is_empty() {
            return Err(ApiError::conflict(format!(
                "review {review_id} holds back no nits, so there is nothing to file"
            )));
        }

        let ticket_id = trimmed(request.ticket_id);
        let ticket = self
            .records
            .request_nit_ticket(review_id, ticket_id.as_deref())?;
        Ok((200, wire::nit_ticket(&ticket)))
    }

    // -- timings ------------------------------------------------------------------

    /// `GET /timings`: how long every finished review took, in one read.
    ///
    /// **No statistics**: the figures and nothing computed from them. DEV-890 puts averages,
    /// a window and a chart out of scope, and a server that had picked any of them would
    /// have decided somebody else's question.
    ///
    /// Finished means posted. A review still open has a span with no end, so a total for it
    /// would grow every time this was read.
    fn review_timings(&self, query: &str) -> Result<(u16, Value), ApiError> {
        let repo = query_value(query, "repo");
        // Checked, as `listRules` checks the same parameter. An unchecked filter answers a
        // typed repository with an empty list, which reads as "nothing finished there"
        // rather than "there is no such repository".
        if let Some(repo) = &repo {
            if !wire::is_repo_name(repo) {
                return Err(ApiError::invalid_value(
                    "repo",
                    format!("{repo:?} is not a repository"),
                    "a repository as owner/repo",
                ));
            }
        }

        let rows = self
            .records
            .finished_review_timings(repo.as_deref())?
            .iter()
            .map(|(review, timings)| {
                json!({
                    "review": wire::review(review),
                    "timings": wire::review_timings(timings),
                    "layers": timings
                        .layers
                        .iter()
                        .map(wire::review_layer)
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();

        Ok((200, json!({ "reviews": rows })))
    }

    // -- rules --------------------------------------------------------------------

    /// `GET /rules`: rules for a repository, by stage, with the week's figures.
    fn rules(&self, query: &str) -> Result<(u16, Value), ApiError> {
        let repo = repo_filter(query)?;

        // Unlike `rules list` on the command line, this includes proposals and retired
        // rules: the screen's whole purpose is deciding on them.
        let rules = self.records.rules_at_stages(
            repo.as_deref(),
            &[
                RuleStage::Proposed,
                RuleStage::Active,
                RuleStage::Lint,
                RuleStage::Ci,
            ],
        )?;
        let summary = self
            .records
            .rules_summary(repo.as_deref(), RULES_WINDOW_DAYS)?;

        let at = now();
        let mut proposed = Vec::new();
        let mut active = Vec::new();
        let mut stale = Vec::new();

        for rule in &rules {
            let counts = self.records.rule_counts(rule.id)?;
            let document = self.with_origin_pr(wire::rule(rule, &counts, at))?;

            match rule.stage {
                RuleStage::Proposed => proposed.push(document),
                // A stale rule is still in force, which is why it is its own group
                // rather than a stage: the developer decides whether to retire it.
                _ if wire::is_stale(rule, at) => stale.push(document),
                RuleStage::Retired => {}
                _ => active.push(document),
            }
        }

        Ok((
            200,
            json!({
                "repo": repo,
                "summary": wire::rules_summary(&summary),
                "proposed": proposed,
                "active": active,
                "stale": stale,
            }),
        ))
    }

    /// Fill in `origin_pr`, which is the pull request the rule's origin review was of.
    ///
    /// `wire::rule` cannot do it: it would need to read another review, and the wire
    /// module does not reach the store.
    fn with_origin_pr(&self, mut document: Value) -> Result<Value, ApiError> {
        if let Some(origin) = document["origin_review_id"].as_i64() {
            // A rule can outlive the review it came out of, so a missing review is a
            // null rather than a failure.
            if let Ok(review) = self.records.review(origin) {
                document["origin_pr"] = json!(review.pr);
            }
        }
        Ok(document)
    }

    /// One rule, as the four write operations return it.
    fn rule_document(&self, rule_id: i64) -> Result<Value, ApiError> {
        let rule = self.records.rule(rule_id)?;
        let counts = self.records.rule_counts(rule_id)?;
        self.with_origin_pr(wire::rule(&rule, &counts, now()))
    }

    /// `POST /rules/{ruleId}/promote`: move a rule one stage along.
    ///
    /// **The developer promotes; the app never promotes a rule itself.** `to_stage` is
    /// stated by the caller rather than inferred, so an accidental double click cannot
    /// skip a stage silently — and this handler is what makes that true, because the
    /// store takes any stage and has no opinion about order.
    fn promote_rule(&self, rule_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: PromoteRuleRequest = read_body(body)?;
        let to_stage: RuleStage = request.to_stage.parse().map_err(ApiError::from)?;

        let rule = self.records.rule(rule_id)?;
        let next = next_stage(rule.stage);

        if next != Some(to_stage) {
            return Err(ApiError::conflict(format!(
                "a rule at {} cannot be promoted to {to_stage}: {}",
                rule.stage,
                match next {
                    Some(next) => format!("the stage that follows is {next}"),
                    None => "there is no stage after it".to_string(),
                }
            )));
        }

        // No reason: promoting carries none, and the store's reason replaces the rule's
        // note, so passing one would overwrite a line that still applies.
        self.records.set_rule_stage(rule_id, to_stage, None)?;
        Ok((200, self.rule_document(rule_id)?))
    }

    /// `POST /rules/{ruleId}/dismiss`: reject a proposal, with the reason.
    ///
    /// The rule moves to `retired` rather than being deleted, so the harvester does not
    /// propose it again next week with no memory of having been told no.
    fn dismiss_rule(&self, rule_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        self.retire_with_reason(rule_id, body, "dismissed")
    }

    /// `POST /rules/{ruleId}/retire`: retire a rule that has gone stale.
    fn retire_rule(&self, rule_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        self.retire_with_reason(rule_id, body, "retired")
    }

    fn retire_with_reason(
        &self,
        rule_id: i64,
        body: &[u8],
        what: &str,
    ) -> Result<(u16, Value), ApiError> {
        let request: RuleReasonRequest = read_body(body)?;
        let reason = trimmed(Some(request.reason)).ok_or_else(|| {
            ApiError::invalid_value(
                "reason",
                format!("a rule {what} with no reason gets proposed again next week"),
                "why",
            )
        })?;

        self.records.rule(rule_id)?;
        self.records
            .set_rule_stage(rule_id, RuleStage::Retired, Some(&reason))?;
        Ok((200, self.rule_document(rule_id)?))
    }

    /// `POST /rules/{ruleId}/defer`: keep a stale rule for a further period.
    ///
    /// This is what takes a rule out of the stale group without pretending a finding
    /// cited it.
    fn defer_rule(&self, rule_id: i64, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: DeferRuleRequest = read_body(body)?;

        if !(1..=365).contains(&request.days) {
            return Err(ApiError::invalid_value(
                "days",
                format!("{} is not a period to keep a rule for", request.days),
                "a whole number of days from 1 to 365",
            ));
        }

        self.records.rule(rule_id)?;
        // The store holds a time, not a count of days: turning "60 more days" into one is
        // the caller's arithmetic, and this is the caller.
        self.records
            .defer_rule(rule_id, now() + request.days * wire::DAY)?;
        Ok((200, self.rule_document(rule_id)?))
    }
}

/// The stage that follows this one, or `None` at the end of the line.
///
/// A rule graduates observation to repository rule to lint check to CI check. `retired`
/// is not on that line: it is reached by dismissing or retiring, which are their own
/// operations and carry a reason.
fn next_stage(stage: RuleStage) -> Option<RuleStage> {
    match stage {
        RuleStage::Proposed => Some(RuleStage::Active),
        RuleStage::Active => Some(RuleStage::Lint),
        RuleStage::Lint => Some(RuleStage::Ci),
        RuleStage::Ci | RuleStage::Retired => None,
    }
}

/// The `repo=` filter, normalised, or a refusal naming the field.
///
/// **One reader, three callers**: `GET /reviews`, `GET /rules` and
/// `GET /duplicate-rounds`. The normalising is load-bearing rather than cosmetic — the
/// store keys on the canonical spelling, so a filter passed through as typed answers
/// "nothing here" about a repository that is there, and `GET /reviews` echoes the value
/// back for the page's own picker. Lifted out of `reviews`, whose comment carries the
/// measurement; `rules` had the same fifteen lines copied, which is how two readers of one
/// rule drift.
fn repo_filter(query: &str) -> Result<Option<String>, ApiError> {
    let repo = query_value(query, "repo").map(|repo| {
        if wire::is_repo_name(&repo) {
            review_desk_store::normalised_repo(&repo)
        } else {
            repo
        }
    });
    if let Some(repo) = &repo {
        if !wire::is_repo_name(repo) {
            return Err(ApiError::invalid_value(
                "repo",
                format!("{repo:?} is not a repository"),
                "a repository as owner/repo",
            ));
        }
    }
    Ok(repo)
}

/// A value from a query string, or `None`. An empty value is `None`: `?repo=` is the
/// picker's "All repositories", not a repository with no name.
fn query_value(query: &str, wanted: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == wanted)
        .map(|(_, value)| percent_decode(value))
        .filter(|value| !value.is_empty())
}

/// Percent-decoding, and `+` for a space, for a query string.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    // Not an escape after all. Kept as written rather than dropped, so a
                    // stray `%` in a value cannot silently change it.
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// A string with its edges trimmed, or `None` when nothing is left.
fn trimmed(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// `StartReviewRequest` from the contract.
///
/// Unknown fields are refused, because the contract sets `additionalProperties: false` on
/// every request body and because a misspelled `head_sha` would otherwise be a GitHub
/// read the caller thought they had saved.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartReviewRequest {
    pr_url: String,
    #[serde(default)]
    head_sha: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    ticket_id: Option<String>,
    /// Where that ticket is. `https` or the store refuses the call; nothing here fetches
    /// it and nothing here derives it from `ticket_id`.
    #[serde(default)]
    ticket_url: Option<String>,
    #[serde(default)]
    changed_lines: Option<i64>,
}

/// `DismissReviewRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DismissReviewRequest {
    reason: String,
}

/// `SetFindingDispositionRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetFindingDispositionRequest {
    disposition: String,
    #[serde(default)]
    reason: Option<String>,
}

/// `SaveDraftRequest` from the contract.
///
/// `body` is optional **when `sections` are given**, because the body is then what they
/// compose to; sent as well it is checked against the composition and refused when the two
/// disagree. Sent on its own it is stored as one block of prose, which is the honest
/// reading of a draft nobody wrote in parts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveDraftRequest {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    sections: Option<Vec<SaveDraftSection>>,
    #[serde(default)]
    edits: Vec<SaveDraftEdit>,
}

/// One block of a draft body, as a caller writes it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveDraftSection {
    kind: DraftSectionKind,
    #[serde(default)]
    finding_seq: Option<i64>,
    #[serde(default)]
    title: Option<String>,
    text: String,
}

impl SaveDraftSection {
    /// The store's shape. The two are separate types for the reason every other request
    /// body here is: the contract's shape is the contract's, and a store type that gained
    /// a field would otherwise widen the API without anybody deciding to.
    fn stored(&self) -> NewDraftSection {
        NewDraftSection {
            kind: self.kind,
            finding_seq: self.finding_seq,
            title: self.title.clone(),
            text: self.text.clone(),
        }
    }
}

/// `DecideFindingRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideFindingRequest {
    decision: String,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// `TicketRestAndApproveRequest` from the contract. One optional field, and so is the body.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketRestAndApproveRequest {
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveDraftEdit {
    finding_seq: i64,
    kind: OutcomeKind,
    summary: String,
}

/// `ApproveDraftRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApproveDraftRequest {
    event: String,
}

/// `SendBackDraftRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SendBackDraftRequest {
    note: String,
}

/// `RequestNitTicketRequest` from the contract. Every field optional, and so is the body.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestNitTicketRequest {
    #[serde(default)]
    ticket_id: Option<String>,
}

/// `PromoteRuleRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromoteRuleRequest {
    to_stage: String,
}

/// `RuleReasonRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleReasonRequest {
    reason: String,
}

/// `DeferRuleRequest` from the contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeferRuleRequest {
    days: i64,
}

/// Read a request body, as the one refusal shape.
fn read_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    if body.is_empty() {
        return Err(ApiError::bad_request(
            "this operation needs a JSON body and none was sent",
        ));
    }

    serde_json::from_slice(body).map_err(|error| ApiError {
        code: Code::BadRequest,
        message: format!("the request body is not the shape this operation takes: {error}"),
        field: None,
        expected: None,
    })
}
