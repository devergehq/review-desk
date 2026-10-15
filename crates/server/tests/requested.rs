//! "Requested, not started": the queue's one group defined by where a pull request is not.
//!
//! DEV-994. Every other group on the queue asks a question of a review — is this brief
//! unconfirmed, is this draft ready, has the author said something, is a layer running, has
//! it posted — and a round that has been opened and not written to answers none of them. So
//! it was in no array at all, and the queue said nothing about a review that had been
//! requested: on 15 October 2026 two of them were on the record and invisible until their
//! sessions started.
//!
//! **These tests are at the server and not at the store, because the group is a property of
//! the answer.** The store says which pull requests are on the desk
//! (`newest_rounds_in_play`, tested in the store's own `inbox.rs`); `api.rs` subtracts the
//! items it has just served. A test of the store could not see the subtraction, and a test
//! of the subtraction against five more reads of the record would be testing a shape this
//! deliberately does not have — the store's own doc carries why.
//!
//! **The ticket's invariant is "drawn in exactly one group", and *drawn* is the page's
//! word.** The five arrays above this group overlap on purpose — the contract's `Inbox`
//! says `in_progress` and `posted_awaiting_author` are "joined onto a row one of those
//! three already made", and a posted review whose author has replied is in two of them —
//! so the thing this file can assert is the half the server owes: every pull request on the
//! desk is in *some* array, and the new array overlaps *none* of them. The other half,
//! that one pull request is one row, is asserted over the rendered page in
//! `web/test/screens.nuxt.spec.ts`. Splitting it that way was not the first guess: the
//! single assertion was written here first and failed on the seeded reply, which is how the
//! overlap came to be stated rather than assumed.

mod common;

use review_desk_store::{
    Actor, Disposition, FindingRef, NewBrief, NewDraft, NewFinding, NewOutcome, OutOfPlay,
    OutcomeKind, PrState, Review, ReviewLayerState, Severity, Store,
};
use serde_json::Value;

const REPO: &str = "acme/portal";

/// A round 1 of a pull request on this repository, with nothing written to it.
fn requested(store: &Store, pr: i64, head: &str) -> Review {
    store
        .open_review(REPO, pr, head)
        .expect("a review requested")
        .review
}

/// Record a round as posted, which is also what lets the next commit open a round of its
/// own — DEV-962.
fn posted(store: &Store, review: i64) {
    store
        .set_review_links(
            review,
            review_desk_store::ReviewLinks {
                github_review_id: Some(format!("gh-review-{review}")),
                ..Default::default()
            },
        )
        .expect("the round was posted");
}

/// A finding on a review, so that an outcome has something to attach to.
fn finding(store: &Store, review: i64) -> i64 {
    store
        .add_finding(
            review,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Should,
                claim: "the retry is unbounded".to_string(),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .expect("a finding")
        .seq
}

/// The author replying to a finding, at a time the caller chooses.
fn author_replied(store: &Store, review: i64, at: i64) {
    let seq = finding(store, review);
    store
        .add_outcome(
            FindingRef {
                review_id: review,
                seq,
            },
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: Actor::Author,
                detail: Some("fixed in the next push".to_string()),
                occurred_at: Some(at),
            },
        )
        .expect("the author replied");
}

/// The pull request numbers one array of the inbox holds.
fn prs_in(inbox: &Value, group: &str) -> Vec<i64> {
    inbox[group]
        .as_array()
        .unwrap_or_else(|| panic!("{group} is an array: {inbox}"))
        .iter()
        .map(|item| {
            item["pull_request"]["pr"]
                .as_i64()
                .unwrap_or_else(|| panic!("{group} item has a pull request number: {item}"))
        })
        .collect()
}

/// The pull request numbers of `awaiting_session`, in the order served.
fn awaiting(inbox: &Value) -> Vec<i64> {
    prs_in(inbox, "awaiting_session")
}

/// A review request GitHub has made for a pull request this repository holds no round of.
///
/// DEV-996's own shape: the dispatcher saw the request and recorded it, and no session has
/// opened anything.
fn asked_for(store: &Store, pr: i64) -> i64 {
    let at = review_desk_store::now();
    store
        .record_review_request(REPO, pr, at, false)
        .expect("a review request");
    at
}

/// A review request GitHub has made **after** every round this test has already opened.
///
/// A minute ahead of the clock the rounds were stamped from, which is what makes it a new
/// ask rather than one of them: satisfaction is a round opened at or after the request, and
/// every round here opened before this instant. GitHub drops a review request when a review
/// is posted and adds it again when the author pushes, so a request newer than the newest
/// round is the ordinary shape rather than a contrived one.
fn asked_again(store: &Store, pr: i64) {
    store
        .record_review_request(REPO, pr, review_desk_store::now() + 60, false)
        .expect("a review request after the rounds");
}

#[test]
fn a_round_with_no_session_and_no_layers_is_requested_not_started() {
    // The ticket's second acceptance criterion, and the simplest shape there is: a round
    // somebody asked for, which nothing has touched. Before DEV-994 this pull request was
    // on the record and on no page.
    let harness = common::start_with_no_github();
    let review = requested(&harness.store, 601, "4f1a9c2");

    let inbox = harness.api("GET", "/inbox", None).json();

    assert_eq!(
        awaiting(&inbox),
        vec![601],
        "a round nothing has started is in the group: {inbox}"
    );
    let item = &inbox["awaiting_session"][0];
    assert_eq!(
        item["review_id"].as_i64(),
        Some(review.id),
        "and the row links that round, which is the newest: {item}"
    );
    assert_eq!(item["round"].as_i64(), Some(1));

    // **And it is the round's own time, not the time of the read.** `is_string()` alone
    // would pass a server serving `now()`, which is the one way this field can be wrong and
    // look right — every row would then read "just now" for ever, which is the opposite of
    // what the group is for. `GET /reviews/{id}` reaches the same column by another path:
    // `ReviewTimings.requested_at` is the review's `created_at`, so the two endpoints have
    // to agree or one of them is inventing a time.
    let detail = harness
        .api("GET", &format!("/reviews/{}", review.id), None)
        .json();
    assert!(
        item["requested_at"].is_string(),
        "with how long it has waited, which is never null: {item}"
    );
    assert_eq!(
        item["requested_at"], detail["timings"]["requested_at"],
        "the inbox and the review agree about when this round was requested: {item}"
    );
}

#[test]
fn the_same_pull_request_leaves_the_group_as_soon_as_a_layer_is_running() {
    // The ticket's third acceptance criterion. **Level-triggered on the record rather than
    // on a dispatch**, which is what makes the group recover on its own: nothing tells
    // Review Desk that a session started, and the first thing a session writes is what
    // takes the row away.
    let harness = common::start_with_no_github();
    let review = requested(&harness.store, 602, "4f1a9c2");
    assert_eq!(
        awaiting(&harness.api("GET", "/inbox", None).json()),
        vec![602]
    );

    harness
        .store
        .set_review_layer(review.id, 4, ReviewLayerState::Running, Some("reading"))
        .expect("a session started");

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        awaiting(&inbox),
        Vec::<i64>::new(),
        "a session is working on it, so it is with the agent: {inbox}"
    );
    assert_eq!(
        prs_in(&inbox, "in_progress"),
        vec![602],
        "and it is drawn there instead, which is the whole of exactly one group: {inbox}"
    );
}

#[test]
fn the_three_round_shape_that_fell_out_of_every_group_is_requested_not_started() {
    // **DEV-990's own documented gap, which is the ticket's first acceptance criterion.**
    // Round 1 posts and the author replies; round 2 opens after the reply and posts, so
    // DEV-990 reads round 2's session as having answered the reply; the author pushes, so
    // round 3 opens with nothing written to it. Round 1's reply is answered, round 2 is not
    // the newest round, and round 3 is in no array of its own — so the pull request was in
    // no group on the queue, and it recovered only if a session started.
    //
    // The reply is stamped in 1970 and the rounds open now, which is the ordering the rule
    // reads: a later round that opened *after* the outcome occurred is a round whose session
    // had it in front of it.
    let harness = common::start_with_no_github();
    let store = &harness.store;

    let one = requested(store, 603, "c3d4e53");
    posted(store, one.id);
    author_replied(store, one.id, 2_000);

    let two = store
        .open_review(REPO, 603, "d4e5f64")
        .expect("round 2")
        .review;
    assert_eq!(two.round, 2);
    posted(store, two.id);

    let three = store
        .open_review(REPO, 603, "e5f6a75")
        .expect("round 3")
        .review;
    assert_eq!(three.round, 3);

    let inbox = harness.api("GET", "/inbox", None).json();

    assert_eq!(
        prs_in(&inbox, "author_replies"),
        Vec::<i64>::new(),
        "round 1's reply is answered by round 2, which is DEV-990: {inbox}"
    );
    assert_eq!(
        prs_in(&inbox, "posted_awaiting_author"),
        Vec::<i64>::new(),
        "and round 2 is not the newest round, so nothing waits on the author: {inbox}"
    );
    assert_eq!(
        awaiting(&inbox),
        vec![603],
        "so the pull request is here, which is the group this shape was left out of: {inbox}"
    );
    assert_eq!(
        inbox["awaiting_session"][0]["review_id"].as_i64(),
        Some(three.id),
        "and the row is round 3, the round the record would open next"
    );
    assert_eq!(inbox["awaiting_session"][0]["round"].as_i64(), Some(3));
}

#[test]
fn a_dismissed_newest_round_is_in_no_group_at_all() {
    // **The one place the invariant below stops, and it stops on purpose.** The ticket's
    // rule is "in play *and not dismissed* means exactly one group": a dismissal is the
    // developer saying they will not act on this round, so a group that put the pull
    // request back in front of them under another heading would undo their own act.
    //
    // **And round 1 is not promoted into its place.** The newest round counts every round
    // whatever became of it, so nothing here resurrects a round that cannot move its own
    // state again.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let one = requested(store, 604, "c3d4e53");
    posted(store, one.id);
    let two = store
        .open_review(REPO, 604, "d4e5f64")
        .expect("round 2")
        .review;
    store
        .dismiss_review(two.id, "the author withdrew the change")
        .expect("dismissed");

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        drawn_anywhere(&inbox, 604),
        0,
        "the developer took this round off the desk: {inbox}"
    );

    // **And a review request recorded afterwards does not bring it back.** DEV-996 added a
    // second way into this group, and without this line the test above would keep passing
    // while the pull request returned under the same heading — with no round to link, no
    // ticket, and no way to be dismissed a second time, because a request row has no review
    // id. The way back is the developer's own: restore the review.
    asked_again(store, 604);
    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        drawn_anywhere(&inbox, 604),
        0,
        "a request cannot undo a dismissal: {inbox}"
    );
}

#[test]
fn an_out_of_play_or_finished_pull_request_is_not_requested() {
    // The ticket's fifth acceptance criterion. A pull request converted to a draft or whose
    // review request was withdrawn is paused and comes back; a merged one is over. Neither
    // is waiting for a session, and a group that drew them would be asking for work on a
    // change nobody has asked to be reviewed.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let merged = requested(store, 605, "9d41c0e");
    requested(store, 606, "eeeeeee");
    let live = requested(store, 607, "4f1a9c2");

    store
        .set_review_pr_state(merged.id, PrState::Merged, None)
        .expect("merged");
    store
        .set_pr_out_of_play(REPO, 606, Some(OutOfPlay::Withdrawn))
        .expect("the review request was withdrawn");

    // **With a review request standing on each of them**, which is what keeps this test
    // about the pull request's state rather than about the absence of a second record.
    // DEV-996: the request is a row of its own, and a merged pull request carrying one is
    // the state the queue cannot correct by itself — Review Desk reads nothing from GitHub,
    // so nothing but this clause stops it drawing for ever.
    asked_again(store, 605);
    asked_again(store, 606);

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        awaiting(&inbox),
        vec![live.pr],
        "only the one still being asked for: {inbox}"
    );

    store
        .set_pr_out_of_play(REPO, 606, None)
        .expect("marked ready again");
    let inbox = harness.api("GET", "/inbox", None).json();
    let mut back = awaiting(&inbox);
    back.sort_unstable();
    assert_eq!(
        back,
        vec![606, 607],
        "and the paused one returns, which the merged one never does: {inbox}"
    );
}

/// Every array of the inbox that carries a pull request.
///
/// **Including the one that is not about a review.** `own_prs_without_reviewer` is a pull
/// request nobody has reviewed and is empty until something supplies it, and it is listed
/// here anyway: the day it is filled, what is asserted below should hold over it rather
/// than quietly stop applying.
const ABOUT_A_PULL_REQUEST: &[&str] = &[
    "briefs_to_confirm",
    "drafts_to_approve",
    "author_replies",
    "own_prs_without_reviewer",
    "in_progress",
    "awaiting_session",
    "posted_awaiting_author",
];

/// How many of those arrays hold this pull request.
///
/// **More than one is not by itself a fault**, which is the thing this file's header is
/// about: `in_progress` and `posted_awaiting_author` are specified to join onto a row one
/// of the three needs-you arrays made, so a posted review whose author has replied is in
/// two. What must never be two is a count that includes `awaiting_session`.
fn drawn_anywhere(inbox: &Value, pr: i64) -> usize {
    ABOUT_A_PULL_REQUEST
        .iter()
        .map(|group| {
            prs_in(inbox, group)
                .iter()
                .filter(|&&one| one == pr)
                .count()
        })
        .sum()
}

/// The arrays other than `awaiting_session` that hold this pull request, by name.
fn also_drawn_in(inbox: &Value, pr: i64) -> Vec<&'static str> {
    ABOUT_A_PULL_REQUEST
        .iter()
        .filter(|group| **group != "awaiting_session")
        .filter(|group| prs_in(inbox, group).contains(&pr))
        .copied()
        .collect()
}

#[test]
fn every_pull_request_on_the_desk_is_in_some_group_and_the_new_one_overlaps_none() {
    // **The server's half of the ticket's fourth acceptance criterion.** One pull request
    // seeded into the shape of each group, and two claims about all of them at once:
    //
    // * every pull request on the desk is in **some** array — never nought, which is the
    //   silence DEV-994 removes;
    // * `awaiting_session` overlaps **nothing** — so the group a client draws from it
    //   cannot be a second row for a pull request already drawn.
    //
    // Together those are what make "one pull request, one row" reachable by a client; the
    // page is where it is asserted, because the five arrays above deliberately overlap and
    // only the page decides which group a row goes in.
    //
    // **Both halves are needed because `awaiting_session` is a *complement*.** The five
    // groups above it are positive predicates, so a mistake in one of them hides a row;
    // a mistake in a complement draws a wrong one. A test of the new group alone would pass
    // over both directions.
    let harness = common::start_with_no_github();
    let store = &harness.store;

    // One per group, in the order the queue draws them.
    let brief = requested(store, 611, "aaaaaa1");
    store
        .put_brief(
            brief.id,
            NewBrief {
                problem: "the importer reads another tenant's rows".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief waiting to be confirmed");

    let draft = requested(store, 612, "aaaaaa2");
    store
        .put_draft(draft.id, NewDraft::ready("The retry is unbounded."))
        .expect("a draft ready for the developer");

    let replied = requested(store, 613, "aaaaaa3");
    posted(store, replied.id);
    author_replied(store, replied.id, 2_000);

    let running = requested(store, 614, "aaaaaa4");
    store
        .set_review_layer(running.id, 5, ReviewLayerState::Running, Some("6 findings"))
        .expect("a session at work");

    let awaits_author = requested(store, 615, "aaaaaa5");
    posted(store, awaits_author.id);

    let unstarted = requested(store, 616, "aaaaaa6");

    // **And the two shapes DEV-996 adds**, so the claim below is about a record holding
    // both kinds of row rather than one where every pull request has a round. #620 is a
    // review GitHub asked for that nothing on the desk answers — the group's second
    // member, and the one that is in no store query of rounds at all. #611 already has a
    // brief waiting on the developer *and* a standing request, which is the state the
    // invariant is most easily broken by: GitHub drops a review request when a review is
    // posted and adds it again when the author pushes, so an outstanding request on a pull
    // request another group holds is ordinary. Both requests are stamped a minute ahead of
    // the rounds, so no round on the record answers them and the subtraction is what has
    // to.
    asked_again(store, 620);
    asked_again(store, 611);

    // And three that are deliberately in no group: the developer dismissed one, one is
    // paused, one is over. They are seeded so that the assertion below is about a record
    // holding both kinds rather than one where every pull request happens to be live.
    let dismissed = requested(store, 617, "aaaaaa7");
    store
        .dismiss_review(dismissed.id, "not worth reviewing")
        .expect("dismissed");
    requested(store, 618, "aaaaaa8");
    store
        .set_pr_out_of_play(REPO, 618, Some(OutOfPlay::Draft))
        .expect("converted to a draft");
    let merged = requested(store, 619, "aaaaaa9");
    store
        .set_review_pr_state(merged.id, PrState::Merged, None)
        .expect("merged");

    let inbox = harness.api("GET", "/inbox", None).json();

    // **The set the claim is about is the store's own answer**, not a list written out
    // here: a list would keep passing after a seed above stopped producing the shape it
    // names, and this is the one assertion in the file that must not be able to go vacuous.
    let on_the_desk: Vec<i64> = store
        .newest_rounds_in_play(None)
        .expect("the pull requests on the desk")
        .iter()
        .map(|review| review.pr)
        .collect();
    let mut sorted = on_the_desk.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        vec![611, 612, 613, 614, 615, 616],
        "the six live pull requests are on the desk, and the other three are not: {inbox}"
    );

    for pr in &on_the_desk {
        assert!(
            drawn_anywhere(&inbox, *pr) >= 1,
            "#{pr} is in play and not dismissed, so some group holds it: {inbox}"
        );
    }
    for pr in [617, 618, 619] {
        assert_eq!(
            drawn_anywhere(&inbox, pr),
            0,
            "#{pr} has left the desk, so it is in no group: {inbox}"
        );
    }
    // **The request-only pull request is drawn, and drawn once.** It is in no query of
    // rounds, so the loop above cannot reach it, and it is the whole of what DEV-996 adds.
    assert_eq!(
        drawn_anywhere(&inbox, 620),
        1,
        "#620 was asked for and nothing has started it: {inbox}"
    );
    // **And a request on a pull request another group holds adds no second row**, which is
    // the half of the invariant the new kind of row could break on its own.
    assert_eq!(
        drawn_anywhere(&inbox, 611),
        1,
        "#611 has a brief waiting and a standing request, and is one thing to deal \
         with: {inbox}"
    );
    for pr in awaiting(&inbox) {
        assert_eq!(
            also_drawn_in(&inbox, pr),
            Vec::<&str>::new(),
            "#{pr} is in the new group, so it must be in no other: {inbox}"
        );
    }

    // **Each group holds the one it was seeded for**, which is what stops the loop above
    // being satisfied by six rows in one array. Without this, a server that put everything
    // in `awaiting_session` would pass both claims and be wrong about every row.
    assert_eq!(prs_in(&inbox, "briefs_to_confirm"), vec![611], "{inbox}");
    assert_eq!(prs_in(&inbox, "drafts_to_approve"), vec![612], "{inbox}");
    assert_eq!(prs_in(&inbox, "author_replies"), vec![613], "{inbox}");
    assert_eq!(prs_in(&inbox, "in_progress"), vec![614], "{inbox}");
    assert_eq!(
        awaiting(&inbox),
        vec![620, unstarted.pr],
        "the group holds both kinds, newest ask first: {inbox}"
    );

    // **And #613 is in two arrays, which is the overlap this file's header is about.** It
    // posted and its author replied, so it is the shape the contract's `Inbox` describes as
    // joined — the author's figures belong on the row that says the author has answered.
    // Asserted rather than tolerated: it is the reason "exactly one" is the page's claim and
    // not this one's, and a server that stopped serving the join would be a change to the
    // contract rather than a tidy-up.
    let mut posted_group = prs_in(&inbox, "posted_awaiting_author");
    posted_group.sort_unstable();
    assert_eq!(
        posted_group,
        vec![613, 615],
        "the posted group holds the replied-to round as well: {inbox}"
    );
    assert_eq!(
        drawn_anywhere(&inbox, 613),
        2,
        "author_replies and posted_awaiting_author, and no third: {inbox}"
    );

    // **And the headline is unchanged by the new group**, which is the ticket's sixth
    // criterion: the three groups that are not waiting on the developer are left out of
    // `totals.items`, and this group is the clearest of them — it says the work has *not*
    // been picked up, and the one thing the developer cannot do about that is pick it up.
    assert_eq!(
        inbox["totals"]["items"].as_u64(),
        Some(3),
        "the brief, the draft and the reply: {inbox}"
    );
}

// -- the second kind of row: a review GitHub asked for, DEV-996 -----------------------

#[test]
fn a_recorded_request_with_no_round_is_requested_not_started() {
    // **The ticket's first acceptance criterion, and the half of the group DEV-994 could
    // not reach.** The Locus dispatcher sees a GitHub review request before any session
    // opens a round and queues it until a slot frees — hours, or forever if it is down —
    // and for that whole time the pull request was on GitHub, in the dispatcher, and in no
    // group here. On 15 October 2026 the queue drew 7 pull requests against 9 requests.
    let harness = common::start_with_no_github();
    let at = asked_for(&harness.store, 701);

    let inbox = harness.api("GET", "/inbox", None).json();

    assert_eq!(
        awaiting(&inbox),
        vec![701],
        "a review nobody has started is in the group: {inbox}"
    );
    let item = &inbox["awaiting_session"][0];
    // **Null, and that is the field a client branches on.** There is no round: the whole of
    // what this row reports is that nobody has opened one.
    assert!(
        item["review_id"].is_null() && item["round"].is_null(),
        "a request row links no round: {item}"
    );
    assert!(
        item["ticket_id"].is_null() && item["ticket_url"].is_null(),
        "and names no ticket, because a ticket reaches the record on a round: {item}"
    );
    assert_eq!(item["pull_request"]["repo"], REPO);
    assert!(
        item["pull_request"]["title"].is_null(),
        "and no title, for the same reason: {item}"
    );

    // **When GitHub asked**, not when the read happened. `is_string()` alone would pass a
    // server serving `now()`, which is the one way this field can be wrong and look right.
    assert_eq!(
        item["requested_at"].as_str(),
        Some(review_desk_server::wire::rfc3339(at).as_str()),
        "the row carries the time the dispatcher reported: {item}"
    );
}

#[test]
fn a_request_answered_by_a_later_round_is_drawn_as_that_round_and_not_twice() {
    // The ticket's fourth acceptance criterion: "A request disappears from the group once a
    // later round opens." The round's row is then the row.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    asked_for(store, 702);
    assert_eq!(
        awaiting(&harness.api("GET", "/inbox", None).json()),
        vec![702]
    );

    let round = requested(store, 702, "4f1a9c2");

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        awaiting(&inbox),
        vec![702],
        "still one row, and still in this group: {inbox}"
    );
    assert_eq!(
        drawn_anywhere(&inbox, 702),
        1,
        "and drawn exactly once across the whole answer: {inbox}"
    );
    assert_eq!(
        inbox["awaiting_session"][0]["review_id"].as_i64(),
        Some(round.id),
        "the row is the round's now, not the request's: {inbox}"
    );
}

#[test]
fn a_request_on_a_pull_request_another_group_holds_is_not_drawn_twice() {
    // The ticket's fifth acceptance criterion, and the one the invariant is most easily
    // broken by: GitHub drops a review request when a review is posted and adds it again
    // when the author pushes, so a request *newer* than the newest round is ordinary — and
    // that request is outstanding on a pull request the answer already draws.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let waiting = requested(store, 703, "4f1a9c2");
    store
        .put_brief(
            waiting.id,
            NewBrief {
                problem: "Retried deliveries are sent without an idempotency key.".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief waiting on the developer");

    asked_again(store, 703);

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        prs_in(&inbox, "briefs_to_confirm"),
        vec![703],
        "the brief is what the developer is asked for: {inbox}"
    );
    assert_eq!(
        awaiting(&inbox),
        Vec::<i64>::new(),
        "and the request is not a second row for the same pull request: {inbox}"
    );
}

#[test]
fn a_withdrawn_request_is_not_drawn() {
    // The ticket's third acceptance criterion. GitHub stopped listing it, and the
    // dispatcher said so.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    asked_for(store, 704);
    assert_eq!(
        awaiting(&harness.api("GET", "/inbox", None).json()),
        vec![704]
    );

    store
        .record_review_request(REPO, 704, review_desk_store::now(), true)
        .expect("withdrawn");

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        drawn_anywhere(&inbox, 704),
        0,
        "nobody is asking for this review any more: {inbox}"
    );
}

#[test]
fn a_request_row_and_a_round_row_are_one_group_ordered_by_when_each_was_asked_for() {
    // **One order over both kinds, by the one field both carry.** The array served
    // `review_id` descending until DEV-996, which is the same order wherever the ids ascend
    // with the clock and is not an order a request can be put in at all.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let round = requested(store, 705, "4f1a9c2");
    // A minute ahead of the round, so the request is the newer of the two and must come
    // first — which is the assertion, rather than "the round happens to sort first".
    store
        .record_review_request(REPO, 706, review_desk_store::now() + 60, false)
        .expect("a newer request");

    let inbox = harness.api("GET", "/inbox", None).json();
    assert_eq!(
        awaiting(&inbox),
        vec![706, 705],
        "newest ask first, whichever kind it is: {inbox}"
    );
    assert_eq!(
        inbox["awaiting_session"][1]["review_id"].as_i64(),
        Some(round.id)
    );
}

#[test]
fn every_row_of_the_group_links_a_round_or_names_none_at_all() {
    // **The pairing the contract states, asserted over a served answer.** `review_id` and
    // `round` are null exactly together: a row with one and not the other is a page
    // drawing "round null" under a link to `/reviews/null`, and neither TypeScript nor the
    // schema can refuse it — both fields are nullable now, so a regression dropping
    // `review_id` from a *round* row serves a document that validates. This is what catches
    // it instead.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    requested(store, 707, "4f1a9c2");
    asked_for(store, 708);

    let inbox = harness.api("GET", "/inbox", None).json();
    let rows = inbox["awaiting_session"].as_array().expect("the group");
    assert_eq!(rows.len(), 2, "both kinds are in it: {inbox}");
    for row in rows {
        assert_eq!(
            row["review_id"].is_null(),
            row["round"].is_null(),
            "a row has a round and a number, or neither: {row}"
        );
        assert!(
            row["requested_at"].is_string(),
            "and every row says how long it has waited: {row}"
        );
    }
}
