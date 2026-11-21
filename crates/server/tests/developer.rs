//! Signing off is the developer's, and of what they were shown — DEV-1346.
//!
//! Two halves, each with its own refusal. **The cookie**: a developer act without it is a
//! `401` that writes nothing, whatever else the request gets right. **The shown hash**: a
//! sign-off whose `shown` no longer matches the record is a `409` `changed_since_shown`
//! that writes nothing, which is the bug DEV-1345 names — a session revising the draft
//! while the developer reads it, and the click approving text they never saw.
//!
//! The tag test at the bottom is what keeps the two lists true: every operation the route
//! table calls a developer act is marked so in the contract, refuses without the cookie,
//! and — for the four sign-offs — refuses without a `shown`.

mod common;

use std::collections::BTreeSet;

use review_desk_server::route::Operation;
use review_desk_store::{DraftStatus, NewBrief, NewDraft};
use serde_json::{json, Value};

/// The draft's `shown`, as `GET …/draft` served it.
fn shown(harness: &common::Harness, review_id: i64) -> Value {
    harness.draft_shown(review_id)
}

/// The session writes a new body under the developer, as `review-desk draft put` does.
fn revise_the_draft(harness: &common::Harness, review_id: i64) {
    harness
        .store
        .put_draft(
            review_id,
            NewDraft::ready("A different body, written while the developer was reading.\n"),
        )
        .expect("the session rewrites the draft");
}

#[test]
fn approving_a_draft_revised_after_it_was_shown_is_409() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    let h1 = shown(&harness, id);
    revise_the_draft(&harness, id);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{id}/draft/approve"),
        Some(&json!({ "event": "comment", "shown": h1 }).to_string()),
    );

    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "changed_since_shown");
    let draft = harness.store.get_draft(id).unwrap();
    assert_eq!(draft.status, DraftStatus::Ready, "nothing was approved");
    assert_eq!(
        draft.approved_shown, None,
        "nothing was recorded as approved"
    );
}

#[test]
fn approving_what_was_shown_records_it_as_what_was_approved() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    let h1 = shown(&harness, id);
    let answer = harness.api(
        "POST",
        &format!("/reviews/{id}/draft/approve"),
        Some(&json!({ "event": "comment", "shown": h1 }).to_string()),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let approved = harness.store.get_draft(id).unwrap().approved_shown.unwrap();
    assert_eq!(serde_json::to_value(&approved).unwrap(), h1);
}

#[test]
fn sending_back_a_draft_revised_after_it_was_shown_is_409() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    let h1 = shown(&harness, id);
    revise_the_draft(&harness, id);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{id}/draft/send-back"),
        Some(&json!({ "note": "F2 is not about this diff", "shown": h1 }).to_string()),
    );

    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "changed_since_shown");
    let draft = harness.store.get_draft(id).unwrap();
    assert_eq!(draft.status, DraftStatus::Ready);
    assert_eq!(draft.sent_back_note, None, "no note was stored");
    assert!(
        harness.store.messages(id).unwrap().is_empty(),
        "no send-back message was left in the conversation"
    );
}

#[test]
fn ticketing_and_approving_a_draft_revised_after_it_was_shown_is_409() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    let h1 = shown(&harness, id);
    let findings_before = harness.store.list_findings(id).unwrap();
    revise_the_draft(&harness, id);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{id}/draft/ticket-and-approve"),
        Some(&json!({ "shown": h1 }).to_string()),
    );

    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "changed_since_shown");
    assert_eq!(
        harness.store.get_draft(id).unwrap().status,
        DraftStatus::Ready
    );
    assert_eq!(
        harness.store.list_findings(id).unwrap(),
        findings_before,
        "no finding was moved into a ticket"
    );
}

#[test]
fn confirming_a_brief_rewritten_after_it_was_shown_is_409() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    let shown = harness.brief_shown(id);
    harness
        .store
        .put_brief(
            id,
            NewBrief {
                problem: "A different problem, written while the developer was reading.".into(),
                ..NewBrief::default()
            },
        )
        .expect("the session rewrites the brief");

    let answer = harness.api(
        "POST",
        &format!("/reviews/{id}/brief/confirm"),
        Some(&json!({ "shown_brief_sha256": shown }).to_string()),
    );

    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "changed_since_shown");
    assert!(!harness.store.get_brief(id).unwrap().is_confirmed());
}

#[test]
fn confirming_the_same_brief_twice_from_one_page_is_one_confirmation() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    // The hash is taken without the confirmation, so the page's second click is not a move.
    let body = json!({ "shown_brief_sha256": harness.brief_shown(id) }).to_string();
    let path = format!("/reviews/{id}/brief/confirm");
    assert_eq!(harness.api("POST", &path, Some(&body)).status, 200);
    assert_eq!(harness.api("POST", &path, Some(&body)).status, 200);
}

#[test]
fn approving_with_a_valid_host_and_origin_but_no_cookie_is_401() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;
    let base = harness.running.base();
    let host = base.trim_start_matches("http://").to_string();
    let body = json!({ "event": "approve", "shown": shown(&harness, id) }).to_string();

    // Exactly what `curl -X POST …/draft/approve` sends, given the right Host and Origin.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{id}/draft/approve"),
        &[
            ("Host", host.as_str()),
            ("Origin", base.as_str()),
            ("Content-Type", "application/json"),
        ],
        Some(&body),
    );

    assert_eq!(answer.status, 401, "{}", answer.text());
    assert_eq!(answer.error_code(), "not_the_developer");
    assert_eq!(
        harness.store.get_draft(id).unwrap().status,
        DraftStatus::Ready
    );
}

#[test]
fn a_cookie_this_run_did_not_hand_out_is_not_the_developer() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();
    let body = json!({ "event": "approve", "shown": shown(&harness, id) }).to_string();

    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{id}/draft/approve"),
        &[
            ("Host", host.as_str()),
            ("Content-Type", "application/json"),
            (
                "Cookie",
                "rd_developer=0000000000000000000000000000000000000000",
            ),
        ],
        Some(&body),
    );

    assert_eq!(answer.status, 401, "{}", answer.text());
}

#[test]
fn the_token_buys_one_cookie_and_is_then_spent() {
    let (harness, token) = common::start_holding_its_token();
    let body = json!({ "token": token }).to_string();

    let first = harness.as_anyone("POST", "/developer/sign-in", Some(&body));
    assert_eq!(first.status, 200, "{}", first.text());
    let set = first.header("set-cookie").expect("a cookie is set");
    assert!(set.starts_with("rd_developer="), "{set}");
    assert!(set.contains("HttpOnly"), "{set}");
    assert!(set.contains("SameSite=Strict"), "{set}");
    assert!(
        !set.contains(&token),
        "the cookie is a value of its own, not the token"
    );
    assert!(
        !first.text().contains(&token),
        "the token is in no response body"
    );

    let second = harness.as_anyone("POST", "/developer/sign-in", Some(&body));
    assert_eq!(second.status, 401, "{}", second.text());
    assert_eq!(second.error_code(), "not_the_developer");
    assert!(second.header("set-cookie").is_none());
}

#[test]
fn a_wrong_token_is_refused_and_does_not_spend_the_right_one() {
    let (harness, token) = common::start_holding_its_token();

    let wrong = harness.as_anyone(
        "POST",
        "/developer/sign-in",
        Some(&json!({ "token": "f".repeat(64) }).to_string()),
    );
    assert_eq!(wrong.status, 401);

    let right = harness.as_anyone(
        "POST",
        "/developer/sign-in",
        Some(&json!({ "token": token }).to_string()),
    );
    assert_eq!(right.status, 200, "{}", right.text());
}

#[test]
fn reads_and_the_other_writes_stay_open() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let id = seeded.review_id;

    for path in [
        "/inbox".to_string(),
        format!("/reviews/{id}"),
        format!("/reviews/{id}/draft"),
    ] {
        let answer = harness.as_anyone("GET", &path, None);
        assert_eq!(answer.status, 200, "GET {path}: {}", answer.text());
    }

    // Saving a draft is not a sign-off, and it moves the revision, so nothing shown before
    // it can be signed off after it.
    let before = shown(&harness, id);
    let saved = harness.as_anyone(
        "PUT",
        &format!("/reviews/{id}/draft"),
        Some(&json!({ "body": "A body the developer typed.\n" }).to_string()),
    );
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_ne!(shown(&harness, id)["draft_rev"], before["draft_rev"]);
}

// -- the tag test ---------------------------------------------------------------------

/// The contract's operations that name `developerCookie`.
fn contract_developer_acts() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.yaml");
    let contract: Value =
        serde_norway::from_str(&std::fs::read_to_string(path).expect("the contract"))
            .expect("YAML");
    contract["paths"]
        .as_object()
        .expect("paths")
        .values()
        .filter_map(Value::as_object)
        .flat_map(|methods| methods.values())
        .filter(|operation| {
            operation["security"]
                .as_array()
                .is_some_and(|all| all.iter().any(|one| one.get("developerCookie").is_some()))
        })
        .filter_map(|operation| operation["operationId"].as_str().map(str::to_string))
        .collect()
}

/// What the contract requires in the body of this operation, by field name.
fn required_in_body(operation_id: &str) -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.yaml");
    let contract: Value =
        serde_norway::from_str(&std::fs::read_to_string(path).expect("the contract"))
            .expect("YAML");
    let operation = contract["paths"]
        .as_object()
        .expect("paths")
        .values()
        .filter_map(Value::as_object)
        .flat_map(|methods| methods.values())
        .find(|operation| operation["operationId"] == operation_id)
        .unwrap_or_else(|| panic!("{operation_id} is in the contract"))
        .clone();
    let reference = operation["requestBody"]["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap_or_else(|| panic!("{operation_id} takes a body"));
    let name = reference.rsplit('/').next().unwrap();
    contract["components"]["schemas"][name]["required"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// **Every operation tagged as a developer act takes both the shown hash and the developer
/// extractor, and a test fails if one does not.** Three lists are held together here: the
/// route table's, the contract's `security`, and what actually happens over the socket.
#[test]
fn every_developer_act_needs_the_cookie_and_every_sign_off_needs_what_was_shown() {
    let tagged: BTreeSet<String> = Operation::ALL
        .iter()
        .filter(|operation| operation.is_developer_act())
        .map(|operation| operation.id().to_string())
        .collect();
    assert_eq!(
        tagged,
        contract_developer_acts(),
        "the route table and the contract disagree about which operations are the developer's"
    );
    assert_eq!(tagged.len(), 6, "the six DEV-1346 names: {tagged:?}");

    for operation in Operation::ALL {
        if operation.binds_what_was_shown() {
            assert!(
                operation.is_developer_act(),
                "{} is held to what was shown but needs no cookie",
                operation.id()
            );
            let required = required_in_body(operation.id());
            assert!(
                required.contains("shown") || required.contains("shown_brief_sha256"),
                "{} is a sign-off and the contract does not require what was shown: {required:?}",
                operation.id()
            );
        }

        if !operation.is_developer_act() {
            continue;
        }

        // Without the cookie: refused before anything else is looked at, and nothing written.
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);
        let id = seeded.review_id;
        let route = operation
            .route()
            .replace("/reviews/1", &format!("/reviews/{id}"));
        let (method, path) = route.split_once(' ').unwrap();
        let before = harness.store.full_review(id).unwrap();

        let refused = harness.as_anyone(method, path, Some("{}"));
        assert_eq!(
            refused.status,
            401,
            "{} answered {} without the cookie: {}",
            operation.id(),
            refused.status,
            refused.text()
        );
        assert_eq!(
            harness.store.full_review(id).unwrap(),
            before,
            "{} wrote something without the cookie",
            operation.id()
        );

        // With the cookie and without what was shown: refused as a malformed request.
        if operation.binds_what_was_shown() {
            let body = match operation {
                Operation::ApproveDraft { .. } => json!({ "event": "comment" }),
                Operation::SendBackDraft { .. } => json!({ "note": "change F2" }),
                _ => json!({}),
            };
            let address = harness
                .running
                .base()
                .trim_start_matches("http://")
                .to_string();
            let cookie = format!("rd_developer={}", harness.cookie);
            let answer = harness.raw(
                method,
                &format!("/api/v1{path}"),
                &[
                    ("Host", address.as_str()),
                    ("Content-Type", "application/json"),
                    ("Cookie", cookie.as_str()),
                ],
                Some(&body.to_string()),
            );
            assert_eq!(
                answer.status,
                400,
                "{} accepted a sign-off that did not say what was shown: {}",
                operation.id(),
                answer.text()
            );
        }
    }
}
