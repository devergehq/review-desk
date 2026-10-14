//! Which commit a finding's line was measured on, run through the binary. DEV-978.
//!
//! **Through the binary rather than the store, because the defect was never in one
//! layer.** A line is written by `finding add`, copied by `finding carry`, and turned into
//! an address by the composition the draft hands over — and the bug was that the last of
//! those three paired its commit with the first one's number. A test that called the store
//! directly would be checking the column; these check the link.
//!
//! Every test gets its own database through [`Desk`], so none can read or write
//! `~/.review-desk/review-desk.db`.

mod common;

use common::{open_review, post_round, Desk};
use serde_json::Value;

/// A finding with a line and the commit that line was counted on, and its number.
fn add_measured(desk: &Desk, review: &str, claim: &str, line: i64, line_sha: &str) -> String {
    add(
        desk,
        review,
        &format!(
            r#"{{"lens": "sibling diff", "severity": "should", "claim": "{claim}",
                 "file": "app/Export.php", "line": {line}, "line_sha": "{line_sha}"}}"#
        ),
    )
}

fn add(desk: &Desk, review: &str, document: &str) -> String {
    let output = desk
        .command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(document.to_string())
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

/// A review's findings, by number, as `review show --json` serves them.
fn findings(desk: &Desk, review: &str) -> Vec<Value> {
    let document = desk.json(&["review", "show", "--review", review]);
    document["findings"]
        .as_array()
        .expect("the findings")
        .clone()
}

fn finding(desk: &Desk, review: &str, seq: i64) -> Value {
    findings(desk, review)
        .into_iter()
        .find(|one| one["seq"] == seq)
        .unwrap_or_else(|| panic!("F{seq} is on review {review}"))
}

/// **A finding carried into round two keeps the commit its line was measured on.**
///
/// DEV-978's first criterion. Round one is at `a1b2c3d` and round two at `e4f5a6b`; the
/// carried finding's line is round one's, so its commit has to be round one's too. Pairing
/// round two's head with round one's line 40 is the defect this issue is.
#[test]
fn a_carried_findings_line_keeps_the_commit_it_was_measured_on() {
    let desk = Desk::new();
    let first = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    add_measured(&desk, &first, "the export is unbounded", 40, "a1b2c3d");
    post_round(&desk, &first);

    let second = open_review(&desk, "acme/portal", "412", "e4f5a6b");
    desk.command()
        .args([
            "finding",
            "carry",
            "--review",
            &second,
            "--from",
            &first,
            "--finding",
            "1",
            "--status",
            "still_open",
            "--by",
            "agent",
        ])
        .assert()
        .success();

    let carried = finding(&desk, &second, 1);
    assert_eq!(
        carried["line"], 40,
        "the line is round one's, as it always was"
    );
    assert_eq!(
        carried["line_sha"], "a1b2c3d",
        "and so is the commit it was counted on. Round two's head is e4f5a6b, and line \
         40 there is a different line of a file that has moved on: {carried:#}"
    );
}

/// **A finding whose line was read on the new head says so.**
///
/// DEV-978's second criterion. The same round, after its head has moved — which is what
/// DEV-962 made ordinary — and a line read against the commit the round is on now.
#[test]
fn a_line_read_on_the_new_head_is_measured_on_the_new_head() {
    let desk = Desk::new();
    let only = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    add_measured(&desk, &only, "the export is unbounded", 40, "a1b2c3d");

    // The author pushes. The round has not posted, so it moves rather than a second one
    // opening beside it — DEV-962.
    let same = open_review(&desk, "acme/portal", "412", "e4f5a6b");
    assert_eq!(same, only, "the round moved rather than a new one opening");

    add_measured(&desk, &only, "the retry has no ceiling", 58, "e4f5a6b");

    assert_eq!(
        finding(&desk, &only, 2)["line_sha"],
        "e4f5a6b",
        "a line counted against this commit is measured on this commit"
    );
}

/// **And the finding nobody re-read is unchanged, and says where it was measured.**
///
/// DEV-978's third criterion, and the other half of the test above: one round, one head
/// move, two findings, and the round's own head is the answer for only one of them. What
/// the page shows is this difference.
#[test]
fn after_a_head_move_the_finding_nobody_re_read_still_points_at_its_own_commit() {
    let desk = Desk::new();
    let only = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    add_measured(&desk, &only, "the export is unbounded", 40, "a1b2c3d");

    open_review(&desk, "acme/portal", "412", "e4f5a6b");
    add_measured(&desk, &only, "the retry has no ceiling", 58, "e4f5a6b");

    let review = desk.json(&["review", "show", "--review", &only]);
    assert_eq!(
        review["review"]["head_sha"], "e4f5a6b",
        "the round is on the new commit"
    );

    assert_eq!(
        finding(&desk, &only, 1)["line_sha"],
        "a1b2c3d",
        "F1's line is untouched by the move, so its commit is untouched too"
    );
    assert_eq!(
        finding(&desk, &only, 2)["line_sha"],
        "e4f5a6b",
        "and F2, read after it, is measured on where the round is now"
    );
}

/// **A finding nobody measured says nothing, and nothing is invented for it.**
///
/// DEV-978's fourth criterion from the other end: a caller that says nothing about the
/// commit gets a null, not the review's head. The head is where the review is, not where
/// a line was counted.
#[test]
fn a_finding_written_without_a_commit_is_not_given_the_reviews_head() {
    let desk = Desk::new();
    let only = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    add(
        &desk,
        &only,
        r#"{"lens": "sibling diff", "severity": "should",
            "claim": "the export is unbounded",
            "file": "app/Export.php", "line": 40}"#,
    );

    assert_eq!(
        finding(&desk, &only, 1)["line_sha"],
        Value::Null,
        "a1b2c3d is where the review is, which is not the same claim as where line 40 \
         was counted"
    );
}

/// **A commit that is a word is refused, naming the field.**
#[test]
fn a_measured_commit_that_is_not_a_commit_is_refused() {
    let desk = Desk::new();
    let only = open_review(&desk, "acme/portal", "412", "a1b2c3d");

    let refusal = desk
        .command()
        .args(["finding", "add", "--review", &only, "--file", "-", "--json"])
        .write_stdin(
            r#"{"lens": "sibling diff", "severity": "should", "claim": "it is unbounded",
                "file": "app/Export.php", "line": 40, "line_sha": "HEAD"}"#
                .to_string(),
        )
        .assert()
        .code(1);
    let text = String::from_utf8(refusal.get_output().stdout.clone()).expect("output is text");
    let document: Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("a refusal is still one document: {error}\n{text}"));

    let message = document["error"]["message"]
        .as_str()
        .expect("a refusal says what was wrong");
    assert!(
        message.contains("line_sha") && message.contains("hexadecimal"),
        "`--line-sha HEAD` is plausible, so the refusal names the field and says what a \
         commit looks like, rather than storing a word where a commit goes: {document:#}"
    );
}

/// **The hand-over tells the posting session which commit each thread's line came from.**
///
/// GitHub anchors a review comment against the pull request's head, and this line may have
/// been counted somewhere else — so a hand-over that gave the path and the line and kept
/// the commit to itself would be asking the session to post an anchor it cannot check.
/// Review Desk states it and posts nothing.
///
/// The plain output says it only where it differs from the head the line above already
/// names, because on an ordinary round every thread would otherwise repeat it and bury the
/// one that does not match.
#[test]
fn the_hand_over_says_which_commit_each_threads_line_was_measured_on() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    let id: i64 = review.parse().expect("the review id");

    let anchored = |seq: i64, file: &str, line: i64, measured: &str| {
        format!(
            r#"{{"lens":"error handling","severity":"should",
                 "claim":"finding {seq}","title":"Finding {seq}",
                 "file":"{file}","line":{line},"line_sha":"{measured}",
                 "flagged_line":{line},
                 "hunk":{{"file":"{file}","start_line":{start},
                          "lines":[{{"marker":"unchanged","text":"    {{"}},
                                   {{"marker":"added","text":"        return $r;"}}]}}}}"#,
            start = line - 1
        )
    };

    add(
        &desk,
        &review,
        &anchored(1, "app/Export.php", 40, "a1b2c3d"),
    );

    // The author pushes and the unposted round moves onto the new commit — DEV-962 — and
    // the session reads the delta and raises a second finding against it.
    open_review(&desk, "acme/portal", "412", "e4f5a6b");
    add(&desk, &review, &anchored(2, "app/Retry.php", 58, "e4f5a6b"));

    let sections = desk.write_file(
        "sections.json",
        r#"[{"kind":"prose","text":"**2 Should — 2 open.** The approach fits."},
            {"kind":"finding","finding_seq":1,"text":"**in diff** · `Open`"},
            {"kind":"finding","finding_seq":2,"text":"**in diff** · `Open`"}]"#,
    );
    desk.command()
        .args([
            "draft",
            "put",
            "--review",
            &review,
            "--sections",
            sections.to_str().expect("a path"),
        ])
        .assert()
        .success();
    desk.store()
        .approve_draft(
            id,
            review_desk_store::ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .expect("the developer approves");

    let handed = desk.json(&["draft", "handover", "--review", &review]);
    assert_eq!(
        handed["review"]["head_sha"], "e4f5a6b",
        "the round moved onto the new commit"
    );

    let threads = handed["threads"].as_array().expect("the threads");
    let measured: Vec<&Value> = threads.iter().map(|thread| &thread["line_sha"]).collect();
    assert_eq!(
        measured,
        vec!["a1b2c3d", "e4f5a6b"],
        "F1's line was counted before the push and F2's after it, and the hand-over says \
         so per thread: one value for the whole call could not: {threads:#?}"
    );

    // And the body's rows link at those two commits rather than both at the head.
    let body = handed["body"].as_str().expect("the body");
    assert!(
        body.contains("blob/a1b2c3d/app/Export.php#L40")
            && body.contains("blob/e4f5a6b/app/Retry.php#L58"),
        "each row links the commit its own line was measured on:\n{body}"
    );

    // The plain output names the one that is not this head, and leaves the other alone.
    let plain = desk.run(&["draft", "handover", "--review", &review]);
    assert!(
        plain.contains("F1 should on app/Export.php:40 (RIGHT) — measured at a1b2c3d"),
        "the thread whose line predates the push says where it came from:\n{plain}"
    );
    assert!(
        plain.contains("F2 should on app/Retry.php:58 (RIGHT)\n"),
        "and the one measured on this head says nothing extra:\n{plain}"
    );
    assert!(
        !plain.contains("none of these records"),
        "and the sentence for a whole list is not drawn where one of them does record \
         a commit:\n{plain}"
    );
}

/// **Where no thread records a commit, the hand-over says so once.**
///
/// Nothing backfills the column, so until sessions record it every finding on a record
/// has none — and saying it per thread would print the same sentence under all nine
/// threads of a real-length review, burying the lines a reader came for. The same rule
/// `DraftThreads.vue` follows for the same sentence. Found by a review of this branch.
#[test]
fn the_hand_over_says_no_commit_was_recorded_once_for_the_whole_list() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "a1b2c3d");
    let id: i64 = review.parse().expect("the review id");

    let anchored = |seq: i64, file: &str, line: i64| {
        format!(
            r#"{{"lens":"error handling","severity":"should",
                 "claim":"finding {seq}","title":"Finding {seq}",
                 "file":"{file}","line":{line},"flagged_line":{line},
                 "hunk":{{"file":"{file}","start_line":{start},
                          "lines":[{{"marker":"unchanged","text":"    {{"}},
                                   {{"marker":"added","text":"        return $r;"}}]}}}}"#,
            start = line - 1
        )
    };

    add(&desk, &review, &anchored(1, "app/Export.php", 40));
    add(&desk, &review, &anchored(2, "app/Retry.php", 58));

    let sections = desk.write_file(
        "sections.json",
        r#"[{"kind":"prose","text":"**2 Should — 2 open.** The approach fits."},
            {"kind":"finding","finding_seq":1,"text":"**in diff** · `Open`"},
            {"kind":"finding","finding_seq":2,"text":"**in diff** · `Open`"}]"#,
    );
    desk.command()
        .args([
            "draft",
            "put",
            "--review",
            &review,
            "--sections",
            sections.to_str().expect("a path"),
        ])
        .assert()
        .success();
    desk.store()
        .approve_draft(
            id,
            review_desk_store::ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .expect("the developer approves");

    let plain = desk.run(&["draft", "handover", "--review", &review]);

    assert_eq!(
        plain
            .matches("none of these records which commit its line was measured on")
            .count(),
        1,
        "once, above the list:\n{plain}"
    );
    assert!(
        !plain.contains("was measured on was not recorded"),
        "and not again under each thread:\n{plain}"
    );
}
