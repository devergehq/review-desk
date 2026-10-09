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
    Actor, Disposition, DraftStatus, NewDraft, NewOutcome, OutcomeKind, ReviewEvent, ReviewFacts,
    RuleStage,
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
            Operation::StartReview => self.start_review(body),
            Operation::GetReview { review_id } => self.review(review_id),
            Operation::ConfirmBrief { review_id } => self.confirm_brief(review_id),
            Operation::SetFindingDisposition { review_id, seq } => {
                self.set_finding_disposition(review_id, seq, body)
            }
            Operation::GetDraft { review_id } => self.draft(review_id),
            Operation::SaveDraft { review_id } => self.save_draft(review_id, body),
            Operation::ApproveDraft { review_id } => self.approve_draft(review_id, body),
            Operation::SendBackDraft { review_id } => self.send_back_draft(review_id, body),
            Operation::RequestNitTicket { review_id } => self.request_nit_ticket(review_id, body),
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
        let proposed = self.records.rules_at_stages(None, &[RuleStage::Proposed])?;

        let briefs_to_confirm = briefs
            .iter()
            .map(|review| -> Result<Value, ApiError> {
                let layers = self.records.review_layers(review.id)?;
                let at_two = layers.iter().find(|layer| layer.number == 2);
                Ok(json!({
                    "review_id": review.id,
                    "pull_request": wire::inbox_pr(review),
                    "ticket_id": review.ticket_id,
                    "brief_ready_at": wire::rfc3339_or_null(at_two.and_then(|l| l.updated_at)),
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
                    "round": review.round,
                    "severities": wire::severity_counts(&in_draft),
                    "held_back": held_back,
                    // "delta since your last review" is what a round above one means.
                    "is_delta": review.round > 1,
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let author_replies = replies
            .iter()
            .map(|review| -> Result<Option<Value>, ApiError> {
                let events: Vec<Value> = self
                    .records
                    .full_review(review.id)?
                    .outcomes
                    .iter()
                    .filter(|outcome| matches!(outcome.actor, Actor::Author | Actor::Harvester))
                    .map(|outcome| {
                        json!({
                            "finding_seq": outcome.finding_seq,
                            "finding_key": wire::finding_key(outcome.finding_seq),
                            "kind": outcome.kind.as_str(),
                            "detail": outcome.detail,
                        })
                    })
                    .collect();

                // `events` has minItems 1, so a review with none is not an item. The
                // store's query should not return one; serving an invalid item if it did
                // would be worse than serving a shorter list.
                Ok((!events.is_empty()).then(|| {
                    json!({
                        "review_id": review.id,
                        "pull_request": wire::inbox_pr(review),
                        "events": events,
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
                    "layer_number": at.layer.number,
                    "layer_name": at.layer.name.as_str(),
                    "layers_total": 7,
                    "session_id": at.review.session_id,
                    // Allele owns session liveness and nothing tells this server about
                    // it. Null is the honest answer, and the contract requires the screen
                    // to render it rather than showing "session alive" by default.
                    "session_alive": Value::Null,
                })
            })
            .collect();

        let since = now() - WINDOW_DAYS * wire::DAY;
        let filed = self.records.filed_nit_tickets(since)?;
        let nits_filed: i64 = filed
            .iter()
            .map(|ticket| ticket.finding_seqs.len() as i64)
            .sum();

        // `own_prs_without_reviewer` is empty, and that is the contract's own position:
        // it is GitHub state, nothing lists the developer's own pull requests, and no
        // issue covers it. Empty reads as absent, which is true.
        let totals_items = briefs_to_confirm.len()
            + drafts_to_approve.len()
            + author_replies.len()
            + proposed_rules.len();
        let repos: std::collections::BTreeSet<&str> = briefs
            .iter()
            .chain(drafts.iter())
            .chain(replies.iter())
            .map(|review| review.repo.as_str())
            .collect();

        Ok((
            200,
            json!({
                "totals": { "items": totals_items, "repos": repos.len() },
                "briefs_to_confirm": briefs_to_confirm,
                "drafts_to_approve": drafts_to_approve,
                "author_replies": author_replies,
                "own_prs_without_reviewer": Value::Array(Vec::new()),
                "proposed_rules": proposed_rules,
                "in_progress": in_progress,
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
        let gaps = ReviewFacts {
            title: resolved
                .as_ref()
                .and_then(|read| read.title.clone())
                .filter(|_| opened.review.title.is_none()),
            ticket_id: None,
            changed_lines: resolved
                .as_ref()
                .and_then(|read| read.changed_lines)
                .filter(|_| opened.review.changed_lines.is_none()),
            not_checked: None,
        };

        let review = if gaps.is_empty() {
            opened.review
        } else {
            self.records.set_review_facts(opened.review.id, &gaps)?
        };

        Ok((201, wire::review(&review)))
    }

    /// `GET /reviews/{reviewId}`: one review whole, as the One review screen shows it.
    fn review(&self, review_id: i64) -> Result<(u16, Value), ApiError> {
        let full = self.records.full_review(review_id)?;
        let options = self.records.brief_options(review_id)?;
        let ticket = self.records.nit_ticket(review_id)?;

        Ok((
            200,
            json!({
                "review": wire::review(&full.review),
                "layers": full.layers.iter().map(wire::review_layer).collect::<Vec<_>>(),
                "brief": full
                    .brief
                    .as_ref()
                    .map_or(Value::Null, |brief| wire::brief(brief, &options)),
                "findings": full.findings.iter().map(wire::finding).collect::<Vec<_>>(),
                "outcomes": full.outcomes.iter().map(wire::outcome).collect::<Vec<_>>(),
                "finding_severities": wire::severity_counts(&full.findings),
                "finding_dispositions": wire::disposition_counts(&full.findings),
                "not_checked": full.review.not_checked,
                "nit_ticket": wire::nit_ticket(&ticket),
                "draft_status": full
                    .draft
                    .as_ref()
                    .map_or(Value::Null, |draft| json!(draft.status.as_str())),
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
        let options = self.records.brief_options(review_id)?;
        Ok((200, wire::brief(&brief, &options)))
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
            .filter(|outcome| {
                matches!(
                    outcome.kind,
                    OutcomeKind::Cut | OutcomeKind::Regraded | OutcomeKind::Rewritten
                )
            })
            .map(wire::draft_edit)
            .collect();

        let held_back: Vec<Value> = findings
            .iter()
            .filter(|finding| finding.disposition == Disposition::HeldBack)
            .map(wire::held_back_finding)
            .collect();

        Ok(json!({
            "draft": wire::draft(draft),
            "review": wire::review(&review),
            "lint": wire::draft_lint(&draft.lint),
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
                body: request.body,
                status: match existing.status {
                    DraftStatus::Ready => DraftStatus::Ready,
                    _ => DraftStatus::Drafting,
                },
                // The body changed, so whatever the lint said about the old one is no
                // longer about this draft. Cleared rather than kept: a stale lint result
                // shown against a new body is worse than no result.
                lint: None,
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
        self.draft_must_be(review_id, DraftStatus::Ready, "sent back")?;

        let sent_back = self.records.send_back_draft(review_id, &note)?;
        Ok((200, self.draft_detail(&sent_back)?))
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

    // -- rules --------------------------------------------------------------------

    /// `GET /rules`: rules for a repository, by stage, with the week's figures.
    fn rules(&self, query: &str) -> Result<(u16, Value), ApiError> {
        let repo = query_value(query, "repo");
        if let Some(repo) = &repo {
            if !wire::is_repo_name(repo) {
                return Err(ApiError::invalid_value(
                    "repo",
                    format!("{repo:?} is not a repository"),
                    "a repository as owner/repo",
                ));
            }
        }

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
    #[serde(default)]
    changed_lines: Option<i64>,
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
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveDraftRequest {
    body: String,
    #[serde(default)]
    edits: Vec<SaveDraftEdit>,
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
