//! The contract's twenty-one operations, as a type.
//!
//! [`Operation::ALL`] is the point of this module: a test walks every operation the
//! contract declares and asserts that each one is routed and answers, rather than the set
//! of routes being whatever the handlers happened to grow. An operation added to
//! `openapi.yaml` and not to the server fails a test instead of 404ing in a browser.
//!
//! **That claim was false until DEV-930 and is now true.** `tests/operations.rs` asserted
//! `ALL.len() == 20` against a literal and read no file, so nothing anywhere tied these
//! variants to `openapi.yaml`'s `paths` — an operation could be added to the contract,
//! given an example, made producible from the store, and left unrouted, with every Rust
//! test green and the dashboard getting a 400. `the_contract_and_this_type_name_the_same_
//! operations` reads the contract's `operationId`s and asserts set equality with these.
//!
//! This file briefly carried a second job. DEV-861 was written while a sibling issue was
//! still changing the store, and thirteen operations could not be answered in full from
//! what `master` held; each named the store item it was waiting for and returned 501.
//! That sibling merged before this pull request opened, so every operation is answered
//! and the mechanism is gone rather than left behind returning `None` for everything.

/// One of the contract's operations, with its path parameters read out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    GetInbox,
    ListRepos,
    ListReviews,
    StartReview,
    GetReview { review_id: i64 },
    ConfirmBrief { review_id: i64 },
    GetProblemStatement { review_id: i64 },
    SetFindingDisposition { review_id: i64, seq: i64 },
    DecideFinding { review_id: i64, seq: i64 },
    GetDraft { review_id: i64 },
    SaveDraft { review_id: i64 },
    ApproveDraft { review_id: i64 },
    SendBackDraft { review_id: i64 },
    RequestNitTicket { review_id: i64 },
    TicketRestAndApprove { review_id: i64 },
    ListReviewTimings,
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
        Operation::ListReviews,
        Operation::StartReview,
        Operation::GetReview { review_id: 1 },
        Operation::ConfirmBrief { review_id: 1 },
        Operation::GetProblemStatement { review_id: 1 },
        Operation::SetFindingDisposition {
            review_id: 1,
            seq: 1,
        },
        Operation::DecideFinding {
            review_id: 1,
            seq: 1,
        },
        Operation::GetDraft { review_id: 1 },
        Operation::SaveDraft { review_id: 1 },
        Operation::ApproveDraft { review_id: 1 },
        Operation::SendBackDraft { review_id: 1 },
        Operation::RequestNitTicket { review_id: 1 },
        Operation::TicketRestAndApprove { review_id: 1 },
        Operation::ListReviewTimings,
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
            Operation::ListReviews => "listReviews",
            Operation::StartReview => "startReview",
            Operation::GetReview { .. } => "getReview",
            Operation::ConfirmBrief { .. } => "confirmBrief",
            Operation::GetProblemStatement { .. } => "getProblemStatement",
            Operation::SetFindingDisposition { .. } => "setFindingDisposition",
            Operation::DecideFinding { .. } => "decideFinding",
            Operation::GetDraft { .. } => "getDraft",
            Operation::SaveDraft { .. } => "saveDraft",
            Operation::ApproveDraft { .. } => "approveDraft",
            Operation::SendBackDraft { .. } => "sendBackDraft",
            Operation::RequestNitTicket { .. } => "requestNitTicket",
            Operation::TicketRestAndApprove { .. } => "ticketRestAndApprove",
            Operation::ListReviewTimings => "listReviewTimings",
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
            Operation::ListReviews => "GET /reviews".to_string(),
            Operation::StartReview => "POST /reviews".to_string(),
            Operation::GetReview { review_id } => format!("GET /reviews/{review_id}"),
            Operation::ConfirmBrief { review_id } => {
                format!("POST /reviews/{review_id}/brief/confirm")
            }
            Operation::GetProblemStatement { review_id } => {
                format!("GET /reviews/{review_id}/brief/problem-statement")
            }
            Operation::SetFindingDisposition { review_id, seq } => {
                format!("PATCH /reviews/{review_id}/findings/{seq}")
            }
            Operation::DecideFinding { review_id, seq } => {
                format!("POST /reviews/{review_id}/findings/{seq}/decide")
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
            Operation::TicketRestAndApprove { review_id } => {
                format!("POST /reviews/{review_id}/draft/ticket-and-approve")
            }
            Operation::ListReviewTimings => "GET /timings".to_string(),
            Operation::ListRules => "GET /rules".to_string(),
            Operation::PromoteRule { rule_id } => format!("POST /rules/{rule_id}/promote"),
            Operation::DismissRule { rule_id } => format!("POST /rules/{rule_id}/dismiss"),
            Operation::RetireRule { rule_id } => format!("POST /rules/{rule_id}/retire"),
            Operation::DeferRule { rule_id } => format!("POST /rules/{rule_id}/defer"),
        }
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

    // A path shape that exists is separated from one that does not, so "no such
    // endpoint" and "not with that verb" are different things to tell a client. Both are
    // refusals the contract has a code for — `not_found` and `bad_request` — and neither
    // is a 405: the contract declares no 405 and `ErrorCode` has no word for one, so
    // inventing a status would be a worse answer than a shape the client already parses.
    let (target, allowed) = match segments.as_slice() {
        ["health"] => (Target::Health, method == "GET"),
        ["inbox"] => (Target::Operation(Operation::GetInbox), method == "GET"),
        ["repos"] => (Target::Operation(Operation::ListRepos), method == "GET"),
        ["rules"] => (Target::Operation(Operation::ListRules), method == "GET"),
        // **`/timings`, not `/reviews/timings`.** The review paths take an integer id, so
        // a sibling segment there would be read as a review called "timings" and come back
        // as `Unknown`.
        ["timings"] => (
            Target::Operation(Operation::ListReviewTimings),
            method == "GET",
        ),
        // **Two methods on one path**, the shape `/reviews/{id}/draft` below already has:
        // reading the list and starting a review are different operations at the same
        // address. The fall-through carries `StartReview` because `Target::WrongMethod`
        // discards the operation, so which of the two it names is immaterial — what
        // matters is that a third verb is a wrong verb and not an unknown path.
        ["reviews"] => match method {
            "GET" => (Target::Operation(Operation::ListReviews), true),
            "POST" => (Target::Operation(Operation::StartReview), true),
            _ => (Target::Operation(Operation::StartReview), false),
        },

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
        ["reviews", id, "brief", "problem-statement"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::GetProblemStatement { review_id }),
                method == "GET",
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
        ["reviews", id, "findings", seq, "decide"] => match (number(id), number(seq)) {
            (Some(review_id), Some(seq)) => (
                Target::Operation(Operation::DecideFinding { review_id, seq }),
                method == "POST",
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
        ["reviews", id, "draft", "ticket-and-approve"] => match number(id) {
            Some(review_id) => (
                Target::Operation(Operation::TicketRestAndApprove { review_id }),
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
