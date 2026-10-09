//! The layer trail after a sign-off.
//!
//! **DEV-862 shipped a screen that read "2 Understand · waiting on you" under a brief the
//! developer had confirmed minutes earlier**, and recorded it as a known risk rather than
//! a bug, because what writes a layer row was out of its scope. The screen was faithful;
//! the record disagreed with itself. These tests are the record agreeing.
//!
//! The last test is the one that matters most, and it is the reason the layer write lives
//! inside the sign-off's transaction rather than after it: two writes that can disagree
//! are two writes that eventually will.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, DraftStatus, NewBrief, NewDraft, Review, ReviewEvent, ReviewLayerState, Store,
};

/// A review with a brief, both written the ordinary way.
fn with_brief(store: &Store) -> Review {
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("a review");
    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a webhook delivery that fails once is never tried again".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");
    review
}

/// A review with a brief and a draft waiting on the developer.
fn with_ready_draft(store: &Store) -> Review {
    let review = with_brief(store);
    store
        .put_draft(review.id, NewDraft::ready("the draft body"))
        .expect("a draft");
    review
}

/// One layer's state and detail.
fn layer(store: &Store, review_id: i64, number: i64) -> (ReviewLayerState, Option<String>) {
    let layer = store
        .get_review_layer(review_id, number)
        .expect("the layer row");
    (layer.state, layer.detail)
}

#[test]
fn confirming_a_brief_finishes_layer_two() {
    let (_dir, store) = temp_store();
    let review = with_brief(&store);

    // Nothing has written layer 2, so the trail reads `pending` — the honest answer
    // before the developer has been asked.
    assert_eq!(
        store.find_review_layer(review.id, 2).expect("a read"),
        None,
        "no layer row until something writes one"
    );

    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer confirms it");

    let (state, detail) = layer(&store, review.id, 2);
    assert_eq!(state, ReviewLayerState::Done);
    assert_eq!(
        detail.as_deref(),
        Some("confirmed by the developer"),
        "the who; the when is the row's own time"
    );

    let written = store.get_review_layer(review.id, 2).expect("the row");
    let confirmed_at = store.get_brief(review.id).unwrap().confirmed_at;
    assert_eq!(
        written.updated_at, confirmed_at,
        "one transaction, so the row and the confirmation carry the same instant"
    );
}

#[test]
fn approving_a_draft_finishes_layer_six() {
    let (_dir, store) = temp_store();
    let review = with_ready_draft(&store);

    store
        .approve_draft(review.id, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer approves it");

    let (state, detail) = layer(&store, review.id, 6);
    assert_eq!(state, ReviewLayerState::Done);
    assert_eq!(detail.as_deref(), Some("approved by the developer"));
}

#[test]
fn sending_a_draft_back_puts_layer_six_in_progress_with_the_note() {
    let (_dir, store) = temp_store();
    let review = with_ready_draft(&store);

    let note = "F2 is the same point as F1; cut one of them";
    store
        .send_back_draft(review.id, note)
        .expect("the developer sends it back");

    let (state, detail) = layer(&store, review.id, 6);
    assert_eq!(
        state,
        ReviewLayerState::Running,
        "the developer has acted, so it is the session's turn — not `waiting_on_you`"
    );
    assert_eq!(
        detail.as_deref(),
        Some(note),
        "the note verbatim: what the developer asked for is the only thing that makes \
         `layer 6, in progress` mean anything to whoever reads the trail next"
    );
}

#[test]
fn marking_a_draft_posted_leaves_layer_seven_pending() {
    let (_dir, store) = temp_store();
    let review = with_ready_draft(&store);
    store
        .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("approved");

    store
        .post_draft(review.id, "gh-review-3312")
        .expect("the session reports it posted");

    let (state, detail) = layer(&store, review.id, 7);
    assert_eq!(
        state,
        ReviewLayerState::Pending,
        "posting makes a harvest possible; it is not the harvest, and `done` here would \
         claim the outcomes had been read back from GitHub"
    );
    assert_eq!(detail.as_deref(), Some("posted, waiting for the harvest"));

    // Layer 6 is still finished with: posting does not unsettle the approval.
    assert_eq!(layer(&store, review.id, 6).0, ReviewLayerState::Done);
}

#[test]
fn a_brief_needing_no_confirmation_finishes_layer_two_on_write() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 7, "c0ffee1", 1))
        .expect("a review");

    let brief = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a log line says `null` where it means `not measured`".to_string(),
                confirmation_required: false,
                confirmation_not_required_because: Some(
                    "38 changed lines, under this caller's 200-line threshold".to_string(),
                ),
                ..Default::default()
            },
        )
        .expect("a brief needing no confirmation");

    assert!(!brief.confirmation_required);
    assert!(!brief.is_confirmed(), "nobody confirmed it");
    assert!(brief.is_settled(), "and nothing is waiting at layer 2");

    let (state, detail) = layer(&store, review.id, 2);
    assert_eq!(state, ReviewLayerState::Done);
    assert_eq!(
        detail.as_deref(),
        Some("confirmation not required: 38 changed lines, under this caller's 200-line threshold"),
        "the trail carries why, because the threshold behind it is the caller's and \
         Review Desk does not hold it"
    );

    // And it is not in the developer's queue: the group exists to ask for the act this
    // field says was unnecessary.
    let queued = store
        .reviews_with_unconfirmed_brief(None)
        .expect("the inbox group");
    assert!(
        queued.is_empty(),
        "a brief needing no confirmation is in nobody's queue: {queued:?}"
    );
}

#[test]
fn requiring_confirmation_again_puts_layer_two_back() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 8, "d00dfee", 1))
        .expect("a review");

    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "the first reading of it".to_string(),
                confirmation_required: false,
                confirmation_not_required_because: Some("a one-line change".to_string()),
                ..Default::default()
            },
        )
        .expect("a brief needing no confirmation");
    assert_eq!(layer(&store, review.id, 2).0, ReviewLayerState::Done);

    // Asking for a confirmation that was not needed is allowed: it costs the developer a
    // click and the record nothing. What it must not do is leave layer 2 saying `done`.
    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a second reading, which turned out to be bigger".to_string(),
                ..Default::default()
            },
        )
        .expect("rewritten as requiring confirmation");

    let (state, detail) = layer(&store, review.id, 2);
    assert_eq!(state, ReviewLayerState::WaitingOnYou);
    assert_eq!(detail.as_deref(), Some("waiting on you"));

    let queued = store
        .reviews_with_unconfirmed_brief(None)
        .expect("the inbox group");
    assert_eq!(
        queued.iter().map(|review| review.id).collect::<Vec<_>>(),
        vec![review.id],
        "and it is back in the developer's queue"
    );
}

#[test]
fn a_sign_off_whose_layer_row_fails_writes_neither() {
    // **The reason the layer write is inside the transaction.** A sign-off that recorded
    // the developer's assent and then failed to move the trail would leave the record
    // disagreeing with itself in the one direction that matters: a confirmed brief whose
    // trail still says the developer has not been asked.
    //
    // The failure is forced with a trigger rather than simulated, because a test that
    // never makes the second write fail proves only that two writes in a row both work.
    let (dir, store) = temp_store();
    let review = with_brief(&store);

    {
        let conn = rusqlite::Connection::open(common::db_path(&dir)).expect("a second connection");
        conn.execute_batch(
            "CREATE TRIGGER no_layer_rows
                 BEFORE INSERT ON review_layers
             BEGIN
                 SELECT RAISE(ABORT, 'this layer row is refused on purpose');
             END;",
        )
        .expect("the trigger");
    }

    let refused = store.confirm_brief(review.id, Actor::Developer);
    assert!(
        refused.is_err(),
        "the layer row could not be written, so the call fails: {refused:?}"
    );

    // **Neither write survived.** The confirmation was issued first and the layer row
    // second, so this is the assertion that fails if they are ever two transactions.
    let brief = store
        .get_brief(review.id)
        .expect("the brief is still there");
    assert_eq!(
        brief.confirmed_at, None,
        "the confirmation was rolled back with the layer row it could not write"
    );
    assert_eq!(brief.confirmed_by, None);
    assert_eq!(
        store.find_review_layer(review.id, 2).expect("a read"),
        None,
        "and no layer row was left behind either"
    );

    // The same for the draft's three transitions, which share the arrangement.
    store
        .put_draft(review.id, NewDraft::ready("the body"))
        .expect("a draft");
    let refused = store.approve_draft(review.id, ReviewEvent::Comment, Actor::Developer);
    assert!(
        refused.is_err(),
        "approving fails the same way: {refused:?}"
    );

    let draft = store
        .get_draft(review.id)
        .expect("the draft is still there");
    assert_eq!(
        draft.status,
        DraftStatus::Ready,
        "an approval nobody could write a trail for is not an approval"
    );
    assert_eq!(draft.approved_by, None);
}
