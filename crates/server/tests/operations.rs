//! Every one of the contract's sixteen operations answers, and each one that cannot be
//! answered in full says so in the contract's error shape.
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
fn every_operation_answers() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for operation in Operation::ALL {
        let answer = call(&harness, *operation, seeded.review_id);
        assert_ne!(
            answer.status,
            404,
            "{} ({}) is not routed: the server answered 404 for a well-formed path",
            operation.route(),
            operation.id()
        );
        assert!(
            answer.status < 500 || answer.status == 501,
            "{} answered {}, which is Review Desk being broken rather than an answer",
            operation.route(),
            answer.status
        );
    }
}

#[test]
fn every_unimplemented_operation_returns_501_naming_what_it_waits_on() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let mut waiting = 0;
    for operation in Operation::ALL {
        let Some(waits_on) = operation.waits_on() else {
            continue;
        };
        waiting += 1;

        let answer = call(&harness, *operation, seeded.review_id);
        assert_eq!(
            answer.status,
            501,
            "{} waits on {waits_on} and must answer 501; it answered {} with {}",
            operation.route(),
            answer.status,
            answer.text()
        );
        assert_eq!(answer.error_code(), "not_implemented");

        // The message has to name the store item, because the point of the 501 is to say
        // what would make it work rather than merely that it does not.
        let message = answer.json()["message"].as_str().unwrap().to_string();
        let first_item = waits_on.split_whitespace().next().unwrap();
        assert!(
            message.contains(first_item),
            "{} should name {first_item} in its message; it said {message:?}",
            operation.route()
        );
        assert!(
            message.contains(operation.id()),
            "{} should name its operationId; it said {message:?}",
            operation.route()
        );
    }

    assert_eq!(
        waiting, 13,
        "three operations are implemented against the store on master and thirteen wait"
    );
}

#[test]
fn an_unknown_review_is_not_found_rather_than_not_implemented() {
    let harness = common::start_with_no_github();

    // Checked on an implemented operation: a 501 would be hiding a 404, and a dashboard
    // cannot tell "this review is gone" from "this endpoint is not built" if the server
    // answers the same way for both.
    let answer = harness.api("POST", "/reviews/9001/brief/confirm", None);
    assert_eq!(answer.status, 404);
    assert_eq!(answer.error_code(), "not_found");
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
    // inbox, and it must not 501 as though startReview were unbuilt.
    let answer = harness.api("GET", "/reviews", None);
    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "bad_request");
}

/// Call one operation with a body it would accept, so a 400 never stands in for a 501.
fn call(harness: &common::Harness, operation: Operation, review_id: i64) -> common::Answer {
    let route = operation
        .route()
        .replace("/reviews/1", &format!("/reviews/{review_id}"));
    let (method, path) = route.split_once(' ').expect("a method and a path");

    let body = match operation {
        Operation::StartReview => {
            Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#)
        }
        Operation::SetFindingDisposition { .. } => {
            Some(r#"{"disposition":"held_back","reason":"a nit, for the ticket"}"#)
        }
        Operation::SaveDraft { .. } => Some(r#"{"body":"A review body."}"#),
        Operation::ApproveDraft { .. } => Some(r#"{"event":"comment"}"#),
        Operation::SendBackDraft { .. } => Some(r#"{"note":"cut the nits"}"#),
        Operation::RequestNitTicket { .. } => Some("{}"),
        Operation::PromoteRule { .. } => Some(r#"{"to_stage":"active"}"#),
        Operation::DismissRule { .. } | Operation::RetireRule { .. } => {
            Some(r#"{"reason":"no consequence named"}"#)
        }
        Operation::DeferRule { .. } => Some(r#"{"days":60}"#),
        _ => None,
    };

    harness.api(method, path, body)
}
