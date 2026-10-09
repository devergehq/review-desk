//! The commits between two rounds of one pull request.
//!
//! "What changed since round 1" is a list of commits with a one-line summary and the files
//! each touched. GitHub answers it in two reads, and neither is a write:
//!
//! | Read | Gives |
//! | -- | -- |
//! | `repos/OWNER/REPO/compare/BASE...HEAD` | the commits, in order, with their messages |
//! | `repos/OWNER/REPO/commits/SHA` | one commit's files |
//!
//! The second is per commit, because a comparison does not carry per-commit files — its
//! `files` are the whole range's. A round with three new commits is four reads.
//!
//! **A commit whose files cannot be read is kept, with no files.** Losing the commit would
//! take a line off "what changed since round 1"; losing its file list costs the `late` flag
//! on findings in that file, which is reported as unanswerable rather than as "not late".

use review_desk_store::NewRoundCommit;

use crate::error::Result;
use crate::github::GitHub;

/// The commits between `base` and `head`, oldest first, ready to record against a round.
///
/// Returns an empty list when the two commits are the same, which is what GitHub answers
/// for a round opened on a head the previous round already reviewed.
pub fn commits_between(
    github: &dyn GitHub,
    repo: &str,
    base: &str,
    head: &str,
) -> Result<Vec<NewRoundCommit>> {
    let comparison = github.compare(repo, base, head)?;

    let mut commits = Vec::with_capacity(comparison.commits.len());
    for commit in comparison.commits {
        // A commit GitHub sent no message for keeps its sha as its summary rather than
        // being dropped or stored blank: the store refuses an empty summary, and a commit
        // missing from the list reads as a commit that is not in the pull request.
        let summary = commit.summary.unwrap_or_else(|| commit.sha.clone());

        // One read per commit, and a failure costs the file list rather than the run. The
        // files are what the `late` flag is answered from, and `late` is already
        // three-valued for exactly this case.
        let files = github
            .commit(repo, &commit.sha)
            .map(|fetched| fetched.paths)
            .unwrap_or_default();

        commits.push(NewRoundCommit {
            sha: commit.sha,
            summary,
            committed_at: commit.committed_at,
            files,
        });
    }

    Ok(commits)
}
