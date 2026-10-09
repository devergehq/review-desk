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
//! **A read that fails fails the whole call**, and that is a correction. It used to keep
//! the commit with an empty file list, on the argument that losing the commit would take a
//! line off "what changed since round 1". That was wrong in a way the file list makes
//! dangerous: `Finding.late` is computed from exactly these paths, so a commit recorded
//! with no files turns a transient GitHub failure into "the previous round could have
//! caught this and did not". `review-desk round commits` replaces what it recorded, so
//! failing leaves the previous answer intact and the operator runs it again.

use review_desk_store::NewRoundCommit;

use crate::error::{HarvestError, Result};
use crate::github::GitHub;

/// The commits between `base` and `head`, oldest first, ready to record against a round.
///
/// Returns an empty list when the two commits are the same, which is what GitHub answers
/// for a round opened on a head the previous round already reviewed.
///
/// Refused when GitHub holds more commits in the range than it lists in one comparison —
/// its cap is 250. A truncated list that nothing says is truncated is the same class of
/// defect as a commit with no files.
pub fn commits_between(
    github: &dyn GitHub,
    repo: &str,
    base: &str,
    head: &str,
) -> Result<Vec<NewRoundCommit>> {
    let comparison = github.compare(repo, base, head)?;

    if let Some(total) = comparison.total_commits {
        if total > comparison.commits.len() as i64 {
            return Err(HarvestError::RangeTooLong {
                repo: repo.to_string(),
                base: base.to_string(),
                head: head.to_string(),
                total,
                listed: comparison.commits.len(),
            });
        }
    }

    let mut commits = Vec::with_capacity(comparison.commits.len());
    for commit in comparison.commits {
        // A commit GitHub sent no message for keeps its sha as its summary rather than
        // being dropped or stored blank: the store refuses an empty summary, and a commit
        // missing from the list reads as a commit that is not in the pull request. A
        // message is cosmetic; the file list is not, which is why only one of the two is
        // allowed to be missing.
        let summary = commit.summary.unwrap_or_else(|| commit.sha.clone());

        // One read per commit, and a failure fails the call. See the module note.
        let files = github.commit(repo, &commit.sha)?.paths;

        commits.push(NewRoundCommit {
            sha: commit.sha,
            summary,
            committed_at: commit.committed_at,
            files,
        });
    }

    Ok(commits)
}
