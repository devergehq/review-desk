//! Every one of the contract's thirty operations is routed and answers.
//!
//! This walks `Operation::ALL` rather than listing routes by hand, so an operation added
//! to the contract and not to the server fails here instead of 404ing in a browser.

mod common;

use std::collections::BTreeSet;

use review_desk_server::route::Operation;
use serde_json::Value;

/// The thirty operations, as `contract/README.md` counts them.
#[test]
fn the_contract_has_thirty_operations() {
    assert_eq!(Operation::ALL.len(), 30);
}

/// The contract's own `paths` and this crate's `Operation` name the same set.
///
/// **`route.rs` claimed this test existed, and it did not.** The claim was that "an
/// operation added to `openapi.yaml` and not to the server fails a test instead of 404ing
/// in a browser", and nothing in the repository tied the two together: the count above was
/// a literal that read no file, and the conformance test reads the contract only at three
/// hard-coded schema pointers. So an operation could be added to the contract, given an
/// example, made producible from the store — and left unrouted, with every Rust test green
/// and the dashboard getting a 400 from a path the contract says answers.
///
/// Found by a reader attacking DEV-930's plan, which was about to walk into it: the whole
/// of the work was contract, store, examples and screens, and nothing would have failed if
/// `route.rs` had been missed.
#[test]
fn the_contract_and_this_type_name_the_same_operations() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.yaml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    let contract: Value =
        serde_norway::from_str(&text).expect("contract/openapi.yaml is not readable as YAML");

    let paths = contract["paths"]
        .as_object()
        .expect("the contract declares paths");
    let declared: BTreeSet<&str> = paths
        .values()
        .filter_map(Value::as_object)
        .flat_map(|methods| methods.values())
        .filter_map(|operation| operation.get("operationId"))
        .filter_map(Value::as_str)
        .collect();

    assert!(
        declared.len() > 15,
        "only {} operationIds were read out of the contract, so this check is vacuous",
        declared.len()
    );

    let routed: BTreeSet<&str> = Operation::ALL.iter().map(|one| one.id()).collect();

    assert_eq!(
        declared,
        routed,
        "the contract's operations and this crate's are not the same set. \
         In the contract and not routed: {:?}. Routed and not in the contract: {:?}",
        declared.difference(&routed).collect::<Vec<_>>(),
        routed.difference(&declared).collect::<Vec<_>>(),
    );
}

#[test]
fn every_operation_answers_from_a_seeded_record() {
    for operation in Operation::ALL {
        // A record of its own per operation. Several of these write — approving a draft,
        // promoting a rule — so a shared record would make the answers depend on the
        // order of the loop, and the loop's order is not a thing this test is about.
        // **A fresh session is asked for only when Allele says the session is gone**, so
        // that one operation is given an Allele that says so — and the review that records
        // a session, in `call`.
        let (_allele, harness) = match operation {
            Operation::RequestFreshSession { .. } => {
                let (dir, socket, _) = common::fake_allele(
                    r#"{"result":"error","code":"bad_request","message":"no session with id x"}"#,
                );
                (Some(dir), common::start_reading_sessions_from(&socket))
            }
            _ => (None, common::start_with_no_github()),
        };
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
        ("GET", "/reviews/9001/brief/problem-statement", None),
        ("GET", "/reviews/9001/session", None),
        ("GET", "/reviews/9001/messages", None),
        ("POST", "/reviews/9001/fresh-session", None),
        (
            "POST",
            "/reviews/9001/messages",
            Some(r#"{"step":"draft","body":"hello"}"#),
        ),
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
        (
            "POST",
            "/reviews/9001/findings/1/decide",
            Some(r#"{"decision":"cut"}"#),
        ),
        ("POST", "/reviews/9001/draft/ticket-and-approve", Some("{}")),
        (
            "POST",
            "/reviews/9001/dismiss",
            Some(r#"{"reason":"the pull request was closed unmerged"}"#),
        ),
        ("POST", "/reviews/9001/restore", None),
        ("DELETE", "/reviews/9001", None),
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

    // **Repointed, not deleted.** `GET /reviews` was this test's only case, and DEV-930
    // made that a real route — leaving the `not_found`/`bad_request` split, and the whole
    // `(target, allowed)` mechanism in `route.rs` that draws it, with no test at all. A
    // refactor collapsing `WrongMethod` into `Unknown` would then have turned every wrong
    // verb into a 404 and passed everything.
    //
    // `POST /inbox` is a path with one verb; `DELETE /reviews` is a path with two, which
    // is the shape `/reviews` now has and the one a third verb has to fall through.
    for (method, path) in [("POST", "/inbox"), ("DELETE", "/reviews")] {
        let answer = harness.api(method, path, None);
        assert_eq!(
            answer.status, 400,
            "{method} {path} is a wrong verb on a path that exists, not an unknown path"
        );
        assert_eq!(answer.error_code(), "bad_request");
    }

    // And the path's two real verbs each reach their own handler, which is what stops one
    // shadowing the other: a `GET` arm that swallowed `POST` would break starting a
    // review, and both failures are a 400 — so the status cannot tell them apart.
    assert_eq!(harness.api("GET", "/reviews", None).status, 200);
    let started = harness.api("POST", "/reviews", Some("{}"));
    assert_eq!(started.status, 400);
    assert!(
        started.text().contains("pr_url"),
        "POST /reviews is refused by its own handler for want of a link, not by the \
         router for want of a route; it said {}",
        started.text()
    );
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

    // **Three operations cannot be asked of the seeded review as it stands**, and that is
    // the record being right rather than the harness being awkward: the seeded review
    // holds a brief, findings and a draft, so a delete of it is correctly refused; it is
    // not dismissed, so a restore of it is correctly refused too; and it is its pull
    // request's only round, so folding it is correctly refused — a round folds into a newer
    // one, and it has none. Each is pointed at a review it can succeed on, so that this
    // test goes on being about whether the operation is routed and answers.
    let path = match operation {
        // A review of its own, with nothing written to it: what the Locus dispatcher
        // leaves behind when it records a request, and the only thing a delete is for.
        Operation::DeleteReview { .. } => {
            let fresh = harness
                .store
                .open_review("acme/portal", 777, "4f1a9c2")
                .expect("a review nobody has started");
            format!("/reviews/{}", fresh.review.id)
        }
        // Dismissed first, through the store, because there is nothing to restore
        // otherwise. The dismissal itself is exercised by its own operation above.
        Operation::RestoreReview { .. } => {
            harness
                .store
                .dismiss_review(seeded.review_id, "the pull request was closed unmerged")
                .expect("dismiss it so there is something to restore");
            path.to_string()
        }
        // An extra unposted round holding nothing, which is the only thing a fold is for.
        // Written through `create_review` because `open_review` cannot make one any more —
        // since DEV-962 a new commit on an unposted round moves it — and the record this
        // tidy exists for was written by the rule before that one.
        Operation::FoldRound { .. } => {
            let extra = harness
                .store
                .create_review(review_desk_store::NewReview {
                    repo: "acme/portal".to_string(),
                    pr: 778,
                    head_sha: "4d9464f".to_string(),
                    round: 1,
                    ..Default::default()
                })
                .expect("an extra round, as the old rule left them");
            harness
                .store
                .create_review(review_desk_store::NewReview {
                    repo: "acme/portal".to_string(),
                    pr: 778,
                    head_sha: "a41c9e2".to_string(),
                    round: 2,
                    ..Default::default()
                })
                .expect("and the newer round it folds into");
            format!("/reviews/{}/fold", extra.id)
        }
        // The review that records a session: the seeded one with a brief records none.
        Operation::RequestFreshSession { .. } => path.replace(
            &format!("/reviews/{}", seeded.review_id),
            &format!("/reviews/{}", seeded.running_review_id),
        ),
        _ => path.to_string(),
    };
    let path = path.as_str();

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
        Operation::AskSession { .. } => {
            Some(r#"{"step":"draft","body":"Why is F2 a nit?"}"#.to_string())
        }
        Operation::RequestNitTicket { .. } => Some("{}".to_string()),
        // Cut, which is the decision that needs the least beside it — and the seeded
        // draft is written in parts, so F1 has a block to cut.
        Operation::DecideFinding { .. } => Some(r#"{"decision":"cut"}"#.to_string()),
        Operation::TicketRestAndApprove { .. } => Some("{}".to_string()),
        // The seeded rule is `active`, so the stage that follows it is `lint`.
        Operation::PromoteRule { .. } => Some(r#"{"to_stage":"lint"}"#.to_string()),
        Operation::DismissRule { .. } | Operation::RetireRule { .. } => {
            Some(r#"{"reason":"no consequence named"}"#.to_string())
        }
        Operation::DismissReview { .. } => {
            Some(r#"{"reason":"the pull request was closed unmerged"}"#.to_string())
        }
        Operation::DeferRule { .. } => Some(r#"{"days":60}"#.to_string()),
        _ => None,
    };

    harness.api(method, path, body.as_deref())
}
