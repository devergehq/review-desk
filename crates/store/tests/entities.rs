//! One round trip per entity: create it through the store, read it back, compare.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, BriefOption, Disposition, FindingRef, NewBrief, NewFinding, NewOutcome, NewReview,
    NewRule, OutcomeKind, Review, RuleScope, RuleStage, Severity, StoreError,
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
            title: Some("Record what a review decided".to_string()),
            ticket_id: Some("DEV-850".to_string()),
            changed_lines: Some(214),
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
                options: Vec::new(),
                provenance: Some("from the ticket and the code".to_string()),
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
                rule_id: None,
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
                rule_id: None,
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
                rule_id: None,
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
            why_proposed: None,
            note: None,
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
            why_proposed: None,
            note: None,
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

#[test]
fn a_brief_holds_its_options_as_cards_in_key_order() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a webhook delivery fails once and is never tried again".to_string(),
                options: vec![
                    BriefOption {
                        key: "B".to_string(),
                        title: "Queue with backoff".to_string(),
                        argument_for: "Meets the 24-hour window".to_string(),
                        argument_against: "Needs an idempotency key".to_string(),
                        chosen: true,
                    },
                    BriefOption {
                        key: "A".to_string(),
                        title: "Retry inside the request".to_string(),
                        argument_for: "Smallest change".to_string(),
                        argument_against: "Cannot span 24 hours".to_string(),
                        chosen: false,
                    },
                ],
                provenance: Some("from the ticket and the code".to_string()),
                ..Default::default()
            },
        )
        .expect("put brief");

    let keys = |brief: &review_desk_store::Brief| {
        brief
            .options
            .iter()
            .map(|option| option.key.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&stored), ["A", "B"], "options come back in key order");
    assert_eq!(stored, store.get_brief(review.id).unwrap());
    assert!(stored.options[1].chosen, "B is what the author built");
    assert_eq!(
        stored.provenance.as_deref(),
        Some("from the ticket and the code")
    );

    // Replacing the brief replaces the options with it: a problem statement that
    // changed under the previous round's cards is not a brief anybody wrote.
    let replaced = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a different problem".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(replaced.options.is_empty());
}

#[test]
fn an_option_map_and_structured_options_together_are_refused() {
    // Two spellings of one fact, and nothing could say which the review meant.
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            option_map: Some("A or B".to_string()),
            options: vec![BriefOption {
                key: "A".to_string(),
                title: "t".to_string(),
                argument_for: "f".to_string(),
                argument_against: "a".to_string(),
                chosen: true,
            }],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "option_map"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_option_with_a_blank_field_or_a_repeated_key_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let option = |key: &str, title: &str| BriefOption {
        key: key.to_string(),
        title: title.to_string(),
        argument_for: "for".to_string(),
        argument_against: "against".to_string(),
        chosen: false,
    };

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            options: vec![option("A", "   ")],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "options.title"),
        other => panic!("a card with a blank on it is refused, got {other:?}"),
    }

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            options: vec![option("A", "one"), option("A", "two")],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, value, .. }) => {
            assert_eq!(field, "options");
            assert_eq!(value, "A", "nothing could say which one B is on the screen");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_rule_carries_its_accepted_exceptions() {
    let (_dir, store) = temp_store();
    let rule = store
        .create_rule(NewRule {
            statement: "Money is stored as whole cents".to_string(),
            prevents: "an invoice header and its lines disagreeing by a cent".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .unwrap();

    let exception = store
        .add_rule_exception(rule.id, "  accepted for the reporting tables  ")
        .expect("accept an exception");
    assert_eq!(exception.rule_id, rule.id);
    assert_eq!(exception.reason, "accepted for the reporting tables");

    store
        .add_rule_exception(rule.id, "and for the legacy importer")
        .unwrap();
    let held = store.list_rule_exceptions(rule.id).unwrap();
    assert_eq!(held.len(), 2, "oldest first");
    assert_eq!(held[0], exception);

    assert_eq!(
        store.get_rule(rule.id).unwrap().stage,
        RuleStage::Active,
        "an exception does not retire the rule it excepts"
    );

    match store.add_rule_exception(rule.id, "  ") {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "reason"),
        other => panic!("an exception with no reason is refused, got {other:?}"),
    }
    match store.add_rule_exception(999_999, "for a rule that is not there") {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!((entity, parent), ("rule exception", "rule"));
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}
