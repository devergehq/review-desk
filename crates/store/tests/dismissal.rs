//! Taking a review out of play, bringing it back, and removing one that never started.
//!
//! **Three claims, and each is the argument for the decision behind it.** DEV-945 asks for
//! dismissal everywhere and a delete only for a review that never got past being
//! requested, and the reasoning is that a review carries work a delete would lose. So the
//! tests here are not only that the columns move: they are that a dismissal keeps every
//! row under the review, that a dismissed review is in front of nobody — not the
//! developer's queue, not the work list a dispatcher reads — and that a delete is refused
//! for anything with a brief, a finding or a draft, in words that say which.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, DiffAtHead, Disposition, DraftStatus, FileCheck, FindingRef, NewBrief, NewDraft,
    NewFileChange, NewFinding, NewOutcome, NewReviewSource, NitTicketStatus, OpenedAction,
    OutcomeKind, Review, ReviewEvent, ReviewFacts, ReviewLayerState, ReviewLinks, ReviewsShown,
    Severity, SourceKind, Store, WorkFilter,
};

const REPO: &str = "acme/portal";

/// A review nobody has started: what the Locus dispatcher leaves behind when it records a
/// review request, and the only shape a delete is offered for.
fn requested(store: &Store, pr: i64, head: &str) -> Review {
    store
        .create_review(new_review(REPO, pr, head, 1))
        .expect("a review")
}

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Blocker,
        claim: claim.to_string(),
        disposition: Disposition::InDraft,
        ..Default::default()
    }
}

/// Every review the work list is offering an agent.
fn work_ids(store: &Store) -> Vec<i64> {
    store
        .work_list(&WorkFilter::default())
        .expect("the work list")
        .into_iter()
        .map(|item| item.review.id)
        .collect()
}

/// The ids the Reviews list draws for one choice about dismissed reviews.
fn listed(store: &Store, dismissed: ReviewsShown) -> Vec<i64> {
    store
        .reviews_at_step(None, dismissed)
        .expect("the reviews list")
        .into_iter()
        .map(|at| at.review.id)
        .collect()
}

#[test]
fn a_dismissal_records_when_and_why() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 777, "4f1a9c2");

    let dismissed = store
        .dismiss_review(review.id, "a build session recorded this by accident")
        .expect("dismiss it");

    assert!(dismissed.is_dismissed());
    assert!(dismissed.dismissed_at.is_some());
    assert_eq!(
        dismissed.dismissed_reason.as_deref(),
        Some("a build session recorded this by accident")
    );
    assert_eq!(
        store.get_review(review.id).unwrap().dismissed_reason,
        dismissed.dismissed_reason,
        "and it is on the record, not only in the answer"
    );
}

#[test]
fn a_reason_is_trimmed_and_a_blank_one_is_refused() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 778, "4f1a9c2");

    for blank in ["", "   ", "\n\t "] {
        let error = store
            .dismiss_review(review.id, blank)
            .expect_err("a review hidden silently is one nobody can say why they hid");
        assert!(error.is_refusal());
        assert_eq!(error.code(), "invalid_value");
        assert!(
            !store.get_review(review.id).unwrap().is_dismissed(),
            "and the refusal wrote nothing"
        );
    }

    let dismissed = store
        .dismiss_review(review.id, "  the pull request was closed unmerged  ")
        .expect("dismiss it");
    assert_eq!(
        dismissed.dismissed_reason.as_deref(),
        Some("the pull request was closed unmerged"),
        "stored trimmed, so the page draws no leading space"
    );
}

#[test]
fn dismissing_a_dismissed_review_is_refused_and_keeps_the_first_words() {
    // The first dismissal is the one the developer made. A second call would move its
    // time and replace its words, which is a worse answer than a refusal naming when it
    // was dismissed.
    let (_dir, store) = temp_store();
    let review = requested(&store, 779, "4f1a9c2");
    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let error = store
        .dismiss_review(review.id, "something else entirely")
        .expect_err("it is already dismissed");
    assert!(error.is_refusal());
    assert_eq!(error.code(), "already_dismissed");

    assert_eq!(
        store
            .get_review(review.id)
            .unwrap()
            .dismissed_reason
            .as_deref(),
        Some("the pull request was closed unmerged"),
        "the words the developer wrote are still there"
    );
}

#[test]
fn a_dismissal_keeps_every_row_under_the_review() {
    // **The whole argument for choosing a dismissal over a delete.** Patrick, 12 October
    // 2026: a review with findings carries your sign-offs and the evidence rules are built
    // from. If a dismissal lost any of it the decision would have been made on a false
    // premise, so it is asserted rather than assumed.
    let (_dir, store) = temp_store();
    let review = requested(&store, 780, "4f1a9c2");

    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "the problem".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer's sign-off at layer 2");
    store
        .add_finding(
            review.id,
            a_finding("a retried delivery has no idempotency key"),
        )
        .expect("a finding");
    store
        .set_finding_disposition(
            review.id,
            1,
            Disposition::HeldBack,
            Some("a nit".to_string()),
        )
        .expect("the developer's disposition");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: "A review body.".to_string(),
                status: DraftStatus::Ready,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");
    store
        .set_review_layer(review.id, 5, ReviewLayerState::Done, Some("1 finding"))
        .expect("a layer");

    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let whole = store.full_review(review.id).expect("still readable whole");
    assert!(whole.brief.is_some(), "the brief is kept");
    assert!(
        whole.brief.as_ref().unwrap().confirmed_at.is_some(),
        "and so is the developer's sign-off on it"
    );
    assert_eq!(whole.findings.len(), 1, "the finding is kept");
    assert_eq!(
        whole.findings[0].disposition,
        Disposition::HeldBack,
        "and the disposition the developer chose"
    );
    assert!(whole.draft.is_some(), "the draft is kept");
    assert_eq!(
        store
            .get_review_layer(review.id, 5)
            .expect("the layer")
            .state,
        ReviewLayerState::Done,
        "and what the layers recorded"
    );
}

#[test]
fn restoring_clears_the_dismissal() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 781, "4f1a9c2");
    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let restored = store.restore_review(review.id).expect("restore it");

    assert!(!restored.is_dismissed());
    assert_eq!(restored.dismissed_at, None);
    assert_eq!(
        restored.dismissed_reason, None,
        "both columns go together: a reason with no time would read as in play to every \
         query and as dismissed to the page"
    );
}

#[test]
fn restoring_a_review_that_is_not_dismissed_is_refused() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 782, "4f1a9c2");

    let error = store
        .restore_review(review.id)
        .expect_err("there is nothing to bring back");
    assert!(error.is_refusal());
    assert_eq!(error.code(), "not_dismissed");
}

#[test]
fn a_dismissed_review_is_offered_to_no_agent() {
    // DEV-945's fifth requirement, and the whole mechanism behind it: the dispatcher reads
    // this list, so a dismissed review that appeared here would have a session started on
    // it.
    let (_dir, store) = temp_store();
    let review = requested(&store, 783, "4f1a9c2");
    assert_eq!(work_ids(&store), vec![review.id]);

    store
        .dismiss_review(review.id, "nobody asked for this review")
        .expect("dismiss it");
    assert_eq!(work_ids(&store), Vec::<i64>::new());

    store.restore_review(review.id).expect("restore it");
    assert_eq!(
        work_ids(&store),
        vec![review.id],
        "and it is work again the moment it is brought back"
    );
}

#[test]
fn dismissing_the_newest_round_resurrects_no_earlier_round() {
    // Round 1 was reviewed and superseded; nothing can move its state again, so it must
    // not become the live round because round 2 was dismissed. The round subquery counts
    // every round for exactly this reason.
    let (_dir, store) = temp_store();
    let round_one = store.open_review(REPO, 784, "aaa1111").expect("round 1");
    store
        .add_finding(round_one.review.id, a_finding("round one was reviewed"))
        .expect("a finding");
    let round_two = store.open_review(REPO, 784, "bbb2222").expect("round 2");
    assert_eq!(work_ids(&store), vec![round_two.review.id]);

    store
        .dismiss_review(round_two.review.id, "the pull request was closed unmerged")
        .expect("dismiss the newest round");

    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "round 1 is still superseded, and a dead round at the head of the queue is what \
         the newest-round rule exists to prevent"
    );
}

#[test]
fn a_dismissed_review_is_in_no_group_of_the_queue() {
    let (_dir, store) = temp_store();

    // One review waiting on a confirmation, one waiting on a draft approval, and one a
    // session is working on: three of the queue's groups, from three reviews.
    let waiting = requested(&store, 790, "aaa1111");
    store
        .put_brief(
            waiting.id,
            NewBrief {
                problem: "the problem".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");

    let ready = requested(&store, 791, "bbb2222");
    store
        .put_draft(
            ready.id,
            NewDraft {
                body: "A review body.".to_string(),
                status: DraftStatus::Ready,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");

    let running = requested(&store, 792, "ccc3333");
    store
        .set_review_layer(running.id, 3, ReviewLayerState::Running, Some("reading"))
        .expect("a running layer");

    assert_eq!(store.reviews_with_unconfirmed_brief(None).unwrap().len(), 1);
    assert_eq!(
        store
            .reviews_with_draft_status(DraftStatus::Ready, None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.reviews_in_progress(None).unwrap().len(), 1);

    for review in [&waiting, &ready, &running] {
        store
            .dismiss_review(review.id, "the pull request was closed unmerged")
            .expect("dismiss it");
    }

    assert!(
        store
            .reviews_with_unconfirmed_brief(None)
            .unwrap()
            .is_empty(),
        "a dismissed review is not waiting for a confirmation"
    );
    assert!(
        store
            .reviews_with_draft_status(DraftStatus::Ready, None)
            .unwrap()
            .is_empty(),
        "nor for a draft to be approved"
    );
    assert!(
        store.reviews_in_progress(None).unwrap().is_empty(),
        "nor is it in progress"
    );
}

#[test]
fn a_dismissed_review_is_in_neither_of_the_queues_two_author_groups() {
    // The other two groups, which need a posted review and an author who has acted on it.
    let (_dir, store) = temp_store();
    let review = requested(&store, 797, "aaa1111");
    store
        .add_finding(
            review.id,
            a_finding("a retried delivery has no idempotency key"),
        )
        .expect("a finding");
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                github_review_id: Some("gh-review-3312".to_string()),
                ..Default::default()
            },
        )
        .expect("posted, the one way the command line can post one");
    store
        .add_outcome(
            FindingRef {
                review_id: review.id,
                seq: 1,
            },
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: Actor::Author,
                detail: Some("the author replied".to_string()),
                occurred_at: None,
            },
        )
        .expect("the author's reply");

    assert_eq!(
        store.posted_reviews_awaiting_author(None).unwrap().len(),
        1,
        "it is waiting on its author"
    );
    assert_eq!(
        store
            .reviews_with_author_activity(None, None)
            .unwrap()
            .len(),
        1,
        "and the author has replied"
    );

    store
        .dismiss_review(
            review.id,
            "the author is handling it in another pull request",
        )
        .expect("dismiss it");

    assert!(
        store
            .posted_reviews_awaiting_author(None)
            .unwrap()
            .is_empty(),
        "a dismissed review is waiting on nobody"
    );
    assert!(
        store
            .reviews_with_author_activity(None, None)
            .unwrap()
            .is_empty(),
        "and its author's reply is not in the developer's queue"
    );
}

#[test]
fn a_dismissed_reviews_filed_nit_ticket_is_still_counted_as_handled() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 798, "aaa1111");
    store
        .add_finding(review.id, a_finding("a nit"))
        .expect("a finding");
    store
        .set_finding_disposition(
            review.id,
            1,
            Disposition::HeldBack,
            Some("a nit".to_string()),
        )
        .expect("held back");
    store
        .request_nit_ticket(review.id, Some("ABC-500"))
        .expect("a ticket was filed for it");
    assert_eq!(store.filed_nit_tickets(None, None).unwrap().len(), 1);

    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");
    assert_eq!(
        store.filed_nit_tickets(None, None).unwrap().len(),
        1,
        "\"handled without you\" is a log of what happened, and dismissing the review \
         afterwards does not unfile the ticket — the same ground the timings are exempted \
         on. This asserted the opposite until a review of this branch put the two \
         readings beside each other."
    );
}

#[test]
fn a_dismissed_review_is_not_counted_open_on_its_repository_and_keeps_it_in_the_picker() {
    // Two halves of one answer. The count is what the queue draws, so a dismissed review
    // must not be in it — but the repository has to stay in the picker, or the Reviews
    // list's dismissed view could not be reached for it.
    let (_dir, store) = temp_store();
    let review = requested(&store, 793, "aaa1111");

    assert_eq!(store.list_repos().unwrap()[0].open_reviews, 1);

    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");

    let repos = store.list_repos().unwrap();
    assert_eq!(repos.len(), 1, "the repository is still in the picker");
    assert_eq!(repos[0].repo, REPO);
    assert_eq!(
        repos[0].open_reviews, 0,
        "and it has no open review any more"
    );
}

#[test]
fn the_reviews_list_leaves_dismissed_reviews_out_until_they_are_asked_for() {
    let (_dir, store) = temp_store();
    let in_play = requested(&store, 794, "aaa1111");
    let gone = requested(&store, 795, "bbb2222");
    store
        .dismiss_review(gone.id, "a duplicate of 794")
        .expect("dismiss it");

    assert_eq!(
        listed(&store, ReviewsShown::default()),
        vec![in_play.id],
        "the default view is the reviews in play"
    );
    assert_eq!(
        listed(&store, ReviewsShown::Dismissed),
        vec![gone.id],
        "and the filter is the dismissed ones on their own"
    );
    let both = listed(&store, ReviewsShown::Everything);
    assert_eq!(both.len(), 2, "or the whole record on one page: {both:?}");

    let row = store
        .reviews_at_step(None, ReviewsShown::Dismissed)
        .unwrap()
        .remove(0);
    assert!(
        row.review.dismissed_at.is_some(),
        "and the row carries when it was dismissed"
    );
    assert_eq!(
        row.review.dismissed_reason.as_deref(),
        Some("a duplicate of 794"),
        "and why, which is what the page draws beside it"
    );
}

#[test]
fn a_dismissed_review_is_not_read_back_from_github() {
    // Wider than the two places DEV-945 names, and the same sentence: Review Desk stops
    // working on a dismissed review, so it does not spend a GitHub read a quarter of an
    // hour on one.
    let (_dir, store) = temp_store();
    let review = requested(&store, 796, "aaa1111");
    assert_eq!(store.reviews_to_harvest(None).unwrap().len(), 1);

    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");
    assert!(store.reviews_to_harvest(None).unwrap().is_empty());

    store.restore_review(review.id).expect("restore it");
    assert_eq!(
        store.reviews_to_harvest(None).unwrap().len(),
        1,
        "and the next sweep reads it again, because this list is decided from the record"
    );
}

#[test]
fn deleting_a_review_nobody_started_takes_every_row_under_it() {
    // What a never-started review can still be holding: the layers a session wrote before
    // it died, and the provenance of an understanding it had begun to record. None of
    // those is work the developer would lose — DEV-945 names the brief, the findings and
    // the draft — and all of them have to go, or the database keeps rows pointing at a
    // review that is not there.
    let (_dir, store) = temp_store();
    let review = requested(&store, 777, "4f1a9c2");

    store
        .set_review_layer(
            review.id,
            1,
            ReviewLayerState::Done,
            Some("the lint passed"),
        )
        .expect("a layer");
    store
        .put_review_sources(
            review.id,
            vec![NewReviewSource {
                kind: SourceKind::Description,
                label: "The pull request description".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .expect("a source");
    store
        .put_file_changes(
            review.id,
            vec![NewFileChange {
                path: "app/Reporting/ColumnPolicy.php".to_string(),
                summary: "the column policy".to_string(),
                lines_added: 12,
                lines_removed: 3,
                checked: FileCheck::ReadInFull,
                how_checked: None,
            }],
        )
        .expect("a file change");

    let deleted = store.delete_review(review.id).expect("delete it");
    assert_eq!(deleted.id, review.id, "it hands back the review that went");
    assert_eq!(deleted.pr, 777);

    assert!(
        store.find_review_by_id(review.id).unwrap().is_none(),
        "the review is gone"
    );
    assert!(
        store
            .find_review_by_head(REPO, 777, "4f1a9c2")
            .unwrap()
            .is_none(),
        "and nothing finds it by its commit"
    );
    assert_eq!(
        store.latest_round(REPO, 777).unwrap(),
        None,
        "the pull request has no rounds again"
    );
    assert_eq!(
        store.list_reviews(REPO, 777).unwrap().len(),
        0,
        "and the pull request lists none"
    );
    assert!(
        store.get_review_layer(review.id, 1).is_err(),
        "its layers went with it"
    );
    // The provenance reads go through `get_review` first, so the rows themselves are
    // read directly: an assertion that only checked `review_provenance` would pass on the
    // `NotFound` the missing review raises and say nothing about whether the rows went.
    assert!(
        store.list_review_sources(review.id).unwrap().is_empty(),
        "its sources went with it"
    );
    assert!(
        store.list_file_changes(review.id).unwrap().is_empty(),
        "and its per-file change map"
    );
    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "and no agent is offered it"
    );
}

#[test]
fn deleting_a_review_that_holds_a_brief_a_finding_or_a_draft_is_refused_saying_which() {
    // The refusal is read by the developer, so it has to name what it is protecting and
    // point at the act that is allowed instead. Three reviews, so each of the three things
    // is the sole reason for its own refusal.
    let (_dir, store) = temp_store();

    let with_brief = requested(&store, 800, "aaa1111");
    store
        .put_brief(
            with_brief.id,
            NewBrief {
                problem: "the problem".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");

    let with_findings = requested(&store, 801, "bbb2222");
    store
        .add_finding(with_findings.id, a_finding("one"))
        .expect("a finding");
    store
        .add_finding(with_findings.id, a_finding("two"))
        .expect("a second finding");

    let with_draft = requested(&store, 802, "ccc3333");
    store
        .put_draft(
            with_draft.id,
            NewDraft {
                body: "A review body.".to_string(),
                status: DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");

    for (review, holds) in [
        (&with_brief, "a brief"),
        (&with_findings, "2 findings"),
        (&with_draft, "a draft"),
    ] {
        let error = store
            .delete_review(review.id)
            .expect_err("deleting it would lose that");
        assert!(error.is_refusal());
        assert_eq!(error.code(), "review_started");
        let said = error.to_string();
        assert!(
            said.contains(holds),
            "the refusal must say what it is protecting; it said {said:?}"
        );
        assert!(
            said.contains("dismiss"),
            "and point at the act that is allowed; it said {said:?}"
        );
        assert!(
            store.find_review_by_id(review.id).unwrap().is_some(),
            "and the review is still there"
        );
    }
}

#[test]
fn a_dismissed_review_can_still_be_deleted_if_it_never_started() {
    // The two are not alternatives on one review: the stray review DEV-945 was raised
    // from will be dismissed and then removed, in that order, because dismissal is the
    // reversible act and the developer may take a moment to decide.
    let (_dir, store) = temp_store();
    let review = requested(&store, 803, "aaa1111");
    store
        .dismiss_review(review.id, "a build session recorded this by accident")
        .expect("dismiss it");

    store.delete_review(review.id).expect("and then delete it");
    assert!(store.find_review_by_id(review.id).unwrap().is_none());
}

#[test]
fn a_dismissed_request_keeps_its_dismissal_when_the_author_pushes() {
    // A push is not the developer changing their mind. The head moves — one request is
    // one review — and the review stays dismissed, so no session is dispatched for a
    // request that was declined. DEV-945 puts automatic dismissal out of scope, and
    // automatic un-dismissal is that decision read backwards.
    let (_dir, store) = temp_store();
    let recorded = store.open_review(REPO, 804, "aaa1111").expect("record it");
    store
        .dismiss_review(recorded.review.id, "nobody asked for this review")
        .expect("dismiss it");

    let again = store
        .open_review(REPO, 804, "bbb2222")
        .expect("record it again on the commit now in front of us");

    assert_eq!(
        again.review.id, recorded.review.id,
        "one request, one review"
    );
    assert_eq!(again.review.head_sha, "bbb2222", "the head moved");
    assert!(again.review.is_dismissed(), "and it is still dismissed");
    assert_eq!(
        work_ids(&store),
        Vec::<i64>::new(),
        "so no agent is offered it"
    );
}

#[test]
fn what_a_review_holds_is_a_question_about_a_delete_and_not_about_a_moved_head() {
    // **It was one predicate and two callers, and DEV-962 took one of them away.** A
    // delete is still refused for a review holding a brief, a finding or a draft — that is
    // work the developer would lose. A head still moves, because the round has not posted:
    // the session re-reads the delta and the record keeps the move. Asserted together, so
    // the two answers cannot drift back into agreeing by accident.
    let (_dir, store) = temp_store();
    let review = requested(&store, 805, "aaa1111");
    assert!(!store.review_work(review.id).unwrap().started());

    store
        .add_finding(review.id, a_finding("a finding"))
        .expect("a finding");

    assert!(store.review_work(review.id).unwrap().started());
    assert!(
        store.delete_review(review.id).is_err(),
        "a delete is refused for it"
    );
    let pushed = store.open_review(REPO, 805, "bbb2222").unwrap();
    assert_eq!(
        pushed.review.round, 1,
        "and a new commit on it moves this round rather than opening the next"
    );
    assert_eq!(pushed.action, OpenedAction::HeadMoved);
    assert_eq!(
        store.head_updates(review.id).unwrap().moved,
        1,
        "with the move on the record, which is what replaced the refusal"
    );
}

#[test]
fn every_query_that_lists_reviews_says_what_it_does_about_the_ones_that_have_left() {
    // **The structural half of "a review off the desk is in front of nobody".** The tests
    // above check the queries that exist today; this one is against the next one, written
    // by somebody who does not know there is a clause to paste. A list that forgot it
    // would put a dismissed review back in the developer's queue and nothing anywhere
    // would fail.
    //
    // **Two ways off the desk since DEV-957, and one clause.** A merged or closed pull
    // request leaves the queue the same way a dismissal does, and `IN_PLAY` carries both
    // — so this check needs no second pass, and a query that pastes the clause gets the
    // finish with the dismissal. That is the point of putting it there: finishing is
    // *derived* rather than stored, so a query spelling it out for itself would be a
    // second definition of when a review is over.
    //
    // Written over the source the way `the_work_query_contains_no_write_statement` is,
    // and for the same reason: "every list carries the clause" is a claim about code, and
    // code stops being like that one convenience at a time.
    //
    // **It walks functions that return a list of reviews**, which is the set the claim is
    // about. A lookup by id or by head commit must *not* exclude a dismissed review —
    // every screen that draws one reads it that way — and `latest_round` must count
    // dismissed rounds or a round number would be reused. Keying on
    // `Result<Vec<…>>` is what tells those apart from the lists.
    //
    // **An exemption is a decision, written here and argued where the query is.** Two
    // lists keep their dismissed reviews on purpose; each is named below with the reason
    // in one line, so that adding a third is a visible act rather than a quiet one. This
    // file's first version scanned two modules by prepared statement and could see
    // neither of them — a review of this branch found that, which is the second time the
    // guard's own blind spot has been the finding.
    const EXEMPT: &[(&str, &str)] = &[
        (
            "list_reviews",
            "one pull request's own rounds, which is history: a hole where round 2 was \
             would make the round numbers read as a gap in the record",
        ),
        (
            "filed_nit_tickets",
            "the Inbox's \"handled without you\": a log of what happened in the window, \
             and dismissing a review afterwards does not unfile the ticket it had",
        ),
        (
            "finished_review_timings",
            "how long each finished review took; dismissing a review afterwards does not \
             unmeasure the time it took, and dropping it would bias whatever is computed \
             from this read",
        ),
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    let mut exempted = 0;

    // **Every module in the directory, not a list of the four that had one.** The list
    // was the blind spot this test's own comment says it has twice been caught by:
    // `round.rs`, `rule.rs` and `house_style.rs` all read `FROM reviews` and were outside
    // it, so a list of reviews added in any of them would have passed in silence. A
    // module added tomorrow is walked without anybody remembering to add it.
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("src/entities"))
        .expect("the entities directory")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 15,
        "only {} modules were found, so this check is walking the wrong directory",
        files.len()
    );

    for file in files {
        let file = file.display().to_string();
        let source = std::fs::read_to_string(&file).expect("the module's source");

        // One block per function: from each `fn name(` to the next. Crude on purpose —
        // the alternative is parsing Rust, and what this needs is only to know which
        // function a statement sits in.
        let starts: Vec<(usize, String)> = source
            .match_indices("fn ")
            .filter_map(|(at, _)| {
                let rest = &source[at + 3..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (!name.is_empty() && rest[name.len()..].starts_with('(')).then_some((at, name))
            })
            .collect();

        for (index, (at, name)) in starts.iter().enumerate() {
            let until = starts
                .get(index + 1)
                .map_or(source.len(), |(next, _)| *next);
            let block = &source[*at..until];

            // A list of reviews, which is what the claim is about — not a lookup, not a
            // scalar, and not the delete.
            if !block.contains("FROM reviews") || !block.contains("Result<Vec<") {
                continue;
            }

            if let Some((_, why)) = EXEMPT.iter().find(|(listed, _)| listed == name) {
                assert!(
                    !block.contains("{IN_PLAY}"),
                    "{file}: {name} is listed as keeping its dismissed reviews ({why}), \
                     and it excludes them. One of the two is out of date."
                );
                exempted += 1;
                continue;
            }

            checked += 1;
            assert!(
                block.contains("{IN_PLAY}") || block.contains("dismissed_at"),
                "{file}: {name} lists reviews and says nothing about dismissed ones. \
                 Paste `AND {{IN_PLAY}}` into its query, or name `dismissed_at` to say \
                 what it does instead — or, if it keeps them on purpose, add it to \
                 EXEMPT in this test with the reason, and argue it where the query is."
            );
        }
    }

    assert!(
        checked >= 7,
        "only {checked} lists of reviews were found, so this check is vacuous"
    );
    assert_eq!(
        exempted,
        EXEMPT.len(),
        "every exemption must name a function that is still there and still a list"
    );
}

#[test]
fn deleting_the_newest_round_makes_the_round_before_it_the_newest_again() {
    // The other direction of `dismissing_the_newest_round_resurrects_no_earlier_round`,
    // and the answer is the opposite — correctly. A dismissal leaves the round on the
    // record, so the round before it stays superseded by something that still exists; a
    // delete removes it, so the pull request's newest round really is the earlier one
    // again. Nothing was lost either way: only a round nobody started can be deleted.
    let (_dir, store) = temp_store();
    let round_one = store.open_review(REPO, 810, "aaa1111").expect("round 1");
    store
        .add_finding(round_one.review.id, a_finding("round one was reviewed"))
        .expect("a finding");
    store
        .set_review_links(
            round_one.review.id,
            ReviewLinks {
                github_review_id: Some("gh-review-810".to_string()),
                ..Default::default()
            },
        )
        .expect("round 1 posted, so the next commit is a round of its own");
    let round_two = store.open_review(REPO, 810, "bbb2222").expect("round 2");
    assert_eq!(work_ids(&store), vec![round_two.review.id]);

    store
        .delete_review(round_two.review.id)
        .expect("round 2 never started, so it can go");

    assert_eq!(
        store.latest_round(REPO, 810).unwrap(),
        Some(1),
        "the pull request is back on round 1"
    );
    assert_eq!(
        work_ids(&store),
        vec![round_one.review.id],
        "and round 1 is the live round again — posted, so it is the harvest's turn"
    );
    assert_eq!(
        store
            .open_review(REPO, 810, "ccc3333")
            .unwrap()
            .review
            .round,
        2,
        "so the next commit is round 2 once more, rather than a round 3 with a gap"
    );
}

#[test]
fn a_request_a_session_has_picked_up_moves_under_it_and_the_record_says_so() {
    // **DEV-945 refused this move; DEV-962 makes it.** The refusal bought a second
    // unposted round of the same pull request — two rows for one request, which is the
    // fault this reversal was raised from — so what protects the session now is the record
    // of the move rather than the refusal of it: `head_updates` says which commit the round
    // was of, the page says it moved, and the session re-reads the delta.
    //
    // What the linked session still buys is `review_started`, which is the fact a
    // dispatcher outside the record cannot otherwise see.
    let (_dir, store) = temp_store();

    let linked = store.open_review(REPO, 820, "aaa1111").expect("a request");
    store
        .set_review_links(
            linked.review.id,
            ReviewLinks {
                session_id: Some("allele-ae03f8ce".to_string()),
                ..Default::default()
            },
        )
        .expect("a session picked it up");

    assert!(
        store.review_started(linked.review.id).unwrap(),
        "a linked session is somebody at work, which is what a caller reads"
    );

    let next = store
        .open_review(REPO, 820, "bbb2222")
        .expect("the author pushed");
    assert!(!next.created, "one request, one review");
    assert_eq!(next.review.id, linked.review.id);
    assert_eq!(next.review.round, 1);
    assert_eq!(next.action, OpenedAction::HeadMoved);
    assert_eq!(next.head_moved_from.as_deref(), Some("aaa1111"));

    let updates = store.head_updates(linked.review.id).unwrap();
    assert_eq!(updates.moved, 1);
    assert_eq!(
        updates.since_sha.as_deref(),
        Some("aaa1111"),
        "so the session can be told what changed since the commit it read"
    );
}

#[test]
fn a_request_whose_lint_has_run_has_started_and_still_moves() {
    // The other sign of a session at work, for a session that writes its layers before it
    // links itself: a layer moved off `pending`. It makes `review_started` true and, since
    // DEV-962, stops nothing.
    let (_dir, store) = temp_store();

    let started = store.open_review(REPO, 821, "aaa1111").expect("a request");
    store
        .set_review_layer(
            started.review.id,
            1,
            ReviewLayerState::Done,
            Some("the lint passed"),
        )
        .expect("layer 1 ran");

    assert!(store.review_started(started.review.id).unwrap());

    let next = store
        .open_review(REPO, 821, "bbb2222")
        .expect("the author pushed");
    assert!(!next.created);
    assert_eq!(next.review.round, 1);
    assert_eq!(next.action, OpenedAction::HeadMoved);
}

#[test]
fn a_request_nobody_has_picked_up_still_moves() {
    // The case the rule is for, asserted beside the two it is not: the dispatcher's own
    // row, with a ledger id and nothing else, moves to the commit now in front of it.
    let (_dir, store) = temp_store();

    let recorded = store.open_review(REPO, 822, "aaa1111").expect("a request");
    store
        .set_review_links(
            recorded.review.id,
            ReviewLinks {
                ledger_id: Some("ledger-9f2".to_string()),
                ..Default::default()
            },
        )
        .expect("the dispatcher's own entry, which is not a session");

    let again = store
        .open_review(REPO, 822, "bbb2222")
        .expect("recorded again");
    assert!(!again.created, "one request, one review");
    assert_eq!(again.head_moved_from.as_deref(), Some("aaa1111"));
    assert_eq!(again.review.ledger_id.as_deref(), Some("ledger-9f2"));
}

#[test]
fn a_review_a_session_picked_up_but_wrote_nothing_to_can_still_be_deleted() {
    // **The two questions are not the same question**, and this is the half that proves
    // it: `delete_review` asks what the review holds — DEV-945's three things — and a
    // session id is not one of them. The stray review this issue was raised from was
    // written by a build session, so it may well carry one, and refusing to delete it for
    // that reason would refuse the one case the delete exists for.
    let (_dir, store) = temp_store();
    let review = requested(&store, 823, "aaa1111");
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                session_id: Some("allele-ae03f8ce".to_string()),
                ..Default::default()
            },
        )
        .expect("a session had it");
    store
        .set_review_layer(
            review.id,
            1,
            ReviewLayerState::Done,
            Some("the lint passed"),
        )
        .expect("and ran the lint");

    store
        .delete_review(review.id)
        .expect("it holds no brief, no finding and no draft");
    assert!(store.find_review_by_id(review.id).unwrap().is_none());
}

#[test]
fn a_session_that_says_who_it_is_on_the_first_call_has_started_at_once() {
    // `review open --session` is one call, and what it buys is that the record knows whose
    // review it is from that call rather than from whenever the session gets to
    // `review link` — which is what `review find`'s `started` reports to a dispatcher that
    // cannot see inside the record. DEV-954 asks for exactly that.
    //
    // **What it no longer buys is a pinned head**, which is DEV-962's reversal: the push
    // below moves the round it is already on.
    let (_dir, store) = temp_store();
    let recorded = store.open_review(REPO, 824, "aaa1111").expect("a request");

    // The command line's `--session` is `open_review_with` followed by `set_review_links`
    // — the same two calls, in one command — so the store's half is this.
    store
        .set_review_links(
            recorded.review.id,
            ReviewLinks {
                session_id: Some("allele-ae03f8ce".to_string()),
                ..Default::default()
            },
        )
        .expect("the session says who it is");

    assert!(
        store.review_started(recorded.review.id).unwrap(),
        "claimed by the first call, before it has written anything else"
    );

    let pushed = store
        .open_review(REPO, 824, "bbb2222")
        .expect("the author pushed");
    assert!(!pushed.created, "the same round, moved");
    assert_eq!(pushed.action, OpenedAction::HeadMoved);
    assert_eq!(pushed.review.id, recorded.review.id);
}

#[test]
fn a_review_posted_outside_review_desk_is_not_a_review_that_never_started() {
    // **The hole a review of this branch found.** `review link --github-review` is the
    // one way the command line can post a review, and a review posted that way holds no
    // brief, no finding and no draft — the process ran outside Review Desk, which
    // `posted_reviews_awaiting_author` admits by name and the Reviews list draws as
    // "posted". Counting only the three would have let a delete remove the record that a
    // review was posted on that pull request at all, which is the opposite of the row
    // that should never have existed.
    let (_dir, store) = temp_store();
    let review = requested(&store, 830, "aaa1111");
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                github_review_id: Some("PRR_kwDOseeded".to_string()),
                ..Default::default()
            },
        )
        .expect("posted, the one way the command line can post one");

    let work = store.review_work(review.id).unwrap();
    assert!(work.started());
    assert_eq!(work.in_words(), "the review it posted on GitHub");

    let error = store
        .delete_review(review.id)
        .expect_err("deleting it would lose the record that it was posted");
    assert_eq!(error.code(), "review_started");
    assert!(error.to_string().contains("posted on GitHub"), "{error}");

    // And its head does not move either: the commit a posted review is of is the commit
    // that was reviewed.
    let pushed = store
        .open_review(REPO, 830, "bbb2222")
        .expect("the author pushed");
    assert!(pushed.created, "a round of its own");
    assert_eq!(
        store.get_review(review.id).unwrap().head_sha,
        "aaa1111",
        "and the posted round still names the commit it was posted on"
    );
}

#[test]
fn a_harvest_having_looked_is_not_work_a_delete_would_lose() {
    // The other half of the same question, and the answer is the opposite. The harvest
    // sweep reads every review whose pull request is open — including one nobody has
    // started — and marks it read, so a `harvested_at` says Review Desk looked at GitHub
    // rather than that anybody did anything. The stray review this issue was raised from
    // will have been swept.
    let (_dir, store) = temp_store();
    let review = requested(&store, 831, "aaa1111");
    store
        .mark_review_harvested(review.id, 1_791_000_000)
        .expect("the sweep looked at it");

    assert!(!store.review_work(review.id).unwrap().started());
    store.delete_review(review.id).expect("and it can still go");
}

#[test]
fn a_moved_head_takes_the_line_count_with_it() {
    // The figure is additions plus deletions of the change as it stood, and the draft's
    // word budget is computed from it — so a count measured on the commit this review
    // has just stopped being of is a wrong answer rather than an old one.
    let (_dir, store) = temp_store();
    store
        .open_review_with(
            REPO,
            832,
            "aaa1111",
            &ReviewFacts {
                title: Some("Give a scheduled export its own context".to_string()),
                changed_lines: Some(50),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("a request, with what the dispatcher knew");

    let moved = store
        .open_review(REPO, 832, "bbb2222")
        .expect("the author pushed");

    assert_eq!(
        moved.review.changed_lines, None,
        "the count went with the commit"
    );
    assert_eq!(
        moved.review.title.as_deref(),
        Some("Give a scheduled export its own context"),
        "the title is about the pull request, not the commit, so it stays"
    );

    // And the next caller that knows writes it.
    let told = store
        .open_review_with(
            REPO,
            832,
            "bbb2222",
            &ReviewFacts {
                changed_lines: Some(3_000),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("recorded again, with the new count");
    assert_eq!(told.review.changed_lines, Some(3_000));
}

#[test]
fn a_change_map_or_a_commit_list_is_a_session_at_work_too() {
    // **The hole the fifth review pass of DEV-945 found, and what DEV-962 does with it.**
    // `provenance changes` and `round commits` need nothing but the review row — no brief,
    // no finding, no draft, no session id, no layer past `pending` — so a session that
    // records them before linking itself has none of the other signs. They still make
    // `review_started` true, which is what a caller outside the record reads.
    //
    // The head now moves anyway, and the change map is left exactly as it was: it is a
    // measurement of the commit it was made on, it is what the session is building, and
    // clearing it would destroy the work in order to protect it from being out of date.
    // `changed_lines` is the one thing cleared, for `open_review`'s stated reason — a bare
    // number nothing can tell is stale, which the next ordinary call refills.
    let (_dir, store) = temp_store();
    let mapped = store.open_review(REPO, 840, "aaa1111").expect("a request");
    store
        .put_file_changes(
            mapped.review.id,
            vec![NewFileChange {
                path: "app/Reporting/ColumnPolicy.php".to_string(),
                summary: "the column policy".to_string(),
                lines_added: 12,
                lines_removed: 3,
                checked: FileCheck::ReadInFull,
                how_checked: None,
            }],
        )
        .expect("a session mapped the change");

    assert!(store.review_started(mapped.review.id).unwrap());

    let pushed = store
        .open_review(REPO, 840, "bbb2222")
        .expect("the author pushed");
    assert_eq!(
        pushed.action,
        OpenedAction::HeadMoved,
        "the same round, moved"
    );
    assert_eq!(
        store.get_review(mapped.review.id).unwrap().head_sha,
        "bbb2222"
    );
    assert_eq!(
        store
            .review_provenance(mapped.review.id)
            .unwrap()
            .files
            .len(),
        1,
        "and the change map is still there, to be rewritten when the session re-reads"
    );

    // And a session that recorded what it did not look at is at work as well.
    let noted = store
        .open_review_with(
            REPO,
            841,
            "aaa1111",
            &ReviewFacts {
                not_checked: Some("the dashboard against the real API".to_string()),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("a request, with what a session did not check");
    assert!(store.review_started(noted.review.id).unwrap());
    assert_eq!(
        store
            .open_review(REPO, 841, "bbb2222")
            .expect("the author pushed")
            .action,
        OpenedAction::HeadMoved
    );
}

#[test]
fn a_review_holding_only_a_change_map_can_still_be_deleted() {
    // The other half, and the reason `claimed` and `held` are two questions: a per-file
    // change map stops a head moving and is not work a delete would lose. The stray
    // review this issue was raised from may carry provenance from the session that wrote
    // it by accident.
    let (_dir, store) = temp_store();
    let review = requested(&store, 842, "aaa1111");
    store
        .put_review_sources(
            review.id,
            vec![NewReviewSource {
                kind: SourceKind::Description,
                label: "The pull request description".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .expect("a source");

    assert!(!store.review_work(review.id).unwrap().started());
    store.delete_review(review.id).expect("and it can still go");
}

#[test]
fn an_id_from_an_unset_shell_variable_does_not_claim_a_review() {
    // **`--session "$SESSION_ID"` with the variable unset is `Some("")`**, and `COALESCE`
    // is only told to leave a column alone by a `NULL` — the fault `normalise::
    // optional_fact` exists for, which `set_review_links` was not reading through. Since
    // DEV-945 it is worse than a wiped id: an empty session id is `is_some()`, so the
    // review reads as one somebody has picked up and its head never moves again, which
    // is the two-unstarted-reviews state the whole rule exists to prevent. Reproduced by
    // a review of this branch.
    let (_dir, store) = temp_store();
    let recorded = store.open_review(REPO, 850, "aaa1111").expect("a request");
    store
        .set_review_links(
            recorded.review.id,
            ReviewLinks {
                session_id: Some(String::new()),
                ledger_id: Some("   ".to_string()),
                ..Default::default()
            },
        )
        .expect("a shell that said nothing");

    let review = store.get_review(recorded.review.id).unwrap();
    assert_eq!(review.session_id, None, "an empty id is no id");
    assert_eq!(review.ledger_id, None);

    let again = store
        .open_review(REPO, 850, "bbb2222")
        .expect("the author pushed");
    assert!(!again.created, "still one request, one review");
    assert_eq!(again.head_moved_from.as_deref(), Some("aaa1111"));
}

#[test]
fn an_empty_github_review_id_does_not_make_a_review_posted() {
    // The same rule on the column `ReviewWork` reads: an empty string is `IS NOT NULL`,
    // so without normalising it the review would hold "the review it posted on GitHub"
    // and could never be deleted.
    let (_dir, store) = temp_store();
    let review = requested(&store, 851, "aaa1111");
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                github_review_id: Some(String::new()),
                ..Default::default()
            },
        )
        .expect("a shell that said nothing");

    assert!(!store.review_work(review.id).unwrap().started());
    store.delete_review(review.id).expect("and it can still go");
}

#[test]
fn an_address_may_follow_the_id_onto_a_request_whose_head_moved() {
    // **The pre-check has to ask about the review the write will land on.** It looked the
    // review up by the new commit, which names nothing when the head is about to move —
    // so it was handed "this review holds no ticket id" about a review that holds one,
    // and refused the address. Reproduced by a review of this branch: record the id, let
    // the author push, record the address.
    let (_dir, store) = temp_store();
    store
        .open_review_with(
            REPO,
            852,
            "aaa1111",
            &ReviewFacts {
                ticket_id: Some("ABC-402".to_string()),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("the id, on the first call");

    let moved = store
        .open_review_with(
            REPO,
            852,
            "bbb2222",
            &ReviewFacts {
                ticket_url: Some("https://linear.app/acme/issue/ABC-402".to_string()),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("the address, on the next call, after a push");

    assert_eq!(moved.head_moved_from.as_deref(), Some("aaa1111"));
    assert_eq!(moved.review.ticket_id.as_deref(), Some("ABC-402"));
    assert_eq!(
        moved.review.ticket_url.as_deref(),
        Some("https://linear.app/acme/issue/ABC-402")
    );
}

#[test]
fn the_refusal_says_them_about_several_things_and_that_about_one() {
    // **The message, not only the page.** The card hides the delete, so this sentence is
    // what a developer reads on the race the hidden button cannot cover — a finding added
    // between the page's read and the `DELETE` — and `contract/README.md` says the two
    // refuse in the same words. It said "holds 2 findings, and deleting it would lose
    // that".
    let (_dir, store) = temp_store();

    let one = requested(&store, 860, "aaa1111");
    store
        .put_brief(
            one.id,
            NewBrief {
                problem: "the problem".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief, and nothing else");
    let said = store
        .delete_review(one.id)
        .expect_err("refused")
        .to_string();
    assert!(
        said.contains("holds a brief, and deleting it would lose that"),
        "{said}"
    );

    let several = requested(&store, 861, "aaa1111");
    for claim in ["one", "two"] {
        store
            .add_finding(several.id, a_finding(claim))
            .expect("a finding");
    }
    let said = store
        .delete_review(several.id)
        .expect_err("refused")
        .to_string();
    assert!(
        said.contains("holds 2 findings, and deleting it would lose them"),
        "{said}"
    );
}

#[test]
fn a_dismissed_review_refuses_the_acts_that_need_an_agent_afterwards() {
    // **Four acts, and each is an item in `work list` — the list a dismissed review is
    // not in.** Performing one would be the developer signing something off that nothing
    // will ever act on, while the dashboard tells them the review session posts it. A
    // review of this branch drove exactly that: dismiss a review, approve its draft, and
    // the draft is approved for ever while `work list` says nothing is waiting.
    let (_dir, store) = temp_store();
    let review = requested(&store, 870, "aaa1111");
    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "the problem".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");
    store
        .add_finding(review.id, a_finding("a nit"))
        .expect("a finding");
    store
        .set_finding_disposition(
            review.id,
            1,
            Disposition::HeldBack,
            Some("a nit".to_string()),
        )
        .expect("held back");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: "A review body.".to_string(),
                status: DraftStatus::Ready,
                lint: None,
                sections: None,
            },
        )
        .expect("a draft");
    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("dismiss it");

    for (what, error) in [
        (
            "confirming the brief",
            store.confirm_brief(review.id, Actor::Developer).err(),
        ),
        (
            "approving the draft",
            store
                .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
                .err(),
        ),
        (
            "sending it back",
            store.send_back_draft(review.id, "cut the nits").err(),
        ),
        (
            "asking for the ticket",
            store.request_nit_ticket(review.id, None).err(),
        ),
    ] {
        let error = error.unwrap_or_else(|| panic!("{what} should be refused"));
        assert!(error.is_refusal(), "{what}");
        assert_eq!(error.code(), "dismissed_review", "{what}");
        assert!(
            error.to_string().contains("bring it back first"),
            "{what}: {error}"
        );
    }

    // **And the record is not frozen.** Everything that changes a review without needing
    // an agent still works, which is what "every record under it is kept" means.
    store
        .set_finding_disposition(review.id, 1, Disposition::InDraft, None)
        .expect("a disposition is the developer's own, and needs nobody after it");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: "A rewritten body.".to_string(),
                status: DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .expect("and a session may still write to it");

    // Restored, the four are offered again.
    store.restore_review(review.id).expect("restore it");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("confirming is the developer's again");
}

#[test]
fn linking_nothing_does_not_move_the_last_activity_time() {
    // `last_activity_at` is the one thing the queue has to say whether a session is still
    // alive, and `review open --session "$SESSION_ID"` from a shell whose variable is
    // unset reaches here with nothing to write. Marking the review active anyway would
    // make a dead session read as freshly alive. `set_review_facts` has had this rule
    // since a `--title ""` wiped a title; this is the same rule on the same row.
    let (_dir, store) = temp_store();
    let review = requested(&store, 871, "aaa1111");
    let before = store.get_review(review.id).unwrap().last_activity_at;

    store
        .set_review_links(
            review.id,
            ReviewLinks {
                session_id: Some("  ".to_string()),
                ..Default::default()
            },
        )
        .expect("a shell that said nothing");

    assert_eq!(
        store.get_review(review.id).unwrap().last_activity_at,
        before,
        "a call with nothing to write writes nothing, the activity time included"
    );
}

#[test]
fn a_blank_head_commit_is_refused_rather_than_wiping_the_one_on_the_record() {
    // **The move is an `UPDATE` and went nowhere near `validate`**, which has refused a
    // blank head since the first slice. So `review open --head "$HEAD"` from a shell
    // whose variable is unset wiped the commit the review was of — on a request nobody
    // had started, which is the row this whole feature is built for — and left a
    // `head_sha` the contract's own pattern refuses. Driven against the binary by a
    // review of this branch.
    let (_dir, store) = temp_store();
    let recorded = store.open_review(REPO, 880, "4f1a9c2").expect("a request");

    for blank in ["", "   "] {
        let error = store
            .open_review(REPO, 880, blank)
            .expect_err("a blank head is not a commit");
        assert!(error.is_refusal());
        assert_eq!(error.code(), "invalid_value");
    }

    assert_eq!(
        store.get_review(recorded.review.id).unwrap().head_sha,
        "4f1a9c2",
        "and the commit the review is of is still there"
    );
}

#[test]
fn a_session_may_report_the_ticket_it_filed_for_a_dismissed_review() {
    // **The ask and the report are two roles of one function**, and only the ask needs an
    // agent afterwards. Refusing the report would lose the id: the ticket exists in the
    // tracker, and the record would never learn which one.
    let (_dir, store) = temp_store();
    let review = requested(&store, 881, "aaa1111");
    store
        .add_finding(review.id, a_finding("a nit"))
        .expect("a finding");
    store
        .set_finding_disposition(
            review.id,
            1,
            Disposition::HeldBack,
            Some("a nit".to_string()),
        )
        .expect("held back");
    store
        .request_nit_ticket(review.id, None)
        .expect("the developer asked for one");
    store
        .dismiss_review(review.id, "the pull request was closed unmerged")
        .expect("and then dismissed the review");

    let filed = store
        .request_nit_ticket(review.id, Some("ABC-500"))
        .expect("the session says which ticket it filed");
    assert_eq!(filed.ticket_id.as_deref(), Some("ABC-500"));
    assert_eq!(filed.status, NitTicketStatus::Filed);

    // And the developer's own ask is still refused on it.
    assert_eq!(
        store
            .request_nit_ticket(review.id, None)
            .expect_err("nothing would file a second one")
            .code(),
        "dismissed_review"
    );
}
