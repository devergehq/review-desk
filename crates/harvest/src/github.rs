//! What the harvester needs from GitHub, and nothing more.
//!
//! The trait is deliberately narrow: four reads, no writes. **Review Desk never posts
//! to GitHub from here** — the harvester's whole job is to read back what a review
//! already did.
//!
//! The types are facts, not wire shapes. GitHub's JSON is turned into these in
//! [`crate::wire`], so a change in the API's shape lands in one module and the
//! harvesting logic stays readable.

use std::collections::BTreeSet;

use review_desk_store::{PrState, Timestamp};

use crate::error::Result;

/// What a pull request is, as GitHub has it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub state: PrState,
    /// When it was merged or closed. `None` while it is open.
    pub state_at: Option<Timestamp>,
    /// The head commit the pull request is at now, which may be past the one reviewed.
    pub head_sha: String,
    /// Who opened it. Used only to tell the author's reply from anyone else's.
    pub author: Option<String>,
}

/// One review comment thread, rooted at the comment a finding was posted as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    /// The id of the thread's first comment — the one a finding links to.
    pub root_comment_id: String,
    pub resolved: bool,
    /// Who resolved it, where GitHub says.
    pub resolved_by: Option<String>,
    /// Every comment after the first, oldest first.
    pub replies: Vec<Reply>,
}

/// One reply in a thread, kept exactly as it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub id: String,
    pub author: Option<String>,
    /// The reply, verbatim. The harvester does not summarise, trim or classify it.
    pub body: String,
    pub created_at: Option<Timestamp>,
}

/// Two commits compared: what happened between the commit a review read and the one
/// the pull request is at now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// GitHub's comparison status: `ahead`, `behind`, `identical` or `diverged`.
    pub status: String,
    /// The commits after the base, oldest first.
    pub commits: Vec<Commit>,
    pub files: Vec<ChangedFile>,
}

impl Comparison {
    /// The changed file at this path, if it changed.
    pub fn file(&self, path: &str) -> Option<&ChangedFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// One commit in a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub committed_at: Option<Timestamp>,
}

/// One file that changed between the two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    /// GitHub's word for what happened to it: `modified`, `added`, `removed`, …
    pub status: String,
    /// The line numbers **in the reviewed commit** whose content is gone: removed, or
    /// replaced. A finding pointing at one of these has had its own lines changed; a
    /// finding elsewhere in the file has only had the file change around it.
    pub changed_lines: BTreeSet<i64>,
}

impl ChangedFile {
    /// Whether the content at this line of the reviewed commit is still there.
    ///
    /// A finding with no line number is treated as "the file changed, its lines did
    /// not" — a claim about a whole file has no lines to have changed.
    pub fn touches_line(&self, line: Option<i64>) -> bool {
        line.is_some_and(|line| self.changed_lines.contains(&line))
    }
}

/// One commit's files, used to say which commit changed a finding's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFiles {
    pub sha: String,
    pub committed_at: Option<Timestamp>,
    pub paths: Vec<String>,
}

/// The reads the harvester makes. Four, all read-only.
pub trait GitHub {
    /// The pull request's state, when it reached it, and where its head is now.
    fn pull_request(&self, repo: &str, pr: i64) -> Result<PullRequest>;

    /// Every review comment thread on the pull request.
    fn review_threads(&self, repo: &str, pr: i64) -> Result<Vec<ReviewThread>>;

    /// What changed between two commits.
    fn compare(&self, repo: &str, base: &str, head: &str) -> Result<Comparison>;

    /// One commit's files.
    fn commit(&self, repo: &str, sha: &str) -> Result<CommitFiles>;
}
