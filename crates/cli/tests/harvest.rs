//! What `harvest` and `finding link` do when the binary is actually run.
//!
//! Every test gets its own database through [`Desk`], so none can read or write
//! `~/.review-desk/review-desk.db`. Nothing here reaches GitHub: the tests that would
//! run with an empty `PATH`, which is how they prove that an absent `gh` is reported as
//! Review Desk being unable to run rather than as an empty harvest.
//!
//! The review and its findings are seeded through `review open` and `finding add` — the
//! commands a caller would use — rather than through the store.

mod common;

use assert_cmd::Command;
use common::{open_review, Desk};
use predicates::prelude::*;
use tempfile::TempDir;

/// A review with one finding, and the finding's sequence number.
fn a_review(desk: &Desk) -> (String, String) {
    let review = open_review(desk, "devergehq/locus", "64", "9535132");
    let finding = desk
        .command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            r#"{"lens": "correctness", "severity": "should", "file": "a.rs", "line": 12,
                "claim": "something"}"#,
        )
        .assert()
        .success();
    let seq = String::from_utf8(finding.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string();
    (review, seq)
}

/// The binary pointed at this desk, with nothing on `PATH` — so running `gh` fails the
/// way it does on a machine that has never had it.
fn without_gh(desk: &Desk, empty: &TempDir) -> Command {
    let mut command = desk.command();
    command.env("PATH", empty.path());
    command
}

#[test]
fn finding_link_records_the_comment_a_finding_was_posted_as() {
    let desk = Desk::new();
    let (review, seq) = a_review(&desk);

    let line = desk.run(&[
        "finding",
        "link",
        "--review",
        &review,
        "--finding",
        &seq,
        "--github-comment",
        "4140231301",
    ]);
    assert!(line.contains("4140231301"), "{line}");

    let document = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        document["findings"][0]["github_comment_id"], "4140231301",
        "and `review show` reports it: {document}"
    );
}

#[test]
fn finding_link_with_json_prints_one_document() {
    let desk = Desk::new();
    let (review, seq) = a_review(&desk);

    let document = desk.json(&[
        "finding",
        "link",
        "--review",
        &review,
        "--finding",
        &seq,
        "--github-comment",
        "4140231301",
    ]);

    assert_eq!(document["finding"]["github_comment_id"], "4140231301");
    assert_eq!(document["finding"]["seq"].to_string(), seq);
}

#[test]
fn a_comment_already_linked_to_another_finding_is_refused() {
    let desk = Desk::new();
    let (review, seq) = a_review(&desk);
    let other = desk
        .command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(r#"{"lens": "clarity", "severity": "nit", "claim": "another"}"#)
        .assert()
        .success();
    let other = String::from_utf8(other.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string();

    desk.run(&[
        "finding",
        "link",
        "--review",
        &review,
        "--finding",
        &seq,
        "--github-comment",
        "4140231301",
    ]);

    let document = desk.refused_json(&[
        "finding",
        "link",
        "--review",
        &review,
        "--finding",
        &other,
        "--github-comment",
        "4140231301",
    ]);

    assert_eq!(document["error"]["code"], "duplicate_github_comment");
    assert_eq!(document["error"]["kind"], "refused");
}

#[test]
fn linking_an_unknown_finding_is_refused() {
    let desk = Desk::new();
    let (review, _seq) = a_review(&desk);

    let document = desk.refused_json(&[
        "finding",
        "link",
        "--review",
        &review,
        "--finding",
        "99",
        "--github-comment",
        "4140231301",
    ]);

    assert_eq!(document["error"]["code"], "not_found");
}

#[test]
fn an_unknown_review_id_exits_one() {
    let desk = Desk::new();
    let (_review, _seq) = a_review(&desk);
    let empty = TempDir::new().unwrap();

    // With nothing on PATH as well: an unknown review must be refused before anything is
    // asked of GitHub, so this is exit 1 and not exit 2.
    without_gh(&desk, &empty)
        .args(["harvest", "--review", "4040"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no review with id 4040"));
}

#[test]
fn an_unknown_review_id_with_json_carries_the_stores_own_code() {
    let desk = Desk::new();
    let (_review, _seq) = a_review(&desk);
    let empty = TempDir::new().unwrap();

    let output = without_gh(&desk, &empty)
        .args(["harvest", "--review", "4040", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();

    let document: serde_json::Value =
        serde_json::from_slice(&output).expect("one JSON document, even on a refusal");
    // `not_found`, the same code `review show --review 4040` gives. One failure, one name.
    assert_eq!(document["error"]["code"], "not_found");
    assert_eq!(document["error"]["kind"], "refused");
}

#[test]
fn a_missing_gh_exits_two_and_says_it_is_gh_that_is_missing() {
    let desk = Desk::new();
    let (review, _seq) = a_review(&desk);
    let empty = TempDir::new().unwrap();

    // The review is real and up for harvest, so the run gets as far as needing `gh`.
    // Without that, this test would pass on a run that never tried.
    without_gh(&desk, &empty)
        .args(["harvest", "--review", &review])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("gh command is not installed"));
}

#[test]
fn a_missing_gh_with_json_reports_a_code_rather_than_prose() {
    let desk = Desk::new();
    let (review, _seq) = a_review(&desk);
    let empty = TempDir::new().unwrap();

    let output = without_gh(&desk, &empty)
        .args(["harvest", "--review", &review, "--json"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();

    let document: serde_json::Value = serde_json::from_slice(&output).expect("one JSON document");
    assert_eq!(document["error"]["code"], "gh_missing");
    assert_eq!(
        document["error"]["kind"], "broken",
        "an absent gh is Review Desk unable to run, not a refusal"
    );
}

#[test]
fn a_harvest_with_nothing_to_do_asks_github_nothing_and_succeeds() {
    let desk = Desk::new();
    let empty = TempDir::new().unwrap();

    // No reviews, so no GitHub call — provable here by the absence of `gh`.
    without_gh(&desk, &empty)
        .arg("harvest")
        .assert()
        .success()
        .stdout(predicate::str::contains("harvested 0 reviews"));
}

#[test]
fn a_harvest_of_a_repository_with_no_reviews_asks_github_nothing() {
    let desk = Desk::new();
    let (_review, _seq) = a_review(&desk);
    let empty = TempDir::new().unwrap();

    without_gh(&desk, &empty)
        .args(["harvest", "--repo", "devergehq/review-desk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("harvested 0 reviews"));
}

#[test]
fn harvest_with_json_prints_one_document_even_with_nothing_to_do() {
    let desk = Desk::new();
    let empty = TempDir::new().unwrap();

    let output = without_gh(&desk, &empty)
        .args(["harvest", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let document: serde_json::Value = serde_json::from_slice(&output).expect("one JSON document");
    assert_eq!(document["harvest"]["reviews"].as_array().unwrap().len(), 0);
}

#[test]
fn harvest_help_documents_all_three_narrowings() {
    let desk = Desk::new();
    desk.command()
        .args(["harvest", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("--repo"))
        .stdout(predicate::str::contains("--review"))
        .stdout(predicate::str::contains("--from"));
}

/// **`--from` replays recorded responses, through the whole of the live path.**
///
/// DEV-957 needed a merged pull request in a record the command line could build: nothing
/// here writes `pr_state` — the harvest is the only writer, which that issue says is right
/// — and the end-to-end seed goes through this binary and writes no SQL. `recorded::Recorded`
/// has been how this crate's own tests run with no network since the first slice; this makes
/// it reachable.
///
/// **With an empty `PATH`**, which is what proves the claim: a run that reached for `gh`
/// here would fail the way `harvest_reports_an_absent_gh_as_review_desk_being_unable_to_run`
/// does, and this one succeeds. Nothing leaves the machine, and nothing could.
///
/// The recording is the one in `crates/harvest/tests/fixtures`, captured from
/// `devergehq/locus#64` — a merged pull request — so the state it writes is `merged` and the
/// step it leaves is `done`.
#[test]
fn harvest_from_a_recording_records_the_merge_with_no_network() {
    let desk = Desk::new();
    let empty = TempDir::new().unwrap();
    let (review, _) = a_review(&desk);

    // Posted, because step 7 is the harvest of a posted review and `record_harvest_step`
    // answers `None` for one nothing has posted.
    desk.run(&[
        "review",
        "link",
        "--review",
        &review,
        "--github-review",
        "PRR_recorded",
    ]);

    let recording = concat!(env!("CARGO_MANIFEST_DIR"), "/../harvest/tests/fixtures");
    let output = without_gh(&desk, &empty)
        .args([
            "harvest", "--review", &review, "--from", recording, "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let document: serde_json::Value = serde_json::from_slice(&output).expect("one JSON document");
    let read = &document["harvest"]["reviews"][0];
    assert_eq!(read["pr_state"], "merged", "{document}");
    assert_eq!(read["step_seven"]["state"], "done", "{document}");

    // And the record holds it, which is what the dashboard reads.
    let shown: serde_json::Value =
        serde_json::from_str(&desk.run(&["review", "show", "--review", &review, "--json"]))
            .expect("the review");
    assert_eq!(shown["review"]["pr_state"], "merged", "{shown}");
}

/// A directory with no recording in it is a refusal that names what it wanted, not a run
/// that found nothing.
#[test]
fn harvest_from_a_directory_with_no_recording_says_what_it_could_not_read() {
    let desk = Desk::new();
    let empty = TempDir::new().unwrap();
    let nowhere = TempDir::new().unwrap();
    let (review, _) = a_review(&desk);
    desk.run(&[
        "review",
        "link",
        "--review",
        &review,
        "--github-review",
        "PRR_recorded",
    ]);

    without_gh(&desk, &empty)
        .args([
            "harvest",
            "--review",
            &review,
            "--from",
            nowhere.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pull_request"));
}

#[test]
fn an_unreadable_database_exits_two() {
    let dir = TempDir::new().unwrap();
    let mut command = Command::cargo_bin("review-desk").unwrap();

    command
        .env("REVIEW_DESK_DB", dir.path())
        .arg("harvest")
        .assert()
        .code(2);
}
