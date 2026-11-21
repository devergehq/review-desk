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

    /// GitHub holds more commits between two rounds than it will list in one comparison.
    ///
    /// Its own cap is 250. Recording the 250 it sent would make "what changed since round
    /// 1" a truncated list that nothing says is truncated, and the `late` flag is computed
    /// from exactly that list — so a missing commit becomes a false accusation on the
    /// page. Loud rather than silent, with the number and the way out in the message.
    #[error(
        "{repo}: {total} commits between {base} and {head}, and GitHub lists only {listed} \
         in one comparison; narrow the range with --base"
    )]
    RangeTooLong {
        repo: String,
        base: String,
        head: String,
        total: i64,
        listed: usize,
    },

    /// The store refused, or broke.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl HarvestError {
    /// The HTTP status `gh` reported for a failed call, when it reported one.
    ///
    /// `gh api` ends its error with `(HTTP 422)` or starts it with `HTTP 422:`, depending on
    /// the version. Read for `review-desk post` (DEV-1347), which must tell a refusal GitHub
    /// made (nothing was created) from a call whose outcome is unknown.
    pub fn http_status(&self) -> Option<u16> {
        let HarvestError::GhFailed { stderr, .. } = self else {
            return None;
        };
        let at = stderr.find("HTTP ")? + "HTTP ".len();
        stderr.get(at..at + 3)?.parse().ok()
    }

    /// Whether GitHub certainly created nothing: `gh` never ran, nobody was logged in, or
    /// GitHub answered with a 4xx that is a refusal. **Anything else may have created the
    /// review** — a 5xx, a timeout, a connection dropped after the request went out — and
    /// is not this.
    pub fn certainly_not_sent(&self) -> bool {
        match self {
            HarvestError::GhMissing | HarvestError::GhNotAuthenticated => true,
            // 408 is the server giving up on a request it may have started on.
            HarvestError::GhFailed { .. } => {
                matches!(self.http_status(), Some(status) if (400..500).contains(&status) && status != 408)
            }
            _ => false,
        }
    }

    /// True when the request was understood and refused, rather than Review Desk being
    /// unable to run. Exit 1 for the first, 2 for the second.
    pub fn is_refusal(&self) -> bool {
        match self {
            HarvestError::Store(error) => error.is_refusal(),
            _ => false,
        }
    }

    /// Whether GitHub answered that the thing asked for is not there. DEV-1344.
    ///
    /// A comparison from a commit a force-push took away is a 404, and that is an answer
    /// about the history — not a reason to stop the run.
    pub fn is_not_found(&self) -> bool {
        match self {
            HarvestError::GhFailed { stderr, .. } => {
                stderr.contains("HTTP 404") || stderr.contains("Not Found")
            }
            _ => false,
        }
    }

    /// Why a code-change answer is unknown, when this failure leaves one read unanswered
    /// rather than the whole run unable to go on. DEV-1344.
    ///
    /// `None` for the failures that stop the run as they always have: no `gh`, nobody
    /// logged in, the store broken. Every read after one of those would fail the same way,
    /// so recording "unknown" per finding would bury the one message that says what to fix.
    pub fn unknown_reason(&self) -> Option<&'static str> {
        match self {
            HarvestError::GhFailed { stderr, .. } => {
                let stderr = stderr.to_ascii_lowercase();
                if stderr.contains("rate limit") || stderr.contains("http 429") {
                    Some("rate_limited")
                } else if stderr.contains("http 401") || stderr.contains("http 403") {
                    Some("auth_failed")
                } else {
                    Some("read_failed")
                }
            }
            HarvestError::Unreadable { .. } => Some("response_unreadable"),
            HarvestError::NotRecorded { .. } | HarvestError::Recording { .. } => {
                Some("read_failed")
            }
            HarvestError::GhMissing
            | HarvestError::GhNotAuthenticated
            | HarvestError::RangeTooLong { .. }
            | HarvestError::Store(_) => None,
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
            HarvestError::RangeTooLong { .. } => "commit_range_too_long",
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
