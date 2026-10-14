//! Every response example in `contract/examples/` can be produced from the store.
//!
//! The contract is the agreement between the local server and the dashboard, and this
//! crate is where its fields have to come from. The test seeds one record covering the
//! whole contract, builds each response document out of the store's public API, and
//! checks that **every field in the example is either produced or declared to have no
//! source**. A field the contract asks for that nothing can answer fails the test
//! rather than reaching the dashboard as a hole.
//!
//! Two things this test deliberately does not do:
//!
//! - **It does not compare values.** The examples carry sample ids and times, and the
//!   store assigns its own. What matters is that the field is answerable.
//! - **It does not match types.** The store holds times as epoch seconds and the
//!   contract is RFC 3339; the conversion belongs to the server, and
//!   `times_are_integers_in_the_store_and_the_server_converts_them` is what records
//!   that this is the only difference.
//!
//! The documents are built here rather than in `crates/server`, which this change does
//! not touch. That is duplication with a purpose: it proves the store can serve the
//! contract without waiting for the server, and it is the store's own statement of
//! which fields it owns.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::temp_store;
use review_desk_store::{
    finding_heading, now, Actor, BlindPassState, Brief, BriefBlindPass, BriefDisagreement,
    BriefFlowBox, BriefFlowRow, BriefOpenChoice, BriefOption, BriefOptionVerdict, BriefPart,
    BriefPartFile, BriefProblem, BriefSupportTable, CriterionStatus, Decision,
    DescriptionLintStatus, DiffAtHead, Disposition, Draft, DraftLint, DraftSection, FileCheck,
    Finding, FindingDecision, FindingHunk, FindingRef, HunkLine, HunkLineMarker, LeftTheDesk,
    NewAcceptanceCriterion, NewBrief, NewDescriptionLintResult, NewDraft, NewFileChange,
    NewFinding, NewOutcome, NewReview, NewReviewSource, NewRule, NitTicket, NotForSigning,
    OptionProposer, Outcome, OutcomeKind, ProblemStatement, ProblemVerdict, Review, ReviewEvent,
    ReviewFacts, ReviewLayer, ReviewLayerName, ReviewLayerRun, ReviewProvenance, ReviewTimings,
    ReviewsShown, Rule, RuleScope, RuleStage, Severity, SourceKind, Store, Verification,
};
use serde_json::{json, Map, Value};

/// Fields in the examples that no store change can supply, with who owns each.
///
/// From `contract/README.md`, "Facts on the mockups that no store change can supply".
/// Every one is declared nullable in the schema, and the dashboard renders the null.
/// **Listed, not faked**: the test asserts each of these is absent or null in what the
/// store can build, so that one of them quietly gaining a made-up value is a failure.
const NO_SOURCE: &[(&str, &str)] = &[
    (
        "/own_prs_without_reviewer",
        "GitHub: nothing lists the developer's own pull requests",
    ),
    (
        "/handled_without_you/description_problems_caught",
        "a CI run",
    ),
    (
        "/in_progress/[]/session_alive",
        "Allele: the store holds a session id, not its liveness",
    ),
    ("/active/[]/counts/caught_by_check", "a CI run"),
    ("/active/[]/counts/raised_in_a_review_since", "a CI run"),
    (
        "/active/[]/counts/last_human_mention_at",
        "a GitHub harvest of what reviewers said",
    ),
    ("/proposed/[]/counts/caught_by_check", "a CI run"),
    ("/proposed/[]/counts/raised_in_a_review_since", "a CI run"),
    (
        "/proposed/[]/counts/last_human_mention_at",
        "a GitHub harvest of what reviewers said",
    ),
    ("/stale/[]/counts/caught_by_check", "a CI run"),
    ("/stale/[]/counts/raised_in_a_review_since", "a CI run"),
    (
        "/stale/[]/counts/last_human_mention_at",
        "a GitHub harvest of what reviewers said",
    ),
    ("/counts/caught_by_check", "a CI run"),
    ("/counts/raised_in_a_review_since", "a CI run"),
    (
        "/counts/last_human_mention_at",
        "a GitHub harvest of what reviewers said",
    ),
    (
        "/summary/replay_matched",
        "the replay consistency check, which does not exist",
    ),
    (
        "/summary/replay_total",
        "the replay consistency check, which does not exist",
    ),
];

/// Where `contract/examples` is, from this crate.
fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .join("contract")
        .join("examples")
}

/// Every leaf path in a document, as a slash-separated pointer with array indices
/// written `[]`.
///
/// Leaves include nulls and empty arrays: a field present and null is a field the
/// contract asks about, and the point of this test is that none of them is a surprise.
fn leaf_paths(value: &Value, prefix: &str, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) if !fields.is_empty() => {
            for (name, field) in fields {
                leaf_paths(field, &format!("{prefix}/{name}"), found);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            // Every element of an array has the same shape, so one path serves all of
            // them. Taking the union rather than the first element catches an example
            // whose second entry carries a field the first leaves out.
            for item in items {
                leaf_paths(item, &format!("{prefix}/[]"), found);
            }
        }
        _ => {
            found.insert(prefix.to_string());
        }
    }
}

/// The value at a leaf path, if the document has one.
///
/// An array step searches every element rather than taking the first. The question
/// this test asks is whether the store can produce a field at all, and a schema's
/// arrays are homogeneous — so one element answering it is the field being answerable.
/// A non-null answer is preferred over a null one for the same reason.
fn at<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    let (step, rest) = match path.trim_start_matches('/').split_once('/') {
        Some((step, rest)) => (step, rest),
        None if path.trim_start_matches('/').is_empty() => return Some(document),
        None => (path.trim_start_matches('/'), ""),
    };

    let next = match document {
        Value::Array(items) if step == "[]" => {
            let mut nulled = None;
            for item in items {
                match at(item, rest) {
                    Some(found) if !found.is_null() => return Some(found),
                    Some(found) => nulled = Some(found),
                    None => {}
                }
            }
            return nulled;
        }
        Value::Object(fields) => fields.get(step)?,
        _ => return None,
    };

    if rest.is_empty() {
        Some(next)
    } else {
        at(next, rest)
    }
}

/// The record every example is built from: one review carried all the way through,
/// plus the rules the Rules screen shows.
struct Seeded {
    review: Review,
    other_review: Review,
    waiting_review: Review,
    /// A review carried all the way to posted, which is what `GET /timings` lists. Read
    /// through the store rather than by id — `finished_review_timings` finds it — and kept
    /// on the struct so the seed's shape is readable from one place.
    #[allow(dead_code)]
    posted_review: Review,
    /// A fifth review, posted **and harvested**, whose author has replied and whose pull
    /// request is still open. It is the one state in which the posted group's three
    /// figures are measurements; `posted_review` above is the same group's unmeasured
    /// item, and the two together are both halves of telling a nought from an absence.
    #[allow(dead_code)]
    answered_review: Review,
    proposed_rule: Rule,
    excepted_rule: Rule,
    stale_rule: Rule,
}

fn seed(store: &Store) -> Seeded {
    // A rule for a finding to cite, as if promoted out of an earlier review.
    let cited_rule = store
        .create_rule(NewRule {
            statement: "A new test must fail when the fix is reverted".to_string(),
            prevents: "a regression shipping behind a test that cannot fail".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .expect("a rule");

    let review = store
        .open_review_with(
            "acme/portal",
            412,
            "3f9c2ab",
            &ReviewFacts {
                title: Some("Retry failed webhook deliveries".to_string()),
                ticket_id: Some("ABC-231".to_string()),
                ticket_url: Some("https://linear.app/acme/issue/ABC-231".to_string()),
                changed_lines: Some(214),
                not_checked: Some(
                    "The queue worker's behaviour under a deploy restart was not run.".to_string(),
                ),
            },
            DiffAtHead::Unsaid,
        )
        .expect("a review")
        .review;

    store
        .set_review_links(
            review.id,
            review_desk_store::ReviewLinks {
                session_id: Some("ses_c81f40ad".to_string()),
                ledger_id: Some("ledger-2026-10-08-412".to_string()),
                ..Default::default()
            },
        )
        .expect("the other systems' ids");

    // **Every state each step passed through, in order**, which is how a session really
    // writes a trail and what gives a finished step a pass with both ends — the
    // `layers[].runs` and `layers[].seconds` the example carries. Step 2 waits on the
    // developer between the brief and the confirmation, so the review has a finished wait
    // and `timings.waiting_on_you_seconds` has something to add up.
    //
    // The clock is the machine's here, so the durations come out at zero seconds. A zero
    // is a measurement and this test compares no values; `tests/timings.rs` is where the
    // arithmetic is asserted, against a clock it can move.
    for (number, states, detail) in [
        (1, &["running", "done"][..], "passed"),
        (
            2,
            &["running", "waiting_on_you", "done"][..],
            "confirmed by you",
        ),
        (3, &["running", "done"][..], "3 options"),
        (4, &["running", "done"][..], "done"),
        (5, &["running", "done"][..], "6 findings"),
        (6, &["running", "waiting_on_you"][..], "waiting on you"),
    ] {
        for (step, state) in states.iter().enumerate() {
            let last = step + 1 == states.len();
            store
                .set_review_layer(
                    review.id,
                    number,
                    state.parse().expect("a state"),
                    last.then_some(detail),
                )
                .expect("a layer");
        }
    }

    // Six findings: three in the draft, two held back, one suppressed — the
    // dispositions the One review example shows.
    let finding =
        |severity: Severity, disposition: Disposition, rule: Option<i64>, claim: &str| NewFinding {
            lens: "sibling diff".to_string(),
            severity,
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(58),
            // The commit that line was counted on. Stated by the session, as every
            // finding's is — the store derives none of it.
            line_sha: Some("3f9c2ab".to_string()),
            claim: claim.to_string(),
            evidence: Some("throwaway test, 2 deliveries for 1 event".to_string()),
            disposition,
            disposition_reason: match disposition {
                Disposition::InDraft => None,
                _ => Some("Held for one ticket with the other nits.".to_string()),
            },
            rule_id: rule,
            ..Default::default()
        };
    for new in [
        NewFinding {
            // The One review example's F1 carries everything DEV-874 added, so every one
            // of those fields has to be answerable from the store or this test fails.
            hunk: Some(FindingHunk {
                file: "jobs/DeliverWebhook.php".to_string(),
                start_line: 56,
                lines: vec![
                    HunkLine {
                        marker: HunkLineMarker::Unchanged,
                        text: "{".to_string(),
                    },
                    HunkLine {
                        marker: HunkLineMarker::Removed,
                        text: "    $client->post($this->endpoint->url, $this->payload);"
                            .to_string(),
                    },
                    HunkLine {
                        marker: HunkLineMarker::Added,
                        text: "    $response = $client->post($this->endpoint->url);".to_string(),
                    },
                ],
            }),
            flagged_line: Some(57),
            inline_comment: Some(
                "This retry re-sends the payload with no idempotency key.".to_string(),
            ),
            why_it_matters: Some(
                "A customer whose endpoint answers slowly is charged twice.".to_string(),
            ),
            proof: Some("deliveries for evt_1: 2, expected 1".to_string()),
            proof_note: Some("A throwaway test, not committed.".to_string()),
            verification: Verification::Reproduced,
            suggested_fix: Some("Send the delivery id as the key.".to_string()),
            fix_references: vec!["SendInvoiceEmail.php:44".to_string()],
            // And the one line the code review page's row carries, added by DEV-878. The
            // example gives F1 a title and F2 none, because a page branches on which it is
            // and one finding cannot show both.
            title: Some("A retried delivery carries no idempotency key".to_string()),
            ..finding(
                Severity::Blocker,
                Disposition::InDraft,
                None,
                "no idempotency key",
            )
        },
        finding(
            Severity::Should,
            Disposition::InDraft,
            None,
            "no way to replay",
        ),
        finding(
            Severity::Should,
            Disposition::InDraft,
            Some(cited_rule.id),
            "the failure test passes with the fix reverted",
        ),
        NewFinding {
            // F4 is what the setFindingDisposition example returns, and that example
            // carries the new fields too — a finding keeps what it is about when the
            // developer moves it out of the draft.
            hunk: Some(FindingHunk {
                file: "config/webhooks.php".to_string(),
                start_line: 12,
                lines: vec![HunkLine {
                    marker: HunkLineMarker::Unchanged,
                    text: "    'backoff' => [30, 120, 600],".to_string(),
                }],
            }),
            flagged_line: Some(12),
            why_it_matters: Some("The two copies will drift.".to_string()),
            proof: Some("From reading both files.".to_string()),
            proof_note: Some("Read, not run: the schedule was not executed.".to_string()),
            verification: Verification::ReadNotRun,
            suggested_fix: Some("Read the schedule from one place.".to_string()),
            ..finding(
                Severity::Nit,
                Disposition::HeldBack,
                None,
                "backoff repeated",
            )
        },
        finding(Severity::Nit, Disposition::HeldBack, None, "unused import"),
        finding(
            Severity::Nit,
            Disposition::Suppressed,
            None,
            "rename attempt",
        ),
    ] {
        store.add_finding(review.id, new).expect("a finding");
    }

    // The brief, whole, with every row the three approved boards draw.
    //
    // **Written after the findings, and that is the order a session writes it in.** The
    // brief's change half points at findings — an open choice may, a disagreement must —
    // so the problem half is written at layer 2 and the rest once the diff has been read.
    // Here it is one write, because this seed is a finished review rather than a
    // transcript of one.
    let verdict = |problem: i64, verdict: ProblemVerdict| BriefOptionVerdict {
        problem,
        verdict,
        why: Some("what it does about that problem, in one line".to_string()),
    };
    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "A webhook delivery fails once and is never tried again.".to_string(),
                diagram: Some(
                    "flowchart LR\n  A[Event happens] --> B[Delivery is lost]".to_string(),
                ),
                approach_verdict: Some("B is the right shape for this ticket.".to_string()),
                provenance: Some(
                    "from the ticket and the code · you corrected one sentence".to_string(),
                ),
                options: ["A", "B", "C"]
                    .iter()
                    .map(|key| BriefOption {
                        key: key.to_string(),
                        title: format!("Option {key}"),
                        argument_for: "the argument for it".to_string(),
                        argument_against: "the argument against it".to_string(),
                        chosen: *key == "B",
                        // Three proposers across three options, so the Alternatives page's
                        // badge has each of them to draw and `blind_pass_state` reads
                        // `ran`.
                        proposed_by: match *key {
                            "A" => OptionProposer::Ticket,
                            "B" => OptionProposer::Both,
                            _ => OptionProposer::BlindPass,
                        },
                        verdicts: vec![
                            verdict(1, ProblemVerdict::Fixed),
                            verdict(2, ProblemVerdict::Partly),
                        ],
                    })
                    .collect(),
                option_map: None,
                headline: Some(
                    "Deliveries move to a queue, so one that fails is retried.".to_string(),
                ),
                how_it_works_today: Some(
                    "A delivery is sent inside the request that caused it, and a failure is \
                     logged and dropped."
                        .to_string(),
                ),
                flow_before_caption: Some("The delivery is sent inline".to_string()),
                flow_after_caption: Some("The delivery is queued".to_string()),
                flow_before: vec![
                    vec![BriefFlowBox {
                        title: "An event happens".to_string(),
                        note: Some("A subscription changes".to_string()),
                        problem: None,
                    }],
                    // A split row, so the example's two-box row has a source.
                    vec![
                        BriefFlowBox {
                            title: "The endpoint answers".to_string(),
                            note: Some("Logged and forgotten".to_string()),
                            problem: Some(2),
                        },
                        BriefFlowBox {
                            title: "The endpoint is down".to_string(),
                            note: Some("Nothing retries".to_string()),
                            problem: Some(1),
                        },
                    ],
                ],
                flow_after: vec![vec![BriefFlowBox {
                    title: "A worker sends it".to_string(),
                    note: Some("Retried with backoff".to_string()),
                    problem: Some(1),
                }]],
                problems: vec![
                    BriefProblem {
                        number: 1,
                        label: Some("The inline delivery".to_string()),
                        was_wrong: "A delivery that fails is never tried again.".to_string(),
                        now_fixed: Some("A worker retries it for 24 hours.".to_string()),
                        detail_title: Some("The delivery sent inline".to_string()),
                        detail_before: vec!["The sender waits inside the request.".to_string()],
                        detail_after: vec!["A row is written before anything is sent.".to_string()],
                    },
                    BriefProblem {
                        number: 2,
                        label: Some("No record of an attempt".to_string()),
                        was_wrong: "Nothing records that a delivery was attempted.".to_string(),
                        now_fixed: Some("Every attempt is a row.".to_string()),
                        detail_title: Some("No record of an attempt".to_string()),
                        detail_before: vec!["Only a log line nothing reads.".to_string()],
                        detail_after: vec!["One row per attempt, with its status.".to_string()],
                    },
                ],
                support_table: Some(BriefSupportTable {
                    title: Some("The idea: two questions, each with one owner".to_string()),
                    columns: vec![
                        "The question".to_string(),
                        "Who should answer it".to_string(),
                        "Before".to_string(),
                        "After".to_string(),
                    ],
                    rows: vec![vec![
                        "Has this delivery been attempted?".to_string(),
                        "The delivery row".to_string(),
                        "A log line nothing reads".to_string(),
                        "A row with a status".to_string(),
                    ]],
                }),
                blind_pass: Some(BriefBlindPass {
                    given: "A web application delivers events inside the request that caused \
                            them, and a failure is logged and dropped."
                        .to_string(),
                    pick_key: Some("B".to_string()),
                    pick_why: Some("A queued delivery answers both problems.".to_string()),
                    looked_up: "nothing".to_string(),
                    questions: vec!["Must a retry be idempotent?".to_string()],
                }),
                parts: vec![BriefPart {
                    title: "The delivery row and its states".to_string(),
                    summary: "One row per attempt, written before anything is sent.".to_string(),
                    one_line: None,
                    fixes: vec![2],
                    files: vec![
                        BriefPartFile {
                            path: "app/Models/WebhookDelivery.php".to_string(),
                            lines_added: 74,
                            lines_removed: 0,
                            in_diff: true,
                        },
                        // Outside the diff, which is how a part claims a file the pull
                        // request did not change.
                        BriefPartFile {
                            path: "app/Support/Backoff.php".to_string(),
                            lines_added: 0,
                            lines_removed: 0,
                            in_diff: false,
                        },
                    ],
                }],
                open_choices: vec![BriefOpenChoice {
                    left_open: "How long a failed delivery is kept".to_string(),
                    chosen: "Thirty days, then the row is pruned".to_string(),
                    why: "The ticket asks for a 24-hour window and stops there.".to_string(),
                    departs_from_ticket: false,
                    finding_seq: Some(1),
                }],
                disagreements: vec![BriefDisagreement {
                    description_says: "A retry carries the first attempt's key.".to_string(),
                    code_does: "The key is generated per attempt.".to_string(),
                    finding_seq: 2,
                }],
                ..Default::default()
            },
        )
        .expect("a brief");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer confirms it");

    // The developer's edits between draft and post, which is where they live.
    for (seq, kind, detail) in [
        (6, OutcomeKind::Cut, "rename suggestion"),
        (3, OutcomeKind::Regraded, "Blocker to Should"),
        (1, OutcomeKind::Rewritten, "first sentence"),
        (3, OutcomeKind::Kept, "kept in the posted draft"),
    ] {
        store
            .add_outcome(
                FindingRef {
                    review_id: review.id,
                    seq,
                },
                NewOutcome {
                    kind,
                    actor: Actor::Developer,
                    detail: Some(detail.to_string()),
                    occurred_at: None,
                },
            )
            .expect("an outcome");
    }
    // And an author's reply, which is what the Inbox's "Author replied" is built from.
    store
        .add_outcome(
            FindingRef {
                review_id: review.id,
                seq: 2,
            },
            NewOutcome {
                kind: OutcomeKind::Rejected,
                actor: Actor::Author,
                detail: Some("The export is already chunked upstream.".to_string()),
                occurred_at: None,
            },
        )
        .expect("a reply");

    // Where the review's understanding came from. Written here in the order a session
    // writes it: the findings above exist already, because a criterion recorded as not
    // met names one.
    store
        .put_review_sources(
            review.id,
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
                    link: Some("https://github.com/acme/portal/pull/412".to_string()),
                    status: None,
                    note: None,
                },
            ],
        )
        .expect("the sources");

    store
        .put_description_lint(
            review.id,
            vec![
                NewDescriptionLintResult {
                    label: "Description length".to_string(),
                    status: DescriptionLintStatus::Pass,
                    detail: Some("1,840 of 2,568 characters".to_string()),
                },
                NewDescriptionLintResult {
                    label: "Template".to_string(),
                    status: DescriptionLintStatus::Warn,
                    detail: Some("4 of 5 headings present".to_string()),
                },
            ],
        )
        .expect("the description lint");

    store
        .put_acceptance_criteria(
            review.id,
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
            ],
        )
        .expect("the criteria");

    store
        .put_file_changes(
            review.id,
            vec![
                NewFileChange {
                    path: "jobs/DeliverWebhook.php".to_string(),
                    summary: "The job releases itself back to the queue on failure.".to_string(),
                    lines_added: 96,
                    lines_removed: 22,
                    checked: FileCheck::ReadInFull,
                    how_checked: Some("Read in full. Compared with 2 sibling senders.".to_string()),
                },
                NewFileChange {
                    path: "migrations/add_attempts.php".to_string(),
                    summary: "Adds an attempts column to deliveries.".to_string(),
                    lines_added: 21,
                    lines_removed: 0,
                    checked: FileCheck::ReadInPart,
                    how_checked: Some("Read the up migration.".to_string()),
                },
                NewFileChange {
                    path: "resources/admin/**".to_string(),
                    summary: "Not changed by this pull request.".to_string(),
                    lines_added: 0,
                    lines_removed: 0,
                    checked: FileCheck::NotReviewed,
                    how_checked: Some(
                        "Not reviewed. The change does not touch the admin screens.".to_string(),
                    ),
                },
            ],
        )
        .expect("the change map");

    store
        .put_draft(
            review.id,
            NewDraft {
                // **Written in parts**, as a session writes a draft: the summary
                // sentence, a block per finding in the draft, and the not-checked line.
                // The body is what they compose to, which is why none is given here.
                body: String::new(),
                sections: Some(vec![
                    review_desk_store::NewDraftSection::prose(
                        "The queue-with-backoff approach fits the ticket.",
                    ),
                    review_desk_store::NewDraftSection::finding(
                        1,
                        "A retried delivery is sent without an idempotency key",
                        "A customer whose endpoint answers slowly receives the event twice.",
                    ),
                    review_desk_store::NewDraftSection::prose(
                        "**Not checked:** worker behaviour across a deploy restart.",
                    ),
                ]),
                status: review_desk_store::DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(118),
                    word_budget: Some(150),
                    findings_in_body: Some(3),
                    findings_expected: Some(3),
                    raised_before_on_pr: Some(0),
                    errors: None,
                    checked_at: Some(now()),
                }),
            },
        )
        .expect("a draft");
    store
        .request_nit_ticket(review.id, Some("ABC-240"))
        .expect("a nit ticket");

    // A second review, in progress on another repository, for the Inbox's groups.
    let other_review = store
        .create_review(NewReview {
            repo: "acme/billing-api".to_string(),
            pr: 88,
            head_sha: "bbbbbbb".to_string(),
            round: 2,
            title: Some("Cache the permissions lookup".to_string()),
            // A ticket and its address, because the Inbox's in-progress item serves them
            // now and the example this is checked against has one. A null here and a value
            // there is the disagreement this file exists to catch.
            ticket_id: Some("ABC-244".to_string()),
            ticket_url: Some("https://linear.app/acme/issue/ABC-244".to_string()),
            ..Default::default()
        })
        .expect("another review");
    store
        .set_review_links(
            other_review.id,
            review_desk_store::ReviewLinks {
                session_id: Some("ses_7f2a91c4".to_string()),
                ..Default::default()
            },
        )
        .expect("the session running it");
    store
        .set_review_layer(
            other_review.id,
            5,
            "running".parse().expect("a state"),
            Some("6 findings"),
        )
        .expect("a layer");

    // A third review, holding at layer 2 for a confirmation. Without one the Inbox's
    // first group is empty and nothing in it could be checked — which is what the
    // first run of this test found.
    let waiting_review = store
        .create_review(NewReview {
            repo: "acme/portal".to_string(),
            pr: 415,
            head_sha: "eeeeeee".to_string(),
            round: 1,
            title: Some("Remove the legacy importer".to_string()),
            ticket_id: Some("ABC-244".to_string()),
            // The address too, because the Inbox's brief item serves it and the example
            // this is checked against has one. A null here and a value there is the
            // disagreement this file exists to catch.
            ticket_url: Some("https://linear.app/acme/issue/ABC-244".to_string()),
            ..Default::default()
        })
        .expect("a review waiting on a confirmation");
    store
        .put_brief(
            waiting_review.id,
            NewBrief {
                problem: "The legacy importer reads another tenant's rows.".to_string(),
                ..Default::default()
            },
        )
        .expect("its brief");
    store
        .set_review_layer(
            waiting_review.id,
            2,
            "waiting_on_you".parse().expect("a state"),
            Some("nothing else runs until you confirm"),
        )
        .expect("holding at layer 2");

    // **A fourth review, finished with**: drafted, signed off and reported posted. It is
    // what `GET /timings` lists — "finished" there means posted — and without one that
    // example would be an empty array, which has no fields and so would check nothing.
    let posted_review = store
        .create_review(NewReview {
            repo: "acme/portal".to_string(),
            pr: 401,
            head_sha: "9d41c0e".to_string(),
            round: 1,
            title: Some("Hold the queue connection open".to_string()),
            ticket_id: Some("ABC-219".to_string()),
            ticket_url: None,
            changed_lines: Some(86),
            session_id: Some("ses_41b7c208".to_string()),
            ledger_id: Some("ledger-2026-10-06-401".to_string()),
            github_review_id: Some("gh-review-3291".to_string()),
        })
        .expect("a review that was posted");
    store
        .put_brief(
            posted_review.id,
            NewBrief {
                problem: "Each delivery opens its own connection to the queue.".to_string(),
                ..Default::default()
            },
        )
        .expect("its brief");
    store
        .confirm_brief(posted_review.id, Actor::Developer)
        .expect("the developer confirmed it");
    for number in [1, 3, 4, 5] {
        store
            .set_review_layer(
                posted_review.id,
                number,
                "running".parse().expect("a state"),
                None,
            )
            .expect("the step starts");
        store
            .set_review_layer(
                posted_review.id,
                number,
                "done".parse().expect("a state"),
                Some("done"),
            )
            .expect("and finishes");
    }
    store
        .put_draft(
            posted_review.id,
            NewDraft::ready("The connection is held open."),
        )
        .expect("its draft");
    store
        .approve_draft(posted_review.id, ReviewEvent::Approve, Actor::Developer)
        .expect("signed off");
    store
        .post_draft(posted_review.id, "gh-review-3291")
        .expect("the session reported it posted");
    store
        .set_review_layer(
            posted_review.id,
            7,
            "done".parse().expect("a state"),
            Some("2 outcomes harvested"),
        )
        .expect("the harvest");

    // **A fifth review, posted and answered**: the one state `posted_awaiting_author`'s
    // three figures are measurements in. The fourth review above is posted and never
    // harvested, which is the same group's unmeasured item — so the two together are both
    // halves of the one rule that matters here, that a nought and an absence are told
    // apart. A different pull request from any above, because the group serves only a
    // pull request's newest round and a second round of #401 would evict it.
    let answered_review = store
        .create_review(NewReview {
            repo: "acme/portal".to_string(),
            pr: 398,
            head_sha: "7c1d4e8".to_string(),
            round: 1,
            title: Some("Split the export job".to_string()),
            ticket_id: Some("ABC-204".to_string()),
            ticket_url: None,
            changed_lines: Some(512),
            session_id: Some("ses_2a7e9014".to_string()),
            ledger_id: Some("ledger-2026-10-08-398".to_string()),
            github_review_id: None,
        })
        .expect("a review the author answered");
    let answered_seq = store
        .add_finding(
            answered_review.id,
            NewFinding {
                lens: "boundary".to_string(),
                severity: Severity::Should,
                claim: "The chunk size is read before the config is bound".to_string(),
                file: Some("jobs/ExportRows.php".to_string()),
                line: Some(44),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .expect("a finding to be replied to")
        .seq;
    store
        .put_draft(
            answered_review.id,
            NewDraft::composed(vec![review_desk_store::NewDraftSection::finding(
                answered_seq,
                "The chunk size is read before the config is bound",
                "The first export after a deploy uses the default size.",
            )]),
        )
        .expect("its draft");
    store
        .approve_draft(
            answered_review.id,
            ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("signed off");
    store
        .post_draft(answered_review.id, "PRR_kwDOA1")
        .expect("the session reported it posted");
    // **The comment the finding was posted as.** The harvester reaches a thread through
    // this and reads none without it, so it is what makes a reply count a measurement
    // rather than a nought over something nobody could see.
    store
        .set_finding_github_comment(answered_review.id, answered_seq, "IC_kwDOA1")
        .expect("the comment it was posted as");
    for (kind, actor, at) in [
        // The author's own two acts. Counted by actor: the harvester records a reply from
        // any other login against the developer, and the group is about the author.
        (OutcomeKind::Replied, Actor::Author, 1_760_000_100),
        (OutcomeKind::ThreadResolved, Actor::Author, 1_760_000_200),
        // The harvester's own observation about the code, which is nobody's act.
        (
            OutcomeKind::CodeChangedAfter,
            Actor::Harvester,
            1_760_000_300,
        ),
    ] {
        store
            .add_outcome(
                FindingRef {
                    review_id: answered_review.id,
                    seq: answered_seq,
                },
                NewOutcome {
                    kind,
                    actor,
                    detail: None,
                    occurred_at: Some(at),
                },
            )
            .expect("what came back");
    }
    store
        .set_review_pr_state(answered_review.id, review_desk_store::PrState::Open, None)
        .expect("still open at the last harvest");
    store
        .mark_review_harvested(answered_review.id, 1_760_000_400)
        .expect("the harvester has looked");

    // A proposal to decide on, a rule with an accepted exception, and a deferred one.
    let proposed_rule = store
        .create_rule(NewRule {
            statement: "Do not raise rename-for-clarity suggestions".to_string(),
            prevents: "a review spending a round on naming".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Proposed,
            origin_review_id: Some(review.id),
            origin_findings: vec![FindingRef {
                review_id: review.id,
                seq: 6,
            }],
            why_proposed: Some("You cut this from 5 of the last 5 drafts.".to_string()),
            note: Some("Would suppress, not delete.".to_string()),
        })
        .expect("a proposal");

    let excepted_rule = store
        .create_rule(NewRule {
            statement: "Money is stored as whole cents".to_string(),
            prevents: "an invoice and its lines disagreeing by a cent".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: Some(review.id),
            origin_findings: vec![FindingRef {
                review_id: review.id,
                seq: 1,
            }],
            why_proposed: None,
            note: Some("Author disagreed once, with a reason you accepted.".to_string()),
        })
        .expect("a rule with an exception");
    store
        .add_rule_exception(excepted_rule.id, "accepted for the reporting tables")
        .expect("an exception");
    store
        .defer_rule(
            excepted_rule.id,
            now() + 60 * 86_400,
            Some("#415 had not merged."),
        )
        .expect("kept for longer");
    // A later finding citing it, so that a rule which has been both cited and kept for
    // longer exists — which is what the deferred example is.
    store
        .add_finding(
            other_review.id,
            NewFinding {
                lens: "money".to_string(),
                severity: Severity::Should,
                file: Some("app/Invoice.php".to_string()),
                line: Some(31),
                claim: "a total is held as a float".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: Some(excepted_rule.id),
                ..Default::default()
            },
        )
        .expect("a later citation");

    // A rule in force that nothing has cited lately, for the Rules screen's "Going
    // stale" group. Its times are moved backwards by the caller, because the store has
    // no function for ageing a rule and should not grow one so a test can pass.
    let stale_rule = store
        .create_rule(NewRule {
            statement: "Legacy importer calls need the tenant guard".to_string(),
            prevents: "an importer reading another tenant's rows".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: Some(review.id),
            origin_findings: vec![FindingRef {
                review_id: review.id,
                seq: 2,
            }],
            why_proposed: None,
            note: Some("No finding has cited it in 60 days.".to_string()),
        })
        .expect("a rule going stale");

    Seeded {
        review,
        other_review,
        waiting_review,
        posted_review,
        answered_review,
        proposed_rule,
        excepted_rule,
        stale_rule,
    }
}

/// Move a rule's times far enough back that nothing has cited it lately.
///
/// In the database's own SQL, as `tests/migrations.rs` seeds an older schema with:
/// there is no store function for ageing a rule, and adding one so that a test can
/// reach a state the real world reaches by waiting would be a function nothing else
/// would ever call.
fn age(path: &Path, rule_id: i64) {
    let conn = rusqlite::Connection::open(path).expect("the database");
    let long_ago = now() - 200 * 86_400;
    conn.execute(
        "UPDATE rules SET created_at = ?2, last_cited_at = ?2 WHERE id = ?1",
        rusqlite::params![rule_id, long_ago],
    )
    .expect("age the rule");
}

// ---------------------------------------------------------------------------
// The server's shape, built out of the store. Times stay as the integers the store
// holds; converting them is the server's job at its own boundary.
// ---------------------------------------------------------------------------

/// The tidy's proposal, as the server serialises it.
fn duplicate_rounds_document(store: &Store, repo: Option<&str>) -> Value {
    let found = store.duplicate_rounds(repo).expect("the proposal");
    json!({
        "pull_requests": found
            .iter()
            .map(|one| json!({
                "repo": one.repo,
                "pr": one.pr,
                "title": one.title,
                "newest": {
                    "review_id": one.newest.review_id,
                    "round": one.newest.round,
                    "head_sha": one.newest.head_sha,
                },
                "extras": one
                    .extras
                    .iter()
                    .map(|extra| json!({
                        "review_id": extra.review_id,
                        "round": extra.round,
                        "head_sha": extra.head_sha,
                        "created_at": extra.created_at,
                        "holds": extra.holds,
                        "act": extra.act,
                    }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "pull_requests_affected": found.len(),
        "extra_rounds": found.iter().map(|one| one.extras.len()).sum::<usize>(),
    })
}

fn review_document(review: &Review) -> Value {
    json!({
        "id": review.id,
        "repo": review.repo,
        "pr": review.pr,
        "head_sha": review.head_sha,
        "round": review.round,
        "title": review.title,
        "ticket_id": review.ticket_id,
        "ticket_url": review.ticket_url,
        "changed_lines": review.changed_lines,
        "session_id": review.session_id,
        "ledger_id": review.ledger_id,
        "github_review_id": review.github_review_id,
        "created_at": review.created_at,
        "pr_state": review.pr_state.map(|state| state.as_str()),
        "pr_state_at": review.pr_state_at,
        "harvested_at": review.harvested_at,
        "dismissed_at": review.dismissed_at,
        "dismissed_reason": review.dismissed_reason,
        "out_of_play": review.out_of_play.map(|why| why.as_str()),
        "out_of_play_at": review.out_of_play_at,
    })
}

fn brief_document(brief: &Brief) -> Value {
    json!({
        "review_id": brief.review_id,
        "problem": brief.problem,
        "diagram": brief.diagram,
        "options": brief.options.iter().map(|option| json!({
            "key": option.key,
            "title": option.title,
            "argument_for": option.argument_for,
            "argument_against": option.argument_against,
            "chosen": option.chosen,
            "proposed_by": option.proposed_by.as_str(),
            "verdicts": option.verdicts.iter().map(|verdict| json!({
                "problem": verdict.problem,
                "verdict": verdict.verdict.as_str(),
                "why": verdict.why,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "approach_verdict": brief.approach_verdict,
        "provenance": brief.provenance,
        // Derived: true exactly when there is a confirmation time.
        "confirmed": brief.confirmed_at.is_some(),
        "confirmed_at": brief.confirmed_at,
        "confirmation_required": brief.confirmation_required,
        "confirmation_not_required_because": brief.confirmation_not_required_because,
        "headline": brief.headline,
        "how_it_works_today": brief.how_it_works_today,
        "flow_before_caption": brief.flow_before_caption,
        "flow_after_caption": brief.flow_after_caption,
        "flow_before": flow_document(&brief.flow_before),
        "flow_after": flow_document(&brief.flow_after),
        "problems": brief.problems.iter().map(|problem| json!({
            "number": problem.number,
            "label": problem.label,
            "was_wrong": problem.was_wrong,
            "now_fixed": problem.now_fixed,
            "detail_title": problem.detail_title,
            "detail_before": problem.detail_before,
            "detail_after": problem.detail_after,
        })).collect::<Vec<_>>(),
        "support_table": brief.support_table.as_ref().map(|table| json!({
            "title": table.title,
            "columns": table.columns,
            "rows": table.rows,
        })),
        "blind_pass": brief.blind_pass.as_ref().map(|pass| json!({
            "given": pass.given,
            "pick_key": pass.pick_key,
            "pick_why": pass.pick_why,
            "looked_up": pass.looked_up,
            "questions": pass.questions,
        })),
        // Derived from the record, the way `confirmed` is.
        "blind_pass_state": match brief.blind_pass_state() {
            BlindPassState::NotRun => "not_run",
            BlindPassState::RanAndFoundNothing => "ran_and_found_nothing",
            BlindPassState::Ran => "ran",
        },
        "parts": brief.parts.iter().map(|part| json!({
            "title": part.title,
            "summary": part.summary,
            "fixes": part.fixes,
            "files": part.files.iter().map(|file| json!({
                "path": file.path,
                "lines_added": file.lines_added,
                "lines_removed": file.lines_removed,
                "in_diff": file.in_diff,
            })).collect::<Vec<_>>(),
            // Summed on the way out, so a card's figure cannot disagree with its list.
            "lines_added": part.files.iter().map(|file| file.lines_added).sum::<i64>(),
            "lines_removed": part.files.iter().map(|file| file.lines_removed).sum::<i64>(),
        })).collect::<Vec<_>>(),
        "open_choices": brief.open_choices.iter().map(|choice| json!({
            "left_open": choice.left_open,
            "chosen": choice.chosen,
            "why": choice.why,
            "departs_from_ticket": choice.departs_from_ticket,
            "finding_seq": choice.finding_seq,
            "finding_key": choice.finding_seq.map(|seq| format!("F{seq}")),
        })).collect::<Vec<_>>(),
        "disagreements": brief.disagreements.iter().map(|clash| json!({
            "description_says": clash.description_says,
            "code_does": clash.code_does,
            "finding_seq": clash.finding_seq,
            "finding_key": format!("F{}", clash.finding_seq),
        })).collect::<Vec<_>>(),
        "updated_at": brief.updated_at,
    })
}

/// The problem half of a brief, built the way the server builds it.
///
/// **Five fields, named rather than subtracted.** A field added to the brief cannot reach
/// this document by accident, which is the same guarantee the store's own
/// `problem_statement` gives.
fn problem_statement_document(statement: &ProblemStatement) -> Value {
    json!({
        "review_id": statement.review_id,
        "problem": statement.problem,
        "how_it_works_today": statement.how_it_works_today,
        "flow_before_caption": statement.flow_before_caption,
        "flow_before": flow_document(&statement.flow_before),
        "problems": statement.problems.iter().map(|problem| json!({
            "number": problem.number,
            "was_wrong": problem.was_wrong,
        })).collect::<Vec<_>>(),
    })
}

/// One flow, as rows of boxes.
fn flow_document(flow: &[BriefFlowRow]) -> Value {
    json!(flow
        .iter()
        .map(|row| row
            .iter()
            .map(|flow_box| json!({
                "title": flow_box.title,
                "note": flow_box.note,
                "problem": flow_box.problem,
            }))
            .collect::<Vec<_>>())
        .collect::<Vec<_>>())
}

fn finding_document(store: &Store, finding: &Finding) -> Value {
    // The three facts the store derives rather than stores, read the way the server reads
    // them: one call for the whole round. A per-finding read here would pass while the
    // server's one call was broken.
    let found = store
        .findings_in_round(finding.review_id)
        .expect("the round's findings")
        .into_iter()
        .find(|found| found.finding.seq == finding.seq)
        .expect("the finding is in its own round");

    json!({
        "review_id": finding.review_id,
        "seq": finding.seq,
        // Derived: `F` and the sequence number.
        "key": format!("F{}", finding.seq),
        "lens": finding.lens,
        "severity": finding.severity,
        "file": finding.file,
        "line": finding.line,
        "line_sha": finding.line_sha,
        "claim": finding.claim,
        "title": finding.title,
        "evidence": finding.evidence,
        "disposition": finding.disposition,
        "disposition_reason": finding.disposition_reason,
        "github_comment_id": finding.github_comment_id,
        "rule_id": finding.rule_id,
        // Derived: `R-` and the rule id, zero-padded to two digits.
        "rule_key": finding.rule_id.map(|id| format!("R-{id:02}")),
        "hunk": finding.hunk.as_ref().map(|hunk| json!({
            "file": hunk.file,
            "start_line": hunk.start_line,
            "lines": hunk
                .numbered_lines()
                .into_iter()
                // Derived: the number beside each line, from the start line and the
                // markers. A removed line is not in the file any more and takes none.
                .map(|(number, line)| json!({
                    "marker": line.marker,
                    "line": number,
                    "text": line.text,
                }))
                .collect::<Vec<_>>(),
        })),
        "flagged_line": finding.flagged_line,
        "inline_comment": finding.inline_comment,
        "why_it_matters": finding.why_it_matters,
        "proof": finding.proof,
        "proof_note": finding.proof_note,
        "verification": finding.verification,
        "suggested_fix": finding.suggested_fix,
        "fix_references": finding.fix_references,
        "created_at": finding.created_at,
        "round_status": finding.round_status,
        "round_status_by": finding.round_status_by,
        "round_status_checked": finding.round_status_checked,
        "round_status_at": finding.round_status_at,
        "round_status_commit": finding.round_status_commit,
        "continues_review_id": finding.continues_review_id,
        "introduced_by_commit": finding.introduced_by_commit,
        "introduced_by_seq": finding.introduced_by_seq,
        // Derived: `F` and the number it was introduced by.
        "introduced_by_key": finding.introduced_by_seq.map(|seq| format!("F{seq}")),
        // Derived from the carry chain and the round's commits.
        "continues_round": found.continues_round,
        "rounds_open": found.rounds_open,
        "late": found.late,
    })
}

fn round_document(round: &review_desk_store::RoundSummary) -> Value {
    json!({
        "review_id": round.review_id,
        "round": round.round,
        "head_sha": round.head_sha,
        "created_at": round.created_at,
        "dismissed": round.dismissed,
        "posted_at": round.posted_at,
        "event": round.event,
        "findings": round.findings,
        "commits": round.commits,
        "replies": round.replies,
        "threads_resolved": round.threads_resolved,
        "statuses": {
            "new": round.statuses.new,
            "fixed": round.statuses.fixed,
            "still_open": round.statuses.still_open,
            "accepted": round.statuses.accepted,
            "withdrawn": round.statuses.withdrawn,
            "ticketed": round.statuses.ticketed,
        },
        "brief_written": round.brief_written,
    })
}

fn round_commit_document(commit: &review_desk_store::RoundCommit) -> Value {
    json!({
        "sha": commit.sha,
        "position": commit.position,
        "summary": commit.summary,
        "committed_at": commit.committed_at,
        "files": commit.files,
    })
}

fn outcome_document(outcome: &Outcome) -> Value {
    json!({
        "id": outcome.id,
        "review_id": outcome.review_id,
        "finding_seq": outcome.finding_seq,
        "finding_key": format!("F{}", outcome.finding_seq),
        "kind": outcome.kind,
        "actor": outcome.actor,
        "occurred_at": outcome.occurred_at,
        "detail": outcome.detail,
    })
}

fn layer_document(layer: &ReviewLayer) -> Value {
    json!({
        "number": layer.number,
        "name": layer.name,
        "state": layer.state,
        "detail": layer.detail,
        "updated_at": layer.updated_at,
        "runs": layer.runs.iter().map(run_document).collect::<Vec<_>>(),
        "seconds": layer.seconds(),
    })
}

/// One pass a step made. The store derives these from its record of the states the step
/// entered; nothing here computes a duration of its own.
fn run_document(run: &ReviewLayerRun) -> Value {
    json!({
        "state": run.state,
        "started_at": run.started_at,
        "ended_as": run.ended_as,
        "finished_at": run.finished_at,
        "seconds": run.seconds(),
        "detail": run.detail,
    })
}

/// How long one review took. The four figures DEV-890 records, and nothing per-step: those
/// are on the layers, with the passes they were computed from.
fn timings_document(timings: &ReviewTimings) -> Value {
    json!({
        "requested_at": timings.requested_at,
        "draft_ready_at": timings.draft_ready_at,
        "signed_off_at": timings.signed_off_at,
        "posted_at": timings.posted_at,
        "with_the_agent_seconds": timings.with_the_agent_seconds,
        "waiting_on_you_seconds": timings.waiting_on_you_seconds,
        "last_activity_at": timings.last_activity_at,
    })
}

/// One row of `GET /timings`: a finished review, its figures and its seven steps.
fn review_timing_rows(store: &Store) -> Value {
    json!({
        "reviews": store
            .finished_review_timings(None)
            .expect("the finished reviews")
            .iter()
            .map(|(review, timings)| json!({
                "review": review_document(review),
                "timings": timings_document(timings),
                "layers": timings.layers.iter().map(layer_document).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// `GET /reviews`: every review, newest first, with the step it has reached.
fn review_list(store: &Store) -> Value {
    let shown = ReviewsShown::default();
    json!({
        // The repository asked about, echoed back. `None` here, for every repository.
        "repo": Value::Null,
        // The choice this answer was composed for, echoed back the same way.
        "show": shown.as_str(),
        "reviews": store
            .reviews_at_step(None, shown)
            .expect("every review")
            .iter()
            .map(|at| {
                let mut row = json!({
                    "review": review_document(&at.review),
                    "rounds_on_pr": at.rounds_on_pr,
                    "steps_total": ReviewLayerName::COUNT,
                    "last_activity_at": at.review.last_activity_at,
                    // How it left the desk, and when its pull request ended. Null on a
                    // row in play, which every row of the seeded record is.
                    "left_the_desk": at.left_the_desk.map(|left| match left {
                        LeftTheDesk::Dismissed => "dismissed",
                        LeftTheDesk::Finished => "finished",
                        LeftTheDesk::OutOfPlay => "out_of_play",
                        LeftTheDesk::Superseded => "superseded",
                    }),
                    "finished_how": at.finished_how.map(|state| state.as_str()),
                    "finished_at": at.finished_at,
                });
                // Absent together, for a review nothing has reported on at all: a step
                // number of 1 would say the lint is pending, which is a different claim.
                if let Some(step) = &at.step {
                    let object = row.as_object_mut().expect("an object");
                    object.insert("step_number".to_string(), json!(step.number));
                    object.insert("step_name".to_string(), json!(step.name.as_str()));
                    object.insert("step_state".to_string(), json!(step.state.as_str()));
                    object.insert("step_detail".to_string(), json!(step.detail));
                }
                row
            })
            .collect::<Vec<_>>(),
    })
}

/// The contract's `NotForSigning`, or null.
fn not_for_signing_document(why: Option<NotForSigning>) -> Value {
    match why {
        None => Value::Null,
        Some(NotForSigning::Dismissed) => json!({ "reason": "dismissed" }),
        Some(NotForSigning::Finished { how, at }) => json!({
            "reason": "finished",
            "how": how.as_str(),
            "at": at,
        }),
        Some(NotForSigning::Superseded { by_round }) => json!({
            "reason": "superseded",
            "by_round": by_round,
        }),
    }
}

fn nit_ticket_document(ticket: &NitTicket) -> Value {
    json!({
        "review_id": ticket.review_id,
        "status": ticket.status,
        "ticket_id": ticket.ticket_id,
        "finding_seqs": ticket.finding_seqs,
        "requested_at": ticket.requested_at,
        "filed_at": ticket.filed_at,
    })
}

fn draft_document(draft: &Draft) -> Value {
    json!({
        "review_id": draft.review_id,
        "body": draft.body,
        "status": draft.status,
        "event": draft.event,
        "approved_at": draft.approved_at,
        "sent_back_note": draft.sent_back_note,
        "sent_back_at": draft.sent_back_at,
        "posted_at": draft.posted_at,
        "signoff_withdrawn_at": draft.signoff_withdrawn_at,
        "signoff_withdrawn_head": draft.signoff_withdrawn_head,
        "updated_at": draft.updated_at,
    })
}

/// The contract's `DraftSection`. The heading is composed, which is the whole point of it.
fn draft_section_document(section: &DraftSection) -> Value {
    json!({
        "position": section.position,
        "kind": section.kind,
        "finding_seq": section.finding_seq,
        "finding_key": section.finding_seq.map(|seq| format!("F{seq}")),
        "severity": section.severity,
        "heading": match (section.severity, section.finding_seq, &section.title) {
            (Some(severity), Some(seq), Some(title)) => {
                Value::String(finding_heading(severity, seq, title))
            }
            _ => Value::Null,
        },
        "title": section.title,
        "text": section.text,
        "agent_title": section.agent_title,
        "agent_text": section.agent_text,
        "rewritten": section.rewritten(),
        "posted": section.posted,
    })
}

fn lint_document(lint: &DraftLint) -> Value {
    json!({
        "passes": lint.passes.unwrap_or(false),
        "visible_words": lint.visible_words,
        "word_budget": lint.word_budget,
        "findings_in_body": lint.findings_in_body,
        "findings_expected": lint.findings_expected,
        "raised_before_on_pr": lint.raised_before_on_pr,
        // One stored string, split for the wire: the store holds what the lint
        // printed, and splitting it is a display decision.
        "errors": lint.errors.as_deref().map(|errors| {
            errors.lines().map(str::to_string).collect::<Vec<_>>()
        }).unwrap_or_default(),
        "checked_at": lint.checked_at,
    })
}

fn severity_counts(findings: &[Finding]) -> Value {
    let count = |severity: Severity| {
        findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    };
    json!({
        "blocker": count(Severity::Blocker),
        "should": count(Severity::Should),
        "nit": count(Severity::Nit),
        "question": count(Severity::Question),
    })
}

fn disposition_counts(findings: &[Finding]) -> Value {
    let count = |disposition: Disposition| {
        findings
            .iter()
            .filter(|finding| finding.disposition == disposition)
            .count()
    };
    json!({
        "in_draft": count(Disposition::InDraft),
        "held_back": count(Disposition::HeldBack),
        "suppressed": count(Disposition::Suppressed),
    })
}

fn rule_document(store: &Store, rule: &Rule) -> Value {
    let counts = store.rule_counts(rule.id).expect("rule counts");
    let origin_pr = rule
        .origin_review_id
        .and_then(|id| store.find_review_by_id(id).expect("the origin review"))
        .map(|review| review.pr);
    // Derived: stale when nothing has cited it lately and it is not being kept.
    let stale = match (rule.last_cited_at, rule.review_again_at) {
        (_, Some(until)) => now() > until,
        (Some(at), None) => now() - at > 60 * 86_400,
        (None, None) => now() - rule.created_at > 60 * 86_400,
    };

    json!({
        "id": rule.id,
        // Derived: `R-` and the id, zero-padded to two digits.
        "key": format!("R-{:02}", rule.id),
        "statement": rule.statement,
        "prevents": rule.prevents,
        "scope": {
            "kind": rule.scope.as_str(),
            "repo": rule.scope.repo(),
        },
        "stage": rule.stage,
        // Derived from the stage and the exception count.
        "stage_label": match (rule.stage, counts.exceptions.unwrap_or(0)) {
            (RuleStage::Active, 0) => "Repository rule".to_string(),
            (RuleStage::Active, n) => format!("Repository rule · {n} exception"),
            (RuleStage::Proposed, _) => "Proposed".to_string(),
            (RuleStage::Lint, _) => "Lint check".to_string(),
            (RuleStage::Ci, _) => "CI check".to_string(),
            (RuleStage::Retired, _) => "Retired".to_string(),
        },
        "why_proposed": rule.why_proposed,
        "note": rule.note,
        "origin_review_id": rule.origin_review_id,
        "origin_pr": origin_pr,
        "origin_findings": rule.origin_findings.iter().map(|finding| json!({
            "review_id": finding.review_id,
            "seq": finding.seq,
        })).collect::<Vec<_>>(),
        "stale": stale,
        "last_cited_at": rule.last_cited_at,
        "review_again_at": rule.review_again_at,
        "counts": {
            "raised": counts.raised,
            "kept": counts.kept,
            "rejected_with_reason": counts.rejected_with_reason,
            "cited_by_findings": counts.cited_by_findings,
            "exceptions": counts.exceptions,
            "caught_by_check": counts.caught_by_check,
            "raised_in_a_review_since": counts.raised_in_a_review_since,
            "last_human_mention_at": counts.last_human_mention_at,
        },
        "created_at": rule.created_at,
    })
}

/// Layer 1's result on the pull request description, with the verdict derived from the
/// results — which is the only place it exists, by design.
fn description_lint_document(provenance: &ReviewProvenance) -> Value {
    match provenance.description_lint_passes() {
        None => Value::Null,
        Some(passes) => json!({
            "passes": passes,
            "results": provenance
                .description_lint
                .iter()
                .map(|result| json!({
                    "position": result.position,
                    "label": result.label,
                    "status": result.status,
                    "detail": result.detail,
                }))
                .collect::<Vec<_>>(),
        }),
    }
}

fn review_detail(store: &Store, review_id: i64) -> Value {
    let full = store.full_review(review_id).expect("the whole review");
    let mut document = Map::new();
    document.insert("review".to_string(), review_document(&full.review));
    // Why nothing on it may be signed off, or null. The One review screen offers the
    // brief's confirmation, which is one of the four acts a review off the desk refuses.
    document.insert(
        "not_for_signing".to_string(),
        not_for_signing_document(store.not_for_signing(review_id).expect("the standing")),
    );
    document.insert(
        "layers".to_string(),
        Value::Array(full.layers.iter().map(layer_document).collect()),
    );
    document.insert(
        "brief".to_string(),
        full.brief
            .as_ref()
            .map(brief_document)
            .unwrap_or(Value::Null),
    );
    document.insert(
        "findings".to_string(),
        Value::Array(
            full.findings
                .iter()
                .map(|finding| finding_document(store, finding))
                .collect(),
        ),
    );
    document.insert(
        "outcomes".to_string(),
        Value::Array(full.outcomes.iter().map(outcome_document).collect()),
    );
    document.insert(
        "finding_severities".to_string(),
        severity_counts(&full.findings),
    );
    document.insert(
        "finding_dispositions".to_string(),
        disposition_counts(&full.findings),
    );
    let provenance = store
        .review_provenance(review_id)
        .expect("where the review's understanding came from");

    // Which record counts as this review's coverage. The server owns this derivation, and
    // this is the store's own statement that both halves of it are answerable: the change
    // map, and the sentence it displaces.
    let source = if !provenance.files.is_empty() {
        "files"
    } else if full.review.not_checked.is_some() {
        "sentence"
    } else {
        "none"
    };

    document.insert(
        "not_checked".to_string(),
        match source {
            "sentence" => json!(full.review.not_checked.clone()),
            _ => Value::Null,
        },
    );
    document.insert(
        "sources".to_string(),
        Value::Array(
            provenance
                .sources
                .iter()
                .map(|source| {
                    json!({
                        "position": source.position,
                        "kind": source.kind,
                        "label": source.label,
                        "link": source.link,
                    })
                })
                .collect(),
        ),
    );
    document.insert(
        "criteria".to_string(),
        Value::Array(
            provenance
                .criteria
                .iter()
                .map(|criterion| {
                    json!({
                        "position": criterion.position,
                        "text": criterion.text,
                        "status": criterion.status,
                        "finding_seq": criterion.finding_seq,
                        "finding_key": criterion.finding_seq.map(|seq| format!("F{seq}")),
                        "file": criterion.file,
                        "line": criterion.line,
                        "note": criterion.note,
                    })
                })
                .collect(),
        ),
    );
    document.insert(
        "description_lint".to_string(),
        description_lint_document(&provenance),
    );
    document.insert(
        "coverage".to_string(),
        json!({
            "source": source,
            "files": provenance
                .files
                .iter()
                .map(|file| {
                    let seqs = store
                        .findings_on_path(review_id, &file.path)
                        .expect("the findings on the file");
                    json!({
                        "position": file.position,
                        "path": file.path,
                        "summary": file.summary,
                        "lines_added": file.lines_added,
                        "lines_removed": file.lines_removed,
                        "checked": file.checked,
                        "how_checked": file.how_checked,
                        "finding_seqs": seqs,
                        "finding_keys": seqs
                            .iter()
                            .map(|seq| format!("F{seq}"))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>(),
        }),
    );
    document.insert(
        "timings".to_string(),
        timings_document(&store.review_timings(review_id).expect("the timings")),
    );
    document.insert(
        "nit_ticket".to_string(),
        nit_ticket_document(&store.nit_ticket(review_id).expect("the nit ticket")),
    );
    document.insert(
        "draft_status".to_string(),
        full.draft
            .as_ref()
            .map(|draft| json!(draft.status))
            .unwrap_or(Value::Null),
    );
    document.insert(
        "rounds".to_string(),
        Value::Array(
            store
                .rounds(&full.review.repo, full.review.pr)
                .expect("the rounds")
                .iter()
                .map(round_document)
                .collect(),
        ),
    );
    // What has happened to this round's head commit. DEV-962, and the fourth question the
    // round-two page asks.
    let updates = store.head_updates(review_id).expect("the head updates");
    document.insert(
        "head_updates".to_string(),
        json!({
            "since_sha": updates.since_sha,
            "moved": updates.moved,
            "base_only": updates.base_only,
            "folded": updates.folded,
            "commits": updates.commits,
            "last_at": updates.last_at,
            "updates": updates
                .updates
                .iter()
                .map(|update| json!({
                    "kind": update.kind,
                    "from_sha": update.from_sha,
                    "to_sha": update.to_sha,
                    "at": update.at,
                }))
                .collect::<Vec<_>>(),
        }),
    );
    document.insert(
        "commits_since_previous_round".to_string(),
        Value::Array(
            store
                .list_round_commits(review_id)
                .expect("the round's commits")
                .iter()
                .map(round_commit_document)
                .collect(),
        ),
    );
    Value::Object(document)
}

/// The contract's `DraftThread`, as the server serialises one.
fn draft_thread_document(thread: &review_desk_store::DraftThread) -> Value {
    json!({
        "finding_seq": thread.finding_seq,
        "finding_key": format!("F{}", thread.finding_seq),
        "severity": thread.severity,
        "title": thread.title,
        "path": thread.path,
        "line": thread.line,
        "line_sha": thread.line_sha,
        "side": thread.side,
        "body": thread.body,
    })
}

fn draft_detail(store: &Store, review_id: i64) -> Value {
    let draft = store.get_draft(review_id).expect("the draft");
    let review = store.get_review(review_id).expect("the review");
    let findings = store.list_findings(review_id).expect("the findings");
    let outcomes = store.list_outcomes(review_id).expect("the outcomes");

    json!({
        "draft": draft_document(&draft),
        "review": review_document(&review),
        // Why this draft may not be signed off, or null when it may. The store answers
        // it and the page draws it, so a screen cannot offer a sign-off the API refuses.
        "not_for_signing": not_for_signing_document(
            store.not_for_signing(review_id).expect("the standing"),
        ),
        "lint": lint_document(&draft.lint),
        "sections": store
            .draft_sections(review_id)
            .expect("the blocks")
            .iter()
            .map(draft_section_document)
            .collect::<Vec<_>>(),
        // Whether the stored body is still what the blocks compose to. False on every
        // draft this build writes, and the one state the hand-over cannot serve. DEV-929.
        "body_is_stale": store
            .draft_body_is_stale(review_id)
            .expect("the body's standing"),
        // The inline threads the draft hands over, which the body does not carry: an
        // anchored finding is a row in the index and a comment on the code. DEV-929.
        "threads": store
            .draft_threads(review_id)
            .expect("the threads")
            .iter()
            .map(draft_thread_document)
            .collect::<Vec<_>>(),
        // The developer's edits are outcomes with actor `developer`, which is where
        // PR #1 already puts them.
        "edits": outcomes.iter().filter(|outcome| {
            outcome.actor == Actor::Developer
                && review_desk_store::DEVELOPER_DECISIONS.contains(&outcome.kind)
        }).map(|outcome| json!({
            "finding_seq": outcome.finding_seq,
            "finding_key": format!("F{}", outcome.finding_seq),
            "kind": outcome.kind,
            "summary": outcome.detail,
            "occurred_at": outcome.occurred_at,
        })).collect::<Vec<_>>(),
        "held_back": findings.iter().filter(|finding| {
            finding.disposition == Disposition::HeldBack
        }).map(|finding| json!({
            "finding_seq": finding.seq,
            "finding_key": format!("F{}", finding.seq),
            "claim": finding.claim,
            "severity": finding.severity,
        })).collect::<Vec<_>>(),
        "nit_ticket": nit_ticket_document(&store.nit_ticket(review_id).expect("the ticket")),
    })
}

fn inbox(store: &Store, seeded: &Seeded) -> Value {
    let pull_request =
        |review: &Review| json!({ "repo": review.repo, "pr": review.pr, "title": review.title });

    let briefs = store
        .reviews_with_unconfirmed_brief(None)
        .expect("briefs to confirm")
        .iter()
        .map(|review| {
            let brief = store.get_brief(review.id).expect("its brief");
            let layer = store
                .find_review_layer(review.id, 2)
                .expect("layer 2")
                .map(|layer| layer.state);
            json!({
                "review_id": review.id,
                "pull_request": pull_request(review),
                "ticket_id": review.ticket_id,
                "ticket_url": review.ticket_url,
                // Which round this brief belongs to; every item that makes a queue row
                // carries one.
                "round": review.round,
                "brief_ready_at": brief.updated_at,
                "blocks_later_layers": layer
                    == Some(review_desk_store::ReviewLayerState::WaitingOnYou),
            })
        })
        .collect::<Vec<_>>();

    let drafts = store
        .reviews_with_draft_status(review_desk_store::DraftStatus::Ready, None)
        .expect("drafts to approve")
        .iter()
        .map(|review| {
            let findings = store.list_findings(review.id).expect("its findings");
            // **The findings in the draft, not the review's.** This file was the third
            // copy of that figure and the second to get it wrong: the server counts
            // `in_draft`, the dashboard's sample records counted the whole review, and so
            // did this — in the file whose own header calls itself "the store's own
            // statement of which fields it owns". The contract said nothing either way
            // until DEV-930, which is how three answers to one question survived. A
            // held-back nit is not in the draft.
            let in_draft: Vec<Finding> = findings
                .iter()
                .filter(|finding| finding.disposition == Disposition::InDraft)
                .cloned()
                .collect();
            json!({
                "review_id": review.id,
                "pull_request": pull_request(review),
                "ticket_id": review.ticket_id,
                "ticket_url": review.ticket_url,
                "round": review.round,
                "severities": severity_counts(&in_draft),
                "held_back": store.held_back_findings(review.id).expect("held back").len(),
                // Derived: a later round reviews only what changed.
                "is_delta": review.round > 1,
                "draft_ready_at": store
                    .draft_ready_at(review.id)
                    .expect("when the draft became ready"),
            })
        })
        .collect::<Vec<_>>();

    let replies = store
        .reviews_with_author_activity(None, None)
        .expect("author replies")
        .iter()
        .map(|review| {
            let outcomes = store.list_outcomes(review.id).expect("its outcomes");
            // Which round the pull request has reached, and which review that is. The
            // queue's single row for a pull request is always its newest round, and this
            // is the one array that can be about an older one — DEV-983.
            let newest = store
                .newest_round(&review.repo, review.pr)
                .expect("the newest round")
                .expect("a pull request with a review has a newest round");
            json!({
                "review_id": review.id,
                "pull_request": pull_request(review),
                "ticket_id": review.ticket_id,
                "ticket_url": review.ticket_url,
                // Which round was replied to: the queue draws one row per pull request,
                // and a reply is on the round that was posted.
                "round": review.round,
                "rounds_on_pr": newest.round,
                "newest_round_review_id": newest.id,
                "events": outcomes.iter().filter(|outcome| {
                    matches!(outcome.actor, Actor::Author | Actor::Harvester)
                }).map(|outcome| json!({
                    "finding_seq": outcome.finding_seq,
                    "finding_key": format!("F{}", outcome.finding_seq),
                    "kind": outcome.kind,
                    "detail": outcome.detail,
                })).collect::<Vec<_>>(),
                "replied_at": outcomes
                    .iter()
                    .filter(|outcome| {
                        matches!(outcome.actor, Actor::Author | Actor::Harvester)
                    })
                    .map(|outcome| outcome.occurred_at)
                    .max(),
            })
        })
        .collect::<Vec<_>>();

    let in_progress = store
        .reviews_in_progress(None)
        .expect("in progress")
        .iter()
        .map(|at| {
            json!({
                "review_id": at.review.id,
                "pull_request": pull_request(&at.review),
                "ticket_id": at.review.ticket_id,
                "ticket_url": at.review.ticket_url,
                // Which round the session is working on; the fourth and last array with one.
                "round": at.review.round,
                "layer_number": at.layer.number,
                "layer_name": at.layer.name,
                "layers_total": ReviewLayerName::COUNT,
                "session_id": at.review.session_id,
                // No source: Allele owns whether the session is alive.
                "session_alive": Value::Null,
                // What the store can say instead: when this step's current pass began,
                // and when anything last wrote to the review.
                "in_state_since": at.layer.in_state_since(),
                "last_activity_at": at.review.last_activity_at,
            })
        })
        .collect::<Vec<_>>();

    let proposals = store
        .list_rules_at_stages(None, &[RuleStage::Proposed])
        .expect("proposals")
        .iter()
        .map(|rule| {
            json!({
                "rule_id": rule.id,
                "rule_key": format!("R-{:02}", rule.id),
                "statement": rule.statement,
                "why_proposed": rule.why_proposed,
                "scope": { "kind": rule.scope.as_str(), "repo": rule.scope.repo() },
            })
        })
        .collect::<Vec<_>>();

    // Posted, and with the author. **The figures plain, and the nulling left to the
    // server**, which is what `entities::inbox`'s header says the division is.
    //
    // A copy of the server's rule stood here and was deleted: it asserted nothing. `check`
    // walks the example's leaf paths and asserts one direction — a value in the example
    // must not be null from the store — and `at` searches every element of an array for a
    // non-null, so the one harvested item in `getInbox.response.json` satisfies it on its
    // own. A rule that can only turn a value *to* null could make this test harder to pass
    // and never easier. What makes it mean anything is the `answered_review` seed below,
    // which gives the store a review those figures are real measurements of.
    let posted = store
        .posted_reviews_awaiting_author(None)
        .expect("posted reviews awaiting their author")
        .iter()
        .map(|item| {
            json!({
                "review_id": item.review.id,
                "pull_request": pull_request(&item.review),
                "ticket_id": item.review.ticket_id,
                "ticket_url": item.review.ticket_url,
                "round": item.review.round,
                // The group holds only the newest round, so these are one number.
                "rounds_on_pr": item.review.round,
                "posted_at": item.posted_at,
                "posted_as": item.posted_as.map(|event| event.as_str()),
                "pr_state": item.review.pr_state.map(|state| state.as_str()),
                "harvested_at": item.review.harvested_at,
                "findings": item.findings,
                "findings_linked": item.findings_linked,
                "replies": item.response.replies,
                "threads_resolved": item.response.threads_resolved,
                "code_changed_after": item.response.code_changed_after,
            })
        })
        .collect::<Vec<_>>();

    let tickets = store.filed_nit_tickets(None, None).expect("filed tickets");
    // **The five waiting groups, and not the other two.** This file held the third answer
    // to that sum — it added the reviews in progress and left out the developer's own pull
    // requests — and survived the reconciliation in DEV-930 because `check` walks the
    // example's leaf *paths* and never its values. The server's own count and
    // `conformance.rs`'s assertion are the other two; all three say the same thing now.
    //
    // `own_prs` is the empty list below: nothing lists the developer's own pull requests.
    // Written as a term rather than left out, so that the day something supplies it, this
    // sum is already right.
    let own_prs: Vec<Value> = Vec::new();
    let items = briefs.len() + drafts.len() + replies.len() + own_prs.len() + proposals.len();
    let repos = store
        .list_repos()
        .expect("the repositories")
        .iter()
        .filter(|summary| summary.open_reviews > 0)
        .count();

    let _ = seeded;
    json!({
        "totals": { "items": items, "repos": repos },
        "briefs_to_confirm": briefs,
        "drafts_to_approve": drafts,
        "author_replies": replies,
        // No source: nothing lists the developer's own pull requests.
        "own_prs_without_reviewer": own_prs,
        "proposed_rules": proposals,
        "in_progress": in_progress,
        "posted_awaiting_author": posted,
        "handled_without_you": {
            "window_days": 7,
            // No source: the description lint runs in CI.
            "description_problems_caught": Value::Null,
            "nits_filed": tickets.iter().map(|t| t.finding_seqs.len()).sum::<usize>(),
            "nit_tickets": tickets.len(),
        },
    })
}

fn rules_page(store: &Store, repo: Option<&str>) -> Value {
    let summary = store.rules_summary(repo, 30).expect("the summary");
    let in_force = store
        .list_rules_at_stages(repo, &[RuleStage::Active, RuleStage::Lint, RuleStage::Ci])
        .expect("the rules in force")
        .iter()
        .map(|rule| rule_document(store, rule))
        .collect::<Vec<_>>();
    let (stale, active): (Vec<_>, Vec<_>) = in_force
        .into_iter()
        .partition(|rule| rule["stale"] == json!(true));

    json!({
        "repo": repo,
        "summary": {
            "active_rules": summary.active_rules,
            "active_rules_cap": summary.active_rules_cap,
            "replay_matched": summary.replay_matched,
            "replay_total": summary.replay_total,
            "findings_kept": summary.findings_kept,
            "findings_total": summary.findings_total,
            "findings_window_days": summary.findings_window_days,
            "raised_twice_on_one_pr": summary.raised_twice_on_one_pr,
        },
        "proposed": store
            .list_rules_at_stages(repo, &[RuleStage::Proposed])
            .expect("the proposals")
            .iter()
            .map(|rule| rule_document(store, rule))
            .collect::<Vec<_>>(),
        "active": active,
        "stale": stale,
    })
}

fn repos_page(store: &Store) -> Value {
    json!({
        "repos": store.list_repos().expect("the repositories").iter().map(|summary| json!({
            "repo": summary.repo,
            "open_reviews": summary.open_reviews,
            "active_rules": summary.active_rules,
        })).collect::<Vec<_>>(),
    })
}

/// Check one example against the document the store can build for it, and say how many
/// fields were checked.
fn check(name: &str, built: &Value) -> usize {
    let path = examples().join(format!("{name}.response.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let example: Value = serde_json::from_str(&text).expect("the example is JSON");

    let mut wanted = BTreeSet::new();
    leaf_paths(&example, "", &mut wanted);
    assert!(
        !wanted.is_empty(),
        "{name} has no fields, so this check is vacuous"
    );

    let no_source = |path: &str| NO_SOURCE.iter().any(|(listed, _)| *listed == path);

    for field in &wanted {
        if no_source(field)
            || NO_SOURCE
                .iter()
                .any(|(listed, _)| field.starts_with(listed))
        {
            // Listed, not faked: the store must not be inventing a value for it.
            if let Some(value) = at(built, field) {
                assert!(
                    value.is_null() || value == &json!([]) || value == &json!({}),
                    "{name}{field} has no source, so it must be null rather than \
                     {value} — see contract/README.md"
                );
            }
            continue;
        }

        let produced = at(built, field).unwrap_or_else(|| {
            panic!(
                "{name}{field} is in the contract and the store cannot produce it, and \
                 it is not listed as having no source"
            )
        });

        if !at(&example, field).expect("the example's value").is_null() {
            assert!(
                !produced.is_null(),
                "{name}{field} is a value in the example and null from the store"
            );
        }
    }

    wanted.len()
}

#[test]
fn every_response_example_can_be_produced_from_the_store() {
    let (dir, store) = temp_store();
    let seeded = seed(&store);
    age(&common::db_path(&dir), seeded.stale_rule.id);

    let review = seeded.review.id;
    let detail = review_detail(&store, review);
    let mut fields = 0;

    fields += check("getReview", &detail);
    fields += check("getInbox", &inbox(&store, &seeded));
    fields += check("getDraft", &draft_detail(&store, review));
    fields += check("saveDraft", &draft_detail(&store, review));
    fields += check("listRepos", &repos_page(&store));
    fields += check("listRules", &rules_page(&store, Some("acme/portal")));
    fields += check("listReviewTimings", &review_timing_rows(&store));
    fields += check("listReviews", &review_list(&store));

    // The groups those two checks read must not have been empty, or they checked
    // nothing: an empty array has no leaf paths, so a vacuous group would pass
    // silently. Asserted here, before anything below retires a rule out of one.
    assert!(
        !store
            .reviews_in_progress(None)
            .expect("in progress")
            .is_empty(),
        "review {} should be in progress",
        seeded.other_review.id
    );
    assert!(
        !store
            .reviews_with_unconfirmed_brief(None)
            .expect("briefs to confirm")
            .is_empty(),
        "review {} should be waiting on a confirmation",
        seeded.waiting_review.id
    );
    assert!(
        !store
            .posted_reviews_awaiting_author(None)
            .expect("posted reviews")
            .is_empty(),
        "review {} should be posted and waiting on its author",
        seeded.answered_review.id
    );
    let page = rules_page(&store, Some("acme/portal"));
    for group in ["proposed", "active", "stale"] {
        assert!(
            page[group]
                .as_array()
                .is_some_and(|rules| !rules.is_empty()),
            "the Rules page's {group} group is empty, so nothing in it was checked"
        );
    }
    fields += check(
        "startReview",
        &review_document(&store.get_review(review).expect("the review")),
    );
    fields += check(
        "confirmBrief",
        &brief_document(&store.get_brief(review).expect("the brief")),
    );
    fields += check(
        "getProblemStatement",
        &problem_statement_document(&store.problem_statement(review).expect("the problem half")),
    );
    fields += check(
        "setFindingDisposition",
        &finding_document(&store, &store.get_finding(review, 4).expect("the finding")),
    );
    fields += check(
        "requestNitTicket",
        &nit_ticket_document(&store.nit_ticket(review).expect("the ticket")),
    );
    fields += check(
        "deferRule",
        &rule_document(&store, &store.get_rule(seeded.excepted_rule.id).unwrap()),
    );

    // The three stage changes, each read back as the endpoint that made it returns it.
    // They come after the Rules page check, because retiring a rule takes it out of
    // the page's `active` group.
    let promoted = store
        .set_rule_stage(seeded.proposed_rule.id, RuleStage::Active, None)
        .expect("promote");
    fields += check("promoteRule", &rule_document(&store, &promoted));

    let retired = store
        .set_rule_stage(
            seeded.stale_rule.id,
            RuleStage::Retired,
            Some("the legacy importer is removed in #415"),
        )
        .expect("retire");
    fields += check("retireRule", &rule_document(&store, &retired));

    let dismissed = store
        .set_rule_stage(
            seeded.proposed_rule.id,
            RuleStage::Retired,
            Some("two of the five I cut were in generated code"),
        )
        .expect("dismiss");
    fields += check("dismissRule", &rule_document(&store, &dismissed));

    // The two drafts that need a status the seeded one is not at.
    store
        .send_back_draft(review, "F2 is the option map talking, not the diff.")
        .expect("send back");
    fields += check("sendBackDraft", &draft_detail(&store, review));

    // Reworked with its lint re-run, which is what the contract says a save does. A
    // body written without one has no lint, deliberately: see
    // `rewriting_a_draft_clears_a_lint_that_described_the_old_body`.
    // The developer's decision, and the draft recomposed from it. **Made before the
    // draft is approved**, which is the only state a decision can be made in.
    //
    // **The blocks the example carries, and the cut it shows.** The example's own document
    // is this draft with F2 cut — F1 stays, and since DEV-929 F1 is anchored, so the
    // document has a thread as well as an index. A draft of F1's block alone, which is
    // what this wrote before, could produce neither once F1 was the finding being cut.
    store
        .put_draft(
            review,
            NewDraft::composed(vec![
                review_desk_store::NewDraftSection::prose(
                    "The queue-with-backoff approach fits the ticket.",
                ),
                review_desk_store::NewDraftSection::finding(
                    1,
                    "A retried delivery is sent without an idempotency key",
                    "A customer whose endpoint answers slowly receives the event twice.",
                ),
                review_desk_store::NewDraftSection::finding(
                    2,
                    "No way to replay a delivery after the window closes",
                    "After 24 hours a failed delivery is marked dead.",
                ),
                review_desk_store::NewDraftSection::finding(
                    3,
                    "The failure test passes with the retry code reverted",
                    "So it does not pin the behaviour.",
                ),
            ]),
        )
        .expect("a draft to decide about");
    let made = store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some("Already covered by the rule.".to_string()),
                ..Decision::of(FindingDecision::Cut)
            },
            Actor::Developer,
        )
        .expect("the developer cuts F2");
    fields += check(
        "decideFinding",
        &json!({
            "finding": finding_document(&store, &made.finding),
            "outcome": outcome_document(&made.outcome),
            "draft": draft_detail(&store, review),
        }),
    );

    // Kept after all, which is the decision track's own move and is what lets the rework
    // below raise F1 again: a suppressed finding cannot be written into a draft.
    store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("kept after all");

    store
        .put_draft(
            review,
            NewDraft {
                // In parts, like the first write: the examples of a draft carry finding
                // blocks, and a rework that dropped to one block of prose would make
                // `approveDraft`'s own example unproducible.
                body: String::new(),
                sections: Some(vec![
                    review_desk_store::NewDraftSection::prose("Reworked, as asked."),
                    review_desk_store::NewDraftSection::finding(
                        1,
                        "A retried delivery is sent without an idempotency key",
                        "A customer whose endpoint answers slowly receives the event twice.",
                    ),
                ]),
                status: review_desk_store::DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(104),
                    word_budget: Some(150),
                    findings_in_body: Some(3),
                    findings_expected: Some(3),
                    raised_before_on_pr: Some(0),
                    errors: None,
                    checked_at: Some(now()),
                }),
            },
        )
        .expect("rework");
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("approve");
    fields += check("approveDraft", &draft_detail(&store, review));

    // And "ticket what is left, and approve", on a review of its own: it approves, and so
    // does `approveDraft` above, and one draft cannot be approved twice two ways.
    let last = store
        .create_review(NewReview {
            repo: "acme/portal".to_string(),
            pr: 413,
            head_sha: "ddddddd".to_string(),
            round: 1,
            title: Some("Record the delivery attempt count".to_string()),
            ticket_id: Some("ABC-247".to_string()),
            ticket_url: Some("https://linear.app/acme/issue/ABC-247".to_string()),
            changed_lines: Some(64),
            ..Default::default()
        })
        .expect("a review to ticket what is left of")
        .id;
    store
        .set_review_links(
            last,
            review_desk_store::ReviewLinks {
                session_id: Some("ses_a1b2c3d4".to_string()),
                ledger_id: Some("ledger-2026-10-08-413".to_string()),
                ..Default::default()
            },
        )
        .expect("the other systems' ids");
    store
        .add_finding(
            last,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "The attempt count is written outside the transaction".to_string(),
                ..Default::default()
            },
        )
        .expect("an open finding");
    let seq = store.list_findings(last).expect("the findings")[0].seq;
    store
        .put_draft(
            last,
            NewDraft::composed(vec![review_desk_store::NewDraftSection::finding(
                seq,
                "The attempt count is written outside the transaction",
                "A crash between the two leaves the count disagreeing with the deliveries.",
            )]),
        )
        .expect("a draft");
    let settled = store
        .ticket_rest_and_approve(last, Some("one ticket for an agent"), Actor::Developer)
        .expect("ticket what is left, and approve");
    fields += check(
        "ticketRestAndApprove",
        &json!({
            "draft": draft_detail(&store, last),
            "nit_ticket": nit_ticket_document(&settled.ticket),
            "ticketed": settled.ticketed,
            "ticketed_keys": settled
                .ticketed
                .iter()
                .map(|seq| format!("F{seq}"))
                .collect::<Vec<_>>(),
        }),
    );

    // **The tidy's two, on a pull request of their own.** Written through `create_review`
    // because `open_review` cannot make a second unposted round since DEV-962 — which is
    // the rule the tidy exists to clean up after — and the fold has to come straight after
    // the proposal that names it, because folding changes what the proposal says.
    let mut extras = Vec::new();
    for (round, head, at) in [(1, "3f9c2ab", 17), (2, "9d02e71", 20), (3, "1b7c4e0", 22)] {
        let _ = at;
        extras.push(
            store
                .create_review(review_desk_store::NewReview {
                    repo: "acme/portal".to_string(),
                    pr: 414,
                    head_sha: head.to_string(),
                    round,
                    title: Some("Retry failed webhook deliveries".to_string()),
                    ..Default::default()
                })
                .expect("a round, as the rule before DEV-962 left them")
                .id,
        );
    }
    store
        .put_draft(
            extras[0],
            NewDraft::ready("One finding on the retry path.\n"),
        )
        .expect("round 1 holds a draft, so the tidy would dismiss it rather than fold it");
    fields += check(
        "listDuplicateRounds",
        &duplicate_rounds_document(&store, None),
    );
    fields += check(
        "foldRound",
        &json!({
            "folded": review_document(
                &store
                    .fold_round(extras[1])
                    .expect("round 2 holds nothing, so it folds")
                    .folded,
            ),
            "into": review_document(&store.get_review(extras[2]).expect("the newest round")),
        }),
    );

    // **The three removals come last, and deliberately**: the first two write to the
    // seeded review and the third deletes a review outright, so anything above them
    // would be checking a record they had moved. Each is the document the server answers
    // with, which for all three is the review itself.
    fields += check(
        "dismissReview",
        &review_document(
            &store
                .dismiss_review(
                    review,
                    "the pull request was closed unmerged, so there is nothing left to act on",
                )
                .expect("dismiss the review"),
        ),
    );
    fields += check(
        "restoreReview",
        &review_document(&store.restore_review(review).expect("restore it")),
    );
    // A review of its own, with nothing written to it — which is the only review a delete
    // is offered for, and is the shape of the stray review DEV-945 was raised from.
    let stray = store
        .open_review("acme/portal", 777, "4f1a9c2")
        .expect("a review nobody started")
        .review
        .id;
    fields += check(
        "deleteReview",
        &review_document(&store.delete_review(stray).expect("delete it")),
    );

    // Said out loud, so that a refactor which quietly stops checking most of the
    // contract fails here rather than passing. The twenty-six examples carry well over
    // three hundred leaf fields between them.
    assert!(
        fields >= 300,
        "only {fields} contract fields were checked against the store"
    );
}

#[test]
fn every_response_example_is_checked() {
    // A new endpoint's example must be added to the test above rather than silently
    // skipped. This is what makes the check above exhaustive over the directory.
    let checked: BTreeSet<&str> = [
        "getReview",
        "getInbox",
        "getDraft",
        "saveDraft",
        "listRepos",
        "listRules",
        "listReviewTimings",
        "listReviews",
        "startReview",
        "confirmBrief",
        "getProblemStatement",
        "setFindingDisposition",
        "requestNitTicket",
        "promoteRule",
        "deferRule",
        "retireRule",
        "dismissRule",
        "approveDraft",
        "sendBackDraft",
        "decideFinding",
        "ticketRestAndApprove",
        "dismissReview",
        "restoreReview",
        "deleteReview",
        "listDuplicateRounds",
        "foldRound",
    ]
    .into_iter()
    .collect();

    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(examples()).expect("contract/examples") {
        let path = entry.expect("an entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if let Some(endpoint) = name.strip_suffix(".response.json") {
            found.insert(endpoint.to_string());
        }
    }

    let missing = found
        .iter()
        .filter(|endpoint| !checked.contains(endpoint.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "these response examples are not checked against the store: {missing:?}"
    );
    assert_eq!(
        found.len(),
        checked.len(),
        "and none of the checks is for an example that is gone"
    );
}

#[test]
fn times_are_integers_in_the_store_and_the_server_converts_them() {
    // The one mismatch the contract warns will look like a bug the first time somebody
    // serialises a store record straight out of this crate. Recorded as a test so that
    // it is a known boundary rather than a surprise.
    let (_dir, store) = temp_store();
    let seeded = seed(&store);

    let review = review_document(&store.get_review(seeded.review.id).unwrap());
    assert!(
        review["created_at"].is_i64(),
        "the store holds epoch seconds; RFC 3339 is the server's conversion"
    );
}
