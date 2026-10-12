//! A review whose pull request merged or closed, and a round a newer round has superseded.
//!
//! **Two ways a review stops being anybody's turn, and neither is a dismissal.** DEV-957:
//! on 13 October 2026 Patrick's queue held ten drafts to approve, five of them rounds of
//! pull requests that had already merged, and two rows for one pull request three times
//! over. None of those drafts could ever be posted. He asked whether a merged review
//! should be dismissed or gone, and ruled neither — finished.
//!
//! So the claims here are about where a review *appears*, which is the thing that was
//! wrong:
//!
//! | Claim | Why it is not obvious |
//! | -- | -- |
//! | A merged pull request's review is in no queue group and no work item | The state is written per round, and only the round a harvest read gets it |
//! | Every round of it is finished together, posted or not | Patrick's #9568 has four rounds and one harvest between them |
//! | A reopened pull request comes back | Nothing can see a reopen it never reads |
//! | A dismissed review that then merges stays dismissed | Two facts about one review, and one of them is somebody's act |
//! | An unposted round under a newer round leaves the queue | It can never be the review that posts |
//! | A posted round is not superseded by a newer one | It did post; a later round does not unmake that |

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, DraftStatus, LeftTheDesk, NewBrief, NewDraft, NewFinding, NewOutcome,
    NotForSigning, OutcomeKind, PrState, Review, ReviewEvent, ReviewLayerName, ReviewLayerState,
    ReviewLinks, ReviewsShown, Severity, Store, WorkFilter,
};

const REPO: &str = "tc/portal";

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Blocker,
        claim: claim.to_string(),
        disposition: Disposition::InDraft,
        ..Default::default()
    }
}

fn a_brief(problem: &str) -> NewBrief {
    NewBrief {
        problem: problem.to_string(),
        confirmation_required: true,
        ..Default::default()
    }
}

/// A round with a brief waiting on the developer and a draft ready for them.
///
/// The two groups the queue drew Patrick's merged rounds under, so a round in this shape
/// is in "briefs to confirm" and "drafts to approve" at once — which is the state the
/// claims below are about.
fn waiting_round(store: &Store, pr: i64, head: &str, round: i64) -> Review {
    let review = store
        .create_review(new_review(REPO, pr, head, round))
        .expect("a round");
    store
        .put_brief(review.id, a_brief("The export runs inside the request"))
        .expect("a brief");
    store
        .add_finding(review.id, a_finding("a retry has no idempotency key"))
        .expect("a finding");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: "One thing blocks the merge.".to_string(),
                status: DraftStatus::Ready,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");
    review
}

/// Say the harvest read this pull request and found it in this state.
///
/// Through `set_review_pr_state`, which is what `harvest_one` calls on the round it read.
/// One round at a time, deliberately: the whole question is what the record says about the
/// *pull request* when only one of its rounds has been read.
fn harvest_found(store: &Store, review_id: i64, state: PrState, at: Option<i64>) {
    store
        .set_review_pr_state(review_id, state, at)
        .expect("the state the harvest read");
}

/// A round nobody has started: the request the dispatcher records before any session runs.
///
/// **This is the shape that yields a work item**, and a `waiting_round` deliberately does
/// not: a brief waiting to be confirmed and a draft waiting to be approved are both the
/// *developer's* turn, and `work_list` is the list of what an agent is to do. So the
/// claims about the queue are made with one shape and the claims about the work list with
/// the other.
fn requested_round(store: &Store, pr: i64, head: &str, round: i64) -> Review {
    store
        .create_review(new_review(REPO, pr, head, round))
        .expect("a round nobody has started")
}

fn work_ids(store: &Store) -> Vec<i64> {
    store
        .work_list(&WorkFilter::default())
        .expect("the work list")
        .into_iter()
        .map(|item| item.review.id)
        .collect()
}

/// Every review in any group of the queue, which is the set DEV-957 is about.
///
/// The five the queue draws, each through the store query that fills it. A review in none
/// of them is in front of nobody, which is what "finished" has to mean.
fn in_any_queue_group(store: &Store, review_id: i64) -> Vec<&'static str> {
    let mut found = Vec::new();
    let has = |reviews: Vec<Review>| reviews.iter().any(|review| review.id == review_id);

    if has(store.reviews_with_unconfirmed_brief(None).expect("briefs")) {
        found.push("briefs to confirm");
    }
    if has(store
        .reviews_with_draft_status(DraftStatus::Ready, None)
        .expect("drafts"))
    {
        found.push("drafts to approve");
    }
    if has(store
        .reviews_with_author_activity(None, None)
        .expect("replies"))
    {
        found.push("author replies");
    }
    if store
        .posted_reviews_awaiting_author(None)
        .expect("posted")
        .iter()
        .any(|posted| posted.review.id == review_id)
    {
        found.push("waiting on the author");
    }
    if store
        .reviews_in_progress(None)
        .expect("in progress")
        .iter()
        .any(|at| at.review.id == review_id)
    {
        found.push("with the agent");
    }
    found
}

fn listed(store: &Store, shown: ReviewsShown) -> Vec<i64> {
    store
        .reviews_at_step(None, shown)
        .expect("the reviews list")
        .into_iter()
        .map(|at| at.review.id)
        .collect()
}

#[test]
fn a_merged_pull_request_leaves_every_queue_group_and_the_work_list() {
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9585, "aaa1111", 1);

    assert_eq!(
        in_any_queue_group(&store, review.id),
        vec!["briefs to confirm", "drafts to approve"],
        "while the pull request is open it is the developer's turn, twice"
    );

    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    assert_eq!(
        in_any_queue_group(&store, review.id),
        Vec::<&str>::new(),
        "and merged it is in front of nobody"
    );
}

#[test]
fn a_merged_pull_request_gives_an_agent_no_work() {
    // The other half, and the one that costs money: the work list is what the Locus
    // dispatcher polls, so an item here is a session started on a review that can never
    // be posted.
    let (_dir, store) = temp_store();
    let review = requested_round(&store, 9581, "aaa1111", 1);

    assert_eq!(
        work_ids(&store),
        vec![review.id],
        "a request nobody has started is work for an agent"
    );

    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "and merged it is work for nobody"
    );
}

#[test]
fn a_closed_pull_request_leaves_them_too() {
    // Closed unmerged is the other settled state and the same answer: the review cannot
    // be posted on a pull request nobody is going to merge.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9586, "aaa1111", 1);

    harvest_found(&store, review.id, PrState::Closed, Some(1_791_000_000));

    assert_eq!(in_any_queue_group(&store, review.id), Vec::<&str>::new());
    assert_eq!(work_ids(&store), Vec::<i64>::new());
}

#[test]
fn an_open_pull_request_is_still_the_developers_turn() {
    // The converse, and worth pinning: a clause that answered "finished" for a pull
    // request nobody has harvested would empty the queue of a fresh database, and
    // `COALESCE(…, 'open')` in the derived state is what stops it.
    let (_dir, store) = temp_store();
    let unlooked = waiting_round(&store, 9587, "aaa1111", 1);
    let open = waiting_round(&store, 9588, "bbb2222", 1);
    harvest_found(&store, open.id, PrState::Open, None);

    for review in [&unlooked, &open] {
        assert_eq!(
            in_any_queue_group(&store, review.id),
            vec!["briefs to confirm", "drafts to approve"],
            "review {} is still waiting on the developer",
            review.id
        );
        assert_eq!(
            store.not_for_signing(review.id).expect("the standing"),
            None,
            "and review {} can still be signed off",
            review.id
        );
    }
}

#[test]
fn every_round_of_a_merged_pull_request_is_finished_whatever_was_harvested() {
    // **Patrick's #9568, which is the record this is built from**: four rounds of one
    // merged pull request, one posted and three not. Only the posted round was ever
    // harvested, so only that round's own `pr_state` column says `merged` — and the
    // queue consequently held the other three drafts for ever.
    let (_dir, store) = temp_store();

    // **In the order it happened**, which matters: round 1 was signed off and posted
    // while it was the only round, and the author's three later pushes each opened a
    // round whose draft was never signed off.
    let mut rounds: Vec<Review> = vec![waiting_round(&store, 9568, "c100000", 1)];
    store
        .approve_draft(rounds[0].id, ReviewEvent::Comment, Actor::Developer)
        .expect("approve round 1's draft");
    store
        .post_draft(rounds[0].id, "PRR_round_one")
        .expect("post it");
    for round in 2..=4 {
        rounds.push(waiting_round(
            &store,
            9568,
            &format!("c{round}00000"),
            round,
        ));
    }

    // The harvest read the round that was posted, and only that one.
    harvest_found(&store, rounds[0].id, PrState::Merged, Some(1_791_000_000));

    for round in &rounds {
        assert_eq!(
            in_any_queue_group(&store, round.id),
            Vec::<&str>::new(),
            "round {} of a merged pull request is in no group",
            round.round
        );
        assert_eq!(
            store.not_for_signing(round.id).expect("the standing"),
            Some(NotForSigning::Finished {
                how: PrState::Merged,
                at: Some(1_791_000_000)
            }),
            "and round {} says the pull request merged, not that it was superseded",
            round.round
        );
    }

    assert_eq!(
        store.get_review(rounds[3].id).unwrap().pr_state,
        None,
        "while round 4's own column still holds nothing, which is the whole point: the \
         state is the pull request's and the record keeps a copy per round"
    );
}

#[test]
fn step_seven_of_a_finished_posted_review_is_done_and_says_how_it_ended() {
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9569, "aaa1111", 1);
    store
        .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("approve the draft");
    store.post_draft(review.id, "PRR_one").expect("post it");

    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));
    let step = store
        .record_harvest_step(review.id)
        .expect("the step")
        .expect("a posted review has a step 7");

    assert_eq!(step.number, ReviewLayerName::Harvest.number());
    assert_eq!(step.state, ReviewLayerState::Done);
    assert_eq!(
        step.detail.as_deref(),
        Some("merged, nothing back yet"),
        "done, and how it ended"
    );
}

#[test]
fn a_reopened_pull_request_returns_its_latest_round_to_play() {
    // The whole of this: a closed pull request is read again, so a reopen can be seen at
    // all, and the derived state then takes the newest reading. Without the first half
    // the sweep would never look, and "the next harvest" would be a harvest that never
    // came.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9590, "aaa1111", 1);

    harvest_found(&store, review.id, PrState::Closed, Some(1_791_000_000));
    assert_eq!(in_any_queue_group(&store, review.id), Vec::<&str>::new());

    let up: Vec<i64> = store
        .reviews_to_harvest(None)
        .expect("the sweep")
        .into_iter()
        .map(|review| review.id)
        .collect();
    assert_eq!(
        up,
        vec![review.id],
        "a closed pull request is still read, which is what lets a reopen be noticed"
    );

    // What that read finds the next time.
    harvest_found(&store, review.id, PrState::Open, None);

    assert_eq!(
        in_any_queue_group(&store, review.id),
        vec!["briefs to confirm", "drafts to approve"],
        "and it is the developer's turn again"
    );
    assert_eq!(
        store.not_for_signing(review.id).expect("the standing"),
        None,
        "and its draft can be signed off once more"
    );
}

#[test]
fn a_reopen_read_on_the_newest_round_outranks_a_stale_close_on_an_older_one() {
    // **Why the derived state is the highest round holding one, and not "any round is
    // settled".** A pull request closed while it was on round 1, reopened, and reviewed
    // again on round 2 holds `closed` on round 1 for ever — nothing rewrites it — so a
    // rule that took any settled round as the answer would finish a live pull request.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 9591, "aaa1111").expect("round 1");
    store
        .add_finding(one.review.id, a_finding("round one was reviewed"))
        .expect("a finding");
    harvest_found(&store, one.review.id, PrState::Closed, Some(1_791_000_000));

    let two = store.open_review(REPO, 9591, "bbb2222").expect("round 2");
    assert_eq!(two.review.round, 2);
    store
        .put_brief(two.review.id, a_brief("reopened, and reviewed again"))
        .expect("a brief");
    harvest_found(&store, two.review.id, PrState::Open, None);

    assert_eq!(
        in_any_queue_group(&store, two.review.id),
        vec!["briefs to confirm"],
        "round 2 is the developer's turn, whatever round 1's column still says"
    );
    assert_eq!(
        store.not_for_signing(two.review.id).expect("the standing"),
        None,
        "and its brief can be confirmed"
    );
}

#[test]
fn a_dismissed_review_whose_pull_request_merges_stays_dismissed() {
    // Two facts about one review, and one of them is somebody's act. The issue settles
    // it: a dismissed review that then merges stays dismissed.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9592, "aaa1111", 1);
    store
        .dismiss_review(review.id, "merged before I got to it")
        .expect("dismiss it");

    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    assert_eq!(
        listed(&store, ReviewsShown::Dismissed),
        vec![review.id],
        "it is under Dismissed"
    );
    assert_eq!(
        listed(&store, ReviewsShown::Finished),
        Vec::<i64>::new(),
        "and not under Finished, which would call a developer's act an outcome"
    );
    assert_eq!(
        store
            .reviews_at_step(None, ReviewsShown::Everything)
            .expect("everything")
            .iter()
            .find(|at| at.review.id == review.id)
            .and_then(|at| at.left_the_desk),
        Some(LeftTheDesk::Dismissed),
        "and the row says dismissed rather than finished"
    );
    assert_eq!(
        store.not_for_signing(review.id).expect("the standing"),
        Some(NotForSigning::Dismissed),
        "and the refusal says so too, so the filter and the words cannot disagree"
    );
}

#[test]
fn the_reviews_list_tells_the_two_ways_off_the_desk_apart() {
    let (_dir, store) = temp_store();
    let in_play = waiting_round(&store, 9593, "aaa1111", 1);
    let finished = waiting_round(&store, 9594, "bbb2222", 1);
    let dismissed = waiting_round(&store, 9595, "ccc3333", 1);

    harvest_found(&store, finished.id, PrState::Merged, Some(1_791_000_000));
    store
        .dismiss_review(dismissed.id, "a duplicate of #9593")
        .expect("dismiss it");

    assert_eq!(
        listed(&store, ReviewsShown::InPlay),
        vec![in_play.id],
        "In play excludes both"
    );
    assert_eq!(listed(&store, ReviewsShown::Finished), vec![finished.id]);
    assert_eq!(listed(&store, ReviewsShown::Dismissed), vec![dismissed.id]);

    let everything = store
        .reviews_at_step(None, ReviewsShown::Everything)
        .expect("everything");
    let said: Vec<(i64, Option<LeftTheDesk>)> = everything
        .iter()
        .map(|at| (at.review.id, at.left_the_desk))
        .collect();
    assert_eq!(
        said,
        vec![
            (dismissed.id, Some(LeftTheDesk::Dismissed)),
            (finished.id, Some(LeftTheDesk::Finished)),
            (in_play.id, None),
        ],
        "and on one page each row says which it is"
    );
    assert_eq!(
        everything
            .iter()
            .find(|at| at.review.id == finished.id)
            .and_then(|at| at.finished_at),
        Some(1_791_000_000),
        "the finished row carries when its pull request merged, which dates the sentence \
         the draft page draws"
    );
    assert!(
        everything
            .iter()
            .find(|at| at.review.id == dismissed.id)
            .and_then(|at| at.finished_at)
            .is_none(),
        "and a dismissed row carries none, because its row does not say it merged"
    );
}

#[test]
fn an_unposted_round_a_newer_round_supersedes_leaves_the_queue_and_the_work_list() {
    // **Patrick's #9541, which drew two rows: round 1 and round 2, both "Draft ready".**
    // Signing round 1 off would have asked a session to post a review of a commit the
    // author has pushed past.
    let (_dir, store) = temp_store();
    let one = waiting_round(&store, 9541, "aaa1111", 1);
    let two = waiting_round(&store, 9541, "bbb2222", 2);

    assert_eq!(
        in_any_queue_group(&store, one.id),
        Vec::<&str>::new(),
        "round 1 can never be the review that posts, so it is in no group"
    );
    assert_eq!(
        in_any_queue_group(&store, two.id),
        vec!["briefs to confirm", "drafts to approve"],
        "and round 2 is the one row the pull request gets"
    );
    assert_eq!(
        store.not_for_signing(one.id).expect("the standing"),
        Some(NotForSigning::Superseded { by_round: 2 }),
        "and round 1's draft page says which round superseded it"
    );
    assert_eq!(store.not_for_signing(two.id).expect("the standing"), None);
}

#[test]
fn a_superseded_round_is_offered_to_no_agent() {
    // The work list has restricted itself to the newest round since DEV-945, for this
    // reason stated the other way round — nothing moves a superseded round's state again,
    // so nothing could retract its item. Pinned here because it is half of DEV-957's "a
    // superseded unposted round leaves the queue and the work list", and because a
    // superseded round at the head of an oldest-first list is work in front of live work.
    let (_dir, store) = temp_store();
    let one = requested_round(&store, 9543, "aaa1111", 1);
    let two = requested_round(&store, 9543, "bbb2222", 2);

    assert_eq!(
        work_ids(&store),
        vec![two.id],
        "one pull request, one item, and it is the newest round"
    );
    assert_eq!(
        store.not_for_signing(one.id).expect("the standing"),
        Some(NotForSigning::Superseded { by_round: 2 })
    );
}

#[test]
fn a_posted_round_is_not_superseded_by_a_newer_one() {
    // A round that posted *did* post, and a later round does not unmake that. It is the
    // condition on supersession rather than a consequence of it: what a newer round
    // supersedes is a draft still waiting, which can no longer be the review that posts.
    let (_dir, store) = temp_store();
    let one = waiting_round(&store, 9596, "aaa1111", 1);
    store
        .approve_draft(one.id, ReviewEvent::Comment, Actor::Developer)
        .expect("approve it");
    store.post_draft(one.id, "PRR_one").expect("post it");

    let two = waiting_round(&store, 9596, "bbb2222", 2);

    assert_eq!(
        store.not_for_signing(one.id).expect("the standing"),
        None,
        "round 1 posted, so a newer round does not make it a thing that never happened"
    );
    assert_eq!(store.not_for_signing(two.id).expect("the standing"), None);
}

#[test]
fn a_posted_round_whose_author_replied_still_reaches_the_developer() {
    // **The fact a newer round must not take away.** #9585 had a posted round 1 with the
    // author's reply on it and an unposted round 2 draft. Round 2 is the row the pull
    // request gets, and the reply is still served — the store keeps the fact and the queue
    // decides it draws one row, which is `app/pages/index.vue`'s merge rather than this.
    let (_dir, store) = temp_store();
    let one = waiting_round(&store, 9585, "aaa1111", 1);
    store
        .approve_draft(one.id, ReviewEvent::Comment, Actor::Developer)
        .expect("approve it");
    store.post_draft(one.id, "PRR_one").expect("post it");
    store
        .add_outcome(
            review_desk_store::FindingRef {
                review_id: one.id,
                seq: 1,
            },
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: Actor::Author,
                detail: Some("{\"body\":\"fixed in the next push\"}".to_string()),
                occurred_at: Some(1_790_000_000),
            },
        )
        .expect("the author's reply");

    let two = waiting_round(&store, 9585, "bbb2222", 2);

    assert_eq!(
        in_any_queue_group(&store, one.id),
        vec!["author replies"],
        "the reply is still the developer's to read"
    );
    assert_eq!(
        in_any_queue_group(&store, two.id),
        vec!["briefs to confirm", "drafts to approve"],
        "and round 2 is where the unposted work is"
    );
}

#[test]
fn the_four_acts_that_need_an_agent_are_refused_on_a_finished_review() {
    // The same four `refuse_if_dismissed` has always guarded, for the reason its own
    // comment gives read off the pull request instead of off the developer: each is an
    // item in `work list`, and a finished review is in no work list — so performing one
    // would be the developer signing something off that nothing will ever act on.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9597, "aaa1111", 1);
    store
        .set_finding_disposition(review.id, 1, Disposition::HeldBack, None)
        .expect("hold the finding back");
    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    let refusals: Vec<(&str, review_desk_store::StoreError)> = vec![
        (
            "confirming the brief",
            store
                .confirm_brief(review.id, Actor::Developer)
                .expect_err("refused"),
        ),
        (
            "approving the draft",
            store
                .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
                .expect_err("refused"),
        ),
        (
            "sending the draft back",
            store
                .send_back_draft(review.id, "raise the retry finding")
                .expect_err("refused"),
        ),
        (
            "asking for a nit ticket",
            store
                .request_nit_ticket(review.id, None)
                .expect_err("refused"),
        ),
    ];

    for (act, error) in refusals {
        assert!(error.is_refusal(), "{act}: refused, not broken");
        assert_eq!(error.code(), "finished_review", "{act}");
        assert!(
            error.to_string().contains("merged"),
            "{act}: the refusal names the reason — {error}"
        );
    }

    assert_eq!(
        store.get_draft(review.id).unwrap().status,
        DraftStatus::Ready,
        "and the refusals wrote nothing"
    );
}

#[test]
fn approving_a_superseded_rounds_draft_is_refused_naming_the_round_that_can_post() {
    let (_dir, store) = temp_store();
    let one = waiting_round(&store, 9542, "aaa1111", 1);
    waiting_round(&store, 9542, "bbb2222", 2);

    let error = store
        .approve_draft(one.id, ReviewEvent::Comment, Actor::Developer)
        .expect_err("a superseded round cannot be the review that posts");

    assert!(error.is_refusal());
    assert_eq!(error.code(), "superseded_round");
    assert!(
        error.to_string().contains("round 2"),
        "and it names the round that can: {error}"
    );
    assert_eq!(
        store.get_draft(one.id).unwrap().status,
        DraftStatus::Ready,
        "and nothing was written"
    );
}

#[test]
fn a_merged_pull_request_outranks_a_supersession_in_what_the_page_says() {
    // Once the pull request is over, which round is newest no longer matters — and the
    // sentence a reader needs is the one about the pull request. Round 1 of Patrick's
    // #9568 is both, and it says merged.
    let (_dir, store) = temp_store();
    let one = waiting_round(&store, 9598, "aaa1111", 1);
    let two = waiting_round(&store, 9598, "bbb2222", 2);
    harvest_found(&store, two.id, PrState::Merged, Some(1_791_000_000));

    assert_eq!(
        store.not_for_signing(one.id).expect("the standing"),
        Some(NotForSigning::Finished {
            how: PrState::Merged,
            at: Some(1_791_000_000)
        }),
        "the pull request merged, which is the bigger fact than being round 1 of 2"
    );
}

#[test]
fn a_finished_review_with_a_session_running_is_in_no_group_either() {
    // "With the agent" is a group too, and a session still writing layers on a merged
    // pull request is work nobody wants the answer to. The exclusion is in `IN_PLAY`, so
    // this group gets it without its own clause — which is the whole argument for putting
    // it there.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9599, "aaa1111", 1);
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                session_id: Some("ses_merged".to_string()),
                ..Default::default()
            },
        )
        .expect("link a session");
    store
        .set_review_layer(
            review.id,
            ReviewLayerName::CodeReview.number(),
            ReviewLayerState::Running,
            Some("code review"),
        )
        .expect("a running layer");

    assert!(
        in_any_queue_group(&store, review.id).contains(&"with the agent"),
        "while the pull request is open a running session is drawn"
    );

    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    assert_eq!(in_any_queue_group(&store, review.id), Vec::<&str>::new());
}

#[test]
fn a_repository_whose_only_review_is_finished_stays_in_the_picker() {
    // `list_repos` applies the clause to the count it draws and not to the set it lists,
    // which `IN_PLAY`'s own comment states. Without the second half, the Finished view
    // could not be reached for such a repository at all.
    let (_dir, store) = temp_store();
    let review = waiting_round(&store, 9600, "aaa1111", 1);
    harvest_found(&store, review.id, PrState::Merged, Some(1_791_000_000));

    let repos = store.list_repos().expect("the repositories");
    let found = repos
        .iter()
        .find(|summary| summary.repo == REPO)
        .expect("the repository is still listed");
    assert_eq!(
        found.open_reviews, 0,
        "and it counts no open review, because there is none"
    );
}
