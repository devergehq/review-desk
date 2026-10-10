//! The developer's decisions, over HTTP.
//!
//! The store holds the rules and `crates/store/tests/composition.rs` holds them against
//! the record. This file is about the two new operations as a client meets them: the shape
//! that comes back, the status codes the contract declares, and the one thing a page has to
//! be able to tell apart — a draft written as one block of markdown, which no decision can
//! act on.

mod common;

use review_desk_store::{Disposition, DraftStatus, NitTicketStatus, RoundStatus, Severity};
use serde_json::Value;

/// `POST /reviews/{id}/findings/{seq}/decide`, as the dashboard sends it.
fn decide(harness: &common::Harness, review_id: i64, seq: i64, body: &str) -> common::Answer {
    harness.api(
        "POST",
        &format!("/reviews/{review_id}/findings/{seq}/decide"),
        Some(body),
    )
}

/// The blocks of the draft, by the finding they are about.
fn block_for(draft: &Value, key: &str) -> Value {
    draft["sections"]
        .as_array()
        .expect("the blocks")
        .iter()
        .find(|section| section["finding_key"] == key)
        .unwrap_or_else(|| panic!("no block for {key} in {draft}"))
        .clone()
}

#[test]
fn the_draft_serves_the_blocks_it_is_composed_of() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("GET", &format!("/reviews/{}/draft", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let draft = answer.json();

    let kinds: Vec<&str> = draft["sections"]
        .as_array()
        .expect("the blocks")
        .iter()
        .map(|section| section["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(kinds, ["prose", "finding", "prose"]);

    let f1 = block_for(&draft, "F1");
    assert_eq!(f1["position"], 2);
    assert_eq!(f1["severity"], "blocker");
    assert_eq!(
        f1["heading"], "### Blocker · F1 · A retried delivery is sent without an idempotency key",
        "the heading the body carries, served so the page does not compose it again"
    );
    assert_eq!(f1["posted"], true);
    assert_eq!(f1["rewritten"], false);
    assert_eq!(f1["agent_text"], f1["text"]);

    let prose = &draft["sections"][0];
    assert!(prose["finding_seq"].is_null());
    assert!(prose["heading"].is_null());
    assert_eq!(prose["posted"], true);
}

#[test]
fn cutting_a_finding_answers_with_the_finding_the_outcome_and_the_draft() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = decide(
        &harness,
        seeded.review_id,
        1,
        r#"{"decision":"cut","note":"Covered by the rule already."}"#,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    let made = answer.json();

    assert_eq!(made["finding"]["key"], "F1");
    assert_eq!(made["finding"]["disposition"], "suppressed");
    assert_eq!(made["outcome"]["kind"], "cut");
    assert_eq!(made["outcome"]["actor"], "developer");
    assert_eq!(made["outcome"]["detail"], "Covered by the rule already.");

    let draft = &made["draft"];
    let body = draft["draft"]["body"].as_str().expect("a body");
    assert!(!body.contains("F1"), "{body}");
    assert!(
        body.contains("**Not checked:**"),
        "the prose stands: {body}"
    );
    assert_eq!(
        block_for(draft, "F1")["posted"],
        false,
        "the block is still served and no longer posts"
    );
    assert!(
        draft["lint"].is_null(),
        "after a decision the lint reads as not checked: {}",
        draft["lint"]
    );
    assert!(
        draft["edits"]
            .as_array()
            .expect("the edits")
            .iter()
            .any(|edit| edit["kind"] == "cut" && edit["finding_key"] == "F1"),
        "and the decision is listed among the developer's edits: {}",
        draft["edits"]
    );
}

#[test]
fn regrading_a_finding_changes_the_heading_the_body_carries() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = decide(
        &harness,
        seeded.review_id,
        1,
        r#"{"decision":"regrade","severity":"should"}"#,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    let made = answer.json();

    assert_eq!(made["finding"]["severity"], "should");
    assert_eq!(made["outcome"]["kind"], "regraded");
    assert_eq!(made["outcome"]["detail"], "Blocker to Should");
    assert!(
        made["draft"]["draft"]["body"]
            .as_str()
            .expect("a body")
            .contains("### Should · F1 ·"),
        "{}",
        made["draft"]["draft"]["body"]
    );
    assert_eq!(
        harness
            .store
            .get_finding(seeded.review_id, 1)
            .expect("the finding")
            .severity,
        Severity::Should
    );
}

#[test]
fn rewriting_a_finding_posts_the_developers_words_and_keeps_the_agents() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = decide(
        &harness,
        seeded.review_id,
        1,
        r#"{"decision":"rewrite","title":"A retry carries no idempotency key",
            "text":"The customer receives the event twice."}"#,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    let made = answer.json();
    let block = block_for(&made["draft"], "F1");

    assert_eq!(block["title"], "A retry carries no idempotency key");
    assert_eq!(block["text"], "The customer receives the event twice.");
    assert_eq!(
        block["agent_title"], "A retried delivery is sent without an idempotency key",
        "what the rewrite replaced is served beside it"
    );
    assert_eq!(block["rewritten"], true);
    assert_eq!(made["outcome"]["kind"], "rewritten");
    assert!(made["draft"]["draft"]["body"]
        .as_str()
        .expect("a body")
        .contains("### Blocker · F1 · A retry carries no idempotency key"));
}

#[test]
fn a_decision_on_a_draft_written_as_one_block_says_so() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // A session writes the body as one block, which is what every draft before this
    // operation existed was.
    let saved = harness.api(
        "PUT",
        &format!("/reviews/{}/draft", seeded.review_id),
        Some(r#"{"body":"One block of markdown, mentioning F1 somewhere in it."}"#),
    );
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_eq!(
        saved.json()["sections"]
            .as_array()
            .expect("the blocks")
            .len(),
        1
    );

    let answer = decide(&harness, seeded.review_id, 1, r#"{"decision":"cut"}"#);
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "conflict");
    let document = answer.json();
    let message = document["message"].as_str().expect("a message");
    assert!(
        message.contains("one block"),
        "the page has to be able to say why, and this is the message it shows: {message}"
    );

    assert_eq!(
        harness
            .store
            .get_finding(seeded.review_id, 1)
            .expect("the finding")
            .disposition,
        Disposition::InDraft,
        "and nothing moved"
    );
}

#[test]
fn a_decision_after_the_sign_off_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let approved = harness.api(
        "POST",
        &format!("/reviews/{}/draft/approve", seeded.review_id),
        Some(r#"{"event":"request_changes"}"#),
    );
    assert_eq!(approved.status, 200, "{}", approved.text());

    let answer = decide(&harness, seeded.review_id, 1, r#"{"decision":"cut"}"#);
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert!(
        harness
            .store
            .get_draft(seeded.review_id)
            .expect("the draft")
            .body
            .contains("F1"),
        "an approved draft is what the session posts, and it does not move under it"
    );
}

#[test]
fn a_decision_the_parts_of_which_do_not_go_together_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let cases = [
        (r#"{"decision":"regrade"}"#, "severity"),
        (r#"{"decision":"rewrite"}"#, "text"),
        (r#"{"decision":"accept_reason"}"#, "note"),
        (r#"{"decision":"keep","severity":"nit"}"#, "severity"),
    ];

    for (body, field) in cases {
        let answer = decide(&harness, seeded.review_id, 1, body);
        assert_eq!(answer.status, 400, "{body}: {}", answer.text());
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], field, "{body}");
        assert!(
            !answer.json()["expected"].is_null(),
            "an invalid_value names what it would have taken: {body}"
        );
    }

    let unknown = decide(&harness, seeded.review_id, 1, r#"{"decision":"shelve"}"#);
    assert_eq!(unknown.status, 400);
    let document = unknown.json();
    let message = document["expected"]
        .as_str()
        .or_else(|| document["message"].as_str())
        .expect("something naming the set")
        .to_string();
    assert!(
        message.contains("keep") && message.contains("accept_reason"),
        "an unknown decision names the eight: {message}"
    );
}

#[test]
fn a_reply_to_a_carried_finding_is_recorded_as_the_developers_decision() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = decide(
        &harness,
        seeded.review_id,
        1,
        r#"{"decision":"accept_reason",
            "note":"Agreed, but I'd like to do it with the dashboard work in ABC-240."}"#,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    let made = answer.json();

    assert_eq!(made["finding"]["round_status"], "accepted");
    assert_eq!(made["finding"]["round_status_by"], "developer");
    assert!(made["finding"]["round_status_checked"]
        .as_str()
        .expect("the reason")
        .contains("ABC-240"));
    assert_eq!(
        made["outcome"]["kind"], "rejected",
        "which is what 'rejected with a reason you accepted' counts"
    );
    assert!(!made["draft"]["draft"]["body"]
        .as_str()
        .expect("a body")
        .contains("F1"));
}

#[test]
fn ticketing_what_is_left_approves_and_asks_for_the_ticket() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/draft/ticket-and-approve", seeded.review_id),
        Some(r#"{"note":"Filed as one ticket for an agent."}"#),
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    let settled = answer.json();

    assert_eq!(settled["ticketed"], serde_json::json!([1]));
    assert_eq!(settled["ticketed_keys"], serde_json::json!(["F1"]));
    assert_eq!(settled["nit_ticket"]["status"], "requested");
    assert_eq!(
        settled["nit_ticket"]["finding_seqs"],
        serde_json::json!([1, 2]),
        "the nit already held back is in the ticket too"
    );
    assert_eq!(settled["draft"]["draft"]["status"], "approved");
    assert_eq!(settled["draft"]["draft"]["event"], "approve");
    assert!(settled["draft"]["lint"].is_null());

    let body = settled["draft"]["draft"]["body"].as_str().expect("a body");
    assert!(!body.contains("### "), "no finding posts as open: {body}");

    let draft = harness
        .store
        .get_draft(seeded.review_id)
        .expect("the draft");
    assert_eq!(draft.status, DraftStatus::Approved);
    let finding = harness
        .store
        .get_finding(seeded.review_id, 1)
        .expect("the finding");
    assert_eq!(finding.disposition, Disposition::HeldBack);
    assert_eq!(finding.round_status, RoundStatus::Ticketed);
    assert_eq!(
        harness
            .store
            .nit_ticket(seeded.review_id)
            .expect("the ticket")
            .status,
        NitTicketStatus::Requested
    );
}

#[test]
fn ticketing_what_is_left_twice_is_refused_rather_than_approved_again() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let path = format!("/reviews/{}/draft/ticket-and-approve", seeded.review_id);

    assert_eq!(harness.api("POST", &path, Some("{}")).status, 200);
    let again = harness.api("POST", &path, Some("{}"));
    assert_eq!(again.status, 409, "{}", again.text());
    assert_eq!(again.error_code(), "conflict");
}

#[test]
fn ticketing_what_is_left_of_a_review_with_nothing_open_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    assert_eq!(
        decide(&harness, seeded.review_id, 1, r#"{"decision":"cut"}"#).status,
        200
    );

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/draft/ticket-and-approve", seeded.review_id),
        Some("{}"),
    );
    assert_eq!(answer.status, 400, "{}", answer.text());
    assert_eq!(
        harness
            .store
            .get_draft(seeded.review_id)
            .expect("the draft")
            .status,
        DraftStatus::Ready,
        "a refusal does not approve the draft on its way out"
    );
}

#[test]
fn a_draft_saved_in_parts_composes_the_body_the_caller_states() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PUT",
        &format!("/reviews/{}/draft", seeded.review_id),
        Some(
            r#"{"body":"The approach fits.\n\n### Blocker · F1 · A retry carries no key\n\nTwice delivered.\n",
                "sections":[
                  {"kind":"prose","text":"The approach fits."},
                  {"kind":"finding","finding_seq":1,"title":"A retry carries no key","text":"Twice delivered."}
                ]}"#,
        ),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(
        answer.json()["draft"]["body"],
        "The approach fits.\n\n### Blocker · F1 · A retry carries no key\n\nTwice delivered.\n"
    );
}

#[test]
fn a_draft_saved_with_a_body_the_blocks_do_not_compose_to_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let before = harness
        .store
        .get_draft(seeded.review_id)
        .expect("the draft")
        .body;

    let answer = harness.api(
        "PUT",
        &format!("/reviews/{}/draft", seeded.review_id),
        Some(
            r#"{"body":"Something else entirely.",
                "sections":[{"kind":"prose","text":"The approach fits."}]}"#,
        ),
    );

    assert_eq!(answer.status, 400, "{}", answer.text());
    assert_eq!(answer.error_code(), "invalid_value");
    assert_eq!(answer.json()["field"], "body");
    assert_eq!(
        harness
            .store
            .get_draft(seeded.review_id)
            .expect("the draft")
            .body,
        before,
        "and nothing was written"
    );
}

#[test]
fn a_draft_saved_with_neither_a_body_nor_blocks_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PUT",
        &format!("/reviews/{}/draft", seeded.review_id),
        Some("{}"),
    );
    assert_eq!(answer.status, 400, "{}", answer.text());
    assert_eq!(answer.error_code(), "invalid_value");
}

#[test]
fn a_block_about_a_finding_the_draft_cannot_raise_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // F2 is held back, so the draft does not raise it. **This is the rule DEV-873 left to
    // DEV-877**, enforced at the write rather than by the body quietly dropping a
    // paragraph the session wrote.
    let answer = harness.api(
        "PUT",
        &format!("/reviews/{}/draft", seeded.review_id),
        Some(
            r#"{"sections":[{"kind":"finding","finding_seq":2,"title":"A nit","text":"Posted after all."}]}"#,
        ),
    );

    assert_eq!(answer.status, 400, "{}", answer.text());
    assert_eq!(answer.error_code(), "invalid_value");
    let document = answer.json();
    let message = document["message"].as_str().expect("a message");
    assert!(message.contains("held_back"), "{message}");
}
