//! The contract's sixteen operations, and which of them the store on `master` can answer.
//!
//! The table below is the whole of DEV-861's "which endpoints, now" decision, in one
//! place and under test. Each operation is either implemented or it names the item from
//! `contract/README.md`'s store list that it waits on — and because [`Operation::ALL`]
//! exists, a test can walk every operation and assert that each one answers, rather than
//! the set of routes being whatever the handlers happen to have grown.
//!
//! An operation that cannot be answered **in full** returns 501 `not_implemented`. It
//! does not return the fields it can reach and invent the rest: a response that is
//! partly real is worse than one that says it is not built, because the dashboard cannot
//! tell which half to trust.

use crate::error::ApiError;

/// One of the contract's operations, with its path parameters read out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    GetInbox,
    ListRepos,
    StartReview,
    GetReview { review_id: i64 },
    ConfirmBrief { review_id: i64 },
    SetFindingDisposition { review_id: i64, seq: i64 },
    GetDraft { review_id: i64 },
    SaveDraft { review_id: i64 },
    ApproveDraft { review_id: i64 },
    SendBackDraft { review_id: i64 },
    RequestNitTicket { review_id: i64 },
    ListRules,
    PromoteRule { rule_id: i64 },
    DismissRule { rule_id: i64 },
    RetireRule { rule_id: i64 },
    DeferRule { rule_id: i64 },
}

impl Operation {
    /// Every operation, with placeholder parameters. For a test that walks the contract.
    pub const ALL: &'static [Operation] = &[
        Operation::GetInbox,
        Operation::ListRepos,
        Operation::StartReview,
        Operation::GetReview { review_id: 1 },
        Operation::ConfirmBrief { review_id: 1 },
        Operation::SetFindingDisposition {
            review_id: 1,
            seq: 1,
        },
        Operation::GetDraft { review_id: 1 },
        Operation::SaveDraft { review_id: 1 },
        Operation::ApproveDraft { review_id: 1 },
        Operation::SendBackDraft { review_id: 1 },
        Operation::RequestNitTicket { review_id: 1 },
        Operation::ListRules,
        Operation::PromoteRule { rule_id: 1 },
        Operation::DismissRule { rule_id: 1 },
        Operation::RetireRule { rule_id: 1 },
        Operation::DeferRule { rule_id: 1 },
    ];

    /// The `operationId` from `contract/openapi.yaml`.
    pub fn id(self) -> &'static str {
        match self {
            Operation::GetInbox => "getInbox",
            Operation::ListRepos => "listRepos",
            Operation::StartReview => "startReview",
            Operation::GetReview { .. } => "getReview",
            Operation::ConfirmBrief { .. } => "confirmBrief",
            Operation::SetFindingDisposition { .. } => "setFindingDisposition",
            Operation::GetDraft { .. } => "getDraft",
            Operation::SaveDraft { .. } => "saveDraft",
            Operation::ApproveDraft { .. } => "approveDraft",
            Operation::SendBackDraft { .. } => "sendBackDraft",
            Operation::RequestNitTicket { .. } => "requestNitTicket",
            Operation::ListRules => "listRules",
            Operation::PromoteRule { .. } => "promoteRule",
            Operation::DismissRule { .. } => "dismissRule",
            Operation::RetireRule { .. } => "retireRule",
            Operation::DeferRule { .. } => "deferRule",
        }
    }

    /// The method and path this operation is reached at, for a test and for the 501's
    /// message.
    pub fn route(self) -> String {
        match self {
            Operation::GetInbox => "GET /inbox".to_string(),
            Operation::ListRepos => "GET /repos".to_string(),
            Operation::StartReview => "POST /reviews".to_string(),
            Operation::GetReview { review_id } => format!("GET /reviews/{review_id}"),
            Operation::ConfirmBrief { review_id } => {
                format!("POST /reviews/{review_id}/brief/confirm")
            }
            Operation::SetFindingDisposition { review_id, seq } => {
                format!("PATCH /reviews/{review_id}/findings/{seq}")
            }
            Operation::GetDraft { review_id } => format!("GET /reviews/{review_id}/draft"),
            Operation::SaveDraft { review_id } => format!("PUT /reviews/{review_id}/draft"),
            Operation::ApproveDraft { review_id } => {
                format!("POST /reviews/{review_id}/draft/approve")
            }
            Operation::SendBackDraft { review_id } => {
                format!("POST /reviews/{review_id}/draft/send-back")
            }
            Operation::RequestNitTicket { review_id } => {
                format!("POST /reviews/{review_id}/held-back/ticket")
            }
            Operation::ListRules => "GET /rules".to_string(),
            Operation::PromoteRule { rule_id } => format!("POST /rules/{rule_id}/promote"),
            Operation::DismissRule { rule_id } => format!("POST /rules/{rule_id}/dismiss"),
            Operation::RetireRule { rule_id } => format!("POST /rules/{rule_id}/retire"),
            Operation::DeferRule { rule_id } => format!("POST /rules/{rule_id}/defer"),
        }
    }

    /// What the store on `master` is missing before this operation can be answered in
    /// full, or `None` when it can be.
    ///
    /// Each entry names the item from the "Store changes this contract needs" list in
    /// `contract/README.md`, and says which **required** response field is unanswerable —
    /// a nullable field with no column is served as null, which the contract calls the
    /// honest answer, so it is never a reason to refuse.
    pub fn waits_on(self) -> Option<&'static str> {
        match self {
            // Implemented.
            Operation::StartReview
            | Operation::ConfirmBrief { .. }
            | Operation::SetFindingDisposition { .. } => None,

            // `Inbox.drafts_to_approve` needs the draft table, `in_progress` needs the
            // per-layer rows, and every group needs reviews across all repositories —
            // the store's review queries are all scoped to one pull request.
            Operation::GetInbox => {
                Some("S1 (draft), S2 (review_layer), S17 (cross-repository review queries)")
            }

            // Distinct repositories with their open-review and active-rule counts.
            Operation::ListRepos => Some("S18 (the repository list)"),

            // `ReviewDetail.layers` is required and must hold exactly seven rows;
            // `nit_ticket` is required and has no columns.
            Operation::GetReview { .. } => {
                Some("S2 (review_layer), S9 (the review's nit-ticket columns)")
            }

            // There is no draft table at all.
            Operation::GetDraft { .. }
            | Operation::SaveDraft { .. }
            | Operation::ApproveDraft { .. }
            | Operation::SendBackDraft { .. } => Some("S1 (the draft table)"),

            // The ticket's status, id and times live on the review, and do not exist.
            Operation::RequestNitTicket { .. } => Some("S9 (the review's nit-ticket columns)"),

            // `Rule.stale` is a required, non-nullable boolean derived from
            // `last_cited_at` and `review_again_at`. Neither column exists, and no value
            // of a boolean means "not measured".
            Operation::ListRules | Operation::PromoteRule { .. } | Operation::DeferRule { .. } => {
                Some("S12 (rule.last_cited_at), S13 (rule.review_again_at)")
            }

            // The same, plus nowhere to record why the rule was dismissed or retired.
            Operation::DismissRule { .. } | Operation::RetireRule { .. } => Some(
                "S12 (rule.last_cited_at), S13 (rule.review_again_at), \
                 S16 (a reason on a rule's stage change)",
            ),
        }
    }

    /// The 501 this operation answers with while it waits.
    pub fn not_implemented(self) -> ApiError {
        let waits_on = self
            .waits_on()
            .expect("not_implemented is only for an operation that waits on the store");
        ApiError::not_implemented(&format!("{} ({})", self.route(), self.id()), waits_on)
    }
}

/// What a request was asking for.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// One of the contract's operations.
    Operation(Operation),
    /// `GET /health`, which is not in the contract: it is the server saying it is up and
    /// its database is open, which a person starting it wants and no screen needs.
    Health,
    /// A path under the API prefix that no operation claims.
    Unknown,
    /// A method this path does have an operation for, but not with this method.
    WrongMethod,
}

/// Match a method and an API path — the part after `/api/v1` — to an operation.
pub fn match_route(method: &str, path: &str) -> Target {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let number = |segment: &str| segment.parse::<i64>().ok().filter(|value| *value >= 1);

    // A path shape that exists is separated from one that does not, so a wrong method
    // answers 405 rather than 404: "no such endpoint" and "not with that verb" are
    // different things to tell a client.
    let (target, allowed) = match segments.as_slice() {
        ["health"] => (Target::Health, method == "GET"),
        ["inbox"] => (Target::Operation(Operation::GetInbox), method == "GET"),
        ["repos"] => (Target::Operation(Operation::ListRepos), method == "GET"),
        ["rules"] => (Target::Operation(Operation::ListRules), method == "GET"),
        ["reviews"] => (Target::Operation(Operation::StartReview), method == "POST"),

        ["reviews", id] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::GetReview { review_id }),
                method == "GET",
            ),
            None => (Target::Unknown, true),
        },
        ["reviews", id, "brief", "confirm"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::ConfirmBrief { review_id }),
                method == "POST",
            ),
            None => (Target::Unknown, true),
        },
        ["reviews", id, "findings", seq] => match (number(id), number(seq)) {
            (Some(review_id), Some(seq)) => (
                Target::Operation(Operation::SetFindingDisposition { review_id, seq }),
                method == "PATCH",
            ),
            _ => (Target::Unknown, true),
        },
        ["reviews", id, "draft"] => match (number(id), method) {
            (Some(review_id), "GET") => {
                (Target::Operation(Operation::GetDraft { review_id }), true)
            }
            (Some(review_id), "PUT") => {
                (Target::Operation(Operation::SaveDraft { review_id }), true)
            }
            (Some(review_id), _) => (Target::Operation(Operation::GetDraft { review_id }), false),
            (None, _) => (Target::Unknown, true),
        },
        ["reviews", id, "draft", "approve"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::ApproveDraft { review_id }),
                method == "POST",
            ),
            None => (Target::Unknown, true),
        },
        ["reviews", id, "draft", "send-back"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::SendBackDraft { review_id }),
                method == "POST",
            ),
            None => (Target::Unknown, true),
        },
        ["reviews", id, "held-back", "ticket"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::RequestNitTicket { review_id }),
                method == "POST",
            ),
            None => (Target::Unknown, true),
        },

        ["rules", id, verb] => match (number(id), *verb) {
            (Some(rule_id), "promote") => (
                Target::Operation(Operation::PromoteRule { rule_id }),
                method == "POST",
            ),
            (Some(rule_id), "dismiss") => (
                Target::Operation(Operation::DismissRule { rule_id }),
                method == "POST",
            ),
            (Some(rule_id), "retire") => (
                Target::Operation(Operation::RetireRule { rule_id }),
                method == "POST",
            ),
            (Some(rule_id), "defer") => (
                Target::Operation(Operation::DeferRule { rule_id }),
                method == "POST",
            ),
            _ => (Target::Unknown, true),
        },

        _ => (Target::Unknown, true),
    };

    match (target, allowed) {
        (Target::Unknown, _) => Target::Unknown,
        (target, true) => target,
        (_, false) => Target::WrongMethod,
    }
}
