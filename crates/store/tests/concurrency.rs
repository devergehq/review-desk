//! What happens when two callers reach the store at once.
//!
//! These are the cases the first version got wrong: both failed with an error that
//! `is_refusal()` called false, so the binary would have exited 2 — "Review Desk is
//! broken" — for ordinary contention.

mod common;

use std::thread;

use common::{new_review, posted};
use review_desk_store::{
    newest_version, Actor, Decision, Disposition, FindingDecision, NewDraft, NewDraftSection,
    NewFinding, ReviewLayerState, Severity, Store,
};

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Nit,
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
fn two_connections_adding_findings_both_succeed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("contended.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let review = first
        .create_review(new_review("devergehq/review-desk", 1, "head", 1))
        .unwrap();
    let review_id = review.id;

    let worker = thread::spawn(move || second.add_finding(review_id, a_finding("from the other")));
    let mine = first.add_finding(review_id, a_finding("from here"));
    let theirs = worker.join().unwrap();

    // Before add_finding took its lock up front, one of these failed with "database is
    // locked" — SQLite refuses to upgrade a read lock to a write lock, so the busy
    // timeout never came into play.
    let mine = mine.expect("this caller's finding");
    let theirs = theirs.expect("the other caller's finding");

    assert_ne!(
        mine.seq, theirs.seq,
        "two findings cannot share a sequence number"
    );
    assert_eq!(first.list_findings(review_id).unwrap().len(), 2);
}

#[test]
fn two_processes_opening_a_fresh_database_both_succeed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("fresh.db");

    let (left, right) = (path.clone(), path.clone());
    let first = thread::spawn(move || Store::open(&left).and_then(|s| s.schema_version()));
    let second = thread::spawn(move || Store::open(&right).and_then(|s| s.schema_version()));

    // Before the version was read again inside the migration's transaction, the loser
    // ran a migration that had just been applied and failed on an existing table.
    assert_eq!(first.join().unwrap().unwrap(), newest_version());
    assert_eq!(second.join().unwrap().unwrap(), newest_version());

    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), newest_version());
}

#[test]
fn two_callers_opening_two_new_head_commits_are_neither_refused_a_round() {
    // The race `open_review` exists to close. Composed from `find_review_by_head`,
    // `latest_round` and `create_review` by a caller, both sides would read the same
    // latest round, both would ask for the same next one, and the loser would be refused
    // as a duplicate of a round it never supplied. Neither is refused here, which is what
    // this test has always been about.
    //
    // **What the two calls now end with is one new round, not two**, and that is DEV-945
    // and DEV-962 rather than a regression. Round one is posted, so the first call through
    // the lock opens round two — and round two has not posted, so the second call moves
    // *its* head rather than opening a round three. Two unposted rounds of one pull request
    // is the state those issues exist to prevent, and it is unreachable through this
    // function now: for two rounds to be created at once, both callers would have to see a
    // posted newest round, and the first of them leaves an unposted one behind.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("two-heads.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let seeded = first
        .create_review(new_review("devergehq/review-desk", 4, "head-one", 1))
        .unwrap();
    first
        .add_finding(seeded.id, a_finding("round one was reviewed"))
        .unwrap();
    posted(&first, seeded.id);

    let worker = thread::spawn(move || second.open_review("devergehq/review-desk", 4, "head-two"));
    let mine = first.open_review("devergehq/review-desk", 4, "head-three");
    let theirs = worker.join().unwrap();

    let mine = mine.expect("this caller's round");
    let theirs = theirs.expect("the other caller's round");

    assert_ne!(
        mine.created, theirs.created,
        "one call opened round two and the other moved it"
    );
    assert_eq!(
        mine.review.id, theirs.review.id,
        "both calls are working on the same round"
    );
    assert_eq!(mine.review.round, 2);
    assert_eq!(
        first.latest_round("devergehq/review-desk", 4).unwrap(),
        Some(2),
        "round one was seeded and reviewed, so the two new commits are round two"
    );
    assert_eq!(
        first
            .list_reviews("devergehq/review-desk", 4)
            .unwrap()
            .len(),
        2,
        "one pull request, two rounds, neither duplicated"
    );
}

#[test]
fn two_callers_recording_one_unstarted_request_on_two_commits_get_one_review() {
    // The race the other way round, and the one DEV-945 is about: the dispatcher
    // recording a review request twice while the author pushes between the two calls.
    // Both calls move the same unposted round — the loser finds the round the winner
    // moved and moves it again — so one request is one review whichever order they land
    // in, and `work list` offers a session for it once.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("one-request.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let recorded = first
        .open_review("devergehq/review-desk", 777, "head-one")
        .unwrap();

    let worker =
        thread::spawn(move || second.open_review("devergehq/review-desk", 777, "head-two"));
    let mine = first.open_review("devergehq/review-desk", 777, "head-three");
    let theirs = worker.join().unwrap();

    let mine = mine.expect("this caller's answer");
    let theirs = theirs.expect("the other caller's answer");

    assert!(
        !mine.created && !theirs.created,
        "neither call created a second review"
    );
    assert_eq!(mine.review.id, recorded.review.id);
    assert_eq!(theirs.review.id, recorded.review.id);
    assert!(
        mine.head_moved_from.is_some() && theirs.head_moved_from.is_some(),
        "each call moved the request's head"
    );
    assert_eq!(
        first.latest_round("devergehq/review-desk", 777).unwrap(),
        Some(1),
        "one request, one review, one round"
    );
    assert_eq!(
        first
            .list_reviews("devergehq/review-desk", 777)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn two_connections_deciding_about_two_findings_both_succeed() {
    // **A decision holds an IMMEDIATE transaction across several statements** — the
    // severity, the block, the disposition, the round status, the recomposed body and the
    // outcome — which is the longest write in this crate. Two of them at once must queue
    // on the busy handler and both land, not fail with "database is locked" and exit 2 for
    // ordinary contention, which is the mistake this file was written about.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("contended.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let review = first
        .create_review(new_review("devergehq/review-desk", 6, "head", 1))
        .unwrap();
    let review_id = review.id;

    for claim in ["the first finding", "the second finding"] {
        first.add_finding(review_id, a_finding(claim)).unwrap();
    }
    first
        .put_draft(
            review_id,
            NewDraft::composed(vec![
                NewDraftSection::finding(1, "the first finding", "a paragraph"),
                NewDraftSection::finding(2, "the second finding", "another paragraph"),
            ]),
        )
        .unwrap();

    let cut = |store: Store, seq: i64| {
        thread::spawn(move || {
            store.decide_finding(
                review_id,
                seq,
                &Decision::of(FindingDecision::Cut),
                Actor::Developer,
            )
        })
    };

    let left = cut(first, 1);
    let right = cut(second, 2);
    let one = left.join().expect("the first thread");
    let two = right.join().expect("the second thread");

    assert!(one.is_ok(), "the first decision failed: {one:?}");
    assert!(two.is_ok(), "the second decision failed: {two:?}");

    // And the body is composed from both of them, not from whichever committed last.
    let after = Store::open(&path).unwrap();
    let body = after.get_draft(review_id).unwrap().body;
    assert_eq!(
        body, "",
        "both findings were cut, so nothing is left to post: {body:?}"
    );
    assert_eq!(after.list_outcomes(review_id).unwrap().len(), 2);
}

#[test]
fn two_connections_reporting_the_same_step_record_one_pass() {
    // **`review-desk layer set` opens its own store, one process per invocation**, so the
    // read of the held state and the append of a new one are not inside a transaction
    // anybody else's write has to wait for. Two sessions reporting the same step at the
    // same moment could each read the old state and each append — and the second row
    // would be a pass of no length that every total then counted.
    //
    // The test and the insert are one statement for that reason, and this is the
    // assertion: one state entered, one pass, whoever got there first.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("contended.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let review_id = first
        .create_review(new_review("devergehq/review-desk", 2, "head", 1))
        .unwrap()
        .id;

    let worker = thread::spawn(move || {
        second.set_review_layer(review_id, 5, ReviewLayerState::Running, Some("from there"))
    });
    let mine = first.set_review_layer(review_id, 5, ReviewLayerState::Running, Some("from here"));
    let theirs = worker.join().unwrap();

    assert!(mine.is_ok(), "one caller failed: {mine:?}");
    assert!(theirs.is_ok(), "the other caller failed: {theirs:?}");

    let layer = first.get_review_layer(review_id, 5).expect("the layer");
    assert_eq!(
        layer.runs.len(),
        1,
        "two reports of one state are one pass, not two: {:?}",
        layer.runs
    );
    assert_eq!(layer.state, ReviewLayerState::Running);

    // And a real change still appends, so the guard above has not turned into "append
    // nothing ever".
    first
        .set_review_layer(review_id, 5, ReviewLayerState::Done, Some("3 findings"))
        .expect("the step finishes");
    let layer = first
        .get_review_layer(review_id, 5)
        .expect("the layer again");
    assert_eq!(layer.runs.len(), 1, "still one pass — now a finished one");
    assert_eq!(layer.runs[0].ended_as, Some(ReviewLayerState::Done));
    assert!(layer.seconds().is_some(), "with both ends, so a length");
}
