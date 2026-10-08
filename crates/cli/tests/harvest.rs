//! What `harvest` and `finding link` do when the binary is actually run.
//!
//! Every test points `REVIEW_DESK_DB` at a temporary file, so none can read or write
//! `~/.review-desk/review-desk.db`. Nothing here reaches GitHub: the one test that
//! would runs with an empty `PATH`, which is how it proves that an absent `gh` is
//! reported as Review Desk being unable to run rather than as an empty harvest.

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use review_desk_store::{Disposition, NewFinding, NewReview, Severity, Store};
use tempfile::TempDir;

/// The binary under test, with the ambient database override cleared.
fn review_desk() -> Command {
    let mut command = Command::cargo_bin("review-desk").expect("the review-desk binary");
    command.env_remove("REVIEW_DESK_DB");
    command
}

/// A database holding one review with one finding, and no linked comment.
fn a_review(dir: &TempDir) -> (std::path::PathBuf, i64) {
    let path = dir.path().join("review-desk.db");
    let store = Store::open(&path).unwrap();
    let review = store
        .create_review(NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: "9535132".to_string(),
            round: 1,
            ..Default::default()
        })
        .unwrap();
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                file: Some("a.rs".to_string()),
                line: Some(12),
                claim: "something".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .unwrap();
    (path, review.id)
}

/// A directory with no `gh` in it, so that running `gh` fails the way it does on a
/// machine that has never had it.
fn without_gh(command: &mut Command, empty: &Path) {
    command.env("PATH", empty);
}

#[test]
fn finding_link_records_the_comment_a_finding_was_posted_as() {
    let dir = TempDir::new().unwrap();
    let (path, review) = a_review(&dir);

    review_desk()
        .env("REVIEW_DESK_DB", &path)
        .args([
            "finding",
            "link",
            "--review",
            &review.to_string(),
            "--finding",
            "1",
            "--github-comment",
            "4140231301",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("4140231301"));

    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.get_finding(review, 1).unwrap().github_comment_id,
        Some("4140231301".to_string())
    );
}

#[test]
fn finding_link_with_json_prints_one_document() {
    let dir = TempDir::new().unwrap();
    let (path, review) = a_review(&dir);

    let output = review_desk()
        .env("REVIEW_DESK_DB", &path)
        .args([
            "finding",
            "link",
            "--review",
            &review.to_string(),
            "--finding",
            "1",
            "--github-comment",
            "4140231301",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("standard output is one JSON document");
    assert_eq!(json["ok"], true);
    assert_eq!(json["review"], review);
    assert_eq!(json["finding"], 1);
    assert_eq!(json["github_comment_id"], "4140231301");
}

#[test]
fn linking_an_unknown_finding_is_refused() {
    let dir = TempDir::new().unwrap();
    let (path, review) = a_review(&dir);

    review_desk()
        .env("REVIEW_DESK_DB", &path)
        .args([
            "finding",
            "link",
            "--review",
            &review.to_string(),
            "--finding",
            "99",
            "--github-comment",
            "4140231301",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no finding"));
}

#[test]
fn an_unknown_review_id_exits_one() {
    let dir = TempDir::new().unwrap();
    let (path, _review) = a_review(&dir);
    let empty = TempDir::new().unwrap();

    // Run without gh on the path as well: an unknown review must be refused before
    // anything is asked of GitHub, so this is exit 1 and not exit 2.
    let mut command = review_desk();
    without_gh(&mut command, empty.path());
    command
        .env("REVIEW_DESK_DB", &path)
        .args(["harvest", "--review", "4040"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no review with id 4040"));
}

#[test]
fn an_unknown_review_id_with_json_still_prints_one_document() {
    let dir = TempDir::new().unwrap();
    let (path, _review) = a_review(&dir);
    let empty = TempDir::new().unwrap();

    let mut command = review_desk();
    without_gh(&mut command, empty.path());
    let output = command
        .env("REVIEW_DESK_DB", &path)
        .args(["harvest", "--review", "4040", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("one JSON document, even on a refusal");
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "refused");
}

#[test]
fn a_missing_gh_exits_two_and_says_it_is_gh_that_is_missing() {
    let dir = TempDir::new().unwrap();
    let (path, review) = a_review(&dir);
    let empty = TempDir::new().unwrap();

    // The review is real and up for harvest, so the run gets as far as needing gh.
    // Without that, this test would pass on a run that never tried.
    let mut command = review_desk();
    without_gh(&mut command, empty.path());
    command
        .env("REVIEW_DESK_DB", &path)
        .args(["harvest", "--review", &review.to_string()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("gh command is not installed"));
}

#[test]
fn a_missing_gh_with_json_reports_a_code_rather_than_prose() {
    let dir = TempDir::new().unwrap();
    let (path, review) = a_review(&dir);
    let empty = TempDir::new().unwrap();

    let mut command = review_desk();
    without_gh(&mut command, empty.path());
    let output = command
        .env("REVIEW_DESK_DB", &path)
        .args(["harvest", "--review", &review.to_string(), "--json"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("one JSON document");
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "gh_missing");
}

#[test]
fn a_harvest_with_nothing_to_do_asks_github_nothing_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.db");
    Store::open(&path).unwrap();
    let empty = TempDir::new().unwrap();

    // No reviews, so no GitHub call — provable here by the absence of gh.
    let mut command = review_desk();
    without_gh(&mut command, empty.path());
    command
        .env("REVIEW_DESK_DB", &path)
        .arg("harvest")
        .assert()
        .success()
        .stdout(predicate::str::contains("harvested 0 reviews"));
}

#[test]
fn harvest_help_documents_both_narrowings() {
    review_desk()
        .args(["harvest", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("--repo"))
        .stdout(predicate::str::contains("--review"))
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn an_unreadable_database_exits_two() {
    let dir = TempDir::new().unwrap();

    review_desk()
        .env("REVIEW_DESK_DB", dir.path())
        .arg("harvest")
        .assert()
        .code(2);
}
