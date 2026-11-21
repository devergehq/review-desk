//! What the developer was shown, counted and compared by the store — DEV-1346.
//!
//! The revision is counted by migration 24's triggers rather than by each writer, so these
//! tests write the draft the ways the record is written — a session's put, a status change,
//! the developer's send-back — and say which of them is a move.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{Actor, DraftShown, DraftStatus, NewDraft, ReviewEvent, Store, StoreError};

fn review_with_a_ready_draft(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("create review");
    store
        .put_draft(review.id, NewDraft::ready("the review body"))
        .expect("put draft");
    review.id
}

#[test]
fn the_revision_moves_when_the_body_does() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let before = store.draft_shown(review).unwrap();

    store
        .put_draft(review, NewDraft::ready("a different body"))
        .unwrap();
    let after = store.draft_shown(review).unwrap();

    assert!(after.draft_rev > before.draft_rev);
    assert_ne!(after.body_sha256, before.body_sha256);
}

#[test]
fn a_change_of_status_alone_is_not_a_move() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let shown = store.draft_shown(review).unwrap();

    // Approving writes the status, the event and the time, and none of them posts.
    store
        .approve_draft(review, ReviewEvent::Comment, Actor::Developer, &shown)
        .unwrap();

    assert_eq!(store.draft_shown(review).unwrap(), shown);
}

#[test]
fn the_body_hash_is_the_sha256_of_the_body() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let body = store.get_draft(review).unwrap().body;

    assert_eq!(
        store.draft_shown(review).unwrap().body_sha256,
        review_desk_store::sha256_hex(body.as_bytes())
    );
    // The known vector, so the hash is SHA-256 and not merely consistent with itself.
    assert_eq!(
        review_desk_store::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn an_approval_records_what_was_approved_and_a_send_back_clears_it() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    assert_eq!(store.get_draft(review).unwrap().approved_shown, None);

    let shown = store.draft_shown(review).unwrap();
    let approved = store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer, &shown)
        .unwrap();
    assert_eq!(approved.approved_shown, Some(shown.clone()));

    let sent_back = store
        .send_back_draft(review, "the blocker is wrong", &shown)
        .unwrap();
    assert_eq!(sent_back.status, DraftStatus::SentBack);
    assert_eq!(
        sent_back.approved_shown, None,
        "the sign-off goes, and so does what it was of"
    );
}

#[test]
fn a_stale_shown_is_refused_and_writes_nothing() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let stale = store.draft_shown(review).unwrap();
    store
        .put_draft(review, NewDraft::ready("rewritten under the developer"))
        .unwrap();
    let before = store.get_draft(review).unwrap();

    let refused = store.approve_draft(review, ReviewEvent::Approve, Actor::Developer, &stale);
    assert!(
        matches!(refused, Err(StoreError::ShownMoved { what: "draft", .. })),
        "{refused:?}"
    );
    assert_eq!(store.get_draft(review).unwrap(), before);

    let refused = store.send_back_draft(review, "a note", &stale);
    assert!(
        matches!(refused, Err(StoreError::ShownMoved { .. })),
        "{refused:?}"
    );
    assert_eq!(store.get_draft(review).unwrap(), before);
}

#[test]
fn the_same_body_written_again_is_still_a_move() {
    // The revision is the compare-and-set: a session that wrote the draft under the
    // developer wrote it, even when the words came out the same.
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let before = store.draft_shown(review).unwrap();

    store
        .put_draft(review, NewDraft::ready("the review body"))
        .unwrap();
    let after = store.draft_shown(review).unwrap();

    assert_eq!(after.body_sha256, before.body_sha256);
    assert!(after.draft_rev > before.draft_rev);
    let refused = store.approve_draft(review, ReviewEvent::Approve, Actor::Developer, &before);
    assert!(
        matches!(refused, Err(StoreError::ShownMoved { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_shown_with_only_the_revision_right_is_refused() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let real = store.draft_shown(review).unwrap();

    for forged in [
        DraftShown {
            body_sha256: "0".repeat(64),
            ..real.clone()
        },
        DraftShown {
            threads_sha256: "0".repeat(64),
            ..real.clone()
        },
    ] {
        let refused = store.approve_draft(review, ReviewEvent::Approve, Actor::Developer, &forged);
        assert!(
            matches!(refused, Err(StoreError::ShownMoved { .. })),
            "{refused:?}"
        );
    }
}
