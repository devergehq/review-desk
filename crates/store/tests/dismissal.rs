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
    Actor, DismissedReviews, Disposition, DraftStatus, FileCheck, FindingRef, NewBrief, NewDraft,
    NewFileChange, NewFinding, NewOutcome, NewReviewSource, OutcomeKind, Review, ReviewLayerState,
    ReviewLinks, Severity, SourceKind, Store, WorkFilter,
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
fn listed(store: &Store, dismissed: DismissedReviews) -> Vec<i64> {
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
fn a_dismissed_reviews_filed_nit_ticket_is_not_in_the_queue() {
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
    assert!(
        store.filed_nit_tickets(None, None).unwrap().is_empty(),
        "the queue's \"handled without you\" does not count a dismissed review's ticket"
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
        listed(&store, DismissedReviews::default()),
        vec![in_play.id],
        "the default view is the reviews in play"
    );
    assert_eq!(
        listed(&store, DismissedReviews::Only),
        vec![gone.id],
        "and the filter is the dismissed ones on their own"
    );
    let both = listed(&store, DismissedReviews::Included);
    assert_eq!(both.len(), 2, "or the whole record on one page: {both:?}");

    let row = store
        .reviews_at_step(None, DismissedReviews::Only)
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
fn what_a_review_holds_is_the_same_question_for_a_delete_and_for_a_moved_head() {
    // One predicate, two callers. If they disagreed, the page would offer a delete for a
    // review the store treats as started, or `open_review` would move a round somebody
    // had begun.
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
    assert_eq!(
        store
            .open_review(REPO, 805, "bbb2222")
            .unwrap()
            .review
            .round,
        2,
        "and a new commit on it is the next round rather than this round moved"
    );
}

#[test]
fn every_query_that_lists_reviews_says_what_it_does_about_dismissed_ones() {
    // **The structural half of "a dismissed review is in front of nobody".** The tests
    // above check the queries that exist today; this one is against the next one, written
    // by somebody who does not know there is a clause to paste. A list that forgot it
    // would put a dismissed review back in the developer's queue and nothing anywhere
    // would fail.
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
            "finished_review_timings",
            "how long each finished review took; dismissing a review afterwards does not \
             unmeasure the time it took, and dropping it would bias whatever is computed \
             from this read",
        ),
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    let mut exempted = 0;

    for file in [
        "src/entities/inbox.rs",
        "src/entities/work.rs",
        "src/entities/review.rs",
        "src/entities/timings.rs",
    ] {
        let source = std::fs::read_to_string(root.join(file)).expect("the module's source");

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
        "and round 1 is the live round again"
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
fn a_request_a_session_has_picked_up_does_not_move_under_it() {
    // **The window between a session being dispatched and its brief.** It links itself
    // and starts a layer long before it writes a brief, a finding or a draft — so a
    // dispatcher polling in that moment would otherwise move the head under a session
    // that had already read the commit it was moved off, and the brief it then wrote
    // would be of one commit on a review the record said was of another.
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

    let next = store
        .open_review(REPO, 820, "bbb2222")
        .expect("the author pushed");
    assert!(next.created, "the push is a round of its own");
    assert_eq!(next.review.round, 2);
    assert_eq!(next.head_moved_from, None);
    assert_eq!(
        store.get_review(linked.review.id).unwrap().head_sha,
        "aaa1111",
        "and the running session's review is still of the commit it read"
    );
}

#[test]
fn a_request_whose_lint_has_run_does_not_move_under_it_either() {
    // The other sign of a session at work, for a session that writes its layers before it
    // links itself: a layer moved off `pending`.
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

    let next = store
        .open_review(REPO, 821, "bbb2222")
        .expect("the author pushed");
    assert!(next.created);
    assert_eq!(next.review.round, 2);
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
fn a_session_that_says_who_it_is_on_the_first_call_is_claimed_at_once() {
    // `review open --session` is one call, and what it buys is that the review is claimed
    // before anything else can reach it: the session's own first call is what closes the
    // window a dispatcher could move the head in.
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

    let pushed = store
        .open_review(REPO, 824, "bbb2222")
        .expect("the author pushed");
    assert!(
        pushed.created,
        "a round of its own, because somebody is at work on round 1"
    );
}
