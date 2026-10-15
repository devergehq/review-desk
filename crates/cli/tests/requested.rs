//! `review requested`: the review GitHub has asked for, recorded before any round of it.
//!
//! DEV-996. **The surface is pinned**, because the Locus dispatcher's half was written
//! against it at the same time as this half:
//!
//! ```text
//! review-desk review requested --repo <owner/name> --pr <N> --requested-at <unix seconds>
//!                              [--withdrawn] [--json]
//! ```
//!
//! So these tests are as much about the shape of the answer as about what it records: the
//! five fields of the `--json` document are a contract with a caller in another repository,
//! and a renamed one is a dispatcher that silently stops reporting.

mod common;

use common::{open_review, post_round, Desk};
use serde_json::json;

const REPO: &str = "acme/portal";

/// Record a request and hand back the one JSON document.
fn record(desk: &Desk, pr: &str, requested_at: &str, extra: &[&str]) -> serde_json::Value {
    desk.json(
        &[
            &[
                "review",
                "requested",
                "--repo",
                REPO,
                "--pr",
                pr,
                "--requested-at",
                requested_at,
            ],
            extra,
        ]
        .concat(),
    )
}

#[test]
fn a_request_is_recorded_and_the_answer_holds_the_five_pinned_fields() {
    let desk = Desk::new();

    let document = record(&desk, "444", "1760000000", &[]);

    assert_eq!(
        document,
        json!({
            "repo": "acme/portal",
            "pr": 444,
            "requested_at": 1_760_000_000_i64,
            "withdrawn": false,
            "satisfied_by_review_id": null,
        }),
        "the shape another repository's caller was written against"
    );
}

#[test]
fn the_line_a_person_reads_says_the_request_and_what_became_of_it() {
    let desk = Desk::new();

    let line = desk.run(&[
        "review",
        "requested",
        "--repo",
        REPO,
        "--pr",
        "444",
        "--requested-at",
        "1760000000",
    ]);

    assert_eq!(
        line,
        "acme/portal#444 requested at 1760000000, no round opened since"
    );
}

#[test]
fn recording_the_same_request_twice_leaves_one_record() {
    let desk = Desk::new();

    record(&desk, "444", "1760000000", &[]);
    record(&desk, "444", "1760000000", &[]);

    // The dispatcher calls this on every tick for as long as GitHub lists the request, so
    // one row is an acceptance criterion rather than a nicety. Read back through the store,
    // because the command line has no verb that lists requests: nothing asks Review Desk
    // what GitHub wants, and the one reader is the queue. `crates/server/tests/requested.rs`
    // is where that reading is asserted.
    let outstanding = desk
        .store()
        .open_review_requests()
        .expect("the outstanding requests");
    assert_eq!(outstanding.len(), 1, "{outstanding:#?}");
    assert_eq!(outstanding[0].pr, 444);
}

#[test]
fn a_later_requested_at_replaces_the_one_before_it() {
    let desk = Desk::new();

    record(&desk, "444", "1760000000", &[]);
    let again = record(&desk, "444", "1760003600", &[]);

    assert_eq!(again["requested_at"], 1_760_003_600_i64);
}

#[test]
fn withdrawn_marks_the_request_and_recording_again_reopens_it() {
    let desk = Desk::new();

    record(&desk, "444", "1760000000", &[]);
    let withdrawn = record(&desk, "444", "1760000000", &["--withdrawn"]);
    assert_eq!(withdrawn["withdrawn"], true);

    // GitHub dropping a request and asking again is the shape the dispatcher watches for,
    // and the second ask is the one the queue has to draw.
    let again = record(&desk, "444", "1760086400", &[]);
    assert_eq!(again["withdrawn"], false);
    assert_eq!(again["requested_at"], 1_760_086_400_i64);
}

#[test]
fn a_round_opened_after_the_request_is_named_in_the_answer() {
    let desk = Desk::new();

    // Recorded as asked for in 2026, and the round opens now — which is after it.
    record(&desk, "412", "1760000000", &[]);
    let review = open_review(&desk, REPO, "412", "3f9c2ab");

    let answered = record(&desk, "412", "1760000000", &[]);
    assert_eq!(
        answered["satisfied_by_review_id"],
        review.parse::<i64>().expect("a review id")
    );
}

#[test]
fn a_request_recorded_after_the_last_round_is_outstanding() {
    let desk = Desk::new();

    // The case DEV-996 lists under Related, from 15 October 2026: GitHub asks for a review
    // again on a pull request whose round is posted and whose ledger entry says done. The
    // ask is after everything the record holds, so nothing answers it.
    let review = open_review(&desk, REPO, "412", "3f9c2ab");
    post_round(&desk, &review);

    let document = record(&desk, "412", "4102444800", &[]);
    assert_eq!(document["satisfied_by_review_id"], serde_json::Value::Null);
}

#[test]
fn a_repository_that_is_not_a_repository_is_refused_with_a_message() {
    let desk = Desk::new();

    let refused = desk.refused_json(&[
        "review",
        "requested",
        "--repo",
        "acme/portal#x",
        "--pr",
        "444",
        "--requested-at",
        "1760000000",
    ]);

    assert_eq!(refused["error"]["code"], "invalid_value");
    assert!(
        refused["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("owner/repo"),
        "{refused}"
    );
}

#[test]
fn a_time_at_or_before_the_epoch_is_refused_with_a_message() {
    let desk = Desk::new();

    let refused = desk.refused_json(&[
        "review",
        "requested",
        "--repo",
        REPO,
        "--pr",
        "444",
        "--requested-at",
        "0",
    ]);

    assert_eq!(refused["error"]["code"], "invalid_value");
    assert!(
        refused["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("since the epoch"),
        "{refused}"
    );
}

#[test]
fn a_missing_requested_at_is_refused_rather_than_guessed_at() {
    let desk = Desk::new();

    // **Required, because Review Desk cannot work it out.** It reads nothing from GitHub,
    // and defaulting to the moment this command ran would make every row say the request
    // arrived when the dispatcher got round to it.
    let refused = desk.refused_json(&["review", "requested", "--repo", REPO, "--pr", "444"]);

    assert_eq!(refused["error"]["code"], "usage");
}
