//! Every response this server actually serves, validated against `contract/openapi.yaml`.
//!
//! The contract is the agreement between this server and the dashboard, and an agreement
//! nothing checks is a document. So the schemas are read from the contract file itself —
//! not transcribed into Rust types, which would only check the server against my reading
//! of it — and each response is validated against the schema the operation declares.
//!
//! `openapi.yaml` is OpenAPI 3.1, whose schema objects *are* JSON Schema 2020-12, so a
//! schema can be validated as it stands: the whole document is handed to the validator
//! with a `$ref` at its root, which is what lets `#/components/schemas/...` resolve.
//!
//! **The validator is itself under test.** `a_wrong_body_fails_validation` exists because
//! a conformance test that silently validates nothing — an unresolved `$ref`, a schema
//! that ignores `required` — passes every time and means nothing.

mod common;

use jsonschema::Validator;
use serde_json::{json, Value};

/// The contract, as JSON.
fn contract() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.yaml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    serde_norway::from_str(&text).expect("contract/openapi.yaml is not readable as YAML")
}

/// A validator for one named schema in the contract.
fn validator(schema_name: &str) -> Validator {
    let mut document = contract();
    assert!(
        document["components"]["schemas"][schema_name].is_object(),
        "contract/openapi.yaml has no schema named {schema_name}"
    );

    // A `$ref` at the root, with the rest of the document still there so the reference
    // resolves inside it. Unknown keywords — `openapi`, `info`, `paths` — are ignored by
    // a JSON Schema validator, which is what makes this work.
    document["$ref"] = json!(format!("#/components/schemas/{schema_name}"));

    jsonschema::validator_for(&document)
        .unwrap_or_else(|error| panic!("building a validator for {schema_name}: {error}"))
}

/// Validate, and say what was wrong if it is not valid.
#[track_caller]
fn assert_valid(schema_name: &str, instance: &Value) {
    let validator = validator(schema_name);
    let problems: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| format!("  at {}: {error}", error.instance_path()))
        .collect();

    assert!(
        problems.is_empty(),
        "this body does not validate against {schema_name}:\n{}\nbody was:\n{}",
        problems.join("\n"),
        serde_json::to_string_pretty(instance).unwrap()
    );
}

#[track_caller]
fn assert_invalid(schema_name: &str, instance: &Value, why: &str) {
    let validator = validator(schema_name);
    assert!(
        !validator.is_valid(instance),
        "{schema_name} accepted a body it should have refused ({why}):\n{}",
        serde_json::to_string_pretty(instance).unwrap()
    );
}

// ---------------------------------------------------------------------------
// The validator is doing something
// ---------------------------------------------------------------------------

#[test]
fn the_validator_refuses_a_wrong_body() {
    // A required field left out.
    assert_invalid(
        "Review",
        &json!({ "id": 1, "repo": "acme/portal", "pr": 412, "round": 1,
                 "created_at": "2026-10-09T00:00:00Z" }),
        "head_sha is required",
    );

    // A field the contract does not declare. Every object sets
    // additionalProperties: false, which is what makes a mistyped field name a failure
    // here rather than a field the dashboard silently never sees.
    assert_invalid(
        "Review",
        &json!({ "id": 1, "repo": "acme/portal", "pr": 412, "head_sha": "3f9c2ab",
                 "round": 1, "created_at": "2026-10-09T00:00:00Z", "headSha": "3f9c2ab" }),
        "headSha is not a field of Review",
    );

    // A closed set broken.
    assert_invalid(
        "Error",
        &json!({ "code": "teapot", "message": "I am a teapot." }),
        "teapot is not an ErrorCode",
    );

    // A pattern broken: `key` is `^F[0-9]+$`.
    assert_invalid(
        "Finding",
        &json!({ "review_id": 1, "seq": 1, "key": "finding-1", "lens": "x",
                 "severity": "nit", "claim": "c", "disposition": "in_draft",
                 "created_at": "2026-10-09T00:00:00Z" }),
        "finding-1 does not match ^F[0-9]+$",
    );

    // And the shape that should pass, so a validator that refuses everything is caught
    // as readily as one that accepts everything.
    assert_valid(
        "Error",
        &json!({ "code": "not_implemented", "message": "Not built yet." }),
    );
}

#[test]
fn not_implemented_is_in_the_contracts_error_codes() {
    let contract = contract();
    let codes = contract["components"]["schemas"]["ErrorCode"]["enum"]
        .as_array()
        .expect("ErrorCode is an enum")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();

    assert!(
        codes.contains(&"not_implemented"),
        "DEV-861 serves 501 not_implemented, so the contract has to allow the code; \
         it allows {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The three operations that are implemented
// ---------------------------------------------------------------------------

#[test]
fn start_review_validates_as_a_review() {
    let harness = common::start(common::recorded_github());

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    assert_valid("Review", &answer.json());
}

#[test]
fn start_review_with_a_supplied_head_validates_as_a_review() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    assert_valid("Review", &answer.json());
}

#[test]
fn confirm_brief_validates_as_a_brief() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", seeded.review_id),
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("Brief", &answer.json());
}

#[test]
fn set_finding_disposition_validates_as_a_finding() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for body in [
        r#"{"disposition":"held_back","reason":"a nit; it goes in the ticket"}"#,
        r#"{"disposition":"suppressed","reason":"no consequence named"}"#,
        r#"{"disposition":"in_draft"}"#,
    ] {
        let answer = harness.api(
            "PATCH",
            &format!("/reviews/{}/findings/1", seeded.review_id),
            Some(body),
        );

        assert_eq!(answer.status, 200, "{}", answer.text());
        assert_valid("Finding", &answer.json());
    }
}

// ---------------------------------------------------------------------------
// Every failure, including the thirteen that wait on the store
// ---------------------------------------------------------------------------

#[test]
fn every_unimplemented_operation_validates_as_an_error() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for operation in review_desk_server::route::Operation::ALL {
        if operation.waits_on().is_none() {
            continue;
        }

        let route = operation
            .route()
            .replace("/reviews/1", &format!("/reviews/{}", seeded.review_id));
        let (method, path) = route.split_once(' ').unwrap();
        let answer = harness.api(method, path, Some("{}"));

        assert_eq!(answer.status, 501, "{route}: {}", answer.text());
        assert_valid("Error", &answer.json());
    }
}

#[test]
fn every_refusal_validates_as_an_error() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let review = seeded.review_id;

    let refusals: Vec<(&str, String, Option<&str>)> = vec![
        // 400, a malformed link
        (
            "POST",
            "/reviews".to_string(),
            Some(r#"{"pr_url":"nonsense"}"#),
        ),
        // 400, a body that is not the right shape
        ("POST", "/reviews".to_string(), Some(r#"{"nope":1}"#)),
        // 400, no body at all
        ("POST", "/reviews".to_string(), None),
        // 400, a closed set broken
        (
            "PATCH",
            format!("/reviews/{review}/findings/1"),
            Some(r#"{"disposition":"wontfix"}"#),
        ),
        // 400, a reason that is required and absent
        (
            "PATCH",
            format!("/reviews/{review}/findings/1"),
            Some(r#"{"disposition":"held_back"}"#),
        ),
        // 404, no such review
        ("POST", "/reviews/9001/brief/confirm".to_string(), None),
        // 404, no such endpoint
        ("GET", "/nonsense".to_string(), None),
        // 502, GitHub could not be read
        (
            "POST",
            "/reviews".to_string(),
            Some(r#"{"pr_url":"https://github.com/acme/portal/pull/77"}"#),
        ),
        // 501, a field the store cannot hold
        (
            "POST",
            "/reviews".to_string(),
            Some(
                r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab","title":"t"}"#,
            ),
        ),
    ];

    for (method, path, body) in refusals {
        let answer = harness.api(method, &path, body);
        assert!(
            answer.status >= 400,
            "{method} {path} was expected to be refused; it answered {}",
            answer.status
        );
        assert_valid("Error", &answer.json());
    }
}

// ---------------------------------------------------------------------------
// The one conversion the contract asks the server to make
// ---------------------------------------------------------------------------

#[test]
fn every_time_on_the_wire_is_rfc_3339_in_utc() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let created = harness
        .api(
            "POST",
            "/reviews",
            Some(r#"{"pr_url":"https://github.com/acme/portal/pull/500","head_sha":"3f9c2ab"}"#),
        )
        .json();
    let confirmed = harness
        .api(
            "POST",
            &format!("/reviews/{}/brief/confirm", seeded.review_id),
            None,
        )
        .json();

    let mut times = Vec::new();
    for body in [&created, &confirmed] {
        collect_times(body, &mut times);
    }

    assert!(!times.is_empty(), "no times were served to check");
    for time in times {
        assert!(
            looks_like_rfc_3339_utc(&time),
            "{time:?} is not RFC 3339 in UTC; the store holds epoch seconds and the \
             server converts at this boundary"
        );
    }
}

/// Every string value under a key the contract declares as a time.
fn collect_times(body: &Value, found: &mut Vec<String>) {
    let Some(object) = body.as_object() else {
        return;
    };
    for (key, value) in object {
        let is_a_time = key.ends_with("_at") || key == "checked_at";
        match value {
            Value::String(text) if is_a_time => found.push(text.clone()),
            Value::Object(_) => collect_times(value, found),
            Value::Array(items) => items.iter().for_each(|item| collect_times(item, found)),
            _ => {}
        }
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ`, which is what the contract's `Z` offset means.
fn looks_like_rfc_3339_utc(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 20
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && bytes[10] == b'T'
        && bytes[11..13].iter().all(u8::is_ascii_digit)
        && bytes[13] == b':'
        && bytes[14..16].iter().all(u8::is_ascii_digit)
        && bytes[16] == b':'
        && bytes[17..19].iter().all(u8::is_ascii_digit)
        && bytes[19] == b'Z'
}
