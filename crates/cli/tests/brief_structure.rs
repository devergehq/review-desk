//! `brief put` with the twelve rows the approved boards draw, and `brief problem-statement`.
//!
//! All of it through the binary, because the command line is the only way an agent session
//! writes a brief — and the problem half is the one read DEV-883's blind pass is given.

mod common;

use common::{open_review, Desk};
use serde_json::Value;

/// A marker nothing in the problem half may carry.
const MARKER: &str = "THE-CHANGE-HALF-WAS-HERE";

/// A brief with every row filled, and the change half marked throughout.
///
/// Rows 2 and 7 — the before flow and how it works today — and the problem half of row 4
/// carry no marker. Everything else does.
fn marked_brief() -> String {
    format!(
        r#"{{
  "problem": "a webhook delivery fails once and is never tried again",
  "diagram": "flowchart LR\n  A[Event] --> B[Lost] {MARKER}",
  "approach_verdict": "B is the right shape {MARKER}",
  "provenance": "from the ticket and the code {MARKER}",

  "headline": "Deliveries move to a queue {MARKER}",
  "how_it_works_today": "A delivery is sent inside the request that caused it.",
  "flow_before_caption": "The delivery is sent inline",
  "flow_after_caption": "The delivery is queued {MARKER}",
  "flow_before": [
    [{{"title": "An event happens", "note": "A subscription changes"}}],
    [{{"title": "The endpoint answers", "note": "Logged and forgotten", "problem": 2}},
     {{"title": "The endpoint is down", "note": "Nothing retries", "problem": 1}}]
  ],
  "flow_after": [
    [{{"title": "A worker sends it {MARKER}", "note": "Retried {MARKER}", "problem": 1}}]
  ],
  "problems": [
    {{"number": 1,
     "was_wrong": "A delivery that fails is never tried again.",
     "now_fixed": "A worker retries it {MARKER}",
     "detail_title": "The delivery sent inline {MARKER}",
     "detail_before": ["The sender waits inside the request. {MARKER}"],
     "detail_after": ["A row is written first. {MARKER}"]}},
    {{"number": 2,
     "was_wrong": "Nothing records that a delivery was attempted.",
     "now_fixed": "Every attempt is a row. {MARKER}"}}
  ],
  "support_table": {{
    "title": "The idea: two questions, each with one owner {MARKER}",
    "columns": ["The question {MARKER}", "Who answers it {MARKER}"],
    "rows": [["Was it attempted? {MARKER}", "The delivery row {MARKER}"]]
  }},
  "options": [
    {{"key": "A", "title": "Retry inside the request {MARKER}",
     "argument_for": "Smallest change {MARKER}",
     "argument_against": "Cannot span 24 hours {MARKER}",
     "chosen": false, "proposed_by": "ticket",
     "verdicts": [{{"problem": 1, "verdict": "partly", "why": "covers a blip {MARKER}"}},
                  {{"problem": 2, "verdict": "stays", "why": "nothing is written {MARKER}"}}]}},
    {{"key": "B", "title": "Queue with backoff {MARKER}",
     "argument_for": "Meets the window {MARKER}",
     "argument_against": "Needs an idempotency key {MARKER}",
     "chosen": true, "proposed_by": "both",
     "verdicts": [{{"problem": 1, "verdict": "fixed", "why": "backoff to 24 hours {MARKER}"}},
                  {{"problem": 2, "verdict": "fixed", "why": "a row per attempt {MARKER}"}}]}},
    {{"key": "C", "title": "Dead-letter and replay {MARKER}",
     "argument_for": "A person can replay one {MARKER}",
     "argument_against": "Nothing happens automatically {MARKER}",
     "chosen": false, "proposed_by": "blind_pass",
     "verdicts": [{{"problem": 1, "verdict": "not_assessed"}}]}}
  ],
  "blind_pass": {{
    "given": "A web application delivers events inside the request. {MARKER}",
    "pick_key": "B",
    "pick_why": "It answers both problems. {MARKER}",
    "looked_up": "nothing {MARKER}",
    "questions": ["Must a retry be idempotent? {MARKER}"]
  }},
  "parts": [
    {{"title": "The delivery row {MARKER}",
     "summary": "One row per attempt. {MARKER}",
     "fixes": [2],
     "files": [{{"path": "app/Models/WebhookDelivery.php", "lines_added": 74}}]}},
    {{"title": "The queue worker {MARKER}",
     "summary": "Sends a delivery and retries it. {MARKER}",
     "fixes": [1, 2],
     "files": [{{"path": "jobs/DeliverWebhook.php", "lines_added": 96, "lines_removed": 31}},
               {{"path": "app/Support/Backoff.php", "in_diff": false}}]}}
  ],
  "open_choices": [
    {{"left_open": "How long a failed delivery is kept {MARKER}",
     "chosen": "Thirty days {MARKER}",
     "why": "The ticket stops at the window. {MARKER}",
     "departs_from_ticket": true,
     "finding_seq": 1}}
  ],
  "disagreements": [
    {{"description_says": "A retry carries the first key. {MARKER}",
     "code_does": "The key is generated per attempt. {MARKER}",
     "finding_seq": 1}}
  ]
}}"#
    )
}

/// A review with one finding, so the brief's pointers have something to point at.
fn seed(desk: &Desk) -> String {
    let review = open_review(desk, "acme/portal", "412", "3f9c2ab");
    let finding = desk.write_file(
        "finding.json",
        r#"{"lens": "sibling diff", "severity": "should",
            "claim": "a retried delivery has no idempotency key"}"#,
    );
    desk.run(&[
        "finding",
        "add",
        "--review",
        &review,
        "--file",
        &finding.display().to_string(),
    ]);
    review
}

/// Write the marked brief and hand back the review's id.
fn write_marked_brief(desk: &Desk) -> String {
    let review = seed(desk);
    let path = desk.write_file("brief.json", &marked_brief());
    desk.run(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        &path.display().to_string(),
    ]);
    review
}

#[test]
fn a_brief_with_every_row_goes_in_through_the_command_line_and_comes_back() {
    let desk = Desk::new();
    let review = write_marked_brief(&desk);

    let document = desk.json(&["review", "show", "--review", &review]);
    let brief = &document["brief"];

    assert_eq!(brief["flow_before"].as_array().expect("the flow").len(), 2);
    assert_eq!(
        brief["flow_before"][1]
            .as_array()
            .expect("the split row")
            .len(),
        2,
        "the split row survives as a row of two"
    );
    assert_eq!(brief["problems"].as_array().expect("the problems").len(), 2);
    assert_eq!(brief["options"].as_array().expect("the options").len(), 3);
    assert_eq!(brief["options"][1]["proposed_by"], Value::from("both"));
    assert_eq!(
        brief["options"][2]["verdicts"][0]["verdict"],
        Value::from("not_assessed")
    );
    assert_eq!(brief["parts"].as_array().expect("the parts").len(), 2);
    assert_eq!(brief["parts"][1]["files"][1]["in_diff"], Value::from(false));
    assert_eq!(
        brief["support_table"]["columns"]
            .as_array()
            .expect("columns")
            .len(),
        2
    );
    assert_eq!(brief["blind_pass"]["pick_key"], Value::from("B"));
    assert_eq!(
        brief["open_choices"].as_array().expect("the choices").len(),
        1
    );
    assert_eq!(
        brief["disagreements"]
            .as_array()
            .expect("the clashes")
            .len(),
        1
    );
}

#[test]
fn the_problem_statement_carries_nothing_from_the_change_half() {
    let desk = Desk::new();
    let review = write_marked_brief(&desk);

    let document = desk.json(&["brief", "problem-statement", "--review", &review]);
    let text = document.to_string();
    assert!(
        !text.contains(MARKER),
        "the change half reached the problem statement: {text}"
    );

    // And the problem half is there, so the assertion above is not passing on an empty
    // document.
    let statement = &document["problem_statement"];
    assert_eq!(
        statement["problems"]
            .as_array()
            .expect("the problems")
            .len(),
        2
    );
    assert_eq!(
        statement["problems"][0]["was_wrong"],
        Value::from("A delivery that fails is never tried again.")
    );
    assert_eq!(
        statement["flow_before"].as_array().expect("the flow").len(),
        2
    );
    assert_eq!(
        statement["how_it_works_today"],
        Value::from("A delivery is sent inside the request that caused it.")
    );
}

#[test]
fn the_problem_statement_as_a_person_reads_it_carries_nothing_from_the_change_half() {
    let desk = Desk::new();
    let review = write_marked_brief(&desk);

    // The plain shape is a second spelling of the same answer, so it is checked too: a
    // leak in the line a person reads is a leak.
    let plain = desk.run(&["brief", "problem-statement", "--review", &review]);
    assert!(!plain.contains(MARKER), "{plain}");
    assert!(plain.contains("A delivery that fails is never tried again."));
    assert!(plain.contains("how it works today"));
}

#[test]
fn the_problem_statement_of_a_review_with_no_brief_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let refusal = desk.refused_json(&["brief", "problem-statement", "--review", &review]);
    assert_eq!(refusal["error"]["code"], Value::from("not_found"));
    assert_eq!(refusal["error"]["kind"], Value::from("refused"));
}

#[test]
fn a_flow_box_marking_a_problem_that_is_not_there_is_refused_with_the_number() {
    let desk = Desk::new();
    let review = seed(&desk);

    let path = desk.write_file(
        "brief.json",
        r#"{"problem": "p",
            "problems": [{"number": 1, "was_wrong": "a delivery is lost"}],
            "flow_before": [[{"title": "a box marked 9", "problem": 9}]]}"#,
    );
    let refusal = desk.refused_json(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        &path.display().to_string(),
    ]);
    assert_eq!(refusal["error"]["code"], Value::from("invalid_value"));
    let message = refusal["error"]["message"].to_string();
    assert!(
        message.contains("flow_before.problem"),
        "the field is in the refusal: {message}"
    );
    assert!(
        message.contains("problem 9"),
        "and so is the number: {message}"
    );
}

#[test]
fn a_flow_row_of_four_boxes_is_refused() {
    let desk = Desk::new();
    let review = seed(&desk);

    let path = desk.write_file(
        "brief.json",
        r#"{"problem": "p", "flow_after": [[
            {"title": "one"}, {"title": "two"}, {"title": "three"}, {"title": "four"}]]}"#,
    );
    let refusal = desk.refused_json(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        &path.display().to_string(),
    ]);
    assert_eq!(refusal["error"]["code"], Value::from("invalid_value"));
    let message = refusal["error"]["message"].to_string();
    assert!(message.contains("flow_after"), "{message}");
    assert!(message.contains("one to three"), "{message}");
}

#[test]
fn a_misspelled_field_is_refused_rather_than_dropped() {
    let desk = Desk::new();
    let review = seed(&desk);

    // `deny_unknown_fields` on `NewBrief`, which is what stops a session writing twelve
    // rows into a brief that quietly holds none of them.
    let path = desk.write_file(
        "brief.json",
        r#"{"problem": "p", "head_line": "a misspelled headline"}"#,
    );
    let refusal = desk.refused_json(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        &path.display().to_string(),
    ]);
    assert_eq!(refusal["error"]["code"], Value::from("bad_input"));
}

#[test]
fn reading_the_problem_statement_confirms_nothing() {
    let desk = Desk::new();
    let review = write_marked_brief(&desk);

    desk.json(&["brief", "problem-statement", "--review", &review]);

    // **A read decides nothing**, and layer 2 is where a decision would show.
    let document = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(document["brief"]["confirmed_at"], Value::Null);
    assert_eq!(document["brief"]["confirmed_by"], Value::Null);
    let layers = document["layers"].as_array().expect("the layers");
    let at_two = layers
        .iter()
        .find(|layer| layer["number"] == 2)
        .expect("layer 2");
    assert_ne!(at_two["state"], Value::from("done"));
}
