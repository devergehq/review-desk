//! One round trip per entity: create it through the store, read it back, compare.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, FindingRef, NewBrief, NewFinding, NewOutcome, NewReview, NewRule,
    OutcomeKind, Review, RuleScope, RuleStage, Severity,
};

/// A review to hang the other entities off.
fn seed_review(store: &review_desk_store::Store) -> Review {
    store
        .create_review(new_review("devergehq/review-desk", 12, "deadbeef", 1))
        .expect("seed review")
}

#[test]
fn a_review_is_created_and_read_back() {
    let (_dir, store) = temp_store();

    let created = store
        .create_review(NewReview {
            repo: "devergehq/review-desk".to_string(),
            pr: 12,
            head_sha: "deadbeef".to_string(),
            round: 2,
            session_id: Some("allele-session-1".to_string()),
            ledger_id: Some("ledger-9".to_string()),
            github_review_id: Some("gh-review-44".to_string()),
        })
        .expect("create review");

    let read = store.get_review(created.id).expect("read review");

    assert_eq!(read, created);
    assert_eq!(read.repo, "devergehq/review-desk");
    assert_eq!(read.pr, 12);
    assert_eq!(read.head_sha, "deadbeef");
    assert_eq!(read.round, 2);
    assert_eq!(read.session_id.as_deref(), Some("allele-session-1"));
    assert_eq!(read.ledger_id.as_deref(), Some("ledger-9"));
    assert_eq!(read.github_review_id.as_deref(), Some("gh-review-44"));
    assert!(read.created_at > 0);
}

#[test]
fn a_brief_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let created = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "Reviews repeat themselves and nothing records why".to_string(),
                diagram: Some("graph TD; A-->B".to_string()),
                option_map: Some("1. do nothing 2. a skill 3. an app".to_string()),
                approach_verdict: Some(
                    "An app, because the record has to outlive a session".to_string(),
                ),
            },
        )
        .expect("put brief");

    let read = store.get_brief(review.id).expect("read brief");

    assert_eq!(read, created);
    assert_eq!(read.review_id, review.id);
    assert_eq!(
        read.problem,
        "Reviews repeat themselves and nothing records why"
    );
    assert_eq!(read.diagram.as_deref(), Some("graph TD; A-->B"));
    assert_eq!(
        read.option_map.as_deref(),
        Some("1. do nothing 2. a skill 3. an app")
    );
    assert!(read.approach_verdict.is_some());
    assert!(!read.is_confirmed(), "a new brief is unconfirmed");
    assert_eq!(read.confirmed_at, None);
}

#[test]
fn a_finding_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let created = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                file: Some("crates/store/src/lib.rs".to_string()),
                line: Some(42),
                claim: "The connection does not enforce foreign keys".to_string(),
                evidence: Some("PRAGMA foreign_keys returns 0".to_string()),
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .expect("add finding");

    let read = store
        .get_finding(review.id, created.seq)
        .expect("read finding");

    assert_eq!(read, created);
    assert_eq!(read.seq, 1, "the first finding of a review is number 1");
    assert_eq!(read.lens, "correctness");
    assert_eq!(read.severity, Severity::Blocker);
    assert_eq!(read.file.as_deref(), Some("crates/store/src/lib.rs"));
    assert_eq!(read.line, Some(42));
    assert_eq!(read.disposition, Disposition::InDraft);
    assert_eq!(read.disposition_reason, None);
}

#[test]
fn an_outcome_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);
    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "clarity".to_string(),
                severity: Severity::Nit,
                file: None,
                line: None,
                claim: "The name reads oddly".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .unwrap();

    let created = store
        .add_outcome(
            FindingRef {
                review_id: review.id,
                seq: finding.seq,
            },
            NewOutcome {
                kind: OutcomeKind::Cut,
                actor: Actor::Developer,
                detail: Some("A nit on a file the change does not touch".to_string()),
                occurred_at: Some(1_760_000_000),
            },
        )
        .expect("add outcome");

    let read = store.get_outcome(created.id).expect("read outcome");

    assert_eq!(read, created);
    assert_eq!(read.review_id, review.id);
    assert_eq!(read.finding_seq, finding.seq);
    assert_eq!(read.kind, OutcomeKind::Cut);
    assert_eq!(read.actor, Actor::Developer);
    assert_eq!(read.occurred_at, 1_760_000_000);
    assert_eq!(
        read.detail.as_deref(),
        Some("A nit on a file the change does not touch")
    );
}

#[test]
fn a_rule_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);
    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "process".to_string(),
                severity: Severity::Should,
                file: None,
                line: None,
                claim: "The pull request description carries working notes".to_string(),
                evidence: Some("Three paragraphs of session narration".to_string()),
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .unwrap();

    let created = store
        .create_rule(NewRule {
            statement: "A pull request description states the decision, not the session"
                .to_string(),
            prevents: "A reader cannot find the decision in the narration".to_string(),
            scope: RuleScope::Repo("devergehq/review-desk".to_string()),
            stage: RuleStage::Active,
            origin_review_id: Some(review.id),
            origin_findings: vec![FindingRef {
                review_id: review.id,
                seq: finding.seq,
            }],
        })
        .expect("create rule");

    let read = store.get_rule(created.id).expect("read rule");

    assert_eq!(read, created);
    assert_eq!(
        read.scope,
        RuleScope::Repo("devergehq/review-desk".to_string())
    );
    assert_eq!(read.scope.repo(), Some("devergehq/review-desk"));
    assert_eq!(read.stage, RuleStage::Active);
    assert_eq!(read.origin_review_id, Some(review.id));
    assert_eq!(
        read.origin_findings,
        vec![FindingRef {
            review_id: review.id,
            seq: finding.seq
        }]
    );
    assert!(!read.prevents.is_empty(), "a rule names what it prevents");
}

#[test]
fn a_global_rule_round_trips_without_a_repository() {
    let (_dir, store) = temp_store();

    let created = store
        .create_rule(NewRule {
            statement: "Every finding carries evidence".to_string(),
            prevents: "Preference presented as a defect".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Proposed,
            origin_review_id: None,
            origin_findings: vec![],
        })
        .expect("create global rule");

    let read = store.get_rule(created.id).unwrap();
    assert_eq!(read.scope, RuleScope::Global);
    assert_eq!(read.scope.repo(), None);
    assert!(read.origin_findings.is_empty());
}

#[test]
fn a_brief_with_a_misspelled_field_is_refused_rather_than_losing_it() {
    // `option_map` is optional, so without `deny_unknown_fields` a typo would
    // deserialise to None and the record would quietly lose what the review wrote.
    let error = serde_json::from_str::<NewBrief>(
        r#"{"problem": "the store is write-only", "optionmap": "a, b or c"}"#,
    )
    .expect_err("an unknown field must be refused");

    assert!(
        error.to_string().contains("optionmap"),
        "the message must name the field that was not understood: {error}"
    );
}

#[test]
fn a_finding_with_a_misspelled_field_is_refused_rather_than_losing_it() {
    let error = serde_json::from_str::<NewFinding>(
        r#"{"lens": "correctness", "severity": "blocker", "claim": "x", "evidenc": "y"}"#,
    )
    .expect_err("an unknown field must be refused");

    assert!(error.to_string().contains("evidenc"), "{error}");
}

#[test]
fn an_unknown_severity_names_the_valid_severities() {
    // The command line takes a finding as JSON, so it is serde that reports a bad
    // severity, not the store's own parser. DEV-851 requires the valid values be named.
    let error = serde_json::from_str::<NewFinding>(
        r#"{"lens": "correctness", "severity": "catastrophic", "claim": "x"}"#,
    )
    .expect_err("an unknown severity must be refused");

    let message = error.to_string();
    for valid in ["blocker", "should", "nit", "question"] {
        assert!(message.contains(valid), "{valid} missing from: {message}");
    }
}
