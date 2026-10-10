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
    now, Actor, Brief, BriefOption, Disposition, Draft, DraftLint, Finding, FindingHunk,
    FindingRef, HunkLine, HunkLineMarker, NewBrief, NewDraft, NewFinding, NewOutcome, NewReview,
    NewRule, NitTicket, Outcome, OutcomeKind, Review, ReviewEvent, ReviewFacts, ReviewLayer,
    ReviewLayerName, Rule, RuleScope, RuleStage, Severity, Store, Verification,
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
                changed_lines: Some(214),
                not_checked: Some(
                    "The queue worker's behaviour under a deploy restart was not run.".to_string(),
                ),
            },
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

    for (number, state, detail) in [
        (1, "done", "passed"),
        (2, "done", "confirmed by you"),
        (3, "done", "3 options"),
        (4, "done", "done"),
        (5, "done", "6 findings"),
        (6, "waiting_on_you", "waiting on you"),
    ] {
        store
            .set_review_layer(
                review.id,
                number,
                state.parse().expect("a state"),
                Some(detail),
            )
            .expect("a layer");
    }

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
                    })
                    .collect(),
                option_map: None,
                ..Default::default()
            },
        )
        .expect("a brief");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer confirms it");

    // Six findings: three in the draft, two held back, one suppressed — the
    // dispositions the One review example shows.
    let finding =
        |severity: Severity, disposition: Disposition, rule: Option<i64>, claim: &str| NewFinding {
            lens: "sibling diff".to_string(),
            severity,
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(58),
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

    store
        .put_draft(
            review.id,
            NewDraft {
                body: "The queue-with-backoff approach fits the ticket.\n".to_string(),
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

fn review_document(review: &Review) -> Value {
    json!({
        "id": review.id,
        "repo": review.repo,
        "pr": review.pr,
        "head_sha": review.head_sha,
        "round": review.round,
        "title": review.title,
        "ticket_id": review.ticket_id,
        "changed_lines": review.changed_lines,
        "session_id": review.session_id,
        "ledger_id": review.ledger_id,
        "github_review_id": review.github_review_id,
        "created_at": review.created_at,
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
        })).collect::<Vec<_>>(),
        "approach_verdict": brief.approach_verdict,
        "provenance": brief.provenance,
        // Derived: true exactly when there is a confirmation time.
        "confirmed": brief.confirmed_at.is_some(),
        "confirmed_at": brief.confirmed_at,
        "confirmation_required": brief.confirmation_required,
        "confirmation_not_required_because": brief.confirmation_not_required_because,
        "updated_at": brief.updated_at,
    })
}

fn finding_document(finding: &Finding) -> Value {
    json!({
        "review_id": finding.review_id,
        "seq": finding.seq,
        // Derived: `F` and the sequence number.
        "key": format!("F{}", finding.seq),
        "lens": finding.lens,
        "severity": finding.severity,
        "file": finding.file,
        "line": finding.line,
        "claim": finding.claim,
        "evidence": finding.evidence,
        "disposition": finding.disposition,
        "disposition_reason": finding.disposition_reason,
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
    })
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
        "updated_at": draft.updated_at,
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

fn review_detail(store: &Store, review_id: i64) -> Value {
    let full = store.full_review(review_id).expect("the whole review");
    let mut document = Map::new();
    document.insert("review".to_string(), review_document(&full.review));
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
        Value::Array(full.findings.iter().map(finding_document).collect()),
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
    document.insert(
        "not_checked".to_string(),
        json!(full.review.not_checked.clone()),
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
    Value::Object(document)
}

fn draft_detail(store: &Store, review_id: i64) -> Value {
    let draft = store.get_draft(review_id).expect("the draft");
    let review = store.get_review(review_id).expect("the review");
    let findings = store.list_findings(review_id).expect("the findings");
    let outcomes = store.list_outcomes(review_id).expect("the outcomes");

    json!({
        "draft": draft_document(&draft),
        "review": review_document(&review),
        "lint": lint_document(&draft.lint),
        // The developer's edits are outcomes with actor `developer`, which is where
        // PR #1 already puts them.
        "edits": outcomes.iter().filter(|outcome| {
            outcome.actor == Actor::Developer
                && matches!(
                    outcome.kind,
                    OutcomeKind::Cut | OutcomeKind::Regraded | OutcomeKind::Rewritten
                )
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
            json!({
                "review_id": review.id,
                "pull_request": pull_request(review),
                "round": review.round,
                "severities": severity_counts(&findings),
                "held_back": store.held_back_findings(review.id).expect("held back").len(),
                // Derived: a later round reviews only what changed.
                "is_delta": review.round > 1,
            })
        })
        .collect::<Vec<_>>();

    let replies = store
        .reviews_with_author_activity(None, None)
        .expect("author replies")
        .iter()
        .map(|review| {
            let outcomes = store.list_outcomes(review.id).expect("its outcomes");
            json!({
                "review_id": review.id,
                "pull_request": pull_request(review),
                "events": outcomes.iter().filter(|outcome| {
                    matches!(outcome.actor, Actor::Author | Actor::Harvester)
                }).map(|outcome| json!({
                    "finding_seq": outcome.finding_seq,
                    "finding_key": format!("F{}", outcome.finding_seq),
                    "kind": outcome.kind,
                    "detail": outcome.detail,
                })).collect::<Vec<_>>(),
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
                "layer_number": at.layer.number,
                "layer_name": at.layer.name,
                "layers_total": ReviewLayerName::COUNT,
                "session_id": at.review.session_id,
                // No source: Allele owns whether the session is alive.
                "session_alive": Value::Null,
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

    let tickets = store.filed_nit_tickets(None, None).expect("filed tickets");
    let items = briefs.len() + drafts.len() + replies.len() + proposals.len() + in_progress.len();
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
        "own_prs_without_reviewer": [],
        "proposed_rules": proposals,
        "in_progress": in_progress,
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
        "setFindingDisposition",
        &finding_document(&store.get_finding(review, 4).expect("the finding")),
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
    store
        .put_draft(
            review,
            NewDraft {
                body: "reworked".to_string(),
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

    // Said out loud, so that a refactor which quietly stops checking most of the
    // contract fails here rather than passing. The sixteen examples carry well over
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
        "startReview",
        "confirmBrief",
        "setFindingDisposition",
        "requestNitTicket",
        "promoteRule",
        "deferRule",
        "retireRule",
        "dismissRule",
        "approveDraft",
        "sendBackDraft",
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
