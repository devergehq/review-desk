//! The one error shape, and which HTTP status each code is.
//!
//! The contract has a single `Error` object with a machine-readable `code`, and says a
//! caller branches on the code and never on the message. So the code is what this type
//! carries and the status is derived from it, rather than the two being chosen
//! separately at each call site and drifting apart.
//!
//! One code is new in this change: `not_implemented`. DEV-861 asks for it, and
//! `contract/openapi.yaml` gains it in the same pull request — an endpoint that says it
//! is not built has to say so in a word the client already knows how to branch on.

use review_desk_harvest::HarvestError;
use review_desk_store::StoreError;
use serde_json::{json, Value};

/// The contract's `ErrorCode`, and nothing outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    BadRequest,
    InvalidValue,
    NotFound,
    Conflict,
    GithubUnavailable,
    StoreUnavailable,
    NotImplemented,
}

impl Code {
    /// The wire string, exactly as the contract's enum spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BadRequest => "bad_request",
            Code::InvalidValue => "invalid_value",
            Code::NotFound => "not_found",
            Code::Conflict => "conflict",
            Code::GithubUnavailable => "github_unavailable",
            Code::StoreUnavailable => "store_unavailable",
            Code::NotImplemented => "not_implemented",
        }
    }

    /// The HTTP status this code is served as.
    ///
    /// Derived rather than passed in: `store_unavailable` must be tellable from a
    /// refusal without reading the message, and that guarantee is worth nothing if one
    /// handler can serve the same code as 500 and another as 400.
    pub fn status(self) -> u16 {
        match self {
            Code::BadRequest | Code::InvalidValue => 400,
            Code::NotFound => 404,
            Code::Conflict => 409,
            Code::StoreUnavailable => 500,
            Code::NotImplemented => 501,
            Code::GithubUnavailable => 502,
        }
    }
}

/// A refusal, or Review Desk being unable to run.
#[derive(Debug)]
pub struct ApiError {
    pub code: Code,
    pub message: String,
    /// The field at fault, when one value caused the refusal.
    pub field: Option<String>,
    /// What would have been accepted. The contract requires it on `invalid_value`: a
    /// caller that rejects input has to say what it would have taken.
    pub expected: Option<String>,
}

impl ApiError {
    fn new(code: Code, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            field: None,
            expected: None,
        }
    }

    /// The request was understood and refused.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(Code::BadRequest, message)
    }

    /// One value was not acceptable. `expected` is not optional here by convention:
    /// the contract says `invalid_value` names what it would have accepted.
    pub fn invalid_value(
        field: impl Into<String>,
        message: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        ApiError {
            field: Some(field.into()),
            expected: Some(expected.into()),
            ..Self::new(Code::InvalidValue, message)
        }
    }

    /// No such review, finding, draft or rule.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(Code::NotFound, message)
    }

    /// The record is not in a state the request can be performed against.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(Code::Conflict, message)
    }

    /// A read of GitHub that Review Desk needed could not be made. The developer's
    /// environment, not their request.
    pub fn github_unavailable(message: impl Into<String>) -> Self {
        Self::new(Code::GithubUnavailable, message)
    }

    /// The store on `master` cannot answer this in full. `waits_on` names the item in
    /// `contract/README.md` that would let it, so a reader of the response does not
    /// have to go and work out which one.
    pub fn not_implemented(what: &str, waits_on: &str) -> Self {
        Self::new(
            Code::NotImplemented,
            format!("{what} is not built yet: it waits on {waits_on}."),
        )
    }

    /// Name the field at fault.
    pub fn about(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    /// The contract's `Error` object.
    pub fn document(&self) -> Value {
        let mut error = json!({ "code": self.code.as_str(), "message": self.message });
        // `field` and `expected` are `[string, 'null']` in the contract, so a null is a
        // valid answer, but writing them only when they say something keeps the body to
        // what is actually known.
        if let Some(field) = &self.field {
            error["field"] = json!(field);
        }
        if let Some(expected) = &self.expected {
            error["expected"] = json!(expected);
        }
        error
    }

    /// The HTTP status to serve this as.
    pub fn status(&self) -> u16 {
        self.code.status()
    }
}

/// The store's failures, mapped to the contract's codes.
///
/// The split is the store's own `is_refusal`, not a second opinion about it: the command
/// line already turns that boolean into exit 1 or exit 2, and the contract says
/// `store_unavailable` is the HTTP equivalent of exit 2. Reproducing the judgement here
/// would let the two surfaces disagree about the same failure.
impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        let message = error.to_string();
        match &error {
            StoreError::NotFound { .. } | StoreError::UnknownParent { .. } => {
                ApiError::not_found(message)
            }
            StoreError::DuplicateReview { .. } | StoreError::DuplicateGithubComment { .. } => {
                ApiError::conflict(message)
            }
            StoreError::InvalidValue {
                field, expected, ..
            } => ApiError::invalid_value(*field, message, expected.clone()),
            _ if !error.is_refusal() => ApiError::new(Code::StoreUnavailable, message),
            _ => ApiError::bad_request(message),
        }
    }
}

/// The harvester's failures, mapped to the contract's codes.
///
/// Everything GitHub-shaped is `github_unavailable`: an absent `gh`, a logged-out `gh`,
/// a pull request that is not there. None of those is a refusal of the request, and the
/// contract keeps the code separate from `store_unavailable` precisely so the screen can
/// say which of the two it was.
impl From<HarvestError> for ApiError {
    fn from(error: HarvestError) -> Self {
        match error {
            HarvestError::Store(error) => error.into(),
            other => ApiError::github_unavailable(other.to_string()),
        }
    }
}
