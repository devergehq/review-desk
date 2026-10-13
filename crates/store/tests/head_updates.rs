//! A round is a posted review: what a new commit does before a post, and what it records.
//! DEV-962.
//!
//! The claims under test are the issue's acceptance criteria. What is being defended is that
//! one review request is one row — Patrick's queue held a pull request twice, round 1 and
//! round 2 both offering "changes since your last review", over a round 1 he had never
//! posted — and that moving a head is a thing the record can afterwards say happened.

mod common;

use common::{posted, temp_store};
use review_desk_store::{
    Actor, DiffAtHead, Disposition, DraftStatus, HeadUpdateKind, NewDraft, NewFinding,
    NewRoundCommit, OpenedAction, ReviewEvent, ReviewFacts, Severity, Store,
};

const REPO: &str = "acme/portal";
const PR: i64 = 41;

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Should,
        claim: claim.to_string(),
        disposition: Disposition::InDraft,
        ..Default::default()
    }
}

/// A round with a draft the developer has signed off, and the commit it was signed off on.
fn signed_off(store: &Store, head: &str) -> i64 {
    let review = store
        .open_review(REPO, PR, head)
        .expect("the request")
        .review
        .id;
    store
        .add_finding(review, a_finding("the retry has no ceiling"))
        .expect("a finding");
    store
        .put_draft(review, NewDraft::ready("One finding on the retry path.\n"))
        .expect("a draft the session finished");
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer signs it off");
    review
}

#[test]
fn a_new_commit_before_a_post_moves_the_round_and_records_the_move() {
    let (_dir, store) = temp_store();

    let opened = store.open_review(REPO, PR, "4d9464f").expect("round 1");
    let moved = store
        .open_review(REPO, PR, "a41c9e2")
        .expect("the author pushed");

    assert_eq!(moved.review.id, opened.review.id, "one request, one review");
    assert_eq!(moved.review.round, 1, "no round was spent");
    assert_eq!(moved.review.head_sha, "a41c9e2");
    assert_eq!(moved.action, OpenedAction::HeadMoved);
    assert_eq!(moved.head_moved_from.as_deref(), Some("4d9464f"));

    let updates = store.head_updates(opened.review.id).expect("the updates");
    assert_eq!(updates.moved, 1);
    assert_eq!(updates.base_only, 0);
    assert_eq!(updates.since_sha.as_deref(), Some("4d9464f"));
    assert_eq!(updates.updates.len(), 1);

    let update = &updates.updates[0];
    assert_eq!(update.kind, HeadUpdateKind::Moved);
    assert_eq!(update.from_sha, "4d9464f");
    assert_eq!(update.to_sha, "a41c9e2");
    assert!(update.at > 0, "and when it happened");
    assert_eq!(
        updates.last_at,
        Some(update.at),
        "which is what the page dates the sentence from"
    );
}

#[test]
fn a_second_move_still_measures_from_the_commit_the_round_was_opened_on() {
    // "Updated for 3 commits since `4d9464f`" is a sentence about the commit the developer
    // was last shown, not about the one before this push — so the first move's `from_sha` is
    // the answer however many moves follow it.
    let (_dir, store) = temp_store();

    let opened = store.open_review(REPO, PR, "4d9464f").expect("round 1");
    store.open_review(REPO, PR, "a41c9e2").expect("one push");
    store.open_review(REPO, PR, "c7730bd").expect("another");

    let updates = store.head_updates(opened.review.id).expect("the updates");
    assert_eq!(updates.moved, 2);
    assert_eq!(updates.since_sha.as_deref(), Some("4d9464f"));
    assert_eq!(
        store.latest_round(REPO, PR).expect("the rounds"),
        Some(1),
        "and it is still one round"
    );
}

#[test]
fn a_new_commit_after_a_post_opens_the_next_round() {
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, PR, "4d9464f").expect("round 1");
    posted(&store, one.review.id);

    let two = store
        .open_review(REPO, PR, "a41c9e2")
        .expect("a commit after the post");

    assert!(two.created);
    assert_eq!(two.action, OpenedAction::Created);
    assert_eq!(two.review.round, 2);
    assert_eq!(two.head_moved_from, None);
    assert_eq!(
        store.head_updates(one.review.id).expect("round 1").moved,
        0,
        "and round 1's head did not move"
    );
}

#[test]
fn a_draft_reported_posted_is_a_post_as_well_as_a_linked_review() {
    // The two spellings of posted, which `review_is_posted` is the one definition of: a
    // draft the session reported posted, and a review linked with no draft at all.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 42, "4d9464f").expect("round 1");
    store
        .put_draft(one.review.id, NewDraft::ready("One finding.\n"))
        .expect("a draft");
    store
        .approve_draft(one.review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("signed off");
    store
        .post_draft(one.review.id, "PRR_kwDOtheposted")
        .expect("the session posted it");

    assert!(store.review_is_posted(one.review.id).expect("posted?"));
    assert_eq!(
        store
            .open_review(REPO, 42, "a41c9e2")
            .expect("a commit after the post")
            .action,
        OpenedAction::Created
    );
}

#[test]
fn commits_that_only_bring_the_base_branch_in_change_nothing() {
    // DEV-962's third row: a merge of the base branch which leaves the pull request's own
    // diff alone is no update, no new round and no session woken. Asserted as "the review
    // row is the row it was", because that is what "changes nothing" means.
    let (_dir, store) = temp_store();

    let opened = store
        .open_review_with(
            REPO,
            43,
            "4d9464f",
            &ReviewFacts {
                changed_lines: Some(214),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("the request");
    let before = store.get_review(opened.review.id).expect("the row");

    let base = store
        .open_review_with(
            REPO,
            43,
            "9d02e71",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the author merged the base branch in");

    assert_eq!(base.action, OpenedAction::DiffUnchanged);
    assert!(!base.created);
    assert_eq!(base.head_moved_from, None);
    assert_eq!(base.review.id, before.id);

    let after = store.get_review(opened.review.id).expect("the row again");
    assert_eq!(after.head_sha, "4d9464f", "the head did not move");
    assert_eq!(
        after.changed_lines,
        Some(214),
        "and the line count is still right, because the diff it measured did not change"
    );
    assert_eq!(
        after.last_activity_at, before.last_activity_at,
        "nothing was active: no session is woken by this"
    );
    assert_eq!(
        store.latest_round(REPO, 43).expect("the rounds"),
        Some(1),
        "and no round was opened"
    );

    // What *is* recorded is that they arrived, so the page can say they changed nothing.
    let updates = store.head_updates(opened.review.id).expect("the updates");
    assert_eq!(updates.base_only, 1);
    assert_eq!(updates.moved, 0);
    assert_eq!(
        updates.since_sha, None,
        "there is no commit to say it was updated since"
    );
    assert_eq!(updates.updates[0].kind, HeadUpdateKind::BaseOnly);
    assert_eq!(updates.updates[0].from_sha, "4d9464f");
    assert_eq!(updates.updates[0].to_sha, "9d02e71");
}

#[test]
fn a_base_branch_merge_after_a_post_opens_no_round_either() {
    // The same word in both columns of the issue's table: nothing. Without this, a base
    // merge after a post would spend a round on a change nobody made.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 44, "4d9464f").expect("round 1");
    posted(&store, one.review.id);

    let base = store
        .open_review_with(
            REPO,
            44,
            "9d02e71",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the base branch came in");

    assert_eq!(base.action, OpenedAction::DiffUnchanged);
    assert_eq!(base.review.id, one.review.id);
    assert_eq!(store.latest_round(REPO, 44).expect("the rounds"), Some(1));
    assert_eq!(
        store
            .head_updates(one.review.id)
            .expect("updates")
            .base_only,
        1,
        "recorded against the round that posted, which is where it happened"
    );
}

#[test]
fn the_first_review_of_a_pull_request_is_opened_even_when_the_diff_is_unchanged() {
    // There is nothing for the commits to have left unchanged until a review exists, so the
    // claim is meaningless and the round is opened. Stated as a test because the alternative
    // — refusing, or answering `diff_unchanged` with no review — would leave a dispatcher
    // with nothing to work with.
    let (_dir, store) = temp_store();

    let opened = store
        .open_review_with(
            REPO,
            45,
            "4d9464f",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the first open");

    assert!(opened.created);
    assert_eq!(opened.action, OpenedAction::Created);
    assert_eq!(opened.review.round, 1);
}

#[test]
fn opening_the_commit_the_round_is_already_of_changes_nothing_and_says_unchanged() {
    let (_dir, store) = temp_store();

    let opened = store.open_review(REPO, 46, "4d9464f").expect("the request");
    let again = store
        .open_review(REPO, 46, "4d9464f")
        .expect("the same commit again");

    assert_eq!(again.action, OpenedAction::Unchanged);
    assert!(!again.created);
    assert_eq!(again.head_moved_from, None);
    assert_eq!(
        store.head_updates(opened.review.id).expect("updates").moved,
        0,
        "nothing happened, so nothing is recorded as having happened"
    );
}

#[test]
fn a_signed_off_draft_whose_head_moves_is_no_longer_signed_off() {
    // DEV-962's acceptance criterion, and the reason it matters: an approved draft is what a
    // session reads and posts under the developer's name, so a draft still marked approved
    // over a commit they never saw is a review posted in their name about code they did not
    // read.
    let (_dir, store) = temp_store();

    let review = signed_off(&store, "4d9464f");
    let signed = store.get_draft(review).expect("the draft");
    assert_eq!(signed.status, DraftStatus::Approved);

    store
        .open_review(REPO, PR, "a41c9e2")
        .expect("the author pushed");

    let draft = store.get_draft(review).expect("the draft again");
    assert_eq!(
        draft.status,
        DraftStatus::Drafting,
        "the session's again: the code it was written against has changed"
    );
    assert_eq!(draft.approved_at, None);
    assert_eq!(draft.approved_by, None);
    assert_eq!(
        draft.signoff_withdrawn_head.as_deref(),
        Some("4d9464f"),
        "and the page can say why: the developer approved different code"
    );
    assert!(draft.signoff_withdrawn_at.is_some());
    assert_eq!(
        draft.event,
        Some(ReviewEvent::RequestChanges),
        "what it is to be posted as is still true"
    );
    assert_eq!(
        draft.body, signed.body,
        "and the body is untouched, for the session to update"
    );
}

#[test]
fn approving_a_draft_again_clears_the_withdrawal() {
    let (_dir, store) = temp_store();

    let review = signed_off(&store, "4d9464f");
    store.open_review(REPO, PR, "a41c9e2").expect("a push");
    store
        .put_draft(
            review,
            NewDraft::ready("One finding on the retry path, still open.\n"),
        )
        .expect("the session updates it for the new commit");
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("signed off again");

    let draft = store.get_draft(review).expect("the draft");
    assert_eq!(draft.status, DraftStatus::Approved);
    assert_eq!(draft.signoff_withdrawn_at, None);
    assert_eq!(draft.signoff_withdrawn_head, None);
    assert_eq!(
        store.head_updates(review).expect("the updates").moved,
        1,
        "and the move is still on the record, which is where a history belongs"
    );
}

#[test]
fn a_draft_nobody_signed_off_is_left_where_it_is_when_the_head_moves() {
    // Only a sign-off is withdrawn. A draft the session has finished is still the draft it
    // finished, and the page says the head moved rather than the record changing a status
    // nobody asked it to change.
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 47, "4d9464f")
        .expect("the request")
        .review
        .id;
    store
        .put_draft(review, NewDraft::ready("One finding.\n"))
        .expect("a draft waiting on the developer");

    store.open_review(REPO, 47, "a41c9e2").expect("a push");

    let draft = store.get_draft(review).expect("the draft");
    assert_eq!(draft.status, DraftStatus::Ready);
    assert_eq!(draft.signoff_withdrawn_at, None);
}

#[test]
fn a_base_branch_merge_leaves_a_sign_off_alone() {
    let (_dir, store) = temp_store();

    let review = signed_off(&store, "4d9464f");
    store
        .open_review_with(
            REPO,
            PR,
            "9d02e71",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the base branch came in");

    let draft = store.get_draft(review).expect("the draft");
    assert_eq!(
        draft.status,
        DraftStatus::Approved,
        "they approved this diff, and this diff has not changed"
    );
    assert_eq!(draft.signoff_withdrawn_at, None);
}

#[test]
fn the_commits_since_the_round_was_opened_are_counted_from_what_a_session_recorded() {
    // "Updated for 2 commits since `4d9464f`". The count is the round's own commits after
    // the commit it was opened on, because the only record of a pull request's commits is
    // what a session wrote when it last read it — Review Desk reads no GitHub itself.
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 48, "4d9464f")
        .expect("the request")
        .review
        .id;
    store.open_review(REPO, 48, "c7730bd").expect("two pushes");

    assert_eq!(
        store.head_updates(review).expect("updates").commits,
        None,
        "nothing recorded them, which is not the same answer as none"
    );

    store
        .put_round_commits(
            review,
            &[
                NewRoundCommit {
                    sha: "4d9464f".to_string(),
                    summary: "Send an idempotency key with each delivery".to_string(),
                    committed_at: None,
                    files: vec![],
                },
                NewRoundCommit {
                    sha: "a41c9e2".to_string(),
                    summary: "Record a delivery as dead after the last attempt".to_string(),
                    committed_at: None,
                    files: vec![],
                },
                NewRoundCommit {
                    sha: "c7730bd".to_string(),
                    summary: "Cap the backoff".to_string(),
                    committed_at: None,
                    files: vec![],
                },
            ],
        )
        .expect("the session re-read the pull request");

    let updates = store.head_updates(review).expect("updates");
    assert_eq!(
        updates.commits,
        Some(2),
        "the two commits after the one the round was opened on"
    );
    assert_eq!(updates.since_sha.as_deref(), Some("4d9464f"));
}

#[test]
fn a_rounds_updates_are_its_own() {
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 49, "4d9464f").expect("round 1");
    store.open_review(REPO, 49, "a41c9e2").expect("a push");
    posted(&store, one.review.id);
    let two = store.open_review(REPO, 49, "c7730bd").expect("round 2");
    store
        .open_review(REPO, 49, "eeeeeee")
        .expect("another push");

    assert_eq!(store.head_updates(one.review.id).expect("round 1").moved, 1);
    assert_eq!(store.head_updates(two.review.id).expect("round 2").moved, 1);
    assert_eq!(
        store
            .head_updates(two.review.id)
            .expect("round 2")
            .since_sha
            .as_deref(),
        Some("c7730bd"),
        "each round measures from the commit it was opened on"
    );
}

#[test]
fn a_dismissed_round_still_moves_and_stays_dismissed() {
    // DEV-945's decision, kept: the developer said they are not reviewing this pull request,
    // and a push is not them changing their mind. `open_review` carries the argument and the
    // cost of it.
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 50, "4d9464f")
        .expect("the request")
        .review
        .id;
    store
        .dismiss_review(review, "not reviewing this one")
        .expect("dismissed");

    let moved = store.open_review(REPO, 50, "a41c9e2").expect("a push");
    assert_eq!(moved.action, OpenedAction::HeadMoved);
    assert!(moved.review.is_dismissed());
    assert_eq!(store.latest_round(REPO, 50).expect("rounds"), Some(1));
}

#[test]
fn the_round_timeline_says_which_rounds_are_dismissed() {
    // DEV-954: every other field on a round summary is a count, and a dismissed round's
    // counts look exactly like a live one's — so the timeline drew it as live.
    let (_dir, store) = temp_store();

    let one = store.open_review(REPO, 51, "4d9464f").expect("round 1");
    posted(&store, one.review.id);
    let two = store.open_review(REPO, 51, "a41c9e2").expect("round 2");
    store
        .dismiss_review(two.review.id, "superseded, waiting for the rewrite")
        .expect("dismissed");

    let rounds = store.rounds(REPO, 51).expect("the timeline");
    assert_eq!(rounds.len(), 2);
    assert!(!rounds[0].dismissed, "round 1 is in play");
    assert!(rounds[1].dismissed, "round 2 is the dismissed one");
}

#[test]
fn a_moved_head_is_a_write_and_a_base_merge_is_not() {
    // The two sides of "no session woken", in the one column that says whether anything is
    // happening to a review.
    let (_dir, store) = temp_store();

    let opened = store.open_review(REPO, 52, "4d9464f").expect("the request");
    let before = opened.review.last_activity_at;

    let moved = store.open_review(REPO, 52, "a41c9e2").expect("a push");
    assert!(moved.review.last_activity_at >= before);

    let settled = store.get_review(opened.review.id).expect("the row");
    store
        .open_review_with(
            REPO,
            52,
            "9d02e71",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the base branch came in");
    assert_eq!(
        store
            .get_review(opened.review.id)
            .expect("the row again")
            .last_activity_at,
        settled.last_activity_at
    );
}

#[test]
fn the_same_commit_spelled_differently_is_not_a_move() {
    // One commit spelled two ways is one commit, which every lookup in this crate
    // normalises for — and a move from a commit to itself is what the table's own CHECK
    // refuses. So the normalising has to happen before the write, or an ordinary reopen
    // from a shell that upper-cased a sha would leave the page saying the round was updated
    // for nothing.
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 53, "4d9464f")
        .expect("the request")
        .review
        .id;

    let again = store
        .open_review(REPO, 53, " 4D9464F ")
        .expect("the same commit, spelled differently");

    assert_eq!(again.action, OpenedAction::Unchanged);
    assert_eq!(
        store.head_updates(review).expect("updates").updates.len(),
        0,
        "nothing moved, so nothing is recorded as having moved"
    );
}

#[test]
fn the_record_of_the_moves_goes_when_the_round_does() {
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 54, "4d9464f")
        .expect("the request")
        .review
        .id;
    store.open_review(REPO, 54, "a41c9e2").expect("a push");
    assert_eq!(store.head_updates(review).expect("updates").moved, 1);

    store.delete_review(review).expect("it holds nothing");
    assert!(
        store.head_updates(review).is_err(),
        "the updates were the round's, and the round has gone"
    );
}

#[test]
fn whether_anybody_has_begun_is_a_fact_a_caller_can_read() {
    // DEV-954: a worker links itself only once it has begun, so a caller watching from
    // outside had to guess from `session_id`.
    let (_dir, store) = temp_store();

    let review = store
        .open_review(REPO, 55, "4d9464f")
        .expect("a request nobody has picked up")
        .review
        .id;
    assert!(!store.review_started(review).expect("started?"));

    store
        .add_finding(review, a_finding("the retry has no ceiling"))
        .expect("a finding");
    assert!(store.review_started(review).expect("started?"));
}
