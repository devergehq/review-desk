//! Reading GitHub, which this server does exactly once and never writes to.
//!
//! One operation reads GitHub: `POST /reviews`, and only to resolve what the caller did
//! not supply — the head commit, the title and the changed-line count, which arrive in one
//! response. The read goes through `crates/harvest`'s `GitHub` trait, which is four
//! read-only methods and has no write in it — so "Review Desk never posts to GitHub" is a
//! property of the type the server depends on, not a promise in a comment.
//!
//! The reader is injected rather than constructed here, which is what lets the tests run
//! with no network: `harvest::recorded::Recorded` replays responses captured off a real
//! pull request and counts its calls, so a test can assert that GitHub was *not* read.
//!
//! It sits behind a mutex because `Recorded` counts calls in a `Cell` and so is `Send`
//! but not `Sync`. Serialising GitHub reads costs this server nothing — there is one
//! developer and one read per started review — and the alternative would be to make the
//! test double thread-safe for the benefit of production code that does not need it.

use std::sync::{Arc, Mutex};

use review_desk_harvest::GitHub;

use crate::error::{ApiError, Code};

/// The GitHub reads this server makes.
#[derive(Clone)]
pub struct Github {
    reader: Arc<Mutex<Box<dyn GitHub + Send>>>,
}

impl std::fmt::Debug for Github {
    // `GitHub` implementors are not required to be `Debug`, and the crate denies missing
    // `Debug` implementations, so this names the type without reaching into it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Github { reader: <dyn GitHub> }")
    }
}

impl Github {
    /// Read GitHub through this reader.
    pub fn new(reader: impl GitHub + Send + 'static) -> Self {
        Github {
            reader: Arc::new(Mutex::new(Box::new(reader))),
        }
    }

    /// What one read of a pull request tells this server.
    ///
    /// Three facts, because they arrive in one response and the screen needs all three: a
    /// review with no head commit cannot be deduplicated, and a review with no title has
    /// nothing for the Inbox to list it under.
    ///
    /// Every failure is `github_unavailable`: `gh` missing, `gh` logged out, the pull
    /// request unreachable. The contract keeps that code apart from `store_unavailable`
    /// because it is the developer's environment rather than their request or a broken
    /// Review Desk, and the screen can then say which it was and offer the link again.
    pub fn pull_request(&self, repo: &str, pr: i64) -> Result<ResolvedPullRequest, ApiError> {
        let reader = self.reader.lock().map_err(|_| ApiError {
            code: Code::StoreUnavailable,
            message: "a failed request left the GitHub reader unusable".to_string(),
            field: None,
            expected: None,
        })?;

        let read = reader.pull_request(repo, pr).map_err(ApiError::from)?;
        Ok(ResolvedPullRequest {
            head_sha: read.head_sha,
            title: read.title,
            changed_lines: read.changed_lines,
        })
    }
}

/// What `POST /reviews` resolves from a pasted link.
///
/// A narrowing of the harvester's `PullRequest`, deliberately: that type also carries the
/// merge state and the author, which this server records nothing about. Naming the three
/// fields it does use keeps the handler from growing a dependency on the other two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPullRequest {
    /// The head commit the review is of.
    pub head_sha: String,
    /// The pull request's title, if GitHub sent one.
    pub title: Option<String>,
    /// Additions plus deletions, if GitHub sent both.
    pub changed_lines: Option<i64>,
}
