//! A review GitHub has asked for, recorded before any round of it exists. DEV-996.
//!
//! The claim under test is the ticket's own sentence — "Review Desk only knows about a pull
//! request once a session opens a round on it" — and the thing being defended is that the
//! record can now hold the request itself without that becoming a second way for one pull
//! request to be drawn twice. So every test here is either about the upsert being an upsert,
//! or about the one question the queue asks of a request: is it still outstanding.
//!
//! **This group is a complement**, which DEV-994 learned the hard way: a mistake in a
//! positive predicate hides a row and a mistake here draws a wrong one. Hence the shape of
//! this file — most of it is states in which a recorded request must *not* be offered.

mod common;

use common::{db_path, new_review, posted};
use review_desk_store::{Clock, OutOfPlay, PrState, Store, Timestamp};
use tempfile::TempDir;

/// A Friday afternoon in October 2026 — `tests/out_of_play.rs`'s constant, for the reason it
/// gives: a real instant keeps a failure message readable.
const START: Timestamp = 1_760_000_000;

const REPO: &str = "acme/portal";

/// A store whose clock the test holds.
///
/// **Every test here is about two times being compared**, and the record's resolution is
/// seconds: a round opened in the same real second as the request it answers is
/// indistinguishable from one opened before it on the system clock, and "the round opened
/// after the request" is exactly what these have to prove.
fn stopped_store() -> (TempDir, Store, Clock) {
    let dir = TempDir::new().expect("temporary directory");
    let clock = Clock::fixed(START);
    let store = Store::open_with_clock(db_path(&dir), clock.clone()).expect("open store");
    (dir, store, clock)
}

/// The pull requests `open_review_requests` offers, as `repo#pr`.
fn outstanding(store: &Store) -> Vec<String> {
    store
        .open_review_requests()
        .expect("the open requests")
        .iter()
        .map(|request| format!("{}#{}", request.repo, request.pr))
        .collect()
}

/// A round of a pull request, opened at whatever the clock now reads.
fn round(store: &Store, repo: &str, pr: i64, head: &str, number: i64) -> i64 {
    store
        .create_review(new_review(repo, pr, head, number))
        .expect("create review")
        .id
}

// -- the record ---------------------------------------------------------------------

#[test]
fn a_request_is_recorded_against_a_pull_request_with_no_review_at_all() {
    let (_dir, store, _clock) = stopped_store();

    let request = store
        .record_review_request(REPO, 444, START, false)
        .expect("record the request");

    assert_eq!(request.repo, REPO);
    assert_eq!(request.pr, 444);
    assert_eq!(request.requested_at, START);
    assert!(!request.is_withdrawn());
    // The whole point of the table: nothing else in the record knows this pull request.
    assert!(store.list_reviews(REPO, 444).expect("rounds").is_empty());
    assert_eq!(outstanding(&store), ["acme/portal#444"]);
}

#[test]
fn recording_the_same_request_twice_leaves_one_record() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 444, START, false)
        .expect("the first tick");
    store
        .record_review_request(REPO, 444, START, false)
        .expect("the second tick");

    // The dispatcher calls this on every tick for as long as GitHub lists the request, so
    // one row is the acceptance criterion rather than a nicety.
    assert_eq!(outstanding(&store), ["acme/portal#444"]);
}

#[test]
fn a_later_requested_at_replaces_the_one_recorded_before_it() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 444, START, false)
        .expect("the first request");
    let again = store
        .record_review_request(REPO, 444, START + 3_600, false)
        .expect("GitHub asked again");

    assert_eq!(again.requested_at, START + 3_600);
    // **And `created_at` does not move.** It is the one thing a caller's clock cannot
    // overwrite: when Review Desk first heard, beside when GitHub says it asked.
    assert_eq!(again.created_at, START);
    assert_eq!(again.updated_at, START);
}

#[test]
fn withdrawing_a_request_takes_it_off_the_outstanding_list() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 444, START, false)
        .expect("the request");
    let withdrawn = store
        .record_review_request(REPO, 444, START, true)
        .expect("GitHub stopped listing it");

    assert!(withdrawn.is_withdrawn());
    assert_eq!(withdrawn.withdrawn_at, Some(START));
    assert!(outstanding(&store).is_empty());
}

#[test]
fn recording_a_request_again_after_a_withdrawal_reopens_it() {
    let (_dir, store, clock) = stopped_store();

    store
        .record_review_request(REPO, 444, START, false)
        .expect("the request");
    store
        .record_review_request(REPO, 444, START, true)
        .expect("withdrawn");
    clock.advance(86_400);

    // GitHub dropping a request and asking again is the shape the dispatcher watches for,
    // and the second ask is the one the queue has to draw. The call asserts the state it has
    // just seen rather than accumulating history.
    let again = store
        .record_review_request(REPO, 444, START + 86_400, false)
        .expect("GitHub asked again");

    assert!(!again.is_withdrawn());
    assert_eq!(again.requested_at, START + 86_400);
    assert_eq!(outstanding(&store), ["acme/portal#444"]);
}

#[test]
fn the_repository_is_recorded_in_the_spelling_the_rest_of_the_record_keys_on() {
    let (_dir, store, _clock) = stopped_store();

    let request = store
        .record_review_request("Acme/Portal", 444, START, false)
        .expect("record the request");

    // **The subtraction that keeps one pull request to one row is a string comparison.** A
    // request under `Acme/Portal` beside rounds under `acme/portal` is two pull requests to
    // the record and one to GitHub, and the queue would draw both.
    assert_eq!(request.repo, "acme/portal");
    assert_eq!(outstanding(&store), ["acme/portal#444"]);
    assert!(store
        .review_request("ACME/PORTAL", 444)
        .expect("the lookup normalises too")
        .is_some());
}

#[test]
fn a_repository_that_is_not_a_repository_is_refused() {
    let (_dir, store, _clock) = stopped_store();

    let refused = store
        .record_review_request("acme/portal#x", 444, START, false)
        .expect_err("a repository with a fragment in it is not a repository");

    assert!(refused.to_string().contains("owner/repo"), "{refused}");
}

#[test]
fn a_pull_request_number_below_one_is_refused() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 0, START, false)
        .expect_err("there is no pull request 0");
}

#[test]
fn a_requested_at_at_or_before_the_epoch_is_refused() {
    let (_dir, store, _clock) = stopped_store();

    // Zero is the epoch and a negative is before it. Neither is a moment GitHub asked for a
    // review, and a row holding one would read "requested 56 years ago".
    store
        .record_review_request(REPO, 444, 0, false)
        .expect_err("the epoch is not when GitHub asked");
    store
        .record_review_request(REPO, 444, -1, false)
        .expect_err("nor is before it");
}

// -- satisfied ----------------------------------------------------------------------

#[test]
fn a_round_opened_after_the_request_satisfies_it() {
    let (_dir, store, clock) = stopped_store();

    store
        .record_review_request(REPO, 412, START, false)
        .expect("the request");
    clock.advance(600);
    let opened = round(&store, REPO, 412, "3f9c2ab", 1);

    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        Some(opened)
    );
    // And the request stops being offered: the round is the row now.
    assert!(outstanding(&store).is_empty());
}

#[test]
fn a_round_opened_in_the_same_second_as_the_request_satisfies_it() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 412, START, false)
        .expect("the request");
    let opened = round(&store, REPO, 412, "3f9c2ab", 1);

    // **At or after, not strictly after.** A dispatcher that records the request and starts
    // a session in one tick is the ordinary case, and the record's resolution is seconds.
    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        Some(opened)
    );
    assert!(outstanding(&store).is_empty());
}

#[test]
fn a_round_that_opened_before_the_request_does_not_satisfy_it() {
    let (_dir, store, clock) = stopped_store();

    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    posted(&store, first);
    clock.advance(86_400);
    store
        .record_review_request(REPO, 412, START + 86_400, false)
        .expect("GitHub asked again after the review was posted");

    // The case DEV-996 lists under Related, from 15 October 2026: GitHub requests a review
    // again on a pull request the dispatcher's ledger calls done. Nothing has opened since
    // the ask, so the ask stands.
    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        None
    );
    assert_eq!(outstanding(&store), ["acme/portal#412"]);
}

#[test]
fn the_round_that_satisfied_a_request_is_the_first_one_opened_after_it() {
    let (_dir, store, clock) = stopped_store();

    store
        .record_review_request(REPO, 412, START, false)
        .expect("the request");
    clock.advance(60);
    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    posted(&store, first);
    clock.advance(60);
    round(&store, REPO, 412, "9d1e4c7", 2);

    // The round that *answered* the request, which is not the row the queue draws for the
    // pull request — that is its newest round, which is DEV-983's rule and a different
    // question.
    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        Some(first)
    );
}

#[test]
fn a_round_that_was_dismissed_still_satisfies_the_request_it_answered() {
    let (_dir, store, clock) = stopped_store();

    store
        .record_review_request(REPO, 412, START, false)
        .expect("the request");
    clock.advance(60);
    let opened = round(&store, REPO, 412, "3f9c2ab", 1);
    store
        .dismiss_review(opened, "not worth a review")
        .expect("the developer declined the round");

    // GitHub asked, a session came, and the developer declined what it found. Leaving the
    // request open would draw the pull request under "Requested, not started" the moment its
    // round left the desk — the queue contradicting itself about one pull request, and the
    // developer's own act undone.
    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        Some(opened)
    );
    assert!(outstanding(&store).is_empty());
}

#[test]
fn satisfied_by_is_none_for_a_pull_request_with_no_request_recorded() {
    let (_dir, store, _clock) = stopped_store();

    round(&store, REPO, 412, "3f9c2ab", 1);

    assert_eq!(
        store
            .review_request_satisfied_by(REPO, 412)
            .expect("the comparison"),
        None
    );
}

// -- the pull request leaving the desk ----------------------------------------------

#[test]
fn a_request_on_a_merged_pull_request_is_not_offered() {
    let (_dir, store, clock) = stopped_store();

    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    store
        .set_review_pr_state(first, PrState::Merged, Some(START))
        .expect("the harvester saw it merge");
    clock.advance(86_400);
    store
        .record_review_request(REPO, 412, START + 86_400, false)
        .expect("a request after the merge");

    // The complement fails open: this is the shape that would draw "Requested, not started"
    // over a pull request nobody can review. The dispatcher will withdraw it on its next
    // tick, and the queue must not draw it in the meantime.
    assert!(outstanding(&store).is_empty());
}

#[test]
fn a_request_on_a_pull_request_out_of_play_is_not_offered_and_comes_back_with_it() {
    let (_dir, store, clock) = stopped_store();

    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    posted(&store, first);
    store
        .set_pr_out_of_play(REPO, 412, Some(OutOfPlay::Draft))
        .expect("the author converted it to a draft");
    clock.advance(86_400);
    store
        .record_review_request(REPO, 412, START + 86_400, false)
        .expect("a request while it is a draft");

    assert!(outstanding(&store).is_empty());

    // **And out of play is the one of the three that undoes itself** — DEV-962 — so the
    // request comes back with the pull request rather than needing to be recorded again.
    store
        .set_pr_out_of_play(REPO, 412, None)
        .expect("the author marked it ready");
    assert_eq!(outstanding(&store), ["acme/portal#412"]);
}

#[test]
fn a_request_is_not_offered_once_the_newest_round_is_dismissed() {
    let (_dir, store, clock) = stopped_store();

    // The shape that reaches this, and it is an ordinary one: round 1 is open and
    // unposted, the author pushes, GitHub re-requests the review, and DEV-962 moves round
    // 1's head rather than opening a round — so nothing will ever open to satisfy the
    // request. The developer then dismisses the round.
    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    clock.advance(600);
    store
        .record_review_request(REPO, 412, START + 600, false)
        .expect("GitHub asked again when the author pushed");
    assert_eq!(outstanding(&store), ["acme/portal#412"]);

    clock.advance(600);
    store
        .dismiss_review(first, "the author withdrew the change")
        .expect("the developer declined the round");

    // **A dismissal is the developer saying they will not act on this round**, and a group
    // that put the pull request back in front of them under another heading would undo
    // their own act — DEV-994 settled that, and a request is such a group. The row it would
    // draw has no round to link, no ticket, and no way to be dismissed again.
    assert!(outstanding(&store).is_empty());
}

#[test]
fn a_dismissal_cannot_be_walked_past_by_recording_the_request_again() {
    let (_dir, store, clock) = stopped_store();

    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    store
        .dismiss_review(first, "not worth a review")
        .expect("the developer declined the round");
    clock.advance(86_400);

    // **The exclusion is unconditional rather than compared against the ask**, which is
    // what makes it hold here. `requested_at` is last-write-wins by the command's pinned
    // contract, so a dispatcher re-asserting the request with its own clock would push
    // every ask past the dismissal and draw the row for ever. The way back is the
    // developer's own: restore the review.
    store
        .record_review_request(REPO, 412, START + 86_400, false)
        .expect("GitHub is still asking");

    assert!(outstanding(&store).is_empty());
}

#[test]
fn an_older_dismissed_round_under_a_live_one_says_nothing_about_the_request() {
    let (_dir, store, clock) = stopped_store();

    let first = round(&store, REPO, 412, "3f9c2ab", 1);
    posted(&store, first);
    store
        .dismiss_review(first, "superseded")
        .expect("the developer declined round 1");
    clock.advance(600);
    round(&store, REPO, 412, "9d1e4c7", 2);
    clock.advance(600);
    store
        .record_review_request(REPO, 412, START + 1_200, false)
        .expect("GitHub asked again after round 2 opened");

    // **It is the newest round the clause asks about.** This pull request is drawn by round
    // 2, so the subtraction in the two backends is what removes the request — and the store
    // offering it here is the honest answer about the record rather than a wrong row: the
    // store cannot see what the answer served.
    assert_eq!(outstanding(&store), ["acme/portal#412"]);
    assert!(!store
        .newest_rounds_in_play(None)
        .expect("the pull requests on the desk")
        .is_empty());
}

#[test]
fn a_time_the_server_could_not_spell_is_refused_rather_than_stored() {
    let (_dir, store, _clock) = stopped_store();

    // **The format's own limit**: past midnight on 1 January 10000 the server writes a
    // five-digit year into a field the contract declares `format: date-time`, so the
    // answer would carry a timestamp nothing can parse — and the queue sorts these as
    // text, which puts the furthest-future row at the bottom of a newest-first list.
    store
        .record_review_request(REPO, 412, 253_402_300_800, false)
        .expect_err("the year 10000 is not a date-time");

    // And the same bound is what catches the unit error, which is the mistake this
    // argument actually invites: `Date.now()` in place of `date +%s`.
    store
        .record_review_request(REPO, 412, 1_760_000_000_000, false)
        .expect_err("milliseconds are not seconds");

    // The second before it is accepted, so the bound is a line rather than a mood.
    store
        .record_review_request(REPO, 412, 253_402_300_799, false)
        .expect("the last second the server can spell");
}

// -- ordering and scope --------------------------------------------------------------

#[test]
fn the_outstanding_requests_are_newest_first_across_every_repository() {
    let (_dir, store, _clock) = stopped_store();

    store
        .record_review_request(REPO, 412, START, false)
        .expect("the oldest");
    store
        .record_review_request("globex/portal", 9568, START + 60, false)
        .expect("the newest");

    // Newest first, which is the order every array of `GET /inbox` serves; the page sorts
    // them oldest first, as it does every group.
    assert_eq!(
        outstanding(&store),
        ["globex/portal#9568", "acme/portal#412"]
    );
}
