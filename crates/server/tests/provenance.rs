//! Where a review's understanding came from, as `GET /reviews/{id}` serves it.
//!
//! The serving is thin — the command line writes all four lists and nothing here writes
//! any of them. What this file is really about is the three facts the **server** owns, and
//! the one of those that was a design decision rather than a conversion:
//!
//! * the findings on each file, derived by path;
//! * the description lint's verdict, derived from its results;
//! * **which record counts as the review's coverage**, so that a page cannot show the
//!   single "not checked" sentence and the per-file change map disagreeing.

mod common;

use review_desk_store::{
    CriterionStatus, DescriptionLintStatus, FileCheck, NewAcceptanceCriterion,
    NewDescriptionLintResult, NewFileChange, NewFinding, NewReviewSource, Severity, SourceKind,
    Store,
};
use serde_json::Value;

/// Board A2's change map, including the area that was not reviewed.
fn change_map() -> Vec<NewFileChange> {
    vec![
        NewFileChange {
            path: "jobs/DeliverWebhook.php".to_string(),
            summary: "The job releases itself back to the queue on failure.".to_string(),
            lines_added: 96,
            lines_removed: 22,
            checked: FileCheck::ReadInFull,
            how_checked: Some("Read in full. Compared with 2 sibling senders.".to_string()),
        },
        NewFileChange {
            path: "tests/WebhookRetryTest.php".to_string(),
            summary: "A new test: a failed delivery is sent again.".to_string(),
            lines_added: 61,
            lines_removed: 0,
            checked: FileCheck::Run,
            how_checked: Some("Run. Run again with the fix reverted, and it failed.".to_string()),
        },
        NewFileChange {
            path: "resources/admin/**".to_string(),
            summary: "Not changed by this pull request.".to_string(),
            lines_added: 0,
            lines_removed: 0,
            checked: FileCheck::NotReviewed,
            how_checked: Some("Not reviewed. The change does not touch it.".to_string()),
        },
    ]
}

/// Two findings on one of those files, so the derived list has more than one entry.
fn add_findings(store: &Store, review_id: i64) {
    for (file, line, claim) in [
        (
            "jobs/DeliverWebhook.php",
            58,
            "A retried delivery is sent without an idempotency key",
        ),
        (
            "jobs/DeliverWebhook.php",
            91,
            "The backoff schedule has no ceiling",
        ),
    ] {
        store
            .add_finding(
                review_id,
                NewFinding {
                    lens: "sibling diff".to_string(),
                    severity: Severity::Should,
                    file: Some(file.to_string()),
                    line: Some(line),
                    claim: claim.to_string(),
                    evidence: None,
                    disposition: review_desk_store::Disposition::InDraft,
                    // The rest from the hand-written `Default`, which is the shape every
                    // call site in these crates uses. Reading it first was the point: it
                    // gives `verification: unknown` rather than serde's first variant,
                    // `reproduced`, so a finding built here does not claim to have been run.
                    ..Default::default()
                },
            )
            .expect("a finding");
    }
}

#[test]
fn a_review_serves_the_four_lists_the_command_line_wrote() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    add_findings(&harness.store, seeded.review_id);

    harness
        .store
        .put_review_sources(
            seeded.review_id,
            vec![
                NewReviewSource {
                    kind: SourceKind::Ticket,
                    label: "ABC-231 · Retry webhook deliveries for 24 hours".to_string(),
                    link: Some("https://linear.app/acme/issue/ABC-231".to_string()),
                },
                NewReviewSource {
                    kind: SourceKind::Description,
                    label: "The pull request description".to_string(),
                    link: None,
                },
            ],
        )
        .expect("the sources");
    harness
        .store
        .put_description_lint(
            seeded.review_id,
            vec![
                NewDescriptionLintResult {
                    label: "Description length".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: Some("1,840 of 2,568 characters".to_string()),
                },
                NewDescriptionLintResult {
                    label: "Template".to_string(),
                    status: DescriptionLintStatus::Warn,
                    detail: Some("4 of 5 headings present".to_string()),
                },
            ],
        )
        .expect("the description lint");
    harness
        .store
        .put_acceptance_criteria(
            seeded.review_id,
            vec![
                NewAcceptanceCriterion {
                    text: "Failed deliveries are retried for up to 24 hours.".to_string(),
                    status: CriterionStatus::Met,
                    finding_seq: None,
                    file: Some("jobs/DeliverWebhook.php".to_string()),
                    line: Some(61),
                    note: None,
                },
                NewAcceptanceCriterion {
                    text: "A customer never processes the same event twice.".to_string(),
                    status: CriterionStatus::NotMet,
                    finding_seq: Some(1),
                    file: None,
                    line: None,
                    note: None,
                },
            ],
        )
        .expect("the criteria");
    harness
        .store
        .put_file_changes(seeded.review_id, change_map())
        .expect("the change map");

    let answer = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let detail = answer.json();

    // Read from.
    assert_eq!(detail["sources"].as_array().unwrap().len(), 2);
    assert_eq!(detail["sources"][0]["kind"], "ticket");
    assert_eq!(detail["sources"][0]["position"], 1);
    assert_eq!(
        detail["sources"][0]["link"],
        "https://linear.app/acme/issue/ABC-231"
    );
    assert_eq!(detail["sources"][1]["link"], Value::Null);

    // What the ticket asks for, with the finding's display key derived by the server.
    let criteria = detail["criteria"].as_array().unwrap();
    assert_eq!(criteria.len(), 2);
    assert_eq!(criteria[0]["status"], "met");
    assert_eq!(criteria[0]["line"], 61);
    assert_eq!(criteria[0]["finding_key"], Value::Null);
    assert_eq!(criteria[1]["status"], "not_met");
    assert_eq!(criteria[1]["finding_seq"], 1);
    assert_eq!(
        criteria[1]["finding_key"], "F1",
        "the dashboard never computes an identifier"
    );

    // The Lint section, as its individual results.
    assert_eq!(
        detail["description_lint"]["passes"], true,
        "a warning does not fail it"
    );
    let results = detail["description_lint"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["label"], "Description length");
    assert_eq!(results[1]["status"], "warn");
}

#[test]
fn the_findings_on_each_file_are_derived_from_the_findings() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    add_findings(&harness.store, seeded.review_id);
    harness
        .store
        .put_file_changes(seeded.review_id, change_map())
        .expect("the change map");

    let detail = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();
    let files = detail["coverage"]["files"]
        .as_array()
        .expect("the change map")
        .clone();
    let findings = detail["findings"].as_array().expect("the findings").clone();

    // **DEV-875's agreement, both ways round**, and stated against the served document
    // rather than against a list written into this test: a finding on a file's row exists
    // and names that row's path, and every finding naming that path is on it.
    for file in &files {
        let path = &file["path"];
        let listed = file["finding_seqs"].as_array().expect("the seqs").clone();

        for seq in &listed {
            let finding = findings
                .iter()
                .find(|finding| finding["seq"] == *seq)
                .expect("the change map names a finding that is not served");
            assert_eq!(
                finding["file"], *path,
                "a finding listed on a file must point at that file"
            );
        }

        let expected: Vec<Value> = findings
            .iter()
            .filter(|finding| finding["file"] == *path)
            .map(|finding| finding["seq"].clone())
            .collect();
        assert_eq!(
            listed, expected,
            "every finding naming {path} is on its row, in sequence order"
        );
        assert_eq!(
            file["finding_keys"],
            Value::Array(
                expected
                    .iter()
                    .map(|seq| Value::String(format!("F{seq}")))
                    .collect()
            ),
            "and the display keys follow the same list"
        );
    }

    let by_path = |path: &str| {
        files
            .iter()
            .find(|file| file["path"] == path)
            .expect("the row")
            .clone()
    };
    assert!(
        !by_path("jobs/DeliverWebhook.php")["finding_seqs"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the file the findings are on has them, or this test asserts nothing"
    );
    assert_eq!(
        by_path("tests/WebhookRetryTest.php")["finding_seqs"],
        Value::Array(Vec::new()),
        "a file with no finding has none rather than a stale one"
    );
    assert_eq!(
        by_path("resources/admin/**")["finding_seqs"],
        Value::Array(Vec::new()),
        "and an area that was not reviewed cannot have findings on it"
    );
}

#[test]
fn a_change_map_is_the_coverage_record_and_the_sentence_is_then_null() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // The seed sets the sentence, so this review starts with the record it has had since
    // DEV-854.
    let before = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();
    assert_eq!(before["coverage"]["source"], "sentence");
    assert_eq!(before["coverage"]["files"], Value::Array(Vec::new()));
    assert_eq!(
        before["not_checked"], "The admin screens, which this change does not touch.",
        "with no change map, the sentence is the coverage record and is served"
    );

    harness
        .store
        .put_file_changes(seeded.review_id, change_map())
        .expect("the change map");

    let after = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();
    assert_eq!(
        after["coverage"]["source"], "files",
        "the change map replaces the sentence as the source for coverage"
    );
    assert_eq!(after["coverage"]["files"].as_array().unwrap().len(), 3);
    assert_eq!(
        after["not_checked"],
        Value::Null,
        "and the sentence is no longer served as coverage, so a page cannot render both"
    );

    // The sentence was not destroyed. It is still on the record, which is where the
    // session that writes the draft body reads it from.
    assert_eq!(
        harness
            .store
            .get_review(seeded.review_id)
            .expect("the review")
            .not_checked
            .as_deref(),
        Some("The admin screens, which this change does not touch."),
        "the posted review still has its sentence"
    );
}

#[test]
fn a_review_with_neither_has_no_coverage_at_all() {
    let harness = common::start_with_no_github();
    let review_id = harness
        .store
        .open_review("acme/portal", 999, "deadbee")
        .expect("a review")
        .review
        .id;

    let detail = harness
        .api("GET", &format!("/reviews/{review_id}"), None)
        .json();

    assert_eq!(detail["coverage"]["source"], "none");
    assert_eq!(detail["coverage"]["files"], Value::Array(Vec::new()));
    assert_eq!(detail["not_checked"], Value::Null);
}

#[test]
fn a_review_that_recorded_none_of_it_reads_as_it_did_before() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let detail = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();

    assert_eq!(detail["sources"], Value::Array(Vec::new()));
    assert_eq!(detail["criteria"], Value::Array(Vec::new()));
    assert_eq!(
        detail["description_lint"],
        Value::Null,
        "a lint that has not run is absent, not a failure"
    );
    assert_eq!(detail["coverage"]["source"], "sentence");

    // And everything the screen read before this change is unchanged.
    assert_eq!(detail["review"]["pr"], 412);
    assert_eq!(detail["layers"].as_array().unwrap().len(), 7);
    assert!(detail["brief"]["problem"].is_string());
    assert!(detail["not_checked"].is_string());
}

#[test]
fn the_description_lint_fails_when_one_result_does() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    harness
        .store
        .put_description_lint(
            seeded.review_id,
            vec![
                NewDescriptionLintResult {
                    label: "Template".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: None,
                },
                NewDescriptionLintResult {
                    label: "Description length".to_string(),
                    status: DescriptionLintStatus::Fail,
                    detail: Some("5,900 of 2,568 characters".to_string()),
                },
            ],
        )
        .expect("the description lint");

    let lint = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json()["description_lint"]
        .clone();

    assert_eq!(lint["passes"], false);
    assert_eq!(
        lint["results"].as_array().unwrap().len(),
        2,
        "the verdict is derived from these, so it cannot contradict them"
    );
}

#[test]
fn the_layer_trail_already_carries_an_outcome_line_per_layer() {
    // DEV-875 lists "a short outcome line per layer" among what a review gains. It
    // already has one, so nothing was added: `layers[].detail`, written by
    // `review-desk layer set --detail`. A second field for the same fact is exactly the
    // disagreement `coverage.source` exists to prevent.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    harness
        .store
        .set_review_layer(
            seeded.review_id,
            4,
            review_desk_store::ReviewLayerState::Done,
            Some("They built B, in 4 files"),
        )
        .expect("layer 4");

    let layers = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json()["layers"]
        .as_array()
        .expect("seven layers")
        .clone();

    assert_eq!(layers.len(), 7);
    assert_eq!(layers[3]["detail"], "They built B, in 4 files");
    assert_eq!(layers[3]["state"], "done");
}

#[test]
fn no_operation_in_the_contract_writes_any_of_it() {
    // **Everything here is written by the agent through the command line**, so the
    // dashboard's API has no write for it. This is the test that keeps it that way: a
    // later change that adds one has to delete this assertion and say why.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for (method, path, body) in [
        ("POST", "/sources", Some(r#"{"sources":[]}"#)),
        ("PUT", "/sources", Some(r#"{"sources":[]}"#)),
        ("POST", "/criteria", Some(r#"{"criteria":[]}"#)),
        ("PUT", "/criteria", Some(r#"{"criteria":[]}"#)),
        ("PUT", "/coverage", Some(r#"{"files":[]}"#)),
        ("PUT", "/description-lint", Some(r#"{"results":[]}"#)),
    ] {
        let answer = harness.api(
            method,
            &format!("/reviews/{}{path}", seeded.review_id),
            body,
        );
        assert!(
            answer.status == 404 || answer.status == 400,
            "{method} {path} should not be an operation, got {} — {}",
            answer.status,
            answer.text()
        );
    }
}
