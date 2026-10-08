//! The harvester's error type.
//!
//! The same split the store makes: a request understood and refused is not the same as
//! Review Desk being unable to run, and a caller must be able to tell them apart
//! without reading message text. Everything GitHub-shaped is "could not run" — an
//! absent `gh`, a logged-out `gh`, a pull request that is no longer there. None of
//! those is a refusal, because the request was fine and it was the world that was not.

use std::path::PathBuf;

use review_desk_store::StoreError;

/// Everything harvesting can fail with.
#[derive(Debug, thiserror::Error)]
pub enum HarvestError {
    /// The `gh` command is not installed, or is not on the path.
    #[error(
        "the gh command is not installed, or is not on PATH; \
             Review Desk reads GitHub by running it"
    )]
    GhMissing,

    /// `gh` is installed but nobody is logged in.
    #[error("gh is installed but not logged in; run `gh auth login`")]
    GhNotAuthenticated,

    /// `gh` ran and failed for some other reason.
    #[error("gh {args} failed ({status}): {stderr}")]
    GhFailed {
        args: String,
        status: String,
        stderr: String,
    },

    /// A response could not be read as the shape it was expected to be.
    #[error("cannot read the {what} response from GitHub: {source}")]
    Unreadable {
        what: &'static str,
        #[source]
        source: serde_json::Error,
    },

    /// A recorded response was asked for that the recording does not hold.
    #[error("no recorded response for {key}")]
    NotRecorded { key: String },

    /// A recorded response could not be read off disk.
    #[error("cannot read the recorded response at {path}: {source}")]
    Recording {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The store refused, or broke.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl HarvestError {
    /// True when the request was understood and refused, rather than Review Desk being
    /// unable to run. Exit 1 for the first, 2 for the second.
    pub fn is_refusal(&self) -> bool {
        match self {
            HarvestError::Store(error) => error.is_refusal(),
            _ => false,
        }
    }

    /// A short machine-readable code, for a `--json` caller that must not parse prose.
    pub fn code(&self) -> &'static str {
        match self {
            HarvestError::GhMissing => "gh_missing",
            HarvestError::GhNotAuthenticated => "gh_not_authenticated",
            HarvestError::GhFailed { .. } => "gh_failed",
            HarvestError::Unreadable { .. } => "github_response_unreadable",
            HarvestError::NotRecorded { .. } => "not_recorded",
            HarvestError::Recording { .. } => "recording_unreadable",
            // The store names its own refusals, and the command line's JSON contract
            // documents those names. A second vocabulary for the same failure would
            // make `harvest --review 99` report a different code from
            // `review show --review 99`.
            HarvestError::Store(error) => error.code(),
        }
    }
}

/// The harvester's result alias.
pub type Result<T> = std::result::Result<T, HarvestError>;
