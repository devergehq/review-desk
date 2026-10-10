//! How long a review took, over the wire.
//!
//! The arithmetic is asserted in `crates/store/tests/timings.rs`, against a clock that
//! test can move. What is asserted here is what a client actually receives: whole seconds
//! rather than formatted text, the passes on the step they belong to, a null where the
//! record cannot answer — and that **no request body can supply a time**, which is the
//! half of DEV-890's second rule that lives at this boundary.

mod common;

use review_desk_store::{Actor, DraftStatus, NewDraft, ReviewEvent, ReviewLayerState};

/// The layer a draft is written and signed off at.
const DRAFT: i64 = 6;

#[test]
fn one_review_serves_its_totals_and_each_steps_passes() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let review = answer.json();

    let timings = &review["timings"];
    assert!(
        timings["requested_at"].is_string(),
        "a review always knows when it was asked for: {timings}"
    );
    assert!(
        timings["last_activity_at"].is_string(),
        "the seed wrote to it, so something has: {timings}"
    );

    // The seed walks every layer to its state in one pass, so step 6 is waiting on the
    // developer and the draft is ready. That is a span with a start and no end.
    assert!(timings["draft_ready_at"].is_string(), "{timings}");

    let step = review["layers"]
        .as_array()
        .expect("seven steps")
        .iter()
        .find(|layer| layer["number"] == DRAFT)
        .expect("step 6")
        .clone();
    let runs = step["runs"].as_array().expect("its passes");
    assert_eq!(runs.len(), 1, "one pass, still going: {step}");
    assert_eq!(runs[0]["state"], "waiting_on_you");
    assert!(runs[0]["started_at"].is_string());
    assert!(
        runs[0]["finished_at"].is_null() && runs[0]["seconds"].is_null(),
        "a pass that has not ended has no length: {step}"
    );
    assert!(
        step["seconds"].is_null(),
        "and neither has the step: {step}"
    );

    // A step nothing has written: not started, which is a third answer again.
    let harvest = review["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["number"] == 7)
        .expect("step 7")
        .clone();
    assert_eq!(harvest["runs"].as_array().map(Vec::len), Some(0));
    assert!(harvest["seconds"].is_null());
}

#[test]
fn every_duration_on_the_wire_is_a_whole_number_of_seconds() {
    // **The contract's rule, asserted rather than described.** "17 min" is the screen's
    // decision; a server that formatted a duration would have made it, and a dashboard
    // would then have two ways to render the same figure.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // A step walked through to a finish, so there is a measured pass to look at.
    harness
        .store
        .set_review_layer(seeded.review_id, 3, ReviewLayerState::Running, None)
        .expect("the step starts again");
    harness
        .store
        .set_review_layer(
            seeded.review_id,
            3,
            ReviewLayerState::Done,
            Some("3 options"),
        )
        .expect("and finishes");

    let review = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();

    let mut measured = 0;
    for layer in review["layers"].as_array().expect("seven steps") {
        let total = &layer["seconds"];
        assert!(
            total.is_null() || total.is_u64(),
            "a step's duration is whole seconds or null, never {total}"
        );
        for run in layer["runs"].as_array().expect("its passes") {
            let seconds = &run["seconds"];
            assert!(
                seconds.is_null() || seconds.is_u64(),
                "a pass's duration is whole seconds or null, never {seconds}"
            );
            if seconds.is_u64() {
                measured += 1;
            }
        }
    }
    assert!(
        measured > 0,
        "nothing was measured, so this test asserted nothing: {review}"
    );

    for figure in ["with_the_agent_seconds", "waiting_on_you_seconds"] {
        let value = &review["timings"][figure];
        assert!(
            value.is_null() || value.is_u64(),
            "{figure} is whole seconds or null, never {value}"
        );
    }
}

#[test]
fn a_request_cannot_supply_a_time_for_a_step() {
    // **No operation takes a time, and a request that sends one is refused rather than
    // quietly ignored.** Every request schema in the contract is
    // `additionalProperties: false` and every request struct here is `deny_unknown_fields`,
    // so this is the boundary holding DEV-890's second rule: a duration a request could
    // set is a duration that could be backdated or invented.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let before = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json()["timings"]["draft_ready_at"]
        .as_str()
        .expect("the draft is ready")
        .to_string();

    // Sending a draft back, with a time the caller made up. The year is 2019, well before
    // Review Desk existed.
    let refused = harness.api(
        "POST",
        &format!("/reviews/{}/draft/send-back", seeded.review_id),
        Some(
            r#"{"note":"cut F2","started_at":"2019-01-01T00:00:00Z",
                "finished_at":"2019-01-01T01:00:00Z","seconds":3600}"#,
        ),
    );
    assert_eq!(refused.status, 400, "{}", refused.text());
    assert_eq!(refused.error_code(), "bad_request");
    assert_eq!(
        harness
            .store
            .get_draft(seeded.review_id)
            .expect("the draft")
            .status,
        DraftStatus::Ready,
        "the draft was not sent back either: every refusal is computed before any write"
    );

    // And signing off, the same way.
    let refused = harness.api(
        "POST",
        &format!("/reviews/{}/draft/approve", seeded.review_id),
        Some(r#"{"event":"comment","signed_off_at":"2019-01-01T02:00:00Z"}"#),
    );
    assert_eq!(refused.status, 400, "{}", refused.text());

    // The same two calls without the invented times, which is the only way through.
    assert_eq!(
        harness
            .api(
                "POST",
                &format!("/reviews/{}/draft/send-back", seeded.review_id),
                Some(r#"{"note":"cut F2"}"#),
            )
            .status,
        200
    );
    harness
        .store
        .put_draft(seeded.review_id, NewDraft::ready("the second draft"))
        .expect("redrafted");
    assert_eq!(
        harness
            .api(
                "POST",
                &format!("/reviews/{}/draft/approve", seeded.review_id),
                Some(r#"{"event":"comment"}"#),
            )
            .status,
        200
    );

    let served = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();
    let timings = served["timings"].clone();
    for field in [
        "requested_at",
        "draft_ready_at",
        "signed_off_at",
        "last_activity_at",
    ] {
        let value = timings[field]
            .as_str()
            .unwrap_or_else(|| panic!("{field} should be set: {timings}"));
        assert!(
            value > "2026-01-01T00:00:00Z",
            "{field} is {value}, which is not a time the store stamped"
        );
    }
    assert!(
        timings["draft_ready_at"].as_str().unwrap() >= before.as_str(),
        "the redraft's ready time cannot be earlier than the first one: {timings}"
    );

    // The step's own passes say the same thing, which is where a backdated figure would
    // have shown up as a duration.
    let step = served["layers"]
        .as_array()
        .expect("seven steps")
        .iter()
        .find(|layer| layer["number"] == DRAFT)
        .expect("step 6")
        .clone();
    assert!(
        step["runs"].as_array().map(Vec::len).unwrap_or(0) >= 3,
        "the draft was redrafted, so step 6 made more than one pass: {step}"
    );
    for run in step["runs"].as_array().expect("its passes") {
        for end in ["started_at", "finished_at"] {
            if let Some(time) = run[end].as_str() {
                assert!(
                    time > "2026-01-01T00:00:00Z",
                    "a pass {end} of {time} came from a request, not from the store"
                );
            }
        }
    }
}

#[test]
fn the_queue_says_how_long_a_review_has_been_where_it_is() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let inbox = harness.api("GET", "/inbox", None).json();

    let running = inbox["in_progress"]
        .as_array()
        .expect("the reviews with the agent")
        .iter()
        .find(|item| item["review_id"] == seeded.running_review_id)
        .expect("the running review")
        .clone();
    assert!(
        running["in_state_since"].is_string(),
        "the queue needs the start of the pass to say how long it has been running: \
         {running}"
    );
    assert!(
        running["last_activity_at"].is_string(),
        "and when anything last wrote to it, where it said the session state was unknown: \
         {running}"
    );
    assert!(
        running["session_alive"].is_null(),
        "which is not a claim about the session: Allele still owns that"
    );

    // A brief whose session never wrote step 2 is still dated, from the brief itself.
    let harness2 = common::start_with_no_github();
    let only_a_brief = harness2
        .store
        .open_review("acme/portal", 500, "ab12cd3")
        .expect("a review")
        .review
        .id;
    harness2
        .store
        .put_brief(
            only_a_brief,
            review_desk_store::NewBrief {
                problem: "The importer reads another tenant's rows.".to_string(),
                ..Default::default()
            },
        )
        .expect("its brief, and nothing else");
    let brief_row = harness2.api("GET", "/inbox", None).json()["briefs_to_confirm"]
        .as_array()
        .expect("the briefs to confirm")
        .iter()
        .find(|item| item["review_id"] == only_a_brief)
        .expect("the review with only a brief")
        .clone();
    assert!(
        brief_row["brief_ready_at"].is_string(),
        "a session that writes a brief and no step still leaves a time to date the row \
         by — the brief's own: {brief_row}"
    );

    let draft = inbox["drafts_to_approve"]
        .as_array()
        .expect("the drafts to sign off")
        .iter()
        .find(|item| item["review_id"] == seeded.review_id)
        .expect("the ready draft")
        .clone();
    assert!(
        draft["draft_ready_at"].is_string(),
        "the age cell read \"no time recorded\" over this row before DEV-890: {draft}"
    );
}

#[test]
fn the_finished_review_list_is_served_as_one_read() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // Nothing is posted yet, and an empty list is the honest answer rather than a 404.
    let answer = harness.api("GET", "/timings", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(answer.json()["reviews"], serde_json::json!([]));

    harness
        .store
        .approve_draft(seeded.review_id, ReviewEvent::Comment, Actor::Developer)
        .expect("the developer signs it off");
    harness
        .store
        .post_draft(seeded.review_id, "gh-review-3312")
        .expect("the session posts it");

    let listed = harness.api("GET", "/timings", None).json();
    let rows = listed["reviews"].as_array().expect("the finished reviews");
    assert_eq!(rows.len(), 1, "one posted review: {listed}");
    assert_eq!(rows[0]["review"]["id"], seeded.review_id);
    assert_eq!(
        rows[0]["layers"].as_array().map(Vec::len),
        Some(7),
        "all seven steps, so a per-step figure can be computed from this read alone"
    );
    assert!(rows[0]["timings"]["posted_at"].is_string());
    assert_eq!(
        harness
            .store
            .get_draft(seeded.review_id)
            .expect("the draft")
            .status,
        DraftStatus::Posted
    );

    // Filtered by repository, like every other list the server serves.
    assert_eq!(
        harness
            .api("GET", "/timings?repo=other%2Frepo", None)
            .json()["reviews"],
        serde_json::json!([]),
        "another repository's reviews are not this one's"
    );
}
