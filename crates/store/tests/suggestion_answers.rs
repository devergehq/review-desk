//! What the developer makes of a session's suggestions — DEV-1334, under DEV-1331.
//!
//! **The session proposes; the developer decides.** Accepting a suggestion goes through the
//! store call the page's own control makes, records the act as the developer's, and adds only
//! which reply prompted it. Dismissing one is kept with the developer's reason. And nothing on
//! the closed list ([`SuggestionType::ALL`], imported rather than copied) signs off, approves
//! or posts.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Accepting, Actor, Basis, DecidedBy, Decision, DeveloperFinding, Disposition, DraftStatus,
    FindingDecision, NewDraft, NewDraftSection, NewFinding, OutcomeKind, PromptedBy,
    ReviewLayerName, Severity, Store, StoreError, SuggestionType, SuggestionVerdict, Verification,
};
use serde_json::{json, Value};

/// A review with two findings in a draft written in parts, F1 a should and F2 a nit.
fn review(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("acme/portal", 514, "7f3c2d1", 1))
        .expect("review");
    for (severity, claim) in [
        (Severity::Should, "The column policy is skipped"),
        (Severity::Nit, "A comment describes the removed branch"),
    ] {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity,
                    claim: claim.to_string(),
                    why_it_matters: Some("the consequence".to_string()),
                    ..Default::default()
                },
            )
            .expect("finding");
    }
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection::prose("The approach fits the ticket."),
                    NewDraftSection::finding(
                        1,
                        "The column policy is skipped",
                        "Private columns leave.",
                    ),
                    NewDraftSection::finding(2, "A comment describes the removed branch", "Stale."),
                ]),
            },
        )
        .expect("the draft");
    review.id
}

/// A session's answer suggesting these, and its number on the review.
fn answer(store: &Store, review: i64, suggests: Value) -> i64 {
    let body = json!({
        "kind": "answer",
        "headline": "Yes, the web path has the same hole.",
        "cites": ["F1", "app/Http/Controllers/ExportController.php:41"],
        "suggests": suggests,
    });
    store
        .answer_from_session(review, ReviewLayerName::CodeReview, &body.to_string(), None)
        .expect("an answer in the contract's shape")
        .seq
}

fn new_finding() -> Value {
    json!({ "type": "new_finding", "severity": "should",
            "title": "The web path exports private columns when none are set",
            "where": "app/Http/Controllers/ExportController.php:41", "basis": "ran" })
}

fn regrade_f1() -> Value {
    json!({ "type": "regrade", "finding": "F1", "from": "should", "to": "blocker",
            "why": "Both paths leak private columns." })
}

fn suggested_finding() -> DeveloperFinding {
    DeveloperFinding {
        severity: Severity::Should,
        title: "The web path exports private columns when none are set".to_string(),
        at: "app/Http/Controllers/ExportController.php:41".to_string(),
        basis: Basis::Ran,
    }
}

fn prompt(message: i64, suggestion: Option<i64>) -> PromptedBy {
    PromptedBy {
        message,
        suggestion,
    }
}

fn regrade_to(severity: Severity) -> Decision {
    Decision {
        severity: Some(severity),
        ..Decision::of(FindingDecision::Regrade)
    }
}

// -- the closed list ----------------------------------------------------------------

/// **No suggestion signs off, approves or posts.** Every type on the closed list — read from
/// `SuggestionType::ALL`, never a copy — is accepted by adding a finding, by one of the
/// developer's decisions on a finding, or by filling the ask box. None of the decisions is a
/// sign-off: `FindingDecision` has no approve, confirm or post, and this names the ones it has.
#[test]
fn no_suggestion_signs_off_approves_or_posts() {
    assert!(!SuggestionType::ALL.is_empty());
    for kind in SuggestionType::ALL {
        let name = kind.as_str();
        for word in ["sign", "approve", "post", "confirm", "send"] {
            assert!(
                !name.contains(word),
                "`{name}` reads as a sign-off; the closed list has none"
            );
        }
        match kind.accepting() {
            Accepting::AddsAFinding | Accepting::FillsTheAskBox => {}
            Accepting::Decides(decisions) => {
                assert!(!decisions.is_empty(), "`{name}` decides nothing");
                for decision in decisions {
                    assert!(
                        matches!(
                            decision,
                            FindingDecision::Regrade
                                | FindingDecision::Rewrite
                                | FindingDecision::Cut
                                | FindingDecision::RaiseAgain
                                | FindingDecision::AcceptReason
                                | FindingDecision::Withdraw
                        ),
                        "`{name}` is accepted as `{decision}`, which is not a decision a \
                         suggestion may make"
                    );
                }
            }
        }
    }
}

// -- adding a finding ---------------------------------------------------------------

#[test]
fn accepting_a_new_finding_adds_it_to_to_post_with_the_reply_that_prompted_it() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([new_finding()]));

    let added = store
        .add_finding_from_conversation(
            review,
            &suggested_finding(),
            prompt(reply, Some(1)),
            Actor::Developer,
        )
        .expect("accepting the suggestion");

    assert_eq!(added.seq, 3, "numbered after the session's own");
    assert_eq!(added.prompted_by, Some(reply));
    assert_eq!(
        added.disposition,
        Disposition::InDraft,
        "it goes into To post"
    );
    assert_eq!(added.severity, Severity::Should);
    assert_eq!(
        added.file.as_deref(),
        Some("app/Http/Controllers/ExportController.php")
    );
    assert_eq!(added.line, Some(41));
    assert_eq!(
        added.verification,
        Verification::Reproduced,
        "`ran` is reproduced"
    );
    assert_eq!(added.round_status_by, None, "no session's name is on it");

    let answers = store.suggestion_answers(review).expect("answers");
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].verdict, SuggestionVerdict::Accepted);
    assert_eq!(answers[0].suggestion, Some(1));
    assert_eq!(answers[0].finding_seq, Some(3));

    // **No block of the draft**: Post on Findings writes one, as for any other finding.
    assert!(store
        .draft_sections(review)
        .expect("sections")
        .iter()
        .all(|section| section.finding_seq != Some(3)));
}

#[test]
fn a_session_raises_findings_with_no_prompt() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    assert!(store
        .list_findings(review)
        .expect("findings")
        .iter()
        .all(|finding| finding.prompted_by.is_none()));
}

#[test]
fn turning_an_answer_into_a_finding_uses_the_same_call() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([]));

    let added = store
        .add_finding_from_conversation(
            review,
            &DeveloperFinding {
                at: "the PR itself".to_string(),
                ..suggested_finding()
            },
            prompt(reply, None),
            Actor::Developer,
        )
        .expect("turning the answer into a finding");
    assert_eq!(added.prompted_by, Some(reply));
    assert_eq!(added.file.as_deref(), Some("the PR itself"), "kept whole");
    assert_eq!(added.line, None);

    // Twice is one finding.
    let again = store.add_finding_from_conversation(
        review,
        &suggested_finding(),
        prompt(reply, None),
        Actor::Developer,
    );
    assert!(
        matches!(again, Err(StoreError::SuggestionAnswered { .. })),
        "{again:?}"
    );
    assert_eq!(store.list_findings(review).expect("findings").len(), 3);
}

#[test]
fn a_working_reply_is_not_turned_into_a_finding() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let asked = store
        .ask_session(
            review,
            ReviewLayerName::CodeReview,
            "Does the web path leak too?",
        )
        .expect("asking");
    let working = store
        .answer_from_session(
            review,
            ReviewLayerName::CodeReview,
            &json!({ "kind": "working", "headline": "Looking.", "activity": ["Read it"] })
                .to_string(),
            Some(asked.seq),
        )
        .expect("working")
        .seq;
    let refused = store.add_finding_from_conversation(
        review,
        &suggested_finding(),
        prompt(working, None),
        Actor::Developer,
    );
    assert!(matches!(
        refused,
        Err(StoreError::InvalidValue {
            field: "prompted_by",
            ..
        })
    ));
}

#[test]
fn only_the_developer_adds_a_finding_from_the_conversation() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([new_finding()]));
    for actor in [Actor::Author, Actor::Harvester] {
        let refused = store.add_finding_from_conversation(
            review,
            &suggested_finding(),
            prompt(reply, Some(1)),
            actor,
        );
        assert!(
            matches!(refused, Err(StoreError::InvalidValue { field: "by", .. })),
            "{actor}: {refused:?}"
        );
    }
}

#[test]
fn a_finding_is_not_added_under_a_signed_off_draft() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([new_finding()]));
    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::Comment,
            Actor::Developer,
            &common::shown(&store, review),
        )
        .expect("signing off");
    let refused = store.add_finding_from_conversation(
        review,
        &suggested_finding(),
        prompt(reply, Some(1)),
        Actor::Developer,
    );
    assert!(
        matches!(refused, Err(StoreError::DraftStatus { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_regrade_is_not_accepted_as_a_new_finding() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([regrade_f1()]));
    let refused = store.add_finding_from_conversation(
        review,
        &suggested_finding(),
        prompt(reply, Some(1)),
        Actor::Developer,
    );
    assert!(matches!(
        refused,
        Err(StoreError::InvalidValue {
            field: "prompted_by",
            ..
        })
    ));
}

#[test]
fn a_prose_reply_has_nothing_to_accept() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let prose = store
        .reply_from_session(review, ReviewLayerName::CodeReview, "It does, yes.", None)
        .expect("a reply in prose")
        .seq;
    let refused = store.add_finding_from_conversation(
        review,
        &suggested_finding(),
        prompt(prose, None),
        Actor::Developer,
    );
    assert!(
        matches!(refused, Err(StoreError::InvalidValue { field: "seq", .. })),
        "{refused:?}"
    );
}

// -- a decision -------------------------------------------------------------------

#[test]
fn accepting_a_regrade_is_the_developers_regrade_with_the_prompt_beside_it() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([regrade_f1()]));

    let made = store
        .decide_finding_prompted(
            review,
            1,
            &regrade_to(Severity::Blocker),
            Some(prompt(reply, Some(1))),
            Actor::Developer,
        )
        .expect("accepting the regrade");

    assert_eq!(made.finding.severity, Severity::Blocker);
    assert_eq!(made.outcome.kind, OutcomeKind::Regraded);
    assert_eq!(
        made.outcome.actor,
        Actor::Developer,
        "the developer's decision"
    );

    let answers = store.suggestion_answers(review).expect("answers");
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].outcome_id, Some(made.outcome.id));
    assert_eq!(answers[0].verdict, SuggestionVerdict::Accepted);

    // Accepted once.
    let again = store.decide_finding_prompted(
        review,
        1,
        &regrade_to(Severity::Should),
        Some(prompt(reply, Some(1))),
        Actor::Developer,
    );
    assert!(
        matches!(again, Err(StoreError::SuggestionAnswered { .. })),
        "{again:?}"
    );
    assert_eq!(
        store.get_finding(review, 1).expect("F1").severity,
        Severity::Blocker,
        "a refused second accept wrote nothing"
    );
}

#[test]
fn a_prompt_must_be_the_suggestion_this_decision_accepts() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([regrade_f1()]));

    // The wrong finding, the wrong decision, and the answer as a whole.
    for (seq, decision, prompted) in [
        (2, regrade_to(Severity::Blocker), prompt(reply, Some(1))),
        (
            1,
            Decision::of(FindingDecision::Cut),
            prompt(reply, Some(1)),
        ),
        (1, regrade_to(Severity::Blocker), prompt(reply, None)),
        (1, regrade_to(Severity::Blocker), prompt(reply, Some(2))),
    ] {
        let refused =
            store.decide_finding_prompted(review, seq, &decision, Some(prompted), Actor::Developer);
        assert!(
            matches!(
                refused,
                Err(StoreError::InvalidValue {
                    field: "prompted_by" | "suggestion",
                    ..
                })
            ),
            "F{seq} {:?} {prompted:?}: {refused:?}",
            decision.decision
        );
    }
    assert!(store
        .suggestion_answers(review)
        .expect("answers")
        .is_empty());
    assert_eq!(
        store.get_finding(review, 1).expect("F1").severity,
        Severity::Should
    );
}

#[test]
fn accepting_a_set_aside_records_the_developer_on_the_round_status_never_an_agent() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(
        &store,
        review,
        json!([{ "type": "set_aside", "finding": "F2", "why": "Covered by F1." }]),
    );
    let made = store
        .decide_finding_prompted(
            review,
            2,
            &Decision {
                note: Some("Covered by F1.".to_string()),
                ..Decision::of(FindingDecision::Cut)
            },
            Some(prompt(reply, Some(1))),
            Actor::Developer,
        )
        .expect("accepting the set aside");
    assert_eq!(made.finding.disposition, Disposition::Suppressed);
    assert_ne!(made.finding.round_status_by, Some(DecidedBy::Agent));
    assert_eq!(made.outcome.actor, Actor::Developer);
}

// -- dismissing -------------------------------------------------------------------

#[test]
fn a_dismissal_is_kept_with_the_reason_said_after_the_click() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([regrade_f1(), new_finding()]));

    let first = store
        .dismiss_suggestion(review, reply, 1, None, Actor::Developer)
        .expect("dismissing");
    assert_eq!(first.verdict, SuggestionVerdict::Dismissed);
    assert_eq!(first.reason, None);

    let said = store
        .dismiss_suggestion(
            review,
            reply,
            1,
            Some("  F1 stays a should.  "),
            Actor::Developer,
        )
        .expect("the reason after");
    assert_eq!(said.reason.as_deref(), Some("F1 stays a should."));
    assert_eq!(store.suggestion_answers(review).expect("answers").len(), 1);

    // A dismissed suggestion is not accepted afterwards.
    let refused = store.decide_finding_prompted(
        review,
        1,
        &regrade_to(Severity::Blocker),
        Some(prompt(reply, Some(1))),
        Actor::Developer,
    );
    assert!(
        matches!(refused, Err(StoreError::SuggestionAnswered { .. })),
        "{refused:?}"
    );

    // The other suggestion is still open.
    store
        .add_finding_from_conversation(
            review,
            &suggested_finding(),
            prompt(reply, Some(2)),
            Actor::Developer,
        )
        .expect("the second suggestion is its own");
}

#[test]
fn an_accepted_suggestion_is_not_dismissed() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([new_finding()]));
    store
        .add_finding_from_conversation(
            review,
            &suggested_finding(),
            prompt(reply, Some(1)),
            Actor::Developer,
        )
        .expect("accepting");
    let refused = store.dismiss_suggestion(review, reply, 1, None, Actor::Developer);
    assert!(
        matches!(refused, Err(StoreError::SuggestionAnswered { .. })),
        "{refused:?}"
    );
}

#[test]
fn only_the_developer_dismisses() {
    let (_dir, store) = temp_store();
    let review = review(&store);
    let reply = answer(&store, review, json!([new_finding()]));
    let refused = store.dismiss_suggestion(review, reply, 1, None, Actor::Harvester);
    assert!(matches!(
        refused,
        Err(StoreError::InvalidValue { field: "by", .. })
    ));
    let past = store.dismiss_suggestion(review, reply, 2, None, Actor::Developer);
    assert!(matches!(
        past,
        Err(StoreError::InvalidValue {
            field: "suggestion",
            ..
        })
    ));
}
