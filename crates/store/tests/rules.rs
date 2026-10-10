//! The learning layer: a rule's counts, its staleness, and the reason it moved.

mod common;

use common::{db_path, new_review, temp_store};
use review_desk_store::{
    now, Actor, Disposition, FindingRef, NewFinding, NewOutcome, NewRule, OutcomeKind, Rule,
    RuleScope, RuleStage, Severity, Store, StoreError, ACTIVE_RULES_CAP,
};

/// A rule at one scope and stage, with nothing else said about it.
fn new_rule(statement: &str, scope: RuleScope, stage: RuleStage) -> NewRule {
    NewRule {
        statement: statement.to_string(),
        prevents: format!("the failure {statement} prevents"),
        scope,
        stage,
        origin_review_id: None,
        origin_findings: vec![],
        why_proposed: None,
        note: None,
    }
}

/// A finding on `review` citing `rule`, held in the draft.
fn cite(store: &Store, review: i64, rule: Option<i64>, claim: &str) -> i64 {
    store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                file: None,
                line: None,
                claim: claim.to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: rule,
                ..Default::default()
            },
        )
        .expect("add finding")
        .seq
}

/// Record an outcome against one finding.
fn outcome(store: &Store, review: i64, seq: i64, kind: OutcomeKind, detail: Option<&str>) {
    store
        .add_outcome(
            FindingRef {
                review_id: review,
                seq,
            },
            NewOutcome {
                kind,
                actor: Actor::Developer,
                detail: detail.map(str::to_string),
                occurred_at: None,
            },
        )
        .expect("add outcome");
}

#[test]
fn a_proposal_carries_why_it_was_proposed_and_the_line_under_it() {
    let (_dir, store) = temp_store();

    let proposed = store
        .create_rule(NewRule {
            why_proposed: Some(
                "You cut this kind of finding from 5 of the last 5 drafts".to_string(),
            ),
            note: Some("Would suppress, not delete".to_string()),
            ..new_rule(
                "Do not raise rename-for-clarity suggestions",
                RuleScope::Repo("acme/portal".to_string()),
                RuleStage::Proposed,
            )
        })
        .expect("create a proposal");

    assert_eq!(
        proposed.why_proposed.as_deref(),
        Some("You cut this kind of finding from 5 of the last 5 drafts")
    );
    assert_eq!(proposed.note.as_deref(), Some("Would suppress, not delete"));
    assert_eq!(
        proposed.updated_at, proposed.created_at,
        "a rule nothing has touched was last changed when it was created"
    );
}

#[test]
fn a_stage_change_records_why_and_when() {
    // Without the reason, a dismissed proposal is proposed again next week with no
    // memory of having been told no.
    let (_dir, store) = temp_store();
    let rule = store
        .create_rule(NewRule {
            note: Some("Would suppress, not delete".to_string()),
            ..new_rule("no renames", RuleScope::Global, RuleStage::Proposed)
        })
        .unwrap();

    let dismissed = store
        .set_rule_stage(
            rule.id,
            RuleStage::Retired,
            Some("  two of the five I cut were in generated code  "),
        )
        .expect("dismiss with a reason");

    assert_eq!(dismissed.stage, RuleStage::Retired);
    assert_eq!(
        dismissed.note.as_deref(),
        Some("two of the five I cut were in generated code"),
        "the reason replaces the line under the statement"
    );
    assert!(
        dismissed.updated_at >= rule.updated_at,
        "and the rule says when it was changed"
    );

    // Promoting carries no reason — its request body has none — and the line
    // explaining the proposal does not survive the proposal being decided.
    let promoted = store
        .set_rule_stage(rule.id, RuleStage::Active, None)
        .unwrap();
    assert_eq!(promoted.stage, RuleStage::Active);
    assert_eq!(promoted.note, None);
}

#[test]
fn a_deferred_rule_says_when_to_ask_again() {
    let (_dir, store) = temp_store();
    let rule = store
        .create_rule(new_rule(
            "tenant guard",
            RuleScope::Global,
            RuleStage::Active,
        ))
        .unwrap();
    assert_eq!(rule.review_again_at, None);

    let until = now() + 60 * 86_400;
    let deferred = store
        .defer_rule(rule.id, until, Some("#415 had not merged"))
        .expect("keep it for longer");

    assert_eq!(deferred.review_again_at, Some(until));
    assert_eq!(deferred.note.as_deref(), Some("#415 had not merged"));
    assert_eq!(
        deferred.stage,
        RuleStage::Active,
        "keeping a rule does not move it between stages"
    );
}

#[test]
fn a_rules_counts_come_from_the_findings_that_cite_it() {
    let (_dir, store) = temp_store();
    let origin = store
        .create_review(new_review("acme/portal", 377, "aaaaaaa", 1))
        .unwrap();
    let later = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    // The finding the rule came out of, created before the rule cites it.
    let origin_seq = cite(&store, origin.id, None, "the finding it came out of");
    let rule = store
        .create_rule(NewRule {
            origin_review_id: Some(origin.id),
            origin_findings: vec![FindingRef {
                review_id: origin.id,
                seq: origin_seq,
            }],
            ..new_rule(
                "A new test must fail when the fix is reverted",
                RuleScope::Repo("acme/portal".to_string()),
                RuleStage::Active,
            )
        })
        .unwrap();

    // The origin finding is updated to cite the rule it produced, and two later
    // findings cite it too.
    store
        .add_finding(
            origin.id,
            NewFinding {
                rule_id: Some(rule.id),
                ..NewFinding {
                    lens: "l".to_string(),
                    severity: Severity::Should,
                    file: None,
                    line: None,
                    claim: "a second finding on the origin review".to_string(),
                    evidence: None,
                    disposition: Disposition::InDraft,
                    disposition_reason: None,
                    rule_id: None,
                    ..Default::default()
                }
            },
        )
        .unwrap();
    let kept_seq = cite(&store, later.id, Some(rule.id), "kept by the developer");
    let rejected_seq = cite(&store, later.id, Some(rule.id), "rejected by the author");

    outcome(&store, later.id, kept_seq, OutcomeKind::Kept, None);
    outcome(
        &store,
        later.id,
        rejected_seq,
        OutcomeKind::Rejected,
        Some("the export is chunked upstream"),
    );
    // A rejection with no reason is not a rejection with a reason.
    outcome(&store, origin.id, 2, OutcomeKind::Rejected, Some("   "));

    let counts = store.rule_counts(rule.id).unwrap();

    assert_eq!(counts.raised, Some(3), "three findings cite it");
    assert_eq!(counts.kept, Some(1));
    assert_eq!(
        counts.rejected_with_reason,
        Some(1),
        "a rejection with a blank detail gave no reason"
    );
    assert_eq!(
        counts.exceptions,
        Some(0),
        "a measured zero, not an absence"
    );

    store
        .add_rule_exception(rule.id, "accepted for reporting")
        .unwrap();
    assert_eq!(store.rule_counts(rule.id).unwrap().exceptions, Some(1));

    // The three the store has no source for are absent, not zero.
    assert_eq!(counts.caught_by_check, None);
    assert_eq!(counts.raised_in_a_review_since, None);
    assert_eq!(counts.last_human_mention_at, None);
}

#[test]
fn cited_by_findings_counts_only_the_findings_on_a_later_review() {
    // "cited by 6 later findings" on the Rules screen, and what reconciles the
    // contract's own example: R-09 is raised 7 times and cited by 6.
    let (_dir, store) = temp_store();
    let origin = store
        .create_review(new_review("acme/portal", 377, "aaaaaaa", 1))
        .unwrap();
    let later = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    let first = cite(&store, origin.id, None, "the origin finding");
    let rule = store
        .create_rule(NewRule {
            origin_review_id: Some(origin.id),
            origin_findings: vec![FindingRef {
                review_id: origin.id,
                seq: first,
            }],
            ..new_rule("a rule", RuleScope::Global, RuleStage::Active)
        })
        .unwrap();

    // One more finding on the review the rule came out of, and one on a later review.
    // Both cite it; only the second is a later finding.
    cite(
        &store,
        origin.id,
        Some(rule.id),
        "same review, after the rule",
    );
    cite(&store, later.id, Some(rule.id), "a later review");

    let counts = store.rule_counts(rule.id).unwrap();
    assert_eq!(counts.raised, Some(2), "both findings cite it");
    assert_eq!(
        counts.cited_by_findings,
        Some(1),
        "but only one of them is on a later review"
    );
}

#[test]
fn rules_can_be_listed_by_stage_for_one_repository_or_all_of_them() {
    let (_dir, store) = temp_store();

    store
        .create_rule(new_rule(
            "global active",
            RuleScope::Global,
            RuleStage::Active,
        ))
        .unwrap();
    store
        .create_rule(new_rule(
            "portal proposed",
            RuleScope::Repo("acme/portal".to_string()),
            RuleStage::Proposed,
        ))
        .unwrap();
    store
        .create_rule(new_rule(
            "portal retired",
            RuleScope::Repo("acme/portal".to_string()),
            RuleStage::Retired,
        ))
        .unwrap();
    store
        .create_rule(new_rule(
            "billing active",
            RuleScope::Repo("acme/billing-api".to_string()),
            RuleStage::Active,
        ))
        .unwrap();

    let statements = |rules: Vec<Rule>| {
        rules
            .into_iter()
            .map(|rule| rule.statement)
            .collect::<Vec<_>>()
    };

    // What the Rules screen asks for: one repository, in force and proposed, which
    // neither `rules_for_repo` nor `list_rules` can answer.
    assert_eq!(
        statements(
            store
                .list_rules_at_stages(
                    Some("ACME/Portal"),
                    &[RuleStage::Proposed, RuleStage::Active, RuleStage::Ci]
                )
                .unwrap()
        ),
        ["global active", "portal proposed"],
        "the global rules and that repository's own, global first, and the repository \
         name normalised as the column is"
    );

    assert_eq!(
        statements(
            store
                .list_rules_at_stages(None, &[RuleStage::Active])
                .unwrap()
        ),
        ["global active", "billing active"],
        "no repository means every repository"
    );

    match store.list_rules_at_stages(None, &[]) {
        Err(StoreError::InvalidValue {
            field, expected, ..
        }) => {
            assert_eq!(field, "stage");
            assert!(expected.contains("proposed"), "{expected}");
        }
        other => panic!("a filter that permits nothing is a mistake: {other:?}"),
    }
}

#[test]
fn the_rules_summary_counts_what_it_can_and_says_what_it_cannot() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    let elsewhere = store
        .create_review(new_review("acme/billing-api", 88, "bbbbbbb", 1))
        .unwrap();

    store
        .create_rule(new_rule(
            "global active",
            RuleScope::Global,
            RuleStage::Active,
        ))
        .unwrap();
    store
        .create_rule(new_rule(
            "portal ci",
            RuleScope::Repo("acme/portal".to_string()),
            RuleStage::Ci,
        ))
        .unwrap();
    store
        .create_rule(new_rule(
            "portal proposed",
            RuleScope::Repo("acme/portal".to_string()),
            RuleStage::Proposed,
        ))
        .unwrap();

    let kept = cite(&store, review.id, None, "kept");
    cite(&store, review.id, None, "not decided on");
    outcome(&store, review.id, kept, OutcomeKind::Kept, None);
    let circled = cite(&store, review.id, None, "raised a second time");
    outcome(&store, review.id, circled, OutcomeKind::RaisedAgain, None);
    cite(&store, elsewhere.id, None, "another repository's finding");

    let summary = store.rules_summary(Some("acme/portal"), 30).unwrap();

    assert_eq!(
        summary.active_rules, 2,
        "in force is active, lint and CI — a proposal is not a rule yet"
    );
    assert_eq!(summary.active_rules_cap, ACTIVE_RULES_CAP);
    assert_eq!(
        summary.findings_total,
        Some(3),
        "this repository's findings"
    );
    assert_eq!(summary.findings_kept, Some(1));
    assert_eq!(summary.findings_window_days, Some(30));
    assert_eq!(summary.raised_twice_on_one_pr, Some(1));
    assert_eq!(summary.replay_matched, None, "there is no replay check");
    assert_eq!(summary.replay_total, None);

    let everywhere = store.rules_summary(None, 30).unwrap();
    assert_eq!(everywhere.findings_total, Some(4));
    assert_eq!(everywhere.active_rules, 2);

    match store.rules_summary(None, 0) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "window_days"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn findings_outside_the_window_are_not_counted() {
    let (dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    let seq = cite(&store, review.id, None, "inside the window");
    outcome(&store, review.id, seq, OutcomeKind::Kept, None);

    let inside = store.rules_summary(None, 30).unwrap();
    assert_eq!(inside.findings_total, Some(1));

    // Aged past the window in the database's own SQL, as `tests/migrations.rs` seeds an
    // old schema: the store has no function for moving a finding's creation time
    // backwards, and it should not grow one so that a test can pass.
    {
        let conn = rusqlite::Connection::open(db_path(&dir)).unwrap();
        conn.execute(
            "UPDATE findings SET created_at = ?1 WHERE review_id = ?2 AND seq = ?3",
            rusqlite::params![now() - 400 * 86_400, review.id, seq],
        )
        .unwrap();
    }

    let outside = store.rules_summary(None, 30).unwrap();
    assert_eq!(outside.findings_total, Some(0), "the window is a window");
    assert_eq!(outside.findings_kept, Some(0));
}
