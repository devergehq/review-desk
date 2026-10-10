//! `finding add` carrying what the finding is about, from one JSON document.
//!
//! The command line is what agent sessions call, so this is the surface DEV-874 has to
//! land on: a review session writes a finding and everything the dashboard shows about it
//! in one call, and reads it back unchanged.
//!
//! **Nothing here signs anything off.** `finding add` records; it does not confirm,
//! approve or decide, and this change adds no command that does.

mod common;

use common::{open_review, Desk};
use serde_json::json;

/// The finding board A2 draws, as the JSON a session would pipe in.
fn board_a2_finding() -> serde_json::Value {
    json!({
        "lens": "sibling diff",
        "severity": "blocker",
        "file": "jobs/DeliverWebhook.php",
        "line": 58,
        "claim": "A retried delivery is sent without an idempotency key",
        "evidence": "throwaway test, 2 deliveries for 1 event",
        "hunk": {
            "file": "jobs/DeliverWebhook.php",
            "start_line": 56,
            "lines": [
                {"marker": "unchanged", "text": "public function handle(WebhookClient $client): void"},
                {"marker": "unchanged", "text": "{"},
                {"marker": "removed", "text": "    $client->post($this->endpoint->url, $this->payload);"},
                {"marker": "added", "text": "    $response = $client->post($this->endpoint->url, $this->payload);"},
                {"marker": "added", "text": ""},
                {"marker": "added", "text": "    if ($response->failed()) {"},
                {"marker": "added", "text": "        $this->release($this->delayFor($this->attempts()));"},
                {"marker": "added", "text": "    }"},
                {"marker": "unchanged", "text": "}"}
            ]
        },
        "flagged_line": 58,
        "inline_comment": "This retry re-sends the payload with no idempotency key, so an endpoint that accepted the first attempt slowly gets the event twice.",
        "why_it_matters": "A customer whose endpoint answers slowly is charged, emailed or updated twice for one event.",
        "proof": "endpoint accepts attempt 1 after 31s\nqueue retries at 30s\ndeliveries for evt_1: 2, expected 1",
        "proof_note": "A throwaway test, not committed.",
        "verification": "reproduced",
        "suggested_fix": "Send the delivery id as the key, as the two sibling senders do.",
        "fix_references": ["SendInvoiceEmail.php:44", "SyncToLedger.php:71"]
    })
}

/// Pipe a finding in and hand back the document `--json` printed.
fn add(desk: &Desk, review: &str, document: serde_json::Value) -> serde_json::Value {
    let output = desk
        .command()
        .args([
            "finding", "add", "--review", review, "--file", "-", "--json",
        ])
        .write_stdin(document.to_string())
        .assert()
        .success();
    let text = String::from_utf8(output.get_output().stdout.clone()).expect("output is text");
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("one document: {error}\n{text}"))
}

#[test]
fn all_six_fields_go_in_from_one_document_and_come_back_unchanged() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    let printed = add(&desk, &review, board_a2_finding());
    let finding = &printed["finding"];

    assert_eq!(finding["seq"], 1);
    assert_eq!(finding["hunk"]["file"], "jobs/DeliverWebhook.php");
    assert_eq!(finding["hunk"]["start_line"], 56);
    assert_eq!(finding["hunk"]["lines"].as_array().expect("lines").len(), 9);
    assert_eq!(finding["hunk"]["lines"][2]["marker"], "removed");
    assert_eq!(finding["hunk"]["lines"][4]["text"], "");
    assert_eq!(finding["flagged_line"], 58);
    assert_eq!(finding["verification"], "reproduced");
    assert_eq!(finding["proof_note"], "A throwaway test, not committed.");
    assert_eq!(
        finding["proof"],
        "endpoint accepts attempt 1 after 31s\nqueue retries at 30s\ndeliveries for evt_1: 2, expected 1"
    );
    assert_eq!(
        finding["fix_references"],
        json!(["SendInvoiceEmail.php:44", "SyncToLedger.php:71"])
    );

    // And `review show` reads the same finding back out of the record, which is what a
    // later round, the harvester and the dashboard all do. `FullReview` serialises the
    // findings at the top level, so this is the record's own copy, not the write's answer.
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        &shown["findings"][0], finding,
        "what was printed is what the record holds"
    );
}

#[test]
fn awkward_text_survives_the_round_trip_through_the_command_line() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    // Tabs, trailing spaces, a line of only whitespace, an empty line and non-ASCII —
    // through JSON, through the store, and back out as JSON.
    let printed = add(
        &desk,
        &review,
        json!({
            "lens": "correctness",
            "severity": "nit",
            "claim": "awkward text",
            "hunk": {
                "file": "src/odd.rs",
                "start_line": 1,
                "lines": [
                    {"marker": "unchanged", "text": "\tif (x) {"},
                    {"marker": "added", "text": "    let y = 1;   "},
                    {"marker": "added", "text": "   "},
                    {"marker": "added", "text": ""},
                    {"marker": "added", "text": "    // naïve — déjà vu, 日本語 🤖"},
                    {"marker": "unchanged", "text": "}"}
                ]
            },
            "proof": "  leading kept\n\ttab line\n   \nnaïve ✓\ntrailing  ",
            "proof_note": "  Trailing and leading, kept.  ",
            "inline_comment": "\tIndented, trailing space ",
            "fix_references": [" src/other.rs:4 "]
        }),
    );
    let lines = &printed["finding"]["hunk"]["lines"];

    assert_eq!(lines[0]["text"], "\tif (x) {");
    assert_eq!(lines[1]["text"], "    let y = 1;   ");
    assert_eq!(lines[2]["text"], "   ");
    assert_eq!(lines[3]["text"], "");
    assert_eq!(lines[4]["text"], "    // naïve — déjà vu, 日本語 🤖");
    assert_eq!(
        printed["finding"]["proof"],
        "  leading kept\n\ttab line\n   \nnaïve ✓\ntrailing  "
    );
    assert_eq!(
        printed["finding"]["inline_comment"],
        "\tIndented, trailing space "
    );
    assert_eq!(
        printed["finding"]["fix_references"],
        json!([" src/other.rs:4 "])
    );
}

#[test]
fn a_finding_with_none_of_it_is_still_accepted() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    let printed = add(
        &desk,
        &review,
        json!({
            "lens": "correctness",
            "severity": "should",
            "claim": "the shape agents already send",
            "file": "src/lib.rs",
            "line": 12
        }),
    );
    let finding = &printed["finding"];

    assert_eq!(finding["hunk"], json!(null));
    assert_eq!(finding["flagged_line"], json!(null));
    assert_eq!(finding["inline_comment"], json!(null));
    assert_eq!(finding["why_it_matters"], json!(null));
    assert_eq!(finding["proof"], json!(null));
    assert_eq!(finding["proof_note"], json!(null));
    assert_eq!(
        finding["verification"], "unknown",
        "saying nothing about the method is `unknown`, which is a value and not a guess"
    );
    assert_eq!(finding["suggested_fix"], json!(null));
    assert_eq!(finding["fix_references"], json!([]));
}

#[test]
fn a_misspelled_field_is_refused_rather_than_dropped() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    // The whole reason `NewFinding` denies unknown fields: a misspelled `proof` is a
    // finding with no proof, which is worse than a refusal, and this is the only chance
    // anyone gets to notice — Review Desk cannot go and fetch it later.
    for wrong in ["prooof", "hunk_file", "why_this_matters", "fix_reference"] {
        let refused = desk
            .command()
            .args([
                "finding", "add", "--review", &review, "--file", "-", "--json",
            ])
            .write_stdin(
                json!({
                    "lens": "correctness",
                    "severity": "should",
                    "claim": "a misspelling",
                    wrong: "something"
                })
                .to_string(),
            )
            .assert()
            .code(1);
        let text = String::from_utf8(refused.get_output().stdout.clone()).expect("text");
        assert!(
            text.contains(wrong),
            "the refusal names the field it did not know: {text}"
        );
    }
}

#[test]
fn a_comment_anchored_outside_the_hunk_is_refused_with_the_lines_it_could_have_been() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    let mut document = board_a2_finding();
    document["flagged_line"] = json!(64);

    let refused = desk
        .command()
        .args([
            "finding", "add", "--review", &review, "--file", "-", "--json",
        ])
        .write_stdin(document.to_string())
        .assert()
        .code(1);
    let text = String::from_utf8(refused.get_output().stdout.clone()).expect("text");

    assert!(text.contains("flagged_line"), "{text}");
    assert!(text.contains("56 to 63"), "{text}");
}

#[test]
fn a_hunk_line_with_a_newline_in_it_is_refused_at_the_command_line() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    // The mistake this catches: building the hunk by joining diff output, which is easy
    // to reach for when `proof` beside it legitimately takes newlines.
    let refused = desk
        .command()
        .args([
            "finding", "add", "--review", &review, "--file", "-", "--json",
        ])
        .write_stdin(
            json!({
                "lens": "correctness",
                "severity": "nit",
                "claim": "a joined diff",
                "hunk": {
                    "file": "src/lib.rs",
                    "start_line": 1,
                    "lines": [{"marker": "added", "text": "one
two
three"}]
                }
            })
            .to_string(),
        )
        .assert()
        .code(1);
    let text = String::from_utf8(refused.get_output().stdout.clone()).expect("text");

    assert!(text.contains("hunk.lines.text"), "{text}");
    assert!(text.contains("one file line"), "{text}");
}

#[test]
fn the_help_names_the_closed_sets_and_the_verbatim_rule() {
    let desk = Desk::new();
    let help = desk.run(&["finding", "add", "--help"]);

    for expected in [
        "hunk",
        "flagged_line",
        "inline_comment",
        "why_it_matters",
        "proof",
        "proof_note",
        "verification",
        "suggested_fix",
        "fix_references",
        "reproduced",
        "read_not_run",
        "unknown",
        "does not fetch code",
    ] {
        assert!(
            help.contains(expected),
            "`finding add --help` does not mention {expected:?}:\n{help}"
        );
    }
}

#[test]
fn an_unknown_verification_is_refused_and_names_the_three() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "874", "c0ffee1");

    let refused = desk
        .command()
        .args([
            "finding", "add", "--review", &review, "--file", "-", "--json",
        ])
        .write_stdin(
            json!({
                "lens": "correctness",
                "severity": "should",
                "claim": "a bad verification",
                "verification": "ran_it_twice"
            })
            .to_string(),
        )
        .assert()
        .code(1);
    let text = String::from_utf8(refused.get_output().stdout.clone()).expect("text");

    for named in ["reproduced", "read_not_run", "unknown"] {
        assert!(text.contains(named), "the refusal names {named}: {text}");
    }
}

/// `finding add --title`, and the row the dashboard draws from it.
///
/// **A finding's title is the one thing the code review page cannot derive.** Board A8 is
/// one line per finding; the record had nothing a line long, so DEV-878 added the field and
/// this is the surface it has to land on — a review session writes the title with the
/// finding, in the same document, and reads it back unchanged.
///
/// It also pins the two states the page branches on. A title given comes back as written; a
/// title left out comes back `null`, which is what tells the page to clamp the claim instead
/// of drawing an empty row.
#[test]
fn a_title_goes_in_with_the_finding_and_comes_back_as_written() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "878", "c0ffee1");

    let titled = add(
        &desk,
        &review,
        json!({
            "lens": "sibling diff",
            "severity": "should",
            "claim": "The retried delivery is sent with no idempotency key, so an \
                      endpoint that accepted the first attempt slowly receives the \
                      same event a second time and acts on it twice",
            "title": "A retried delivery carries no idempotency key",
        }),
    );
    assert_eq!(
        titled["finding"]["title"],
        "A retried delivery carries no idempotency key"
    );

    let untitled = add(
        &desk,
        &review,
        json!({"lens": "naming", "severity": "nit", "claim": "The accessor is still public"}),
    );
    assert!(
        untitled["finding"]["title"].is_null(),
        "a finding nobody titled has no title, and the page clamps its claim instead"
    );

    // The record's own copy, not the write's answer.
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        shown["findings"][0]["title"],
        "A retried delivery carries no idempotency key"
    );
    assert!(shown["findings"][1]["title"].is_null());
}

/// A blank title is refused by name, and the finding is not written.
///
/// `""` is the one value that would be worse than nothing: the page draws the title when
/// there is one, so an empty string draws an empty row where no title draws the claim — and
/// nothing afterwards could tell the two apart.
#[test]
fn a_blank_title_is_refused_and_names_the_field() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "878", "c0ffee1");

    let refused = desk
        .command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({
                "lens": "naming",
                "severity": "nit",
                "claim": "The accessor is still public",
                "title": "  ",
            })
            .to_string(),
        )
        .assert()
        .failure();
    let said = String::from_utf8(refused.get_output().stderr.clone()).expect("stderr is text");
    assert!(
        said.contains("title"),
        "the refusal names the field: {said}"
    );

    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        shown["findings"].as_array().expect("findings").len(),
        0,
        "a refused title leaves no finding behind"
    );
}
