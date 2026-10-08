//! GitHub, replayed from responses captured off a real pull request.
//!
//! This is how the harvester's tests run with no network. The responses in
//! `crates/harvest/tests/fixtures` were captured from `devergehq/locus#64` with the
//! `gh` commands listed in that directory's README, and they go through
//! [`crate::wire`] — the same parsing the live run uses. A fake that returned
//! hand-built facts would test the harvesting logic against the harvester's own
//! assumptions; this tests it against what GitHub actually sent.
//!
//! It also counts its calls, so a test can assert that a review was *not* fetched.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{HarvestError, Result};
use crate::github::{CommitFiles, Comparison, GitHub, PullRequest, ReviewThread};
use crate::wire;

/// Recorded GitHub responses, by what was asked for.
#[derive(Debug, Default)]
pub struct Recorded {
    dir: Option<PathBuf>,
    files: HashMap<String, PathBuf>,
    calls: Cell<usize>,
}

impl Recorded {
    /// A recording of nothing. Every read fails, which is what a test asserting "this
    /// review is never fetched" wants.
    pub fn nothing() -> Self {
        Self::default()
    }

    /// A directory holding `pull_request.json`, `review_threads.json`, `compare.json`
    /// and `commit-SHA.json` for the first seven characters of each commit.
    pub fn from_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: Some(dir.into()),
            ..Self::default()
        }
    }

    /// Replace one response, for a case the recording does not hold — a pull request
    /// still open, say.
    pub fn with(mut self, key: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.files.insert(key.into(), path.into());
        self
    }

    /// How many reads have been made. A test that expects none asserts on this.
    pub fn calls(&self) -> usize {
        self.calls.get()
    }

    fn load(&self, key: &str, conventional: &str) -> Result<String> {
        self.calls.set(self.calls.get() + 1);

        let path = self
            .files
            .get(key)
            .cloned()
            .or_else(|| self.dir.as_ref().map(|dir| dir.join(conventional)))
            .ok_or_else(|| HarvestError::NotRecorded {
                key: key.to_string(),
            })?;

        if !path.exists() {
            return Err(HarvestError::NotRecorded {
                key: format!("{key} ({})", path.display()),
            });
        }

        std::fs::read_to_string(&path).map_err(|source| HarvestError::Recording { path, source })
    }
}

impl GitHub for Recorded {
    fn pull_request(&self, _repo: &str, _pr: i64) -> Result<PullRequest> {
        wire::pull_request(&self.load("pull_request", "pull_request.json")?)
    }

    fn review_threads(&self, _repo: &str, _pr: i64) -> Result<Vec<ReviewThread>> {
        wire::review_threads(&self.load("review_threads", "review_threads.json")?)
    }

    fn compare(&self, _repo: &str, _base: &str, _head: &str) -> Result<Comparison> {
        wire::comparison(&self.load("compare", "compare.json")?)
    }

    fn commit(&self, _repo: &str, sha: &str) -> Result<CommitFiles> {
        let short: String = sha.chars().take(7).collect();
        wire::commit(&self.load(&format!("commit:{sha}"), &format!("commit-{short}.json"))?)
    }
}
