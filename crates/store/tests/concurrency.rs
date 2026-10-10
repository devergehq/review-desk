//! What happens when two callers reach the store at once.
//!
//! These are the cases the first version got wrong: both failed with an error that
//! `is_refusal()` called false, so the binary would have exited 2 — "Review Desk is
//! broken" — for ordinary contention.

mod common;

use std::thread;

use common::new_review;
use review_desk_store::{newest_version, Disposition, NewFinding, Severity, Store};

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
fn two_callers_opening_two_new_head_commits_both_get_a_round() {
    // The race `open_review` exists to close. Composed from `find_review_by_head`,
    // `latest_round` and `create_review` by a caller, both sides would read the same
    // latest round, both would ask for the same next one, and the loser would be refused
    // as a duplicate of a round it never supplied.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("two-heads.db");

    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    first
        .create_review(new_review("devergehq/review-desk", 4, "head-one", 1))
        .unwrap();

    let worker = thread::spawn(move || second.open_review("devergehq/review-desk", 4, "head-two"));
    let mine = first.open_review("devergehq/review-desk", 4, "head-three");
    let theirs = worker.join().unwrap();

    let mine = mine.expect("this caller's round");
    let theirs = theirs.expect("the other caller's round");

    assert!(mine.created && theirs.created);
    assert_ne!(
        mine.review.round, theirs.review.round,
        "two reviews of one pull request cannot share a round"
    );
    assert_eq!(
        first.latest_round("devergehq/review-desk", 4).unwrap(),
        Some(3),
        "round one was seeded, so the two new commits are rounds two and three"
    );
}
