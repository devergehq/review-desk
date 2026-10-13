//! Where a review's understanding came from: the four lists, and what each one refuses.
//!
//! The refusals are most of this file, deliberately. Each of the four records is a list
//! replaced whole, so the writing is uninteresting; what makes the record worth having is
//! that it cannot hold a criterion marked unmet with nothing saying so, an unreviewed area
//! with no reason, or two coverage claims about one file.

mod common;

use common::{new_review, posted, temp_store};
use review_desk_store::{
    AcceptanceCriterion, CriterionStatus, DecidedBy, DescriptionLintStatus, FileCheck,
    FindingCarry, NewAcceptanceCriterion, NewDescriptionLintResult, NewFileChange, NewFinding,
    NewReviewSource, RoundStatus, Severity, SourceKind, Store, StoreError,
};

/// A review with three findings, two of them on files, matching board A2.
fn seeded() -> (tempfile::TempDir, Store, i64) {
    let (dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("the review")
        .id;

    for (lens, file, line, claim) in [
        (
            "sibling diff",
            Some("jobs/DeliverWebhook.php"),
            Some(58),
            "A retried delivery is sent without an idempotency key",
        ),
        (
            "boundaries",
            Some("jobs/DeliverWebhook.php"),
            Some(91),
            "The backoff schedule has no ceiling",
        ),
        (
            "duplication",
            Some("config/webhooks.php"),
            Some(4),
            "Backoff schedule is repeated in two files",
        ),
    ] {
        store
            .add_finding(
                review,
                NewFinding {
                    lens: lens.to_string(),
                    severity: Severity::Should,
                    file: file.map(str::to_string),
                    line,
                    claim: claim.to_string(),
                    evidence: None,
                    disposition: review_desk_store::Disposition::InDraft,
                    // The rest from the hand-written `Default`, which is the shape every
                    // call site in these crates uses. Reading it first was the point: it
                    // gives `verification: unknown` rather than serde's first variant,
                    // `reproduced`, so a finding built here does not claim to have been run.
                    ..Default::default()
                },
            )
            .expect("the finding");
    }

    (dir, store, review)
}

fn invalid_field(error: StoreError) -> String {
    match error {
        StoreError::InvalidValue { field, .. } => field.to_string(),
        other => panic!("expected an invalid value, got {other:?}"),
    }
}

/* -------------------------------- sources ---------------------------------- */

#[test]
fn sources_are_read_back_in_the_order_they_were_written() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_review_sources(
            review,
            vec![
                NewReviewSource {
                    kind: SourceKind::Ticket,
                    label: "ABC-231 · Retry webhook deliveries for 24 hours".to_string(),
                    link: Some("https://linear.app/acme/issue/ABC-231".to_string()),
                    status: None,
                    note: None,
                },
                NewReviewSource {
                    kind: SourceKind::Description,
                    label: "The pull request description".to_string(),
                    link: None,
                    status: None,
                    note: None,
                },
                NewReviewSource {
                    kind: SourceKind::Comment,
                    label: "2 earlier comments on the pull request".to_string(),
                    link: None,
                    status: None,
                    note: None,
                },
            ],
        )
        .expect("the sources");

    assert_eq!(written.len(), 3);
    assert_eq!(written[0].position, 1);
    assert_eq!(written[0].kind, SourceKind::Ticket);
    assert_eq!(written[2].kind, SourceKind::Comment);
    assert_eq!(
        written,
        store.list_review_sources(review).unwrap(),
        "what the write returns is what a read returns"
    );
}

#[test]
fn writing_sources_again_replaces_the_whole_list() {
    let (_dir, store, review) = seeded();

    store
        .put_review_sources(
            review,
            vec![
                NewReviewSource {
                    kind: SourceKind::Ticket,
                    label: "ABC-231".to_string(),
                    link: None,
                    status: None,
                    note: None,
                },
                NewReviewSource {
                    kind: SourceKind::Comment,
                    label: "one comment".to_string(),
                    link: None,
                    status: None,
                    note: None,
                },
            ],
        )
        .unwrap();

    let second = store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Description,
                label: "The pull request description".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .unwrap();

    assert_eq!(second.len(), 1, "the list is the statement, not an append");
    assert_eq!(second[0].position, 1, "and the positions start again at 1");
}

#[test]
fn an_empty_list_clears_the_sources() {
    let (_dir, store, review) = seeded();

    store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "ABC-231".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .unwrap();

    assert!(
        store
            .put_review_sources(review, Vec::new())
            .unwrap()
            .is_empty(),
        "removing an entry is restating the list, including as nothing"
    );
}

#[test]
fn a_source_with_no_label_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "   ".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .expect_err("a source with nothing to call it");

    assert_eq!(invalid_field(refused), "sources.label");
}

#[test]
fn a_blank_link_is_no_link_rather_than_an_empty_one() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "ABC-231".to_string(),
                link: Some("  ".to_string()),
                status: None,
                note: None,
            }],
        )
        .unwrap();

    assert_eq!(written[0].link, None);
}

#[test]
fn sources_for_a_review_that_is_not_there_are_refused() {
    let (_dir, store, _review) = seeded();

    let refused = store
        .put_review_sources(
            9_999,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "ABC-231".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .expect_err("no such review");

    assert!(
        matches!(
            refused,
            StoreError::UnknownParent {
                parent: "review",
                ..
            }
        ),
        "got {refused:?}"
    );
}

#[test]
fn clearing_the_list_of_a_review_that_is_not_there_is_refused_too() {
    let (_dir, store, _review) = seeded();

    // The delete-then-insert shape means an empty list touches no foreign key at all, so
    // without the explicit check this would report success for a review nobody has.
    let refused = store
        .put_review_sources(9_999, Vec::new())
        .expect_err("no such review");

    assert!(
        matches!(
            refused,
            StoreError::UnknownParent {
                parent: "review",
                ..
            }
        ),
        "got {refused:?}"
    );
}

/* ------------------------------- criteria ----------------------------------- */

/// Board A2's three criteria: met with a place, not met with a finding, not in this
/// pull request with a note.
fn board_a2_criteria() -> Vec<NewAcceptanceCriterion> {
    vec![
        NewAcceptanceCriterion {
            text: "Failed deliveries are retried for up to 24 hours.".to_string(),
            status: CriterionStatus::Met,
            finding_seq: None,
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(61),
            note: None,
        },
        NewAcceptanceCriterion {
            text: "A customer never processes the same event twice.".to_string(),
            status: CriterionStatus::NotMet,
            finding_seq: Some(1),
            file: None,
            line: None,
            note: None,
        },
        NewAcceptanceCriterion {
            text: "The customer can see which deliveries failed.".to_string(),
            status: CriterionStatus::NotInThisPr,
            finding_seq: None,
            file: None,
            line: None,
            note: Some("The description says it follows in ABC-240".to_string()),
        },
    ]
}

#[test]
fn the_three_statuses_each_carry_their_own_kind_of_pointer() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_acceptance_criteria(review, board_a2_criteria())
        .expect("the criteria");

    assert_eq!(written.len(), 3);
    assert_eq!(written[0].status, CriterionStatus::Met);
    assert_eq!(written[0].file.as_deref(), Some("jobs/DeliverWebhook.php"));
    assert_eq!(written[0].line, Some(61));
    assert_eq!(written[1].status, CriterionStatus::NotMet);
    assert_eq!(written[1].finding_seq, Some(1));
    assert_eq!(written[2].status, CriterionStatus::NotInThisPr);
    assert_eq!(
        written[2].note.as_deref(),
        Some("The description says it follows in ABC-240")
    );
    assert!(written.iter().all(AcceptanceCriterion::has_pointer));
}

#[test]
fn a_criterion_recorded_as_not_met_must_name_a_finding() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_acceptance_criteria(
            review,
            vec![NewAcceptanceCriterion {
                text: "A customer never processes the same event twice.".to_string(),
                status: CriterionStatus::NotMet,
                finding_seq: None,
                file: None,
                line: None,
                // A note is a pointer, and it is still not a finding: a review that says
                // the change fails a criterion and raises nothing about it posts a review
                // that never mentions the failure.
                note: Some("it just does not".to_string()),
            }],
        )
        .expect_err("not met with no finding");

    assert_eq!(invalid_field(refused), "criteria.finding_seq");
    assert!(
        store.list_acceptance_criteria(review).unwrap().is_empty(),
        "and nothing was written: every refusal is computed before the write"
    );
}

#[test]
fn a_criterion_cannot_point_at_a_finding_on_another_review() {
    let (_dir, store, review) = seeded();
    let other = store
        .create_review(new_review("acme/portal", 413, "aaaaaaa", 1))
        .unwrap()
        .id;

    let refused = store
        .put_acceptance_criteria(
            other,
            vec![NewAcceptanceCriterion {
                text: "A customer never processes the same event twice.".to_string(),
                status: CriterionStatus::NotMet,
                finding_seq: Some(1),
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect_err("review 413 has no finding 1");

    assert!(
        matches!(
            &refused,
            StoreError::UnknownParent {
                parent: "finding",
                ..
            }
        ),
        "got {refused:?}"
    );
    assert_eq!(
        store.list_findings(review).unwrap().len(),
        3,
        "the other review's findings are untouched"
    );
}

#[test]
fn a_criterion_may_carry_one_pointer_and_not_two() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_acceptance_criteria(
            review,
            vec![NewAcceptanceCriterion {
                text: "Failed deliveries are retried for up to 24 hours.".to_string(),
                status: CriterionStatus::Met,
                finding_seq: None,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(61),
                note: Some("and also in the test".to_string()),
            }],
        )
        .expect_err("two pointers");

    assert_eq!(invalid_field(refused), "criteria.finding_seq");
}

#[test]
fn a_line_with_no_file_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_acceptance_criteria(
            review,
            vec![NewAcceptanceCriterion {
                text: "Failed deliveries are retried for up to 24 hours.".to_string(),
                status: CriterionStatus::Met,
                finding_seq: None,
                file: None,
                line: Some(61),
                note: None,
            }],
        )
        .expect_err("a line pointing nowhere");

    assert_eq!(invalid_field(refused), "criteria.line");
}

#[test]
fn a_criterion_with_no_text_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_acceptance_criteria(
            review,
            vec![NewAcceptanceCriterion {
                text: " ".to_string(),
                status: CriterionStatus::Met,
                finding_seq: None,
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect_err("a status against nothing");

    assert_eq!(invalid_field(refused), "criteria.text");
}

#[test]
fn a_criterion_that_is_met_needs_no_pointer_at_all() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_acceptance_criteria(
            review,
            vec![NewAcceptanceCriterion {
                text: "Failed deliveries are retried for up to 24 hours.".to_string(),
                status: CriterionStatus::Met,
                finding_seq: None,
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect("met with nothing to point at is thin, not wrong");

    assert!(!written[0].has_pointer());
}

/* --------------------------------- files ------------------------------------ */

/// Board A2's five rows, the last of them an area that was not reviewed.
fn board_a2_files() -> Vec<NewFileChange> {
    vec![
        NewFileChange {
            path: "jobs/DeliverWebhook.php".to_string(),
            summary: "The job now releases itself back to the queue when a delivery fails."
                .to_string(),
            lines_added: 96,
            lines_removed: 22,
            checked: FileCheck::ReadInFull,
            how_checked: Some("Read in full. Compared with 2 sibling senders.".to_string()),
        },
        NewFileChange {
            path: "tests/WebhookRetryTest.php".to_string(),
            summary: "A new test: a failed delivery is sent again.".to_string(),
            lines_added: 61,
            lines_removed: 0,
            checked: FileCheck::Run,
            how_checked: Some("Run. Run again with the fix reverted, and it failed.".to_string()),
        },
        NewFileChange {
            path: "config/webhooks.php".to_string(),
            summary: "Adds the backoff schedule.".to_string(),
            lines_added: 14,
            lines_removed: 0,
            checked: FileCheck::ReadInFull,
            how_checked: Some("Read in full.".to_string()),
        },
        NewFileChange {
            path: "migrations/add_attempts.php".to_string(),
            summary: "Adds an attempts column to deliveries.".to_string(),
            lines_added: 21,
            lines_removed: 0,
            checked: FileCheck::ReadInFull,
            how_checked: Some("Read in full.".to_string()),
        },
        NewFileChange {
            path: "resources/admin/**".to_string(),
            summary: "Not changed by this pull request.".to_string(),
            lines_added: 0,
            lines_removed: 0,
            checked: FileCheck::NotReviewed,
            how_checked: Some("Not reviewed. The change does not touch it.".to_string()),
        },
    ]
}

#[test]
fn the_change_map_keeps_its_order_and_its_counts() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_file_changes(review, board_a2_files())
        .expect("the change map");

    assert_eq!(written.len(), 5);
    assert_eq!(written[0].path, "jobs/DeliverWebhook.php");
    assert_eq!((written[0].lines_added, written[0].lines_removed), (96, 22));
    assert_eq!(written[1].checked, FileCheck::Run);
    assert_eq!(written[4].checked, FileCheck::NotReviewed);
    assert_eq!(
        written.iter().map(|file| file.position).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

#[test]
fn an_area_that_was_not_reviewed_must_say_why() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_file_changes(
            review,
            vec![NewFileChange {
                path: "resources/admin/**".to_string(),
                summary: "Not changed by this pull request.".to_string(),
                lines_added: 0,
                lines_removed: 0,
                checked: FileCheck::NotReviewed,
                how_checked: None,
            }],
        )
        .expect_err("a gap with no reason");

    assert_eq!(invalid_field(refused), "files.how_checked");
}

#[test]
fn a_file_reviewed_normally_need_not_say_how() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_file_changes(
            review,
            vec![NewFileChange {
                path: "config/webhooks.php".to_string(),
                summary: "Adds the backoff schedule.".to_string(),
                lines_added: 14,
                lines_removed: 0,
                checked: FileCheck::ReadInFull,
                how_checked: None,
            }],
        )
        .expect("the sentence is worth having and is not owed");

    assert_eq!(written[0].how_checked, None);
}

#[test]
fn the_same_path_twice_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_file_changes(
            review,
            vec![
                NewFileChange {
                    path: "config/webhooks.php".to_string(),
                    summary: "Adds the backoff schedule.".to_string(),
                    lines_added: 14,
                    lines_removed: 0,
                    checked: FileCheck::ReadInFull,
                    how_checked: None,
                },
                NewFileChange {
                    path: "config/webhooks.php".to_string(),
                    summary: "And was not reviewed.".to_string(),
                    lines_added: 0,
                    lines_removed: 0,
                    checked: FileCheck::NotReviewed,
                    how_checked: Some("ran out of time".to_string()),
                },
            ],
        )
        .expect_err("two coverage claims about one file");

    assert_eq!(invalid_field(refused), "files.path");
}

#[test]
fn a_negative_line_count_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_file_changes(
            review,
            vec![NewFileChange {
                path: "config/webhooks.php".to_string(),
                summary: "Adds the backoff schedule.".to_string(),
                lines_added: -1,
                lines_removed: 0,
                checked: FileCheck::ReadInFull,
                how_checked: None,
            }],
        )
        .expect_err("a count of lines is never negative");

    assert_eq!(invalid_field(refused), "files.lines_added");
}

#[test]
fn the_findings_on_a_file_come_from_the_findings_and_not_from_the_entry() {
    let (_dir, store, review) = seeded();
    store.put_file_changes(review, board_a2_files()).unwrap();

    assert_eq!(
        store
            .findings_on_path(review, "jobs/DeliverWebhook.php")
            .unwrap(),
        vec![1, 2],
        "two findings name that file"
    );
    assert_eq!(
        store
            .findings_on_path(review, "config/webhooks.php")
            .unwrap(),
        vec![3]
    );
    assert_eq!(
        store
            .findings_on_path(review, "tests/WebhookRetryTest.php")
            .unwrap(),
        Vec::<i64>::new(),
        "and a file with no finding has none rather than a stale one"
    );
    assert_eq!(
        store
            .findings_on_path(review, "resources/admin/**")
            .unwrap(),
        Vec::<i64>::new(),
        "an area that was not reviewed cannot have findings on it"
    );

    // Adding a finding changes the answer without the change map being rewritten, which
    // is the whole of why it is not stored: the two cannot drift apart.
    store
        .add_finding(
            review,
            NewFinding {
                lens: "duplication".to_string(),
                severity: Severity::Nit,
                file: Some("tests/WebhookRetryTest.php".to_string()),
                line: Some(12),
                claim: "the test asserts the delay twice".to_string(),
                evidence: None,
                disposition: review_desk_store::Disposition::InDraft,
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        store
            .findings_on_path(review, "tests/WebhookRetryTest.php")
            .unwrap(),
        vec![4]
    );
}

/* ---------------------------- description lint ------------------------------ */

#[test]
fn the_description_lint_is_a_list_of_named_results() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_description_lint(
            review,
            vec![
                NewDescriptionLintResult {
                    label: "Description length".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: Some("1,840 of 2,568 characters".to_string()),
                },
                NewDescriptionLintResult {
                    label: "Working notes".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: Some("In a comment, linked from the description".to_string()),
                },
                NewDescriptionLintResult {
                    label: "Template".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: Some("All 5 headings present".to_string()),
                },
            ],
        )
        .expect("the results");

    assert_eq!(written.len(), 3);
    assert_eq!(written[0].label, "Description length");
    assert_eq!(written[2].detail.as_deref(), Some("All 5 headings present"));
}

#[test]
fn the_overall_verdict_is_derived_from_the_results() {
    let (_dir, store, review) = seeded();

    assert_eq!(
        store
            .review_provenance(review)
            .unwrap()
            .description_lint_passes(),
        None,
        "a lint that has not run has no verdict, which is not the same as failing"
    );

    store
        .put_description_lint(
            review,
            vec![NewDescriptionLintResult {
                label: "Template".to_string(),
                status: DescriptionLintStatus::Warn,
                detail: Some("4 of 5 headings present".to_string()),
            }],
        )
        .unwrap();
    assert_eq!(
        store
            .review_provenance(review)
            .unwrap()
            .description_lint_passes(),
        Some(true),
        "a warning does not fail it, matching the draft lint's own rule"
    );

    store
        .put_description_lint(
            review,
            vec![
                NewDescriptionLintResult {
                    label: "Template".to_string(),
                    status: DescriptionLintStatus::Warn,
                    detail: None,
                },
                NewDescriptionLintResult {
                    label: "Description length".to_string(),
                    status: DescriptionLintStatus::Fail,
                    detail: Some("5,900 of 2,568 characters".to_string()),
                },
            ],
        )
        .unwrap();
    assert_eq!(
        store
            .review_provenance(review)
            .unwrap()
            .description_lint_passes(),
        Some(false),
        "and one failure fails it, with no stored verdict able to say otherwise"
    );
}

#[test]
fn a_lint_result_with_no_label_is_refused() {
    let (_dir, store, review) = seeded();

    let refused = store
        .put_description_lint(
            review,
            vec![NewDescriptionLintResult {
                label: String::new(),
                status: DescriptionLintStatus::Pass,
                detail: Some("1,840 of 2,568 characters".to_string()),
            }],
        )
        .expect_err("a result nothing names");

    assert_eq!(invalid_field(refused), "description_lint.label");
}

/* ------------------------------ all of it ----------------------------------- */

#[test]
fn a_review_that_recorded_none_of_it_reads_back_empty() {
    let (_dir, store, review) = seeded();

    let provenance = store.review_provenance(review).expect("the provenance");

    assert!(provenance.is_empty());
    assert_eq!(provenance.review_id, review);
    assert!(provenance.sources.is_empty());
    assert!(provenance.criteria.is_empty());
    assert!(provenance.files.is_empty());
    assert!(provenance.description_lint.is_empty());
    assert_eq!(provenance.not_reviewed().count(), 0);
}

#[test]
fn one_read_answers_all_four() {
    let (_dir, store, review) = seeded();

    store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "ABC-231".to_string(),
                link: None,
                status: None,
                note: None,
            }],
        )
        .unwrap();
    store.put_file_changes(review, board_a2_files()).unwrap();
    store
        .put_acceptance_criteria(review, board_a2_criteria())
        .unwrap();
    store
        .put_description_lint(
            review,
            vec![NewDescriptionLintResult {
                label: "Template".to_string(),
                status: DescriptionLintStatus::Pass,
                detail: Some("All 5 headings present".to_string()),
            }],
        )
        .unwrap();

    let provenance = store.review_provenance(review).expect("the provenance");

    assert!(!provenance.is_empty());
    assert_eq!(provenance.sources.len(), 1);
    assert_eq!(provenance.criteria.len(), 3);
    assert_eq!(provenance.files.len(), 5);
    assert_eq!(provenance.description_lint.len(), 1);
    assert_eq!(
        provenance
            .not_reviewed()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec!["resources/admin/**"],
        "one area, highlighted, with its reason beside it"
    );
}

#[test]
fn every_closed_set_names_its_own_values() {
    // The error a caller gets has to say what it would have accepted, which is the whole
    // reason these are enums rather than strings.
    for (text, expected) in [
        ("ticket, description, comment", SourceKind::valid_values()),
        (
            "met, not_met, not_in_this_pr",
            CriterionStatus::valid_values(),
        ),
        (
            "read_in_full, read_in_part, run, not_reviewed",
            FileCheck::valid_values(),
        ),
        ("pass, warn, fail", DescriptionLintStatus::valid_values()),
    ] {
        assert_eq!(text, expected);
    }

    assert!("sideways".parse::<SourceKind>().is_err());
    assert!("partly".parse::<CriterionStatus>().is_err());
    assert!("skimmed".parse::<FileCheck>().is_err());
    assert!("warning".parse::<DescriptionLintStatus>().is_err());
}

/* --------------------------- across rounds (DEV-873) ------------------------- */

#[test]
fn a_criterion_can_point_at_a_finding_carried_into_a_later_round() {
    // **Since migration 7 a finding's number is its identity across rounds**, so a
    // criterion naming F1 in round 2 names the same F1 round 1 raised. This is the case
    // that would have broken quietly: `review_criteria`'s foreign key is to
    // `findings (review_id, seq)`, and a carried finding is a *new row* under the later
    // review keeping its number — so the key resolves only because the carry writes it.
    let (_dir, store) = temp_store();
    let first = store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("round 1")
        .review
        .id;
    let seq = store
        .add_finding(
            first,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(58),
                claim: "A retried delivery is sent without an idempotency key".to_string(),
                evidence: None,
                disposition: review_desk_store::Disposition::InDraft,
                disposition_reason: None,
                ..Default::default()
            },
        )
        .expect("the finding")
        .seq;

    // Round one is posted, so the next commit is round 2 rather than round 1 moved —
    // DEV-962's rule, and what a real pull request does between two rounds.
    posted(&store, first);
    let second = store
        .open_review("acme/portal", 412, "bbbbbbb")
        .expect("round 2")
        .review
        .id;
    assert_ne!(first, second, "a new head commit is a new round");

    store
        .carry_finding(
            first,
            second,
            seq,
            &FindingCarry {
                status: RoundStatus::StillOpen,
                by: DecidedBy::Agent,
                checked: None,
                commit: None,
            },
        )
        .expect("carry it into round 2");

    let written = store
        .put_acceptance_criteria(
            second,
            vec![NewAcceptanceCriterion {
                text: "A customer never processes the same event twice.".to_string(),
                status: CriterionStatus::NotMet,
                finding_seq: Some(seq),
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect("round 2's criterion points at the carried finding");

    assert_eq!(written[0].finding_seq, Some(seq));
    assert_eq!(
        written[0].status,
        CriterionStatus::NotMet,
        "still not met, and the finding saying so is the same finding"
    );
}

#[test]
fn a_criterion_cannot_point_at_a_finding_that_was_not_carried() {
    // The other side of the same rule, and it is the right refusal: a criterion whose
    // finding was fixed and left behind is a criterion whose status should have changed to
    // `met`, not one pointing at a number that means nothing in this round.
    let (_dir, store) = temp_store();
    let first = store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("round 1")
        .review
        .id;
    let seq = store
        .add_finding(
            first,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(58),
                claim: "A retried delivery is sent without an idempotency key".to_string(),
                evidence: None,
                disposition: review_desk_store::Disposition::InDraft,
                disposition_reason: None,
                ..Default::default()
            },
        )
        .expect("the finding")
        .seq;

    // Round one is posted, so the next commit is round 2 rather than round 1 moved —
    // DEV-962's rule, and what a real pull request does between two rounds.
    posted(&store, first);
    let second = store
        .open_review("acme/portal", 412, "bbbbbbb")
        .expect("round 2")
        .review
        .id;

    let refused = store
        .put_acceptance_criteria(
            second,
            vec![NewAcceptanceCriterion {
                text: "A customer never processes the same event twice.".to_string(),
                status: CriterionStatus::NotMet,
                finding_seq: Some(seq),
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect_err("round 2 has no such finding, because it was not carried");

    assert!(
        matches!(
            refused,
            StoreError::UnknownParent {
                parent: "finding",
                ..
            }
        ),
        "got {refused:?}"
    );
}

#[test]
fn a_carried_finding_brings_no_criteria_with_it() {
    // **A criterion is not carried, and that is a decision.** It belongs to the review,
    // not to the finding: it is this round's assessment of what the ticket asks for, and
    // copying it forward would assert that round 2 reached the same verdict with no agent
    // having looked.
    //
    // DEV-873's `every_column_of_a_finding_is_decided_about_when_it_is_carried` forces
    // that choice for anything hanging off a finding, but it finds those tables by the
    // name prefix `finding%` — which `review_criteria` does not carry, so the guard cannot
    // ask. This is the answer, enforced rather than asserted in a comment.
    let (_dir, store) = temp_store();
    let first = store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("round 1")
        .review
        .id;
    let seq = store
        .add_finding(
            first,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(58),
                claim: "A retried delivery is sent without an idempotency key".to_string(),
                evidence: None,
                disposition: review_desk_store::Disposition::InDraft,
                disposition_reason: None,
                ..Default::default()
            },
        )
        .expect("the finding")
        .seq;
    store
        .put_acceptance_criteria(
            first,
            vec![NewAcceptanceCriterion {
                text: "A customer never processes the same event twice.".to_string(),
                status: CriterionStatus::NotMet,
                finding_seq: Some(seq),
                file: None,
                line: None,
                note: None,
            }],
        )
        .expect("round 1's criterion");

    // Round one is posted, so the next commit is round 2 rather than round 1 moved —
    // DEV-962's rule, and what a real pull request does between two rounds.
    posted(&store, first);
    let second = store
        .open_review("acme/portal", 412, "bbbbbbb")
        .expect("round 2")
        .review
        .id;
    store
        .carry_finding(
            first,
            second,
            seq,
            &FindingCarry {
                status: RoundStatus::StillOpen,
                by: DecidedBy::Agent,
                checked: None,
                commit: None,
            },
        )
        .expect("carry the finding");

    assert!(
        store.list_acceptance_criteria(second).unwrap().is_empty(),
        "round 2 has to be told what the ticket asks for; the carry does not say it"
    );
    assert_eq!(
        store.list_acceptance_criteria(first).unwrap().len(),
        1,
        "and round 1 keeps its own"
    );

    // The same for the other three lists: none of them hangs off a finding at all, and a
    // carry is about one finding.
    let provenance = store.review_provenance(second).unwrap();
    assert!(
        provenance.is_empty(),
        "a carry moves a finding, not the review's understanding: {provenance:?}"
    );
}

/// What a source says about the ticket it came from — DEV-917.
///
/// "Already decided" is three levels now and its third is a table, which needs two columns
/// the record did not hold: the state the session read off the ticket, and the one line
/// saying what the source tells us. Both are optional, and a row with neither is the row
/// every source recorded before this change draws.
#[test]
fn a_source_carries_the_state_and_the_line_the_session_read() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_review_sources(
            review,
            vec![
                NewReviewSource {
                    kind: SourceKind::Ticket,
                    label: "ABC-231 · Retry webhook deliveries for 24 hours".to_string(),
                    link: None,
                    status: Some("Backlog".to_string()),
                    note: Some("Names the helper this change deletes".to_string()),
                },
                NewReviewSource {
                    kind: SourceKind::Comment,
                    label: "2 earlier comments on the pull request".to_string(),
                    link: None,
                    status: None,
                    note: None,
                },
            ],
        )
        .expect("the sources");

    assert_eq!(written[0].status.as_deref(), Some("Backlog"));
    assert_eq!(
        written[0].note.as_deref(),
        Some("Names the helper this change deletes")
    );
    assert_eq!(
        (written[1].status.as_deref(), written[1].note.as_deref()),
        (None, None),
        "a source that reported neither shows its reference alone"
    );
    assert_eq!(
        store.list_review_sources(review).unwrap(),
        written,
        "and both are read back as they were written"
    );
}

/// A state or a note of spaces is nothing, and is recorded as nothing.
///
/// The same rule the label follows, for the same reason: a field nobody filled in wearing
/// the clothes of one somebody did would draw an empty cell that reads as a fetched blank.
#[test]
fn a_blank_state_or_note_on_a_source_is_recorded_as_absent() {
    let (_dir, store, review) = seeded();

    let written = store
        .put_review_sources(
            review,
            vec![NewReviewSource {
                kind: SourceKind::Ticket,
                label: "ABC-231 · the ticket".to_string(),
                link: None,
                status: Some("   ".to_string()),
                note: Some(String::new()),
            }],
        )
        .expect("the sources");

    assert_eq!(
        (written[0].status.clone(), written[0].note.clone()),
        (None, None)
    );
}
