//! What the harvest columns hold: a finding's GitHub comment, a pull request's state,
//! the key that makes a second harvest write nothing, and what a harvest leaves on
//! step 7.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, FindingRef, NewFinding, NewOutcome, OutcomeKind, PrState, ReviewLayerState,
    Severity, Store, StoreError,
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

// -- what a harvest leaves on step 7 (DEV-933) ---------------------------------------

/// A posted review of an open pull request, with one finding to hang outcomes on.
///
/// Posted through `github_review_id`, which is what `post_draft` writes and what
/// `record_harvest_step` reads to tell a review that is on GitHub from one still being
/// drafted.
fn a_posted_review() -> (tempfile::TempDir, Store, FindingRef) {
    let (dir, store) = temp_store();
    let review = store
        .create_review(review_desk_store::NewReview {
            github_review_id: Some("gh-review-3312".to_string()),
            ..new_review("devergehq/locus", 64, "9535132", 1)
        })
        .unwrap();
    let finding = store
        .add_finding(review.id, a_finding("the claim", Some("a.rs"), Some(12)))
        .unwrap();
    store
        .set_review_pr_state(review.id, PrState::Open, None)
        .unwrap();

    let reference = FindingRef {
        review_id: review.id,
        seq: finding.seq,
    };
    (dir, store, reference)
}

/// Record one outcome the way a harvest records it, keyed on the GitHub fact.
fn harvested(store: &Store, finding: FindingRef, kind: OutcomeKind, key: &str) {
    store
        .add_harvested_outcome(
            finding,
            NewOutcome {
                kind,
                actor: Actor::Harvester,
                detail: None,
                occurred_at: None,
            },
            key,
        )
        .unwrap();
}

/// Step 7, as the record now holds it.
fn step_seven(store: &Store, review_id: i64) -> review_desk_store::ReviewLayer {
    store.get_review_layer(review_id, 7).unwrap()
}

#[test]
fn a_harvest_that_found_nothing_puts_step_seven_at_running_and_says_when_it_read() {
    let (_dir, store, finding) = a_posted_review();

    let written = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .expect("the review is posted, so it has a harvest step");

    assert_eq!(written.state, ReviewLayerState::Running);
    assert_eq!(
        written.detail.as_deref(),
        Some("nothing back yet"),
        "not \"posted, waiting for the harvest\": a harvest has read this review"
    );
    assert_eq!(
        written
            .facts
            .iter()
            .map(|fact| (fact.label.as_str(), fact.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("Pull request", "open")],
        "where the pull request stands and nothing else — which is also what gives the \
         step a card for the page to put the read time on"
    );

    let read_at = written.updated_at.expect("when the harvest read it");
    let harvested_at = store
        .mark_review_harvested(finding.review_id, read_at)
        .unwrap()
        .harvested_at;
    assert_eq!(
        harvested_at,
        Some(read_at),
        "the step's time and the review's last harvest are the same reading"
    );
}

#[test]
fn every_kind_of_thing_that_came_back_is_a_fact_on_step_seven() {
    let (_dir, store, finding) = a_posted_review();

    harvested(&store, finding, OutcomeKind::Replied, "reply:4140231301");
    harvested(&store, finding, OutcomeKind::Replied, "reply:4140231302");
    harvested(
        &store,
        finding,
        OutcomeKind::ThreadResolved,
        "thread:4140231301",
    );
    // Two commits touched the one finding's file, which is one finding overtaken.
    harvested(
        &store,
        finding,
        OutcomeKind::CodeChangedAfter,
        "code_changed:46f6ea1",
    );
    harvested(
        &store,
        finding,
        OutcomeKind::CodeChangedAfter,
        "code_changed:cf4f69c",
    );

    let written = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .expect("a step");

    assert_eq!(written.state, ReviewLayerState::Running);
    assert_eq!(
        written.detail.as_deref(),
        Some("2 replies back"),
        "the strongest fact on the one line, and the rest on the card"
    );
    let said: Vec<(String, String)> = written
        .facts
        .iter()
        .map(|fact| (fact.label.clone(), fact.value.clone()))
        .collect();
    assert_eq!(
        said,
        vec![
            ("Pull request".to_string(), "open".to_string()),
            ("Replies".to_string(), "2".to_string()),
            ("Threads resolved".to_string(), "1".to_string()),
            ("Code changed since".to_string(), "1 finding".to_string()),
        ],
        "a finding two later commits overtook is one finding, not two"
    );
}

#[test]
fn a_merged_pull_request_finishes_step_seven() {
    let (_dir, store, finding) = a_posted_review();
    harvested(&store, finding, OutcomeKind::Replied, "reply:4140231301");
    store
        .set_review_pr_state(finding.review_id, PrState::Merged, Some(1_790_765_682))
        .unwrap();

    let written = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .expect("a step");

    assert_eq!(written.state, ReviewLayerState::Done);
    assert_eq!(written.detail.as_deref(), Some("merged, 1 reply back"));
    assert_eq!(
        written
            .facts
            .first()
            .map(|fact| (fact.label.clone(), fact.value.clone())),
        Some(("Pull request".to_string(), "merged".to_string())),
        "how it ended, first on the card"
    );
}

#[test]
fn a_closed_pull_request_finishes_step_seven_too() {
    let (_dir, store, finding) = a_posted_review();
    store
        .set_review_pr_state(finding.review_id, PrState::Closed, Some(1_790_765_682))
        .unwrap();

    let written = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .expect("a step");

    assert_eq!(
        written.state,
        ReviewLayerState::Done,
        "a pull request closed unmerged has nothing further to say either"
    );
    assert_eq!(written.detail.as_deref(), Some("closed, nothing back yet"));

    // **And it still records a fact, which is what keeps its card.** A settled step 7
    // has no read time — that is written only while one is `running` — so this pair is
    // the whole of what the trail opens a card for here, and the card is the only way
    // into anything the step has to say: its name, and whatever the record holds about
    // how long it took. Made conditional on a count, this case would draw no card at
    // all and no other test would notice.
    assert_eq!(
        written
            .facts
            .iter()
            .map(|fact| (fact.label.as_str(), fact.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("Pull request", "closed")],
    );
}

#[test]
fn a_review_nothing_has_posted_has_no_harvest_step_to_move() {
    // The sweep covers every review whose pull request is open, including one whose draft
    // is still being written. Step 7 is the harvest of a *posted* review: `running` on
    // this one would put two steps of the trail in progress at once.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/locus", 64, "9535132", 1))
        .unwrap();
    store
        .set_review_pr_state(review.id, PrState::Open, None)
        .unwrap();

    assert!(
        store.record_harvest_step(review.id).unwrap().is_none(),
        "nothing was posted, so nothing is said about step 7"
    );
    assert!(
        store.find_review_layer(review.id, 7).unwrap().is_none(),
        "and no row was written for it"
    );
}

#[test]
fn a_second_harvest_that_found_nothing_new_writes_no_second_state() {
    let (_dir, store, finding) = a_posted_review();
    harvested(&store, finding, OutcomeKind::Replied, "reply:4140231301");

    let first = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .unwrap();
    let again = store
        .record_harvest_step(finding.review_id)
        .unwrap()
        .unwrap();

    assert_eq!(
        again.detail, first.detail,
        "the same words, from the record"
    );
    assert_eq!(
        again.runs.len(),
        1,
        "one pass, still open: a second row for every quarter-hourly harvest would turn \
         one pass into ninety-six of no length: {:?}",
        again.runs
    );
    assert!(
        again.updated_at >= first.updated_at,
        "only the time it read moved"
    );
}

#[test]
fn a_harvest_moves_step_seven_off_what_posting_wrote() {
    // The whole of DEV-933, in the order it happens: the session reports the review
    // posted, the trail says it is waiting for a harvest, and the first harvest answers.
    let (_dir, store, finding) = a_posted_review();
    store
        .set_review_layer(
            finding.review_id,
            7,
            ReviewLayerState::Pending,
            Some("posted, waiting for the harvest"),
        )
        .unwrap();

    store.record_harvest_step(finding.review_id).unwrap();

    let moved = step_seven(&store, finding.review_id);
    assert_eq!(moved.state, ReviewLayerState::Running);
    assert_ne!(
        moved.detail.as_deref(),
        Some("posted, waiting for the harvest"),
        "which is now shown only for a review no harvest has read"
    );
    assert_eq!(
        moved.runs.len(),
        1,
        "the pass starts when the harvest does, not when the review was posted"
    );
}

#[test]
fn a_review_whose_only_running_step_is_the_harvest_is_not_with_the_agent() {
    // **Found by the review of this branch.** The Queue's "With the agent · nothing for
    // you to do" is every review with a `running` layer, and step 7 is `running` for as
    // long as the pull request stays open — so without this a posted review sat in that
    // group for weeks at "Layer 7 of 7", refreshed by every quarter-hourly sweep, and a
    // review whose author had replied was listed twice on one page.
    let (_dir, store, finding) = a_posted_review();
    for number in 1..=6 {
        store
            .set_review_layer(finding.review_id, number, ReviewLayerState::Done, None)
            .unwrap();
    }
    assert!(store.reviews_in_progress(None).unwrap().is_empty());

    store.record_harvest_step(finding.review_id).unwrap();

    assert!(
        store.reviews_in_progress(None).unwrap().is_empty(),
        "a harvest reading GitHub is not a session working on the review"
    );
}

#[test]
fn a_step_a_session_left_running_is_still_reported_under_a_harvested_step_seven() {
    // The other half of the same rule: leaving step 7 out must not lose the review. The
    // layer reported is the highest-numbered running one of the six, which is what
    // "Layer 5 of 7" on the Queue is.
    let (_dir, store, finding) = a_posted_review();
    store
        .set_review_layer(finding.review_id, 5, ReviewLayerState::Running, None)
        .unwrap();
    store.record_harvest_step(finding.review_id).unwrap();

    let in_progress = store.reviews_in_progress(None).unwrap();
    assert_eq!(in_progress.len(), 1);
    assert_eq!(
        in_progress[0].layer.number, 5,
        "the step the session left running, not the harvest's"
    );
}
