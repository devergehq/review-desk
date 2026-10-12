//! The edges between the API and the dashboard, and the edge of a request.
//!
//! Every test here is a regression test for something a review found rather than
//! something a handler was written for. They are in one file because they are all the
//! same kind of mistake: a boundary that looked like it held and did not.

mod common;

use review_desk_server::assets;

#[test]
fn a_path_that_merely_starts_with_the_api_prefix_is_not_an_api_call() {
    let harness = common::start_with_no_github();

    // `/api/v1reviews` is not `/api/v1/reviews`. Without a separator check, stripping the
    // prefix leaves `reviews` and this really creates a review — a write from a URL that
    // names no endpoint.
    let answer = harness.request(
        "POST",
        "/api/v1reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#),
    );

    assert_ne!(
        answer.status,
        201,
        "/api/v1reviews must not be routed as POST /reviews: {}",
        answer.text()
    );
    assert!(
        harness
            .store
            .list_reviews("acme/portal", 412)
            .unwrap()
            .is_empty(),
        "a path that is not an endpoint must not write to the record"
    );
}

#[test]
fn a_path_that_merely_starts_with_the_api_prefix_is_not_the_health_endpoint() {
    let harness = common::start_with_no_github();

    let answer = harness.request("GET", "/api/v1health", None);
    assert!(
        !answer.text().contains("\"status\":\"ok\""),
        "/api/v1health must not answer the health endpoint: {}",
        answer.text()
    );
}

#[test]
fn the_api_prefix_with_nothing_after_it_is_handled() {
    let harness = common::start_with_no_github();

    // Not a crash and not the dashboard: it is under the API, so it gets the API's 404.
    for path in ["/api/v1", "/api/v1/"] {
        let answer = harness.request("GET", path, None);
        assert_eq!(answer.status, 404, "{path}: {}", answer.text());
        assert_eq!(
            answer.header("Content-Type"),
            Some("application/json; charset=utf-8"),
            "{path} is under the API, so its 404 is JSON"
        );
    }
}

#[test]
fn an_over_long_body_is_refused_rather_than_truncated() {
    let harness = common::start_with_no_github();

    // Five megabytes, over the four-megabyte limit. The refusal has to be about the size,
    // not a complaint that the JSON was malformed — it was not; it would have been cut in
    // half on the way in.
    let body = format!(
        r#"{{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"{}"}}"#,
        "a".repeat(5 * 1024 * 1024)
    );
    let answer = harness.api("POST", "/reviews", Some(&body));

    assert_eq!(answer.status, 400, "{}", answer.text());
    let message = answer.json()["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        message.contains("limit"),
        "an over-long body is refused for its size, not for the shape it was cut into: \
         {message:?}"
    );
}

#[test]
fn a_route_segment_with_a_dot_still_loads_the_dashboard() {
    if !assets::embedded() {
        return; // Asserted by the run CI makes with the front end built.
    }

    let harness = common::start_with_no_github();

    // A repository is `owner/repo` and the contract's `RepoName` permits a dot, so a
    // client-side route can legitimately carry one. Treating every dotted last segment as
    // a file would 404 a link the client router could have resolved.
    let answer = harness.request("GET", "/reviews/acme/portal.io", None);

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(
        answer.header("Content-Type"),
        Some("text/html; charset=utf-8")
    );
}

#[test]
fn a_missing_asset_with_a_known_extension_is_still_a_404() {
    if !assets::embedded() {
        return;
    }

    let harness = common::start_with_no_github();

    // The other half of the same decision. Falling back to the index for a missing script
    // would answer it with HTML, and the browser's error would then be a parse failure
    // somewhere else entirely.
    for path in [
        "/_nuxt/does-not-exist.js",
        "/_nuxt/does-not-exist.css",
        "/missing.woff2",
    ] {
        let answer = harness.request("GET", path, None);
        assert_eq!(answer.status, 404, "{path}: {}", answer.text());
    }
}

#[test]
fn a_draft_in_the_wrong_state_is_a_conflict_not_a_bad_request() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let review = seeded.review_id;

    harness.api(
        "POST",
        &format!("/reviews/{review}/draft/approve"),
        Some(r#"{"event":"comment"}"#),
    );

    // The contract declares 409 for each of these, and a dashboard branches on the code.
    // The store refuses them too, but as a refusal that would otherwise map to 400 —
    // "this draft is approved" is a statement about the record, not about the request.
    //
    // **Sending it back is not one of them, since DEV-929**: taking the sign-off back is
    // the one transition out of `approved`, and it is the developer's. Without it a draft
    // the hand-over refuses has nowhere to go at all, which is the state the worker half
    // measured. It has a test of its own below.
    for (method, suffix, body) in [
        ("PUT", "", r#"{"body":"edited underneath"}"#),
        ("POST", "/approve", r#"{"event":"comment"}"#),
    ] {
        let answer = harness.api(
            method,
            &format!("/reviews/{review}/draft{suffix}"),
            Some(body),
        );
        assert_eq!(
            answer.status,
            409,
            "{method} /draft{suffix} on an approved draft: {}",
            answer.text()
        );
        assert_eq!(answer.error_code(), "conflict");
    }
}

/// **The one transition out of a sign-off, and what it takes with it.**
///
/// A developer who has signed off and changed their mind — because the hand-over refused
/// the draft, or for any other reason — can send it back while nothing is posted. The
/// sign-off goes with it: a record still naming the event and whose assent it was would
/// say a draft was approved when it is waiting to be rewritten.
#[test]
fn an_approved_draft_is_sent_back_and_the_sign_off_goes_with_it() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let review = seeded.review_id;

    let approved = harness.api(
        "POST",
        &format!("/reviews/{review}/draft/approve"),
        Some(r#"{"event":"comment"}"#),
    );
    assert_eq!(approved.status, 200, "{}", approved.text());
    assert_eq!(approved.json()["draft"]["event"], "comment");

    let sent_back = harness.api(
        "POST",
        &format!("/reviews/{review}/draft/send-back"),
        Some(r#"{"note":"the index is in the old shape; write it again"}"#),
    );
    assert_eq!(sent_back.status, 200, "{}", sent_back.text());

    let draft = &sent_back.json()["draft"];
    assert_eq!(draft["status"], "sent_back");
    assert_eq!(
        draft["sent_back_note"],
        "the index is in the old shape; write it again"
    );
    assert!(
        draft["event"].is_null(),
        "the sign-off is taken back: {draft}"
    );
    assert!(draft["approved_at"].is_null());

    // And a session cannot post what is no longer approved, whichever order the two land
    // in: `post_draft` takes an approved draft and nothing else.
    let refused = harness
        .store
        .post_draft(review, "gh-review-3312")
        .expect_err("a sent-back draft is not posted");
    assert!(
        refused.to_string().contains("approved"),
        "the refusal says what it wanted: {refused}"
    );
}

#[test]
fn an_approved_draft_keeps_the_body_that_was_approved() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let review = seeded.review_id;

    let approved = harness.api(
        "POST",
        &format!("/reviews/{review}/draft/approve"),
        Some(r#"{"event":"comment"}"#),
    );
    assert_eq!(approved.status, 200, "{}", approved.text());
    let body = approved.json()["draft"]["body"]
        .as_str()
        .unwrap()
        .to_string();

    harness.api(
        "PUT",
        &format!("/reviews/{review}/draft"),
        Some(r#"{"body":"edited underneath"}"#),
    );

    // The refusal is only worth anything if it actually refused.
    let after = harness.api("GET", &format!("/reviews/{review}/draft"), None);
    assert_eq!(
        after.json()["draft"]["body"],
        body,
        "an approved draft is what the review session reads and posts"
    );
}
