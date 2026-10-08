//! The JSON API: one function from a request to a status and a body.
//!
//! [`Api::answer`] is deliberately a pure function of the request and the two things the
//! server owns — the store and the GitHub reader. It does no HTTP, which is what lets the
//! conformance test call it with a request and validate what comes back against
//! `contract/openapi.yaml`, and lets `lib.rs` be the only place that knows about sockets.
//!
//! Three operations are answered here. The other thirteen return 501 with the store item
//! they wait on; the table that decides which is which is [`crate::route::Operation`].
//!
//! **Every refusal is computed before any write.** A validation that runs after the store
//! has been changed gives the caller an error for a request that succeeded, which is the
//! one failure mode a record-keeping API cannot have.

use review_desk_store::Disposition;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{ApiError, Code};
use crate::github::Github;
use crate::route::{match_route, Operation, Target};
use crate::store::Records;
use crate::{assets, wire};

/// The path every operation in the contract sits under, from the contract's `servers`.
pub const API_PREFIX: &str = "/api/v1";

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

    /// Answer one request. `path` is the part after [`API_PREFIX`].
    pub fn answer(&self, method: &str, path: &str, body: &[u8]) -> (u16, Value) {
        let answered = match match_route(method, path) {
            Target::Health => self.health(),
            Target::Operation(operation) => self.operation(operation, body),
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

    fn operation(&self, operation: Operation, body: &[u8]) -> Result<(u16, Value), ApiError> {
        match operation {
            Operation::StartReview => self.start_review(body),
            Operation::ConfirmBrief { review_id } => self.confirm_brief(review_id),
            Operation::SetFindingDisposition { review_id, seq } => {
                self.set_finding_disposition(review_id, seq, body)
            }
            waiting => Err(waiting.not_implemented()),
        }
    }

    /// `POST /reviews`: start a review from a pull request link.
    ///
    /// The order of the checks is the point of this handler, and DEV-861 states it: the
    /// request is checked in full before anything reaches for GitHub, so a machine with no
    /// `gh` reports the real problem. A malformed link is a `400`, not a report that `gh`
    /// is missing.
    fn start_review(&self, body: &[u8]) -> Result<(u16, Value), ApiError> {
        let request: StartReviewRequest = read_body(body)?;

        // The store has no column for any of these three — S5, S6 and S7. Accepting them
        // and dropping them would hand back a review that silently does not hold what was
        // sent, so the operation refuses as not built for that field instead.
        for (field, supplied) in [
            ("title", request.title.is_some()),
            ("ticket_id", request.ticket_id.is_some()),
            ("changed_lines", request.changed_lines.is_some()),
        ] {
            if supplied {
                return Err(ApiError::not_implemented(
                    &format!("storing {field} on a review"),
                    "S5, S6 and S7 (review.title, review.ticket_id, review.changed_lines); \
                     omit the field and the review is created without it",
                )
                .about(field));
            }
        }

        let link = wire::parse_pr_link(&request.pr_url)?;

        let head_sha = match request.head_sha {
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
                head_sha
            }
            // Omitted, which is what the Inbox screen does: it has only a pasted link.
            // The head is resolved before the review is created, because the store
            // deduplicates on it and a headless review could not be matched on the next
            // click. If the read fails, nothing is created.
            None => self.github.head_sha(&link.repo, link.pr)?,
        };

        let opened = self.records.open_review(&link.repo, link.pr, &head_sha)?;
        Ok((201, wire::review(&opened.review)))
    }

    /// `POST /reviews/{reviewId}/brief/confirm`: the developer's act at layer 2.
    ///
    /// **A sign-off endpoint.** Only the developer can sign off — Patrick, 9 October 2026
    /// — and this API is the only surface where it happens; the command line agents use
    /// has no command for it. What records the actor is a store column DEV-860 adds; see
    /// `store::Records::confirm_brief`.
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

        let reason = request
            .reason
            .map(|reason| reason.trim().to_string())
            .filter(|reason| !reason.is_empty());

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
