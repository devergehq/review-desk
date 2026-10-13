//! What GitHub receives: which findings can be anchored, and the shape they post in.
//!
//! **DEV-929.** The draft stays a list of blocks — one per finding, so the developer can
//! cut, regrade and rewrite — and what is posted is composed from the same record in the
//! house style's shape: a verdict line and `### Problem fit` as the session wrote them, an
//! index of every finding, each anchored finding as an inline thread on the line it
//! concerns, and each finding with nowhere to anchor as a section of the body.
//!
//! This first half is the question everything else rests on: **is the line inside the
//! diff**. It is read off the hunk the finding already carries, and the three branches of
//! the worker's brief are what make that readable — a hunk from `git diff -U3` carries
//! `added` or `removed` markers, a hunk from `git show` is every entry `unchanged`, and a
//! finding with no file or no line carries no hunk at all.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Decision, Disposition, DraftStatus, FindingAnchor, FindingDecision, FindingHunk,
    HunkLine, HunkLineMarker, NewDraft, NewDraftSection, NewFinding, Severity, Store,
};

/// A hunk as `git diff -U3` drew it: three lines of context, one of them replaced.
fn diff_hunk(file: &str, start_line: i64) -> FindingHunk {
    FindingHunk {
        file: file.to_string(),
        start_line,
        lines: vec![
            line(
                HunkLineMarker::Unchanged,
                "    public function handle($payload)",
            ),
            line(HunkLineMarker::Unchanged, "    {"),
            line(
                HunkLineMarker::Removed,
                "        $this->client->post($this->url, $payload);",
            ),
            line(
                HunkLineMarker::Added,
                "        $response = $this->client->post($this->url, $payload);",
            ),
            line(HunkLineMarker::Added, "        return $response;"),
            line(HunkLineMarker::Unchanged, "    }"),
        ],
    }
}

/// A hunk as `git show <head>:<path>` drew it: the code at head, with no change in it.
fn shown_hunk(file: &str, start_line: i64) -> FindingHunk {
    FindingHunk {
        file: file.to_string(),
        start_line,
        lines: vec![
            line(
                HunkLineMarker::Unchanged,
                "    public function url(): string",
            ),
            line(HunkLineMarker::Unchanged, "    {"),
            line(HunkLineMarker::Unchanged, "        return $this->url;"),
            line(HunkLineMarker::Unchanged, "    }"),
        ],
    }
}

fn line(marker: HunkLineMarker, text: &str) -> HunkLine {
    HunkLine {
        marker,
        text: text.to_string(),
    }
}

/// One finding, written and read back, so that the anchor is read off stored rows.
fn written(store: &Store, review_id: i64, new: NewFinding) -> review_desk_store::Finding {
    let seq = store.add_finding(review_id, new).expect("finding").seq;
    store.get_finding(review_id, seq).expect("read back")
}

fn a_review(store: &Store) -> i64 {
    store
        .create_review(new_review("acme/portal", 514, "d41c8e2", 1))
        .expect("review")
        .id
}

#[test]
fn a_finding_whose_hunk_the_diff_drew_anchors_on_the_hunks_file_and_the_flagged_line() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let finding = written(
        &store,
        review,
        NewFinding {
            lens: "error handling".to_string(),
            severity: Severity::Should,
            claim: "the response is returned without checking its status".to_string(),
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(16),
            hunk: Some(diff_hunk("jobs/DeliverWebhook.php", 13)),
            flagged_line: Some(16),
            ..Default::default()
        },
    );

    assert_eq!(
        finding.anchor(),
        Some(FindingAnchor {
            path: "jobs/DeliverWebhook.php".to_string(),
            line: 16,
        })
    );
    assert_eq!(FindingAnchor::SIDE, "RIGHT");
}

#[test]
fn a_finding_whose_hunk_is_every_line_unchanged_has_nowhere_to_anchor() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // The second of the worker's three branches: `git show` at the head commit, because
    // `git diff -U3` draws no hunk for this line. The record says so by carrying no
    // marker other than `unchanged`, and that is the whole of the reading.
    let finding = written(
        &store,
        review,
        NewFinding {
            lens: "naming".to_string(),
            severity: Severity::Nit,
            claim: "the accessor repeats the type in its name".to_string(),
            file: Some("jobs/Webhook.php".to_string()),
            line: Some(42),
            hunk: Some(shown_hunk("jobs/Webhook.php", 40)),
            flagged_line: Some(42),
            ..Default::default()
        },
    );

    assert_eq!(finding.anchor(), None);
    assert!(!finding.hunk.as_ref().expect("hunk").drawn_by_the_diff());
}

#[test]
fn a_finding_with_a_diff_hunk_and_no_flagged_line_has_nowhere_to_anchor() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    let finding = written(
        &store,
        review,
        NewFinding {
            lens: "error handling".to_string(),
            severity: Severity::Should,
            claim: "the retry budget is shared across flags".to_string(),
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(16),
            hunk: Some(diff_hunk("jobs/DeliverWebhook.php", 13)),
            ..Default::default()
        },
    );

    assert_eq!(finding.anchor(), None);
}

#[test]
fn a_finding_with_no_file_has_nowhere_to_anchor() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // A correction to the pull request's description: no code, so no hunk and no line.
    let finding = written(
        &store,
        review,
        NewFinding {
            lens: "description".to_string(),
            severity: Severity::Should,
            claim: "the description does not say what happens to a skipped export".to_string(),
            ..Default::default()
        },
    );

    assert_eq!(finding.anchor(), None);
}

#[test]
fn the_severities_rank_most_serious_first_with_question_above_nit() {
    let mut order = [
        Severity::Nit,
        Severity::Blocker,
        Severity::Question,
        Severity::Should,
    ];
    order.sort_by_key(|severity| severity.rank());

    assert_eq!(
        order,
        [
            Severity::Blocker,
            Severity::Should,
            Severity::Question,
            Severity::Nit
        ]
    );
}

#[test]
fn every_severity_has_the_house_styles_own_glyph_beside_its_word() {
    assert_eq!(
        [
            Severity::Blocker.glyph(),
            Severity::Should.glyph(),
            Severity::Nit.glyph(),
            Severity::Question.glyph(),
        ],
        ["🔴", "🟠", "⚪", "🔵"]
    );
    assert_eq!(Severity::Blocker.label(), "Blocker");
}

// -- the shape GitHub receives ---------------------------------------------------------

/// The draft the issue asks to be measured against: **six findings, four of them anchored**.
///
/// F1 is a blocker on an added line; F2 is a should on an **unchanged context line inside a
/// drawn hunk**, which GitHub can anchor and this record has to agree it can; F3 is a should
/// whose hunk came from `git show`, so the change does not touch it; F4 is a question on an
/// added line; F5 is a nit on an added line, for the lighter thread shape; and F6 is a
/// should with no file at all — a correction to the pull request's description.
///
/// The blocks are the house style's own order: the verdict and `**Method**` line, then
/// `### Problem fit` with a before-and-after diagram in it, then a block per finding in
/// severity order, then the folded provenance carrying `Suppressed`.
fn house_shape(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("acme/portal", 514, "d41c8e2", 1))
        .expect("review");

    let anchored = |file: &str, line: i64| NewFinding {
        file: Some(file.to_string()),
        line: Some(line),
        hunk: Some(diff_hunk(file, line - 3)),
        flagged_line: Some(line),
        ..Default::default()
    };

    let findings = vec![
        NewFinding {
            lens: "correctness".to_string(),
            severity: Severity::Blocker,
            claim: "the column policy is skipped when the definition names no columns".to_string(),
            title: Some(
                "The column policy is skipped entirely when the definition names no columns"
                    .to_string(),
            ),
            ..anchored("app/Reporting/ColumnPolicy.php", 52)
        },
        NewFinding {
            lens: "error handling".to_string(),
            severity: Severity::Should,
            claim: "a schedule whose tenant row has gone is dispatched anyway".to_string(),
            title: Some("A schedule whose tenant row has gone is dispatched anyway".to_string()),
            // The flagged line is the hunk's **first** line, which `diff_hunk` draws as
            // `unchanged`: context, inside a hunk the diff drew. GitHub anchors it.
            ..anchored("app/Jobs/RunScheduledExport.php", 13)
        },
        NewFinding {
            lens: "audit".to_string(),
            severity: Severity::Should,
            claim: "the audit entry may no longer fit".to_string(),
            title: Some("Whether the audit entry still fits is not established".to_string()),
            file: Some("app/Audit/RecordsAnExport.php".to_string()),
            line: Some(22),
            // `git show` at the head commit: the change does not reach this line, so every
            // entry is `unchanged` and there is nowhere for GitHub to hang a comment.
            hunk: Some(shown_hunk("app/Audit/RecordsAnExport.php", 19)),
            flagged_line: Some(22),
            ..Default::default()
        },
        NewFinding {
            lens: "concurrency".to_string(),
            severity: Severity::Question,
            claim: "two schedules for one tenant would write the audit entry twice".to_string(),
            title: Some(
                "Two schedules for one tenant at once would write the audit entry twice"
                    .to_string(),
            ),
            ..anchored("app/Reporting/ExportPipeline.php", 71)
        },
        NewFinding {
            lens: "naming".to_string(),
            severity: Severity::Nit,
            claim: "the removed branch leaves a comment describing it".to_string(),
            title: Some("The removed branch leaves a comment describing it".to_string()),
            ..anchored("app/Reporting/ReportBuilder.php", 28)
        },
        NewFinding {
            lens: "description".to_string(),
            severity: Severity::Should,
            claim: "the description does not say what happens to a skipped export".to_string(),
            title: Some(
                "The description does not say what happens to an export that is skipped"
                    .to_string(),
            ),
            ..Default::default()
        },
    ];

    for finding in findings {
        store.add_finding(review.id, finding).expect("finding");
    }

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(house_blocks()),
            },
        )
        .expect("the draft");

    review.id
}

/// The blocks of that draft, in the order the body reads.
fn house_blocks() -> Vec<NewDraftSection> {
    vec![
        NewDraftSection::prose(
            "**1 Blocker · 3 Should · 1 Question · 1 Nit — 6 open.** The scheduled path \
             builds its own context and the branch is out of `ReportBuilder`; the blocker is \
             the composition of two reasonable-looking decisions.\n\n**Method** · read the \
             scheduled path and the policy at `d41c8e2` · ran the export suite · **1 \
             suppressed** · **1 not verified** · **1 area not reviewed** — detail at the end",
        ),
        NewDraftSection::prose(PROBLEM_FIT),
        NewDraftSection::finding(
            1,
            "The column policy is skipped entirely when the definition names no columns",
            "**in diff** · `Open — needs a decision`\n\n**What** · The array cast answers \
             `[]` for a null column and this early return reads `[]` as no policy.\n**Why it \
             matters** · Eleven definitions in staging export every column, including the \
             two their tenant's policy hides.\n**What I'd do** · Call `ColumnPolicy::for()` \
             here, as the web path does.",
        ),
        NewDraftSection::finding(
            2,
            "A schedule whose tenant row has gone is dispatched anyway",
            "**in diff** · `Open — needs a decision`\n\n**What** · The dispatch query does \
             not filter on the tenant's existence.\n**Why it matters** · The job fails inside \
             `ExportContext::forSchedule` with a null dereference, four frames \
             deep.\n**What I'd do** · Filter the query, so the same failure is readable.",
        ),
        NewDraftSection::finding(
            3,
            "Whether the audit entry still fits is not established",
            "**pre-existing** · `Open — needs a ticket`\n\n**What** · `RecordsAnExport` is \
             outside this change and the scheduled path passes it a system actor.\n**Why it \
             matters** · An audit row attributed to nobody is an audit row nobody can \
             follow.\n**What I'd do** · Say in the description which actor a scheduled \
             export records.",
        ),
        NewDraftSection::finding(
            6,
            "The description does not say what happens to an export that is skipped",
            "**pre-existing** · `Open — needs a decision`\n\n**What** · Three of the \
             change's early returns each decide what an export produces, and none of them \
             appears in the description.\n**Why it matters** · A reader has to read the job \
             to answer the first question they will ask.\n**What I'd do** · Three sentences \
             under \"What is skipped\".",
        ),
        NewDraftSection::finding(
            4,
            "Two schedules for one tenant at once would write the audit entry twice",
            "**in diff** · `Open`\n\n**What** · Can two schedules for one tenant be due in \
             the same minute?\n**Why it matters** · Both would write an audit row keyed on \
             the tenant and the date.\n**What I'd do** · Say whether the unique index on \
             that pair is coming back.",
        ),
        NewDraftSection::finding(
            5,
            "The removed branch leaves a comment describing it",
            "`ReportBuilder.php:28` · `Open`\nThe branch this comment describes was removed \
             four lines above.\n**Fix:** delete the comment with it.",
        ),
        NewDraftSection::prose(
            "<details><summary>How this review was made — scope, suppressed findings, what I \
             could not verify</summary>\n\nRead the scheduled path, the policy and the audit \
             writer at `d41c8e2`. **Suppressed (1)**: a redaction helper's naming, because \
             the file is being replaced next week. **Not verified (1)**: the export suite's \
             slowest case, which needs a tenant fixture this environment has \
             not got.\n\n</details>",
        ),
    ]
}

/// `### Problem fit` with the before-and-after diagram beneath it, as a prose block.
///
/// **The fence is the test.** A prose block is carried through character for character, and
/// a mermaid fence inside one is how a review draws the workflow a finding is about — so a
/// composition that re-wrapped, re-indented or escaped anything here would stop the diagram
/// rendering on GitHub.
const PROBLEM_FIT: &str = r#"### Problem fit

A scheduled export used to build its context from the request that never made it, which is
why the branch existed. Giving the scheduled path a context of its own is the right shape;
what is new is that the policy now runs in two places.

```mermaid
flowchart LR
  A[Schedule due] --> B[Build context]
  B --> C{Policy?}
  C -->|named columns| D[Narrow the select]
  C -->|none named| E[Every column]
```"#;

#[test]
fn an_unchanged_context_line_inside_a_drawn_hunk_is_anchorable_on_the_right() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    // **The case GitHub is strict about.** A hunk the diff drew carries context lines as
    // well as changed ones, and GitHub anchors a comment on any line the diff shows — so a
    // finding on a context line inside a drawn hunk posts as a thread. The side is the new
    // file's, because the line is still in the file.
    let finding = store.get_finding(review, 2).expect("F2");
    let anchor = finding
        .anchor()
        .expect("a context line in a drawn hunk anchors");

    assert_eq!(anchor.path, "app/Jobs/RunScheduledExport.php");
    assert_eq!(anchor.line, 13, "the hunk's first line, drawn as unchanged");
    assert_eq!(
        finding.hunk.as_ref().expect("hunk").lines[0].marker,
        HunkLineMarker::Unchanged
    );
    assert_eq!(FindingAnchor::SIDE, "RIGHT");
}

#[test]
fn a_removed_line_cannot_be_flagged_so_no_thread_is_ever_on_the_left() {
    let (_dir, store) = temp_store();
    let review = a_review(&store);

    // **Why every thread is `RIGHT`, and why it is not a choice made here.** The third line
    // of `diff_hunk` is the removal, and a removed line carries no number — so the number
    // it would have had belongs to the added line that replaced it. The store refuses a
    // flagged line the hunk does not draw, which is every line the change deleted, and the
    // worker's brief says the same thing from the other end: flag the line that replaced it
    // and say in the comment that the code at fault is gone.
    let hunk = diff_hunk("jobs/DeliverWebhook.php", 13);
    let numbers: Vec<Option<i64>> = hunk
        .numbered_lines()
        .iter()
        .map(|(number, _)| *number)
        .collect();
    assert_eq!(
        numbers,
        vec![Some(13), Some(14), None, Some(15), Some(16), Some(17)],
        "the removal takes no number, so the added line after it is 15"
    );

    let refused = store.add_finding(
        review,
        NewFinding {
            lens: "error handling".to_string(),
            severity: Severity::Should,
            claim: "the call that was deleted was the only one checking the status".to_string(),
            file: Some("jobs/DeliverWebhook.php".to_string()),
            line: Some(15),
            hunk: Some(hunk),
            // There is no such line: 13, 14, 15, 16 and 17 are drawn and the removal is
            // not one of them. Asking for a line beyond the hunk is how a caller would try
            // to point at the old file's numbering.
            flagged_line: Some(99),
            ..Default::default()
        },
    );

    let message = refused
        .expect_err("a line the hunk does not draw")
        .to_string();
    assert!(
        message.contains("flagged_line") && message.contains("13 to 17"),
        "the refusal names the range it will accept: {message}"
    );
}

#[test]
fn the_index_carries_one_row_per_posted_finding_in_severity_order() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    let body = store.get_draft(review).expect("draft").body;

    let rows: Vec<&str> = index_rows(&body);
    assert_eq!(rows.len(), 6, "one row per posted finding: {rows:#?}");

    let order: Vec<&str> = rows
        .iter()
        .map(|row| row.split(" | ").next().expect("the severity cell"))
        .collect();
    assert_eq!(
        order,
        vec![
            "| 🔴 Blocker · F1",
            "| 🟠 Should · F2",
            "| 🟠 Should · F3",
            "| 🟠 Should · F6",
            "| 🔵 Question · F4",
            "| ⚪ Nit · F5",
        ],
        "most serious first, then by number, with a question above a nit"
    );

    // Four columns, which is `review_lint.py`'s budget for a body table. Counted on the
    // header, because the fifth column is what leaves the page.
    let header = body
        .lines()
        .find(|line| line.starts_with("| | Where"))
        .expect("the index's header");
    assert_eq!(header.matches('|').count(), 5, "four columns: {header}");
}

#[test]
fn an_anchored_finding_is_a_row_and_a_thread_and_not_a_section() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    let draft = store.get_draft(review).expect("draft");
    let threads = store.draft_threads(review).expect("threads");

    assert!(
        !draft.body.contains("### Blocker · F1 ·"),
        "F1 is anchored, so the body carries no section for it:\n{}",
        draft.body
    );
    assert!(
        draft.body.contains(
            "| 🔴 Blocker · F1 | [`ColumnPolicy.php:52`]\
             (https://github.com/acme/portal/blob/d41c8e2/app/Reporting/ColumnPolicy.php#L52)"
        ),
        "and its row links to the line. No session measured this one, so the commit is \
         the review's head — see the DEV-978 tests below for the case where it is not:\n{}",
        draft.body
    );

    let seqs: Vec<i64> = threads.iter().map(|thread| thread.finding_seq).collect();
    assert_eq!(
        seqs,
        vec![1, 2, 4, 5],
        "the four anchored findings, most serious first"
    );
    assert!(
        threads
            .iter()
            .all(|thread| thread.side == "RIGHT" && thread.line > 0),
        "every thread is on the new file's side: {threads:#?}"
    );
}

#[test]
fn a_finding_with_nowhere_to_anchor_keeps_its_section_and_a_row_that_points_at_it() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    let body = store.get_draft(review).expect("draft").body;
    let threads = store.draft_threads(review).expect("threads");

    // F3's hunk came from `git show`; F6 has no file at all.
    assert!(
        body.contains("### Should · F3 · Whether the audit entry still fits is not established")
    );
    assert!(body.contains(
        "### Should · F6 · The description does not say what happens to an export that is skipped"
    ));
    assert!(
        !threads
            .iter()
            .any(|thread| thread.finding_seq == 3 || thread.finding_seq == 6),
        "neither of them can be a thread: {threads:#?}"
    );

    // The row for a finding with a file links to the line; the row for a finding with none
    // links to the pull request, because that is where such a finding is about.
    assert!(body.contains(
        "| 🟠 Should · F3 | [`RecordsAnExport.php:22`]\
         (https://github.com/acme/portal/blob/d41c8e2/app/Audit/RecordsAnExport.php#L22)"
    ));
    assert!(body.contains(
        "| 🟠 Should · F6 | [the pull request](https://github.com/acme/portal/pull/514)"
    ));

    // Every row carries a link, which is what `review_lint.py`'s `index.rows_linked` asks
    // for, and none of them is an anchor into the body — those never resolve there.
    assert!(index_rows(&body).iter().all(|row| row.contains("](http")));
    assert!(!body.contains("](#"));

    // The status cell says which of the two a reader is looking at.
    assert_eq!(
        index_rows(&body)
            .iter()
            .filter(|row| row.ends_with("| Open · in the body |"))
            .count(),
        2
    );
    assert_eq!(
        index_rows(&body)
            .iter()
            .filter(|row| row.ends_with("| Open · on the code |"))
            .count(),
        4
    );
}

#[test]
fn a_prose_blocks_mermaid_fence_arrives_character_for_character() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    let body = store.get_draft(review).expect("draft").body;

    assert!(
        body.contains(PROBLEM_FIT),
        "the Problem fit block, fence and all, is in the body as it was written:\n{body}"
    );
    assert!(body.contains("```mermaid\nflowchart LR\n  A[Schedule due] --> B[Build context]"));
}

#[test]
fn each_thread_is_in_the_alert_its_severity_picks_and_a_nit_has_no_rail() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    let threads = store.draft_threads(review).expect("threads");

    let of = |seq: i64| {
        threads
            .iter()
            .find(|thread| thread.finding_seq == seq)
            .expect("thread")
    };

    assert!(of(1).body.starts_with(
        "> [!CAUTION]\n> **Blocker · F1** — The column policy is skipped entirely when the \
         definition names no columns · `Open`\n>\n> **in diff** · `Open — needs a decision`"
    ));
    assert!(of(2).body.starts_with("> [!WARNING]\n> **Should · F2** —"));
    assert!(of(4).body.starts_with("> [!NOTE]\n> **Question · F4** —"));

    // The nit takes the lighter shape: a glyph, a title line, and the block's text under
    // it. Four labels on a one-line nit is why nits read as heavy as blockers.
    let nit = of(5);
    assert!(!nit.body.contains("[!"), "no rail on a nit: {}", nit.body);
    assert_eq!(
        nit.body,
        "⚪ **Nit · F5** — The removed branch leaves a comment describing it · `Open`\n\
         `ReportBuilder.php:28` · `Open`\nThe branch this comment describes was removed four \
         lines above.\n**Fix:** delete the comment with it.\n\n<!-- review-desk:F5 -->"
    );

    // **Every comment ends with the line the session matches it back by**, outside the rail
    // on the ones that have a rail — it is how the comment is joined, not what is claimed.
    for thread in &threads {
        assert!(
            thread
                .body
                .ends_with(&format!("\n\n<!-- review-desk:F{} -->", thread.finding_seq)),
            "F{} does not end with its marker: {}",
            thread.finding_seq,
            thread.body
        );
    }

    // The block's own text is carried through, blockquoted and otherwise untouched.
    let block = store
        .draft_sections(review)
        .expect("blocks")
        .into_iter()
        .find(|block| block.finding_seq == Some(1))
        .expect("F1's block");
    for line in block.text.lines().filter(|line| !line.is_empty()) {
        assert!(
            of(1).body.contains(&format!("> {line}")),
            "every line of the block is in the thread: {line}"
        );
    }
}

/// The index's rows, without its heading, its header or its rule.
fn index_rows(body: &str) -> Vec<&str> {
    body.lines()
        .filter(|line| {
            line.starts_with("| ") && !line.starts_with("| -- |") && !line.contains("| Where |")
        })
        .collect()
}

#[test]
fn a_cut_finding_has_no_row_and_no_thread() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Cut),
            Actor::Developer,
        )
        .expect("the developer cuts F1");

    let body = store.get_draft(review).expect("draft").body;
    let threads = store.draft_threads(review).expect("threads");

    assert_eq!(index_rows(&body).len(), 5, "five rows left:\n{body}");
    assert!(!body.contains("🔴 Blocker · F1"), "no row for it:\n{body}");
    assert!(
        !threads.iter().any(|thread| thread.finding_seq == 1),
        "and no thread: {threads:#?}"
    );

    // The block stays on the record, which is the difference between a decision and a
    // deletion — and it is what a later "post it" would put back.
    assert!(store
        .draft_sections(review)
        .expect("blocks")
        .iter()
        .any(|block| block.finding_seq == Some(1) && !block.posted));
}

#[test]
fn a_regrade_changes_the_row_the_section_and_the_threads_alert() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    // F2 is anchored, so the regrade has to reach its row and its thread; F3 is in the
    // body, so it has to reach its row and its heading.
    store
        .decide_finding(
            review,
            2,
            &Decision {
                severity: Some(Severity::Blocker),
                ..Decision::of(FindingDecision::Regrade)
            },
            Actor::Developer,
        )
        .expect("the developer regrades F2 up");
    store
        .decide_finding(
            review,
            3,
            &Decision {
                severity: Some(Severity::Nit),
                ..Decision::of(FindingDecision::Regrade)
            },
            Actor::Developer,
        )
        .expect("and F3 down");

    let body = store.get_draft(review).expect("draft").body;
    let threads = store.draft_threads(review).expect("threads");

    assert!(
        body.contains("| 🔴 Blocker · F2 |"),
        "the live severity:\n{body}"
    );
    assert!(body.contains("| ⚪ Nit · F3 |"), "both ways:\n{body}");
    assert!(
        body.contains("### Nit · F3 · Whether the audit entry still fits is not established"),
        "and the section's heading with it:\n{body}"
    );

    let regraded = threads
        .iter()
        .find(|thread| thread.finding_seq == 2)
        .expect("F2's thread");
    assert!(
        regraded
            .body
            .starts_with("> [!CAUTION]\n> **Blocker · F2** —"),
        "a regrade changes the alert, because the alert is the severity: {}",
        regraded.body
    );

    // And the rows are in the new order: F2 is a blocker now, F3 a nit.
    let order: Vec<&str> = index_rows(&body)
        .iter()
        .map(|row| row.split(" | ").next().expect("the severity cell"))
        .collect();
    assert_eq!(
        order,
        vec![
            "| 🔴 Blocker · F1",
            "| 🔴 Blocker · F2",
            "| 🟠 Should · F6",
            "| 🔵 Question · F4",
            "| ⚪ Nit · F3",
            "| ⚪ Nit · F5",
        ]
    );
}

#[test]
fn a_draft_of_prose_alone_composes_exactly_as_it_did_before() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 601, "0b1c2d3", 1))
        .expect("review");

    // **The index appears only where there are findings to index.** A draft with no finding
    // block is the one DEV-929 does not touch: no heading, no table, and the body is the
    // blocks joined as they always were.
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection::prose("**Nothing blocks the merge.** One thing to say."),
                    NewDraftSection::prose("**Not checked:** the admin screens."),
                ]),
            },
        )
        .expect("a draft of prose");

    assert_eq!(
        store.get_draft(review.id).expect("draft").body,
        "**Nothing blocks the merge.** One thing to say.\n\n**Not checked:** the admin \
         screens.\n"
    );
    assert!(store.draft_threads(review.id).expect("threads").is_empty());
}

#[test]
fn a_title_too_long_for_a_cell_is_clamped_at_a_word_and_the_whole_one_stays_in_the_section() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 602, "0b1c2d3", 1))
        .expect("review");

    // 171 characters: longer than the 140 of shown text `review_lint.py` fails a cell at,
    // and the kind of title a finding nobody titled falls back to its claim for.
    let long = "The scheduled export path reads the column policy twice and the second read \
                answers differently, because the first one runs before the context is built \
                and the second after it";
    assert!(long.chars().count() > 140);

    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: long.to_string(),
                ..Default::default()
            },
        )
        .expect("a finding nobody titled");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection {
                    kind: review_desk_store::DraftSectionKind::Finding,
                    finding_seq: Some(1),
                    title: None,
                    text: "**in diff** · `Open`\n\n**What** · it reads twice.".to_string(),
                }]),
            },
        )
        .expect("the draft");

    let body = store.get_draft(review.id).expect("draft").body;
    let row = index_rows(&body)
        .into_iter()
        .next()
        .expect("the one row")
        .to_string();
    let cell = row.split(" | ").nth(2).expect("the finding cell");

    assert!(cell.ends_with(" …"), "clamped at a word: {cell}");
    assert!(
        cell.chars().count() <= 140,
        "{} characters",
        cell.chars().count()
    );
    assert!(long.starts_with(cell.trim_end_matches(" …")));
    assert!(
        body.contains(&format!("### Should · F1 · {long}")),
        "and the whole title is still the section's heading:\n{body}"
    );
}

#[test]
fn moving_a_finding_out_of_the_draft_takes_its_row_and_its_thread_with_it() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    // **The other way a finding stops posting.** `decide_finding` is the developer's cut;
    // this is the code review page's three lists — the draft, held back, suppressed — and
    // it used to write the column and leave `drafts.body` as it was. Since the threads are
    // composed from the record at read time, a stale body would now disagree with them.
    store
        .set_finding_disposition(
            review,
            1,
            Disposition::HeldBack,
            Some("for the nit ticket".to_string()),
        )
        .expect("the blocker is held back");

    let body = store.get_draft(review).expect("draft").body;
    let threads = store.draft_threads(review).expect("threads");

    assert_eq!(index_rows(&body).len(), 5, "five rows left:\n{body}");
    assert!(
        !body.contains("F1"),
        "and F1 is nowhere in the body:\n{body}"
    );
    assert!(!threads.iter().any(|thread| thread.finding_seq == 1));
}

#[test]
fn moving_a_posted_finding_once_the_draft_is_signed_off_is_refused() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("the developer signs off");

    let refused = store.set_finding_disposition(review, 1, Disposition::HeldBack, None);
    let message = refused
        .expect_err("an approved draft does not move")
        .to_string();
    assert!(
        message.contains("what the review posts"),
        "the refusal says why: {message}"
    );

    // And the body is untouched, which is the point: it is what the session posts.
    assert!(store
        .get_draft(review)
        .expect("draft")
        .body
        .contains("| 🔴 Blocker · F1 |"));
}

#[test]
fn moving_a_finding_that_posts_nowhere_is_allowed_after_the_sign_off() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    // A seventh finding the draft never raised: held back for the ticket, with no block.
    store
        .add_finding(
            review,
            NewFinding {
                lens: "naming".to_string(),
                severity: Severity::Nit,
                claim: "the accessor repeats the type in its name".to_string(),
                disposition: Disposition::HeldBack,
                disposition_reason: Some("a nit; it goes in the ticket".to_string()),
                ..Default::default()
            },
        )
        .expect("a held-back nit");
    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("the developer signs off");

    let before = store.get_draft(review).expect("draft").body;

    // **Held back to suppressed changes no character of the body**, so it is not refused:
    // the developer is entitled to tidy the list they are not posting, and what the refusal
    // protects is the text the session is about to post.
    store
        .set_finding_disposition(
            review,
            7,
            Disposition::Suppressed,
            Some("the file is replaced next week".to_string()),
        )
        .expect("a move that changes nothing about the body");

    assert_eq!(
        store.get_draft(review).expect("draft").body,
        before,
        "and the approved body is untouched, down to the character"
    );
    assert_eq!(
        store.get_finding(review, 7).expect("F7").disposition,
        Disposition::Suppressed
    );

    // Back into the draft is the move that *would* change it, and that one is refused even
    // though the finding is not in the body now.
    let refused = store.set_finding_disposition(review, 7, Disposition::InDraft, None);
    assert!(refused
        .expect_err("raising a finding on an approved draft")
        .to_string()
        .contains("what the review posts"));
}

#[test]
fn a_title_too_long_to_clamp_at_a_word_keeps_the_start_of_it() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 603, "0b1c2d3", 1))
        .expect("review");

    // One word of 180 characters: a finding nobody titled, whose claim is a pasted
    // identifier. Clamping at a word boundary keeps nothing, and a row reading `…` says
    // less about the finding than a sentence cut mid-word does.
    let long = "a".repeat(180);
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "naming".to_string(),
                severity: Severity::Nit,
                claim: long.clone(),
                ..Default::default()
            },
        )
        .expect("a finding nobody titled");
    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection {
                    kind: review_desk_store::DraftSectionKind::Finding,
                    finding_seq: Some(1),
                    title: None,
                    text: "`ReportBuilder.php:28` · `Open`\nIt is a long name.".to_string(),
                }]),
            },
        )
        .expect("the draft");

    let body = store.get_draft(review.id).expect("draft").body;
    let row = index_rows(&body)
        .into_iter()
        .next()
        .expect("the one row")
        .to_string();
    let cell = row.split(" | ").nth(2).expect("the finding cell");

    assert!(cell.ends_with(" …"), "clamped: {cell}");
    assert!(
        cell.chars().count() <= 140,
        "{} characters",
        cell.chars().count()
    );
    assert!(
        cell.starts_with("aaaa"),
        "and it is the start of the claim rather than nothing: {cell}"
    );
}

#[test]
fn a_move_that_changes_nothing_leaves_the_lint_that_measured_the_body() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);

    store
        .add_finding(
            review,
            NewFinding {
                lens: "naming".to_string(),
                severity: Severity::Nit,
                claim: "the accessor repeats the type in its name".to_string(),
                disposition: Disposition::HeldBack,
                disposition_reason: Some("a nit; it goes in the ticket".to_string()),
                ..Default::default()
            },
        )
        .expect("a held-back nit");

    // The same blocks again, with what the review-craft lint said about the body they
    // compose. A draft at `ready` is the state the developer reads it in.
    store
        .put_draft(
            review,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: Some(review_desk_store::DraftLint {
                    passes: Some(true),
                    visible_words: Some(125),
                    word_budget: Some(150),
                    findings_in_body: Some(6),
                    findings_expected: Some(6),
                    raised_before_on_pr: Some(0),
                    errors: None,
                    checked_at: None,
                }),
                sections: Some(house_blocks()),
            },
        )
        .expect("the draft, linted");

    let before = store.get_draft(review).expect("draft");
    assert_eq!(before.lint.passes, Some(true));

    // **The lint goes with the body it measured, so a move that composes the same body
    // leaves it alone.** Held back to suppressed is such a move: neither disposition posts,
    // so no block's words change — and clearing the result would have the record say nobody
    // had checked words that had just been checked.
    store
        .set_finding_disposition(
            review,
            7,
            Disposition::Suppressed,
            Some("the file is replaced next week".to_string()),
        )
        .expect("a move that changes nothing about the body");

    let after = store.get_draft(review).expect("draft");
    assert_eq!(after.body, before.body);
    assert_eq!(
        after.lint.passes,
        Some(true),
        "the lint still describes this body"
    );

    // And a move that does change the body clears it, which is the rule it is an exception
    // to rather than a hole in.
    store
        .set_finding_disposition(
            review,
            1,
            Disposition::HeldBack,
            Some("for the ticket".to_string()),
        )
        .expect("the blocker is held back");
    assert_eq!(
        store.get_draft(review).expect("draft").lint.passes,
        None,
        "a body that changed is a body nobody has checked"
    );
}

#[test]
fn two_findings_on_one_line_are_told_apart_by_the_marker_and_not_by_the_line() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 604, "0b1c2d3", 1))
        .expect("review");

    // **A Should and a Nit on the same line is ordinary.** GitHub answers the review call
    // with the review rather than with the comments it made, so a session reads them back
    // and has to match each one to its finding — and `(path, line)` does not tell these two
    // apart. Measured by the DEV-929 worker half, which is where this test comes from.
    for severity in [Severity::Should, Severity::Nit] {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "error handling".to_string(),
                    severity,
                    claim: format!("what the {severity} says about line 16"),
                    title: Some(format!("The {severity} on line 16")),
                    file: Some("jobs/DeliverWebhook.php".to_string()),
                    line: Some(16),
                    hunk: Some(diff_hunk("jobs/DeliverWebhook.php", 13)),
                    flagged_line: Some(16),
                    ..Default::default()
                },
            )
            .expect("a finding on line 16");
    }

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection::finding(1, "The should on line 16", "**in diff** · `Open`"),
                    NewDraftSection::finding(2, "The nit on line 16", "A name, nothing more."),
                ]),
            },
        )
        .expect("the draft");

    let threads = store.draft_threads(review.id).expect("threads");
    assert_eq!(threads.len(), 2);
    assert_eq!(
        (threads[0].path.as_str(), threads[0].line),
        (threads[1].path.as_str(), threads[1].line),
        "both sit on the same line, which is the case this is about"
    );

    assert_eq!(threads[0].finding_key, "F1");
    assert_eq!(threads[1].finding_key, "F2");
    assert!(threads[0].body.ends_with("<!-- review-desk:F1 -->"));
    assert!(threads[1].body.ends_with("<!-- review-desk:F2 -->"));

    // And the marker is what a session reading the comments back joins on: the two bodies
    // differ by it even where everything else about where they sit is the same.
    assert_ne!(threads[0].body, threads[1].body);
}

#[test]
fn a_body_the_blocks_no_longer_compose_to_is_not_handed_over_and_has_a_way_out() {
    let (dir, store) = temp_store();
    let review = house_shape(&store);
    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("the developer signs off");

    // **The state an upgrade leaves behind, written the way the database holds it.** A
    // draft approved before DEV-929 carries the body the old composition made — every
    // finding a section, no index — beside blocks that would compose differently now.
    // Nothing in this build can produce it, which is the point: every write composes the
    // body from the blocks in the same transaction. So it is written in the database's own
    // SQL, as `tests/rules.rs` ages a citation, rather than by adding a store function that
    // exists so that a test can reach a state the real world reaches by upgrading.
    let old_shape = "The approach fits the ticket.\n\n### Blocker · F1 · The column policy \
                     is skipped\n\n**in diff** · `Open`\n";
    let conn = rusqlite::Connection::open(common::db_path(&dir)).expect("the database");
    conn.execute(
        "UPDATE drafts SET body = ?2 WHERE review_id = ?1",
        rusqlite::params![review, old_shape],
    )
    .expect("the body an older build composed");

    assert!(store.draft_body_is_stale(review).expect("the standing"));
    let message = store
        .draft_handover(review)
        .expect_err("a body the blocks do not compose to")
        .to_string();
    assert!(
        message.contains("approved in an older shape") && message.contains("send it back"),
        "the refusal names the state and the way out: {message}"
    );

    // **And the way out it names works.** This is the whole of what the worker half
    // measured: before a send-back could take an approved draft, such a draft could not be
    // handed over, could not be rewritten and could not be sent back, and the only
    // transition left was posting the body the refusal exists to stop.
    store
        .send_back_draft(review, "the index is in the old shape; write it again")
        .expect("the developer takes the sign-off back");

    let draft = store.get_draft(review).expect("draft");
    assert_eq!(draft.status, DraftStatus::SentBack);
    assert_eq!(draft.event, None, "the sign-off went with it");
    assert_eq!(draft.approved_at, None);

    // The session sees it the way it sees every other send-back.
    let waiting = store
        .work_list(&review_desk_store::WorkFilter::default())
        .expect("the work list");
    let item = waiting
        .iter()
        .find(|item| item.review.id == review)
        .expect("the review is waiting on the session");
    assert_eq!(item.kind, review_desk_store::WorkKind::DraftSentBack);
    assert_eq!(
        item.note.as_deref(),
        Some("the index is in the old shape; write it again")
    );

    // It writes the draft again, the developer signs it off again, and the hand-over
    // serves it.
    store
        .put_draft(
            review,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(house_blocks()),
            },
        )
        .expect("the session writes it again");
    assert!(!store.draft_body_is_stale(review).expect("the standing"));

    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("and the developer signs it off again");

    let handover = store.draft_handover(review).expect("handed over");
    assert_eq!(handover.threads.len(), 4);
    assert!(handover.draft.body.contains("### Open — needs a decision"));
}

#[test]
fn a_sign_off_taken_back_while_a_session_is_mid_post_stops_the_post() {
    let (_dir, store) = temp_store();
    let review = house_shape(&store);
    store
        .approve_draft(
            review,
            review_desk_store::ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("the developer signs off");

    // **The race the send-back opens, in the order that matters.** A session reads the
    // approved draft, starts posting, and the developer takes the sign-off back while it is
    // in flight. What must not happen is the old body being recorded as posted — and
    // `post_draft` takes an approved draft and nothing else, which is what closes it.
    store
        .send_back_draft(review, "hold on — the blocker is wrong")
        .expect("the developer takes it back");

    let refused = store
        .post_draft(review, "gh-review-3312")
        .expect_err("a sent-back draft is not posted");
    assert!(
        refused.to_string().contains("approved"),
        "the refusal says what it wanted: {refused}"
    );

    // And the review holds no GitHub review id, which is the fact a second session would
    // otherwise read as "already posted".
    assert_eq!(
        store.get_review(review).expect("review").github_review_id,
        None
    );
}

// -- which commit a row links at (DEV-978) ---------------------------------------------

/// A review of round two at `e4f5a6b`, holding one finding whose line was measured on
/// round one's `a1b2c3d` and one measured on this round's own head.
fn two_commits_in_one_round(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("globex/portal", 731, "e4f5a6b", 2))
        .expect("review");

    let anchored = |file: &str, line: i64, measured: Option<&str>| NewFinding {
        file: Some(file.to_string()),
        line: Some(line),
        line_sha: measured.map(str::to_string),
        hunk: Some(diff_hunk(file, line - 3)),
        flagged_line: Some(line),
        ..Default::default()
    };

    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "the export is unbounded".to_string(),
                title: Some("The export is unbounded".to_string()),
                ..anchored("app/Export.php", 40, Some("a1b2c3d"))
            },
        )
        .expect("the carried finding's line, measured in round one");

    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "error handling".to_string(),
                severity: Severity::Should,
                claim: "the retry has no ceiling".to_string(),
                title: Some("The retry has no ceiling".to_string()),
                ..anchored("app/Retry.php", 58, Some("e4f5a6b"))
            },
        )
        .expect("a line read on this round's head");

    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "naming".to_string(),
                severity: Severity::Nit,
                claim: "the helper is named for its direction".to_string(),
                title: Some("The helper is named for its direction".to_string()),
                ..anchored("app/Shape.php", 18, None)
            },
        )
        .expect("a finding written before the column existed");

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection::finding(1, "The export is unbounded", "**What** · it is."),
                    NewDraftSection::finding(2, "The retry has no ceiling", "**What** · it is."),
                    NewDraftSection::finding(
                        3,
                        "The helper is named for its direction",
                        "**What** · it is.",
                    ),
                ]),
            },
        )
        .expect("the draft");

    review.id
}

#[test]
fn a_row_links_at_the_commit_its_line_was_measured_on_and_not_at_the_head() {
    let (_dir, store) = temp_store();
    let review = two_commits_in_one_round(&store);
    let body = store.get_draft(review).expect("draft").body;

    assert!(
        body.contains(
            "[`Export.php:40`]\
             (https://github.com/globex/portal/blob/a1b2c3d/app/Export.php#L40)"
        ),
        "line 40 was counted on a1b2c3d, so that is where the row sends a reader. \
         The review's head is e4f5a6b, and line 40 there is something else:\n{body}"
    );
    assert!(
        !body.contains("blob/e4f5a6b/app/Export.php"),
        "and the head appears nowhere near this finding:\n{body}"
    );
}

#[test]
fn a_row_whose_line_was_measured_on_this_head_links_at_this_head() {
    let (_dir, store) = temp_store();
    let review = two_commits_in_one_round(&store);
    let body = store.get_draft(review).expect("draft").body;

    assert!(
        body.contains(
            "[`Retry.php:58`]\
             (https://github.com/globex/portal/blob/e4f5a6b/app/Retry.php#L58)"
        ),
        "a line read on this round's head links to this round's head, which is the \
         ordinary case and the one that must not change:\n{body}"
    );
}

#[test]
fn a_row_with_no_measured_commit_links_exactly_as_it_did_before() {
    let (_dir, store) = temp_store();
    let review = two_commits_in_one_round(&store);
    let body = store.get_draft(review).expect("draft").body;

    assert!(
        body.contains(
            "[`Shape.php:18`]\
             (https://github.com/globex/portal/blob/e4f5a6b/app/Shape.php#L18)"
        ),
        "a finding written before the column existed has no commit of its own, and nothing \
         invents one for it: the link is the review's head, character for character what \
         it was. What says the commit was not recorded is the page, not this:\n{body}"
    );
}

#[test]
fn a_thread_hands_over_the_commit_its_line_was_measured_on() {
    let (_dir, store) = temp_store();
    let review = two_commits_in_one_round(&store);
    let threads = store.draft_threads(review).expect("threads");

    let measured: Vec<(i64, Option<&str>)> = threads
        .iter()
        .map(|thread| (thread.finding_seq, thread.line_sha.as_deref()))
        .collect();

    assert_eq!(
        measured,
        vec![(1, Some("a1b2c3d")), (2, Some("e4f5a6b")), (3, None)],
        "GitHub anchors a comment against the pull request's head, so the session \
         posting these is the one thing in the loop that can tell F1's line was counted \
         somewhere else. Review Desk states it and posts nothing"
    );
}
