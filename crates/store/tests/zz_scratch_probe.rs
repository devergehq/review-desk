//! Temporary probe, deleted after the review.
mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Decision, DraftStatus, FindingDecision, NewDraft, NewDraftSection, NewFinding, Severity,
    Store,
};

fn seeded(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("review");
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "A retried delivery is sent without an idempotency key".to_string(),
                why_it_matters: Some("the consequence".to_string()),
                ..Default::default()
            },
        )
        .expect("finding");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection::prose("Summary sentence."),
                    NewDraftSection::finding(1, "A retried delivery", "Twice delivered."),
                    NewDraftSection::prose("**Not checked:** the admin screens."),
                ]),
            },
        )
        .expect("draft");
    review.id
}

#[test]
fn probe_rewrite_with_newlines() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            1,
            &Decision {
                text: Some("\nThe customer receives it twice.\n".to_string()),
                ..Decision::of(FindingDecision::Rewrite)
            },
            Actor::Developer,
        )
        .expect("a rewrite with newlines at both ends");

    println!("BODY >>>{}<<<", made.draft.body.escape_debug());
}

#[test]
fn probe_rewrite_with_empty_title() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let answer = store.decide_finding(
        review,
        1,
        &Decision {
            title: Some(String::new()),
            ..Decision::of(FindingDecision::Rewrite)
        },
        Actor::Developer,
    );
    println!("EMPTY TITLE >>>{answer:?}<<<");
}

#[test]
fn probe_whitespace_only_body() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);
    let answer = store.put_draft(
        review,
        NewDraft {
            body: "   ".to_string(),
            status: DraftStatus::Ready,
            lint: None,
            sections: None,
        },
    );
    println!("WHITESPACE BODY >>>{answer:?}<<<");
}

#[test]
fn probe_keep_a_held_back_nit_with_trailing_newline() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 500, "aaaaaaa", 1))
        .expect("review");
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "clarity".to_string(),
                severity: Severity::Nit,
                claim: "Backoff schedule is repeated".to_string(),
                why_it_matters: Some("Two copies will drift.\n".to_string()),
                disposition: review_desk_store::Disposition::HeldBack,
                ..Default::default()
            },
        )
        .expect("finding");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection::prose("Summary sentence.")]),
            },
        )
        .expect("draft");

    let answer = store.decide_finding(
        review.id,
        1,
        &Decision::of(FindingDecision::Keep),
        Actor::Developer,
    );
    println!(
        "KEEP TRAILING NEWLINE >>>{:?}<<<",
        answer.map(|m| m.draft.body)
    );
}
