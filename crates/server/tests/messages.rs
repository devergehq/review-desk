//! The conversation on a review, over HTTP. DEV-1141.

mod common;

use common::{seed, start_with_no_github};

#[test]
fn the_developer_asks_and_the_question_waits_to_be_delivered() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);
    let path = format!("/reviews/{}/messages", seeded.review_id);

    let asked = harness.api(
        "POST",
        &path,
        Some(r#"{"step":"code_review","body":"  Why is F2 a nit?  "}"#),
    );
    assert_eq!(asked.status, 201, "{}", asked.text());
    let message = asked.json();
    assert_eq!(message["author"], "developer");
    assert_eq!(message["kind"], "ask");
    assert_eq!(message["step"], "code_review");
    assert_eq!(message["body"], "Why is F2 a nit?");
    assert!(message["read_at"].is_null());

    let unread = harness
        .store
        .unread_messages(Some(seeded.review_id))
        .expect("unread");
    assert_eq!(unread.len(), 1);
}

#[test]
fn the_request_cannot_choose_who_wrote_it_or_what_kind_it_is() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);
    let path = format!("/reviews/{}/messages", seeded.review_id);

    for body in [
        r#"{"step":"draft","body":"hi","author":"session"}"#,
        r#"{"step":"draft","body":"hi","kind":"reply"}"#,
        r#"{"step":"draft","body":"hi","kind":"approve"}"#,
    ] {
        let refused = harness.api("POST", &path, Some(body));
        assert_eq!(refused.status, 400, "{body}: {}", refused.text());
    }
    assert!(harness
        .store
        .messages(seeded.review_id)
        .expect("messages")
        .is_empty());
}

#[test]
fn an_empty_message_or_a_step_that_is_not_one_is_refused() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);
    let path = format!("/reviews/{}/messages", seeded.review_id);

    for body in [
        r#"{"step":"draft","body":"   "}"#,
        r#"{"step":"posting","body":"hi"}"#,
    ] {
        let refused = harness.api("POST", &path, Some(body));
        assert_eq!(refused.status, 400, "{body}: {}", refused.text());
        assert_eq!(refused.error_code(), "invalid_value");
    }
}

#[test]
fn a_send_back_keeps_its_note_in_the_conversation() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);

    let sent = harness.api(
        "POST",
        &format!("/reviews/{}/draft/send-back", seeded.review_id),
        Some(r#"{"note":"Regrade F13 to Blocker."}"#),
    );
    assert_eq!(sent.status, 200, "{}", sent.text());

    let list = harness
        .api(
            "GET",
            &format!("/reviews/{}/messages", seeded.review_id),
            None,
        )
        .json();
    let messages = list["messages"].as_array().expect("a list");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["kind"], "send_back");
    assert_eq!(messages[0]["step"], "draft");
    assert_eq!(messages[0]["body"], "Regrade F13 to Blocker.");
}

#[test]
fn a_refused_send_back_leaves_no_message() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);

    let refused = harness.api(
        "POST",
        &format!("/reviews/{}/draft/send-back", seeded.review_id),
        Some(r#"{"note":"   "}"#),
    );
    assert_eq!(refused.status, 400);
    assert!(harness
        .store
        .messages(seeded.review_id)
        .expect("messages")
        .is_empty());
}
