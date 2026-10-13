//! A pull request that is neither open nor finished: converted to draft, or its review
//! request withdrawn. DEV-962, item 3 of "What changes".
//!
//! The claim under test is the issue's own sentence — a review out of play "leaves the queue
//! and returns when it's ready or re-requested" — and the thing being defended is that it is
//! the *third* way off the queue rather than a second spelling of one of the other two. A
//! dismissal is the developer declining a review and only they undo it; merged and closed are
//! final. This is neither, and that is why it is a column the caller sets and clears.

mod common;

use common::temp_store;
use review_desk_store::{
    Disposition, LeftTheDesk, NewBrief, NewFinding, OutOfPlay, PrState, ReviewsShown, Severity,
    Store, WorkFilter,
};

const REPO: &str = "acme/portal";

/// The reviews the work list offers a session.
fn work_ids(store: &Store) -> Vec<i64> {
    store
        .work_list(&WorkFilter::default())
        .expect("the work list")
        .into_iter()
        .map(|item| item.review.id)
        .collect()
}

/// A review request nobody has started, which is what `work list` offers a session.
fn requested(store: &Store, pr: i64, head: &str) -> i64 {
    store
        .open_review(REPO, pr, head)
        .expect("the request")
        .review
        .id
}

/// A review request with a brief waiting to be confirmed, which puts it in a queue group.
fn waiting(store: &Store, pr: i64, head: &str) -> i64 {
    let review = store
        .open_review(REPO, pr, head)
        .expect("the request")
        .review
        .id;
    store
        .put_brief(
            review,
            NewBrief {
                problem: "Retried deliveries are sent without an idempotency key.".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief waiting on the developer");
    store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "the retry has no ceiling".to_string(),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .expect("a finding");
    review
}

#[test]
fn a_pull_request_converted_to_draft_leaves_the_queue_with_the_reason() {
    let (_dir, store) = temp_store();
    let review = waiting(&store, 61, "4d9464f");
    let elsewhere = requested(&store, 161, "4d9464f");
    assert_eq!(
        work_ids(&store),
        vec![elsewhere],
        "the request nobody has started is the one a session is offered"
    );
    assert!(!store
        .reviews_with_unconfirmed_brief(None)
        .expect("the group")
        .is_empty());

    let rounds = store
        .set_pr_out_of_play(REPO, 61, Some(OutOfPlay::Draft))
        .expect("the author converted it to a draft");

    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].out_of_play, Some(OutOfPlay::Draft));
    assert!(rounds[0].out_of_play_at.is_some());
    assert!(rounds[0].is_out_of_play());

    assert!(
        store
            .reviews_with_unconfirmed_brief(None)
            .expect("the group")
            .is_empty(),
        "and it is in no group of the developer's queue"
    );

    store
        .set_pr_out_of_play(REPO, 161, Some(OutOfPlay::Draft))
        .expect("and the plain request too");
    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "so no session is dispatched for either of them"
    );
    assert!(
        store.find_review_by_id(review).expect("the read").is_some(),
        "and both reviews are still on the record, waiting to come back"
    );
}

#[test]
fn a_withdrawn_review_request_is_out_of_play_with_its_own_reason() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 62, "4d9464f");
    assert_eq!(work_ids(&store), vec![review]);

    store
        .set_pr_out_of_play(REPO, 62, Some(OutOfPlay::Withdrawn))
        .expect("somebody took the review request away");

    assert_eq!(
        store.get_review(review).expect("the row").out_of_play,
        Some(OutOfPlay::Withdrawn),
        "which of the two it is, because the page says which"
    );
    assert_eq!(work_ids(&store), Vec::<i64>::new());
}

#[test]
fn it_returns_to_play_on_the_round_it_was_on() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 63, "4d9464f");

    store
        .set_pr_out_of_play(REPO, 63, Some(OutOfPlay::Draft))
        .expect("out of play");
    let back = store
        .set_pr_out_of_play(REPO, 63, None)
        .expect("ready for review again");

    assert_eq!(back[0].out_of_play, None);
    assert_eq!(back[0].out_of_play_at, None, "and no time left behind");
    assert_eq!(back[0].round, 1, "the same round, not a new one");
    assert_eq!(
        work_ids(&store),
        vec![review],
        "and it is in front of a session again"
    );
}

#[test]
fn every_round_of_the_pull_request_carries_it() {
    // **The difference from `pr_state`.** A state read off one round would leave an earlier
    // round still saying `draft` after the pull request is ready, and the queue would then be
    // answering from whichever round a query reached first.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 64, "4d9464f").expect("round 1");
    common::posted(&store, one.review.id);
    let two = store.open_review(REPO, 64, "a41c9e2").expect("round 2");

    let rounds = store
        .set_pr_out_of_play(REPO, 64, Some(OutOfPlay::Draft))
        .expect("out of play");
    assert_eq!(rounds.len(), 2);
    assert!(rounds.iter().all(|round| round.is_out_of_play()));

    store
        .set_pr_out_of_play(REPO, 64, None)
        .expect("back in play");
    assert!(
        [one.review.id, two.review.id]
            .iter()
            .all(|id| !store.get_review(*id).expect("the row").is_out_of_play()),
        "and no round holds a value afterwards, which is what makes the return unambiguous"
    );
}

#[test]
fn a_round_opened_while_the_pull_request_is_out_of_play_is_in_play_until_it_is_said_again() {
    // Stated rather than silently true: `review open` writes no `out_of_play`, so a round
    // created while the pull request is a draft is in play. The caller watching GitHub says
    // so again, which is the same call it made the first time — and in practice nothing
    // opens a round for a pull request it has just taken out of play.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 65, "4d9464f").expect("round 1");
    common::posted(&store, one.review.id);
    store
        .set_pr_out_of_play(REPO, 65, Some(OutOfPlay::Draft))
        .expect("out of play");

    let two = store.open_review(REPO, 65, "a41c9e2").expect("round 2");
    assert!(!two.review.is_out_of_play());

    let rounds = store
        .set_pr_out_of_play(REPO, 65, Some(OutOfPlay::Draft))
        .expect("said again");
    assert!(rounds.iter().all(|round| round.is_out_of_play()));
}

#[test]
fn the_reviews_list_keeps_it_and_says_where_the_review_went() {
    // **Not a filter position.** A pull request out of play is coming back, so the one row
    // that explains where the review went must not be the row nobody can find.
    let (_dir, store) = temp_store();
    let review = waiting(&store, 66, "4d9464f");
    store
        .set_pr_out_of_play(REPO, 66, Some(OutOfPlay::Draft))
        .expect("out of play");

    let in_play = store
        .reviews_at_step(None, ReviewsShown::InPlay)
        .expect("the Reviews list");
    let row = in_play
        .iter()
        .find(|row| row.review.id == review)
        .expect("drawn under In play");
    assert_eq!(row.left_the_desk, Some(LeftTheDesk::OutOfPlay));
    assert_eq!(row.finished_how, None, "it has not ended; it has paused");
}

#[test]
fn a_dismissal_outranks_it_and_so_does_a_finish() {
    // [`LeftTheDesk`]'s order, which is the one both the filter and the row's own words read:
    // the two answers nothing takes back come first.
    let (_dir, store) = temp_store();

    let dismissed = waiting(&store, 67, "4d9464f");
    store
        .set_pr_out_of_play(REPO, 67, Some(OutOfPlay::Draft))
        .expect("out of play");
    store
        .dismiss_review(dismissed, "not reviewing this one")
        .expect("dismissed as well");

    let merged = waiting(&store, 68, "4d9464f");
    store
        .set_pr_out_of_play(REPO, 68, Some(OutOfPlay::Withdrawn))
        .expect("out of play");
    store
        .set_review_pr_state(merged, PrState::Merged, Some(1_791_000_000))
        .expect("and then it merged");

    let rows = store
        .reviews_at_step(None, ReviewsShown::Everything)
        .expect("the whole record");
    let left = |id: i64| {
        rows.iter()
            .find(|row| row.review.id == id)
            .expect("the row")
            .left_the_desk
    };
    assert_eq!(left(dismissed), Some(LeftTheDesk::Dismissed));
    assert_eq!(left(merged), Some(LeftTheDesk::Finished));
}

#[test]
fn a_pull_request_with_no_review_cannot_be_taken_out_of_play() {
    // A write of nothing reported as success is how a caller comes to believe a state it
    // never set.
    let (_dir, store) = temp_store();

    let error = store
        .set_pr_out_of_play(REPO, 69, Some(OutOfPlay::Draft))
        .expect_err("there is no review of it");

    assert!(error.is_refusal());
    assert_eq!(error.code(), "not_found");
}

#[test]
fn a_sign_off_is_still_offered_while_it_is_out_of_play() {
    // **Deliberately not a refusal.** The pull request is coming back, so a draft the
    // developer wants to approve is a draft that will post when it does; refusing would
    // leave the stuck review DEV-929 removed rather than protect anybody.
    let (_dir, store) = temp_store();
    let review = waiting(&store, 70, "4d9464f");
    store
        .set_pr_out_of_play(REPO, 70, Some(OutOfPlay::Withdrawn))
        .expect("out of play");

    assert_eq!(
        store.not_for_signing(review).expect("the standing"),
        None,
        "nothing about this review may not be signed off"
    );
}

#[test]
fn the_repository_is_one_repository_however_it_is_spelled() {
    let (_dir, store) = temp_store();
    waiting(&store, 71, "4d9464f");

    let rounds = store
        .set_pr_out_of_play(" Acme/Portal ", 71, Some(OutOfPlay::Draft))
        .expect("the same pull request");
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].repo, REPO);
}
