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
    FindingAnchor, FindingHunk, HunkLine, HunkLineMarker, NewFinding, Severity, Store,
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
