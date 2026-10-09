//! GitHub's JSON, turned into the facts the harvester records.
//!
//! **Both the live path and the tests parse through here.** [`crate::gh::Gh`] hands
//! these functions what `gh` printed; [`crate::recorded::Recorded`] hands them a file
//! captured from a real pull request. That is what makes the recorded tests worth
//! something: they exercise the same parsing the live run does, so a response shape
//! this module reads wrongly fails a test rather than being quietly re-encoded into
//! whatever the test expected.

use serde::Deserialize;

use review_desk_store::{PrState, Timestamp};

use crate::error::{HarvestError, Result};
use crate::github::{
    ChangedFile, Commit, CommitFiles, Comparison, PullRequest, Reply, ReviewThread,
};
use crate::patch::changed_lines;

/// Read `gh api repos/OWNER/REPO/pulls/N`.
pub fn pull_request(json: &str) -> Result<PullRequest> {
    let wire: WirePullRequest = read(json, "pull request")?;

    let state = if wire.merged.unwrap_or(false) || wire.merged_at.is_some() {
        PrState::Merged
    } else if wire.state.eq_ignore_ascii_case("closed") {
        PrState::Closed
    } else {
        PrState::Open
    };

    // GitHub's `state` is only "open" or "closed"; merged is a closed pull request with
    // a merge time. Reading merged as closed would lose the one distinction the brief
    // asks for by name.
    let state_at = match state {
        PrState::Merged => wire.merged_at.as_deref().and_then(timestamp),
        PrState::Closed => wire.closed_at.as_deref().and_then(timestamp),
        PrState::Open => None,
    };

    // Additions plus deletions, and only when GitHub sent both. A response carrying one
    // of the two would make the sum a half-measurement, and `review.changed_lines` is
    // shown as a figure — "214 changed lines" — so half of one is worse than none.
    let changed_lines = match (wire.additions, wire.deletions) {
        (Some(additions), Some(deletions)) => Some(additions + deletions),
        _ => None,
    };

    Ok(PullRequest {
        state,
        state_at,
        head_sha: wire.head.sha,
        author: wire.user.and_then(|user| user.login),
        // Trimmed, and an empty title is no title: it would otherwise be shown as a
        // review with a blank name rather than one with none.
        title: wire
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty()),
        changed_lines,
    })
}

/// Read the GraphQL response for a pull request's review threads.
///
/// The resolved flag lives only in GraphQL: the REST comments endpoint does not carry
/// it, which is why this one read is not `gh api repos/...`.
pub fn review_threads(json: &str) -> Result<Vec<ReviewThread>> {
    let wire: WireGraphQl = read(json, "review threads")?;
    let threads = wire.data.repository.pull_request.review_threads;

    Ok(threads
        .nodes
        .into_iter()
        .filter_map(|thread| {
            let mut comments = thread.comments.nodes.into_iter();
            // A thread with no comments cannot be a finding's thread.
            let root = comments.next()?;

            Some(ReviewThread {
                root_comment_id: root.database_id?.to_string(),
                resolved: thread.is_resolved,
                resolved_by: thread.resolved_by.and_then(|user| user.login),
                replies: comments
                    .map(|comment| Reply {
                        id: comment
                            .database_id
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                        author: comment.author.and_then(|user| user.login),
                        body: comment.body,
                        created_at: comment.created_at.as_deref().and_then(timestamp),
                    })
                    .collect(),
            })
        })
        .collect())
}

/// Read `gh api repos/OWNER/REPO/compare/BASE...HEAD`.
pub fn comparison(json: &str) -> Result<Comparison> {
    let wire: WireComparison = read(json, "comparison")?;

    Ok(Comparison {
        status: wire.status,
        commits: wire
            .commits
            .into_iter()
            .map(|commit| Commit {
                committed_at: commit.committed_at(),
                summary: commit.summary(),
                sha: commit.sha,
            })
            .collect(),
        files: wire
            .files
            .into_iter()
            .map(|file| ChangedFile {
                path: file.filename,
                status: file.status,
                // A file with no patch — a binary, or one too large for GitHub to send
                // — changed, but says nothing about which lines. That is the
                // "only the file changed" case, not a reason to fail the run.
                changed_lines: file.patch.as_deref().map(changed_lines).unwrap_or_default(),
            })
            .collect(),
    })
}

/// Read `gh api repos/OWNER/REPO/commits/SHA`.
pub fn commit(json: &str) -> Result<CommitFiles> {
    let wire: WireCommit = read(json, "commit")?;

    Ok(CommitFiles {
        committed_at: wire.committed_at(),
        sha: wire.sha,
        paths: wire
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|file| file.filename)
            .collect(),
    })
}

/// Seconds since the epoch from an ISO 8601 instant like `2026-09-30T10:54:42Z`.
///
/// GitHub emits UTC to the second and nothing else, so this reads that and declines
/// anything it does not recognise rather than guessing. A time it cannot read costs the
/// outcome its timestamp, not the run.
pub fn timestamp(text: &str) -> Option<Timestamp> {
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;

    let time = time.trim_end_matches('Z');
    let time = time.split_once('.').map_or(time, |(seconds, _)| seconds);
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time.next().unwrap_or("0").parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days from 1970-01-01 to a civil date. Howard Hinnant's algorithm, which is exact
/// over the whole proleptic Gregorian calendar and needs no date library.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Parse, naming what failed to parse. A GitHub response the harvester cannot read is
/// Review Desk unable to run, not a refusal.
fn read<T: for<'de> Deserialize<'de>>(json: &str, what: &'static str) -> Result<T> {
    serde_json::from_str(json).map_err(|source| HarvestError::Unreadable { what, source })
}

/// Every field but `state` and `head` is optional, which is what lets a response captured
/// before a field was read still parse. `head` is not: a pull request with no head commit
/// is not one this crate can say anything about.
#[derive(Debug, Deserialize)]
struct WirePullRequest {
    state: String,
    merged: Option<bool>,
    merged_at: Option<String>,
    closed_at: Option<String>,
    head: WireRef,
    user: Option<WireUser>,
    title: Option<String>,
    additions: Option<i64>,
    deletions: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct WireRef {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct WireUser {
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireGraphQl {
    data: WireGraphQlData,
}

#[derive(Debug, Deserialize)]
struct WireGraphQlData {
    repository: WireRepository,
}

#[derive(Debug, Deserialize)]
struct WireRepository {
    #[serde(rename = "pullRequest")]
    pull_request: WireThreadedPullRequest,
}

#[derive(Debug, Deserialize)]
struct WireThreadedPullRequest {
    #[serde(rename = "reviewThreads")]
    review_threads: WireNodes<WireThread>,
}

#[derive(Debug, Deserialize)]
struct WireNodes<T> {
    nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct WireThread {
    #[serde(rename = "isResolved")]
    is_resolved: bool,
    #[serde(rename = "resolvedBy")]
    resolved_by: Option<WireUser>,
    comments: WireNodes<WireComment>,
}

#[derive(Debug, Deserialize)]
struct WireComment {
    #[serde(rename = "databaseId")]
    database_id: Option<i64>,
    author: Option<WireUser>,
    body: String,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireComparison {
    status: String,
    #[serde(default)]
    commits: Vec<WireCommit>,
    #[serde(default)]
    files: Vec<WireFile>,
}

#[derive(Debug, Deserialize)]
struct WireCommit {
    sha: String,
    commit: Option<WireCommitDetail>,
    files: Option<Vec<WireFile>>,
}

impl WireCommit {
    /// The commit message's first line, trimmed. A blank message is no summary.
    fn summary(&self) -> Option<String> {
        self.commit
            .as_ref()?
            .message
            .as_deref()?
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
    }

    fn committed_at(&self) -> Option<Timestamp> {
        self.commit
            .as_ref()?
            .committer
            .as_ref()?
            .date
            .as_deref()
            .and_then(timestamp)
    }
}

#[derive(Debug, Deserialize)]
struct WireCommitDetail {
    committer: Option<WireCommitter>,
    /// The whole message. Only its first line is kept.
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireCommitter {
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireFile {
    filename: String,
    #[serde(default)]
    status: String,
    patch: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_and_a_known_instant() {
        assert_eq!(timestamp("1970-01-01T00:00:00Z"), Some(0));
        // 2026-09-30T10:54:42Z, when devergehq/locus#64 was merged.
        assert_eq!(timestamp("2026-09-30T10:54:42Z"), Some(1_790_765_682));
        assert_eq!(timestamp("2000-02-29T12:00:00Z"), Some(951_825_600));
    }

    #[test]
    fn a_time_that_is_not_an_instant_is_declined_not_guessed() {
        assert_eq!(timestamp(""), None);
        assert_eq!(timestamp("yesterday"), None);
        assert_eq!(timestamp("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn a_merged_pull_request_is_merged_not_closed() {
        let json = r#"{"state":"closed","merged":true,"merged_at":"2026-09-30T10:54:42Z",
                       "closed_at":"2026-09-30T10:54:42Z","head":{"sha":"abc"},
                       "user":{"login":"someone"}}"#;

        let pr = pull_request(json).unwrap();
        assert_eq!(pr.state, PrState::Merged);
        assert_eq!(pr.state_at, Some(1_790_765_682));
        assert_eq!(pr.author.as_deref(), Some("someone"));
    }

    #[test]
    fn the_title_and_the_changed_lines_are_read_as_github_sent_them() {
        // The fields as `gh api repos/devergehq/locus/pulls/64` printed them on
        // 9 October 2026, which is also what `tests/fixtures/pull_request.json` holds.
        let json = r#"{"state":"closed","merged":true,
                       "title":"DEV-795: give dispatcher watchers an exit condition, and a way to see the ones that have none",
                       "additions":1097,"deletions":3,
                       "head":{"sha":"46f6ea1645a9bbc9308f791411b2cac1a093367b"},
                       "user":{"login":"patrickdorival"}}"#;

        let pr = pull_request(json).unwrap();
        assert_eq!(
            pr.title.as_deref(),
            Some(
                "DEV-795: give dispatcher watchers an exit condition, \
                 and a way to see the ones that have none"
            )
        );
        // Additions plus deletions, which is the one figure the record holds.
        assert_eq!(pr.changed_lines, Some(1100));
    }

    #[test]
    fn a_response_from_before_these_fields_were_read_still_parses() {
        // A recording captured for the harvester alone carries neither field. It has to
        // keep working: the fields are nullable so that an old recording is a pull
        // request with no title, not an unreadable response.
        let json = r#"{"state":"open","head":{"sha":"abc"},"user":{"login":"someone"}}"#;

        let pr = pull_request(json).unwrap();
        assert_eq!(pr.title, None);
        assert_eq!(pr.changed_lines, None);
    }

    #[test]
    fn a_blank_title_is_no_title_and_half_a_count_is_no_count() {
        let blank = r#"{"state":"open","title":"   ","additions":10,
                        "head":{"sha":"abc"},"user":null}"#;
        let pr = pull_request(blank).unwrap();

        // A review shown with a blank name reads as broken; one shown with no name reads
        // as what it is.
        assert_eq!(pr.title, None);
        // Additions with no deletions is not "10 changed lines".
        assert_eq!(pr.changed_lines, None);
    }

    #[test]
    fn an_open_pull_request_has_no_state_time() {
        let json = r#"{"state":"open","merged":false,"merged_at":null,"closed_at":null,
                       "head":{"sha":"abc"},"user":{"login":"someone"}}"#;

        let pr = pull_request(json).unwrap();
        assert_eq!(pr.state, PrState::Open);
        assert_eq!(pr.state_at, None);
    }

    #[test]
    fn a_closed_but_unmerged_pull_request_is_closed() {
        let json = r#"{"state":"closed","merged":false,"merged_at":null,
                       "closed_at":"2026-09-30T10:54:42Z","head":{"sha":"abc"},"user":null}"#;

        let pr = pull_request(json).unwrap();
        assert_eq!(pr.state, PrState::Closed);
        assert_eq!(pr.state_at, Some(1_790_765_682));
    }

    #[test]
    fn a_response_of_the_wrong_shape_is_unreadable_not_a_refusal() {
        let error = pull_request("{}").expect_err("a pull request needs a head");
        assert!(!error.is_refusal());
        assert_eq!(error.code(), "github_response_unreadable");
    }
}
