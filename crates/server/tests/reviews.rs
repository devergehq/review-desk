//! The three operations the store on `master` can answer in full.
//!
//! `POST /reviews` carries most of the weight, because it is the one operation that reads
//! GitHub and DEV-861 is specific about the order it does things in: the request is
//! checked first, so a machine with no `gh` reports the real problem; and if the read
//! fails, nothing is created.

mod common;

use review_desk_store::Disposition;

#[test]
fn a_link_alone_creates_a_review_from_a_recorded_github_response() {
    let harness = common::start(common::recorded_github());

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["repo"], "devergehq/locus");
    assert_eq!(review["pr"], 64);
    assert_eq!(
        review["head_sha"],
        common::RECORDED_HEAD_SHA,
        "the head commit has to be the one GitHub reported, resolved before creation"
    );
    assert_eq!(review["round"], 1);

    // No network was involved: the only GitHub this server had was a directory of files.
    let stored = harness
        .store
        .find_review_by_id(review["id"].as_i64().unwrap())
        .unwrap()
        .expect("the review is on the record");
    assert_eq!(stored.head_sha, common::RECORDED_HEAD_SHA);
}

#[test]
fn a_malformed_link_is_refused_without_reading_github() {
    // The reader behind the counter is the recorded one, so it *would* have answered. A
    // non-zero count therefore means the handler reached for GitHub before it had
    // finished checking the request — which is the test DEV-861 asks for: one that fails
    // if the GitHub trait is called.
    let (github, calls) = common::counted(common::recorded_github());
    let harness = common::start(github);

    for (link, why) in [
        (
            "https://github.com/acme/portal/pulls/412",
            "no /pull/ segment",
        ),
        ("https://github.com/acme/portal/pull/", "no number"),
        ("https://github.com/acme/portal/pull/zero", "not a number"),
        (
            "https://github.com/portal/pull/412",
            "the host would become the owner",
        ),
        ("not a url at all", "not a link"),
        ("https://github.com/acme/portal/pull/0", "number below 1"),
        (
            "ftp://github.com/acme/portal/pull/412",
            "not an http scheme",
        ),
        ("github.com/acme/portal/pull/412", "no scheme"),
        (
            "https://github.com/acme/portal/pull/412/../../evil/pull/9",
            "a path that is not the shape",
        ),
    ] {
        let body = format!(r#"{{"pr_url":{}}}"#, serde_json::to_string(link).unwrap());
        let answer = harness.api("POST", "/reviews", Some(&body));

        assert_eq!(
            answer.status,
            400,
            "{link:?} ({why}) should be refused as a bad link, not answered {}: {}",
            answer.status,
            answer.text()
        );
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "pr_url");
        assert!(
            answer.json()["expected"].is_string(),
            "invalid_value has to say what it would have accepted"
        );
    }

    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a malformed link must be refused before any GitHub read is made"
    );
}

/// The same assertion, made where the call count can actually be read.
///
/// The server owns its GitHub reader once it is built, so the count is checked by giving
/// the server a reader that has nothing recorded and asserting the refusal is about the
/// link rather than about GitHub. A read would have produced `github_unavailable`.
#[test]
fn a_malformed_link_never_reports_github_as_the_problem() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/issues/412"}"#),
    );

    assert_eq!(answer.error_code(), "invalid_value");
    assert_ne!(
        answer.error_code(),
        "github_unavailable",
        "a machine with no gh must be told its link is wrong, not that gh is missing"
    );
}

#[test]
fn github_being_unavailable_returns_the_contracts_code_and_creates_nothing() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412"}"#),
    );

    assert_eq!(answer.status, 502, "{}", answer.text());
    assert_eq!(answer.error_code(), "github_unavailable");

    // Nothing was created. A review with no resolvable head could not be deduplicated or
    // given a round, which is the reason the contract resolves the head first.
    assert!(
        harness
            .store
            .list_reviews("acme/portal", 412)
            .unwrap()
            .is_empty(),
        "a failed GitHub read must leave no review behind"
    );
}

#[test]
fn a_supplied_head_commit_needs_no_github_read() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    assert_eq!(answer.json()["head_sha"], "3f9c2ab");
}

#[test]
fn the_same_head_commit_returns_the_same_review() {
    let harness = common::start_with_no_github();
    let body = r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#;

    let first = harness.api("POST", "/reviews", Some(body));
    let again = harness.api("POST", "/reviews", Some(body));

    assert_eq!(first.status, 201);
    assert_eq!(again.status, 201);
    assert_eq!(
        first.json()["id"],
        again.json()["id"],
        "a second click of \"Understand it first\" must not open a second review"
    );
    assert_eq!(again.json()["round"], 1);
}

#[test]
fn a_bad_head_commit_is_refused() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"NOTAHASH"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "invalid_value");
    assert_eq!(answer.json()["field"], "head_sha");
}

#[test]
fn a_field_the_store_cannot_hold_is_refused_rather_than_dropped() {
    let harness = common::start_with_no_github();

    for (field, body) in [
        (
            "title",
            r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab","title":"Retry failed webhook deliveries"}"#,
        ),
        (
            "ticket_id",
            r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab","ticket_id":"ABC-231"}"#,
        ),
        (
            "changed_lines",
            r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab","changed_lines":214}"#,
        ),
    ] {
        let answer = harness.api("POST", "/reviews", Some(body));
        assert_eq!(
            answer.status,
            501,
            "{field} has no column on master, so it must not be silently dropped: {}",
            answer.text()
        );
        assert_eq!(answer.error_code(), "not_implemented");
        assert_eq!(answer.json()["field"], field);
    }

    assert!(
        harness
            .store
            .list_reviews("acme/portal", 412)
            .unwrap()
            .is_empty(),
        "a refused request must create nothing"
    );
}

#[test]
fn an_unknown_field_in_the_body_is_refused() {
    let harness = common::start_with_no_github();

    // The contract sets additionalProperties: false, and a misspelled `head_sha` would
    // otherwise be a GitHub read the caller thought they had saved.
    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_shaa":"3f9c2ab"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "bad_request");
}

#[test]
fn confirming_a_brief_sets_the_confirmation_and_says_so() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", seeded.review_id),
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let brief = answer.json();
    assert_eq!(brief["review_id"], seeded.review_id);
    assert_eq!(brief["confirmed"], true);
    assert!(
        brief["confirmed_at"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z')),
        "the confirmation time is RFC 3339 in UTC: {brief}"
    );

    let stored = harness.store.get_brief(seeded.review_id).unwrap();
    assert!(stored.is_confirmed(), "the store holds the confirmation");
}

#[test]
fn confirming_a_confirmed_brief_returns_it_unchanged() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let path = format!("/reviews/{}/brief/confirm", seeded.review_id);

    let first = harness.api("POST", &path, None);
    let again = harness.api("POST", &path, None);

    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(
        first.json()["confirmed_at"],
        again.json()["confirmed_at"],
        "confirming twice keeps the first confirmation time"
    );
}

#[test]
fn confirming_a_brief_that_was_never_written_is_a_conflict() {
    let harness = common::start_with_no_github();
    let opened = harness
        .store
        .open_review("acme/portal", 99, "abc1234")
        .unwrap();

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", opened.review.id),
        None,
    );

    // 409 and not 404: the review exists, and the contract gives the operation both.
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "conflict");
}

#[test]
fn a_findings_disposition_moves_with_its_reason() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/2", seeded.review_id),
        Some(r#"{"disposition":"held_back","reason":"a nit; it goes in the ticket"}"#),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let finding = answer.json();
    assert_eq!(finding["seq"], 2);
    assert_eq!(
        finding["key"], "F2",
        "the display key is derived by the server"
    );
    assert_eq!(finding["disposition"], "held_back");
    assert_eq!(
        finding["disposition_reason"],
        "a nit; it goes in the ticket"
    );

    let stored = harness.store.get_finding(seeded.review_id, 2).unwrap();
    assert_eq!(stored.disposition, Disposition::HeldBack);
}

#[test]
fn holding_a_finding_back_with_no_reason_is_refused_and_changes_nothing() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for body in [
        r#"{"disposition":"held_back"}"#,
        r#"{"disposition":"suppressed","reason":"   "}"#,
    ] {
        let answer = harness.api(
            "PATCH",
            &format!("/reviews/{}/findings/1", seeded.review_id),
            Some(body),
        );

        assert_eq!(answer.status, 400, "{}", answer.text());
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "reason");

        // The refusal is computed before the write, so the finding is where it was.
        let stored = harness.store.get_finding(seeded.review_id, 1).unwrap();
        assert_eq!(
            stored.disposition,
            Disposition::InDraft,
            "a refused disposition change must leave the finding alone"
        );
    }
}

#[test]
fn moving_a_finding_back_into_the_draft_needs_no_reason() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/1", seeded.review_id),
        Some(r#"{"disposition":"in_draft"}"#),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
}

#[test]
fn an_unknown_disposition_names_the_ones_that_exist() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/1", seeded.review_id),
        Some(r#"{"disposition":"wontfix"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "invalid_value");
    let expected = answer.json()["expected"].as_str().unwrap().to_string();
    assert!(
        expected.contains("in_draft") && expected.contains("held_back"),
        "a refusal has to say what it would have accepted; it said {expected:?}"
    );
}

#[test]
fn an_unknown_finding_is_not_found() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/99", seeded.review_id),
        Some(r#"{"disposition":"in_draft"}"#),
    );

    assert_eq!(answer.status, 404);
    assert_eq!(answer.error_code(), "not_found");
}
