//! Whether a finding's code changed, answered from file contents. DEV-1344.
//!
//! **Every repository here is synthetic**: an `acme/portal` built in the test, with its
//! commits, files and comparisons written below. No client repository's data is recorded
//! or committed. The GitHub responses are still composed as GitHub's JSON and parsed by
//! [`review_desk_harvest::wire`], the same parsing the live run uses, so a shape the
//! harvester reads wrongly fails here rather than being skipped by a hand-built fact.
//!
//! The pull request, in every fixture: `acme/portal#512`, five files of its own, posted on
//! [`POSTED`] and now at [`HEAD`].

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use review_desk_harvest::{
    harvest, wire, BlobAt, CommitFiles, Comparison, GitHub, HarvestError, PullRequest,
    ReviewThread, Selection,
};
use review_desk_store::{
    Disposition, FindingRef, NewFinding, NewReview, Outcome, OutcomeKind, Review, Severity, Store,
};
use serde_json::{json, Value};
use tempfile::TempDir;

const REPO: &str = "acme/portal";
const PR: i64 = 512;

/// The commit the review read, which is also the one it was posted on unless a comment
/// says otherwise.
const POSTED: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
/// The pull request's head now, after a rebase onto a moved base.
const HEAD: &str = "f9e8d7c6b5a40312a1b2c3d4e5f6a7b8c9d0e1f2";
/// The base branch now.
const BASE: &str = "0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c";

/// The pull request's own five files.
const OWN: [&str; 5] = [
    "app/Exports/ScheduledExport.php",
    "app/Exports/ExportJob.php",
    "app/Exports/TenantScope.php",
    "config/exports.php",
    "tests/Exports/ScheduledExportTest.php",
];

/// A file only upstream touched: the base moved under the pull request and changed it.
const UPSTREAM_ONLY: &str = "app/Billing/Invoice.php";

/// Ten lines each, so the five own files add up to the pull request's 50 changed lines.
const LINES_EACH: i64 = 10;

/// A file of `n` numbered lines, each saying which file and version it is from.
fn text(name: &str, version: &str, n: usize) -> String {
    (1..=n)
        .map(|line| format!("    // {name} {version} line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text`, with one line rewritten.
fn with_line(base: &str, line: usize, replacement: &str) -> String {
    base.lines()
        .enumerate()
        .map(|(at, held)| {
            if at + 1 == line {
                replacement.to_string()
            } else {
                held.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text`, with lines inserted at the top: the finding moves; it does not change.
fn pushed_down(base: &str, by: usize) -> String {
    let mut lines: Vec<String> = (0..by).map(|n| format!("    // inserted {n}")).collect();
    lines.extend(base.lines().map(str::to_string));
    lines.join("\n")
}

/// A synthetic GitHub: commits holding files, comparisons written out, and nothing else.
#[derive(Default)]
struct Acme {
    pull_request: Value,
    threads: Vec<Value>,
    /// Comparisons by `(base, head)`. One that is absent answers 404, which is what GitHub
    /// says about a comparison from a commit a force-push took away.
    compares: HashMap<(String, String), Value>,
    /// Each commit GitHub still has, and the files at it.
    trees: HashMap<String, BTreeMap<String, String>>,
    /// Paths whose every read fails as GitHub fails a rate-limited one.
    rate_limited: HashSet<String>,
    reads: RefCell<Vec<String>>,
}

impl Acme {
    /// The pull request: 30 added and 20 deleted is the 50 lines its own files add up to.
    fn new() -> Self {
        Self {
            pull_request: json!({
                "state": "open",
                "merged": false,
                "title": "Hold a scheduled export's tenant for the whole run",
                "additions": 30,
                "deletions": 20,
                "head": {"sha": HEAD},
                "base": {"sha": BASE},
                "user": {"login": "dana-okoye"},
            }),
            ..Self::default()
        }
    }

    /// The pull request's own files, read from its base: five, ten lines each.
    fn own_files_compare(mut self, total_lines: i64) -> Self {
        let mut files: Vec<Value> = OWN
            .iter()
            .map(|path| json!({"filename": path, "status": "modified", "changes": LINES_EACH}))
            .collect();
        // Whatever is left over goes on the last file, so a test can make the sum wrong.
        let short = 5 * LINES_EACH - total_lines;
        files[4]["changes"] = json!(LINES_EACH - short);
        self.compares.insert(
            (BASE.to_string(), HEAD.to_string()),
            json!({"status": "ahead", "total_commits": 4, "commits": [], "files": files}),
        );
        self
    }

    fn compare(mut self, base: &str, value: Value) -> Self {
        self.compares
            .insert((base.to_string(), HEAD.to_string()), value);
        self
    }

    fn at(mut self, commit: &str, path: &str, contents: &str) -> Self {
        self.trees
            .entry(commit.to_string())
            .or_default()
            .insert(path.to_string(), contents.to_string());
        self
    }

    fn reads_of(&self, prefix: &str) -> usize {
        self.reads
            .borrow()
            .iter()
            .filter(|read| read.starts_with(prefix))
            .count()
    }
}

/// A comparison response, as GitHub shapes one.
fn comparison(status: &str, commits: &[(&str, bool)], files: &[&str], total: i64) -> Value {
    json!({
        "status": status,
        "total_commits": total,
        "commits": commits.iter().map(|(sha, merge)| json!({
            "sha": sha,
            "parents": if *merge { json!([{"sha": "p1"}, {"sha": "p2"}]) } else { json!([{"sha": "p1"}]) },
            "commit": {"message": "a commit", "committer": {"date": "2026-11-12T10:00:00Z"}},
        })).collect::<Vec<_>>(),
        "files": files.iter().map(|path| json!({"filename": path, "status": "modified", "changes": 2})).collect::<Vec<_>>(),
    })
}

fn not_found(what: String) -> HarvestError {
    HarvestError::GhFailed {
        args: what,
        status: "exit 1".to_string(),
        stderr: "gh: Not Found (HTTP 404)".to_string(),
    }
}

/// A content id: equal for equal contents, and only for them, as git's blob ids are.
fn oid(contents: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:040x}", hasher.finish())
}

impl GitHub for Acme {
    fn pull_request(&self, _repo: &str, _pr: i64) -> review_desk_harvest::Result<PullRequest> {
        wire::pull_request(&self.pull_request.to_string())
    }

    fn review_threads(
        &self,
        _repo: &str,
        _pr: i64,
    ) -> review_desk_harvest::Result<Vec<ReviewThread>> {
        wire::review_threads(
            &json!({"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": self.threads}}}}})
                .to_string(),
        )
    }

    fn compare(
        &self,
        _repo: &str,
        base: &str,
        head: &str,
    ) -> review_desk_harvest::Result<Comparison> {
        self.reads
            .borrow_mut()
            .push(format!("compare {base}...{head}"));
        match self.compares.get(&(base.to_string(), head.to_string())) {
            Some(value) => wire::comparison(&value.to_string()),
            None => Err(not_found(format!("compare {base}...{head}"))),
        }
    }

    fn commit(&self, _repo: &str, sha: &str) -> review_desk_harvest::Result<CommitFiles> {
        // Only a trusted comparison asks which commit changed a file. Every commit in
        // these fixtures touched every file it lists, which is all that read is for.
        wire::commit(
            &json!({"sha": sha, "commit": {"committer": {"date": "2026-11-12T10:00:00Z"}},
                    "files": OWN.iter().map(|path| json!({"filename": path})).collect::<Vec<_>>()})
            .to_string(),
        )
    }

    fn blob(&self, _repo: &str, commit: &str, path: &str) -> review_desk_harvest::Result<BlobAt> {
        self.reads
            .borrow_mut()
            .push(format!("blob {commit}:{path}"));
        if self.rate_limited.contains(path) {
            return Err(HarvestError::GhFailed {
                args: format!("api graphql {commit}:{path}"),
                status: "exit 1".to_string(),
                stderr: "gh: API rate limit exceeded for user. (HTTP 403)".to_string(),
            });
        }
        let tree = self.trees.get(commit);
        let response = json!({"data": {"repository": {
            "commit": tree.map(|_| json!({"oid": commit})),
            "file": tree.and_then(|tree| tree.get(path)).map(|contents| json!({
                "oid": oid(contents), "text": contents, "isBinary": false, "isTruncated": false,
            })),
        }}});
        wire::blob(&response.to_string())
    }
}

/// A posted review of `acme/portal#512` at [`POSTED`], with one finding per entry.
fn a_review(findings: &[(&str, i64, Option<&str>)]) -> (TempDir, Store, Review) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();
    let review = store
        .create_review(NewReview {
            repo: REPO.to_string(),
            pr: PR,
            head_sha: POSTED.to_string(),
            round: 1,
            github_review_id: Some("PRR_acme512".to_string()),
            ..Default::default()
        })
        .unwrap();

    for (file, line, comment) in findings {
        let finding = store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity: Severity::Should,
                    file: Some(file.to_string()),
                    line: Some(*line),
                    claim: format!("something about {file}"),
                    evidence: None,
                    disposition: Disposition::InDraft,
                    disposition_reason: None,
                    rule_id: None,
                    ..Default::default()
                },
            )
            .unwrap();
        if let Some(comment) = comment {
            store
                .set_finding_github_comment(review.id, finding.seq, comment)
                .unwrap();
        }
    }

    (dir, store, review)
}

fn outcomes(store: &Store, review: &Review, seq: i64, kind: OutcomeKind) -> Vec<Outcome> {
    store
        .list_outcomes_for_finding(FindingRef {
            review_id: review.id,
            seq,
        })
        .unwrap()
        .into_iter()
        .filter(|outcome| outcome.kind == kind)
        .collect()
}

fn detail(outcome: &Outcome) -> Value {
    serde_json::from_str(outcome.detail.as_deref().expect("a detail")).expect("JSON")
}

/// The four fields every figure carries, whatever it says.
fn assert_carries_its_method(held: &Value) {
    assert!(held["method"].is_string(), "method: {held}");
    assert!(held["checks_run"].is_u64(), "checks_run: {held}");
    assert!(held["checks_passed"].is_u64(), "checks_passed: {held}");
    assert_eq!(held["method_version"], 1, "method_version: {held}");
    assert!(
        held["checks_passed"].as_u64() <= held["checks_run"].as_u64(),
        "a check cannot pass without running: {held}"
    );
}

/// The diverged fixture: the pull request was rebased onto a base that moved, and upstream
/// touched 120 files — among them [`UPSTREAM_ONLY`] — against the pull request's own 5.
fn diverged() -> Acme {
    let upstream: Vec<String> = (0..119)
        .map(|n| format!("app/Upstream/File{n}.php"))
        .chain([UPSTREAM_ONLY.to_string()])
        .collect();
    let mut files: Vec<&str> = upstream.iter().map(String::as_str).collect();
    files.extend(OWN);

    let export = text("ScheduledExport", "v1", 60);
    let job = text("ExportJob", "v1", 60);
    Acme::new()
        .own_files_compare(50)
        .compare(
            POSTED,
            comparison(
                "diverged",
                &[("c0ffee1", false), ("c0ffee2", false)],
                &files,
                2,
            ),
        )
        // Upstream rewrote the invoice between the posted commit and the rebased head.
        .at(POSTED, UPSTREAM_ONLY, &text("Invoice", "v1", 80))
        .at(HEAD, UPSTREAM_ONLY, &text("Invoice", "v2", 80))
        // The author changed line 40 of the export.
        .at(POSTED, OWN[0], &export)
        .at(
            HEAD,
            OWN[0],
            &with_line(&export, 40, "    $tenant = $this->tenant;"),
        )
        // The job is the pull request's own file, carried through the rebase unchanged.
        .at(POSTED, OWN[1], &job)
        .at(HEAD, OWN[1], &job)
}

#[test]
fn a_file_only_upstream_touched_is_not_credited_to_the_author() {
    let (_dir, store, review) = a_review(&[(UPSTREAM_ONLY, 30, None)]);

    let report = harvest(&store, &diverged(), Selection::everything()).unwrap();

    assert!(
        outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter).is_empty(),
        "upstream rewrote the invoice; the author did not, and the record must not say so"
    );
    assert!(outcomes(&store, &review, 1, OutcomeKind::CodeChangeUnknown).is_empty());
    assert_eq!(report.reviews[0].code_not_the_pull_requests, 1);
    assert_eq!(report.reviews[0].code_changed_recorded, 0);
}

#[test]
fn a_file_the_author_changed_is_still_credited_after_the_rebase() {
    let (_dir, store, review) = a_review(&[(OWN[0], 40, None), (OWN[1], 12, None)]);

    harvest(&store, &diverged(), Selection::everything()).unwrap();

    let changed = outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter);
    assert_eq!(changed.len(), 1, "the author rewrote line 40");
    let held = detail(&changed[0]);
    assert_eq!(held["lines_changed"], true);
    assert_eq!(held["file_status"], "modified");
    assert_eq!(held["method"], "blob_sha+content_window");
    assert_eq!(
        held["commit"], HEAD,
        "a diverged comparison is not trusted to name the commit; the head is credited"
    );
    assert_eq!(held["basis"], format!("compare {POSTED}...{HEAD}"));
    assert_carries_its_method(&held);

    assert!(
        outcomes(&store, &review, 2, OutcomeKind::CodeChangedAfter).is_empty(),
        "the job came through the rebase byte for byte, so its blob is the same"
    );
}

/// The `ahead` fixture: three base-branch merges in range, 250 commits listed of 260, and
/// 300 files — every reason a comparison may not be read — against 5 files of its own.
fn ahead_with_merges() -> Acme {
    let mut commits: Vec<(String, bool)> = (0..250).map(|n| (format!("{n:07x}"), false)).collect();
    for merge in [10, 120, 240] {
        commits[merge].1 = true;
    }
    let commits: Vec<(&str, bool)> = commits.iter().map(|(sha, m)| (sha.as_str(), *m)).collect();
    let files: Vec<String> = (0..295)
        .map(|n| format!("vendor/upstream/File{n}.php"))
        .chain(OWN.iter().map(|path| path.to_string()))
        .collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();

    let export = text("ScheduledExport", "v1", 60);
    let scope = text("TenantScope", "v1", 40);
    Acme::new()
        .own_files_compare(50)
        .compare(POSTED, comparison("ahead", &commits, &files, 260))
        // The file changed above the finding, which moved it and did not change it.
        .at(POSTED, OWN[0], &export)
        .at(HEAD, OWN[0], &pushed_down(&export, 4))
        // The scope's line 20 was rewritten.
        .at(POSTED, OWN[2], &scope)
        .at(HEAD, OWN[2], &with_line(&scope, 20, "    return null;"))
        // The config did not change at all.
        .at(POSTED, OWN[3], "<?php return [];")
        .at(HEAD, OWN[3], "<?php return [];")
}

#[test]
fn a_range_with_merges_and_capped_lists_is_refused_and_every_finding_still_answered() {
    let (_dir, store, review) =
        a_review(&[(OWN[0], 30, None), (OWN[2], 20, None), (OWN[3], 1, None)]);
    let github = ahead_with_merges();

    let report = harvest(&store, &github, Selection::everything()).unwrap();

    // Refused: not one commit of the 250 was read to name who changed what.
    assert_eq!(
        github.reads_of("commit"),
        0,
        "a comparison with merges in range names no commit"
    );

    let moved = outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter);
    assert_eq!(moved.len(), 1);
    let moved = detail(&moved[0]);
    assert_eq!(
        moved["lines_changed"], false,
        "four lines inserted above the finding moved it; the seven around it are all there"
    );
    assert_eq!(moved["method"], "blob_sha+content_window");
    assert_eq!(moved["commit"], HEAD);
    assert_carries_its_method(&moved);

    let rewritten = outcomes(&store, &review, 2, OutcomeKind::CodeChangedAfter);
    assert_eq!(rewritten.len(), 1);
    assert_eq!(detail(&rewritten[0])["lines_changed"], true);

    assert!(
        outcomes(&store, &review, 3, OutcomeKind::CodeChangedAfter).is_empty(),
        "the same blob at both commits is unchanged, whatever the comparison listed"
    );
    assert_eq!(report.reviews[0].code_unknown_recorded, 0);
}

/// The fixture after a force-push took the posted commit: GitHub has no such commit.
fn history_rewritten() -> Acme {
    // No comparison from POSTED is written, so it answers 404; no tree at POSTED either.
    Acme::new()
        .own_files_compare(50)
        .at(HEAD, OWN[0], &text("ScheduledExport", "v2", 60))
}

#[test]
fn a_posted_commit_that_is_gone_is_unknown_never_unchanged_and_never_nought() {
    let (_dir, store, review) = a_review(&[(OWN[0], 40, None)]);

    let report = harvest(&store, &history_rewritten(), Selection::everything()).unwrap();

    assert!(outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter).is_empty());
    let unknown = outcomes(&store, &review, 1, OutcomeKind::CodeChangeUnknown);
    assert_eq!(unknown.len(), 1);
    let held = detail(&unknown[0]);
    assert_eq!(held["reason"], "history_rewritten");
    assert_eq!(held["file"], OWN[0]);
    assert_eq!(held["line"], 40);
    assert_carries_its_method(&held);
    assert!(
        held["checks_passed"].as_u64() < held["checks_run"].as_u64(),
        "an unknown is a check that did not pass: {held}"
    );
    assert_eq!(report.reviews[0].code_unknown_recorded, 1);
    assert!(
        report.summary().contains("1 finding's code change unknown"),
        "{}",
        report.summary()
    );

    // Step 7 says "—", not a count of changes and not nothing.
    let step = store.get_review_layer(review.id, 7).unwrap();
    assert!(
        step.facts
            .iter()
            .any(|fact| fact.label == "Code change unknown" && fact.value == "— 1 finding"),
        "{:?}",
        step.facts
    );
    assert!(
        !step
            .facts
            .iter()
            .any(|fact| fact.label == "Code changed since"),
        "nothing is known to have changed"
    );

    // And the queue does not draw it as something the author did.
    assert!(
        store
            .reviews_with_author_activity(None, None)
            .unwrap()
            .is_empty(),
        "an unknown is not the author's act"
    );
    assert!(store.author_activity(review.id).unwrap().is_empty());

    // Twice is once.
    let second = harvest(&store, &history_rewritten(), Selection::everything()).unwrap();
    assert_eq!(second.reviews[0].code_unknown_recorded, 0);
    assert_eq!(second.reviews[0].already_recorded, 1);
}

#[test]
fn own_files_that_do_not_add_up_are_unverified_and_nothing_is_credited() {
    // 49 lines in the comparison against the pull request's 50: GitHub left something out,
    // or the range is not the pull request's.
    let github = diverged().own_files_compare(49);
    let (_dir, store, review) = a_review(&[(OWN[0], 40, None)]);

    harvest(&store, &github, Selection::everything()).unwrap();

    assert!(outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter).is_empty());
    let unknown = outcomes(&store, &review, 1, OutcomeKind::CodeChangeUnknown);
    assert_eq!(unknown.len(), 1);
    let held = detail(&unknown[0]);
    assert_eq!(held["reason"], "own_files_unverified");
    assert_eq!(held["checks_run"], 4);
    assert_eq!(held["checks_passed"], 3);
}

#[test]
fn the_baseline_is_the_commit_the_comment_was_posted_on() {
    // The desk read POSTED; the comment went out on a later commit the author had already
    // pushed, which rewrote line 40 before they could have seen the finding.
    const ON: &str = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";
    let export = text("ScheduledExport", "v1", 60);
    let fixed = with_line(&export, 40, "    $tenant = $this->tenant;");

    let mut github = diverged()
        .compare(ON, comparison("ahead", &[], &[], 0))
        .at(ON, OWN[0], &fixed)
        .at(HEAD, OWN[0], &fixed);
    github.threads = vec![json!({
        "isResolved": false,
        "resolvedBy": null,
        "comments": {"nodes": [{
            "databaseId": 9001, "author": {"login": "patrick-d"}, "body": "F1",
            "createdAt": "2026-11-12T09:00:00Z", "originalCommit": {"oid": ON},
        }]},
    })];
    let (_dir, store, review) = a_review(&[(OWN[0], 40, Some("9001"))]);

    harvest(&store, &github, Selection::everything()).unwrap();

    assert!(
        outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter).is_empty(),
        "the change was made between reading and posting, so it is not the author's answer"
    );
}

#[test]
fn a_rate_limited_read_is_reported_and_not_stored() {
    let mut github = diverged();
    github.rate_limited.insert(OWN[0].to_string());
    let (_dir, store, review) = a_review(&[(OWN[0], 40, None)]);

    let report = harvest(&store, &github, Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].code_unknown_this_run, 1);
    assert_eq!(report.reviews[0].code_unknown_recorded, 0);
    assert!(
        store.list_outcomes(review.id).unwrap().is_empty(),
        "the next harvest asks again; a stored unknown would outlive its answer"
    );
}

#[test]
fn a_deleted_file_changed_and_its_lines_went_with_it() {
    let github = diverged().at(POSTED, OWN[4], &text("Test", "v1", 30));
    let (_dir, store, review) = a_review(&[(OWN[4], 10, None)]);

    harvest(&store, &github, Selection::everything()).unwrap();

    let changed = outcomes(&store, &review, 1, OutcomeKind::CodeChangedAfter);
    assert_eq!(changed.len(), 1);
    let held = detail(&changed[0]);
    assert_eq!(held["file_status"], "removed");
    assert_eq!(held["lines_changed"], true);
}

#[test]
fn each_file_is_read_once_at_each_commit() {
    let github = diverged();
    let (_dir, _store, _review) = {
        let (dir, store, review) = a_review(&[(OWN[0], 40, None), (OWN[0], 10, None)]);
        harvest(&store, &github, Selection::everything()).unwrap();
        (dir, store, review)
    };

    assert_eq!(
        github.reads_of("blob"),
        2,
        "two findings, one file, two commits"
    );
    assert_eq!(
        github.reads_of(&format!("compare {BASE}")),
        1,
        "the pull request's own files are read once"
    );
}
