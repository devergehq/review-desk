//! What the command line can fail with, and which exit code that is.
//!
//! The store already separates a refused request from a broken Review Desk. This adds
//! the one failure the store cannot have an opinion on: input the caller supplied that
//! could not be used. A `--file` that is not there is the caller's mistake, not Review
//! Desk being unable to run, so it is a refusal.

use std::fmt;

use review_desk_store::StoreError;
use serde_json::{json, Value};

/// The command line's result.
pub type Result<T> = std::result::Result<T, CliError>;

/// Everything a command can fail with.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// The store refused the request, or could not run.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Input the caller supplied could not be used: an unreadable file, a document that
    /// is not JSON, a field that is not understood, or a flag the command needs.
    #[error("{what}: {detail}")]
    BadInput { what: String, detail: String },

    /// A record on the database could not be turned into JSON. Nothing a caller did
    /// causes this, so it is Review Desk being broken rather than a refusal.
    #[error("cannot turn the record into JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl CliError {
    /// A bad-input refusal, with what was being read and why it could not be.
    pub fn bad_input(what: impl Into<String>, detail: impl fmt::Display) -> Self {
        CliError::BadInput {
            what: what.into(),
            detail: detail.to_string(),
        }
    }

    /// Add what the document could have said, for a refusal over a JSON document.
    ///
    /// serde names a closed set's values when the value is an unknown string, but not
    /// when it is the wrong type, and not when the field is missing. The command line
    /// has to name them in every case, so it appends them itself rather than relying on
    /// which shape the mistake took.
    pub fn naming(self, values: impl fmt::Display) -> Self {
        match self {
            CliError::BadInput { what, detail } => CliError::BadInput {
                what,
                detail: format!("{detail} ({values})"),
            },
            other => other,
        }
    }

    /// A short, stable name for this failure, for a caller that branches on the reason.
    pub fn code(&self) -> &'static str {
        match self {
            CliError::Store(error) => error.code(),
            CliError::BadInput { .. } => "bad_input",
            CliError::Json(_) => "cannot_serialise",
        }
    }

    /// True when the request was understood and refused, rather than Review Desk being
    /// unable to run. Refused is exit 1, broken is exit 2.
    pub fn is_refusal(&self) -> bool {
        match self {
            CliError::Store(error) => error.is_refusal(),
            CliError::BadInput { .. } => true,
            CliError::Json(_) => false,
        }
    }

    /// This failure as the one JSON document `--json` promises, on any exit code.
    pub fn document(&self) -> Value {
        error_document(self.code(), &self.to_string(), self.is_refusal())
    }
}

/// The shape every failure takes with `--json`: a machine-readable code, a message for
/// a person, and which of the two exit codes it is, so a caller reading only standard
/// output still knows.
pub fn error_document(code: &str, message: &str, refused: bool) -> Value {
    json!({
        "error": {
            "code": code,
            "message": message,
            "kind": if refused { "refused" } else { "broken" },
        }
    })
}
