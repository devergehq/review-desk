//! The store's error type.
//!
//! Callers must be able to tell a refused request from a broken Review Desk without
//! reading message text, so constraint failures get their own variants rather than
//! arriving as an opaque SQLite error.

use std::path::PathBuf;

/// Everything the store can fail with.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A review already exists for this repository, pull request and round.
    #[error("review already exists for {repo}#{pr} round {round}")]
    DuplicateReview { repo: String, pr: i64, round: i64 },

    /// A GitHub review comment is already linked to a different finding.
    #[error("GitHub comment {comment_id} is already linked to finding {review_id}/{seq}")]
    DuplicateGithubComment {
        comment_id: String,
        review_id: i64,
        seq: i64,
    },

    /// A row was requested, or referenced, that is not there.
    #[error("no {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    /// A row referenced a parent row that does not exist.
    #[error("{entity} refers to {parent} {id}, which does not exist")]
    UnknownParent {
        entity: &'static str,
        parent: &'static str,
        id: String,
    },

    /// A value read from the database is not one this version of the code knows.
    #[error("{field} holds {value:?}, which is not a valid {field}; expected one of: {expected}")]
    InvalidValue {
        field: &'static str,
        value: String,
        expected: String,
    },

    /// The database could not be opened, or its directory could not be created.
    #[error("cannot open database at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A migration failed to apply.
    #[error("migration {version} ({name}) failed: {source}")]
    Migration {
        version: i64,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },

    /// The database is newer than this binary understands.
    #[error("database is at version {found}, but this build only knows up to {known}")]
    FromTheFuture { found: i64, known: i64 },

    /// No home directory could be determined, so the default path is unknowable.
    #[error("cannot determine the home directory; set REVIEW_DESK_DB to choose a database")]
    NoHome,

    /// Anything else SQLite reports.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// The store's result alias.
pub type Result<T> = std::result::Result<T, StoreError>;

impl StoreError {
    /// A short, stable name for this failure, for a caller that must branch on the
    /// reason rather than read the message.
    ///
    /// The exit code already says refused or broken; this says which refusal. It is
    /// part of the command line's JSON contract, so a new variant must be given a name
    /// here rather than falling through to a catch-all.
    pub fn code(&self) -> &'static str {
        match self {
            StoreError::DuplicateReview { .. } => "duplicate_review",
            StoreError::NotFound { .. } => "not_found",
            StoreError::UnknownParent { .. } => "unknown_parent",
            StoreError::InvalidValue { .. } => "invalid_value",
            StoreError::Open { .. } => "cannot_open_database",
            StoreError::Migration { .. } => "migration_failed",
            StoreError::FromTheFuture { .. } => "database_from_the_future",
            StoreError::NoHome => "no_home",
            StoreError::Sqlite(_) => "database_error",
        }
    }

    /// True when the request was understood and refused, rather than Review Desk
    /// itself being broken. DEV-851's exit codes rest on this split: refused is 1,
    /// broken is 2.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            StoreError::DuplicateReview { .. }
                | StoreError::DuplicateGithubComment { .. }
                | StoreError::NotFound { .. }
                | StoreError::UnknownParent { .. }
                | StoreError::InvalidValue { .. }
        )
    }
}
