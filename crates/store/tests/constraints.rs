//! What the store refuses, and with which named error.
//!
//! A caller has to tell a refused request from a broken Review Desk without reading
//! message text, so every refusal here is asserted by variant.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, FindingRef, NewBrief, NewFinding, NewOutcome, NewReview, NewRule,
    OutcomeKind, PrState, RuleScope, RuleStage, Severity, StoreError,
};

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Should,
        file: None,
        line: None,
        claim: claim.to_string(),
        evidence: None,
        disposition: Disposition::InDraft,
        disposition_reason: None,
        rule_id: None,
        ..Default::default()
    }
}

#[test]
fn a_second_review_of_the_same_repo_pr_and_round_is_refused() {
    let (_dir, store) = temp_store();

    store
        .create_review(new_review("devergehq/review-desk", 3, "aaa", 1))
        .expect("the first review is fine");

    // Same repository, pull request and round. A different head commit, to show the
    // uniqueness is on the round rather than the commit.
    match store.create_review(new_review("devergehq/review-desk", 3, "bbb", 1)) {
        Err(StoreError::DuplicateReview { repo, pr, round }) => {
            assert_eq!(repo, "devergehq/review-desk");
            assert_eq!(pr, 3);
            assert_eq!(round, 1);
        }
        other => panic!("expected DuplicateReview, got {other:?}"),
    }
}

#[test]
fn another_round_of_the_same_pull_request_is_allowed() {
    let (_dir, store) = temp_store();

    store
        .create_review(new_review("devergehq/review-desk", 3, "aaa", 1))
        .unwrap();
    let second = store
        .create_review(new_review("devergehq/review-desk", 3, "bbb", 2))
        .expect("round 2 is a different review");

    assert_eq!(second.round, 2);
    assert_eq!(
        store
            .list_reviews("devergehq/review-desk", 3)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_finding_against_an_absent_review_is_refused() {
    let (_dir, store) = temp_store();
    let absent = 9_999;

    match store.add_finding(absent, a_finding("nothing to attach to")) {
        Err(StoreError::UnknownParent { entity, parent, id }) => {
            assert_eq!(entity, "finding");
            assert_eq!(parent, "review");
            assert_eq!(id, absent.to_string());
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }

    assert!(
        store.list_findings(absent).unwrap().is_empty(),
        "nothing was written"
    );
}

#[test]
fn a_brief_against_an_absent_review_is_refused() {
    let (_dir, store) = temp_store();

    match store.put_brief(
        4_242,
        NewBrief {
            problem: "orphan".to_string(),
            ..Default::default()
        },
    ) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!(entity, "brief");
            assert_eq!(parent, "review");
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn an_outcome_against_an_absent_finding_is_refused() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/review-desk", 5, "ccc", 1))
        .unwrap();

    match store.add_outcome(
        FindingRef {
            review_id: review.id,
            seq: 77,
        },
        NewOutcome {
            kind: OutcomeKind::Kept,
            actor: Actor::Author,
            detail: None,
            occurred_at: None,
        },
    ) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!(entity, "outcome");
            assert_eq!(parent, "finding");
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn a_rule_citing_an_absent_finding_is_refused() {
    let (_dir, store) = temp_store();

    match store.create_rule(NewRule {
        statement: "cites nothing".to_string(),
        prevents: "nothing".to_string(),
        scope: RuleScope::Global,
        stage: RuleStage::Proposed,
        origin_review_id: None,
        origin_findings: vec![FindingRef {
            review_id: 1,
            seq: 1,
        }],
        why_proposed: None,
        note: None,
    }) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!(entity, "rule");
            assert_eq!(parent, "finding");
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }

    assert!(
        store.list_rules().unwrap().is_empty(),
        "the rule was rolled back with its links"
    );
}

#[test]
fn reading_something_absent_is_not_found() {
    let (_dir, store) = temp_store();

    match store.get_review(1) {
        Err(StoreError::NotFound { entity, .. }) => assert_eq!(entity, "review"),
        other => panic!("expected NotFound, got {other:?}"),
    }
    assert!(store.find_review_by_id(1).unwrap().is_none());
}

#[test]
fn every_refusal_is_a_refusal_and_not_a_breakage() {
    let (_dir, store) = temp_store();
    store
        .create_review(new_review("devergehq/review-desk", 3, "aaa", 1))
        .unwrap();

    let duplicate = store
        .create_review(new_review("devergehq/review-desk", 3, "aaa", 1))
        .unwrap_err();
    let orphan = store.add_finding(123, a_finding("orphan")).unwrap_err();
    let missing = store.get_review(456).unwrap_err();
    let bad_value = "whenever".parse::<Severity>().unwrap_err();

    for error in [duplicate, orphan, missing, bad_value] {
        assert!(
            error.is_refusal(),
            "{error:?} should be a refusal, not a breakage"
        );
    }
}

#[test]
fn an_unknown_value_names_what_was_expected() {
    match "catastrophic".parse::<Severity>() {
        Err(StoreError::InvalidValue {
            field,
            value,
            expected,
        }) => {
            assert_eq!(field, "severity");
            assert_eq!(value, "catastrophic");
            assert_eq!(expected, "blocker, should, nit, question");
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }

    // Every closed set can say what it accepts; a command line has to print this.
    assert_eq!(
        Disposition::valid_values(),
        "in_draft, held_back, suppressed"
    );
    assert_eq!(Actor::valid_values(), "developer, author, harvester");
    assert_eq!(
        OutcomeKind::valid_values(),
        "cut, regraded, rewritten, kept, rejected, ticketed, withdrawn, raised_again, \
         replied, thread_resolved, code_changed_after",
        "the developer's eight decisions, then the three the harvester reads back"
    );
    assert_eq!(
        RuleStage::valid_values(),
        "proposed, active, lint, ci, retired"
    );
    assert_eq!(PrState::valid_values(), "open, merged, closed");
}

#[test]
fn a_rule_scoped_to_a_repository_must_name_one() {
    // The scope and its repository are one value in Rust, so the invalid combination
    // cannot be constructed; the database refuses it too.
    assert_eq!(RuleScope::Global.repo(), None);
    assert!(RuleScope::from_columns("repo", None).is_err());
    assert_eq!(
        RuleScope::from_columns("repo", Some("owner/repo".to_string())).unwrap(),
        RuleScope::Repo("owner/repo".to_string())
    );
    assert_eq!(
        RuleScope::from_columns("global", None).unwrap(),
        RuleScope::Global
    );
    assert!(RuleScope::from_columns("everywhere", None).is_err());
}

#[test]
fn a_rule_citing_the_same_finding_twice_cites_it_once() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/review-desk", 3, "aaa", 1))
        .unwrap();
    let finding = store
        .add_finding(review.id, a_finding("said twice"))
        .unwrap();
    let cited = FindingRef {
        review_id: review.id,
        seq: finding.seq,
    };

    // A set, not a list. Citing one finding twice is a redundant input, not a
    // breakage — and before this was deduplicated it broke the link table's primary
    // key and reached the caller as exit 2 rather than a no-op.
    let rule = store
        .create_rule(NewRule {
            statement: "evidence or it is a preference".to_string(),
            prevents: "preference presented as a defect".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Active,
            origin_review_id: Some(review.id),
            origin_findings: vec![cited, cited, cited],
            why_proposed: None,
            note: None,
        })
        .expect("a repeated citation is not a failure");

    assert_eq!(rule.origin_findings, vec![cited]);
}

#[test]
fn a_review_must_name_a_repository_a_pull_request_and_a_round() {
    let (_dir, store) = temp_store();

    let cases: [(NewReview, &str); 6] = [
        (new_review("", 1, "head", 1), "repo"),
        (new_review("no-slash", 1, "head", 1), "repo"),
        (new_review("owner/", 1, "head", 1), "repo"),
        (new_review("devergehq/review-desk", 0, "head", 1), "pr"),
        (new_review("devergehq/review-desk", 1, "head", 0), "round"),
        (new_review("devergehq/review-desk", 1, "", 1), "head_sha"),
    ];

    for (new, expected_field) in cases {
        match store.create_review(new.clone()) {
            Err(StoreError::InvalidValue {
                field, expected, ..
            }) => {
                assert_eq!(field, expected_field, "for {new:?}");
                assert!(!expected.is_empty(), "the error says what it wanted");
            }
            other => panic!("expected InvalidValue on {expected_field}, got {other:?}"),
        }
    }

    // A review whose repository is empty would silently partition the record, because
    // rules are scoped by the same string.
    assert!(store.list_reviews("", 1).unwrap().is_empty());
}

#[test]
fn a_rule_scoped_to_a_repository_cannot_name_an_empty_one() {
    let (_dir, store) = temp_store();

    match store.create_rule(NewRule {
        statement: "scoped to nothing".to_string(),
        prevents: "nothing".to_string(),
        scope: RuleScope::Repo("   ".to_string()),
        stage: RuleStage::Active,
        origin_review_id: None,
        origin_findings: vec![],
        why_proposed: None,
        note: None,
    }) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "scope_repo"),
        other => panic!("expected InvalidValue on scope_repo, got {other:?}"),
    }
}

#[test]
fn a_repo_scope_with_no_repository_blames_the_right_column() {
    // "repo" is a valid scope, so the error must not claim otherwise.
    match RuleScope::from_columns("repo", None) {
        Err(StoreError::InvalidValue {
            field, expected, ..
        }) => {
            assert_eq!(field, "scope_repo");
            assert!(
                expected.contains("owner/repo"),
                "it should say what it wanted, got {expected:?}"
            );
        }
        other => panic!("expected InvalidValue on scope_repo, got {other:?}"),
    }
}
