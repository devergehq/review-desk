//! A brief that needs no confirmation, and the one thing an agent may not do with it.
//!
//! **This is a hole in the project's load-bearing rule, deliberately cut and deliberately
//! narrow.** Only the developer signs off — Patrick, 9 October 2026 — and
//! `confirmation_required: false` is the single place where an agent gets to decide that a
//! developer step was unnecessary at all. Whether it is decided at all is the caller's,
//! and **it is not a size**: Patrick's ruling is `brief_gate: never|always` in Locus, and
//! Review Desk holds no number either way. An earlier reading of that decision had it as
//! "more than 200 lines"; DEV-882 corrected it everywhere but the shipped SQL of migration
//! 5, which is never edited.
//!
//! So the rule that matters is not that the flag exists. It is that the flag cannot be
//! reached for after the fact: a brief written as requiring confirmation, which the
//! developer may already be looking at, cannot be rewritten as needing none. The first
//! test in this file is that rule, and it was written and watched failing before the rule
//! existed.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{NewBrief, StoreError};

/// A brief with a problem and nothing else.
fn brief(problem: &str) -> NewBrief {
    NewBrief {
        problem: problem.to_string(),
        ..Default::default()
    }
}

#[test]
fn a_brief_written_as_requiring_confirmation_cannot_be_rewritten_as_not_requiring_it() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("a review");

    // Written the ordinary way, so the developer is being asked.
    let asked = store
        .put_brief(
            review.id,
            brief("a webhook delivery that fails once is dropped"),
        )
        .expect("the first brief");
    assert!(
        asked.confirmation_required,
        "a brief requires confirmation unless the caller says otherwise"
    );

    // The same review, rewritten to need no confirmation. **This is the refusal.** An
    // agent that could do this would be releasing its own review's later layers by
    // withdrawing a question the developer had already been asked.
    let refused = store.put_brief(
        review.id,
        NewBrief {
            problem: "a webhook delivery that fails once is dropped".to_string(),
            confirmation_required: false,
            confirmation_not_required_because: Some(
                "38 changed lines, under this caller's 200-line threshold".to_string(),
            ),
            ..Default::default()
        },
    );

    match refused {
        Err(StoreError::InvalidValue { field, .. }) => {
            assert_eq!(field, "confirmation_required");
        }
        other => {
            panic!("rewriting a brief as needing no confirmation must be refused, got {other:?}")
        }
    }

    // And the record is untouched: still requiring confirmation, still asking.
    let held = store.get_brief(review.id).expect("the brief");
    assert!(held.confirmation_required);
    assert_eq!(held.confirmation_not_required_because, None);
    assert!(
        !held.is_settled(),
        "layer 2 is still waiting on the developer"
    );
}
