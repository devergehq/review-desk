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

/// **The self-review's probe, kept as a regression test.** A page that read body A, while a
/// session wrote B, and was handed a `shown` read after the write, approved B — text it
/// never drew. `shown_of` hashes what the page was served, so the same interleaving now
/// produces a `shown` no state of the record matches, and the approval is refused.
#[test]
fn a_shown_taken_from_what_was_served_cannot_vouch_for_a_draft_written_between_reads() {
    let (_dir, page) = temp_store();
    let review = page
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap()
        .id;
    page.put_draft(review, NewDraft::ready("body A, the one the page renders"))
        .unwrap();
    let agent = Store::open(page.path()).unwrap();

    let served = page.get_draft(review).unwrap();
    agent
        .put_draft(review, NewDraft::ready("body B, never served"))
        .unwrap();
    let threads = page.draft_threads(review).unwrap();
    let shown = review_desk_store::shown_of(&served, &threads);

    let refused = page.approve_draft(review, ReviewEvent::Approve, Actor::Developer, &shown);
    assert!(
        matches!(refused, Err(StoreError::ShownMoved { .. })),
        "{refused:?}"
    );
    assert_eq!(page.get_draft(review).unwrap().status, DraftStatus::Ready);
}

/// The repeat approval is held to what was shown, as the first is: the second click is still
/// a click on what the page drew.
#[test]
fn approving_again_with_a_stale_shown_is_refused() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let shown = store.draft_shown(review).unwrap();
    store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer, &shown)
        .unwrap();

    let stale = DraftShown {
        draft_rev: shown.draft_rev - 1,
        ..shown.clone()
    };
    let refused = store.approve_draft(review, ReviewEvent::Approve, Actor::Developer, &stale);
    assert!(
        matches!(refused, Err(StoreError::ShownMoved { .. })),
        "{refused:?}"
    );
}

/// **A sign-off given before migration 24 recorded nothing it was of**, and approving it
/// again from the page is what records it.
#[test]
fn approving_again_records_a_sign_off_that_recorded_nothing() {
    let (dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let shown = store.draft_shown(review).unwrap();
    store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer, &shown)
        .unwrap();

    // What a pre-migration-24 approval looks like: approved, and of nothing recorded.
    let raw = rusqlite::Connection::open(common::db_path(&dir)).unwrap();
    raw.execute(
        "UPDATE drafts SET approved_body_sha256 = NULL, approved_threads_sha256 = NULL,
                           approved_rev = NULL WHERE review_id = ?1",
        [review],
    )
    .unwrap();
    assert_eq!(store.get_draft(review).unwrap().approved_shown, None);

    let again = store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer, &shown)
        .unwrap();
    assert_eq!(again.approved_shown, Some(shown));
}

/// One test per trigger, each writing only the column it watches — the hashes would hide a
/// missing trigger otherwise, which the self-review measured: deleting either of the two
/// update triggers failed nothing.
#[test]
fn each_trigger_moves_the_revision_on_its_own() {
    let (dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    let raw = rusqlite::Connection::open(common::db_path(&dir)).unwrap();
    let rev = || store.get_draft(review).unwrap().rev;

    let before = rev();
    raw.execute(
        "UPDATE drafts SET body = 'a body nobody composed' WHERE review_id = ?1",
        [review],
    )
    .unwrap();
    assert_eq!(rev(), before + 1, "drafts_rev_on_body");

    let before = rev();
    raw.execute(
        "UPDATE drafts SET body = body WHERE review_id = ?1",
        [review],
    )
    .unwrap();
    assert_eq!(rev(), before, "the same body is not a change of body");

    let before = rev();
    raw.execute(
        "UPDATE draft_sections SET text = 'a block nobody composed' WHERE review_id = ?1",
        [review],
    )
    .unwrap();
    assert_eq!(rev(), before + 1, "drafts_rev_on_section_update");

    let before = rev();
    raw.execute(
        "UPDATE draft_sections SET agent_text = 'the agent''s words, which post nothing'
         WHERE review_id = ?1",
        [review],
    )
    .unwrap();
    assert_eq!(rev(), before, "the agent's own words post nothing");

    let before = rev();
    raw.execute("DELETE FROM draft_sections WHERE review_id = ?1", [review])
        .unwrap();
    assert_eq!(rev(), before + 1, "drafts_rev_on_section_delete");
}
