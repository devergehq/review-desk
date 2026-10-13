//! What `review open` does with a new commit when the binary is actually run, and how a
//! dispatcher reads the answer. DEV-962, and DEV-954's `started`.
//!
//! **Driven through the binary because the issue asks for it**: "New commits on a started,
//! unposted review move its head; no new round is created, tested against the real binary."
//! Every test gets its own database through [`Desk`], so none can read or write
//! `~/.review-desk/review-desk.db`.

mod common;

use common::{open_review, post_round, Desk};

/// A finding, so that somebody has begun on the round.
fn begin(desk: &Desk, review: &str) {
    desk.command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(
            serde_json::json!({
                "lens": "correctness",
                "severity": "should",
                "claim": "a session read this round",
            })
            .to_string(),
        )
        .assert()
        .success();
}

#[test]
fn a_new_commit_on_an_unposted_round_moves_it_and_says_so() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "41", "4d9464f");
    begin(&desk, &review);

    let moved = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "41",
        "--head",
        "a41c9e2",
    ]);

    assert_eq!(moved["action"], "head_moved");
    assert_eq!(moved["created"], false);
    assert_eq!(moved["head_moved_from"], "4d9464f");
    assert_eq!(moved["review"]["id"].to_string(), review);
    assert_eq!(moved["review"]["round"], 1, "no round was spent");
    assert_eq!(moved["review"]["head_sha"], "a41c9e2");

    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "41"]);
    assert_eq!(
        found["reviews"].as_array().expect("the rounds").len(),
        1,
        "one request, one row: {found}"
    );
}

#[test]
fn a_new_commit_after_a_post_opens_the_next_round() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "42", "4d9464f");
    begin(&desk, &review);
    post_round(&desk, &review);

    let created = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "42",
        "--head",
        "a41c9e2",
    ]);

    assert_eq!(created["action"], "created");
    assert_eq!(created["created"], true);
    assert_eq!(created["review"]["round"], 2);
    assert_eq!(created["head_moved_from"], serde_json::Value::Null);
}

#[test]
fn the_same_commit_again_is_unchanged() {
    let desk = Desk::new();
    open_review(&desk, "acme/portal", "43", "4d9464f");

    let again = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "43",
        "--head",
        "4d9464f",
    ]);

    assert_eq!(again["action"], "unchanged");
    assert_eq!(again["created"], false);
    assert_eq!(again["head_moved_from"], serde_json::Value::Null);
}

#[test]
fn commits_that_only_brought_the_base_branch_in_change_nothing() {
    let desk = Desk::new();
    let review = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "44",
        "--head",
        "4d9464f",
        "--changed-lines",
        "214",
    ]);
    let id = review["review"]["id"].to_string();

    let base = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "44",
        "--head",
        "9d02e71",
        "--diff-unchanged",
    ]);

    assert_eq!(base["action"], "diff_unchanged");
    assert_eq!(base["created"], false);
    assert_eq!(base["head_moved_from"], serde_json::Value::Null);
    assert_eq!(
        base["review"]["head_sha"], "4d9464f",
        "the head did not move"
    );
    assert_eq!(
        base["review"]["changed_lines"], 214,
        "and the line count is still right, because the diff it measured did not change"
    );

    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "44"]);
    assert_eq!(found["reviews"].as_array().expect("the rounds").len(), 1);
    assert_eq!(found["reviews"][0]["id"].to_string(), id);
}

#[test]
fn review_open_still_prints_the_id_and_nothing_else() {
    // The whole of what a shell caller has to do is `ID=$(review-desk review open …)`, and
    // the four actions must not change that.
    let desk = Desk::new();
    let id = open_review(&desk, "acme/portal", "45", "4d9464f");
    assert!(id.parse::<i64>().is_ok(), "{id}");

    let moved = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "45",
        "--head",
        "a41c9e2",
    ]);
    assert_eq!(moved, id, "the same review, so the same id");

    let base = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "45",
        "--head",
        "9d02e71",
        "--diff-unchanged",
    ]);
    assert_eq!(base, id);
}

#[test]
fn review_find_says_whether_anybody_has_begun() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "46", "4d9464f");

    let before = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "46"]);
    assert_eq!(
        before["reviews"][0]["started"], false,
        "a request nobody has picked up: {before}"
    );

    begin(&desk, &review);

    let after = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "46"]);
    assert_eq!(after["reviews"][0]["started"], true);
    assert_eq!(
        after["reviews"][0]["id"].to_string(),
        review,
        "and the row is still the review's own fields, with one key more"
    );
    assert_eq!(after["reviews"][0]["head_sha"], "4d9464f");
}

#[test]
fn a_session_that_opens_with_its_id_has_started_on_the_first_call() {
    // DEV-954: the worker passes `--session` on its *first* `review open`, and what that
    // buys is that a dispatcher reading `started` sees the round as somebody's from that
    // call rather than from whenever the session gets to `review link`.
    let desk = Desk::new();
    desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "47",
        "--head",
        "4d9464f",
        "--session",
        "allele-ae03f8ce",
    ]);

    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "47"]);
    assert_eq!(found["reviews"][0]["session_id"], "allele-ae03f8ce");
    assert_eq!(
        found["reviews"][0]["started"], true,
        "claimed by the first call: {found}"
    );
}

#[test]
fn a_pull_request_out_of_play_leaves_the_work_list_and_comes_back() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "48", "4d9464f");

    let work = desk.json(&["work", "list"]);
    assert_eq!(work["work"][0]["review_id"].to_string(), review);

    let out = desk.json(&[
        "review",
        "out-of-play",
        "--repo",
        "acme/portal",
        "--pr",
        "48",
        "--reason",
        "draft",
    ]);
    assert_eq!(out["out_of_play"], "draft");
    assert_eq!(out["rounds"][0]["out_of_play"], "draft");
    assert_eq!(
        desk.json(&["work", "list"])["work"]
            .as_array()
            .expect("the work list")
            .len(),
        0,
        "no session is dispatched for a pull request the author has pulled back"
    );

    let back = desk.json(&["review", "in-play", "--repo", "acme/portal", "--pr", "48"]);
    assert_eq!(back["out_of_play"], serde_json::Value::Null);
    assert_eq!(
        desk.json(&["work", "list"])["work"][0]["review_id"].to_string(),
        review,
        "and it is in front of a session again, on the round it was on"
    );
}

#[test]
fn a_withdrawn_request_says_which_of_the_two_reasons_it_is() {
    let desk = Desk::new();
    open_review(&desk, "acme/portal", "49", "4d9464f");

    let out = desk.run(&[
        "review",
        "out-of-play",
        "--repo",
        "acme/portal",
        "--pr",
        "49",
        "--reason",
        "withdrawn",
    ]);
    assert_eq!(out, "acme/portal#49 is out of play (withdrawn), 1 round");

    let back = desk.run(&["review", "in-play", "--repo", "acme/portal", "--pr", "49"]);
    assert_eq!(back, "acme/portal#49 is back in play, 1 round");
}

#[test]
fn a_reason_that_is_not_one_of_the_two_is_refused_by_name() {
    let desk = Desk::new();
    open_review(&desk, "acme/portal", "50", "4d9464f");

    let failure = desk
        .command()
        .args([
            "review",
            "out-of-play",
            "--repo",
            "acme/portal",
            "--pr",
            "50",
            "--reason",
            "ready",
        ])
        .assert()
        .failure();
    let said = String::from_utf8(failure.get_output().stderr.clone()).expect("text");
    assert!(said.contains("draft"), "{said}");
    assert!(said.contains("withdrawn"), "{said}");
    assert!(
        said.contains("review in-play"),
        "a caller who typed the way back has to be told where it is: {said}"
    );
}

#[test]
fn taking_a_pull_request_nobody_has_reviewed_out_of_play_is_refused() {
    let desk = Desk::new();

    desk.command()
        .args([
            "review",
            "out-of-play",
            "--repo",
            "acme/portal",
            "--pr",
            "51",
            "--reason",
            "draft",
        ])
        .assert()
        .failure();
}
