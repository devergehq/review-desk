//! The commands an agent session calls for the parts of the record DEV-860 adds:
//! the seven layers, the structured option map, a finding that cites a rule, and the
//! draft.
//!
//! The whole of it is driven through the built binary, because that is what an agent
//! actually does. Two things are seeded through the store, and only two: a rule,
//! which the command line deliberately cannot write, and the developer's approval,
//! which is sign-off and belongs to the dashboard.

mod common;

use common::{open_review, start_review, Desk};
use review_desk_store::{
    Actor, NewRule, ReviewEvent, ReviewLayerName, ReviewLayerState, RuleScope, RuleStage,
};
use serde_json::json;

/// The draft body the session writes: markdown, with a fence and a quotation mark in
/// it, both of which have to survive being stored and read back.
const BODY: &str = r#"The queue-with-backoff approach fits the ticket.

### Blocker · F1 · A retried delivery is sent without an idempotency key

See `jobs/DeliverWebhook.php:58`, and the author's "it only happens on a timeout".

```mermaid
flowchart LR
  A[Attempt 1 times out] --> B[Endpoint still processes it]
```

**Not checked:** the worker's behaviour across a deploy restart.
"#;

#[test]
fn a_whole_review_with_layers_options_a_cited_rule_and_a_draft() {
    let desk = Desk::new();

    // A rule to cite. The command line cannot write one — promoting a rule is the
    // developer's act — so this is the one thing seeded through the store.
    let rule = desk
        .store()
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
        .expect("seed a rule");

    // Layer 1: the lint ran. The review is opened with what the caller knows about the
    // pull request.
    let review = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--title",
        "Retry failed webhook deliveries",
        "--ticket",
        "ABC-231",
        "--changed-lines",
        "214",
    ]);
    assert_eq!(review, "1", "review open prints the id and nothing else");

    desk.run(&[
        "layer", "set", "--review", &review, "--layer", "1", "--state", "done", "--detail",
        "passed",
    ]);

    // Layer 2: the brief, with the option map as cards and where it came from.
    let brief = json!({
        "problem": "A webhook delivery fails once and is never tried again.",
        "diagram": "flowchart LR\n  A[Event] --> B[Lost]",
        "approach_verdict": "B is the right shape for this ticket.",
        "provenance": "from the ticket and the code · you corrected one sentence",
        "options": [
            {
                "key": "A",
                "title": "Retry inside the request",
                "argument_for": "Smallest change, no new tables.",
                "argument_against": "Cannot span 24 hours.",
                "chosen": false
            },
            {
                "key": "B",
                "title": "Queue with backoff",
                "argument_for": "Meets the 24-hour window.",
                "argument_against": "Needs an idempotency key.",
                "chosen": true
            }
        ]
    });
    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(brief.to_string())
        .assert()
        .success();
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "2",
        "--state",
        "waiting_on_you",
        "--detail",
        "waiting on you",
    ]);

    // The developer confirms, in the dashboard's place. There is no command for it.
    desk.store()
        .confirm_brief(review.parse().expect("the review id"), Actor::Developer)
        .expect("the developer confirms the brief");
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "2",
        "--state",
        "done",
        "--detail",
        "confirmed by you",
    ]);

    // Layers 3 and 4 ran; 4 was skipped, which is not the same as not having run.
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "3",
        "--state",
        "done",
        "--detail",
        "2 options",
    ]);
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "4",
        "--state",
        "skipped",
        "--detail",
        "no diagram needed",
    ]);

    // Layer 5: two findings, the second citing the rule it was raised under.
    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({
                "lens": "sibling diff",
                "severity": "blocker",
                "file": "jobs/DeliverWebhook.php",
                "line": 58,
                "claim": "A retried delivery is sent without an idempotency key",
                "evidence": "throwaway test, 2 deliveries for 1 event"
            })
            .to_string(),
        )
        .assert()
        .success();
    let cited = desk
        .command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({
                "lens": "can this test fail",
                "severity": "should",
                "file": "tests/WebhookRetryTest.php",
                "line": 31,
                "claim": "The failure test passes with the retry code reverted",
                "rule_id": rule.id
            })
            .to_string(),
        )
        .assert()
        .success();
    assert_eq!(
        String::from_utf8(cited.get_output().stdout.clone())
            .unwrap()
            .trim(),
        "2"
    );
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "5",
        "--state",
        "done",
        "--detail",
        "2 findings",
    ]);

    // Layer 6: the draft, with the lint that was run over it.
    let lint = desk.dir().join("lint.json");
    std::fs::write(
        &lint,
        json!({
            "passes": true,
            "visible_words": 118,
            "word_budget": 150,
            "findings_in_body": 2,
            "findings_expected": 2,
            "raised_before_on_pr": 0
        })
        .to_string(),
    )
    .expect("write the lint document");
    let body = desk.dir().join("draft.md");
    std::fs::write(&body, BODY).expect("write the draft body");

    desk.run(&[
        "draft",
        "put",
        "--review",
        &review,
        "--file",
        body.to_str().expect("a path"),
        "--lint",
        lint.to_str().expect("a path"),
    ]);
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "6",
        "--state",
        "waiting_on_you",
        "--detail",
        "waiting on you",
    ]);

    // The session reports what it did not check by reopening the same head commit: the
    // same review comes back, and no round is spent.
    let again = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--not-checked",
        "the worker's behaviour across a deploy restart",
    ]);
    assert_eq!(again, review, "the same head commit is the same review");

    // Read the whole record back.
    let shown = desk.json(&["review", "show", "--review", &review]);

    let record = &shown["review"];
    assert_eq!(record["title"], "Retry failed webhook deliveries");
    assert_eq!(record["ticket_id"], "ABC-231");
    assert_eq!(record["changed_lines"], 214);
    assert_eq!(
        record["not_checked"],
        "the worker's behaviour across a deploy restart"
    );
    assert_eq!(record["round"], 1, "and reopening spent no round");

    let layers = shown["layers"].as_array().expect("layers");
    assert_eq!(
        layers.len(),
        ReviewLayerName::COUNT as usize,
        "all seven, whether or not each has been written"
    );
    assert_eq!(layers[0]["name"], "lint");
    assert_eq!(layers[0]["state"], "done");
    assert_eq!(layers[0]["detail"], "passed");
    assert_eq!(layers[3]["state"], "skipped");
    assert_eq!(layers[5]["state"], "waiting_on_you");
    assert_eq!(
        layers[6]["state"], "pending",
        "the harvest layer has not run, and says so rather than being absent"
    );
    assert_eq!(layers[6]["updated_at"], json!(null));

    let shown_brief = &shown["brief"];
    assert_eq!(
        shown_brief["provenance"],
        "from the ticket and the code · you corrected one sentence"
    );
    assert!(shown_brief["confirmed_at"].is_i64());
    assert_eq!(shown_brief["confirmed_by"], "developer");
    let options = shown_brief["options"].as_array().expect("options");
    assert_eq!(options.len(), 2);
    assert_eq!(options[0]["key"], "A");
    assert_eq!(options[0]["chosen"], false);
    assert_eq!(options[1]["key"], "B");
    assert_eq!(options[1]["title"], "Queue with backoff");
    assert_eq!(options[1]["chosen"], true);
    assert_eq!(options[1]["argument_against"], "Needs an idempotency key.");

    let findings = shown["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0]["rule_id"], json!(null));
    assert_eq!(
        findings[1]["rule_id"], rule.id,
        "the finding names the rule it was raised under"
    );

    let draft = &shown["draft"];
    assert_eq!(draft["status"], "ready");
    assert_eq!(draft["body"], BODY, "the body is stored verbatim");
    assert_eq!(draft["lint"]["passes"], true);
    assert_eq!(draft["lint"]["visible_words"], 118);
    assert!(
        draft["lint"]["checked_at"].is_i64(),
        "a lint that reported something says when it ran"
    );

    // The rule now knows it was cited, which is what the whole feedback loop needs.
    let counts = desk.store().rule_counts(rule.id).expect("rule counts");
    assert_eq!(counts.raised, Some(1));
    assert!(desk
        .store()
        .get_rule(rule.id)
        .unwrap()
        .last_cited_at
        .is_some());

    // And the draft, read as a review session reads it while it waits.
    let waiting = desk.run(&["draft", "show", "--review", &review]);
    assert!(waiting.starts_with("draft of review 1: ready"), "{waiting}");
    assert!(waiting.contains("post as: not approved yet"), "{waiting}");
    assert!(waiting.contains("118 of 150 words"), "{waiting}");
    assert!(waiting.contains("flowchart LR"), "the body is printed too");

    // The developer approves — in the dashboard — and the session posts and reports it.
    desk.store()
        .approve_draft(
            review.parse().expect("the review id"),
            ReviewEvent::RequestChanges,
            Actor::Developer,
        )
        .expect("the developer approves");

    let told = desk.run(&["draft", "show", "--review", &review]);
    assert!(told.contains("draft of review 1: approved"), "{told}");
    assert!(
        told.contains("post as: request_changes"),
        "this is how the session learns what to post: {told}"
    );

    desk.run(&[
        "draft",
        "posted",
        "--review",
        &review,
        "--github-review",
        "gh-review-3312",
    ]);
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "7",
        "--state",
        "running",
        "--detail",
        "harvesting",
    ]);

    let finished = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(finished["draft"]["status"], "posted");
    assert_eq!(finished["review"]["github_review_id"], "gh-review-3312");
    assert_eq!(finished["layers"][6]["state"], "running");
}

#[test]
fn review_find_lists_a_pull_requests_rounds_newest_first() {
    let desk = Desk::new();

    // Each round is reviewed before the next commit arrives, which is what makes the
    // next commit a round: since DEV-945 a commit on a round nobody started moves that
    // round instead.
    let first = open_review(&desk, "acme/portal", "412", "aaaaaaa");
    start_review(&desk, &first);
    let second = open_review(&desk, "acme/portal", "412", "bbbbbbb");
    start_review(&desk, &second);
    let third = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "ccccccc",
        "--title",
        "Retry failed webhook deliveries",
    ]);
    // Another pull request, which must not appear.
    open_review(&desk, "acme/portal", "407", "ddddddd");

    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "412"]);
    let reviews = found["reviews"].as_array().expect("reviews");

    assert_eq!(reviews.len(), 3, "only this pull request's rounds");
    assert_eq!(
        reviews
            .iter()
            .map(|review| review["round"].as_i64().expect("a round"))
            .collect::<Vec<_>>(),
        [3, 2, 1],
        "newest round first, which is the one an agent is working on"
    );
    assert_eq!(reviews[0]["id"].to_string(), third);
    assert_eq!(reviews[0]["head_sha"], "ccccccc");

    let plain = desk.run(&["review", "find", "--repo", "acme/portal", "--pr", "412"]);
    assert!(
        plain.starts_with("3 round 3 head ccccccc — Retry failed webhook deliveries"),
        "{plain}"
    );

    let none = desk.run(&["review", "find", "--repo", "acme/portal", "--pr", "999"]);
    assert_eq!(none, "no reviews of acme/portal#999");
}

#[test]
fn an_invalid_layer_state_or_number_is_refused_with_the_valid_ones_named() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let document = desk.refused_json(&[
        "layer", "set", "--review", &review, "--layer", "5", "--state", "finished",
    ]);
    assert_eq!(document["error"]["code"], "invalid_value");
    let message = document["error"]["message"].as_str().expect("a message");
    for state in ReviewLayerState::ALL {
        assert!(message.contains(state.as_str()), "{message}");
    }

    let document = desk.refused_json(&[
        "layer", "set", "--review", &review, "--layer", "8", "--state", "done",
    ]);
    assert_eq!(document["error"]["code"], "invalid_value");
    assert!(document["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("1 to 7"));
}

#[test]
fn a_draft_body_comes_from_standard_input_as_well_as_from_a_file() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    desk.command()
        .args(["draft", "put", "--review", &review, "--file", "-"])
        .write_stdin(BODY)
        .assert()
        .success();

    assert_eq!(
        desk.json(&["draft", "show", "--review", &review])["draft"]["body"],
        BODY,
        "piped markdown is stored exactly as given, fences and quotation marks included"
    );
}

#[test]
fn a_draft_body_that_cannot_be_read_is_refused_and_opens_no_database() {
    let desk = Desk::new();

    let document = desk.refused_json(&[
        "draft",
        "put",
        "--review",
        "1",
        "--file",
        "/nowhere/at/all/draft.md",
    ]);

    assert_eq!(document["error"]["code"], "bad_input");
    assert!(!desk.db().exists(), "a refused read creates no database");
}

#[test]
fn a_draft_show_on_a_review_with_no_draft_is_not_found() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let document = desk.refused_json(&["draft", "show", "--review", &review]);
    assert_eq!(document["error"]["code"], "not_found");
}

#[test]
fn the_view_a_person_reads_names_the_rule_a_finding_cites() {
    // The citation is the field the feedback loop runs through. Printing it only in
    // the JSON would hide it from the one view a person actually reads — the same
    // argument as the line number that has no file.
    let desk = Desk::new();
    let rule = desk
        .store()
        .create_rule(NewRule {
            statement: "A new test must fail when the fix is reverted".to_string(),
            prevents: "a regression behind a test that cannot fail".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .expect("a rule");
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            json!({
                "lens": "can this test fail",
                "severity": "should",
                "claim": "the test passes with the fix reverted",
                "rule_id": rule.id
            })
            .to_string(),
        )
        .assert()
        .success();

    let shown = desk.run(&["review", "show", "--review", &review]);
    assert!(
        shown.contains(&format!("same ruling as R-{:02}", rule.id)),
        "the cited rule must survive into the plain view:\n{shown}"
    );
}
