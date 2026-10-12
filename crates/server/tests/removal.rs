//! Dismissing a review, bringing it back, and deleting one that never started.
//!
//! **The dashboard's API is the only way any of this happens.** DEV-945: it is the
//! developer's act, from the page, never a session's — `crates/cli/tests/cli.rs` holds the
//! other half of that claim, which is that no command does it.
//!
//! The record is read back through the store after every call, because the one thing a
//! screen cannot show is whether a refusal wrote anything.

mod common;

use review_desk_store::{DismissedReviews, DraftStatus, NewDraft};

/// A review nobody has started, as the Locus dispatcher records a review request.
fn requested(harness: &common::Harness, pr: i64, head: &str) -> i64 {
    harness
        .store
        .open_review("acme/portal", pr, head)
        .expect("a review")
        .review
        .id
}

/// The ids `GET /reviews` draws for one choice about dismissed reviews.
fn listed(harness: &common::Harness, query: &str) -> Vec<i64> {
    let answer = harness.api("GET", &format!("/reviews{query}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()["reviews"]
        .as_array()
        .expect("the reviews")
        .iter()
        .map(|row| row["review"]["id"].as_i64().expect("an id"))
        .collect()
}

#[test]
fn dismissing_a_review_records_when_and_why() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/dismiss", seeded.review_id),
        Some(r#"{"reason":"the pull request was closed unmerged"}"#),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["id"], seeded.review_id);
    assert!(
        review["dismissed_at"].is_string(),
        "the answer says when: {}",
        answer.text()
    );
    assert_eq!(
        review["dismissed_reason"], "the pull request was closed unmerged",
        "and why, which is what the Reviews list draws beside it"
    );

    let stored = harness
        .store
        .get_review(seeded.review_id)
        .expect("the review");
    assert!(stored.is_dismissed(), "and it is on the record");
}

#[test]
fn a_dismissal_with_no_reason_is_refused_and_names_the_field() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for body in [r#"{"reason":""}"#, r#"{"reason":"   "}"#] {
        let answer = harness.api(
            "POST",
            &format!("/reviews/{}/dismiss", seeded.review_id),
            Some(body),
        );
        assert_eq!(answer.status, 400, "{}", answer.text());
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(
            answer.json()["field"],
            "reason",
            "the page puts the message under the box the developer typed in"
        );
        assert!(
            answer.json()["expected"].is_string(),
            "and `invalid_value` says what it would have taken: {}",
            answer.text()
        );
    }

    // A body with no `reason` at all, which is the shape a caller gets wrong rather than
    // a value they left blank.
    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/dismiss", seeded.review_id),
        Some("{}"),
    );
    assert_eq!(answer.status, 400, "{}", answer.text());

    assert!(
        !harness
            .store
            .get_review(seeded.review_id)
            .unwrap()
            .is_dismissed(),
        "and no refusal wrote anything"
    );
}

#[test]
fn dismissing_a_dismissed_review_is_a_conflict() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let path = format!("/reviews/{}/dismiss", seeded.review_id);

    let first = harness.api(
        "POST",
        &path,
        Some(r#"{"reason":"the pull request was closed unmerged"}"#),
    );
    assert_eq!(first.status, 200, "{}", first.text());

    let again = harness.api("POST", &path, Some(r#"{"reason":"something else"}"#));
    assert_eq!(again.status, 409, "{}", again.text());
    assert_eq!(again.error_code(), "conflict");

    assert_eq!(
        harness
            .store
            .get_review(seeded.review_id)
            .unwrap()
            .dismissed_reason
            .as_deref(),
        Some("the pull request was closed unmerged"),
        "the words the developer wrote are still there"
    );
}

#[test]
fn restoring_brings_a_review_back_and_a_review_in_play_is_a_conflict() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let refused = harness.api(
        "POST",
        &format!("/reviews/{}/restore", seeded.review_id),
        None,
    );
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert_eq!(refused.error_code(), "conflict");

    harness
        .store
        .dismiss_review(seeded.review_id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/restore", seeded.review_id),
        None,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert!(answer.json()["dismissed_at"].is_null());
    assert!(answer.json()["dismissed_reason"].is_null());
    assert!(!harness
        .store
        .get_review(seeded.review_id)
        .unwrap()
        .is_dismissed());
}

#[test]
fn the_reviews_list_leaves_dismissed_reviews_out_until_they_are_asked_for() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let stray = requested(&harness, 777, "4f1a9c2");
    harness
        .store
        .dismiss_review(stray, "a build session recorded this by accident")
        .expect("dismiss it");

    let default = listed(&harness, "");
    assert!(
        default.contains(&seeded.review_id) && !default.contains(&stray),
        "the default view is the reviews in play: {default:?}"
    );

    let only = listed(&harness, "?dismissed=only");
    assert_eq!(only, vec![stray], "and the filter is the dismissed ones");

    let included = listed(&harness, "?dismissed=included");
    assert!(
        included.contains(&seeded.review_id) && included.contains(&stray),
        "or the whole record on one page: {included:?}"
    );

    // The choice is echoed back, as the repository is, so the page's filter reads what it
    // was given rather than trusting its own state.
    let answer = harness.api("GET", "/reviews?dismissed=only", None);
    assert_eq!(answer.json()["dismissed"], "only");
    assert_eq!(
        harness.api("GET", "/reviews", None).json()["dismissed"],
        "excluded",
        "and a caller that asked for nothing is told what it got"
    );

    // The dismissed row carries the two fields the page draws.
    let row = answer.json()["reviews"][0]["review"].clone();
    assert!(row["dismissed_at"].is_string());
    assert_eq!(
        row["dismissed_reason"],
        "a build session recorded this by accident"
    );
}

#[test]
fn a_dismissed_choice_that_is_not_one_is_refused_rather_than_ignored() {
    // A mistyped filter answering with the default view would read as "there are no
    // dismissed reviews", which is the one wrong answer this parameter can give.
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    let answer = harness.api("GET", "/reviews?dismissed=yes", None);
    assert_eq!(answer.status, 400, "{}", answer.text());
    assert_eq!(answer.error_code(), "invalid_value");
    assert_eq!(answer.json()["field"], "dismissed");
    assert!(
        answer.json()["expected"]
            .as_str()
            .is_some_and(|expected| expected.contains("only")),
        "and it says what it would have taken: {}",
        answer.text()
    );
}

#[test]
fn deleting_a_review_nobody_started_removes_it() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);
    let stray = requested(&harness, 777, "4f1a9c2");

    let answer = harness.api("DELETE", &format!("/reviews/{stray}"), None);

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(
        answer.json()["id"],
        stray,
        "it answers with the review that went, which is the only copy left of it"
    );
    assert_eq!(answer.json()["pr"], 777);
    assert!(
        harness.store.find_review_by_id(stray).unwrap().is_none(),
        "and the review is gone"
    );

    // And it is gone from the Reviews list in every view, which is the difference between
    // a delete and a dismissal.
    for query in ["", "?dismissed=only", "?dismissed=included"] {
        assert!(
            !listed(&harness, query).contains(&stray),
            "a deleted review is in no view, and {query:?} had it"
        );
    }

    let again = harness.api("DELETE", &format!("/reviews/{stray}"), None);
    assert_eq!(again.status, 404, "{}", again.text());
    assert_eq!(again.error_code(), "not_found");
}

#[test]
fn deleting_a_review_with_work_on_it_is_refused_saying_what_it_holds() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("DELETE", &format!("/reviews/{}", seeded.review_id), None);

    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "conflict");
    let body = answer.json();
    let said = body["message"].as_str().expect("a message");
    assert!(
        said.contains("brief"),
        "the refusal names what it is protecting: {said}"
    );
    assert!(
        said.contains("dismiss"),
        "and points at the act that is allowed: {said}"
    );
    assert!(
        harness
            .store
            .find_review_by_id(seeded.review_id)
            .unwrap()
            .is_some(),
        "and the review is still there"
    );
}

#[test]
fn a_dismissed_review_is_refused_a_delete_for_its_work_and_not_for_the_dismissal() {
    // The two acts are independent: dismissal is reversible and is what the developer
    // reaches for first, so a dismissed review with findings must still be refused a
    // delete — and for the reason that it holds work, not because it is dismissed.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    harness
        .store
        .dismiss_review(seeded.review_id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let answer = harness.api("DELETE", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert!(answer.json()["message"]
        .as_str()
        .is_some_and(|said| said.contains("brief")));

    // And a dismissed review that never started is deleted, which is the sequence the
    // stray review DEV-945 was raised from will be removed by.
    let stray = requested(&harness, 777, "4f1a9c2");
    harness
        .store
        .dismiss_review(stray, "a build session recorded this by accident")
        .expect("dismiss it");
    assert_eq!(
        harness
            .api("DELETE", &format!("/reviews/{stray}"), None)
            .status,
        200
    );
}

#[test]
fn a_draft_alone_is_enough_to_refuse_a_delete() {
    // The three things a delete protects are asked about separately, because a review can
    // hold any one of them on its own: a session that wrote a draft and no brief is a real
    // state — `work list` has a kind for it.
    let harness = common::start_with_no_github();
    let drafted = requested(&harness, 778, "aaa1111");
    harness
        .store
        .put_draft(
            drafted,
            NewDraft {
                body: "A review body.".to_string(),
                status: DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");

    let answer = harness.api("DELETE", &format!("/reviews/{drafted}"), None);
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert!(
        answer.json()["message"]
            .as_str()
            .is_some_and(|said| said.contains("draft")),
        "{}",
        answer.text()
    );
}

#[test]
fn every_screen_that_reads_a_review_can_tell_it_is_dismissed() {
    // The page a review is dismissed from reads `GET /reviews/{id}`, and the list reads
    // `GET /reviews`. Both have to carry the dismissal or one of them draws a review the
    // other calls gone.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    harness
        .store
        .dismiss_review(seeded.review_id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let detail = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(detail.status, 200, "{}", detail.text());
    assert!(
        detail.json()["review"]["dismissed_at"].is_string(),
        "the one review screen can say so: {}",
        detail.text()
    );
    assert_eq!(
        detail.json()["review"]["dismissed_reason"],
        "the pull request was closed unmerged"
    );

    let row = harness
        .store
        .reviews_at_step(None, DismissedReviews::Only)
        .expect("the dismissed reviews")
        .remove(0);
    assert_eq!(row.review.id, seeded.review_id);
}

#[test]
fn a_review_that_is_not_there_is_not_found_whatever_the_body_says() {
    // **The order the checks run in, asserted.** The handler judged the reason first, so
    // a dismissal of a review that does not exist answered `invalid_value` about a field
    // on a review nobody has — and `an_unknown_review_is_not_found_on_every_operation`
    // could not catch it, because it sends a reason that is valid.
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    let answer = harness.api("POST", "/reviews/9001/dismiss", Some(r#"{"reason":"   "}"#));
    assert_eq!(answer.status, 404, "{}", answer.text());
    assert_eq!(answer.error_code(), "not_found");
}

#[test]
fn starting_a_review_that_was_dismissed_hands_it_back_saying_so() {
    // **`POST /reviews` deduplicates**, so pasting the link of a pull request whose review
    // the developer dismissed hands that review back. The answer has to carry the
    // dismissal, because nothing else on that screen can tell: the review is in no queue
    // group, yields no work item and is not in the Reviews list's default view, so a
    // caller told only "here is your review" would say it was open and started while
    // nothing at all was going to happen to it.
    let harness = common::start(common::recorded_github());
    let started = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let review_id = started.json()["id"].as_i64().expect("an id");

    harness
        .store
        .dismiss_review(review_id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let again = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );

    assert_eq!(again.status, 201, "{}", again.text());
    assert_eq!(
        again.json()["id"],
        review_id,
        "the same review, not a second"
    );
    assert!(
        again.json()["dismissed_at"].is_string(),
        "and the answer says it is dismissed: {}",
        again.text()
    );
    assert_eq!(
        again.json()["dismissed_reason"],
        "the pull request was closed unmerged"
    );
    /* And starting it did not quietly bring it back. */
    assert!(harness.store.get_review(review_id).unwrap().is_dismissed());
}
