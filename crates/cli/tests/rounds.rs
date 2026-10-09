//! What the `round` group and `finding carry` do when the binary is actually run.
//!
//! Every test gets its own database through [`Desk`], so none can read or write
//! `~/.review-desk/review-desk.db`.
//!
//! `round commits` reads GitHub, so the tests that exercise it put a shell script called
//! `gh` on `PATH` and nothing else — the same mechanism `tests/harvest.rs` uses to prove an
//! absent `gh` is reported rather than swallowed, pointed the other way. The script answers
//! the two reads the command makes and refuses everything else, so a command that started
//! making a third read, or any kind of write, fails here.

mod common;

use std::path::Path;

use common::{open_review, Desk};
use tempfile::TempDir;

/// A `gh` that answers the two reads `round commits` makes, and refuses anything else.
///
/// **Any `gh` subcommand other than `api` exits non-zero**, which is what makes this a test
/// of "Review Desk reads GitHub and never writes to it" rather than a convenience: a
/// command that grew a `gh pr review` would fail here.
fn fake_gh(dir: &Path) -> TempDir {
    let bin = TempDir::new().expect("a directory for the fake gh");
    let script = format!(
        r#"#!/bin/sh
if [ "$1" != "api" ]; then
  echo "this fake gh only answers reads, and was asked: $*" >&2
  exit 1
fi
case "$2" in
  */compare/*) /bin/cat {dir}/compare.json ;;
  */commits/a41c9e2) /bin/cat {dir}/commit-a41c9e2.json ;;
  */commits/c7730bd) /bin/cat {dir}/commit-c7730bd.json ;;
  *) echo "not recorded: $2" >&2; exit 1 ;;
esac
"#,
        dir = dir.display()
    );

    let path = bin.path().join("gh");
    std::fs::write(&path, script).expect("write the fake gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make it executable");
    }
    bin
}

/// The two commits between round 1 and round 2, as GitHub would answer for them.
fn record_responses(dir: &Path) {
    std::fs::write(
        dir.join("compare.json"),
        r#"{"status":"ahead","commits":[
             {"sha":"a41c9e2","commit":{"message":"Send an idempotency key with each delivery\n\nbody","committer":{"date":"2026-10-09T22:14:00Z"}}},
             {"sha":"c7730bd","commit":{"message":"Record a delivery as dead after the last attempt","committer":{"date":"2026-10-09T23:02:00Z"}}}
           ],"files":[]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("commit-a41c9e2.json"),
        r#"{"sha":"a41c9e2","commit":{"committer":{"date":"2026-10-09T22:14:00Z"}},
            "files":[{"filename":"jobs/DeliverWebhook.php","status":"modified"}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("commit-c7730bd.json"),
        r#"{"sha":"c7730bd","commit":{"committer":{"date":"2026-10-09T23:02:00Z"}},
            "files":[{"filename":"jobs/DeliverWebhook.php","status":"modified"},
                     {"filename":"tests/WebhookRetryTest.php","status":"added"}]}"#,
    )
    .unwrap();
}

/// A finding on a review, and the number the command line printed for it.
fn add(desk: &Desk, review: &str, claim: &str, document: &str) -> String {
    let body = format!(
        r#"{{"lens": "sibling diff", "severity": "should", "claim": "{claim}"{document}}}"#
    );
    let output = desk
        .command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(body)
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn opening_round_two_hands_back_round_ones_findings_ready_to_be_carried() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    add(&desk, &first, "no idempotency key", "");
    add(&desk, &first, "the backoff has no ceiling", "");

    let document = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "9d02e71",
    ]);

    assert_eq!(document["review"]["round"], 2);
    assert_eq!(document["created"], true);
    assert_eq!(
        document["carryable"]["from_round"], 1,
        "a second-round review that cannot see round 1's findings cannot say what became \
         of them: {document}"
    );
    assert_eq!(document["carryable"]["findings"][0]["seq"], 1);
    assert_eq!(document["carryable"]["findings"][1]["seq"], 2);
    assert_eq!(
        document["carryable"]["findings"][1]["claim"],
        "the backoff has no ceiling"
    );
}

#[test]
fn opening_a_first_round_has_nothing_to_carry() {
    let desk = Desk::new();
    let document = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
    ]);

    assert!(
        document["carryable"].is_null(),
        "null rather than an empty list: there is no earlier round, which is not the same \
         as an earlier round that found nothing — {document}"
    );
}

#[test]
fn review_open_still_prints_the_id_and_nothing_else() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    add(&desk, &first, "a finding", "");

    // `ID=$(review-desk review open ...)` is the whole of what a shell caller does, and
    // the carryable findings went into the document rather than onto this line.
    let line = open_review(&desk, "acme/portal", "412", "9d02e71");
    assert!(
        line.parse::<i64>().is_ok(),
        "the plain output is still one id: {line:?}"
    );
}

#[test]
fn a_carried_finding_keeps_its_number_and_reports_the_rounds_it_has_been_open() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    add(&desk, &first, "no idempotency key", "");
    let f2 = add(&desk, &first, "the retry delay ignores Retry-After", "");
    assert_eq!(f2, "2");

    let second = open_review(&desk, "acme/portal", "412", "9d02e71");
    let line = desk.run(&[
        "finding",
        "carry",
        "--review",
        &second,
        "--from",
        &first,
        "--finding",
        &f2,
        "--status",
        "still_open",
        "--by",
        "agent",
        "--checked",
        "Code unchanged, no reply.",
    ]);
    assert!(line.contains("still_open"), "{line}");
    assert!(line.contains("decided by agent"), "{line}");
    assert!(
        line.contains("open for 2 rounds"),
        "the sentence the page shows: {line}"
    );

    let third = open_review(&desk, "acme/portal", "412", "eeeeeee");
    let document = desk.json(&[
        "finding",
        "carry",
        "--review",
        &third,
        "--from",
        &second,
        "--finding",
        &f2,
        "--status",
        "still_open",
        "--by",
        "developer",
    ]);

    assert_eq!(
        document["finding"]["seq"], 2,
        "F2 in round 1 is F2 in round 3: {document}"
    );
    assert_eq!(document["rounds_open"], 3, "open for three rounds");
    assert_eq!(document["finding"]["round_status_by"], "developer");
}

#[test]
fn a_new_finding_names_the_commit_and_the_finding_it_was_introduced_by() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let f1 = add(&desk, &first, "no idempotency key", "");

    let second = open_review(&desk, "acme/portal", "412", "9d02e71");
    desk.run(&[
        "finding",
        "carry",
        "--review",
        &second,
        "--from",
        &first,
        "--finding",
        &f1,
        "--status",
        "fixed",
        "--by",
        "agent",
        "--commit",
        "a41c9e2",
        "--checked",
        "their new test fails with the fix reverted",
    ]);

    let seq = add(
        &desk,
        &second,
        "the idempotency key is different on every retry",
        r#", "file": "jobs/DeliverWebhook.php", "line": 59,
            "introduced_by_commit": "a41c9e2", "introduced_by_seq": 1"#,
    );
    assert_eq!(seq, "2", "the next number unused on the pull request");

    let document = desk.json(&["review", "show", "--review", &second]);
    let introduced = &document["findings"][1];
    assert_eq!(introduced["introduced_by_commit"], "a41c9e2");
    assert_eq!(introduced["introduced_by_seq"], 1);

    let line = desk.run(&["review", "show", "--review", &second]);
    assert!(
        line.contains("introduced by a41c9e2, which fixed F1"),
        "the one view a person reads has to show it: {line}"
    );
}

#[test]
fn an_attribution_naming_a_finding_nobody_raised_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let document = desk.refused_json(&["finding", "add", "--review", &review, "--file", "-"]);
    // `refused_json` cannot write stdin, so the refusal above is the missing document.
    // The attribution's own refusal is next.
    assert_eq!(document["error"]["code"], "bad_input");

    let refused = desk
        .command()
        .args([
            "finding", "add", "--review", &review, "--file", "-", "--json",
        ])
        .write_stdin(
            r#"{"lens": "l", "severity": "should", "claim": "c", "introduced_by_seq": 99}"#,
        )
        .assert()
        .code(1);
    let text = String::from_utf8(refused.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains("99"),
        "the refusal names the number that is not there: {text}"
    );
}

#[test]
fn finding_became_changes_the_status_of_a_finding_raised_in_this_round() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let f1 = add(&desk, &review, "a preference with no consequence", "");

    let document = desk.json(&[
        "finding",
        "became",
        "--review",
        &review,
        "--finding",
        &f1,
        "--status",
        "withdrawn",
        "--by",
        "developer",
        "--checked",
        "a preference, and no rule names it",
    ]);

    assert_eq!(document["finding"]["round_status"], "withdrawn");
    assert_eq!(document["rounds_open"], 0, "it is not open any more");
}

#[test]
fn an_unknown_status_names_the_ones_it_would_have_taken() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let f1 = add(&desk, &review, "a finding", "");

    let document = desk.refused_json(&[
        "finding",
        "became",
        "--review",
        &review,
        "--finding",
        &f1,
        "--status",
        "resolved",
        "--by",
        "agent",
    ]);

    assert_eq!(document["error"]["code"], "invalid_value");
    let message = document["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("still_open") && message.contains("ticketed"),
        "a caller that rejects input has to say what it would have accepted: {document}"
    );
}

#[test]
fn signing_off_is_still_not_on_the_command_line() {
    let desk = Desk::new();

    // The project's rule, checked against the binary rather than remembered: nothing added
    // for rounds confirms a brief, approves a draft or decides a review.
    for arguments in [
        ["brief", "confirm"],
        ["draft", "approve"],
        ["round", "approve"],
        ["round", "sign-off"],
        ["finding", "approve"],
    ] {
        let output = desk.command().args(arguments).assert().failure();
        let said = format!(
            "{}{}",
            String::from_utf8(output.get_output().stderr.clone()).expect("text"),
            String::from_utf8(output.get_output().stdout.clone()).expect("text"),
        );
        assert!(
            said.contains("unrecognized subcommand") || said.contains("has been removed"),
            "`{}` must not sign anything off: {said}",
            arguments.join(" ")
        );
    }
}

#[test]
fn round_commits_records_what_changed_since_the_previous_round() {
    let desk = Desk::new();
    record_responses(desk.dir());
    let bin = fake_gh(desk.dir());

    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    assert!(!first.is_empty());
    let second = open_review(&desk, "acme/portal", "412", "9d02e71");

    let output = desk
        .command()
        .env("PATH", bin.path())
        .args(["round", "commits", "--review", &second, "--json"])
        .assert()
        .success();
    let text = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let document: serde_json::Value = serde_json::from_str(&text).expect("one document");

    assert_eq!(
        document["base"], "3f9c2ab",
        "the previous round's head commit"
    );
    assert_eq!(document["commits"][0]["sha"], "a41c9e2");
    assert_eq!(
        document["commits"][0]["summary"], "Send an idempotency key with each delivery",
        "the message's first line and not its body: {document}"
    );
    assert_eq!(document["commits"][0]["position"], 0);
    assert_eq!(
        document["commits"][1]["files"][1], "tests/WebhookRetryTest.php",
        "the files each commit touched, read one commit at a time"
    );

    // And it reads back, which is what the page loads.
    let timeline = desk.json(&["round", "list", "--repo", "acme/portal", "--pr", "412"]);
    assert_eq!(
        timeline["rounds"][0]["commits"], 0,
        "round 1 had none recorded"
    );
    assert_eq!(timeline["rounds"][1]["commits"], 2);
}

#[test]
fn round_commits_on_a_first_round_says_what_it_needs_rather_than_reading_github() {
    let desk = Desk::new();
    record_responses(desk.dir());
    let bin = fake_gh(desk.dir());

    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let output = desk
        .command()
        .env("PATH", bin.path())
        .args(["round", "commits", "--review", &first, "--json"])
        .assert()
        .code(1);
    let text = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(
        text.contains("--base"),
        "it says what would let it answer, and made no GitHub read: {text}"
    );
}

#[test]
fn round_list_reports_each_rounds_findings_by_status() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let f1 = add(&desk, &first, "no idempotency key", "");
    let f2 = add(&desk, &first, "the backoff has no ceiling", "");

    let second = open_review(&desk, "acme/portal", "412", "9d02e71");
    desk.run(&[
        "finding",
        "carry",
        "--review",
        &second,
        "--from",
        &first,
        "--finding",
        &f1,
        "--status",
        "fixed",
        "--by",
        "agent",
    ]);
    desk.run(&[
        "finding",
        "carry",
        "--review",
        &second,
        "--from",
        &first,
        "--finding",
        &f2,
        "--status",
        "still_open",
        "--by",
        "agent",
    ]);
    add(&desk, &second, "the key is different on every retry", "");

    let document = desk.json(&["round", "list", "--repo", "acme/portal", "--pr", "412"]);
    let two = &document["rounds"][1];

    assert_eq!(two["round"], 2);
    assert_eq!(two["findings"], 3);
    assert_eq!(two["statuses"]["fixed"], 1);
    assert_eq!(two["statuses"]["still_open"], 1);
    assert_eq!(
        two["statuses"]["new"], 1,
        "one fixed, one still open, one new — the sentence the record could not say: \
         {document}"
    );

    let line = desk.run(&["round", "list", "--repo", "acme/portal", "--pr", "412"]);
    assert!(line.contains("1 fixed, 1 still open, 1 new"), "{line}");
}

#[test]
fn round_list_on_a_pull_request_nobody_has_reviewed_says_so() {
    let desk = Desk::new();
    let line = desk.run(&["round", "list", "--repo", "acme/portal", "--pr", "999"]);
    assert_eq!(line, "no reviews of acme/portal#999");
}
