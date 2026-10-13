//! What `GET /reviews/{id}` says about a head that moved, and what the draft says about a
//! sign-off that was withdrawn under it. DEV-962, and DEV-954's dismissed round.
//!
//! The record is written through the store, which is what `review open` writes through —
//! the API has no operation that moves a head, because the caller that sees a new commit is
//! the one watching GitHub. What is under test here is the wire: a page cannot say "updated
//! for 2 commits since 4d9464f" from a field nothing serves.

mod common;

use review_desk_store::{
    Actor, DiffAtHead, Disposition, NewDraft, NewFinding, NewRoundCommit, ReviewEvent, ReviewFacts,
    Severity,
};

const REPO: &str = "acme/portal";

fn a_finding(claim: &str) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity: Severity::Should,
        claim: claim.to_string(),
        disposition: Disposition::InDraft,
        ..Default::default()
    }
}

#[test]
fn a_review_whose_head_moved_says_so_and_for_how_many_commits() {
    let harness = common::start_with_no_github();
    let store = &harness.store;

    let review = store
        .open_review(REPO, 41, "4d9464f")
        .expect("the request")
        .review
        .id;
    store
        .add_finding(review, a_finding("the retry has no ceiling"))
        .expect("a finding");
    store
        .open_review(REPO, 41, "1b7c4e0")
        .expect("the author pushed");
    store
        .put_round_commits(
            review,
            &[
                NewRoundCommit {
                    sha: "4d9464f".to_string(),
                    summary: "Send an idempotency key with each delivery".to_string(),
                    committed_at: None,
                    files: vec![],
                },
                NewRoundCommit {
                    sha: "a41c9e2".to_string(),
                    summary: "Record a delivery as dead".to_string(),
                    committed_at: None,
                    files: vec![],
                },
                NewRoundCommit {
                    sha: "1b7c4e0".to_string(),
                    summary: "Cap the retry backoff".to_string(),
                    committed_at: None,
                    files: vec![],
                },
            ],
        )
        .expect("the session re-read the pull request");

    let answer = harness.api("GET", &format!("/reviews/{review}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let body = answer.json();
    let updates = &body["head_updates"];

    assert_eq!(updates["since_sha"], "4d9464f");
    assert_eq!(updates["moved"], 1);
    assert_eq!(updates["base_only"], 0);
    assert_eq!(updates["folded"], 0);
    assert_eq!(
        updates["commits"], 2,
        "the two commits after the one this round was opened on: {updates}"
    );
    assert_eq!(updates["updates"][0]["kind"], "moved");
    assert_eq!(updates["updates"][0]["from_sha"], "4d9464f");
    assert_eq!(updates["updates"][0]["to_sha"], "1b7c4e0");
    assert!(
        updates["updates"][0]["at"]
            .as_str()
            .expect("a time")
            .ends_with('Z'),
        "times on the wire are RFC 3339, as every other time this API serves: {updates}"
    );
    assert_eq!(
        body["review"]["head_sha"], "1b7c4e0",
        "and the review is of the commit it moved to"
    );
}

#[test]
fn a_round_whose_head_never_moved_serves_the_empty_answer() {
    // Not an absent field: a page that has to say "nothing has changed" needs the answer,
    // and `null` would make every reader write the same branch.
    let harness = common::start_with_no_github();
    let review = harness
        .store
        .open_review(REPO, 42, "4d9464f")
        .expect("the request")
        .review
        .id;

    let body = harness
        .api("GET", &format!("/reviews/{review}"), None)
        .json();
    let updates = &body["head_updates"];
    assert!(updates["since_sha"].is_null());
    assert_eq!(updates["moved"], 0);
    assert!(
        updates["commits"].is_null(),
        "null rather than zero: nothing recorded them, which is not 'nothing changed'"
    );
    assert_eq!(updates["updates"].as_array().expect("the rows").len(), 0);
}

#[test]
fn commits_that_only_brought_the_base_branch_in_are_served_as_such() {
    let harness = common::start_with_no_github();
    let review = harness
        .store
        .open_review(REPO, 43, "4d9464f")
        .expect("the request")
        .review
        .id;
    harness
        .store
        .open_review_with(
            REPO,
            43,
            "7e2a9d3",
            &ReviewFacts::default(),
            DiffAtHead::Unchanged,
        )
        .expect("the base branch came in");

    let body = harness
        .api("GET", &format!("/reviews/{review}"), None)
        .json();
    let updates = &body["head_updates"];
    assert_eq!(updates["base_only"], 1);
    assert_eq!(updates["moved"], 0);
    assert!(
        updates["since_sha"].is_null(),
        "there is no commit to say the review was updated since"
    );
    assert_eq!(updates["updates"][0]["kind"], "base_only");
    assert_eq!(
        body["review"]["head_sha"], "4d9464f",
        "and the head did not move"
    );
}

#[test]
fn a_draft_whose_sign_off_was_withdrawn_says_which_commit_was_approved() {
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let review = store
        .open_review(REPO, 44, "4d9464f")
        .expect("the request")
        .review
        .id;
    store
        .add_finding(review, a_finding("the retry has no ceiling"))
        .expect("a finding");
    store
        .put_draft(review, NewDraft::ready("One finding on the retry path.\n"))
        .expect("a draft");
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer signed it off");
    store
        .open_review(REPO, 44, "1b7c4e0")
        .expect("and then the author pushed");

    let answer = harness.api("GET", &format!("/reviews/{review}/draft"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let body = answer.json();

    assert_eq!(
        body["draft"]["status"], "drafting",
        "no longer signed off: the developer approved different code"
    );
    assert_eq!(body["draft"]["approved_at"], serde_json::Value::Null);
    assert_eq!(
        body["draft"]["signoff_withdrawn_head"], "4d9464f",
        "the commit they did approve, which is what the page names: {body}"
    );
    assert!(body["draft"]["signoff_withdrawn_at"].is_string());
    assert_eq!(
        body["draft"]["event"], "request_changes",
        "and what it is to be posted as is still true"
    );
}

#[test]
fn the_round_timeline_says_which_rounds_are_dismissed() {
    // DEV-954. Every other field on a round summary is a count, so a dismissed round's
    // summary is indistinguishable from a live one's without this.
    let harness = common::start_with_no_github();
    let store = &harness.store;

    let one = store.open_review(REPO, 45, "4d9464f").expect("round 1");
    store
        .set_review_links(
            one.review.id,
            review_desk_store::ReviewLinks {
                github_review_id: Some("gh-review-45".to_string()),
                ..Default::default()
            },
        )
        .expect("round 1 posted");
    let two = store.open_review(REPO, 45, "1b7c4e0").expect("round 2");
    store
        .dismiss_review(two.review.id, "superseded, waiting for the rewrite")
        .expect("dismissed");

    let body = harness
        .api("GET", &format!("/reviews/{}", one.review.id), None)
        .json();
    let rounds = body["rounds"].as_array().expect("the timeline");
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0]["dismissed"], false);
    assert_eq!(
        rounds[1]["dismissed"], true,
        "so the timeline can draw it as dismissed rather than as live: {body}"
    );
}

#[test]
fn a_review_whose_pull_request_is_out_of_play_says_so_on_the_wire() {
    let harness = common::start_with_no_github();
    let review = harness
        .store
        .open_review(REPO, 46, "4d9464f")
        .expect("the request")
        .review
        .id;
    harness
        .store
        .set_pr_out_of_play(REPO, 46, Some(review_desk_store::OutOfPlay::Draft))
        .expect("the author converted it to a draft");

    let body = harness
        .api("GET", &format!("/reviews/{review}"), None)
        .json();
    assert_eq!(body["review"]["out_of_play"], "draft");
    assert!(body["review"]["out_of_play_at"].is_string());
    assert_eq!(
        body["not_for_signing"],
        serde_json::Value::Null,
        "and it still may be signed off: the pull request is coming back"
    );

    let listed = harness.api("GET", "/reviews", None).json();
    let row = listed["reviews"]
        .as_array()
        .expect("the rows")
        .iter()
        .find(|row| row["review"]["id"] == review)
        .expect("drawn under In play");
    assert_eq!(
        row["left_the_desk"], "out_of_play",
        "the row says where the review went rather than disappearing: {row}"
    );
}

#[test]
fn the_tidy_shows_each_extra_unposted_round_and_what_it_would_do() {
    // The proposal is a read and decides nothing: DEV-962 asks for a tidy that shows each
    // pull request's extra unposted rounds and acts only on the developer's confirmation.
    let harness = common::start_with_no_github();
    let store = &harness.store;

    // Written through `create_review`, because `open_review` cannot make a second unposted
    // round any more — that is the whole of DEV-962 — and this tidy is for the record the
    // rule before it left.
    let mut rounds = Vec::new();
    for (round, head) in [(1, "4d9464f"), (2, "a41c9e2"), (3, "1b7c4e0")] {
        rounds.push(
            store
                .create_review(review_desk_store::NewReview {
                    repo: REPO.to_string(),
                    pr: 61,
                    head_sha: head.to_string(),
                    round,
                    ..Default::default()
                })
                .expect("a round, as the old rule left it")
                .id,
        );
    }
    store
        .put_draft(rounds[0], NewDraft::ready("One finding.\n"))
        .expect("round 1 holds a draft");

    let answer = harness.api("GET", "/duplicate-rounds", None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let body = answer.json();

    assert_eq!(body["pull_requests_affected"], 1);
    assert_eq!(body["extra_rounds"], 2);
    let found = &body["pull_requests"][0];
    assert_eq!(found["repo"], REPO);
    assert_eq!(found["pr"], 61);
    assert_eq!(found["newest"]["review_id"], rounds[2]);
    assert_eq!(found["newest"]["round"], 3);
    assert_eq!(found["extras"][0]["review_id"], rounds[0]);
    assert_eq!(
        found["extras"][0]["act"], "dismiss",
        "it holds a draft, so it is kept and hidden rather than removed: {found}"
    );
    assert_eq!(found["extras"][0]["holds"], "a draft");
    assert_eq!(found["extras"][1]["act"], "fold");
    assert_eq!(found["extras"][1]["holds"], "nothing");
}

#[test]
fn folding_a_round_answers_with_both_rounds_and_records_the_head() {
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let one = store
        .create_review(review_desk_store::NewReview {
            repo: REPO.to_string(),
            pr: 62,
            head_sha: "4d9464f".to_string(),
            round: 1,
            ..Default::default()
        })
        .expect("round 1")
        .id;
    let two = store
        .create_review(review_desk_store::NewReview {
            repo: REPO.to_string(),
            pr: 62,
            head_sha: "a41c9e2".to_string(),
            round: 2,
            ..Default::default()
        })
        .expect("round 2")
        .id;

    let answer = harness.api("POST", &format!("/reviews/{one}/fold"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    let body = answer.json();
    assert_eq!(body["folded"]["id"], one);
    assert_eq!(body["into"]["id"], two);

    assert_eq!(
        harness.api("GET", &format!("/reviews/{one}"), None).status,
        404,
        "the empty row has gone"
    );
    let newest = harness.api("GET", &format!("/reviews/{two}"), None).json();
    assert_eq!(newest["head_updates"]["folded"], 1);
    assert_eq!(newest["head_updates"]["updates"][0]["kind"], "folded");
    assert_eq!(
        newest["head_updates"]["updates"][0]["from_sha"], "4d9464f",
        "so the history survives the row: {newest}"
    );
}

#[test]
fn folding_a_round_that_holds_work_is_refused_and_names_what_it_holds() {
    let harness = common::start_with_no_github();
    let store = &harness.store;
    let one = store
        .create_review(review_desk_store::NewReview {
            repo: REPO.to_string(),
            pr: 63,
            head_sha: "4d9464f".to_string(),
            round: 1,
            ..Default::default()
        })
        .expect("round 1")
        .id;
    store
        .create_review(review_desk_store::NewReview {
            repo: REPO.to_string(),
            pr: 63,
            head_sha: "a41c9e2".to_string(),
            round: 2,
            ..Default::default()
        })
        .expect("round 2");
    store
        .add_finding(one, a_finding("the retry has no ceiling"))
        .expect("a finding");

    let answer = harness.api("POST", &format!("/reviews/{one}/fold"), None);
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(
        answer.error_code(),
        "conflict",
        "the contract's code for a refusal about the state of the record"
    );
    assert!(
        answer.text().contains("1 finding"),
        "the refusal names what it holds, so the screen can offer a dismissal: {}",
        answer.text()
    );
    assert_eq!(
        harness.api("GET", &format!("/reviews/{one}"), None).status,
        200,
        "and the round is still there"
    );
}

#[test]
fn folding_a_round_nobody_has_is_not_found() {
    let harness = common::start_with_no_github();
    let answer = harness.api("POST", "/reviews/9001/fold", None);
    assert_eq!(answer.status, 404, "{}", answer.text());
    assert_eq!(answer.error_code(), "not_found");
}

#[test]
fn the_tidy_can_be_asked_about_one_repository_and_refuses_a_filter_that_is_not_one() {
    // The filter is the Reviews page's own picker, so it reaches this read the way it
    // reaches `GET /reviews` — through the one function both of them ask it through.
    let harness = common::start_with_no_github();
    let store = &harness.store;
    for (repo, pr) in [(REPO, 71), ("devergehq/locus", 72)] {
        for (round, head) in [(1, "4d9464f"), (2, "a41c9e2")] {
            store
                .create_review(review_desk_store::NewReview {
                    repo: repo.to_string(),
                    pr,
                    head_sha: head.to_string(),
                    round,
                    ..Default::default()
                })
                .expect("a round, as the old rule left it");
        }
    }

    let all = harness.api("GET", "/duplicate-rounds", None).json();
    assert_eq!(all["pull_requests_affected"], 2);

    let one = harness
        .api("GET", "/duplicate-rounds?repo=ACME%2FPortal", None)
        .json();
    assert_eq!(
        one["pull_requests_affected"], 1,
        "one repository, however it is spelled: {one}"
    );
    assert_eq!(one["pull_requests"][0]["repo"], REPO);

    let refused = harness.api("GET", "/duplicate-rounds?repo=portal", None);
    assert_eq!(refused.status, 400, "{}", refused.text());
    assert_eq!(refused.error_code(), "invalid_value");
}

#[test]
fn pasting_the_link_of_a_pull_request_that_moved_does_not_invent_a_line_count() {
    // **The Inbox's own entry point reaches the move path now**, which DEV-962 made true:
    // `POST /reviews` resolves the head from GitHub when none is given, so a developer
    // pasting the link of a pull request the author has pushed to moves the unposted round.
    // The move clears `changed_lines`, because a figure measured on the commit the review has
    // stopped being of is a wrong answer — and the gap-filling below it would then write the
    // *whole pull request's* additions plus deletions into a round that is about a delta.
    // Found by a review of this branch.
    let harness = common::start(common::recorded_github());
    let store = &harness.store;

    // Round 1 of the recorded pull request, opened on an earlier commit with the delta the
    // session actually reviewed, and posted — so the next commit is a round of its own.
    let one = store
        .open_review_with(
            "devergehq/locus",
            64,
            "4d9464f",
            &ReviewFacts {
                changed_lines: Some(180),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("round 1")
        .review
        .id;
    store
        .set_review_links(
            one,
            review_desk_store::ReviewLinks {
                github_review_id: Some("PRR_round_one".to_string()),
                ..Default::default()
            },
        )
        .expect("round 1 posted");

    // Round 2, opened by a session on the commit it read, with its own delta.
    let two = store
        .open_review_with(
            "devergehq/locus",
            64,
            "a41c9e2",
            &ReviewFacts {
                changed_lines: Some(31),
                ..Default::default()
            },
            DiffAtHead::Unsaid,
        )
        .expect("round 2")
        .review
        .id;

    // The developer pastes the link. The server resolves the pull request's head, which is a
    // commit round 2 has not seen, and round 2's head moves to it.
    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );
    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["id"], two, "the same round, moved");
    assert_eq!(review["head_sha"], common::RECORDED_HEAD_SHA);
    assert_eq!(
        review["changed_lines"],
        serde_json::Value::Null,
        "and no figure is invented for it: {review}"
    );
    assert_ne!(
        review["changed_lines"],
        common::RECORDED_CHANGED_LINES,
        "least of all the whole pull request's, on a round that is about a delta"
    );
}
