//! What the harvest columns hold: a finding's GitHub comment, a pull request's state,
//! and the key that makes a second harvest write nothing.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, FindingRef, NewFinding, NewOutcome, OutcomeKind, PrState, Severity, Store,
    StoreError,
};

fn a_finding(claim: &str, file: Option<&str>, line: Option<i64>) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Should,
        file: file.map(str::to_string),
        line,
        claim: claim.to_string(),
        evidence: None,
        disposition: Disposition::InDraft,
        disposition_reason: None,
        rule_id: None,
        ..Default::default()
    }
}

/// A store holding one review with one finding.
fn with_a_finding() -> (tempfile::TempDir, Store, FindingRef) {
    let (dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/locus", 64, "9535132", 1))
        .unwrap();
    let finding = store
        .add_finding(review.id, a_finding("the claim", Some("a.rs"), Some(12)))
        .unwrap();
    let reference = FindingRef {
        review_id: review.id,
        seq: finding.seq,
    };
    (dir, store, reference)
}

#[test]
fn a_finding_carries_the_comment_it_was_posted_as() {
    let (_dir, store, finding) = with_a_finding();

    let linked = store
        .set_finding_github_comment(finding.review_id, finding.seq, "4140231301")
        .expect("link the comment");

    assert_eq!(linked.github_comment_id.as_deref(), Some("4140231301"));
    assert_eq!(
        store
            .get_finding(finding.review_id, finding.seq)
            .unwrap()
            .github_comment_id
            .as_deref(),
        Some("4140231301"),
        "and it is still there when read back"
    );
}

#[test]
fn linking_the_same_comment_to_the_same_finding_again_changes_nothing() {
    let (_dir, store, finding) = with_a_finding();

    let first = store
        .set_finding_github_comment(finding.review_id, finding.seq, "4140231301")
        .unwrap();
    let second = store
        .set_finding_github_comment(finding.review_id, finding.seq, "4140231301")
        .expect("re-running the command is not an error");

    assert_eq!(first, second);
}

#[test]
fn a_comment_belongs_to_one_finding() {
    let (_dir, store, finding) = with_a_finding();
    let other = store
        .add_finding(finding.review_id, a_finding("another claim", None, None))
        .unwrap();

    store
        .set_finding_github_comment(finding.review_id, finding.seq, "4140231301")
        .unwrap();

    match store.set_finding_github_comment(finding.review_id, other.seq, "4140231301") {
        Err(StoreError::DuplicateGithubComment {
            comment_id,
            review_id,
            seq,
        }) => {
            assert_eq!(comment_id, "4140231301");
            assert_eq!((review_id, seq), (finding.review_id, finding.seq));
        }
        other => panic!("expected DuplicateGithubComment, got {other:?}"),
    }
}

#[test]
fn linking_an_absent_finding_is_not_found() {
    let (_dir, store, finding) = with_a_finding();

    match store.set_finding_github_comment(finding.review_id, 99, "4140231301") {
        Err(StoreError::NotFound { entity, .. }) => assert_eq!(entity, "finding"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn an_empty_comment_id_is_refused() {
    let (_dir, store, finding) = with_a_finding();

    match store.set_finding_github_comment(finding.review_id, finding.seq, "  ") {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "github_comment_id"),
        other => panic!("expected InvalidValue, got {other:?}"),
    }
}

#[test]
fn a_harvested_outcome_is_written_once_however_often_it_is_offered() {
    let (_dir, store, finding) = with_a_finding();

    let outcome = NewOutcome {
        kind: OutcomeKind::Replied,
        actor: Actor::Author,
        detail: Some(r#"{"login":"someone","body":"Taken."}"#.to_string()),
        occurred_at: Some(1_790_765_682),
    };

    let first = store
        .add_harvested_outcome(finding, outcome.clone(), "reply:4140271863")
        .unwrap();
    assert!(first.is_some(), "the first offer writes the row");

    let second = store
        .add_harvested_outcome(finding, outcome, "reply:4140271863")
        .unwrap();
    assert!(second.is_none(), "the second offer writes nothing");

    assert_eq!(store.list_outcomes_for_finding(finding).unwrap().len(), 1);
}

#[test]
fn the_key_is_what_separates_two_harvested_outcomes_of_one_kind() {
    let (_dir, store, finding) = with_a_finding();

    let outcome = NewOutcome {
        kind: OutcomeKind::Replied,
        actor: Actor::Author,
        detail: None,
        occurred_at: None,
    };

    assert!(store
        .add_harvested_outcome(finding, outcome.clone(), "reply:1")
        .unwrap()
        .is_some());
    assert!(
        store
            .add_harvested_outcome(finding, outcome, "reply:2")
            .unwrap()
            .is_some(),
        "a different reply is a different outcome"
    );

    assert_eq!(store.list_outcomes_for_finding(finding).unwrap().len(), 2);
}

#[test]
fn an_outcome_recorded_by_hand_is_never_caught_by_the_harvest_key() {
    // Two developers may record "kept" against one finding twice; the dedupe index is
    // partial so that it only governs rows the harvester wrote.
    let (_dir, store, finding) = with_a_finding();

    let outcome = NewOutcome {
        kind: OutcomeKind::Kept,
        actor: Actor::Developer,
        detail: None,
        occurred_at: Some(5),
    };

    store.add_outcome(finding, outcome.clone()).unwrap();
    store
        .add_outcome(finding, outcome)
        .expect("a hand-recorded outcome has no key, so nothing refuses the second");

    let outcomes = store.list_outcomes_for_finding(finding).unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|outcome| outcome.harvest_key.is_none()));
}

#[test]
fn a_harvested_outcome_against_an_absent_finding_is_refused() {
    let (_dir, store, finding) = with_a_finding();

    let error = store
        .add_harvested_outcome(
            FindingRef {
                review_id: finding.review_id,
                seq: 99,
            },
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: Actor::Author,
                detail: None,
                occurred_at: None,
            },
            "reply:1",
        )
        .expect_err("no such finding");

    assert!(error.is_refusal());
    assert!(matches!(error, StoreError::UnknownParent { .. }));
}

#[test]
fn a_review_carries_the_pull_requests_state_and_when_it_was_harvested() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/locus", 64, "9535132", 1))
        .unwrap();

    assert_eq!(review.pr_state, None, "nobody has looked yet");
    assert!(!review.is_settled());

    let stated = store
        .set_review_pr_state(review.id, PrState::Merged, Some(1_790_765_682))
        .unwrap();
    assert_eq!(stated.pr_state, Some(PrState::Merged));
    assert_eq!(stated.pr_state_at, Some(1_790_765_682));
    assert!(stated.is_settled());

    let marked = store
        .mark_review_harvested(review.id, 1_790_765_700)
        .unwrap();
    assert_eq!(marked.harvested_at, Some(1_790_765_700));
    assert_eq!(
        marked.pr_state,
        Some(PrState::Merged),
        "marking the harvest does not clear the state"
    );
}

#[test]
fn only_reviews_whose_pull_request_was_open_are_up_for_harvest() {
    let (_dir, store) = temp_store();

    let never_looked = store
        .create_review(new_review("devergehq/locus", 1, "aaa", 1))
        .unwrap();
    let open = store
        .create_review(new_review("devergehq/locus", 2, "bbb", 1))
        .unwrap();
    let merged = store
        .create_review(new_review("devergehq/locus", 3, "ccc", 1))
        .unwrap();
    let closed = store
        .create_review(new_review("devergehq/review-desk", 4, "ddd", 1))
        .unwrap();

    store
        .set_review_pr_state(open.id, PrState::Open, None)
        .unwrap();
    store
        .set_review_pr_state(merged.id, PrState::Merged, Some(1))
        .unwrap();
    store
        .set_review_pr_state(closed.id, PrState::Closed, Some(2))
        .unwrap();

    let up: Vec<i64> = store
        .reviews_to_harvest(None)
        .unwrap()
        .into_iter()
        .map(|review| review.id)
        .collect();
    assert_eq!(
        up,
        vec![never_looked.id, open.id],
        "a review never looked at qualifies; a settled one does not"
    );

    let narrowed: Vec<i64> = store
        .reviews_to_harvest(Some("devergehq/review-desk"))
        .unwrap()
        .into_iter()
        .map(|review| review.id)
        .collect();
    assert!(
        narrowed.is_empty(),
        "that repository's only review is closed"
    );
}

#[test]
fn the_repository_filter_is_normalised_like_the_column_it_matches() {
    // The repository is identity and is stored canonical, so a filter that was not
    // normalised would answer "nothing to harvest" for a caller who typed the name the
    // way GitHub displays it.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("DevergeHQ/Locus", 64, "9535132", 1))
        .unwrap();
    assert_eq!(review.repo, "devergehq/locus", "stored canonical");

    for spelling in ["devergehq/locus", "DevergeHQ/Locus", "  DEVERGEHQ/LOCUS  "] {
        let found: Vec<i64> = store
            .reviews_to_harvest(Some(spelling))
            .unwrap()
            .into_iter()
            .map(|review| review.id)
            .collect();
        assert_eq!(found, vec![review.id], "{spelling:?} names that repository");
    }

    assert!(
        store
            .reviews_to_harvest(Some("devergehq/elsewhere"))
            .unwrap()
            .is_empty(),
        "and a different repository is still a different repository"
    );
}
