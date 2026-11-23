//! `review-desk message`: a session reads the developer's messages, marks them read and
//! answers. DEV-1141.

mod common;

use common::{open_review, Desk};
use review_desk_store::ReviewLayerName;

#[test]
fn a_session_reads_a_question_marks_it_read_and_answers_it() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let id: i64 = review.parse().expect("an id");
    desk.store()
        .ask_session(id, ReviewLayerName::Draft, "Why is F13 a Should?")
        .expect("the developer asks");

    let unread = desk.json(&["message", "list", "--unread"]);
    let messages = unread["messages"].as_array().expect("a list");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["body"], "Why is F13 a Should?");
    assert_eq!(messages[0]["author"], "developer");

    let read = desk.json(&[
        "message",
        "read",
        "--review",
        &review,
        "--seq",
        "1",
        "--session",
        "allele-1",
    ]);
    assert_eq!(read["message"]["read_by_session"], "allele-1");
    let unread = desk.json(&["message", "list", "--unread", "--review", &review]);
    assert!(unread["messages"].as_array().expect("a list").is_empty());

    let replied = desk.json(&[
        "message",
        "reply",
        "--review",
        &review,
        "--step",
        "draft",
        "--to",
        "1",
        "--body",
        "Because nothing leaks; send it back to regrade.",
    ]);
    assert_eq!(replied["message"]["kind"], "reply");
    assert_eq!(replied["message"]["in_reply_to"], 1);

    let all = desk.json(&["message", "list", "--review", &review]);
    assert_eq!(all["messages"].as_array().expect("a list").len(), 2);
}

#[test]
fn there_is_no_command_that_writes_the_developer_s_words() {
    let desk = Desk::new();
    let help = desk.run(&["message", "--help"]);
    for verb in ["ask", "send-back", "approve"] {
        assert!(
            !help.lines().any(|line| line.trim_start().starts_with(verb)),
            "`message {verb}` exists:\n{help}"
        );
    }
}

#[test]
fn a_step_that_is_not_one_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let refused = desk.refused_json(&[
        "message", "reply", "--review", &review, "--step", "posting", "--body", "hi",
    ]);
    assert!(refused.to_string().contains("layer"), "{refused}");
}

// -- when the review's session last checked, DEV-1381 -------------------------------------

#[test]
fn a_watcher_s_pass_records_that_its_review_was_checked() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let id: i64 = review.parse().expect("an id");
    assert_eq!(desk.store().messages_checked_at(id).unwrap(), None);

    desk.json(&["message", "list", "--unread", "--review", &review]);
    assert!(desk.store().messages_checked_at(id).unwrap().is_some());
}

#[test]
fn the_bare_sweep_and_a_whole_conversation_record_no_check() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let id: i64 = review.parse().expect("an id");

    desk.json(&["message", "list", "--unread"]);
    desk.json(&["message", "list", "--review", &review]);
    assert_eq!(desk.store().messages_checked_at(id).unwrap(), None);
}

// -- the answer contract, DEV-1332 -------------------------------------------------------

const ANSWER: &str = r#"{
  "kind": "answer",
  "about": { "finding": "F9" },
  "headline": "Yes, the web path has the same hole.",
  "points": [{ "basis": "read", "text": "ExportController.php:41 passes it straight through." }],
  "cites": ["F9", "app/Http/Controllers/ExportController.php:41"],
  "suggests": [{ "type": "regrade", "finding": "F9", "from": "should", "to": "blocker",
                 "why": "Both paths leak private columns." }]
}"#;

#[test]
fn a_reply_in_the_contract_s_shape_is_kept_parsed_from_a_file() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let path = desk.write_file("answer.json", ANSWER);
    let replied = desk.json(&[
        "message",
        "reply",
        "--review",
        &review,
        "--step",
        "code_review",
        "--answer",
        path.to_str().unwrap(),
    ]);
    let message = &replied["message"];
    assert_eq!(message["parsed"], true);
    assert_eq!(message["answer"]["suggests"][0]["type"], "regrade");
    assert_eq!(message["about"]["tab"], "findings");
    assert_eq!(message["about"]["finding"], 9);
    assert_eq!(message["body"], ANSWER.trim());
}

#[test]
fn a_reply_in_the_contract_s_shape_is_read_from_standard_input() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let output = desk
        .command()
        .args([
            "message", "reply", "--review", &review, "--step", "draft", "--answer", "-", "--json",
        ])
        .write_stdin(ANSWER)
        .assert()
        .success();
    let printed: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");
    assert_eq!(printed["message"]["parsed"], true);
}

#[test]
fn a_reply_that_breaks_a_rule_exits_1_naming_the_rule_and_keeps_nothing() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let broken = ANSWER.replace("\"regrade\"", "\"approve\"");
    let output = desk
        .command()
        .args([
            "message", "reply", "--review", &review, "--step", "draft", "--answer", "-", "--json",
        ])
        .write_stdin(broken)
        .assert()
        .code(1);
    let refusal: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");
    assert_eq!(refusal["error"]["code"], "not_the_contract");
    let said = refusal["error"]["message"].as_str().unwrap();
    assert!(said.contains("suggests[0].type"), "{said}");
    assert!(said.contains("approve"), "{said}");

    let all = desk.json(&["message", "list", "--review", &review]);
    assert!(all["messages"].as_array().unwrap().is_empty());
}

#[test]
fn a_prose_body_is_still_accepted_and_kept_as_not_parsed() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let replied = desk.json(&[
        "message", "reply", "--review", &review, "--step", "draft", "--body", "Because.",
    ]);
    assert_eq!(replied["message"]["parsed"], false);
    assert!(replied["message"]["answer"].is_null());
}

#[test]
fn a_reply_takes_prose_or_the_contract_and_not_both() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    desk.refused_json(&["message", "reply", "--review", &review, "--step", "draft"]);
    desk.refused_json(&[
        "message", "reply", "--review", &review, "--step", "draft", "--body", "x", "--answer", "-",
    ]);
}
