//! The one-off tidy for the duplicate unposted rounds already on the record. DEV-962.
//!
//! What is being defended is the issue's own sentence — the tidy "shows each pull request's
//! extra unposted rounds and folds them into the newest (or dismisses them); nothing is
//! deleted without confirmation" — and the two shapes Patrick's record actually held on 13
//! October 2026: a pull request with round 1 unposted and base merges after it, and one that
//! reached round 4 with no post before it merged.

mod common;

use common::{posted, temp_store};
use review_desk_store::{
    Disposition, HeadUpdateKind, NewBrief, NewDraft, NewFinding, Severity, Store, StoreError,
    TidyAct,
};

const REPO: &str = "acme/portal";

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Should,
        claim: claim.to_string(),
        disposition: Disposition::InDraft,
        ..Default::default()
    }
}

/// A round of this pull request, opened the way the old rule left them: each round is
/// *created* rather than moved, which is the state this tidy exists to clean up.
///
/// **Written through `create_review` on purpose.** `open_review` cannot produce a second
/// unposted round any more — that is the whole of DEV-962 — so a test that seeded through it
/// would be testing the new rule rather than the record the tidy was built for.
fn legacy_round(store: &Store, pr: i64, round: i64, head: &str) -> i64 {
    store
        .create_review(common::new_review(REPO, pr, head, round))
        .expect("the round, as the old rule left it")
        .id
}

#[test]
fn a_pull_request_with_one_round_is_not_in_the_answer() {
    let (_dir, store) = temp_store();
    store.open_review(REPO, 41, "4d9464f").expect("round 1");

    assert_eq!(
        store.duplicate_rounds(None).expect("the proposal").len(),
        0,
        "there is nothing to tidy"
    );
}

#[test]
fn four_unposted_rounds_are_three_extras_and_the_newest() {
    // Patrick's second shape: a pull request that reached round 4 with no post at all.
    let (_dir, store) = temp_store();
    let rounds: Vec<i64> = (1..=4)
        .map(|round| legacy_round(&store, 9568, round, &format!("{round}aaaaaa")))
        .collect();

    let found = store.duplicate_rounds(None).expect("the proposal");
    assert_eq!(found.len(), 1, "one pull request");
    let pull_request = &found[0];

    assert_eq!(pull_request.repo, REPO);
    assert_eq!(pull_request.pr, 9568);
    assert_eq!(
        pull_request.newest.review_id, rounds[3],
        "round 4 is the one the others fold into"
    );
    assert_eq!(pull_request.newest.round, 4);
    assert_eq!(
        pull_request
            .extras
            .iter()
            .map(|extra| extra.round)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "oldest first, which is the order they were opened in"
    );
    assert!(
        pull_request
            .extras
            .iter()
            .all(|extra| extra.act == TidyAct::Fold),
        "none of them holds anything, so each would be folded"
    );
    assert_eq!(pull_request.extras[0].holds, "nothing");
}

#[test]
fn a_round_holding_work_would_be_dismissed_rather_than_folded() {
    // The sentence the screen says before the developer agrees to anything.
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 42, 1, "4d9464f");
    legacy_round(&store, 42, 2, "a41c9e2");

    store
        .put_brief(
            one,
            NewBrief {
                problem: "Retried deliveries are sent without an idempotency key.".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");
    store
        .add_finding(one, a_finding("the retry has no ceiling"))
        .expect("a finding");

    let found = store.duplicate_rounds(None).expect("the proposal");
    let extra = &found[0].extras[0];
    assert_eq!(extra.act, TidyAct::Dismiss);
    assert_eq!(
        extra.holds, "a brief and 1 finding",
        "and the screen says so, which is what makes a dismissal the obvious choice"
    );
}

#[test]
fn a_posted_round_is_not_an_extra_and_neither_is_a_dismissed_one() {
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 43, 1, "4d9464f");
    let two = legacy_round(&store, 43, 2, "a41c9e2");
    let three = legacy_round(&store, 43, 3, "c7730bd");
    posted(&store, one);
    store
        .dismiss_review(two, "superseded, waiting for the rewrite")
        .expect("dismissed");

    let found = store.duplicate_rounds(None).expect("the proposal");
    assert_eq!(
        found.len(),
        0,
        "round 1 posted and round 2 is out of play, so round 3 has no duplicates: {found:?}"
    );

    // A fourth round, and now round 3 is an extra: unposted, undismissed, not the newest.
    legacy_round(&store, 43, 4, "eeeeeee");
    let found = store.duplicate_rounds(None).expect("the proposal");
    assert_eq!(
        found[0]
            .extras
            .iter()
            .map(|extra| extra.review_id)
            .collect::<Vec<_>>(),
        vec![three],
        "the posted round and the dismissed one are not offered"
    );
}

#[test]
fn folding_records_the_folded_rounds_head_and_takes_the_row() {
    // What `open_review` would have done had DEV-962's rule existed when the commit
    // arrived, which is why the history survives the row.
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 44, 1, "4d9464f");
    let two = legacy_round(&store, 44, 2, "a41c9e2");

    let folded = store.fold_round(one).expect("it holds nothing");

    assert_eq!(folded.folded.id, one);
    assert_eq!(folded.into.id, two);
    assert!(
        store.find_review_by_id(one).expect("the read").is_none(),
        "the empty row has gone"
    );

    let updates = store.head_updates(two).expect("the updates");
    assert_eq!(updates.folded, 1);
    assert_eq!(updates.moved, 0, "a fold is not a move");
    assert_eq!(updates.since_sha, None);
    assert_eq!(updates.updates[0].kind, HeadUpdateKind::Folded);
    assert_eq!(
        updates.updates[0].from_sha, "4d9464f",
        "the request was at this commit before it was at the one it is at"
    );
    assert_eq!(updates.updates[0].to_sha, "a41c9e2");
}

#[test]
fn folding_a_round_that_holds_work_is_refused_by_what_it_holds() {
    // One deletion path and one refusal: a screen that offered a fold the store refuses is
    // the shape this avoids, and `duplicate_rounds` says `dismiss` for exactly these.
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 45, 1, "4d9464f");
    legacy_round(&store, 45, 2, "a41c9e2");
    store
        .put_draft(one, NewDraft::ready("One finding on the retry path.\n"))
        .expect("a draft");

    let error = store.fold_round(one).expect_err("it holds a draft");
    assert!(error.is_refusal());
    assert_eq!(error.code(), "review_started");
    assert!(matches!(error, StoreError::ReviewStarted { .. }));
    assert!(
        store.find_review_by_id(one).expect("the read").is_some(),
        "and the round is still there"
    );
    assert_eq!(
        store.head_updates(2).expect("the updates").folded,
        0,
        "with nothing recorded against the round it would have folded into"
    );
}

#[test]
fn folding_the_newest_round_is_refused_by_name() {
    let (_dir, store) = temp_store();
    legacy_round(&store, 46, 1, "4d9464f");
    let two = legacy_round(&store, 46, 2, "a41c9e2");

    let error = store.fold_round(two).expect_err("it is the newest round");
    assert!(error.is_refusal());
    assert_eq!(error.code(), "invalid_value");
    assert!(
        error.to_string().contains("the newest round"),
        "the refusal says why: {error}"
    );
}

#[test]
fn folding_a_posted_round_is_refused() {
    // A round that posted is a review that happened. Folding it would delete the record
    // that a review was posted on that pull request at all.
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 47, 1, "4d9464f");
    legacy_round(&store, 47, 2, "a41c9e2");
    // Posted the one way the command line can say so, which is also the only way left for
    // a round a newer one supersedes: `not_for_signing` refuses the sign-off such a round
    // would need to reach `post_draft`, which is DEV-957's rule and not this one's.
    posted(&store, one);

    let error = store.fold_round(one).expect_err("it posted");
    assert!(error.is_refusal());
    assert!(
        error.to_string().contains("never posted"),
        "the refusal says why: {error}"
    );
}

#[test]
fn the_proposal_can_be_asked_for_one_repository() {
    let (_dir, store) = temp_store();
    legacy_round(&store, 48, 1, "4d9464f");
    legacy_round(&store, 48, 2, "a41c9e2");
    store
        .create_review(common::new_review("devergehq/locus", 70, "4d9464f", 1))
        .expect("another repository");
    store
        .create_review(common::new_review("devergehq/locus", 70, "a41c9e2", 2))
        .expect("and its second round");

    assert_eq!(store.duplicate_rounds(None).expect("all").len(), 2);
    assert_eq!(
        store
            .duplicate_rounds(Some(" Acme/Portal "))
            .expect("one repository, however it is spelled")
            .len(),
        1
    );
}

#[test]
fn two_rounds_of_the_same_commit_fold_with_nothing_recorded() {
    // `open_review` cannot produce this — it hands back the review of a head already
    // reviewed — but a record written by hand can hold it, and the table refuses a move from
    // a commit to itself. The honest reading is that there is nothing to record.
    let (_dir, store) = temp_store();
    let one = legacy_round(&store, 49, 1, "4d9464f");
    let two = legacy_round(&store, 49, 2, "4d9464f");

    store.fold_round(one).expect("it holds nothing");

    assert!(store.find_review_by_id(one).expect("the read").is_none());
    assert_eq!(
        store.head_updates(two).expect("the updates").folded,
        0,
        "the request was already where it is"
    );
}
