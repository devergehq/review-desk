//! `brief put` and the confirmation the developer may not have to give.
//!
//! The store holds the rule; this is the exit code a caller branches on. **Exit 1 is the
//! point**: an agent that tried to withdraw a confirmation the developer had already been
//! asked for has made a request that was understood and refused, not found Review Desk
//! unable to run, and DEV-864 asks for that code by name.

mod common;

use common::{open_review, Desk};
use serde_json::{json, Value};

/// Write a brief and hand back the stored document.
fn put(desk: &Desk, review: &str, brief: &Value) -> Value {
    let path = desk.dir().join("brief.json");
    std::fs::write(&path, brief.to_string()).expect("write the brief");
    desk.json(&[
        "brief",
        "put",
        "--review",
        review,
        "--file",
        path.to_str().expect("a path"),
    ])
}

/// Try to write a brief that will be refused, and hand back the failure document.
fn refused(desk: &Desk, review: &str, brief: &Value) -> Value {
    let path = desk.dir().join("refused.json");
    std::fs::write(&path, brief.to_string()).expect("write the brief");
    desk.refused_json(&[
        "brief",
        "put",
        "--review",
        review,
        "--file",
        path.to_str().expect("a path"),
    ])
}

#[test]
fn a_brief_requires_confirmation_unless_the_caller_says_otherwise() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let stored = put(&desk, &review, &json!({ "problem": "the problem" }));
    assert_eq!(
        stored["brief"]["confirmation_required"], true,
        "the default is the strict answer, and a caller that never heard of the field \
         gets it"
    );
    assert_eq!(
        stored["brief"]["confirmation_not_required_because"],
        Value::Null
    );
}

#[test]
fn a_brief_may_be_recorded_as_needing_no_confirmation_with_a_reason() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "7", "c0ffee1");

    let stored = put(
        &desk,
        &review,
        &json!({
            "problem": "a log line says null where it means not measured",
            "confirmation_required": false,
            "confirmation_not_required_because":
                "38 changed lines, under this caller's 200-line threshold",
        }),
    );

    assert_eq!(stored["brief"]["confirmation_required"], false);
    assert_eq!(
        stored["brief"]["confirmation_not_required_because"],
        "38 changed lines, under this caller's 200-line threshold"
    );

    // Layer 2 is finished with on write, and the trail says why.
    let shown = desk.json(&["review", "show", "--review", &review]);
    let layer = &shown["layers"][1];
    assert_eq!(layer["number"], 2);
    assert_eq!(layer["state"], "done");
    assert_eq!(
        layer["detail"],
        "confirmation not required: 38 changed lines, under this caller's 200-line threshold"
    );

    // And an agent may carry on past it.
    let items = desk.json(&["work", "list"]);
    assert_eq!(items["work"][0]["kind"], "brief_settled");
    assert_eq!(
        items["work"][0]["sign_off"], "waived",
        "nobody confirmed it, and nothing downstream may be allowed to think they did"
    );
}

#[test]
fn needing_no_confirmation_without_a_reason_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "8", "d00dfee");

    let failure = refused(
        &desk,
        &review,
        &json!({ "problem": "the problem", "confirmation_required": false }),
    );

    assert_eq!(failure["error"]["code"], "invalid_value");
    assert_eq!(failure["error"]["kind"], "refused");
    assert!(
        failure["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("confirmation_not_required_because"),
        "{failure}"
    );

    // Nothing was written, so there is no brief at all.
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(shown["brief"], Value::Null);
}

#[test]
fn a_reason_on_a_brief_that_does_require_confirmation_is_refused() {
    // The two fields are one fact. A brief that requires confirmation and also carries a
    // reason it does not need one is a record nothing could read.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "9", "eeeffff");

    let failure = refused(
        &desk,
        &review,
        &json!({
            "problem": "the problem",
            "confirmation_required": true,
            "confirmation_not_required_because": "it is small",
        }),
    );
    assert_eq!(failure["error"]["code"], "invalid_value");
}

#[test]
fn rewriting_a_brief_as_needing_no_confirmation_exits_one() {
    // **The heart of it.** `confirmation_required: false` is the one place an agent
    // decides a developer step was unnecessary, and the only thing that makes it safe is
    // that it cannot be reached for after the developer has been asked.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    put(&desk, &review, &json!({ "problem": "the problem" }));

    let failure = refused(
        &desk,
        &review,
        &json!({
            "problem": "the problem",
            "confirmation_required": false,
            "confirmation_not_required_because": "on reflection it is small",
        }),
    );

    assert_eq!(failure["error"]["code"], "invalid_value");
    assert_eq!(failure["error"]["kind"], "refused", "exit 1, not exit 2");
    let message = failure["error"]["message"].as_str().expect("a message");
    assert!(message.contains("confirmation_required"), "{message}");
    assert!(message.contains("cannot be withdrawn"), "{message}");

    // The record is untouched, and the developer is still being asked.
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(shown["brief"]["confirmation_required"], true);
    assert!(desk.json(&["work", "list"])["work"]
        .as_array()
        .expect("a list")
        .is_empty());
}

#[test]
fn asking_for_a_confirmation_that_was_not_needed_is_allowed() {
    // The other direction costs the developer a click and the record nothing, so it is
    // permitted — and it puts layer 2 back, because leaving it `done` would claim the
    // layer was settled while the developer is being asked.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "10", "abcdef1");

    put(
        &desk,
        &review,
        &json!({
            "problem": "the first reading",
            "confirmation_required": false,
            "confirmation_not_required_because": "a one-line change",
        }),
    );

    let stored = put(
        &desk,
        &review,
        &json!({ "problem": "a second reading, which turned out to be bigger" }),
    );
    assert_eq!(stored["brief"]["confirmation_required"], true);
    assert_eq!(
        stored["brief"]["confirmation_not_required_because"],
        Value::Null
    );

    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(shown["layers"][1]["state"], "waiting_on_you");
    assert!(
        desk.json(&["work", "list"])["work"]
            .as_array()
            .expect("a list")
            .is_empty(),
        "and nothing is waiting on an agent while the developer is being asked"
    );
}
