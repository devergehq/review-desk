//! What a finding carries, over the wire: the hunk, the comment as it will post, the
//! consequence, the proof and the fix.
//!
//! `conformance.rs` already validates this server's `Finding` against the contract's
//! schema, which catches a field with the wrong *shape*. This file checks the two things
//! a schema cannot: that the derived line numbers are the ones the design draws, and that
//! the text arrives byte for byte as it was stored. Review Desk does not fetch code, so a
//! server that tidied a hunk on the way out would be the last thing between the agent's
//! evidence and the developer reading something that was never in the pull request.

mod common;

use review_desk_store::{
    FindingHunk, HunkLine, HunkLineMarker, NewFinding, Severity, Store, Verification,
};
use serde_json::Value;

/// A review with one finding on it, written by `write`, and the finding as served.
fn served(store: &Store, write: impl FnOnce() -> NewFinding) -> (i64, NewFinding) {
    let review = store
        .open_review("acme/portal", 874, "c0ffee1")
        .expect("a review")
        .review
        .id;
    let new = write();
    store.add_finding(review, new.clone()).expect("a finding");
    (review, new)
}

/// The one finding of a review, as the API serves it.
fn one_finding(harness: &common::Harness, review: i64) -> Value {
    let answer = harness.api("GET", &format!("/reviews/{review}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()["findings"][0].clone()
}

#[test]
fn the_hunk_is_served_with_the_numbers_board_a2_draws() {
    let harness = common::start_with_no_github();
    let (review, _) = served(&harness.store, || NewFinding {
        lens: "sibling diff".to_string(),
        severity: Severity::Blocker,
        claim: "no idempotency key".to_string(),
        hunk: Some(FindingHunk {
            file: "jobs/DeliverWebhook.php".to_string(),
            start_line: 56,
            lines: vec![
                HunkLine {
                    marker: HunkLineMarker::Unchanged,
                    text: "public function handle(WebhookClient $client): void".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Unchanged,
                    text: "{".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Removed,
                    text: "    $client->post($this->endpoint->url);".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Added,
                    text: "    $response = $client->post($this->endpoint->url);".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Unchanged,
                    text: "}".to_string(),
                },
            ],
        }),
        flagged_line: Some(58),
        ..Default::default()
    });

    let finding = one_finding(&harness, review);
    let lines = finding["hunk"]["lines"].as_array().expect("lines").clone();

    assert_eq!(finding["hunk"]["file"], "jobs/DeliverWebhook.php");
    assert_eq!(finding["hunk"]["start_line"], 56);
    assert_eq!(
        lines
            .iter()
            .map(|line| line["line"].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::from(56),
            Value::from(57),
            Value::Null,
            Value::from(58),
            Value::from(59),
        ],
        "an added or unchanged line takes the next number; the removed line takes none, \
         because it is not in the file any more — which is what board A2 draws"
    );
    assert_eq!(finding["flagged_line"], 58);
}

#[test]
fn the_hunk_text_is_served_exactly_as_it_was_stored() {
    let harness = common::start_with_no_github();
    let awkward = [
        ("unchanged", "\tif ($response->failed()) {"),
        ("added", "    $this->release($delay);   "),
        ("added", "   "),
        ("added", ""),
        ("unchanged", "    // naïve — déjà vu, 日本語 🤖"),
    ];
    let (review, _) = served(&harness.store, || NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Nit,
        claim: "awkward text".to_string(),
        hunk: Some(FindingHunk {
            file: "src/odd.rs".to_string(),
            start_line: 1,
            lines: awkward
                .iter()
                .map(|(marker, text)| HunkLine {
                    marker: marker.parse().expect("a marker"),
                    text: (*text).to_string(),
                })
                .collect(),
        }),
        proof: Some("  leading kept\n\ttab line\n   \ntrailing  ".to_string()),
        inline_comment: Some("\tIndented, trailing space ".to_string()),
        why_it_matters: Some(" spaces either side ".to_string()),
        suggested_fix: Some("Use the delivery id.\n".to_string()),
        fix_references: vec![" SendInvoiceEmail.php:44 ".to_string()],
        ..Default::default()
    });

    let finding = one_finding(&harness, review);
    let lines = finding["hunk"]["lines"].as_array().expect("lines").clone();

    for (index, (marker, text)) in awkward.iter().enumerate() {
        assert_eq!(&lines[index]["marker"], marker);
        assert_eq!(
            &lines[index]["text"], text,
            "line {index} came back altered"
        );
    }
    assert_eq!(
        finding["proof"],
        "  leading kept\n\ttab line\n   \ntrailing  "
    );
    assert_eq!(finding["inline_comment"], "\tIndented, trailing space ");
    assert_eq!(finding["why_it_matters"], " spaces either side ");
    assert_eq!(finding["suggested_fix"], "Use the delivery id.\n");
    assert_eq!(
        finding["fix_references"][0], " SendInvoiceEmail.php:44 ",
        "a reference is checked for being blank and otherwise left alone"
    );
}

#[test]
fn a_finding_with_no_hunk_serves_null_rather_than_an_invented_one() {
    let harness = common::start_with_no_github();
    let (review, _) = served(&harness.store, || NewFinding {
        lens: "approach".to_string(),
        severity: Severity::Should,
        file: Some("src/lib.rs".to_string()),
        line: Some(12),
        claim: "the shape agents already send".to_string(),
        ..Default::default()
    });

    let finding = one_finding(&harness, review);

    // Every field is written, and every absent one is null. `FindingHunk` requires a
    // file, a start line and a line, so there is no shape of the object that means
    // "none" — inventing one would break the contract's own rule about a made-up value.
    assert!(finding["hunk"].is_null(), "{finding}");
    assert!(finding["flagged_line"].is_null());
    assert!(finding["inline_comment"].is_null());
    assert!(finding["why_it_matters"].is_null());
    assert!(finding["proof"].is_null());
    assert!(finding["suggested_fix"].is_null());

    // The two that are never null, and are not null-shaped either.
    assert_eq!(
        finding["verification"], "unknown",
        "a finding that said nothing about its method has said nothing"
    );
    assert_eq!(
        finding["fix_references"],
        Value::Array(vec![]),
        "empty, not null: a finding with no references has none, which is a measurement"
    );

    // And what it did carry is untouched.
    assert_eq!(finding["file"], "src/lib.rs");
    assert_eq!(finding["line"], 12);
}

#[test]
fn moving_a_finding_out_of_the_draft_serves_its_evidence_back() {
    let harness = common::start_with_no_github();
    let (review, _) = served(&harness.store, || NewFinding {
        lens: "repetition".to_string(),
        severity: Severity::Nit,
        claim: "backoff repeated".to_string(),
        hunk: Some(FindingHunk {
            file: "config/webhooks.php".to_string(),
            start_line: 12,
            lines: vec![HunkLine {
                marker: HunkLineMarker::Unchanged,
                text: "    'backoff' => [30, 120, 600],".to_string(),
            }],
        }),
        proof: Some("read both files".to_string()),
        verification: Verification::ReadNotRun,
        ..Default::default()
    });

    // The one write the dashboard makes against a finding. It answers with the finding,
    // and that answer has to carry what the finding is about or the card it came from
    // would redraw itself empty.
    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{review}/findings/1"),
        Some(r#"{"disposition": "held_back", "reason": "goes in the nit ticket"}"#),
    );
    assert_eq!(answer.status, 200, "{}", answer.text());

    let finding = answer.json();
    assert_eq!(finding["disposition"], "held_back");
    assert_eq!(finding["hunk"]["file"], "config/webhooks.php");
    assert_eq!(finding["hunk"]["lines"][0]["line"], 12);
    assert_eq!(finding["verification"], "read_not_run");
}
