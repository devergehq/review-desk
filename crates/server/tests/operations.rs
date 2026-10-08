//! Every one of the contract's sixteen operations is routed and answers.
//!
//! This walks `Operation::ALL` rather than listing routes by hand, so an operation added
//! to the contract and not to the server fails here instead of 404ing in a browser.

mod common;

use review_desk_server::route::Operation;

/// The sixteen operations, as `contract/README.md` counts them.
#[test]
fn the_contract_has_sixteen_operations() {
    assert_eq!(Operation::ALL.len(), 16);
}

#[test]
fn every_operation_answers_from_a_seeded_record() {
    for operation in Operation::ALL {
        // A record of its own per operation. Several of these write — approving a draft,
        // promoting a rule — so a shared record would make the answers depend on the
        // order of the loop, and the loop's order is not a thing this test is about.
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);

        let answer = call(&harness, *operation, &seeded);
        assert!(
            (200..300).contains(&answer.status),
            "{} ({}) should answer from a seeded record; it answered {} with {}",
            operation.route(),
            operation.id(),
            answer.status,
            answer.text()
        );
    }
}

/// Nothing answers 501 any more, and that is worth asserting rather than assuming.
///
/// This server was built while a sibling issue was still changing the store, and
/// thirteen operations returned 501 naming the store item they waited on. That issue
/// merged before this pull request opened. A 501 from any operation now means a handler
/// was left behind, which is exactly the thing that would otherwise be noticed in a
/// browser weeks later.
#[test]
fn no_operation_says_it_is_not_built() {
    for operation in Operation::ALL {
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);

        let answer = call(&harness, *operation, &seeded);
        assert_ne!(
            answer.status,
            501,
            "{} still answers 501: {}",
            operation.route(),
            answer.text()
        );
    }
}

#[test]
fn an_unknown_review_is_not_found_on_every_operation_that_names_one() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    let missing = [
        ("GET", "/reviews/9001", None),
        ("POST", "/reviews/9001/brief/confirm", None),
        (
            "PATCH",
            "/reviews/9001/findings/1",
            Some(r#"{"disposition":"in_draft"}"#),
        ),
        ("GET", "/reviews/9001/draft", None),
        ("PUT", "/reviews/9001/draft", Some(r#"{"body":"x"}"#)),
        (
            "POST",
            "/reviews/9001/draft/approve",
            Some(r#"{"event":"comment"}"#),
        ),
        (
            "POST",
            "/reviews/9001/draft/send-back",
            Some(r#"{"note":"cut the nits"}"#),
        ),
        ("POST", "/reviews/9001/held-back/ticket", Some("{}")),
    ];

    for (method, path, body) in missing {
        let answer = harness.api(method, path, body);
        assert_eq!(
            answer.status,
            404,
            "{method} {path} should be 404, not {}: {}",
            answer.status,
            answer.text()
        );
        assert_eq!(answer.error_code(), "not_found");
    }
}

#[test]
fn an_unknown_rule_is_not_found_on_every_rule_operation() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    let missing = [
        ("/rules/9001/promote", r#"{"to_stage":"active"}"#),
        ("/rules/9001/dismiss", r#"{"reason":"no"}"#),
        ("/rules/9001/retire", r#"{"reason":"stale"}"#),
        ("/rules/9001/defer", r#"{"days":60}"#),
    ];

    for (path, body) in missing {
        let answer = harness.api("POST", path, Some(body));
        assert_eq!(answer.status, 404, "{path}: {}", answer.text());
        assert_eq!(answer.error_code(), "not_found");
    }
}

#[test]
fn a_path_no_operation_claims_is_not_found() {
    let harness = common::start_with_no_github();

    for path in [
        "/nonsense",
        "/reviews/abc",
        "/rules/1/explode",
        "/reviews/0",
    ] {
        let answer = harness.api("GET", path, None);
        assert_eq!(
            answer.status, 404,
            "{path} should be 404, not {}",
            answer.status
        );
        assert_eq!(answer.error_code(), "not_found");
    }
}

#[test]
fn a_wrong_method_is_refused_rather_than_answered() {
    let harness = common::start_with_no_github();

    // `GET /reviews` is `POST /reviews` with the wrong verb. It must not be read as the
    // inbox, and it must not be answered as though it were a route of its own.
    let answer = harness.api("GET", "/reviews", None);
    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "bad_request");
}

/// Call one operation with a body it would accept, so a 400 never stands in for a
/// handler that is missing.
pub fn call(
    harness: &common::Harness,
    operation: Operation,
    seeded: &common::Seeded,
) -> common::Answer {
    let route = operation
        .route()
        .replace("/reviews/1", &format!("/reviews/{}", seeded.review_id))
        .replace("/rules/1/", &format!("/rules/{}/", seeded.rule_id));
    let (method, path) = route.split_once(' ').expect("a method and a path");

    let body = match operation {
        Operation::StartReview => Some(
            r#"{"pr_url":"https://github.com/acme/portal/pull/500","head_sha":"3f9c2ab"}"#
                .to_string(),
        ),
        Operation::SetFindingDisposition { .. } => {
            Some(r#"{"disposition":"held_back","reason":"a nit, for the ticket"}"#.to_string())
        }
        Operation::SaveDraft { .. } => Some(r#"{"body":"A review body."}"#.to_string()),
        Operation::ApproveDraft { .. } => Some(r#"{"event":"comment"}"#.to_string()),
        Operation::SendBackDraft { .. } => Some(r#"{"note":"cut the nits"}"#.to_string()),
        Operation::RequestNitTicket { .. } => Some("{}".to_string()),
        // The seeded rule is `active`, so the stage that follows it is `lint`.
        Operation::PromoteRule { .. } => Some(r#"{"to_stage":"lint"}"#.to_string()),
        Operation::DismissRule { .. } | Operation::RetireRule { .. } => {
            Some(r#"{"reason":"no consequence named"}"#.to_string())
        }
        Operation::DeferRule { .. } => Some(r#"{"days":60}"#.to_string()),
        _ => None,
    };

    harness.api(method, path, body.as_deref())
}
