//! The twelve rows the three approved boards draw, held by the brief.
//!
//! Patrick approved a page per step on 10 October 2026 and the record could feed none of
//! them. This file is what holds the record able to: a brief whole, round-tripped; the
//! refusals that keep the problem numbers meaningful; and the one guarantee the whole
//! structure exists for — **that the problem half can be handed to a session without
//! handing it the answer.**

mod common;

use std::path::{Path, PathBuf};

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, BlindPassState, Brief, BriefBlindPass, BriefDisagreement, BriefFlowBox, BriefOpenChoice,
    BriefOption, BriefOptionVerdict, BriefPart, BriefPartFile, BriefProblem, BriefSupportTable,
    NewBrief, NewFinding, OptionProposer, ProblemVerdict, Review, Severity, Store, StoreError,
};

/// A review with four findings, so the brief's pointers have something to point at.
fn seed(store: &Store) -> Review {
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("a review");
    for claim in [
        "no idempotency key",
        "no way to replay",
        "backoff repeated",
        "unused import",
    ] {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "sibling diff".to_string(),
                    severity: Severity::Should,
                    claim: claim.to_string(),
                    ..Default::default()
                },
            )
            .expect("a finding");
    }
    review
}

fn flow_box(title: &str, problem: Option<i64>) -> BriefFlowBox {
    BriefFlowBox {
        title: title.to_string(),
        note: Some("a short note".to_string()),
        problem,
    }
}

fn problem(number: i64) -> BriefProblem {
    BriefProblem {
        number,
        label: Some(format!("problem {number}")),
        was_wrong: format!("what was wrong, number {number}"),
        now_fixed: Some(format!("what it is now, number {number}")),
        detail_title: Some(format!("the detail behind {number}")),
        detail_before: vec!["a line of before".to_string()],
        detail_after: vec!["a line of after".to_string()],
    }
}

fn option(
    key: &str,
    proposed_by: OptionProposer,
    verdicts: Vec<BriefOptionVerdict>,
) -> BriefOption {
    BriefOption {
        key: key.to_string(),
        title: format!("option {key}"),
        argument_for: "the argument for it".to_string(),
        argument_against: "the argument against it".to_string(),
        chosen: key == "B",
        proposed_by,
        verdicts,
    }
}

fn verdict(problem: i64, verdict: ProblemVerdict, why: Option<&str>) -> BriefOptionVerdict {
    BriefOptionVerdict {
        problem,
        verdict,
        why: why.map(str::to_string),
    }
}

/// A brief carrying every one of the twelve rows.
fn whole_brief() -> NewBrief {
    NewBrief {
        problem: "a webhook delivery fails once and is never tried again".to_string(),
        diagram: Some("flowchart LR\n  A --> B".to_string()),
        approach_verdict: Some("B is the right shape".to_string()),
        provenance: Some("from the ticket and the code".to_string()),
        headline: Some("Deliveries move to a queue, so one that fails is retried.".to_string()),
        how_it_works_today: Some("A delivery is sent inside the request.".to_string()),
        flow_before_caption: Some("The delivery is sent inline".to_string()),
        flow_after_caption: Some("The delivery is queued".to_string()),
        flow_before: vec![
            vec![flow_box("An event happens", None)],
            // A split row: two boxes the page draws side by side.
            vec![
                flow_box("The endpoint answers", Some(2)),
                flow_box("The endpoint is down", Some(1)),
            ],
        ],
        flow_after: vec![vec![flow_box("A worker sends it", Some(1))]],
        problems: vec![problem(1), problem(2)],
        support_table: Some(BriefSupportTable {
            title: Some("The idea: two questions, each with one owner".to_string()),
            columns: vec!["The question".to_string(), "Who answers it".to_string()],
            rows: vec![
                vec![
                    "Was it attempted?".to_string(),
                    "The delivery row".to_string(),
                ],
                vec!["Try again?".to_string(), "The queue worker".to_string()],
            ],
        }),
        options: vec![
            option(
                "A",
                OptionProposer::Ticket,
                vec![
                    verdict(1, ProblemVerdict::Partly, Some("covers a blip")),
                    verdict(2, ProblemVerdict::Stays, Some("nothing is written down")),
                ],
            ),
            option(
                "B",
                OptionProposer::Both,
                vec![
                    verdict(1, ProblemVerdict::Fixed, Some("backoff to 24 hours")),
                    verdict(2, ProblemVerdict::Fixed, Some("a row per attempt")),
                ],
            ),
            option(
                "C",
                OptionProposer::BlindPass,
                vec![verdict(1, ProblemVerdict::NotAssessed, None)],
            ),
        ],
        blind_pass: Some(BriefBlindPass {
            given: "A web application delivers events inside the request.".to_string(),
            pick_key: Some("B".to_string()),
            pick_why: Some("It answers both problems at once.".to_string()),
            looked_up: "nothing".to_string(),
            questions: vec![
                "Must a retry be idempotent?".to_string(),
                "How long is a delivery worth sending?".to_string(),
            ],
        }),
        parts: vec![
            BriefPart {
                title: "The delivery row".to_string(),
                summary: "One row per attempt.".to_string(),
                one_line: Some("One row per attempt, with what became of it.".to_string()),
                fixes: vec![2],
                files: vec![BriefPartFile {
                    path: "app/Models/WebhookDelivery.php".to_string(),
                    lines_added: 74,
                    lines_removed: 0,
                    in_diff: true,
                }],
            },
            BriefPart {
                title: "The queue worker".to_string(),
                summary: "Sends a delivery and retries it.".to_string(),
                one_line: None,
                fixes: vec![1, 2],
                files: vec![
                    BriefPartFile {
                        path: "jobs/DeliverWebhook.php".to_string(),
                        lines_added: 96,
                        lines_removed: 31,
                        in_diff: true,
                    },
                    // Outside the diff: a file the pull request did not change, which a
                    // part still has to be able to claim.
                    BriefPartFile {
                        path: "app/Support/Backoff.php".to_string(),
                        lines_added: 0,
                        lines_removed: 0,
                        in_diff: false,
                    },
                ],
            },
        ],
        open_choices: vec![BriefOpenChoice {
            left_open: "How long a failed delivery is kept".to_string(),
            chosen: "Thirty days".to_string(),
            why: "The ticket stops at the retry window.".to_string(),
            departs_from_ticket: true,
            finding_seq: Some(3),
        }],
        disagreements: vec![BriefDisagreement {
            description_says: "A retry carries the first attempt's key.".to_string(),
            code_does: "The key is generated per attempt.".to_string(),
            finding_seq: 2,
        }],
        ..Default::default()
    }
}

/// What a refusal said, or a panic naming what came back instead.
#[track_caller]
fn refusal(result: Result<Brief, StoreError>) -> (&'static str, String, String) {
    match result {
        Err(StoreError::InvalidValue {
            field,
            value,
            expected,
        }) => (field, value, expected),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_brief_carrying_every_row_reads_back_exactly_as_it_was_written() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let written = whole_brief();
    let stored = store
        .put_brief(review.id, written.clone())
        .expect("a brief");

    // Field by field rather than one comparison, so a failure says which row broke.
    assert_eq!(stored.headline, written.headline);
    assert_eq!(stored.how_it_works_today, written.how_it_works_today);
    assert_eq!(stored.flow_before_caption, written.flow_before_caption);
    assert_eq!(stored.flow_after_caption, written.flow_after_caption);
    assert_eq!(stored.flow_before, written.flow_before);
    assert_eq!(stored.flow_after, written.flow_after);
    assert_eq!(stored.problems, written.problems);
    assert_eq!(stored.support_table, written.support_table);
    assert_eq!(stored.blind_pass, written.blind_pass);
    assert_eq!(stored.parts, written.parts);
    assert_eq!(stored.open_choices, written.open_choices);
    assert_eq!(stored.disagreements, written.disagreements);
    assert_eq!(stored.options, written.options);

    // And again on a fresh read, so nothing above is the write's return value flattering
    // itself.
    assert_eq!(store.get_brief(review.id).expect("the brief"), stored);
}

#[test]
fn the_split_row_survives_as_a_row_of_two() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store.put_brief(review.id, whole_brief()).expect("a brief");
    assert_eq!(stored.flow_before.len(), 2, "two rows");
    assert_eq!(stored.flow_before[0].len(), 1, "the first row is one box");
    assert_eq!(
        stored.flow_before[1].len(),
        2,
        "the second is the split row"
    );
}

#[test]
fn a_brief_written_the_old_way_reads_back_unchanged() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // Three free-text fields and options with a paragraph each way, which is the whole of
    // what a brief could hold before this change.
    let old_way = NewBrief {
        problem: "a webhook delivery fails once and is never tried again".to_string(),
        diagram: Some("flowchart LR\n  A[Event] --> B[Lost]".to_string()),
        provenance: Some("from the ticket and the code".to_string()),
        options: vec![option("A", OptionProposer::Ticket, Vec::new())],
        ..Default::default()
    };
    let stored = store
        .put_brief(review.id, old_way.clone())
        .expect("a brief");

    assert_eq!(stored.problem, old_way.problem);
    assert_eq!(stored.diagram, old_way.diagram);
    assert_eq!(stored.provenance, old_way.provenance);
    assert_eq!(stored.options, old_way.options);

    // And every new field is its empty answer rather than an invented one.
    assert_eq!(stored.headline, None);
    assert_eq!(stored.how_it_works_today, None);
    assert_eq!(stored.flow_before_caption, None);
    assert_eq!(stored.flow_after_caption, None);
    assert!(stored.flow_before.is_empty());
    assert!(stored.flow_after.is_empty());
    assert!(stored.problems.is_empty());
    assert_eq!(stored.support_table, None);
    assert_eq!(stored.blind_pass, None);
    assert!(stored.parts.is_empty());
    assert!(stored.open_choices.is_empty());
    assert!(stored.disagreements.is_empty());
    assert_eq!(stored.blind_pass_state(), BlindPassState::NotRun);
}

#[test]
fn an_option_written_before_this_change_is_read_as_the_tickets() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // What a caller on the older command line sends: no `proposed_by` at all.
    let sent: NewBrief = serde_json::from_str(
        r#"{"problem": "p", "options": [{"key": "A", "title": "t",
            "argument_for": "f", "argument_against": "a", "chosen": true}]}"#,
    )
    .expect("the older shape still deserialises");
    let stored = store.put_brief(review.id, sent).expect("a brief");

    // The ticket, because nothing had run a blind pass when such an option was written.
    assert_eq!(stored.options[0].proposed_by, OptionProposer::Ticket);
    assert!(stored.options[0].verdicts.is_empty());
}

#[test]
fn a_flow_row_of_four_boxes_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            flow_before: vec![vec![
                flow_box("one", None),
                flow_box("two", None),
                flow_box("three", None),
                flow_box("four", None),
            ]],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "flow_before");
    assert!(value.contains("row 1"), "{value}");
    assert!(value.contains('4'), "the refusal says how many: {value}");
    assert!(expected.contains("one to three"), "{expected}");
}

#[test]
fn an_empty_flow_row_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, _, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            flow_after: vec![Vec::new()],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "flow_after");
}

#[test]
fn a_flow_box_marking_a_problem_that_is_not_there_is_refused_by_field_and_number() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            flow_before: vec![vec![flow_box("a box marked 9", Some(9))]],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "flow_before.problem");
    assert!(value.contains('9'), "the number is in the refusal: {value}");
    assert!(expected.contains("1, 2"), "and what there is: {expected}");
}

#[test]
fn an_option_verdict_on_a_problem_that_is_not_there_is_refused_by_field_and_number() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            options: vec![option(
                "A",
                OptionProposer::Ticket,
                vec![verdict(7, ProblemVerdict::Fixed, Some("it does"))],
            )],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "options.verdicts.problem");
    assert!(value.contains('7'), "{value}");
}

#[test]
fn a_solution_part_fixing_a_problem_that_is_not_there_is_refused_by_field_and_number() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            parts: vec![BriefPart {
                title: "a part".to_string(),
                summary: "one line".to_string(),
                one_line: None,
                fixes: vec![4],
                files: Vec::new(),
            }],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "parts.fixes");
    assert!(value.contains('4'), "{value}");
}

#[test]
fn a_brief_that_numbers_no_problems_says_so_when_something_names_one() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (_, _, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            problem: "p".to_string(),
            flow_before: vec![vec![flow_box("a box marked 1", Some(1))]],
            ..Default::default()
        },
    ));
    assert!(
        expected.contains("numbers none"),
        "an empty list is its own message: {expected}"
    );
}

#[test]
fn two_problems_with_one_number_are_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            problems: vec![problem(1), problem(1)],
            flow_before: Vec::new(),
            flow_after: Vec::new(),
            options: Vec::new(),
            blind_pass: None,
            parts: Vec::new(),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "problems.number");
    assert!(value.contains('1'), "{value}");
}

#[test]
fn a_verdict_that_is_a_judgement_needs_a_reason_and_not_assessed_does_not() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            options: vec![option(
                "A",
                OptionProposer::Ticket,
                vec![verdict(1, ProblemVerdict::Fixed, None)],
            )],
            blind_pass: None,
            ..whole_brief()
        },
    ));
    assert_eq!(field, "options.verdicts.why");
    assert!(value.contains("problem 1"), "{value}");

    // And the one verdict that is the absence of a judgement needs none.
    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                options: vec![option(
                    "A",
                    OptionProposer::Ticket,
                    vec![verdict(1, ProblemVerdict::NotAssessed, None)],
                )],
                blind_pass: None,
                ..whole_brief()
            },
        )
        .expect("not assessed needs no reason");
    assert_eq!(stored.options[0].verdicts[0].why, None);
}

#[test]
fn a_ragged_row_of_the_supporting_table_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            support_table: Some(BriefSupportTable {
                title: None,
                columns: vec!["one".to_string(), "two".to_string()],
                rows: vec![vec!["only one cell".to_string()]],
            }),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "support_table.rows");
    assert!(value.contains("row 1"), "{value}");
    assert!(expected.contains('2'), "{expected}");
}

#[test]
fn a_supporting_table_with_a_heading_and_no_columns_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, _, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            support_table: Some(BriefSupportTable {
                title: Some("The idea".to_string()),
                columns: Vec::new(),
                rows: Vec::new(),
            }),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "support_table.columns");
    assert!(expected.contains("leave the table out"), "{expected}");
}

#[test]
fn a_blank_cell_of_the_supporting_table_is_an_answer() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                support_table: Some(BriefSupportTable {
                    title: None,
                    columns: vec!["question".to_string(), "owner".to_string()],
                    rows: vec![vec!["who owns this?".to_string(), String::new()]],
                }),
                ..whole_brief()
            },
        )
        .expect("a blank cell is allowed");
    let table = stored.support_table.expect("the table");
    assert_eq!(
        table.rows,
        vec![vec!["who owns this?".to_string(), String::new()]]
    );
}

#[test]
fn a_part_may_claim_a_file_the_pull_request_did_not_change() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store.put_brief(review.id, whole_brief()).expect("a brief");
    let outside = stored.parts[1]
        .files
        .iter()
        .find(|file| !file.in_diff)
        .expect("the file outside the diff");
    assert_eq!(outside.path, "app/Support/Backoff.php");
    assert_eq!((outside.lines_added, outside.lines_removed), (0, 0));
}

#[test]
fn a_file_outside_the_diff_with_lines_against_it_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            parts: vec![BriefPart {
                title: "a part".to_string(),
                summary: "one line".to_string(),
                one_line: None,
                fixes: Vec::new(),
                files: vec![BriefPartFile {
                    path: "app/Support/Backoff.php".to_string(),
                    lines_added: 4,
                    lines_removed: 0,
                    in_diff: false,
                }],
            }],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "parts.files.in_diff");
    assert!(value.contains("Backoff.php"), "{value}");
    assert!(expected.contains("did not change"), "{expected}");
}

#[test]
fn one_file_cannot_belong_to_two_parts() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let part = |title: &str| BriefPart {
        title: title.to_string(),
        summary: "one line".to_string(),
        one_line: None,
        fixes: Vec::new(),
        files: vec![BriefPartFile {
            path: "jobs/DeliverWebhook.php".to_string(),
            lines_added: 1,
            lines_removed: 0,
            in_diff: true,
        }],
    };
    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            parts: vec![part("first"), part("second")],
            ..whole_brief()
        },
    ));
    assert_eq!(field, "parts.files.path");
    assert!(value.contains("DeliverWebhook.php"), "{value}");
}

#[test]
fn the_three_states_of_a_blind_pass_are_distinguishable() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // Did not run: no record of one.
    let not_run = store
        .put_brief(
            review.id,
            NewBrief {
                blind_pass: None,
                options: vec![option("A", OptionProposer::Ticket, Vec::new())],
                ..whole_brief()
            },
        )
        .expect("a brief");
    assert_eq!(not_run.blind_pass_state(), BlindPassState::NotRun);
    assert_eq!(not_run.blind_pass, None);

    // Ran and found nothing: a record of one, and no option came from it.
    let found_nothing = store
        .put_brief(
            review.id,
            NewBrief {
                blind_pass: Some(BriefBlindPass {
                    given: "the problem, as handed over".to_string(),
                    pick_key: None,
                    pick_why: None,
                    looked_up: "nothing".to_string(),
                    questions: Vec::new(),
                }),
                options: vec![option("A", OptionProposer::Ticket, Vec::new())],
                ..whole_brief()
            },
        )
        .expect("a brief");
    assert_eq!(
        found_nothing.blind_pass_state(),
        BlindPassState::RanAndFoundNothing
    );
    assert!(found_nothing.blind_pass.is_some());

    // Ran: an option carries it.
    let ran = store.put_brief(review.id, whole_brief()).expect("a brief");
    assert_eq!(ran.blind_pass_state(), BlindPassState::Ran);
}

#[test]
fn an_option_proposed_by_both_counts_as_the_blind_pass_having_run() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                options: vec![option("B", OptionProposer::Both, Vec::new())],
                blind_pass: Some(BriefBlindPass {
                    given: "the problem, as handed over".to_string(),
                    pick_key: Some("B".to_string()),
                    pick_why: Some("it answers both".to_string()),
                    looked_up: "nothing".to_string(),
                    questions: Vec::new(),
                }),
                ..whole_brief()
            },
        )
        .expect("a brief");
    assert_eq!(stored.blind_pass_state(), BlindPassState::Ran);
}

#[test]
fn an_option_from_a_blind_pass_that_is_not_recorded_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // **The direction the first draft of this change missed.** `blind_pass_state` answers
    // `not_run` as soon as there is no pass on the record, so a brief with an option
    // claiming a pass and no pass to show would have the Alternatives page badge that
    // option "Blind" with no blind column to put it in — a brief disagreeing with its own
    // options, which is the one thing deriving the state is supposed to make impossible.
    for proposer in [OptionProposer::BlindPass, OptionProposer::Both] {
        let (field, value, _) = refusal(store.put_brief(
            review.id,
            NewBrief {
                blind_pass: None,
                options: vec![option("A", proposer, Vec::new())],
                ..whole_brief()
            },
        ));
        assert_eq!(field, "options.proposed_by");
        assert!(value.contains("option A"), "{value}");
    }

    // And with the pass recorded, the same brief is fine.
    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                options: vec![option("A", OptionProposer::BlindPass, Vec::new())],
                blind_pass: Some(BriefBlindPass {
                    given: "the problem, as handed over".to_string(),
                    pick_key: Some("A".to_string()),
                    pick_why: Some("it answers it".to_string()),
                    looked_up: "nothing".to_string(),
                    questions: Vec::new(),
                }),
                ..whole_brief()
            },
        )
        .expect("a recorded pass makes the option honest");
    assert_eq!(stored.blind_pass_state(), BlindPassState::Ran);
}

#[test]
fn a_blank_line_on_the_brief_is_refused_rather_than_stored() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // Migration 10 adds these four with `ALTER TABLE`, and SQLite cannot put a CHECK on a
    // table that already exists — so `put_brief` is the only layer there is. A blank
    // headline draws an empty line where the page's first sentence goes.
    for (field, brief) in [
        (
            "headline",
            NewBrief {
                headline: Some("   ".to_string()),
                ..whole_brief()
            },
        ),
        (
            "how_it_works_today",
            NewBrief {
                how_it_works_today: Some(String::new()),
                ..whole_brief()
            },
        ),
        (
            "flow_before_caption",
            NewBrief {
                flow_before_caption: Some("\t".to_string()),
                ..whole_brief()
            },
        ),
        (
            "flow_after_caption",
            NewBrief {
                flow_after_caption: Some(String::new()),
                ..whole_brief()
            },
        ),
    ] {
        let (refused, _, expected) = refusal(store.put_brief(review.id, brief));
        assert_eq!(refused, field);
        assert!(expected.contains("leave the field out"), "{expected}");
    }

    // Leaving it out is the way to say there is none, and that is still allowed.
    store
        .put_brief(
            review.id,
            NewBrief {
                headline: None,
                ..whole_brief()
            },
        )
        .expect("an absent headline is not a blank one");
}

/// A numbered problem's short label: kept as written, absent when it was not written, and
/// never the empty string.
///
/// **The three states are what the Alternatives page branches on.** It heads a matrix
/// column with the label when there is one and with the number alone when there is not, and
/// it never prints `was_wrong` as a heading — so a blank label stored as a label would head
/// a column with nothing at all, which looks exactly like a page that failed to draw.
#[test]
fn a_problem_keeps_the_short_label_it_was_written_with() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problems: vec![
                    BriefProblem {
                        label: Some("The shared slot".to_string()),
                        ..problem(1)
                    },
                    BriefProblem {
                        label: None,
                        ..problem(2)
                    },
                ],
                flow_before: vec![vec![flow_box("a box", Some(1))]],
                flow_after: vec![vec![flow_box("a fixed box", Some(1))]],
                options: Vec::new(),
                blind_pass: None,
                parts: Vec::new(),
                open_choices: Vec::new(),
                disagreements: Vec::new(),
                ..whole_brief()
            },
        )
        .expect("a label is optional, one problem each way");

    assert_eq!(
        stored.problems[0].label.as_deref(),
        Some("The shared slot"),
        "the label is kept as written, not shortened from was_wrong"
    );
    assert_eq!(
        stored.problems[1].label, None,
        "a problem written with no label reads back with none"
    );

    // And no length is enforced, here or anywhere: a label that runs long is a writing
    // fault and belongs to the linter in Locus, the same ruling migration 10 made about
    // every other field of the brief.
    let long = "a label nobody should write ".repeat(12);
    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problems: vec![BriefProblem {
                    label: Some(long.clone()),
                    ..problem(1)
                }],
                flow_before: vec![vec![flow_box("a box", Some(1))]],
                flow_after: vec![vec![flow_box("a fixed box", Some(1))]],
                options: Vec::new(),
                blind_pass: None,
                parts: Vec::new(),
                open_choices: Vec::new(),
                disagreements: Vec::new(),
                ..whole_brief()
            },
        )
        .expect("no length is enforced on a label");
    assert_eq!(stored.problems[0].label.as_deref(), Some(long.as_str()));
}

#[test]
fn a_blank_label_on_a_problem_is_refused_rather_than_stored() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (refused, _, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            problems: vec![BriefProblem {
                label: Some("  ".to_string()),
                ..problem(1)
            }],
            flow_before: vec![vec![flow_box("a box", Some(1))]],
            flow_after: vec![vec![flow_box("a fixed box", Some(1))]],
            options: Vec::new(),
            blind_pass: None,
            parts: Vec::new(),
            open_choices: Vec::new(),
            disagreements: Vec::new(),
            ..whole_brief()
        },
    ));
    assert_eq!(refused, "problems.label");
    assert!(expected.contains("leave the field out"), "{expected}");
}

/// A part's one line: kept as written, absent when it was not written, never empty.
///
/// **The three states are what the Solution page branches on.** Its table draws the one line
/// under the title when there is one and cuts the summary to a line when there is not —
/// and on the review board A10 was measured against those summaries ran 33 to 52 words, so
/// the branch is the difference between a row and four wrapped lines. A blank one stored as
/// a one line would draw an empty row under the title, which looks exactly like a page that
/// failed to join.
#[test]
fn a_solution_part_keeps_the_one_line_it_was_written_with() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let part = |title: &str, one_line: Option<&str>| BriefPart {
        title: title.to_string(),
        summary: "What this part did, at the length a session writes it.".to_string(),
        one_line: one_line.map(str::to_string),
        fixes: Vec::new(),
        files: vec![BriefPartFile {
            path: format!("app/{title}.php"),
            lines_added: 3,
            lines_removed: 1,
            in_diff: true,
        }],
    };

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problems: Vec::new(),
                flow_before: Vec::new(),
                flow_after: Vec::new(),
                flow_before_caption: None,
                flow_after_caption: None,
                options: Vec::new(),
                blind_pass: None,
                parts: vec![
                    part(
                        "Written",
                        Some("The queue keeps the attempt under its own name."),
                    ),
                    part("Unwritten", None),
                ],
                open_choices: Vec::new(),
                disagreements: Vec::new(),
                ..whole_brief()
            },
        )
        .expect("a one line is optional, one part each way");

    assert_eq!(
        stored.parts[0].one_line.as_deref(),
        Some("The queue keeps the attempt under its own name."),
        "the one line is kept as written, not shortened from the summary"
    );
    assert_eq!(
        stored.parts[1].one_line, None,
        "a part written with no one line reads back with none, not with its summary"
    );

    // And no length is enforced, which is migration 10's ruling about every field of the
    // brief: a one line that runs to three lines is a writing fault and belongs to the
    // linter in Locus.
    let long = "a line nobody should write ".repeat(12);
    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                problems: Vec::new(),
                flow_before: Vec::new(),
                flow_after: Vec::new(),
                flow_before_caption: None,
                flow_after_caption: None,
                options: Vec::new(),
                blind_pass: None,
                parts: vec![part("Long", Some(&long))],
                open_choices: Vec::new(),
                disagreements: Vec::new(),
                ..whole_brief()
            },
        )
        .expect("no length is enforced on a one line");
    assert_eq!(stored.parts[0].one_line.as_deref(), Some(long.as_str()));
}

#[test]
fn a_blank_one_line_on_a_part_is_refused_rather_than_stored() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (refused, _, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            problems: Vec::new(),
            flow_before: Vec::new(),
            flow_after: Vec::new(),
            flow_before_caption: None,
            flow_after_caption: None,
            options: Vec::new(),
            blind_pass: None,
            parts: vec![BriefPart {
                title: "a part".to_string(),
                summary: "what it did".to_string(),
                one_line: Some("  ".to_string()),
                fixes: Vec::new(),
                files: Vec::new(),
            }],
            open_choices: Vec::new(),
            disagreements: Vec::new(),
            ..whole_brief()
        },
    ));
    assert_eq!(refused, "parts.one_line");
    assert!(expected.contains("leave the field out"), "{expected}");
}

#[test]
fn a_caption_over_an_empty_flow_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // The same fault as a heading over an empty table: the page draws the words and then
    // a blank where the picture goes.
    let (field, _, expected) = refusal(store.put_brief(
        review.id,
        NewBrief {
            flow_after: Vec::new(),
            flow_after_caption: Some("The delivery is queued".to_string()),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "flow_after_caption");
    assert!(expected.contains("a flow to caption"), "{expected}");
}

#[test]
fn a_blind_pass_picking_an_option_that_is_not_on_the_list_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, value, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            blind_pass: Some(BriefBlindPass {
                given: "the problem, as handed over".to_string(),
                pick_key: Some("Z".to_string()),
                pick_why: Some("it would".to_string()),
                looked_up: "nothing".to_string(),
                questions: Vec::new(),
            }),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "blind_pass.pick_key");
    assert_eq!(value, "Z");
}

#[test]
fn a_blind_pass_pick_with_no_reason_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let (field, _, _) = refusal(store.put_brief(
        review.id,
        NewBrief {
            blind_pass: Some(BriefBlindPass {
                given: "the problem, as handed over".to_string(),
                pick_key: Some("B".to_string()),
                pick_why: None,
                looked_up: "nothing".to_string(),
                questions: Vec::new(),
            }),
            ..whole_brief()
        },
    ));
    assert_eq!(field, "blind_pass.pick_why");
}

#[test]
fn a_disagreement_naming_a_finding_that_is_not_on_this_review_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    match store.put_brief(
        review.id,
        NewBrief {
            disagreements: vec![BriefDisagreement {
                description_says: "one thing".to_string(),
                code_does: "another".to_string(),
                finding_seq: 99,
            }],
            ..whole_brief()
        },
    ) {
        Err(StoreError::UnknownParent { parent, id, .. }) => {
            assert_eq!(parent, "finding");
            assert!(id.ends_with("/99"), "{id}");
        }
        other => panic!("expected an unknown finding, got {other:?}"),
    }
}

#[test]
fn an_open_choice_naming_a_finding_that_is_not_on_this_review_is_refused() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    match store.put_brief(
        review.id,
        NewBrief {
            open_choices: vec![BriefOpenChoice {
                left_open: "something".to_string(),
                chosen: "something else".to_string(),
                why: "because".to_string(),
                departs_from_ticket: false,
                finding_seq: Some(42),
            }],
            ..whole_brief()
        },
    ) {
        Err(StoreError::UnknownParent { parent, id, .. }) => {
            assert_eq!(parent, "finding");
            assert!(id.ends_with("/42"), "{id}");
        }
        other => panic!("expected an unknown finding, got {other:?}"),
    }
}

#[test]
fn an_open_choice_may_point_at_nothing() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                open_choices: vec![BriefOpenChoice {
                    left_open: "where the allowed columns are stored".to_string(),
                    chosen: "a column on the definition".to_string(),
                    why: "a definition is already per tenant".to_string(),
                    departs_from_ticket: false,
                    finding_seq: None,
                }],
                ..whole_brief()
            },
        )
        .expect("a choice is a choice, not a fault");
    assert_eq!(stored.open_choices[0].finding_seq, None);
}

#[test]
fn a_refused_brief_leaves_the_one_already_held_untouched() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let held = store.put_brief(review.id, whole_brief()).expect("a brief");

    // A write that fails on its last row: everything before it is valid.
    let refused = store.put_brief(
        review.id,
        NewBrief {
            headline: Some("a headline that must not reach the record".to_string()),
            disagreements: vec![BriefDisagreement {
                description_says: "one thing".to_string(),
                code_does: "another".to_string(),
                finding_seq: 99,
            }],
            ..whole_brief()
        },
    );
    assert!(refused.is_err(), "the write is refused");

    // The whole brief is one record, so a refusal rolls all of it back.
    assert_eq!(store.get_brief(review.id).expect("the brief"), held);
}

#[test]
fn the_store_holds_no_length_limit() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    // The brief that prompted this change ran 310 words where the approved board shows
    // 85. **That is the linter's to catch, not the store's** — a CHECK on a length would
    // make a writing rule into a refusal, and the refusal would arrive at the one moment
    // a session has the words in hand and nowhere to put them.
    let long = "word ".repeat(400);
    let stored = store
        .put_brief(
            review.id,
            NewBrief {
                headline: Some(long.clone()),
                how_it_works_today: Some(long.clone()),
                problems: vec![BriefProblem {
                    was_wrong: long.clone(),
                    ..problem(1)
                }],
                flow_before: vec![vec![flow_box("a box", Some(1))]],
                flow_after: Vec::new(),
                flow_after_caption: None,
                options: Vec::new(),
                blind_pass: None,
                parts: Vec::new(),
                ..whole_brief()
            },
        )
        .expect("the store refuses structural nonsense, not long writing");
    assert_eq!(stored.headline, Some(long));
}

#[test]
fn writing_the_brief_again_replaces_every_row_of_it() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    store.put_brief(review.id, whole_brief()).expect("a brief");
    let replaced = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "the problem, restated".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");

    // The same bargain `put_brief` already struck with its options: a brief whose problem
    // changed under last week's flows is not a brief anybody wrote.
    assert!(replaced.problems.is_empty());
    assert!(replaced.flow_before.is_empty());
    assert!(replaced.parts.is_empty());
    assert_eq!(replaced.support_table, None);
    assert_eq!(replaced.blind_pass, None);
    assert!(replaced.disagreements.is_empty());
}

#[test]
fn writing_the_brief_again_does_not_confirm_it() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let stored = store.put_brief(review.id, whole_brief()).expect("a brief");

    // **Nothing added here confirms anything.** Twelve more rows on a brief is twelve
    // more things the developer has not yet agreed to.
    assert!(!stored.is_confirmed());
    assert!(!stored.is_settled());
    assert_eq!(stored.confirmed_by, None);
}

// -- the problem half -----------------------------------------------------------------

/// A marker nothing in the problem half may carry.
const MARKER: &str = "THE-CHANGE-HALF-WAS-HERE";

/// A brief whose every change-half field carries the marker.
fn brief_marked_through_the_change_half() -> NewBrief {
    let marked = |text: &str| format!("{text} {MARKER}");
    let whole = whole_brief();
    NewBrief {
        // Row 1.
        headline: Some(marked("the headline")),
        // Row 3.
        flow_after_caption: Some(marked("the after caption")),
        flow_after: vec![vec![BriefFlowBox {
            title: marked("an after box"),
            note: Some(marked("an after note")),
            problem: Some(1),
        }]],
        // Rows 4 and 5: the "now" column, and the named detail on both sides.
        problems: vec![BriefProblem {
            number: 1,
            label: Some(marked("the label")),
            was_wrong: "a delivery that fails is never tried again".to_string(),
            now_fixed: Some(marked("what it is now")),
            detail_title: Some(marked("the detail title")),
            detail_before: vec![marked("a line of detail, before")],
            detail_after: vec![marked("a line of detail, after")],
        }],
        // Row 6.
        support_table: Some(BriefSupportTable {
            title: Some(marked("the table")),
            columns: vec![marked("a heading")],
            rows: vec![vec![marked("a cell")]],
        }),
        // Rows 8 and 9.
        options: vec![BriefOption {
            key: "A".to_string(),
            title: marked("an option"),
            argument_for: marked("for"),
            argument_against: marked("against"),
            chosen: true,
            proposed_by: OptionProposer::Both,
            verdicts: vec![verdict(1, ProblemVerdict::Fixed, Some(&marked("why")))],
        }],
        // Row 10.
        blind_pass: Some(BriefBlindPass {
            given: marked("what the pass was given"),
            pick_key: Some("A".to_string()),
            pick_why: Some(marked("why it would pick it")),
            looked_up: marked("nothing"),
            questions: vec![marked("a question")],
        }),
        // Row 11.
        parts: vec![BriefPart {
            title: marked("a part"),
            summary: marked("one line"),
            one_line: Some(marked("the one line")),
            fixes: vec![1],
            files: vec![BriefPartFile {
                path: format!("app/{MARKER}.php"),
                lines_added: 1,
                lines_removed: 0,
                in_diff: true,
            }],
        }],
        // Row 12.
        open_choices: vec![BriefOpenChoice {
            left_open: marked("left open"),
            chosen: marked("chosen"),
            why: marked("why"),
            departs_from_ticket: true,
            finding_seq: Some(1),
        }],
        disagreements: vec![BriefDisagreement {
            description_says: marked("the description says"),
            code_does: marked("the code does"),
            finding_seq: 2,
        }],
        // And the superseded field that holds both halves at once.
        diagram: Some(marked("a diagram of before and after")),
        approach_verdict: Some(marked("the verdict on the approach")),
        // Rows 2 and 7 are the problem half, and carry no marker.
        flow_before: vec![vec![flow_box("the delivery is sent inline", Some(1))]],
        flow_before_caption: Some("The delivery is sent inline".to_string()),
        how_it_works_today: Some("A delivery is sent inside the request.".to_string()),
        problem: "a webhook delivery fails once and is never tried again".to_string(),
        provenance: Some(marked("where this came from")),
        ..whole
    }
}

#[test]
fn nothing_from_the_change_half_reaches_the_problem_statement() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    store
        .put_brief(review.id, brief_marked_through_the_change_half())
        .expect("a brief");

    let statement = store
        .problem_statement(review.id)
        .expect("the problem half");
    let serialised = serde_json::to_string(&statement).expect("the problem half serialises");

    assert!(
        !serialised.contains(MARKER),
        "the change half reached the problem statement: {serialised}"
    );

    // And the problem half is actually there, so the assertion above is not passing
    // because the read returned nothing.
    assert_eq!(statement.problems.len(), 1);
    assert_eq!(
        statement.problems[0].was_wrong,
        "a delivery that fails is never tried again"
    );
    assert_eq!(statement.flow_before.len(), 1);
    assert!(statement.how_it_works_today.is_some());
}

#[test]
fn the_problem_statement_of_a_brief_written_the_old_way_is_its_problem() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a webhook delivery fails once and is never tried again".to_string(),
                diagram: Some("flowchart LR\n  A --> B".to_string()),
                ..Default::default()
            },
        )
        .expect("a brief");

    let statement = store
        .problem_statement(review.id)
        .expect("the problem half");
    assert_eq!(
        statement.problem,
        "a webhook delivery fails once and is never tried again"
    );
    assert!(statement.flow_before.is_empty());
    assert!(statement.problems.is_empty());
    assert_eq!(statement.how_it_works_today, None);
}

#[test]
fn a_review_with_no_brief_has_no_problem_statement() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    match store.problem_statement(review.id) {
        Err(StoreError::NotFound { entity, .. }) => assert_eq!(entity, "brief"),
        other => panic!("expected a missing brief, got {other:?}"),
    }
}

#[test]
fn reading_the_problem_statement_decides_nothing() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    store.put_brief(review.id, whole_brief()).expect("a brief");
    store
        .problem_statement(review.id)
        .expect("the problem half");

    let brief = store.get_brief(review.id).expect("the brief");
    assert!(!brief.is_confirmed(), "a read does not confirm");
    assert!(!brief.is_settled());

    // Layer 2 is where a confirmation shows, so it is checked too.
    let layers = store.list_review_layers(review.id).expect("the layers");
    let at_two = layers.iter().find(|layer| layer.number == 2);
    assert!(
        at_two.is_none_or(|layer| layer.detail.as_deref() != Some("confirmed by the developer")),
        "reading the problem half must not settle layer 2"
    );
}

#[test]
fn only_the_developer_can_confirm_a_brief_with_every_row_on_it() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    store.put_brief(review.id, whole_brief()).expect("a brief");
    for actor in [Actor::Author, Actor::Harvester] {
        assert!(
            store.confirm_brief(review.id, actor).is_err(),
            "{actor} must not confirm a brief"
        );
    }
    assert!(store.confirm_brief(review.id, Actor::Developer).is_ok());
}

// -- the contract's real-length example -----------------------------------------------

fn example(name: &str) -> serde_json::Value {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .join("contract")
        .join("examples")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("the example is JSON")
}

/// The contract's real-length brief is **at real length**, measured rather than asserted.
///
/// DEV-882 asks for one example at the length a real review produces, because an example
/// of one problem and two options proves the shape and hides whether it scales. The
/// figures below are that issue's, and this is what stops the example being trimmed back
/// to something comfortable.
#[test]
fn the_contracts_real_length_brief_is_at_real_length() {
    let brief = example("confirmBrief.response.real-length.json");

    let rows = |field: &str| {
        brief[field]
            .as_array()
            .unwrap_or_else(|| panic!("{field} is an array"))
            .clone()
    };

    assert!(
        rows("problems").len() >= 3,
        "at least three problems, not {}",
        rows("problems").len()
    );

    let before = rows("flow_before");
    let after = rows("flow_after");
    assert_eq!(before.len(), 5, "the before flow is five rows");
    assert_eq!(after.len(), 6, "the after flow is six rows");
    let split = |flow: &[serde_json::Value]| {
        flow.iter()
            .filter(|row| row.as_array().is_some_and(|boxes| boxes.len() > 1))
            .count()
    };
    assert_eq!(split(&before), 1, "one split row in the before flow");
    assert_eq!(split(&after), 1, "one split row in the after flow");

    let options = rows("options");
    assert_eq!(options.len(), 8, "eight options");
    let sources: std::collections::BTreeSet<&str> = options
        .iter()
        .map(|option| option["proposed_by"].as_str().expect("a proposer"))
        .collect();
    assert!(
        sources.contains("ticket") || sources.contains("both"),
        "the ticket is one source: {sources:?}"
    );
    assert!(
        sources.contains("blind_pass") || sources.contains("both"),
        "the blind pass is the other: {sources:?}"
    );
    assert!(
        sources.len() >= 2,
        "two sources, not {}: {sources:?}",
        sources.len()
    );

    assert_eq!(rows("parts").len(), 6, "six solution parts");

    // And one thing the issue did not ask for but the real review needed: a part claiming
    // a file the pull request never changed.
    let outside = rows("parts")
        .iter()
        .flat_map(|part| part["files"].as_array().expect("files").clone())
        .filter(|file| file["in_diff"] == serde_json::json!(false))
        .count();
    assert!(outside >= 1, "at least one file outside the diff");
}

/// The real-length example is a brief the store can hold, not only one the schema allows.
#[test]
fn the_contracts_real_length_brief_can_be_written_and_read_back() {
    let (_dir, store) = temp_store();
    let review = seed(&store);

    let example = example("confirmBrief.response.real-length.json");

    // The example is a *response*; a write takes `NewBrief`. The fields a response adds
    // and a write does not take are dropped, and `deny_unknown_fields` on `NewBrief` is
    // what makes that list explicit rather than silent.
    let mut document = example.as_object().expect("an object").clone();
    for served_only in [
        "review_id",
        "confirmed",
        "confirmed_at",
        "blind_pass_state",
        "updated_at",
    ] {
        document.remove(served_only);
    }
    for option in document["options"].as_array_mut().expect("the options") {
        // `finding_key` and a part's summed lines are derived on the way out.
        option.as_object_mut().expect("an option");
    }
    for part in document["parts"].as_array_mut().expect("the parts") {
        let part = part.as_object_mut().expect("a part");
        part.remove("lines_added");
        part.remove("lines_removed");
    }
    for choice in document["open_choices"]
        .as_array_mut()
        .expect("the choices")
    {
        choice
            .as_object_mut()
            .expect("a choice")
            .remove("finding_key");
    }
    for clash in document["disagreements"]
        .as_array_mut()
        .expect("the disagreements")
    {
        clash
            .as_object_mut()
            .expect("a clash")
            .remove("finding_key");
    }

    // The example's pointers are that review's finding numbers; this seed has four
    // findings, so they are rewritten to ones that exist here. What is under test is the
    // brief's size, not its finding numbers.
    for choice in document["open_choices"]
        .as_array_mut()
        .expect("the choices")
    {
        let choice = choice.as_object_mut().expect("a choice");
        if !choice["finding_seq"].is_null() {
            choice.insert("finding_seq".to_string(), serde_json::json!(1));
        }
    }
    for clash in document["disagreements"]
        .as_array_mut()
        .expect("the disagreements")
    {
        clash
            .as_object_mut()
            .expect("a clash")
            .insert("finding_seq".to_string(), serde_json::json!(2));
    }

    let new: NewBrief = serde_json::from_value(serde_json::Value::Object(document))
        .expect("the example deserialises as a brief to write");
    let stored = store
        .put_brief(review.id, new)
        .expect("a brief at real length");

    assert_eq!(stored.problems.len(), 3);
    assert_eq!(stored.flow_before.len(), 5);
    assert_eq!(stored.flow_after.len(), 6);
    assert_eq!(stored.options.len(), 8);
    assert_eq!(stored.parts.len(), 6);
    assert_eq!(
        stored
            .options
            .iter()
            .map(|o| o.verdicts.len())
            .sum::<usize>(),
        24,
        "eight options judged against three problems"
    );
    assert_eq!(stored.blind_pass_state(), BlindPassState::Ran);
    assert_eq!(store.get_brief(review.id).expect("the brief"), stored);
}
