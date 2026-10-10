//! One round trip per entity: create it through the store, read it back, compare.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, BriefOption, Disposition, FindingRef, NewBrief, NewFinding, NewOutcome, NewReview,
    NewRule, NitTicketStatus, OutcomeKind, Review, ReviewFacts, RuleScope, RuleStage, Severity,
    StoreError,
};

/// A review to hang the other entities off.
fn seed_review(store: &review_desk_store::Store) -> Review {
    store
        .create_review(new_review("devergehq/review-desk", 12, "deadbeef", 1))
        .expect("seed review")
}

#[test]
fn a_review_is_created_and_read_back() {
    let (_dir, store) = temp_store();

    let created = store
        .create_review(NewReview {
            repo: "devergehq/review-desk".to_string(),
            pr: 12,
            head_sha: "deadbeef".to_string(),
            round: 2,
            title: Some("Record what a review decided".to_string()),
            ticket_id: Some("DEV-850".to_string()),
            changed_lines: Some(214),
            session_id: Some("allele-session-1".to_string()),
            ledger_id: Some("ledger-9".to_string()),
            github_review_id: Some("gh-review-44".to_string()),
        })
        .expect("create review");

    let read = store.get_review(created.id).expect("read review");

    assert_eq!(read, created);
    assert_eq!(read.repo, "devergehq/review-desk");
    assert_eq!(read.pr, 12);
    assert_eq!(read.head_sha, "deadbeef");
    assert_eq!(read.round, 2);
    assert_eq!(read.session_id.as_deref(), Some("allele-session-1"));
    assert_eq!(read.ledger_id.as_deref(), Some("ledger-9"));
    assert_eq!(read.github_review_id.as_deref(), Some("gh-review-44"));
    assert!(read.created_at > 0);
}

#[test]
fn a_brief_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let created = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "Reviews repeat themselves and nothing records why".to_string(),
                diagram: Some("graph TD; A-->B".to_string()),
                option_map: Some("1. do nothing 2. a skill 3. an app".to_string()),
                approach_verdict: Some(
                    "An app, because the record has to outlive a session".to_string(),
                ),
                options: Vec::new(),
                provenance: Some("from the ticket and the code".to_string()),
                ..Default::default()
            },
        )
        .expect("put brief");

    let read = store.get_brief(review.id).expect("read brief");

    assert_eq!(read, created);
    assert_eq!(read.review_id, review.id);
    assert_eq!(
        read.problem,
        "Reviews repeat themselves and nothing records why"
    );
    assert_eq!(read.diagram.as_deref(), Some("graph TD; A-->B"));
    assert_eq!(
        read.option_map.as_deref(),
        Some("1. do nothing 2. a skill 3. an app")
    );
    assert!(read.approach_verdict.is_some());
    assert!(!read.is_confirmed(), "a new brief is unconfirmed");
    assert_eq!(read.confirmed_at, None);
}

#[test]
fn a_finding_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let created = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                file: Some("crates/store/src/lib.rs".to_string()),
                line: Some(42),
                claim: "The connection does not enforce foreign keys".to_string(),
                evidence: Some("PRAGMA foreign_keys returns 0".to_string()),
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                ..Default::default()
            },
        )
        .expect("add finding");

    let read = store
        .get_finding(review.id, created.seq)
        .expect("read finding");

    assert_eq!(read, created);
    assert_eq!(read.seq, 1, "the first finding of a review is number 1");
    assert_eq!(read.lens, "correctness");
    assert_eq!(read.severity, Severity::Blocker);
    assert_eq!(read.file.as_deref(), Some("crates/store/src/lib.rs"));
    assert_eq!(read.line, Some(42));
    assert_eq!(read.disposition, Disposition::InDraft);
    assert_eq!(read.disposition_reason, None);
}

#[test]
fn an_outcome_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);
    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "clarity".to_string(),
                severity: Severity::Nit,
                file: None,
                line: None,
                claim: "The name reads oddly".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                ..Default::default()
            },
        )
        .unwrap();

    let created = store
        .add_outcome(
            FindingRef {
                review_id: review.id,
                seq: finding.seq,
            },
            NewOutcome {
                kind: OutcomeKind::Cut,
                actor: Actor::Developer,
                detail: Some("A nit on a file the change does not touch".to_string()),
                occurred_at: Some(1_760_000_000),
            },
        )
        .expect("add outcome");

    let read = store.get_outcome(created.id).expect("read outcome");

    assert_eq!(read, created);
    assert_eq!(read.review_id, review.id);
    assert_eq!(read.finding_seq, finding.seq);
    assert_eq!(read.kind, OutcomeKind::Cut);
    assert_eq!(read.actor, Actor::Developer);
    assert_eq!(read.occurred_at, 1_760_000_000);
    assert_eq!(
        read.detail.as_deref(),
        Some("A nit on a file the change does not touch")
    );
}

#[test]
fn a_rule_is_created_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);
    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "process".to_string(),
                severity: Severity::Should,
                file: None,
                line: None,
                claim: "The pull request description carries working notes".to_string(),
                evidence: Some("Three paragraphs of session narration".to_string()),
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                ..Default::default()
            },
        )
        .unwrap();

    let created = store
        .create_rule(NewRule {
            statement: "A pull request description states the decision, not the session"
                .to_string(),
            prevents: "A reader cannot find the decision in the narration".to_string(),
            scope: RuleScope::Repo("devergehq/review-desk".to_string()),
            stage: RuleStage::Active,
            origin_review_id: Some(review.id),
            origin_findings: vec![FindingRef {
                review_id: review.id,
                seq: finding.seq,
            }],
            why_proposed: None,
            note: None,
        })
        .expect("create rule");

    let read = store.get_rule(created.id).expect("read rule");

    assert_eq!(read, created);
    assert_eq!(
        read.scope,
        RuleScope::Repo("devergehq/review-desk".to_string())
    );
    assert_eq!(read.scope.repo(), Some("devergehq/review-desk"));
    assert_eq!(read.stage, RuleStage::Active);
    assert_eq!(read.origin_review_id, Some(review.id));
    assert_eq!(
        read.origin_findings,
        vec![FindingRef {
            review_id: review.id,
            seq: finding.seq
        }]
    );
    assert!(!read.prevents.is_empty(), "a rule names what it prevents");
}

#[test]
fn a_global_rule_round_trips_without_a_repository() {
    let (_dir, store) = temp_store();

    let created = store
        .create_rule(NewRule {
            statement: "Every finding carries evidence".to_string(),
            prevents: "Preference presented as a defect".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Proposed,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .expect("create global rule");

    let read = store.get_rule(created.id).unwrap();
    assert_eq!(read.scope, RuleScope::Global);
    assert_eq!(read.scope.repo(), None);
    assert!(read.origin_findings.is_empty());
}

#[test]
fn a_brief_with_a_misspelled_field_is_refused_rather_than_losing_it() {
    // `option_map` is optional, so without `deny_unknown_fields` a typo would
    // deserialise to None and the record would quietly lose what the review wrote.
    let error = serde_json::from_str::<NewBrief>(
        r#"{"problem": "the store is write-only", "optionmap": "a, b or c"}"#,
    )
    .expect_err("an unknown field must be refused");

    assert!(
        error.to_string().contains("optionmap"),
        "the message must name the field that was not understood: {error}"
    );
}

#[test]
fn a_finding_with_a_misspelled_field_is_refused_rather_than_losing_it() {
    let error = serde_json::from_str::<NewFinding>(
        r#"{"lens": "correctness", "severity": "blocker", "claim": "x", "evidenc": "y"}"#,
    )
    .expect_err("an unknown field must be refused");

    assert!(error.to_string().contains("evidenc"), "{error}");
}

#[test]
fn an_unknown_severity_names_the_valid_severities() {
    // The command line takes a finding as JSON, so it is serde that reports a bad
    // severity, not the store's own parser. DEV-851 requires the valid values be named.
    let error = serde_json::from_str::<NewFinding>(
        r#"{"lens": "correctness", "severity": "catastrophic", "claim": "x"}"#,
    )
    .expect_err("an unknown severity must be refused");

    let message = error.to_string();
    for valid in ["blocker", "should", "nit", "question"] {
        assert!(message.contains(valid), "{valid} missing from: {message}");
    }
}

#[test]
fn a_brief_holds_its_options_as_cards_in_key_order() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a webhook delivery fails once and is never tried again".to_string(),
                options: vec![
                    BriefOption {
                        key: "B".to_string(),
                        title: "Queue with backoff".to_string(),
                        argument_for: "Meets the 24-hour window".to_string(),
                        argument_against: "Needs an idempotency key".to_string(),
                        chosen: true,
                    },
                    BriefOption {
                        key: "A".to_string(),
                        title: "Retry inside the request".to_string(),
                        argument_for: "Smallest change".to_string(),
                        argument_against: "Cannot span 24 hours".to_string(),
                        chosen: false,
                    },
                ],
                provenance: Some("from the ticket and the code".to_string()),
                ..Default::default()
            },
        )
        .expect("put brief");

    let keys = |brief: &review_desk_store::Brief| {
        brief
            .options
            .iter()
            .map(|option| option.key.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&stored), ["A", "B"], "options come back in key order");
    assert_eq!(stored, store.get_brief(review.id).unwrap());
    assert!(stored.options[1].chosen, "B is what the author built");
    assert_eq!(
        stored.provenance.as_deref(),
        Some("from the ticket and the code")
    );

    // Replacing the brief replaces the options with it: a problem statement that
    // changed under the previous round's cards is not a brief anybody wrote.
    let replaced = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a different problem".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(replaced.options.is_empty());
}

#[test]
fn an_option_map_and_structured_options_together_are_refused() {
    // Two spellings of one fact, and nothing could say which the review meant.
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            option_map: Some("A or B".to_string()),
            options: vec![BriefOption {
                key: "A".to_string(),
                title: "t".to_string(),
                argument_for: "f".to_string(),
                argument_against: "a".to_string(),
                chosen: true,
            }],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "option_map"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_option_with_a_blank_field_or_a_repeated_key_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let option = |key: &str, title: &str| BriefOption {
        key: key.to_string(),
        title: title.to_string(),
        argument_for: "for".to_string(),
        argument_against: "against".to_string(),
        chosen: false,
    };

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            options: vec![option("A", "   ")],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "options.title"),
        other => panic!("a card with a blank on it is refused, got {other:?}"),
    }

    match store.put_brief(
        review.id,
        NewBrief {
            problem: "x".to_string(),
            options: vec![option("A", "one"), option("A", "two")],
            ..Default::default()
        },
    ) {
        Err(StoreError::InvalidValue { field, value, .. }) => {
            assert_eq!(field, "options");
            assert_eq!(value, "A", "nothing could say which one B is on the screen");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_rule_carries_its_accepted_exceptions() {
    let (_dir, store) = temp_store();
    let rule = store
        .create_rule(NewRule {
            statement: "Money is stored as whole cents".to_string(),
            prevents: "an invoice header and its lines disagreeing by a cent".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .unwrap();

    let exception = store
        .add_rule_exception(rule.id, "  accepted for the reporting tables  ")
        .expect("accept an exception");
    assert_eq!(exception.rule_id, rule.id);
    assert_eq!(exception.reason, "accepted for the reporting tables");

    store
        .add_rule_exception(rule.id, "and for the legacy importer")
        .unwrap();
    let held = store.list_rule_exceptions(rule.id).unwrap();
    assert_eq!(held.len(), 2, "oldest first");
    assert_eq!(held[0], exception);

    assert_eq!(
        store.get_rule(rule.id).unwrap().stage,
        RuleStage::Active,
        "an exception does not retire the rule it excepts"
    );

    match store.add_rule_exception(rule.id, "  ") {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "reason"),
        other => panic!("an exception with no reason is refused, got {other:?}"),
    }
    match store.add_rule_exception(999_999, "for a rule that is not there") {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!((entity, parent), ("rule exception", "rule"));
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn a_review_carries_the_pull_requests_own_facts() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let with_facts = store
        .set_review_facts(
            review.id,
            &ReviewFacts {
                title: Some("Retry failed webhook deliveries".to_string()),
                ticket_id: Some("ABC-231".to_string()),
                changed_lines: Some(214),
                not_checked: None,
            },
        )
        .expect("set facts");

    assert_eq!(
        with_facts.title.as_deref(),
        Some("Retry failed webhook deliveries")
    );
    assert_eq!(with_facts.ticket_id.as_deref(), Some("ABC-231"));
    assert_eq!(with_facts.changed_lines, Some(214));
    assert_eq!(with_facts.not_checked, None);

    // A fact left out is left alone, so a session can report what it did not check
    // without having to repeat the title it set an hour ago.
    let later = store
        .set_review_facts(
            review.id,
            &ReviewFacts {
                not_checked: Some("the worker's behaviour across a deploy".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        later.title.as_deref(),
        Some("Retry failed webhook deliveries"),
        "setting one fact does not clear the others"
    );
    assert_eq!(
        later.not_checked.as_deref(),
        Some("the worker's behaviour across a deploy")
    );
}

#[test]
fn opening_a_review_records_what_the_caller_knows_about_it() {
    let (_dir, store) = temp_store();

    let facts = ReviewFacts {
        title: Some("Retry failed webhook deliveries".to_string()),
        ticket_id: Some("ABC-231".to_string()),
        changed_lines: Some(214),
        not_checked: None,
    };

    let opened = store
        .open_review_with("acme/portal", 412, "3f9c2ab", &facts)
        .expect("open with facts");
    assert!(opened.created);
    assert_eq!(
        opened.review.title.as_deref(),
        Some("Retry failed webhook deliveries")
    );

    // The same head commit again is the same review, and the facts given are written
    // to it. This is how a finished session reports what it did not check.
    let again = store
        .open_review_with(
            "acme/portal",
            412,
            "3f9c2ab",
            &ReviewFacts {
                not_checked: Some("production delivery volumes".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    assert!(
        !again.created,
        "a head commit already reviewed is handed back"
    );
    assert_eq!(again.review.id, opened.review.id);
    assert_eq!(again.review.round, 1, "and spends no round");
    assert_eq!(
        again.review.not_checked.as_deref(),
        Some("production delivery volumes")
    );
    assert_eq!(
        again.review.changed_lines,
        Some(214),
        "the facts it already had are untouched"
    );
}

#[test]
fn a_finding_names_the_rule_it_was_raised_under_and_moves_its_last_citation() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);
    let rule = store
        .create_rule(NewRule {
            statement: "A new test must fail when the fix is reverted".to_string(),
            prevents: "a regression shipping behind a test that cannot fail".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .unwrap();

    assert_eq!(
        store.get_rule(rule.id).unwrap().last_cited_at,
        None,
        "a rule nothing has cited has never been cited, which is not the same as being \
         cited at the epoch"
    );

    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "can this test fail".to_string(),
                severity: Severity::Should,
                file: None,
                line: None,
                claim: "The failure test passes with the retry code reverted".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: Some(rule.id),
                ..Default::default()
            },
        )
        .expect("add a finding citing a rule");

    assert_eq!(finding.rule_id, Some(rule.id));
    let cited = store.get_rule(rule.id).unwrap();
    assert_eq!(
        cited.last_cited_at,
        Some(finding.created_at),
        "the citation moved the rule's last-cited time"
    );
    assert_eq!(
        cited.updated_at, finding.created_at,
        "and the rule was changed, so it says when"
    );
}

#[test]
fn a_finding_citing_a_rule_that_is_not_there_names_the_rule() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    match store.add_finding(
        review.id,
        NewFinding {
            lens: "l".to_string(),
            severity: Severity::Nit,
            file: None,
            line: None,
            claim: "c".to_string(),
            evidence: None,
            disposition: Disposition::InDraft,
            disposition_reason: None,
            rule_id: Some(404),
            ..Default::default()
        },
    ) {
        Err(StoreError::UnknownParent { entity, parent, id }) => {
            assert_eq!((entity, parent), ("finding", "rule"));
            assert_eq!(id, "404", "the message blames the rule, not the review");
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }

    assert!(
        store.list_findings(review.id).unwrap().is_empty(),
        "and the finding was not written"
    );
}

#[test]
fn one_ticket_covers_every_nit_held_back_from_a_review() {
    let (_dir, store) = temp_store();
    let review = seed_review(&store);

    let nit = |claim: &str| NewFinding {
        lens: "repetition".to_string(),
        severity: Severity::Nit,
        file: None,
        line: None,
        claim: claim.to_string(),
        evidence: None,
        disposition: Disposition::HeldBack,
        disposition_reason: Some("held for one ticket with the other nits".to_string()),
        rule_id: None,
        ..Default::default()
    };
    store
        .add_finding(review.id, nit("backoff repeated in two files"))
        .unwrap();
    store.add_finding(review.id, nit("unused import")).unwrap();
    store
        .add_finding(
            review.id,
            NewFinding {
                disposition: Disposition::InDraft,
                ..nit("in the draft, not held back")
            },
        )
        .unwrap();

    let before = store.nit_ticket(review.id).unwrap();
    assert_eq!(before.status, NitTicketStatus::NotRequested);
    assert_eq!(
        before.finding_seqs,
        [1, 2],
        "the findings it covers are read from the dispositions, not stored against it"
    );
    assert_eq!(before.requested_at, None);

    let requested = store
        .request_nit_ticket(review.id, None)
        .expect("the developer asks for one");
    assert_eq!(requested.status, NitTicketStatus::Requested);
    assert!(requested.requested_at.is_some());
    assert_eq!(requested.ticket_id, None);
    assert_eq!(requested.filed_at, None);

    let filed = store
        .request_nit_ticket(review.id, Some("ABC-240"))
        .expect("an agent reports the ticket it filed");
    assert_eq!(filed.status, NitTicketStatus::Filed);
    assert_eq!(filed.ticket_id.as_deref(), Some("ABC-240"));
    assert_eq!(
        filed.requested_at, requested.requested_at,
        "the time it was asked for is kept: the request was made once"
    );
    assert!(filed.filed_at.is_some());

    match store.request_nit_ticket(review.id, Some("   ")) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "nit_ticket_id"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}
