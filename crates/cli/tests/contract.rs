//! The contract every command holds to, whatever it does.
//!
//! Exit 0 is done, 1 is refused, 2 is broken — the split Locus's review worker depends
//! on, because it treats 2 as a loud warning that never blocks a review. And with
//! `--json`, standard output is one JSON document, on a refusal as much as on success.

mod common;

use common::{open_review, Desk};
use predicates::prelude::*;
use review_desk_store::{Actor, Disposition, OutcomeKind, Severity};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn an_invalid_severity_exits_one_and_names_the_valid_severities() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let output = desk
        .command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({ "lens": "correctness", "severity": "catastrophic", "claim": "x" }).to_string(),
        )
        .assert()
        .code(1);

    let message = String::from_utf8(output.get_output().stderr.clone()).expect("text");
    for valid in Severity::ALL {
        assert!(
            message.contains(valid.as_str()),
            "{} is missing from: {message}",
            valid.as_str()
        );
    }
}

#[test]
fn an_invalid_disposition_exits_one_and_names_the_valid_dispositions() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&[
        "finding",
        "set",
        "--review",
        &review,
        "--finding",
        "1",
        "--disposition",
        "deferred",
    ]);

    assert_eq!(document["error"]["code"], "invalid_value");
    let message = document["error"]["message"].as_str().expect("a message");
    for valid in Disposition::ALL {
        assert!(message.contains(valid.as_str()), "{message}");
    }
}

#[test]
fn an_invalid_outcome_kind_exits_one_and_names_the_valid_kinds() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&[
        "outcome",
        "add",
        "--review",
        &review,
        "--finding",
        "1",
        "--kind",
        "ignored",
        "--by",
        "developer",
    ]);

    assert_eq!(document["error"]["code"], "invalid_value");
    let message = document["error"]["message"].as_str().expect("a message");
    for valid in OutcomeKind::ALL {
        assert!(message.contains(valid.as_str()), "{message}");
    }
}

#[test]
fn an_invalid_actor_exits_one_and_names_the_valid_actors() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&[
        "outcome",
        "add",
        "--review",
        &review,
        "--finding",
        "1",
        "--kind",
        "cut",
        "--by",
        "reviewer",
    ]);

    let message = document["error"]["message"].as_str().expect("a message");
    for valid in Actor::ALL {
        assert!(message.contains(valid.as_str()), "{message}");
    }
}

#[test]
fn a_field_the_record_does_not_have_is_refused_rather_than_dropped() {
    // A misspelled optional field would otherwise deserialise to None, and the record
    // would quietly lose what the review actually wrote.
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let output = desk
        .command()
        .args(["brief", "put", "--review", &review, "--file", "-", "--json"])
        .write_stdin(json!({ "problem": "x", "optionmap": "a or b" }).to_string())
        .assert()
        .code(1);
    let document: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");

    assert_eq!(document["error"]["code"], "bad_input");
    assert!(document["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("optionmap"));
}

#[test]
fn a_file_that_is_not_there_is_refused_not_reported_as_broken() {
    // The caller's mistake, not Review Desk being unable to run: 1, not 2.
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        "/nowhere/at/all/brief.json",
    ]);

    assert_eq!(document["error"]["code"], "bad_input");
}

#[test]
fn a_document_that_is_not_json_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin("this is not JSON")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("standard input"));
}

#[test]
fn a_database_that_cannot_be_opened_exits_two_for_a_review_command() {
    // A directory is not a database. Review Desk cannot run, which is 2 — and the JSON
    // document says broken rather than refused, so a caller reading only standard output
    // learns the same thing.
    let dir = TempDir::new().unwrap();

    let output = assert_cmd::Command::cargo_bin("review-desk")
        .unwrap()
        .env("REVIEW_DESK_DB", dir.path())
        .args([
            "review",
            "open",
            "--repo",
            "devergehq/review-desk",
            "--pr",
            "7",
            "--head",
            "c0ffee1",
            "--json",
        ])
        .assert()
        .code(2);

    let document: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");
    assert_eq!(document["error"]["code"], "cannot_open_database");
    assert_eq!(document["error"]["kind"], "broken");
}

#[test]
fn a_usage_error_with_json_is_still_one_document() {
    // clap would otherwise render its own text to standard error and leave a caller that
    // asked for JSON parsing nothing.
    let desk = Desk::new();

    let output = desk
        .command()
        .args([
            "review",
            "open",
            "--repo",
            "devergehq/review-desk",
            "--json",
        ])
        .assert()
        .code(1);

    let document: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");
    assert_eq!(document["error"]["code"], "usage");
    assert_eq!(document["error"]["kind"], "refused");
}

#[test]
fn every_command_prints_exactly_one_json_document() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(json!({ "problem": "x" }).to_string())
        .assert()
        .success();
    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(json!({ "lens": "l", "severity": "nit", "claim": "c" }).to_string())
        .assert()
        .success();

    // Every command that takes no document on standard input. The two that do are
    // checked above, and `brief put --json` in tests/review.rs.
    let commands: [&[&str]; 9] = [
        &["db", "path"],
        &["db", "version"],
        &[
            "review",
            "open",
            "--repo",
            "devergehq/review-desk",
            "--pr",
            "7",
            "--head",
            "c0ffee1",
        ],
        &["review", "link", "--review", &review, "--session", "s-1"],
        &["review", "show", "--review", &review],
        &["brief", "confirm", "--review", &review],
        &[
            "finding",
            "set",
            "--review",
            &review,
            "--finding",
            "1",
            "--disposition",
            "held_back",
            "--reason",
            "r",
        ],
        &[
            "outcome",
            "add",
            "--review",
            &review,
            "--finding",
            "1",
            "--kind",
            "cut",
            "--by",
            "developer",
        ],
        &["rules", "list", "--repo", "devergehq/review-desk"],
    ];

    for command in commands {
        let document = desk.json(command);
        assert!(
            document.is_object(),
            "{command:?} must print one JSON object, got {document}"
        );
    }
}

#[test]
fn the_help_text_names_every_value_the_store_accepts() {
    // The help text lists the closed sets by hand, because clap's derive cannot call a
    // function in an attribute. This is what keeps that list from drifting.
    let desk = Desk::new();

    let dispositions = desk.run(&["finding", "set", "--help"]);
    for valid in Disposition::ALL {
        assert!(
            dispositions.contains(valid.as_str()),
            "finding set --help does not name {}",
            valid.as_str()
        );
    }

    let outcomes = desk.run(&["outcome", "add", "--help"]);
    for valid in OutcomeKind::ALL {
        assert!(
            outcomes.contains(valid.as_str()),
            "outcome add --help does not name {}",
            valid.as_str()
        );
    }
    for valid in Actor::ALL {
        assert!(
            outcomes.contains(valid.as_str()),
            "outcome add --help does not name {}",
            valid.as_str()
        );
    }
}

#[test]
fn a_severity_of_the_wrong_type_still_names_the_valid_severities() {
    // serde names a closed set's values for an unknown *string*, but says only "expected
    // value" for `null` and only "missing field" when it is absent. All three are an
    // invalid severity, and DEV-851 asks for the valid values in each case.
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    for body in [
        json!({ "lens": "l", "severity": null, "claim": "x" }),
        json!({ "lens": "l", "severity": 3, "claim": "x" }),
        json!({ "lens": "l", "claim": "x" }),
    ] {
        let output = desk
            .command()
            .args([
                "finding", "add", "--review", &review, "--file", "-", "--json",
            ])
            .write_stdin(body.to_string())
            .assert()
            .code(1);
        let document: serde_json::Value =
            serde_json::from_slice(&output.get_output().stdout).expect("one document");
        let message = document["error"]["message"].as_str().expect("a message");

        for valid in Severity::ALL {
            assert!(
                message.contains(valid.as_str()),
                "{body} was refused without naming {}: {message}",
                valid.as_str()
            );
        }
    }
}

#[test]
fn a_file_that_cannot_be_read_is_not_told_about_severities() {
    // The valid values belong to a refusal from the parse, not from the read. Bolted
    // onto "no such file or directory" they point the reader at the wrong mistake.
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&[
        "finding",
        "add",
        "--review",
        &review,
        "--file",
        "/nowhere/at/all/finding.json",
    ]);

    let message = document["error"]["message"].as_str().expect("a message");
    assert!(
        message.contains("/nowhere/at/all/finding.json"),
        "{message}"
    );
    assert!(
        !message.contains("severity is one of"),
        "a file that cannot be read is not a bad severity: {message}"
    );
}

#[test]
fn a_finding_with_a_line_but_no_file_still_shows_the_line() {
    // The store makes `file` and `line` independently optional, and a line is worth
    // saying even when nothing says where. The plain view used to print `-` and lose it.
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({ "lens": "correctness", "severity": "nit", "claim": "a line with no file", "line": 12 })
                .to_string(),
        )
        .assert()
        .success();

    let shown = desk.run(&["review", "show", "--review", &review]);

    assert!(
        shown.contains("line 12"),
        "the line number must survive into the only view a person reads:\n{shown}"
    );
}

#[test]
fn one_repository_spelled_two_ways_opens_one_review() {
    // Verified through the binary, because this is what an agent actually does: one
    // caller runs `gh repo view --json nameWithOwner`, another types the name.
    let desk = Desk::new();

    let first = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");
    let second = open_review(&desk, "DevergeHQ/Review-Desk", "7", "C0FFEE1");

    assert_eq!(
        first, second,
        "two spellings of one repository and commit must be one review"
    );

    let shown = desk.json(&["review", "show", "--review", &first]);
    assert_eq!(shown["review"]["repo"], "devergehq/review-desk");
    assert_eq!(shown["review"]["head_sha"], "c0ffee1");
    assert_eq!(shown["review"]["round"], 1);
}

#[test]
fn an_empty_brief_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let output = desk
        .command()
        .args(["brief", "put", "--review", &review, "--file", "-", "--json"])
        .write_stdin(json!({ "problem": "" }).to_string())
        .assert()
        .code(1);
    let document: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");

    assert_eq!(document["error"]["code"], "invalid_value");
}
