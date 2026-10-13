//! A pull request that is neither open nor finished: converted to draft, or its review
//! request withdrawn. DEV-962, item 3 of "What changes".
//!
//! The claim under test is the issue's own sentence — a review out of play "leaves the queue
//! and returns when it's ready or re-requested" — and the thing being defended is that it is
//! the *third* way off the queue rather than a second spelling of one of the other two. A
//! dismissal is the developer declining a review and only they undo it; merged and closed are
//! final. This is neither, and that is why it is a column the caller sets and clears.

mod common;

use common::{db_path, temp_store};
use review_desk_store::{
    Clock, Disposition, LeftTheDesk, NewBrief, NewFinding, OutOfPlay, PrState, ReviewsShown,
    Severity, Store, Timestamp, WorkFilter,
};
use tempfile::TempDir;

/// A Friday afternoon in October 2026, as seconds since the epoch — `tests/timings.rs`'s
/// constant, for the same reason it gives: a real instant keeps a failure message readable.
const START: Timestamp = 1_760_000_000;

/// A store whose clock the test holds, for the two assertions about *when* a pull request
/// went out of play. Seconds is the record's resolution, so two calls in one real second are
/// indistinguishable on the system clock — and "the time did not move" is exactly what one of
/// these tests has to prove.
fn stopped_store() -> (TempDir, Store, Clock) {
    let dir = TempDir::new().expect("temporary directory");
    let clock = Clock::fixed(START);
    let store = Store::open_with_clock(db_path(&dir), clock.clone()).expect("open store");
    (dir, store, clock)
}

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
fn a_round_opened_while_the_pull_request_is_out_of_play_inherits_it() {
    // **The one case that could have made "no round holds one" false.** The poller records
    // a request on sight, so it reaches `review open` on any tick GitHub still shows the
    // request — and a round opened without the state would put a pull request the author has
    // pulled back back in front of a session, which is the thing the column exists to stop.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 65, "4d9464f").expect("round 1");
    common::posted(&store, one.review.id);
    let out = store
        .set_pr_out_of_play(REPO, 65, Some(OutOfPlay::Draft))
        .expect("out of play");
    let since = out[0].out_of_play_at;

    let two = store.open_review(REPO, 65, "a41c9e2").expect("round 2");
    assert_eq!(
        two.review.out_of_play,
        Some(OutOfPlay::Draft),
        "the round inherits the pull request's state"
    );
    assert_eq!(
        two.review.out_of_play_at, since,
        "and since when, because the request has been out of play since the caller said so"
    );
    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "so no session is offered it"
    );

    store
        .set_pr_out_of_play(REPO, 65, None)
        .expect("ready for review again");
    assert!(
        !store
            .get_review(two.review.id)
            .expect("the row")
            .is_out_of_play(),
        "and one call clears every round"
    );
}

#[test]
fn saying_the_same_reason_again_does_not_move_when_it_went_out_of_play() {
    // The caller is a poller. Re-asserting `draft` every tick must not reset "out of play
    // since", or a page would say "a draft since 2 minutes ago" about a pull request pulled
    // back last week. Measured on a clock the test moves, because the two calls would
    // otherwise land in the same second and prove nothing.
    let (_dir, store, clock) = stopped_store();
    requested(&store, 72, "4d9464f");

    let first = store
        .set_pr_out_of_play(REPO, 72, Some(OutOfPlay::Draft))
        .expect("out of play");
    assert_eq!(first[0].out_of_play_at, Some(START));

    clock.advance(15 * 60);
    let again = store
        .set_pr_out_of_play(REPO, 72, Some(OutOfPlay::Draft))
        .expect("the next tick says the same thing");
    assert_eq!(
        again[0].out_of_play_at,
        Some(START),
        "fifteen minutes later, and it has been out of play since the first tick"
    );
    assert_eq!(again[0].out_of_play, Some(OutOfPlay::Draft));
}

#[test]
fn a_different_reason_replaces_the_one_held() {
    // A pull request can go draft and have its review request withdrawn, in either order.
    // The newer assertion is the one that is true, and it is a different fact, so it takes
    // its own time.
    let (_dir, store, clock) = stopped_store();
    requested(&store, 73, "4d9464f");

    store
        .set_pr_out_of_play(REPO, 73, Some(OutOfPlay::Draft))
        .expect("converted to a draft");
    clock.advance(15 * 60);
    let changed = store
        .set_pr_out_of_play(REPO, 73, Some(OutOfPlay::Withdrawn))
        .expect("and then the request was withdrawn");

    assert_eq!(changed[0].out_of_play, Some(OutOfPlay::Withdrawn));
    assert_eq!(
        changed[0].out_of_play_at,
        Some(START + 15 * 60),
        "a different reason is a different fact, so it takes its own time"
    );
}

#[test]
fn putting_a_pull_request_in_play_that_already_is_changes_nothing_and_is_not_an_error() {
    // The caller cannot always know whether it has already said so, and asking first would
    // be a read it should not need.
    let (_dir, store) = temp_store();
    let review = requested(&store, 74, "4d9464f");

    let back = store
        .set_pr_out_of_play(REPO, 74, None)
        .expect("it is already in play");
    assert_eq!(back[0].out_of_play, None);
    assert_eq!(work_ids(&store), vec![review]);
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
