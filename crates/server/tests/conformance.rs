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

/// A validator for a schema in the contract, named or pointed at.
///
/// A name is `#/components/schemas/<name>`; anything containing a `/` is taken as a JSON
/// pointer, which is how an operation whose response schema is declared inline — `/repos`
/// is the only one — gets validated against the contract rather than against a
/// transcription of it.
fn validator(schema: &str) -> Validator {
    let pointer = if schema.contains('/') {
        schema.to_string()
    } else {
        format!("#/components/schemas/{schema}")
    };

    let mut document = contract();
    assert!(
        document
            .pointer(pointer.trim_start_matches('#'))
            .is_some_and(Value::is_object),
        "contract/openapi.yaml has nothing at {pointer}"
    );

    // A `$ref` at the root, with the rest of the document still there so the reference
    // resolves inside it. Unknown keywords — `openapi`, `info`, `paths` — are ignored by
    // a JSON Schema validator, which is what makes this work.
    document["$ref"] = json!(pointer);

    jsonschema::validator_for(&document)
        .unwrap_or_else(|error| panic!("building a validator for {pointer}: {error}"))
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
        &json!({ "code": "conflict", "message": "The draft is approved." }),
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
fn the_problem_statement_validates_and_carries_nothing_of_the_change() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "GET",
        &format!("/reviews/{}/brief/problem-statement", seeded.review_id),
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("ProblemStatement", &answer.json());

    // `additionalProperties: false` on the schema is what makes the validation above a
    // statement about what is absent as well as what is present. These are the names the
    // change half would arrive under, asserted by name so a field added to `Brief` and
    // copied into this read by habit fails here.
    let served = answer.json();
    let fields = served.as_object().expect("an object");
    for change_half in [
        "headline",
        "diagram",
        "flow_after",
        "flow_after_caption",
        "support_table",
        "options",
        "blind_pass",
        "blind_pass_state",
        "parts",
        "open_choices",
        "disagreements",
        "approach_verdict",
    ] {
        assert!(
            !fields.contains_key(change_half),
            "{change_half} is the change half and reached the problem statement"
        );
    }
    for problem in served["problems"].as_array().expect("the problems") {
        let problem = problem.as_object().expect("a problem");
        assert!(
            !problem.contains_key("now_fixed"),
            "the \"now\" column leaked"
        );
        assert!(!problem.contains_key("detail_before"), "the detail leaked");
        assert!(!problem.contains_key("detail_after"), "the detail leaked");
    }
}

#[test]
fn a_brief_with_every_row_on_it_validates_whole() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", seeded.review_id),
        None,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());

    let brief = answer.json();
    assert_valid("Brief", &brief);

    // The seed writes every one of the twelve rows, so this is the contract's own
    // statement that the server serves each of them.
    assert!(!brief["flow_before"]
        .as_array()
        .expect("the flow")
        .is_empty());
    assert!(!brief["flow_after"].as_array().expect("the flow").is_empty());
    assert!(!brief["problems"]
        .as_array()
        .expect("the problems")
        .is_empty());
    assert!(brief["support_table"].is_object());
    assert!(brief["blind_pass"].is_object());
    assert_eq!(brief["blind_pass_state"], json!("ran"));
    assert!(!brief["parts"].as_array().expect("the parts").is_empty());
    assert!(!brief["open_choices"]
        .as_array()
        .expect("the choices")
        .is_empty());
    assert!(!brief["disagreements"]
        .as_array()
        .expect("the clashes")
        .is_empty());
    assert!(!brief["options"][0]["verdicts"]
        .as_array()
        .expect("the verdicts")
        .is_empty());
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
// The thirteen the store can answer since DEV-860
// ---------------------------------------------------------------------------

#[test]
fn the_inbox_validates() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("GET", "/inbox", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("Inbox", &answer.json());

    let inbox = answer.json();
    assert!(
        !inbox["briefs_to_confirm"].as_array().unwrap().is_empty(),
        "the seeded record has an unconfirmed brief: {inbox}"
    );
    assert!(
        !inbox["drafts_to_approve"].as_array().unwrap().is_empty(),
        "the seeded record has a draft waiting: {inbox}"
    );
    assert_eq!(
        inbox["in_progress"][0]["layers_total"], 7,
        "the screen's \"Layer 5 of 7\" needs the total stated"
    );
    assert!(
        inbox["in_progress"][0]["session_alive"].is_null(),
        "Allele owns session liveness and nothing tells this server about it; null is \
         the honest answer and the screen has to render it"
    );

    // **`totals.items` is the five waiting groups and nothing else**, which nothing
    // asserted until DEV-930 — and by then three files computed it three ways: the server
    // left out the developer's own pull requests, sample mode put them in, and the store's
    // example test added the reviews in progress as well. The sum is written down here so
    // a fourth answer fails rather than ships.
    let group = |name: &str| inbox[name].as_array().expect(name).len();
    assert_eq!(
        inbox["totals"]["items"]
            .as_u64()
            .map(|items| items as usize),
        Some(
            group("briefs_to_confirm")
                + group("drafts_to_approve")
                + group("author_replies")
                + group("own_prs_without_reviewer")
                + group("proposed_rules")
        ),
        "totals.items is the five waiting groups' combined length: {inbox}"
    );
    // **`in_progress` is not empty**, or the sum above would hold whether it counted or
    // not. Nothing in it asks the developer for anything, and the screen's headline counts
    // what does.
    //
    // The posted group *is* empty here — the seeded record has nothing posted — so its
    // term in that sum was vacuous. Posting one below is what makes it carry weight.
    assert!(
        group("in_progress") > 0,
        "the seeded record has a session running: {inbox}"
    );

    harness
        .store
        .approve_draft(
            seeded.review_id,
            review_desk_store::ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .expect("signed off");
    harness
        .store
        .post_draft(seeded.review_id, "gh-review-3401")
        .expect("posted");

    let after = harness.api("GET", "/inbox", None).json();
    assert_valid("Inbox", &after);
    let group = |name: &str| after[name].as_array().expect(name).len();
    assert!(
        group("posted_awaiting_author") > 0,
        "a posted review with an open pull request: {after}"
    );
    assert_eq!(
        after["totals"]["items"]
            .as_u64()
            .map(|items| items as usize),
        Some(
            group("briefs_to_confirm")
                + group("drafts_to_approve")
                + group("author_replies")
                + group("own_prs_without_reviewer")
                + group("proposed_rules")
        ),
        "the sum still names the five waiting groups once a review has been posted. \
         Posting changes what is in them — an approved draft leaves `drafts_to_approve` — \
         and what it must not change is which groups are counted: {after}"
    );
}

#[test]
fn a_posted_review_whose_pull_request_is_open_is_served_as_waiting_on_its_author() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // Before anything is posted the group is empty, and an empty array is a claim: it
    // says the record holds no posted review with an open pull request.
    let before = harness.api("GET", "/inbox", None).json();
    assert_valid("Inbox", &before);
    assert_eq!(
        before["posted_awaiting_author"].as_array().map(Vec::len),
        Some(0)
    );

    harness
        .store
        .approve_draft(
            seeded.review_id,
            review_desk_store::ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .expect("signed off");
    harness
        .store
        .post_draft(seeded.review_id, "gh-review-3312")
        .expect("posted");

    let answer = harness.api("GET", "/inbox", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let inbox = answer.json();
    assert_valid("Inbox", &inbox);

    let posted = &inbox["posted_awaiting_author"];
    assert_eq!(
        posted.as_array().map(Vec::len),
        Some(1),
        "an empty group would validate and check nothing: {inbox}"
    );
    let item = &posted[0];
    assert_eq!(item["review_id"], seeded.review_id);
    assert_eq!(item["posted_as"], "request_changes");
    assert!(item["posted_at"].is_string());
    assert_eq!(
        item["rounds_on_pr"], item["round"],
        "the group holds only the newest round, so these are the same number"
    );

    // **Nothing has harvested it, so the three figures are absent and not nought.**
    // Zero replies on a review nobody has read back is the absence of a measurement.
    assert!(item["harvested_at"].is_null());
    assert!(item["pr_state"].is_null(), "nothing has looked");
    for field in ["replies", "threads_resolved", "code_changed_after"] {
        assert!(
            item[field].is_null(),
            "{field} is served as null until something has looked: {item}"
        );
    }

    // And a merged pull request leaves the group: there is no author left to wait for.
    harness
        .store
        .set_review_pr_state(
            seeded.review_id,
            review_desk_store::PrState::Merged,
            Some(review_desk_store::now()),
        )
        .expect("merged");
    let after = harness.api("GET", "/inbox", None).json();
    assert_valid("Inbox", &after);
    assert_eq!(
        after["posted_awaiting_author"].as_array().map(Vec::len),
        Some(0),
        "merged at its last harvest: {after}"
    );
}

#[test]
fn every_pull_request_state_the_store_has_validates_on_the_wire() {
    // **Every value, not the one the happy path writes.** A contract enum missing a Rust
    // variant is caught here and nowhere else, which is the idiom the provenance tests
    // already use for `SourceKind` and `FileCheck`.
    for state in review_desk_store::PrState::ALL {
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);
        harness
            .store
            .set_review_pr_state(seeded.review_id, *state, Some(review_desk_store::now()))
            .expect("the state");

        let answer = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
        assert_eq!(answer.status, 200, "{}", answer.text());
        assert_valid("ReviewDetail", &answer.json());
        assert_eq!(
            answer.json()["review"]["pr_state"],
            state.as_str(),
            "the wire word for {state:?}"
        );
    }
}

#[test]
fn the_reviews_list_validates() {
    let harness = common::start_with_no_github();

    // Empty first: a list with nothing in it still has to be the shape it says, and that
    // is the shape a first run serves.
    let empty = harness.api("GET", "/reviews", None);
    assert_eq!(empty.status, 200, "{}", empty.text());
    assert_valid(REVIEWS_SCHEMA, &empty.json());
    assert!(
        empty.json()["repo"].is_null(),
        "no repository was asked for"
    );

    let seeded = common::seed(&harness.store);
    let answer = harness.api("GET", "/reviews", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let listed = answer.json();
    assert_valid(REVIEWS_SCHEMA, &listed);

    let rows = listed["reviews"].as_array().expect("the rows");
    assert!(
        !rows.is_empty(),
        "an empty list would validate and check nothing: {listed}"
    );
    let row = rows
        .iter()
        .find(|row| row["review"]["id"] == seeded.review_id)
        .expect("the seeded review is listed");
    assert_eq!(row["steps_total"], 7, "stated, so no screen carries a 7");
    assert!(
        row["step_state"].is_string(),
        "the state comes with the number: a bare step name reads as the step the review \
         is waiting at, and for a finished step that is the step it has left — {row}"
    );

    // The repository filter, and the refusal that stops a typo reading as an empty
    // record: "nothing here" and "no such repository" are different answers.
    let filtered = harness.api("GET", "/reviews?repo=acme/portal", None);
    assert_eq!(filtered.status, 200, "{}", filtered.text());
    assert_valid(REVIEWS_SCHEMA, &filtered.json());
    assert_eq!(filtered.json()["repo"], "acme/portal");

    let refused = harness.api("GET", "/reviews?repo=notarepo", None);
    assert_eq!(refused.status, 400);
    assert_eq!(refused.error_code(), "invalid_value");
    assert_valid("Error", &refused.json());
}

/// The inline 200 schema of `listReviews`, by pointer: like `listReviewTimings`, it has no
/// named component because only this operation serves that shape.
const REVIEWS_SCHEMA: &str = "#/paths/~1reviews/get/responses/200/content/application~1json/schema";

#[test]
fn an_empty_inbox_still_validates() {
    // Nothing seeded. Every group is an empty array and every figure is a real zero or a
    // null — the shape a first run serves, which is the one nobody tests and everybody
    // sees first.
    let harness = common::start_with_no_github();

    let answer = harness.api("GET", "/inbox", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("Inbox", &answer.json());
    assert_eq!(answer.json()["totals"]["items"], 0);
}

#[test]
fn the_repository_list_validates() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    let answer = harness.api("GET", "/repos", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid(
        "#/paths/~1repos/get/responses/200/content/application~1json/schema",
        &answer.json(),
    );

    let repos = answer.json();
    let names: Vec<String> = repos["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["repo"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"acme/portal".to_string())
            && names.contains(&"acme/billing-api".to_string()),
        "both seeded repositories should be listed; got {names:?}"
    );
}

#[test]
fn one_review_validates_whole() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("ReviewDetail", &answer.json());

    let detail = answer.json();
    assert_eq!(
        detail["layers"].as_array().unwrap().len(),
        7,
        "the contract requires exactly seven layers, written or not"
    );
    assert_eq!(
        detail["brief"]["options"].as_array().unwrap().len(),
        2,
        "the structured options come from the brief_option table: {detail}"
    );
    assert_eq!(detail["draft_status"], "ready");
    assert_eq!(detail["finding_dispositions"]["in_draft"], 1);
    assert_eq!(detail["finding_dispositions"]["held_back"], 1);
}

#[test]
fn a_review_with_nothing_on_it_still_validates() {
    // No brief, no findings, no draft, no layers written. Every optional thing is null
    // or empty and the seven layers are all `pending`.
    let harness = common::start_with_no_github();
    let bare = harness
        .store
        .open_review("acme/bare", 1, "abc1234")
        .unwrap()
        .review
        .id;

    let answer = harness.api("GET", &format!("/reviews/{bare}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("ReviewDetail", &answer.json());
    assert!(answer.json()["brief"].is_null());
    assert!(answer.json()["draft_status"].is_null());
    assert_eq!(answer.json()["layers"].as_array().unwrap().len(), 7);
    assert_eq!(answer.json()["coverage"]["source"], "none");
    assert!(answer.json()["description_lint"].is_null());
}

#[test]
fn a_review_that_recorded_where_its_understanding_came_from_validates_whole() {
    // The other half of `a_review_with_nothing_on_it_still_validates`: every one of the
    // four lists DEV-875 adds is present and non-empty, each closed set has a value on the
    // wire, and the coverage record is the change map rather than the sentence.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    harness
        .store
        .put_review_sources(
            seeded.review_id,
            review_desk_store::SourceKind::ALL
                .iter()
                .map(|kind| review_desk_store::NewReviewSource {
                    kind: *kind,
                    label: format!("read from the {kind}"),
                    link: Some("https://example.invalid/x".to_string()),
                    status: None,
                    note: None,
                })
                .collect(),
        )
        .expect("the sources");

    harness
        .store
        .put_description_lint(
            seeded.review_id,
            review_desk_store::DescriptionLintStatus::ALL
                .iter()
                .map(|status| review_desk_store::NewDescriptionLintResult {
                    label: format!("the {status} check"),
                    status: *status,
                    detail: Some("what it found".to_string()),
                })
                .collect(),
        )
        .expect("the description lint");

    harness
        .store
        .put_acceptance_criteria(
            seeded.review_id,
            vec![
                review_desk_store::NewAcceptanceCriterion {
                    text: "met, pointing at a place".to_string(),
                    status: review_desk_store::CriterionStatus::Met,
                    finding_seq: None,
                    file: Some("jobs/DeliverWebhook.php".to_string()),
                    line: Some(61),
                    note: None,
                },
                review_desk_store::NewAcceptanceCriterion {
                    text: "not met, pointing at a finding".to_string(),
                    status: review_desk_store::CriterionStatus::NotMet,
                    finding_seq: Some(1),
                    file: None,
                    line: None,
                    note: None,
                },
                review_desk_store::NewAcceptanceCriterion {
                    text: "not in this pull request, with a note".to_string(),
                    status: review_desk_store::CriterionStatus::NotInThisPr,
                    finding_seq: None,
                    file: None,
                    line: None,
                    note: Some("it follows in ABC-240".to_string()),
                },
            ],
        )
        .expect("the criteria");

    harness
        .store
        .put_file_changes(
            seeded.review_id,
            review_desk_store::FileCheck::ALL
                .iter()
                .enumerate()
                .map(|(index, checked)| review_desk_store::NewFileChange {
                    path: format!("src/file{index}.rs"),
                    summary: format!("looked at it: {checked}"),
                    lines_added: index as i64,
                    lines_removed: 0,
                    checked: *checked,
                    // Required for `not_reviewed` and harmless for the rest.
                    how_checked: Some(format!("{checked}, and here is why")),
                })
                .collect(),
        )
        .expect("the change map");

    let answer = harness.api("GET", &format!("/reviews/{}", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("ReviewDetail", &answer.json());

    let detail = answer.json();
    assert_eq!(detail["sources"].as_array().unwrap().len(), 3);
    assert_eq!(detail["criteria"].as_array().unwrap().len(), 3);
    assert_eq!(
        detail["description_lint"]["passes"], false,
        "one of the three results is a failure"
    );
    assert_eq!(detail["coverage"]["source"], "files");
    assert_eq!(detail["coverage"]["files"].as_array().unwrap().len(), 4);
    assert!(
        detail["not_checked"].is_null(),
        "the change map is the coverage record, so the sentence is not served as one"
    );
}

#[test]
fn the_draft_validates() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api("GET", &format!("/reviews/{}/draft", seeded.review_id), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("DraftDetail", &answer.json());

    let detail = answer.json();
    assert_eq!(detail["lint"]["passes"], true);
    assert_eq!(
        detail["held_back"].as_array().unwrap().len(),
        1,
        "the seeded record holds one nit back"
    );
    assert_eq!(
        detail["edits"].as_array().unwrap().len(),
        1,
        "a developer's cut is an edit the screen lists: {detail}"
    );
}

#[test]
fn a_draft_whose_lint_has_not_run_serves_a_null_lint() {
    let harness = common::start_with_no_github();
    let review_id = harness
        .store
        .open_review("acme/portal", 9, "abc1234")
        .unwrap()
        .review
        .id;
    harness
        .store
        .put_draft(
            review_id,
            review_desk_store::NewDraft {
                body: String::new(),
                status: review_desk_store::DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .unwrap();

    let answer = harness.api("GET", &format!("/reviews/{review_id}/draft"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("DraftDetail", &answer.json());
    assert!(
        answer.json()["lint"].is_null(),
        "a lint that has not run is null, not passes: false — the contract was changed \
         in this pull request to allow it"
    );
}

#[test]
fn saving_approving_and_sending_back_a_draft_all_validate() {
    for (method, suffix, body) in [
        ("PUT", "", r#"{"body":"An edited review body.","edits":[]}"#),
        ("POST", "/approve", r#"{"event":"request_changes"}"#),
        ("POST", "/send-back", r#"{"note":"cut the nits"}"#),
    ] {
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);
        let path = format!("/reviews/{}/draft{suffix}", seeded.review_id);

        let answer = harness.api(method, &path, Some(body));
        assert_eq!(answer.status, 200, "{method} {path}: {}", answer.text());
        assert_valid("DraftDetail", &answer.json());
    }
}

#[test]
fn the_nit_ticket_validates() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let path = format!("/reviews/{}/held-back/ticket", seeded.review_id);

    let asked = harness.api("POST", &path, Some("{}"));
    assert_eq!(asked.status, 200, "{}", asked.text());
    assert_valid("NitTicket", &asked.json());
    assert_eq!(asked.json()["status"], "requested");

    let filed = harness.api("POST", &path, Some(r#"{"ticket_id":"ABC-999"}"#));
    assert_eq!(filed.status, 200, "{}", filed.text());
    assert_valid("NitTicket", &filed.json());
    assert_eq!(filed.json()["status"], "filed");
    assert_eq!(filed.json()["ticket_id"], "ABC-999");
}

#[test]
fn the_finished_review_timings_validate() {
    // **The one new operation whose served response nothing validated.**
    // `tests/timings.rs` asserts its values and `store/tests/contract_examples.rs` checks
    // what the *store* can build; neither puts what the server actually writes past the
    // schema, which is this file's whole job — an agreement nothing checks is a document.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // Empty first: an operation whose list is empty still has to be the shape it says.
    let empty = harness.api("GET", "/timings", None);
    assert_eq!(empty.status, 200, "{}", empty.text());
    assert_valid(TIMINGS_SCHEMA, &empty.json());

    harness
        .store
        .approve_draft(
            seeded.review_id,
            review_desk_store::ReviewEvent::Comment,
            review_desk_store::Actor::Developer,
        )
        .expect("signed off");
    harness
        .store
        .post_draft(seeded.review_id, "gh-review-3312")
        .expect("posted");

    let answer = harness.api("GET", "/timings", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let listed = answer.json();
    assert_valid(TIMINGS_SCHEMA, &listed);
    assert_eq!(
        listed["reviews"].as_array().map(Vec::len),
        Some(1),
        "an empty list would validate and check nothing: {listed}"
    );
}

/// The inline 200 schema of `listReviewTimings`, by pointer: it has no named component,
/// being the one shape only this operation serves.
const TIMINGS_SCHEMA: &str = "#/paths/~1timings/get/responses/200/content/application~1json/schema";

#[test]
fn the_rules_page_validates() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    for path in ["/rules", "/rules?repo=acme/portal"] {
        let answer = harness.api("GET", path, None);
        assert_eq!(answer.status, 200, "{path}: {}", answer.text());
        assert_valid("RulesPage", &answer.json());
    }

    let all = harness.api("GET", "/rules", None).json();
    assert_eq!(
        all["proposed"].as_array().unwrap().len(),
        1,
        "the screen's whole purpose is deciding on proposals, so they are included: {all}"
    );
    assert_eq!(all["active"].as_array().unwrap().len(), 1);
    assert!(all["repo"].is_null(), "no repo means every repository");
    assert_eq!(all["summary"]["active_rules_cap"], 20);
}

#[test]
fn every_rule_operation_validates() {
    for (suffix, body) in [
        ("promote", r#"{"to_stage":"lint"}"#),
        ("dismiss", r#"{"reason":"no consequence named"}"#),
        ("retire", r#"{"reason":"nothing has cited it"}"#),
        ("defer", r#"{"days":60}"#),
    ] {
        let harness = common::start_with_no_github();
        let seeded = common::seed(&harness.store);
        let path = format!("/rules/{}/{suffix}", seeded.rule_id);

        let answer = harness.api("POST", &path, Some(body));
        assert_eq!(answer.status, 200, "{path}: {}", answer.text());
        assert_valid("Rule", &answer.json());
    }
}

// ---------------------------------------------------------------------------
// Every failure, including the thirteen that wait on the store
// ---------------------------------------------------------------------------

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
        // 409, a draft that holds back no nits has no ticket to ask for
        (
            "POST",
            format!("/reviews/{}/held-back/ticket", seeded.other_review_id),
            Some("{}"),
        ),
        // 400, a stage that does not follow the rule's current one
        (
            "POST",
            format!("/rules/{}/promote", seeded.rule_id),
            Some(r#"{"to_stage":"proposed"}"#),
        ),
        // 400, a period outside what the contract allows
        (
            "POST",
            format!("/rules/{}/defer", seeded.rule_id),
            Some(r#"{"days":4000}"#),
        ),
        // 400, a repository that is not one
        ("GET", "/rules?repo=notarepo".to_string(), None),
    ];

    for (method, path, body) in refusals {
        let answer = harness.api(method, &path, body);
        assert!(
            answer.status >= 400,
            "{method} {path} with body {body:?} was expected to be refused; \
             it answered {} with {}",
            answer.status,
            answer.text()
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

// -- a second round of the same pull request (DEV-873) -------------------------------

#[test]
fn a_second_round_validates_whole_and_carries_the_timeline() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let store = &harness.store;

    // The seeded review is round 1 of acme/portal#412 with two findings. Post it, let
    // something come back, and open round 2 on a new head commit.
    store
        .approve_draft(
            seeded.review_id,
            review_desk_store::ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .expect("the developer approves round 1");
    store
        .post_draft(seeded.review_id, "gh-review-1")
        .expect("the session reports it posted");
    store
        .add_outcome(
            review_desk_store::FindingRef {
                review_id: seeded.review_id,
                seq: 1,
            },
            review_desk_store::NewOutcome {
                kind: review_desk_store::OutcomeKind::Replied,
                actor: review_desk_store::Actor::Author,
                detail: Some("I would rather do it in ABC-240.".to_string()),
                occurred_at: None,
            },
        )
        .expect("the author replied");

    let second = store
        .open_review("acme/portal", 412, "9d02e71")
        .expect("round 2")
        .review
        .id;

    store
        .put_round_commits(
            second,
            &[review_desk_store::NewRoundCommit {
                sha: "a41c9e2".to_string(),
                summary: "Send an idempotency key with each delivery".to_string(),
                committed_at: Some(1_791_584_040),
                files: vec!["jobs/DeliverWebhook.php".to_string()],
            }],
        )
        .expect("what changed since round 1");

    store
        .carry_finding(
            seeded.review_id,
            second,
            1,
            &review_desk_store::FindingCarry {
                status: review_desk_store::RoundStatus::Fixed,
                by: review_desk_store::DecidedBy::Agent,
                checked: Some("Their new test fails with the fix reverted.".to_string()),
                commit: Some("a41c9e2".to_string()),
            },
        )
        .expect("carry F1 as fixed");

    let answer = harness.api("GET", &format!("/reviews/{second}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_valid("ReviewDetail", &answer.json());

    let detail = answer.json();

    // The timeline.
    let rounds = detail["rounds"].as_array().expect("the rounds");
    assert_eq!(rounds.len(), 2, "oldest first: {detail}");
    assert_eq!(rounds[0]["round"], 1);
    assert_eq!(rounds[0]["event"], "request_changes");
    assert!(
        rounds[0]["posted_at"].is_string(),
        "round 1 was posted, and the time is RFC 3339 at this boundary: {detail}"
    );
    assert_eq!(rounds[0]["replies"], 1, "and one reply came back");
    assert_eq!(rounds[1]["round"], 2);
    assert_eq!(rounds[1]["posted_at"], Value::Null);
    assert_eq!(rounds[1]["commits"], 1);
    assert_eq!(rounds[1]["statuses"]["fixed"], 1);

    // What changed since the previous round.
    let commits = detail["commits_since_previous_round"]
        .as_array()
        .expect("the commits");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["sha"], "a41c9e2");
    assert_eq!(commits[0]["position"], 0);
    assert_eq!(commits[0]["files"][0], "jobs/DeliverWebhook.php");

    // And the finding's place in this round.
    let carried = &detail["findings"][0];
    assert_eq!(carried["seq"], 1, "F1 in round 1 is F1 in round 2");
    assert_eq!(carried["key"], "F1");
    assert_eq!(carried["round_status"], "fixed");
    assert_eq!(carried["round_status_by"], "agent");
    assert_eq!(carried["round_status_commit"], "a41c9e2");
    assert_eq!(carried["continues_review_id"], seeded.review_id);
    assert_eq!(carried["continues_round"], 1, "\"raised in round 1\"");
    assert_eq!(carried["rounds_open"], 0, "it is fixed, so it is not open");
    assert_eq!(
        carried["late"],
        Value::Null,
        "a carried finding is not late; it is open or it is answered"
    );
}

#[test]
fn a_first_round_serves_a_timeline_of_one_and_no_commits() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let detail = harness
        .api("GET", &format!("/reviews/{}", seeded.review_id), None)
        .json();

    assert_eq!(
        detail["rounds"].as_array().unwrap().len(),
        1,
        "a review is itself a round, so the timeline is never empty: {detail}"
    );
    assert_eq!(detail["rounds"][0]["round"], 1);
    assert_eq!(
        detail["commits_since_previous_round"],
        json!([]),
        "a first round has nothing before it to have changed"
    );
    assert_eq!(detail["findings"][0]["round_status"], "new");
    assert_eq!(detail["findings"][0]["continues_review_id"], Value::Null);
    assert_eq!(detail["findings"][0]["late"], Value::Null);
}
