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
