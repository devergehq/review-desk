//! The draft, and the state machine that is the sign-off boundary.
//!
//! Every refusal here is load-bearing. **Only the developer signs off**, decided on
//! 9 October 2026, and the only thing that makes that true of the record rather than of
//! the agents using it is that `put_draft` cannot write `approved` and `post_draft`
//! will not accept a draft that is not approved.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{Actor, DraftLint, DraftStatus, NewDraft, ReviewEvent, Store, StoreError};

/// A review with a draft at `ready`, which is where the developer picks it up.
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
fn a_draft_is_stored_with_its_lint_and_read_back() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    let stored = store
        .put_draft(
            review.id,
            NewDraft {
                body: "## Blocker · F1\n\nthe body, verbatim".to_string(),
                status: DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(118),
                    word_budget: Some(150),
                    findings_in_body: Some(3),
                    findings_expected: Some(3),
                    raised_before_on_pr: Some(0),
                    errors: None,
                    checked_at: Some(1_760_000_000),
                }),
                sections: None,
            },
        )
        .expect("put draft");

    assert_eq!(stored, store.get_draft(review.id).unwrap());
    assert_eq!(
        stored.body, "## Blocker · F1\n\nthe body, verbatim\n",
        "the body is what the blocks compose to, and a composition ends in one newline"
    );
    assert_eq!(stored.status, DraftStatus::Ready);
    assert_eq!(stored.event, None, "nothing is chosen until approval");
    assert_eq!(stored.approved_at, None);
    assert_eq!(stored.approved_by, None);
    assert_eq!(stored.lint.passes, Some(true));
    assert_eq!(stored.lint.visible_words, Some(118));
    assert_eq!(stored.lint.word_budget, Some(150));
    assert_eq!(stored.lint.checked_at, Some(1_760_000_000));
}

#[test]
fn a_lint_that_reported_something_is_stamped_with_when_it_ran() {
    // A lint result with no time cannot be told from one that is a week old.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    let stored = store
        .put_draft(
            review.id,
            NewDraft {
                body: "b".to_string(),
                status: DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(false),
                    errors: Some("the description is 2.3× its budget".to_string()),
                    ..DraftLint::default()
                }),
                sections: None,
            },
        )
        .unwrap();

    assert!(stored.lint.checked_at.is_some());
    assert_eq!(stored.lint.passes, Some(false));

    // A draft written with no lint at all is not stamped: nothing ran.
    let other = store
        .create_review(new_review("acme/portal", 413, "aaaaaaa", 1))
        .unwrap();
    let unlinted = store.put_draft(other.id, NewDraft::ready("b")).unwrap();
    assert!(unlinted.lint.is_empty());
    assert_eq!(unlinted.lint.checked_at, None);
}

#[test]
fn a_draft_can_be_written_while_drafting_and_then_made_ready() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    let drafting = store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .expect("a session may save work in progress");
    assert_eq!(drafting.status, DraftStatus::Drafting);
    assert_eq!(drafting.body, "", "an empty body is allowed while drafting");

    let ready = store
        .put_draft(review.id, NewDraft::ready("finished"))
        .unwrap();
    assert_eq!(ready.status, DraftStatus::Ready);
    assert_eq!(ready.body, "finished\n");
}

#[test]
fn putting_a_draft_cannot_approve_it() {
    // The whole of the sign-off boundary. If this passes, a caller that can write a
    // draft can sign it off, and the decision is a convention rather than a rule.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    for status in [
        DraftStatus::Approved,
        DraftStatus::Posted,
        DraftStatus::SentBack,
    ] {
        match store.put_draft(
            review.id,
            NewDraft {
                body: "b".to_string(),
                status,
                lint: None,
                sections: None,
            },
        ) {
            Err(StoreError::InvalidValue {
                field, expected, ..
            }) => {
                assert_eq!(field, "status");
                assert!(
                    expected.contains("drafting") && expected.contains("ready"),
                    "the refusal names what it would have accepted: {expected}"
                );
                assert!(
                    expected.contains("dashboard"),
                    "and says where approving happens: {expected}"
                );
            }
            other => panic!("writing {status} must be refused, got {other:?}"),
        }
    }
}

#[test]
fn an_approved_draft_cannot_be_rewritten() {
    // An approved draft is what the review session reads and posts. Editing it
    // underneath would change what gets posted after it was approved.
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .unwrap();

    match store.put_draft(review, NewDraft::ready("something else")) {
        Err(StoreError::DraftStatus { found, wanted, .. }) => {
            assert_eq!(found, DraftStatus::Approved);
            assert!(wanted.contains("rewritten"), "{wanted}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }

    assert_eq!(
        store.get_draft(review).unwrap().body,
        "the review body\n",
        "the body that was approved is the body that is held"
    );
}

#[test]
fn approving_requires_the_developer_and_records_them() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);

    for by in [Actor::Author, Actor::Harvester] {
        match store.approve_draft(review, ReviewEvent::Comment, by) {
            Err(StoreError::InvalidValue {
                field, expected, ..
            }) => {
                assert_eq!(field, "by");
                assert!(expected.contains("developer"), "{expected}");
            }
            other => panic!("{by} must not be able to approve a draft, got {other:?}"),
        }
    }

    let approved = store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer approves");
    assert_eq!(approved.status, DraftStatus::Approved);
    assert_eq!(approved.event, Some(ReviewEvent::RequestChanges));
    assert_eq!(approved.approved_by, Some(Actor::Developer));
    assert!(approved.approved_at.is_some());
    assert_eq!(approved.posted_at, None, "approving posts nothing");
}

#[test]
fn approving_twice_is_one_approval_and_changing_the_event_is_refused() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);

    let first = store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer)
        .unwrap();
    let again = store
        .approve_draft(review, ReviewEvent::Approve, Actor::Developer)
        .expect("a retried call is safe");
    assert_eq!(first, again, "the first approval's time is kept");

    match store.approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer) {
        Err(StoreError::DraftStatus { found, .. }) => assert_eq!(found, DraftStatus::Approved),
        other => panic!("a different event would change what gets posted: {other:?}"),
    }
}

#[test]
fn a_draft_can_only_be_approved_from_ready() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    store
        .put_draft(
            review.id,
            NewDraft {
                body: "half written".to_string(),
                status: DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .unwrap();

    match store.approve_draft(review.id, ReviewEvent::Comment, Actor::Developer) {
        Err(StoreError::DraftStatus { found, wanted, .. }) => {
            assert_eq!(found, DraftStatus::Drafting);
            assert!(wanted.contains("ready"), "{wanted}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_draft_is_sent_back_with_a_note_and_can_be_reworked() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);

    let sent_back = store
        .send_back_draft(review, "  F2 is the option map talking, not the diff.  ")
        .expect("send back");
    assert_eq!(sent_back.status, DraftStatus::SentBack);
    assert_eq!(
        sent_back.sent_back_note.as_deref(),
        Some("F2 is the option map talking, not the diff."),
        "the note is trimmed, not padded"
    );
    assert!(sent_back.sent_back_at.is_some());

    let reworked = store
        .put_draft(review, NewDraft::ready("reworked body"))
        .expect("a sent-back draft is rewritten to get back to ready");
    assert_eq!(reworked.status, DraftStatus::Ready);
    assert_eq!(
        reworked.sent_back_note.as_deref(),
        Some("F2 is the option map talking, not the diff."),
        "what the developer asked for stays on the record once it has been done"
    );
}

#[test]
fn sending_a_draft_back_with_no_note_is_refused() {
    // A bare rejection teaches the session nothing, and the record exists to be
    // learned from.
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);

    match store.send_back_draft(review, "   ") {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "sent_back_note"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(store.get_draft(review).unwrap().status, DraftStatus::Ready);
}

#[test]
fn a_draft_cannot_be_posted_unless_it_is_approved() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);

    match store.post_draft(review, "gh-review-3312") {
        Err(StoreError::DraftStatus { found, wanted, .. }) => {
            assert_eq!(found, DraftStatus::Ready);
            assert!(wanted.contains("approved"), "{wanted}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }

    assert_eq!(
        store.get_review(review).unwrap().github_review_id,
        None,
        "a refused posting records no GitHub review either"
    );
}

#[test]
fn posting_records_the_github_review_and_the_time_together() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .unwrap();

    let posted = store
        .post_draft(review, "  gh-review-3312  ")
        .expect("the session reports what it posted");

    assert_eq!(posted.status, DraftStatus::Posted);
    assert!(posted.posted_at.is_some());
    assert_eq!(
        store
            .get_review(review)
            .unwrap()
            .github_review_id
            .as_deref(),
        Some("gh-review-3312"),
        "the draft's posting and the review's GitHub id are one fact"
    );

    let again = store
        .post_draft(review, "gh-review-3312")
        .expect("reporting the same posting twice changes nothing");
    assert_eq!(posted, again);

    match store.post_draft(review, "gh-review-9999") {
        Err(StoreError::DraftStatus { found, .. }) => assert_eq!(found, DraftStatus::Posted),
        other => panic!("one draft is posted once: {other:?}"),
    }
}

#[test]
fn an_empty_github_review_id_is_refused() {
    let (_dir, store) = temp_store();
    let review = review_with_a_ready_draft(&store);
    store
        .approve_draft(review, ReviewEvent::Comment, Actor::Developer)
        .unwrap();

    match store.post_draft(review, "  ") {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "github_review_id"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_review_with_no_draft_has_none_and_an_unknown_review_cannot_have_one() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    assert_eq!(store.find_draft(review.id).unwrap(), None);
    match store.get_draft(review.id) {
        Err(StoreError::NotFound { entity, .. }) => assert_eq!(entity, "draft"),
        other => panic!("expected NotFound, got {other:?}"),
    }

    match store.put_draft(999_999, NewDraft::ready("b")) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!((entity, parent), ("draft", "review"));
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn rewriting_a_draft_clears_a_lint_that_described_the_old_body() {
    // The lint is a statement about a particular body. Keeping it across a rewrite
    // would attribute "passes, 118 of 150 words" to text it never saw, and the
    // contract says the lint is re-run on every save.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    store
        .put_draft(
            review.id,
            NewDraft {
                body: "the first body".to_string(),
                status: DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(118),
                    ..DraftLint::default()
                }),
                sections: None,
            },
        )
        .unwrap();

    let rewritten = store
        .put_draft(review.id, NewDraft::ready("a different body"))
        .unwrap();

    assert!(
        rewritten.lint.is_empty(),
        "a body saved with no lint has not been linted: {:?}",
        rewritten.lint
    );
}
