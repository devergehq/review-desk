//! What a finding is about, and that it comes back exactly as it went in.
//!
//! **The point of this file is one sentence: Review Desk does not fetch code.** The hunk
//! a finding carries was written once, by an agent, and the store holds the only copy. A
//! trim, a re-wrap, a tab turned into spaces, a trailing space dropped — any of those and
//! the record now shows source that was never in the pull request. So the awkward input
//! is not an edge case here, it is the subject: tabs, trailing spaces, a line that is
//! nothing but whitespace, an empty line, and text outside ASCII.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Disposition, FindingHunk, HunkLine, HunkLineMarker, NewFinding, Severity, Store, StoreError,
    Verification,
};

/// A review to hang findings off.
fn a_review(store: &Store) -> i64 {
    store
        .create_review(new_review("devergehq/review-desk", 874, "c0ffee1", 1))
        .expect("a review")
        .id
}

/// A hunk line, said shortly.
fn line(marker: HunkLineMarker, text: &str) -> HunkLine {
    HunkLine {
        marker,
        text: text.to_string(),
    }
}

/// The hunk board A2 draws for F1, lines 56 to 63 of `jobs/DeliverWebhook.php`.
fn board_a2_hunk() -> FindingHunk {
    FindingHunk {
        file: "jobs/DeliverWebhook.php".to_string(),
        start_line: 56,
        lines: vec![
            line(
                HunkLineMarker::Unchanged,
                "public function handle(WebhookClient $client): void",
            ),
            line(HunkLineMarker::Unchanged, "{"),
            line(
                HunkLineMarker::Removed,
                "    $client->post($this->endpoint->url, $this->payload);",
            ),
            line(
                HunkLineMarker::Added,
                "    $response = $client->post($this->endpoint->url, $this->payload);",
            ),
            line(HunkLineMarker::Added, ""),
            line(HunkLineMarker::Added, "    if ($response->failed()) {"),
            line(
                HunkLineMarker::Added,
                "        $this->release($this->delayFor($this->attempts()));",
            ),
            line(HunkLineMarker::Added, "    }"),
            line(HunkLineMarker::Unchanged, "}"),
        ],
    }
}

#[test]
fn a_finding_carries_all_six_fields_and_reads_back_unchanged() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(58),
                claim: "A retried delivery is sent without an idempotency key".to_string(),
                hunk: Some(board_a2_hunk()),
                flagged_line: Some(58),
                inline_comment: Some(
                    "This retry re-sends the payload with no idempotency key.".to_string(),
                ),
                why_it_matters: Some(
                    "A customer whose endpoint answers slowly is charged twice.".to_string(),
                ),
                proof: Some(
                    "endpoint accepts attempt 1 after 31s\nqueue retries at 30s".to_string(),
                ),
                proof_note: Some("A throwaway test, not committed.".to_string()),
                verification: Verification::Reproduced,
                suggested_fix: Some("Send the delivery id as the key.".to_string()),
                fix_references: vec![
                    "SendInvoiceEmail.php:44".to_string(),
                    "SyncToLedger.php:71".to_string(),
                ],
                ..Default::default()
            },
        )
        .expect("a finding with everything on it");

    // What `add_finding` returned, and what a fresh read returns, are the same finding.
    // Two assertions rather than one: the first says the write answered truthfully, the
    // second says the database holds it.
    let read = store.get_finding(review, written.seq).expect("read back");
    assert_eq!(read, written);

    assert_eq!(read.hunk.as_ref().unwrap(), &board_a2_hunk());
    assert_eq!(read.flagged_line, Some(58));
    assert_eq!(read.verification, Verification::Reproduced);
    assert_eq!(
        read.fix_references,
        ["SendInvoiceEmail.php:44", "SyncToLedger.php:71"],
        "the references come back in the order they were given"
    );
    assert_eq!(
        read.proof.as_deref(),
        Some("endpoint accepts attempt 1 after 31s\nqueue retries at 30s"),
        "the proof's newline is part of the proof"
    );
    assert_eq!(
        read.proof_note.as_deref(),
        Some("A throwaway test, not committed."),
        "the caption both boards draw under the proof block, in its own field"
    );
}

#[test]
fn the_hunk_numbers_its_lines_the_way_board_a2_draws_them() {
    let hunk = board_a2_hunk();

    let numbers: Vec<Option<i64>> = hunk
        .numbered_lines()
        .into_iter()
        .map(|(number, _)| number)
        .collect();

    assert_eq!(
        numbers,
        [
            Some(56),
            Some(57),
            None,
            Some(58),
            Some(59),
            Some(60),
            Some(61),
            Some(62),
            Some(63)
        ],
        "an added or unchanged line takes the next number; a removed line is not in the \
         file any more and takes none — which is exactly what board A2 draws"
    );
    assert!(hunk.draws_line(58));
    assert!(!hunk.draws_line(64));
}

#[test]
fn a_hunk_may_open_on_a_removed_line_and_still_number_from_its_start() {
    // Board A3's F6: the first row is a removal, and the numbering starts at 58 on the
    // row after it. A `start_line` meaning "the first row's number" could not say this.
    let hunk = FindingHunk {
        file: "jobs/DeliverWebhook.php".to_string(),
        start_line: 58,
        lines: vec![
            line(HunkLineMarker::Removed, "$response = $client->post($url);"),
            line(HunkLineMarker::Added, "$response = $client"),
            line(
                HunkLineMarker::Added,
                "    ->withHeaders(['Idempotency-Key' => Str::uuid()])",
            ),
        ],
    };

    let numbers: Vec<Option<i64>> = hunk
        .numbered_lines()
        .into_iter()
        .map(|(number, _)| number)
        .collect();
    assert_eq!(numbers, [None, Some(58), Some(59)]);
}

#[test]
fn a_hunk_of_forty_lines_round_trips_with_every_marker_intact() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // Forty lines, the three markers cycling, so that a read which lost the order or
    // substituted a marker cannot pass by luck. The text carries the position too.
    let markers = [
        HunkLineMarker::Unchanged,
        HunkLineMarker::Added,
        HunkLineMarker::Removed,
    ];
    let lines: Vec<HunkLine> = (0..40)
        .map(|index| line(markers[index % 3], &format!("line {index}")))
        .collect();

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "forty lines".to_string(),
                hunk: Some(FindingHunk {
                    file: "src/long.rs".to_string(),
                    start_line: 100,
                    lines: lines.clone(),
                }),
                ..Default::default()
            },
        )
        .expect("a forty-line hunk");

    let read = store.get_finding(review, written.seq).expect("read back");
    let hunk = read.hunk.expect("the hunk");
    assert_eq!(hunk.lines.len(), 40);
    assert_eq!(hunk.lines, lines, "in order, with every marker as given");

    // And the numbering over forty lines. Thirteen of them are removals — indices 2, 5,
    // … 38 — so twenty-seven lines are drawn and the last number is 100 + 26.
    let drawn: Vec<i64> = hunk
        .numbered_lines()
        .into_iter()
        .filter_map(|(number, _)| number)
        .collect();
    assert_eq!(drawn.len(), 27);
    assert_eq!(drawn.first(), Some(&100));
    assert_eq!(drawn.last(), Some(&126));
}

#[test]
fn awkward_text_is_stored_exactly_as_it_was_given() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // Each of these has a way of being quietly repaired by something in the middle.
    let awkward = vec![
        line(HunkLineMarker::Unchanged, "\tif (x) {"),
        line(HunkLineMarker::Added, "    let y = 1;   "),
        line(HunkLineMarker::Added, "   "),
        line(HunkLineMarker::Added, "\t\t"),
        line(HunkLineMarker::Added, ""),
        line(
            HunkLineMarker::Added,
            "    // naïve — déjà vu, 日本語, emoji 🤖",
        ),
        line(HunkLineMarker::Removed, "  \t  mixed \t tabs  "),
        line(HunkLineMarker::Unchanged, "}"),
    ];

    let proof = "  leading spaces kept\n\ttab-indented line\n   \nnaïve ✓\ntrailing  ";
    let comment = "\tIndented comment, trailing space \nsecond line  ";
    let why = " It matters, and the space before this sentence is the agent's. ";
    let fix = "Use `Str::uuid()`\u{2014}or the delivery id.\n";

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Nit,
                claim: "awkward text".to_string(),
                hunk: Some(FindingHunk {
                    // Even the file name is left alone: no trim, no case change. A path
                    // is not identity here the way a repository name is.
                    file: " src/odd name.rs ".to_string(),
                    start_line: 1,
                    lines: awkward.clone(),
                }),
                inline_comment: Some(comment.to_string()),
                why_it_matters: Some(why.to_string()),
                proof: Some(proof.to_string()),
                proof_note: Some("  Trailing and leading, kept.  ".to_string()),
                suggested_fix: Some(fix.to_string()),
                fix_references: vec![" SendInvoiceEmail.php:44 ".to_string()],
                ..Default::default()
            },
        )
        .expect("awkward text is accepted");

    let read = store.get_finding(review, written.seq).expect("read back");
    let hunk = read.hunk.expect("the hunk");

    assert_eq!(hunk.file, " src/odd name.rs ");
    assert_eq!(hunk.lines, awkward);
    // Spelt out as well as compared, so that a failure says which character went.
    assert_eq!(hunk.lines[0].text, "\tif (x) {");
    assert_eq!(hunk.lines[1].text, "    let y = 1;   ");
    assert_eq!(hunk.lines[2].text, "   ", "a line of only spaces survives");
    assert_eq!(hunk.lines[3].text, "\t\t", "and a line of only tabs");
    assert_eq!(hunk.lines[4].text, "", "and an empty line");
    assert_eq!(
        hunk.lines[5].text,
        "    // naïve — déjà vu, 日本語, emoji 🤖"
    );
    assert_eq!(hunk.lines[6].text, "  \t  mixed \t tabs  ");

    assert_eq!(read.proof.as_deref(), Some(proof));
    assert_eq!(
        read.proof_note.as_deref(),
        Some("  Trailing and leading, kept.  ")
    );
    assert_eq!(read.inline_comment.as_deref(), Some(comment));
    assert_eq!(read.why_it_matters.as_deref(), Some(why));
    assert_eq!(read.suggested_fix.as_deref(), Some(fix));
    assert_eq!(
        read.fix_references,
        [" SendInvoiceEmail.php:44 "],
        "a reference is checked for being blank and is otherwise left alone"
    );
}

#[test]
fn a_finding_written_without_any_of_it_reads_as_it_did_before() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                file: Some("src/lib.rs".to_string()),
                line: Some(12),
                claim: "the old shape".to_string(),
                evidence: Some("the diff".to_string()),
                ..Default::default()
            },
        )
        .expect("a finding with no evidence of the new kind");

    let read = store.get_finding(review, written.seq).expect("read back");
    assert_eq!(read.file.as_deref(), Some("src/lib.rs"));
    assert_eq!(read.line, Some(12));
    assert_eq!(read.evidence.as_deref(), Some("the diff"));
    assert_eq!(read.disposition, Disposition::InDraft);

    assert_eq!(read.hunk, None);
    assert_eq!(read.flagged_line, None);
    assert_eq!(read.inline_comment, None);
    assert_eq!(read.why_it_matters, None);
    assert_eq!(read.proof, None);
    assert_eq!(read.proof_note, None);
    assert_eq!(
        read.verification,
        Verification::Unknown,
        "saying nothing about the method is `unknown`, not `read_not_run`"
    );
    assert_eq!(read.suggested_fix, None);
    assert!(read.fix_references.is_empty());
}

#[test]
fn every_finding_of_a_review_is_listed_with_its_own_hunk() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // Three findings, the middle one with no hunk: the batched read has to put each hunk
    // against the right finding and leave the one in the middle empty.
    for (claim, hunk) in [
        ("first", Some(board_a2_hunk())),
        ("second", None),
        (
            "third",
            Some(FindingHunk {
                file: "src/third.rs".to_string(),
                start_line: 7,
                lines: vec![line(HunkLineMarker::Added, "third")],
            }),
        ),
    ] {
        store
            .add_finding(
                review,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity: Severity::Should,
                    claim: claim.to_string(),
                    hunk,
                    fix_references: vec![format!("{claim}.rs:1")],
                    ..Default::default()
                },
            )
            .expect("a finding");
    }

    let listed = store.list_findings(review).expect("the findings");
    assert_eq!(listed.len(), 3);
    assert_eq!(
        listed[0].hunk.as_ref().map(|hunk| hunk.file.as_str()),
        Some("jobs/DeliverWebhook.php")
    );
    assert_eq!(listed[0].hunk.as_ref().unwrap().lines.len(), 9);
    assert_eq!(listed[1].hunk, None, "the one in the middle has none");
    assert_eq!(
        listed[2].hunk.as_ref().map(|hunk| hunk.file.as_str()),
        Some("src/third.rs")
    );
    assert_eq!(listed[0].fix_references, ["first.rs:1"]);
    assert_eq!(listed[1].fix_references, ["second.rs:1"]);
    assert_eq!(listed[2].fix_references, ["third.rs:1"]);
}

#[test]
fn a_comment_anchored_to_a_line_the_hunk_does_not_draw_is_refused() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // 64 is one past the end of board A2's hunk.
    let error = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "anchored off the end".to_string(),
                hunk: Some(board_a2_hunk()),
                flagged_line: Some(64),
                ..Default::default()
            },
        )
        .expect_err("a comment the page cannot place is refused");

    match error {
        StoreError::InvalidValue {
            field, expected, ..
        } => {
            assert_eq!(field, "flagged_line");
            assert!(
                expected.contains("56 to 63"),
                "the refusal names the lines it could have been: {expected}"
            );
        }
        other => panic!("expected an invalid value, got {other:?}"),
    }

    assert!(
        store.list_findings(review).unwrap().is_empty(),
        "and nothing was written: the check runs before the transaction"
    );
}

#[test]
fn a_comment_anchored_to_a_removed_line_is_refused_because_it_has_no_number() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // Line 57 is the last *unchanged* line of board A2's hunk and is drawn. The removed
    // line below it carries no number at all, so there is no value of `flagged_line` that
    // names it — which is a real limit of this model and is stated rather than guessed at.
    let refused = store.add_finding(
        review,
        NewFinding {
            lens: "correctness".to_string(),
            severity: Severity::Should,
            claim: "on a removal".to_string(),
            hunk: Some(FindingHunk {
                file: "src/lib.rs".to_string(),
                start_line: 1,
                lines: vec![line(HunkLineMarker::Removed, "gone")],
            }),
            flagged_line: Some(1),
            ..Default::default()
        },
    );

    assert!(
        refused.is_err(),
        "a hunk of nothing but removals draws no line to anchor to"
    );
}

#[test]
fn a_flagged_line_on_its_own_is_kept_because_a_finding_need_not_show_code() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "a comment with no hunk".to_string(),
                flagged_line: Some(4096),
                inline_comment: Some("Posted on line 4096.".to_string()),
                ..Default::default()
            },
        )
        .expect("checked only against a hunk that was given");

    assert_eq!(written.flagged_line, Some(4096));
    assert_eq!(written.hunk, None);
}

#[test]
fn a_hunk_line_holding_more_than_one_file_line_is_refused() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // **The numbering is why this matters, not tidiness.** `numbered_lines` gives exactly
    // one number to each row that is not a removal, so a row holding three file lines
    // draws three lines under one gutter number and every number after it is short by
    // two — and `flagged_line` then anchors the comment to the wrong line of the file.
    // An agent that builds a hunk by joining diff output makes exactly this mistake, and
    // `proof` and `inline_comment` take newlines legitimately right beside it.
    for bad in ["a\nb", "a\r\nb", "trailing\n", "\n"] {
        let error = store
            .add_finding(
                review,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity: Severity::Nit,
                    claim: "a block of text in one line".to_string(),
                    hunk: Some(FindingHunk {
                        file: "src/lib.rs".to_string(),
                        start_line: 1,
                        lines: vec![
                            line(HunkLineMarker::Unchanged, "fine"),
                            line(HunkLineMarker::Added, bad),
                        ],
                    }),
                    ..Default::default()
                },
            )
            .expect_err("a hunk line is one file line");

        match error {
            StoreError::InvalidValue {
                field,
                value,
                expected,
            } => {
                assert_eq!(field, "hunk.lines.text");
                assert!(value.contains("line 2"), "it names which line: {value}");
                assert!(expected.contains("one file line"), "{expected}");
            }
            other => panic!("expected an invalid value, got {other:?}"),
        }
    }

    assert!(
        store.list_findings(review).unwrap().is_empty(),
        "and none of the four attempts wrote a finding"
    );

    // A proof, a comment, a consequence and a fix all take newlines. Only a hunk line
    // does not, and that distinction is the point rather than an inconsistency.
    store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Nit,
                claim: "newlines where they belong".to_string(),
                proof: Some("one\ntwo\nthree".to_string()),
                proof_note: Some("a note\nover two lines".to_string()),
                inline_comment: Some("first\n\nsecond".to_string()),
                why_it_matters: Some("because\nof this".to_string()),
                suggested_fix: Some("do\nthis".to_string()),
                ..Default::default()
            },
        )
        .expect("the text fields take newlines; only a hunk line does not");
}

#[test]
fn a_blank_fix_reference_is_refused_by_name() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    for blank in ["", "   ", "\t"] {
        let error = store
            .add_finding(
                review,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity: Severity::Nit,
                    claim: "a blank reference".to_string(),
                    fix_references: vec!["SendInvoiceEmail.php:44".to_string(), blank.to_string()],
                    ..Default::default()
                },
            )
            .expect_err("a reference the page would draw as a blank line");

        match error {
            StoreError::InvalidValue { field, .. } => assert_eq!(field, "fix_references"),
            other => panic!("expected an invalid value, got {other:?}"),
        }
    }

    assert!(
        store.list_findings(review).unwrap().is_empty(),
        "and none of the three attempts wrote a finding"
    );
}

#[test]
fn a_hunk_with_no_lines_or_no_file_is_refused() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let attempt = |hunk: FindingHunk| {
        store.add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Nit,
                claim: "a hunk that is not one".to_string(),
                hunk: Some(hunk),
                ..Default::default()
            },
        )
    };

    for (hunk, field) in [
        (
            FindingHunk {
                file: "src/lib.rs".to_string(),
                start_line: 1,
                lines: vec![],
            },
            "hunk.lines",
        ),
        (
            FindingHunk {
                file: "   ".to_string(),
                start_line: 1,
                lines: vec![line(HunkLineMarker::Added, "x")],
            },
            "hunk.file",
        ),
        (
            FindingHunk {
                file: "src/lib.rs".to_string(),
                start_line: 0,
                lines: vec![line(HunkLineMarker::Added, "x")],
            },
            "hunk.start_line",
        ),
    ] {
        match attempt(hunk).expect_err("refused") {
            StoreError::InvalidValue { field: named, .. } => assert_eq!(named, field),
            other => panic!("expected an invalid value for {field}, got {other:?}"),
        }
    }
}

#[test]
fn a_start_line_with_no_room_for_the_hunk_after_it_is_refused() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // Two lines to number, starting one short of the end of `i64`. The second line has
    // no number to take. Refused on the way in, rather than served as a line number that
    // wrapped — and `numbered_lines` saturates as well, so a row somebody put in the
    // database file by hand cannot panic the server either.
    let error = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Nit,
                claim: "nowhere to count to".to_string(),
                hunk: Some(FindingHunk {
                    file: "src/lib.rs".to_string(),
                    start_line: i64::MAX,
                    lines: vec![
                        line(HunkLineMarker::Added, "first"),
                        line(HunkLineMarker::Added, "second"),
                    ],
                }),
                ..Default::default()
            },
        )
        .expect_err("a start line with no room after it");

    match error {
        StoreError::InvalidValue { field, .. } => assert_eq!(field, "hunk.start_line"),
        other => panic!("expected an invalid value, got {other:?}"),
    }

    // The saturation, shown directly: every line after the ceiling holds it rather than
    // wrapping to a negative line number.
    let absurd = FindingHunk {
        file: "src/lib.rs".to_string(),
        start_line: i64::MAX,
        lines: vec![
            line(HunkLineMarker::Added, "first"),
            line(HunkLineMarker::Added, "second"),
        ],
    };
    assert_eq!(
        absurd
            .numbered_lines()
            .into_iter()
            .map(|(number, _)| number)
            .collect::<Vec<_>>(),
        [Some(i64::MAX), Some(i64::MAX)]
    );
}

#[test]
fn the_evidence_survives_a_change_of_disposition() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "repetition".to_string(),
                severity: Severity::Nit,
                claim: "held back later".to_string(),
                hunk: Some(board_a2_hunk()),
                proof: Some("read both files".to_string()),
                verification: Verification::ReadNotRun,
                fix_references: vec!["config/webhooks.php:12".to_string()],
                ..Default::default()
            },
        )
        .expect("a finding");

    let moved = store
        .set_finding_disposition(
            review,
            written.seq,
            Disposition::HeldBack,
            Some("goes in the nit ticket".to_string()),
        )
        .expect("held back");

    assert_eq!(moved.disposition, Disposition::HeldBack);
    assert_eq!(
        moved.hunk.as_ref(),
        Some(&board_a2_hunk()),
        "what the finding is about does not change when the developer moves it"
    );
    assert_eq!(moved.verification, Verification::ReadNotRun);
    assert_eq!(moved.fix_references, ["config/webhooks.php:12"]);
}

#[test]
fn the_finding_found_by_its_github_comment_carries_its_hunk_too() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let written = store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "posted".to_string(),
                hunk: Some(board_a2_hunk()),
                fix_references: vec!["SyncToLedger.php:71".to_string()],
                ..Default::default()
            },
        )
        .expect("a finding");
    store
        .set_finding_github_comment(review, written.seq, "rc_1")
        .expect("linked");

    // The harvester's way in. It reads a finding by the comment it was posted as, and a
    // read path that skipped the hunk would hand it a finding with its evidence missing.
    let found = store
        .find_finding_by_github_comment("rc_1")
        .expect("the read")
        .expect("the finding");
    assert_eq!(found.hunk.as_ref(), Some(&board_a2_hunk()));
    assert_eq!(found.fix_references, ["SyncToLedger.php:71"]);
}
