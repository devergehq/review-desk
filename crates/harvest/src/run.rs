//! One harvest: which reviews to read, what to record, and what to say about it.
//!
//! The order is fixed, and each step records only what GitHub stated:
//!
//! 1. The pull request's state and the time it reached it, onto the review.
//! 2. Every reply in a linked finding's thread, verbatim.
//! 3. Whether a linked finding's thread is resolved.
//! 4. Whether the file a finding points at changed after the commit the review read,
//!    and whether the finding's own lines changed with it.
//!
//! **Running it twice records nothing twice.** Every row written carries the id of the
//! GitHub fact it came from, and a unique index on that refuses the second write. The
//! harvester does not remember what it has done; the database does.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::json;

use review_desk_store::{
    now, Actor, Finding, FindingRef, NewOutcome, OutcomeKind, PrState, Review, Store, StoreError,
    Timestamp,
};

use crate::error::{HarvestError, Result};
use crate::github::{CommitFiles, GitHub, PullRequest, ReviewThread};

/// Which reviews to harvest.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Only this repository, as `owner/repo`.
    pub repo: Option<String>,
    /// Only this review.
    pub review: Option<i64>,
}

impl Selection {
    /// Every review whose pull request was open at its last harvest.
    pub fn everything() -> Self {
        Self::default()
    }
}

/// What one harvest did.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub reviews: Vec<ReviewReport>,
}

/// What one review's harvest did.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewReport {
    pub review_id: i64,
    pub repo: String,
    pub pr: i64,
    pub round: i64,
    pub pr_state: PrState,
    pub pr_state_at: Option<Timestamp>,
    /// Whether this review's pull request was already merged or closed before this
    /// run. Only a review named with `--review` can be: the sweep leaves them alone.
    pub was_settled: bool,
    /// The commit the review read.
    pub reviewed_head: String,
    /// The commit the pull request is at now.
    pub current_head: String,
    pub findings: usize,
    /// Findings with no linked GitHub comment. They are skipped for replies and for
    /// resolution, and still checked for a later code change.
    pub findings_without_comment: usize,
    /// Findings whose linked comment GitHub no longer has a thread for.
    pub findings_with_missing_thread: usize,
    pub replies_recorded: usize,
    pub threads_resolved_recorded: usize,
    pub code_changed_recorded: usize,
    /// Outcomes GitHub reported that were already in the record. On a second harvest
    /// of the same responses, this is where every one of them lands.
    pub already_recorded: usize,
}

impl Report {
    /// Whether anything at all was recorded.
    pub fn recorded_anything(&self) -> bool {
        self.reviews.iter().any(|review| {
            review.replies_recorded
                + review.threads_resolved_recorded
                + review.code_changed_recorded
                > 0
        })
    }

    /// One line saying what the run did, for a person.
    pub fn summary(&self) -> String {
        let reviews = self.reviews.len();
        let replies: usize = self.reviews.iter().map(|r| r.replies_recorded).sum();
        let resolved: usize = self
            .reviews
            .iter()
            .map(|r| r.threads_resolved_recorded)
            .sum();
        let changed: usize = self.reviews.iter().map(|r| r.code_changed_recorded).sum();
        let already: usize = self.reviews.iter().map(|r| r.already_recorded).sum();
        let unlinked: usize = self
            .reviews
            .iter()
            .map(|r| r.findings_without_comment)
            .sum();

        let mut summary = format!(
            "harvested {reviews} {}: {replies} {}, {resolved} {} resolved, \
             {changed} code changed after",
            plural(reviews, "review", "reviews"),
            plural(replies, "reply", "replies"),
            plural(resolved, "thread", "threads"),
        );
        if already > 0 {
            summary.push_str(&format!("; {already} already recorded"));
        }
        if unlinked > 0 {
            summary.push_str(&format!(
                "; {unlinked} {} no linked comment, skipped for replies and resolution",
                plural(unlinked, "finding has", "findings have"),
            ));
        }
        let missing: usize = self
            .reviews
            .iter()
            .map(|review| review.findings_with_missing_thread)
            .sum();
        if missing > 0 {
            // Worth saying out loud: a linked comment GitHub has no thread for usually
            // means the id is wrong, and a count buried in --json would not be read.
            summary.push_str(&format!(
                "; {missing} linked {} no thread on GitHub",
                plural(missing, "comment has", "comments have"),
            ));
        }
        summary
    }

    /// A line per review, under the summary.
    pub fn lines(&self) -> Vec<String> {
        self.reviews
            .iter()
            .map(|review| {
                let settled = if review.was_settled {
                    " (named explicitly; its pull request was already settled)"
                } else {
                    ""
                };
                format!(
                    "{}#{} round {} (review {}): {}; {} replies, {} resolved, {} code changed, \
                     {} already recorded, {} of {} findings unlinked",
                    review.repo,
                    review.pr,
                    review.round,
                    review.review_id,
                    review.pr_state,
                    review.replies_recorded,
                    review.threads_resolved_recorded,
                    review.code_changed_recorded,
                    review.already_recorded,
                    review.findings_without_comment,
                    review.findings,
                ) + settled
            })
            .collect()
    }
}

/// Read the outcomes of posted reviews back from GitHub and record them.
pub fn harvest(store: &Store, github: &dyn GitHub, selection: Selection) -> Result<Report> {
    let reviews = select(store, &selection)?;

    let mut report = Report {
        reviews: Vec::with_capacity(reviews.len()),
    };

    for review in reviews {
        report.reviews.push(harvest_one(store, github, &review)?);
    }

    Ok(report)
}

/// Which reviews this run covers.
///
/// Without `--review`, the reviews whose pull request was open at their last harvest:
/// a settled pull request has nothing further to say, and fetching it again would cost
/// a GitHub call per run for ever.
///
/// With `--review`, that review, whatever state it is in. A developer naming one review
/// has asked for it, and answering "no" to a direct request while offering no way to
/// override would be worse than the call it saves. The report says it was settled.
fn select(store: &Store, selection: &Selection) -> Result<Vec<Review>> {
    let Some(id) = selection.review else {
        return Ok(store.reviews_to_harvest(selection.repo.as_deref())?);
    };

    // An unknown review id is a refusal, not a run that found nothing.
    let review = store.get_review(id)?;

    if let Some(repo) = &selection.repo {
        if &review.repo != repo {
            return Err(HarvestError::Store(StoreError::InvalidValue {
                field: "repo",
                value: repo.clone(),
                expected: format!("{}, the repository of review {id}", review.repo),
            }));
        }
    }

    Ok(vec![review])
}

/// Harvest one review.
fn harvest_one(store: &Store, github: &dyn GitHub, review: &Review) -> Result<ReviewReport> {
    let pull_request = github.pull_request(&review.repo, review.pr)?;
    store.set_review_pr_state(review.id, pull_request.state, pull_request.state_at)?;

    let findings = store.list_findings(review.id)?;
    let linked: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.github_comment_id.is_some())
        .collect();

    let mut report = ReviewReport {
        review_id: review.id,
        repo: review.repo.clone(),
        pr: review.pr,
        round: review.round,
        pr_state: pull_request.state,
        pr_state_at: pull_request.state_at,
        was_settled: review.is_settled(),
        reviewed_head: review.head_sha.clone(),
        current_head: pull_request.head_sha.clone(),
        findings: findings.len(),
        findings_without_comment: findings.len() - linked.len(),
        findings_with_missing_thread: 0,
        replies_recorded: 0,
        threads_resolved_recorded: 0,
        code_changed_recorded: 0,
        already_recorded: 0,
    };

    if !linked.is_empty() {
        // One read for every thread, rather than one per finding.
        let threads = github.review_threads(&review.repo, review.pr)?;
        let by_root: HashMap<&str, &ReviewThread> = threads
            .iter()
            .map(|thread| (thread.root_comment_id.as_str(), thread))
            .collect();

        for finding in &linked {
            let comment_id = finding.github_comment_id.as_deref().unwrap_or_default();
            let Some(thread) = by_root.get(comment_id) else {
                report.findings_with_missing_thread += 1;
                continue;
            };
            harvest_thread(store, finding, thread, &pull_request, &mut report)?;
        }
    }

    harvest_code_changes(store, github, review, &pull_request, &findings, &mut report)?;

    store.mark_review_harvested(review.id, now())?;
    Ok(report)
}

/// Replies and resolution for one finding's thread.
fn harvest_thread(
    store: &Store,
    finding: &Finding,
    thread: &ReviewThread,
    pull_request: &PullRequest,
    report: &mut ReviewReport,
) -> Result<()> {
    let reference = FindingRef {
        review_id: finding.review_id,
        seq: finding.seq,
    };

    for reply in &thread.replies {
        // The reply, as written. Classifying it as an acceptance or a rejection needs
        // judgement, which this command does not have and must not fake.
        let detail = json!({
            "comment_id": reply.id,
            "in_reply_to": thread.root_comment_id,
            "login": reply.author,
            "body": reply.body,
            "time_basis": "github",
        });

        let written = store.add_harvested_outcome(
            reference,
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: actor_for(reply.author.as_deref(), pull_request.author.as_deref()),
                detail: Some(detail.to_string()),
                occurred_at: reply.created_at,
            },
            &format!("reply:{}", reply.id),
        )?;

        count(
            written.is_some(),
            &mut report.replies_recorded,
            &mut report.already_recorded,
        );
    }

    if thread.resolved {
        // GitHub does not say when a thread was resolved, so the time recorded is when
        // the harvester saw it. `time_basis` says so rather than letting a reader take
        // it for the moment it happened.
        let detail = json!({
            "thread": thread.root_comment_id,
            "resolved_by": thread.resolved_by,
            "time_basis": "observed",
        });

        let written = store.add_harvested_outcome(
            reference,
            NewOutcome {
                kind: OutcomeKind::ThreadResolved,
                actor: actor_for(
                    thread.resolved_by.as_deref(),
                    pull_request.author.as_deref(),
                ),
                detail: Some(detail.to_string()),
                occurred_at: None,
            },
            &format!("thread:{}", thread.root_comment_id),
        )?;

        count(
            written.is_some(),
            &mut report.threads_resolved_recorded,
            &mut report.already_recorded,
        );
    }

    Ok(())
}

/// Whether the files the findings point at changed after the commit the review read.
fn harvest_code_changes(
    store: &Store,
    github: &dyn GitHub,
    review: &Review,
    pull_request: &PullRequest,
    findings: &[Finding],
    report: &mut ReviewReport,
) -> Result<()> {
    if pull_request.head_sha == review.head_sha || pull_request.head_sha.is_empty() {
        return Ok(());
    }
    if !findings.iter().any(|finding| finding.file.is_some()) {
        return Ok(());
    }

    // One comparison, not a diff per commit: see `crate::patch` for why the basis
    // matters. GitHub returns at most 300 files and 250 commits in a comparison, so a
    // pull request larger than that would under-report — a file missing from the
    // response reads as a file that did not change. Nothing here can tell the
    // difference, and paging it is the harvester's next job rather than this one's.
    let comparison = github.compare(&review.repo, &review.head_sha, &pull_request.head_sha)?;
    let basis = format!("compare {}...{}", review.head_sha, pull_request.head_sha);
    let mut commits: HashMap<String, CommitFiles> = HashMap::new();

    for finding in findings {
        let Some(path) = finding.file.as_deref() else {
            continue;
        };
        let Some(changed) = comparison.file(path) else {
            continue;
        };

        // Which commit changed it, not merely which commit the branch is at now.
        let commit = newest_commit_touching(github, &review.repo, &comparison, path, &mut commits)?;

        let detail = json!({
            "file": path,
            "line": finding.line,
            "lines_changed": changed.touches_line(finding.line),
            "file_status": changed.status,
            "commit": commit.as_deref().unwrap_or(pull_request.head_sha.as_str()),
            "basis": basis,
        });

        let key = format!(
            "code_changed:{}",
            commit.as_deref().unwrap_or(pull_request.head_sha.as_str())
        );

        let written = store.add_harvested_outcome(
            FindingRef {
                review_id: finding.review_id,
                seq: finding.seq,
            },
            NewOutcome {
                kind: OutcomeKind::CodeChangedAfter,
                actor: Actor::Harvester,
                detail: Some(detail.to_string()),
                occurred_at: commit
                    .as_deref()
                    .and_then(|sha| commits.get(sha))
                    .and_then(|commit| commit.committed_at),
            },
            &key,
        )?;

        count(
            written.is_some(),
            &mut report.code_changed_recorded,
            &mut report.already_recorded,
        );
    }

    Ok(())
}

/// The newest commit in the comparison that touched this path.
///
/// The comparison says *that* the file changed; it does not say which commit changed
/// it. Each commit is fetched at most once, and only when a finding needs it.
fn newest_commit_touching(
    github: &dyn GitHub,
    repo: &str,
    comparison: &crate::github::Comparison,
    path: &str,
    cache: &mut HashMap<String, CommitFiles>,
) -> Result<Option<String>> {
    for commit in comparison.commits.iter().rev() {
        let files = match cache.get(&commit.sha) {
            Some(files) => files,
            None => {
                let fetched = github.commit(repo, &commit.sha)?;
                cache.entry(commit.sha.clone()).or_insert(fetched)
            }
        };

        if files.paths.iter().any(|touched| touched == path) {
            return Ok(Some(commit.sha.clone()));
        }
    }

    Ok(None)
}

/// Who brought an outcome about, from a GitHub login.
///
/// `Actor` is a closed set of three, and a GitHub login is not one of them. The pull
/// request's author maps to `author`; anyone else who replies is a `developer`; a fact
/// GitHub attributes to nobody is the `harvester`'s. **The login itself is kept
/// verbatim in the outcome's detail**, so nothing is lost by the mapping.
fn actor_for(login: Option<&str>, pr_author: Option<&str>) -> Actor {
    match (login, pr_author) {
        (Some(login), Some(author)) if login.eq_ignore_ascii_case(author) => Actor::Author,
        (Some(_), _) => Actor::Developer,
        (None, _) => Actor::Harvester,
    }
}

/// Count a write, or count that the row was already there — which is the whole of
/// what a second harvest does.
fn count(written: bool, recorded: &mut usize, already: &mut usize) {
    if written {
        *recorded += 1;
    } else {
        *already += 1;
    }
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 {
        one
    } else {
        many
    }
}
