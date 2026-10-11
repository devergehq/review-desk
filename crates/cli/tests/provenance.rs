//! `provenance sources`, `criteria`, `changes`, `lint` and `show`, through the binary.
//!
//! One review, board A2's own record, written entirely from the command line — which is
//! DEV-875's first acceptance criterion and the only way any of it is ever written.

mod common;

use common::{open_review, Desk};
use serde_json::Value;

/// Write a JSON document to a file in the desk's directory and hand back its path.
fn document(desk: &Desk, name: &str, json: &str) -> String {
    let path = desk.dir().join(name);
    std::fs::write(&path, json).expect("write the document");
    path.display().to_string()
}

const SOURCES: &str = r#"{"sources": [
  {"kind": "ticket", "label": "ABC-231 · Retry webhook deliveries for 24 hours",
   "link": "https://linear.app/acme/issue/ABC-231"},
  {"kind": "description", "label": "The pull request description"},
  {"kind": "comment", "label": "2 earlier comments on the pull request"}
]}"#;

const LINT: &str = r#"{"results": [
  {"label": "Description length", "status": "pass", "detail": "1,840 of 2,568 characters"},
  {"label": "Working notes", "status": "pass", "detail": "In a comment, linked from the description"},
  {"label": "Template", "status": "pass", "detail": "All 5 headings present"}
]}"#;

const CRITERIA: &str = r#"{"criteria": [
  {"text": "Failed deliveries are retried for up to 24 hours.", "status": "met",
   "file": "jobs/DeliverWebhook.php", "line": 61},
  {"text": "A customer never processes the same event twice.", "status": "not_met",
   "finding_seq": 1},
  {"text": "The customer can see which deliveries failed.", "status": "not_in_this_pr",
   "note": "The description says it follows in ABC-240"}
]}"#;

const CHANGES: &str = r#"{"files": [
  {"path": "jobs/DeliverWebhook.php",
   "summary": "The job now releases itself back to the queue when a delivery fails.",
   "lines_added": 96, "lines_removed": 22, "checked": "read_in_full",
   "how_checked": "Read in full. Compared with 2 sibling senders."},
  {"path": "tests/WebhookRetryTest.php",
   "summary": "A new test: a failed delivery is sent again.",
   "lines_added": 61, "checked": "run",
   "how_checked": "Run. Run again with the fix reverted, and it failed."},
  {"path": "config/webhooks.php", "summary": "Adds the backoff schedule.",
   "lines_added": 14, "checked": "read_in_full", "how_checked": "Read in full."},
  {"path": "migrations/add_attempts.php",
   "summary": "Adds an attempts column to deliveries.",
   "lines_added": 21, "checked": "read_in_full", "how_checked": "Read in full."},
  {"path": "resources/admin/**", "summary": "Not changed by this pull request.",
   "checked": "not_reviewed",
   "how_checked": "Not reviewed. The change does not touch it."}
]}"#;

/// Board A2's review, written the way a session writes it: findings before criteria,
/// because a criterion recorded as not met names one.
fn seeded(desk: &Desk) -> String {
    let review = open_review(desk, "acme/portal", "412", "3f9c2ab");

    for finding in [
        r#"{"lens":"sibling diff","severity":"blocker","file":"jobs/DeliverWebhook.php","line":58,"claim":"A retried delivery is sent without an idempotency key"}"#,
        r#"{"lens":"boundaries","severity":"should","file":"jobs/DeliverWebhook.php","line":91,"claim":"The backoff schedule has no ceiling"}"#,
        r#"{"lens":"duplication","severity":"nit","file":"config/webhooks.php","line":4,"claim":"Backoff schedule is repeated in two files"}"#,
    ] {
        let path = document(desk, &format!("f{}.json", finding.len()), finding);
        desk.run(&["finding", "add", "--review", &review, "--file", &path]);
    }

    for (name, body, command) in [
        ("sources.json", SOURCES, "sources"),
        ("lint.json", LINT, "lint"),
        ("criteria.json", CRITERIA, "criteria"),
        ("changes.json", CHANGES, "changes"),
    ] {
        let path = document(desk, name, body);
        desk.run(&["provenance", command, "--review", &review, "--file", &path]);
    }

    review
}

#[test]
fn the_whole_record_board_a2_shows_is_written_from_the_command_line() {
    let desk = Desk::new();
    let review = seeded(&desk);

    let shown = desk.json(&["provenance", "show", "--review", &review]);
    let provenance = &shown["provenance"];

    assert_eq!(provenance["sources"].as_array().unwrap().len(), 3);
    assert_eq!(provenance["sources"][0]["kind"], "ticket");
    assert_eq!(
        provenance["sources"][0]["link"],
        "https://linear.app/acme/issue/ABC-231"
    );
    assert_eq!(provenance["sources"][1]["kind"], "description");
    assert_eq!(provenance["sources"][2]["kind"], "comment");

    assert_eq!(provenance["description_lint"].as_array().unwrap().len(), 3);
    assert_eq!(
        provenance["description_lint"][0]["label"],
        "Description length"
    );
    assert_eq!(
        shown["description_lint_passes"], true,
        "derived from the three results, not stored"
    );

    let criteria = provenance["criteria"].as_array().unwrap();
    assert_eq!(criteria.len(), 3);
    assert_eq!(criteria[0]["status"], "met");
    assert_eq!(criteria[0]["line"], 61);
    assert_eq!(criteria[1]["status"], "not_met");
    assert_eq!(criteria[1]["finding_seq"], 1);
    assert_eq!(criteria[2]["status"], "not_in_this_pr");
    assert_eq!(
        criteria[2]["note"],
        "The description says it follows in ABC-240"
    );

    let files = provenance["files"].as_array().unwrap();
    assert_eq!(files.len(), 5);
    assert_eq!(files[0]["lines_added"], 96);
    assert_eq!(files[0]["lines_removed"], 22);
    assert_eq!(files[1]["checked"], "run");
    assert_eq!(files[4]["path"], "resources/admin/**");
    assert_eq!(files[4]["checked"], "not_reviewed");

    // The findings on each file, read from the findings themselves.
    assert_eq!(
        shown["findings_by_path"]["jobs/DeliverWebhook.php"],
        Value::Array(vec![1.into(), 2.into()])
    );
    assert_eq!(
        shown["findings_by_path"]["config/webhooks.php"],
        Value::Array(vec![3.into()])
    );
    assert_eq!(
        shown["findings_by_path"]["resources/admin/**"],
        Value::Array(Vec::new()),
        "an area that was not reviewed has no findings on it"
    );
}

#[test]
fn review_show_prints_all_four_alongside_the_rest_of_the_record() {
    let desk = Desk::new();
    let review = seeded(&desk);

    let shown = desk.json(&["review", "show", "--review", &review]);

    assert_eq!(shown["provenance"]["sources"].as_array().unwrap().len(), 3);
    assert_eq!(shown["provenance"]["criteria"].as_array().unwrap().len(), 3);
    assert_eq!(shown["provenance"]["files"].as_array().unwrap().len(), 5);
    assert_eq!(
        shown["provenance"]["description_lint"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(shown["description_lint_passes"], true);

    // And the plain form, which is what a person reads.
    let plain = desk.run(&["review", "show", "--review", &review]);
    assert!(plain.contains("read from: 3"), "{plain}");
    assert!(plain.contains("what the ticket asks for: 3"), "{plain}");
    assert!(plain.contains("what they built: 5"), "{plain}");
    assert!(plain.contains("not_met"), "{plain}");
    assert!(
        plain.contains("resources/admin/** +0 -0 not_reviewed"),
        "the area that was not reviewed is in the table: {plain}"
    );
    assert!(
        plain.contains("findings: F1, F2"),
        "and the findings are on their file: {plain}"
    );
}

#[test]
fn the_layer_trails_outcome_line_round_trips_as_it_already_did() {
    // DEV-875 lists "a short outcome line per layer" among what a review gains. It
    // already has one — `review_layers.detail`, written by `layer set` — so nothing was
    // added for it, and this is the test that says so rather than a second field that
    // could disagree with the first.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    for (layer, state, detail) in [
        ("1", "done", "Description passes"),
        ("2", "done", "1 problem, 3 criteria"),
        ("3", "done", "3 ways to solve it"),
        ("4", "done", "They built B, in 4 files"),
        ("5", "done", "3 findings"),
        ("6", "waiting_on_you", "Waiting on you"),
        ("7", "pending", "After it is posted"),
    ] {
        desk.run(&[
            "layer", "set", "--review", &review, "--layer", layer, "--state", state, "--detail",
            detail,
        ]);
    }

    let layers = desk.json(&["review", "show", "--review", &review])["layers"]
        .as_array()
        .expect("seven layers")
        .clone();

    assert_eq!(layers.len(), 7);
    assert_eq!(layers[0]["detail"], "Description passes");
    assert_eq!(layers[3]["detail"], "They built B, in 4 files");
    assert_eq!(layers[6]["detail"], "After it is posted");
}

#[test]
fn a_review_that_recorded_none_of_it_shows_nothing_rather_than_failing() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let shown = desk.json(&["provenance", "show", "--review", &review]);
    assert_eq!(shown["provenance"]["sources"], Value::Array(Vec::new()));
    assert_eq!(shown["provenance"]["criteria"], Value::Array(Vec::new()));
    assert_eq!(shown["provenance"]["files"], Value::Array(Vec::new()));
    assert_eq!(
        shown["description_lint_passes"],
        Value::Null,
        "a lint that has not run has no verdict, which is not the same as failing"
    );

    assert_eq!(
        desk.run(&["provenance", "show", "--review", &review]),
        format!("review {review} provenance\n  nothing recorded")
    );
}

#[test]
fn a_criterion_not_met_with_no_finding_is_refused_and_says_which_one() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let path = document(
        &desk,
        "criteria.json",
        r#"{"criteria": [
             {"text": "retried for 24 hours", "status": "met"},
             {"text": "never processed twice", "status": "not_met"}
           ]}"#,
    );

    let refused = desk.refused_json(&[
        "provenance",
        "criteria",
        "--review",
        &review,
        "--file",
        &path,
    ]);

    assert_eq!(refused["error"]["code"], "invalid_value");
    assert_eq!(refused["error"]["kind"], "refused");
    let message = refused["error"]["message"].as_str().expect("a message");
    assert!(
        message.contains("criterion 2"),
        "the refusal says which criterion: {message}"
    );
    assert!(
        message.contains("raise the finding first"),
        "and what to do about it: {message}"
    );
}

#[test]
fn an_area_that_was_not_reviewed_with_no_reason_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let path = document(
        &desk,
        "changes.json",
        r#"{"files": [{"path": "resources/admin/**",
                      "summary": "Not changed by this pull request.",
                      "checked": "not_reviewed"}]}"#,
    );

    let refused = desk.refused_json(&[
        "provenance",
        "changes",
        "--review",
        &review,
        "--file",
        &path,
    ]);

    assert_eq!(refused["error"]["code"], "invalid_value");
    assert!(refused["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("why this was not reviewed"));
}

#[test]
fn an_unknown_value_in_a_closed_set_is_refused_with_the_valid_ones_named() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    for (command, body, values) in [
        (
            "sources",
            r#"{"sources": [{"kind": "spreadsheet", "label": "x"}]}"#,
            "ticket, description, comment",
        ),
        (
            "criteria",
            r#"{"criteria": [{"text": "x", "status": "partly"}]}"#,
            "met, not_met, not_in_this_pr",
        ),
        (
            "changes",
            r#"{"files": [{"path": "a.rs", "summary": "x", "checked": "skimmed"}]}"#,
            "read_in_full, read_in_part, run, not_reviewed",
        ),
        (
            "lint",
            r#"{"results": [{"label": "Template", "status": "warning"}]}"#,
            "pass, warn, fail",
        ),
    ] {
        let path = document(&desk, &format!("{command}-bad.json"), body);
        let refused =
            desk.refused_json(&["provenance", command, "--review", &review, "--file", &path]);

        assert_eq!(refused["error"]["code"], "bad_input", "{command}");
        let message = refused["error"]["message"].as_str().expect("a message");
        assert!(
            message.contains(values),
            "{command} must name what it would have accepted: {message}"
        );
    }
}

#[test]
fn a_misspelled_top_level_key_is_refused_rather_than_clearing_the_record() {
    let desk = Desk::new();
    let review = seeded(&desk);

    // The danger this guards: an empty list clears the record, so a document whose one
    // key is misspelled would deserialise to nothing and silently wipe three sources.
    let path = document(&desk, "typo.json", r#"{"sauces": []}"#);
    let refused = desk.refused_json(&[
        "provenance",
        "sources",
        "--review",
        &review,
        "--file",
        &path,
    ]);

    assert_eq!(refused["error"]["code"], "bad_input");
    assert_eq!(
        desk.json(&["provenance", "show", "--review", &review])["provenance"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "and the record still holds what it held"
    );
}

#[test]
fn an_unknown_review_is_refused_rather_than_read_as_empty() {
    let desk = Desk::new();
    open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let refused = desk.refused_json(&["provenance", "show", "--review", "99"]);
    assert_eq!(refused["error"]["code"], "not_found");

    let path = document(&desk, "sources.json", SOURCES);
    let refused = desk.refused_json(&["provenance", "sources", "--review", "99", "--file", &path]);
    assert_eq!(refused["error"]["code"], "unknown_parent");
}

#[test]
fn writing_a_list_again_replaces_it() {
    let desk = Desk::new();
    let review = seeded(&desk);

    let path = document(
        &desk,
        "fewer.json",
        r#"{"sources": [{"kind": "description", "label": "The pull request description"}]}"#,
    );
    let written = desk.json(&[
        "provenance",
        "sources",
        "--review",
        &review,
        "--file",
        &path,
    ]);

    assert_eq!(written["sources"].as_array().unwrap().len(), 1);
    assert_eq!(written["sources"][0]["position"], 1);
}

#[test]
fn the_description_lint_fails_when_one_of_its_results_does() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let path = document(
        &desk,
        "failing.json",
        r#"{"results": [
             {"label": "Template", "status": "warn", "detail": "4 of 5 headings present"},
             {"label": "Description length", "status": "fail", "detail": "5,900 of 2,568 characters"}
           ]}"#,
    );

    let written = desk.json(&["provenance", "lint", "--review", &review, "--file", &path]);

    assert_eq!(
        written["passes"], false,
        "one failure fails it, and no stored verdict can say otherwise"
    );
    assert_eq!(
        desk.run(&["provenance", "lint", "--review", &review, "--file", &path]),
        format!("review {review}: 2 results, description lint fails")
    );
}

#[test]
fn nothing_here_confirms_approves_or_signs_anything_off() {
    // **Only the developer signs off, in the dashboard** — Patrick, 9 October 2026. This
    // change adds five subcommands, and the guard against one of them ever becoming a
    // decision is a test that reads the help rather than a comment that asks nicely.
    let desk = Desk::new();
    let help = desk.run(&["provenance", "--help"]);

    for forbidden in ["confirm", "approve", "sign", "decide", "post"] {
        assert!(
            !help.to_lowercase().contains(forbidden),
            "`provenance` must not offer anything that {forbidden}s: {help}"
        );
    }

    assert!(help.contains("sources"));
    assert!(help.contains("criteria"));
    assert!(help.contains("changes"));
    assert!(help.contains("lint"));
    assert!(help.contains("show"));
}

/* ------------------------------- DEV-917's two writes ------------------------------- */

/// `layer set --fact`, which is where the facts the step's card draws come from.
#[test]
fn a_steps_facts_are_written_from_the_command_line_and_survive_the_next_progress_report() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let written = desk.json(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "3",
        "--state",
        "done",
        "--detail",
        "8 options, C built",
        "--fact",
        "On the ticket=3",
        "--fact",
        "Blind pass picked=C, without the second queue",
    ]);

    let facts = written["layer"]["facts"].as_array().expect("the facts");
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["label"], "On the ticket");
    assert_eq!(facts[0]["value"], "3");
    assert_eq!(
        facts[1]["value"], "C, without the second queue",
        "split on the first = alone, so a value may contain others"
    );

    // The call a session makes to report progress, saying nothing about the facts.
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "3",
        "--state",
        "done",
        "--detail",
        "8 options, C built",
    ]);
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        shown["layers"][2]["facts"].as_array().unwrap().len(),
        2,
        "facts cleared by a call that said nothing about them would be facts nobody could keep"
    );

    // And the way to take them off.
    let cleared = desk.json(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "3",
        "--state",
        "done",
        "--clear-facts",
    ]);
    assert_eq!(cleared["layer"]["facts"], Value::Array(Vec::new()));
}

#[test]
fn a_fact_without_an_equals_sign_is_refused_naming_the_shape_it_wanted() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let refused = desk.refused_json(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "1",
        "--state",
        "done",
        "--fact",
        "three problems",
    ]);
    assert_eq!(refused["error"]["code"], "bad_input");
    let message = refused["error"]["message"].as_str().expect("a message");
    assert!(
        message.contains("--fact") && message.contains("label=value"),
        "the refusal names the flag and the shape it wanted: {message}"
    );
}

/// A source's two reading columns, through `provenance sources`.
#[test]
fn a_sources_state_and_note_are_written_from_the_command_line() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let path = document(
        &desk,
        "sources-read.json",
        r#"{"sources": [
          {"kind": "ticket", "label": "ABC-388 · Retire the tenant slot",
           "status": "Backlog", "note": "Names the helper this change deletes"},
          {"kind": "comment", "label": "2 earlier comments on the pull request"}
        ]}"#,
    );
    let written = desk.json(&[
        "provenance",
        "sources",
        "--review",
        &review,
        "--file",
        &path,
    ]);

    let sources = written["sources"].as_array().expect("the sources");
    assert_eq!(sources[0]["status"], "Backlog");
    assert_eq!(sources[0]["note"], "Names the helper this change deletes");
    assert_eq!(
        (&sources[1]["status"], &sources[1]["note"]),
        (&Value::Null, &Value::Null),
        "a source that reported neither shows its reference alone"
    );
}
