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

    /// A draft is not at a status the change asked for can be made from.
    ///
    /// Its own variant rather than an `InvalidValue`, because nothing the caller typed
    /// was invalid: the request was understood and the record is simply not in a state
    /// that allows it. A caller branching on `code` can tell the two apart.
    #[error("the draft of review {review_id} is {found}, and {wanted}")]
    DraftStatus {
        review_id: i64,
        found: crate::model::DraftStatus,
        wanted: &'static str,
    },

    /// A decision was made about a finding the draft is not composed of.
    ///
    /// Its own variant, because it is the one refusal a screen has something to say about:
    /// the draft was written as one block of markdown, so there is no paragraph of it that
    /// is F6's and nothing can cut one without guessing which characters to remove. A
    /// session that writes the draft in parts gets the buttons. Every draft written before
    /// migration 9 is in this state.
    #[error(
        "the draft of review {review_id} has no block for F{seq}, so there is nothing to \
         decide about: it was written as one block of markdown rather than in parts"
    )]
    NoDraftSection { review_id: i64, seq: i64 },

    /// A review is already dismissed, so a second dismissal would overwrite the time and
    /// the words of the one the developer actually made.
    ///
    /// **No time in the message.** The store holds times as integers and the server
    /// converts them at its own boundary, so a timestamp here would reach the developer
    /// as `1791780818`. When it was dismissed is on the review, which is what the page
    /// re-reads.
    #[error(
        "review {review_id} is already dismissed; restore it first to dismiss it again \
         with different words"
    )]
    AlreadyDismissed { review_id: i64 },

    /// A review that is not dismissed cannot be brought back.
    #[error("review {review_id} is not dismissed, so there is nothing to bring back")]
    NotDismissed { review_id: i64 },

    /// A review holds work a delete would lose.
    #[error(
        "review {review_id} holds {holds}, and deleting it would lose that: dismiss it \
         instead, which takes it out of the queue and keeps the record"
    )]
    ReviewStarted { review_id: i64, holds: String },

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
            StoreError::DuplicateGithubComment { .. } => "duplicate_github_comment",
            StoreError::DraftStatus { .. } => "draft_status",
            StoreError::NoDraftSection { .. } => "no_draft_section",
            StoreError::AlreadyDismissed { .. } => "already_dismissed",
            StoreError::NotDismissed { .. } => "not_dismissed",
            StoreError::ReviewStarted { .. } => "review_started",
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
                | StoreError::DraftStatus { .. }
                | StoreError::NoDraftSection { .. }
                | StoreError::AlreadyDismissed { .. }
                | StoreError::NotDismissed { .. }
                | StoreError::ReviewStarted { .. }
                | StoreError::NotFound { .. }
                | StoreError::UnknownParent { .. }
                | StoreError::InvalidValue { .. }
        )
    }
}
