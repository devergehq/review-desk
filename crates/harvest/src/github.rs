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
///
/// Four of these six facts are the harvester's: a review it posted was merged or closed,
/// at some time, on some head, by some author. `title` and `changed_lines` are not — they
/// are read by the local server when it starts a review from a pasted link, which is the
/// only thing the dashboard can give it.
///
/// **They are here rather than in a second trait method because they arrive in the same
/// response.** `gh api repos/OWNER/REPO/pulls/N` carries all six, so a separate method
/// would be a second identical request to read two more fields off it. `contract/README.md`
/// item S22 said this read already supplied them; it did not, and DEV-862 is where it
/// started to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub state: PrState,
    /// When it was merged or closed. `None` while it is open.
    pub state_at: Option<Timestamp>,
    /// The head commit the pull request is at now, which may be past the one reviewed.
    pub head_sha: String,
    /// Who opened it. Used only to tell the author's reply from anyone else's.
    pub author: Option<String>,
    /// The pull request's title, as GitHub has it.
    ///
    /// `None` when the response carried none. A pull request always has a title, so this
    /// is nullable for the narrower reason that a recorded response captured before
    /// DEV-862 does not hold one — and an absent title is a fact to record as absent
    /// rather than a run to fail.
    pub title: Option<String>,
    /// Additions plus deletions, as GitHub counts them.
    ///
    /// `None` when the response carried neither figure. Summed here rather than kept
    /// apart because that is the one number the record holds — `review.changed_lines` —
    /// and the draft's word budget is computed from it.
    pub changed_lines: Option<i64>,
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
    /// The commit message's first line, as GitHub sent it.
    ///
    /// `None` when the response carried no message. A commit always has one, so this is
    /// nullable for the narrower reason `PullRequest::title` is: a response captured
    /// before DEV-873 read it does not hold one, and an absent summary is a fact to
    /// record as absent rather than a run to fail.
    pub summary: Option<String>,
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

/// The reads Review Desk makes of GitHub. Four, all read-only.
///
/// **There is no write here and there will not be one.** "Review Desk never posts to
/// GitHub" is a property of this trait rather than a promise in a comment: the harvester
/// reads back what a review already did, and the local server reads a pull request to
/// start a review from a link. Everything outward — posting the review, asking for a
/// reviewer, filing a ticket — belongs to whatever drives the session.
pub trait GitHub {
    /// The pull request's state, when it reached it, where its head is now, and the title
    /// and changed-line count the local server records when it starts a review.
    fn pull_request(&self, repo: &str, pr: i64) -> Result<PullRequest>;

    /// Every review comment thread on the pull request.
    fn review_threads(&self, repo: &str, pr: i64) -> Result<Vec<ReviewThread>>;

    /// What changed between two commits.
    fn compare(&self, repo: &str, base: &str, head: &str) -> Result<Comparison>;

    /// One commit's files.
    fn commit(&self, repo: &str, sha: &str) -> Result<CommitFiles>;
}
