//! A whole review, recorded from the command line and read back.
//!
//! This is the test DEV-851 asks for: the binary driven through open, link, brief put,
//! three findings, a changed disposition, an outcome and show, with every value checked
//! against what was written.
//!
//! The developer's confirmation of the brief is written through the store, because
//! **there is no command for it**: confirming a brief is sign-off, and sign-off happens
//! in the dashboard. That is what the dashboard will do to the same record.

mod common;

use common::{open_review, Desk};
use review_desk_store::Actor;
use serde_json::json;

/// The findings the walk writes, in the order it writes them.
fn findings() -> [serde_json::Value; 3] {
    [
        json!({
            "lens": "correctness",
            "severity": "blocker",
            "file": "crates/cli/src/main.rs",
            "line": 48,
            "claim": "a refused request exits 2, which pages whoever treats 2 as broken",
            "evidence": "clap's default exit code is 2 and nothing overrides it"
        }),
        json!({
            "lens": "contract",
            "severity": "should",
            "file": "crates/cli/src/commands/review.rs",
            "claim": "review link with no ids reports success having changed nothing"
        }),
        json!({
            "lens": "clarity",
            "severity": "nit",
            "claim": "the module is called util"
        }),
    ]
}

#[test]
fn a_whole_review_is_recorded_and_read_back() {
    let desk = Desk::new();

    // Open.
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");
    assert_eq!(review, "1", "the first review of a fresh desk is review 1");

    // Link the other systems' records.
    desk.run(&[
        "review",
        "link",
        "--review",
        &review,
        "--session",
        "allele-ae03f8ce",
        "--ledger",
        "ledger-851",
        "--github-review",
        "gh-review-3312",
    ]);

    // The brief, then the developer's confirmation of it — the second through the
    // store, as the dashboard does it.
    let brief = json!({
        "problem": "nothing outside Rust can write a review",
        "diagram": "agent -> review-desk -> store -> sqlite",
        "option_map": "a command line, a local HTTP API, or a Rust library only",
        "approach_verdict": "the command line first: any agent on any platform can call it"
    });
    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(brief.to_string())
        .assert()
        .success();
    desk.store()
        .confirm_brief(review.parse().expect("the review id"), Actor::Developer)
        .expect("the developer confirms the brief");

    // Three findings. Each prints its sequence number within the review.
    let mut sequences = Vec::new();
    for finding in findings() {
        let printed = desk
            .command()
            .args(["finding", "add", "--review", &review, "--file", "-"])
            .write_stdin(finding.to_string())
            .assert()
            .success();
        sequences.push(
            String::from_utf8(printed.get_output().stdout.clone())
                .expect("output is text")
                .trim()
                .to_string(),
        );
    }
    assert_eq!(sequences, ["1", "2", "3"], "findings are numbered in order");

    // One finding held back, with the reason.
    desk.run(&[
        "finding",
        "set",
        "--review",
        &review,
        "--finding",
        "3",
        "--disposition",
        "held_back",
        "--reason",
        "a nit in a file this change does not touch",
    ]);

    // One outcome against another.
    desk.run(&[
        "outcome",
        "add",
        "--review",
        &review,
        "--finding",
        "1",
        "--kind",
        "kept",
        "--by",
        "developer",
        "--detail",
        "fixed in main.rs before the pull request opened",
    ]);

    // Show, and check every value written came back.
    let shown = desk.json(&["review", "show", "--review", &review]);

    let record = &shown["review"];
    assert_eq!(record["id"], 1);
    assert_eq!(record["repo"], "devergehq/review-desk");
    assert_eq!(record["pr"], 7);
    assert_eq!(record["head_sha"], "c0ffee1");
    assert_eq!(record["round"], 1);
    assert_eq!(record["session_id"], "allele-ae03f8ce");
    assert_eq!(record["ledger_id"], "ledger-851");
    assert_eq!(record["github_review_id"], "gh-review-3312");

    let shown_brief = &shown["brief"];
    assert_eq!(shown_brief["problem"], brief["problem"]);
    assert_eq!(shown_brief["diagram"], brief["diagram"]);
    assert_eq!(shown_brief["option_map"], brief["option_map"]);
    assert_eq!(shown_brief["approach_verdict"], brief["approach_verdict"]);
    assert!(
        shown_brief["confirmed_at"].is_i64(),
        "the brief was confirmed, so it carries a time"
    );

    let shown_findings = shown["findings"].as_array().expect("findings");
    assert_eq!(shown_findings.len(), 3);
    for (written, shown) in findings().iter().zip(shown_findings) {
        for field in ["lens", "severity", "claim"] {
            assert_eq!(shown[field], written[field], "{field}");
        }
        assert_eq!(
            shown["file"],
            written.get("file").cloned().unwrap_or(json!(null))
        );
        assert_eq!(
            shown["line"],
            written.get("line").cloned().unwrap_or(json!(null))
        );
        assert_eq!(
            shown["evidence"],
            written.get("evidence").cloned().unwrap_or(json!(null))
        );
    }
    assert_eq!(shown_findings[0]["disposition"], "in_draft");
    assert_eq!(shown_findings[1]["disposition"], "in_draft");
    assert_eq!(shown_findings[2]["disposition"], "held_back");
    assert_eq!(
        shown_findings[2]["disposition_reason"],
        "a nit in a file this change does not touch"
    );

    let outcomes = shown["outcomes"].as_array().expect("outcomes");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0]["finding_seq"], 1);
    assert_eq!(outcomes[0]["kind"], "kept");
    assert_eq!(outcomes[0]["actor"], "developer");
    assert_eq!(
        outcomes[0]["detail"],
        "fixed in main.rs before the pull request opened"
    );
    assert!(outcomes[0]["occurred_at"].is_i64());
}

#[test]
fn opening_the_same_head_commit_twice_returns_the_same_review() {
    let desk = Desk::new();

    let first = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");
    let again = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    assert_eq!(first, again);

    let opened = desk.json(&[
        "review",
        "open",
        "--repo",
        "devergehq/review-desk",
        "--pr",
        "7",
        "--head",
        "c0ffee1",
    ]);
    assert_eq!(
        opened["created"], false,
        "nothing was created the third time"
    );
    assert_eq!(opened["review"]["round"], 1);
}

#[test]
fn a_new_head_commit_opens_the_next_round() {
    let desk = Desk::new();

    let round_one = desk.json(&[
        "review",
        "open",
        "--repo",
        "devergehq/review-desk",
        "--pr",
        "7",
        "--head",
        "c0ffee1",
    ]);
    let round_two = desk.json(&[
        "review",
        "open",
        "--repo",
        "devergehq/review-desk",
        "--pr",
        "7",
        "--head",
        "c0ffee2",
    ]);

    assert_eq!(round_one["review"]["round"], 1);
    assert_eq!(round_two["review"]["round"], 2);
    assert_eq!(round_two["created"], true);
    assert_ne!(round_one["review"]["id"], round_two["review"]["id"]);

    // And the earlier commit is still handed back rather than opening a round three.
    let reopened = desk.json(&[
        "review",
        "open",
        "--repo",
        "devergehq/review-desk",
        "--pr",
        "7",
        "--head",
        "c0ffee1",
    ]);
    assert_eq!(reopened["created"], false);
    assert_eq!(reopened["review"]["id"], round_one["review"]["id"]);
}

#[test]
fn a_brief_is_read_from_a_file_as_well_as_from_standard_input() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let path = desk.db().with_file_name("brief.json");
    std::fs::write(&path, json!({ "problem": "written to a file" }).to_string()).unwrap();

    let stored = desk.json(&[
        "brief",
        "put",
        "--review",
        &review,
        "--file",
        path.to_str().unwrap(),
    ]);

    assert_eq!(stored["brief"]["problem"], "written to a file");
}

#[test]
fn replacing_a_brief_undoes_its_confirmation() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(json!({ "problem": "first" }).to_string())
        .assert()
        .success();
    let confirmed = desk
        .store()
        .confirm_brief(review.parse().expect("the review id"), Actor::Developer)
        .expect("the developer confirms the brief");
    assert!(confirmed.confirmed_at.is_some());

    let replaced = desk
        .command()
        .args(["brief", "put", "--review", &review, "--file", "-", "--json"])
        .write_stdin(json!({ "problem": "second" }).to_string())
        .assert()
        .success();
    let document: serde_json::Value =
        serde_json::from_slice(&replaced.get_output().stdout).expect("one document");

    assert_eq!(document["brief"]["problem"], "second");
    assert!(
        document["brief"]["confirmed_at"].is_null(),
        "the developer confirmed the text that was there, not what replaced it"
    );
}

#[test]
fn an_unknown_review_is_refused_with_a_code() {
    let desk = Desk::new();

    let document = desk.refused_json(&["review", "show", "--review", "99"]);

    assert_eq!(document["error"]["code"], "not_found");
    assert_eq!(document["error"]["kind"], "refused");
    assert!(document["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("99"));
}

#[test]
fn a_finding_on_a_review_that_is_not_there_is_refused() {
    let desk = Desk::new();

    let output = desk
        .command()
        .args(["finding", "add", "--review", "99", "--file", "-", "--json"])
        .write_stdin(json!({ "lens": "correctness", "severity": "nit", "claim": "x" }).to_string())
        .assert()
        .code(1);
    let document: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("one document");

    assert_eq!(document["error"]["code"], "unknown_parent");
}

#[test]
fn review_link_that_would_change_nothing_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    let document = desk.refused_json(&["review", "link", "--review", &review]);

    assert_eq!(document["error"]["code"], "bad_input");
    for flag in ["--session", "--ledger", "--github-review"] {
        assert!(
            document["error"]["message"]
                .as_str()
                .expect("a message")
                .contains(flag),
            "the refusal must name {flag}"
        );
    }
}

#[test]
fn linking_one_system_leaves_the_others_alone() {
    let desk = Desk::new();
    let review = open_review(&desk, "devergehq/review-desk", "7", "c0ffee1");

    desk.run(&[
        "review",
        "link",
        "--review",
        &review,
        "--session",
        "allele-1",
        "--ledger",
        "ledger-1",
    ]);
    let linked = desk.json(&[
        "review",
        "link",
        "--review",
        &review,
        "--github-review",
        "gh-9",
    ]);

    assert_eq!(linked["review"]["session_id"], "allele-1");
    assert_eq!(linked["review"]["ledger_id"], "ledger-1");
    assert_eq!(linked["review"]["github_review_id"], "gh-9");
}

/// **`--ticket-url` records where the ticket is, beside the id `--ticket` records.**
///
/// The two are separate fields because nothing can derive one from the other: a ticket's
/// id is opaque, and the tracker and its workspace are the session's knowledge. The
/// dashboard draws the id as a link when the address is there and as text when it is not,
/// so this is the field that decides which.
#[test]
fn a_reviews_ticket_carries_its_address_beside_its_id() {
    let desk = Desk::new();

    let review = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--ticket",
        "ABC-231",
        "--ticket-url",
        "https://linear.app/acme/issue/ABC-231",
    ]);

    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(shown["review"]["ticket_id"], "ABC-231");
    assert_eq!(
        shown["review"]["ticket_url"],
        "https://linear.app/acme/issue/ABC-231"
    );

    // The only view a person reads prints it, on its own line beside the id.
    let printed = desk.run(&["review", "show", "--review", &review]);
    assert!(
        printed.contains("ticket: ABC-231"),
        "the id is printed: {printed}"
    );
    assert!(
        printed.contains("ticket url: https://linear.app/acme/issue/ABC-231"),
        "and so is the address: {printed}"
    );

    // The id alone is a review whose header draws text rather than a link, which is the
    // state every review from before this field is in.
    let plain = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "413",
        "--head",
        "aa11bb2",
        "--ticket",
        "ABC-232",
    ]);
    let shown = desk.json(&["review", "show", "--review", &plain]);
    assert_eq!(shown["review"]["ticket_id"], "ABC-232");
    assert_eq!(
        shown["review"]["ticket_url"],
        json!(null),
        "nothing derives an address from an id"
    );
}

/// **An address that is not `https` is refused, and the review is not opened with it.**
///
/// The one thing a stored address is for is being drawn as a link, so a `javascript:`
/// value is the whole of the risk and the write is where it can be refused once. `http` is
/// refused with the rest: Review Desk never fetches a tracker, so nothing here needs the
/// insecure form.
#[test]
fn a_ticket_address_that_is_not_https_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    for refused in [
        "javascript:alert(1)",
        "http://linear.app/acme/issue/ABC-231",
        "linear.app/acme/issue/ABC-231",
    ] {
        let document = desk.refused_json(&[
            "review",
            "open",
            "--repo",
            "acme/portal",
            "--pr",
            "412",
            "--head",
            "3f9c2ab",
            "--ticket",
            "ABC-231",
            "--ticket-url",
            refused,
        ]);
        assert_eq!(document["error"]["code"], "invalid_value");
        let message = document["error"]["message"]
            .as_str()
            .expect("a message")
            .to_string();
        assert!(
            message.contains("ticket_url") && message.contains("https://"),
            "the refusal names the field and what it wanted: {message}"
        );
    }

    // And nothing was written on the way to being refused — including the ticket id,
    // which travels in the same call.
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(shown["review"]["ticket_url"], json!(null));
    assert_eq!(shown["review"]["ticket_id"], json!(null));

    // **Nor is a review opened for a head commit nobody had reviewed.** The address is
    // checked before the review is created, so a refused call spends no round and leaves
    // no review behind for the next caller to find.
    desk.refused_json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "ffff999",
        "--ticket-url",
        "javascript:alert(1)",
    ]);
    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "412"]);
    let rounds = found["reviews"].as_array().expect("the rounds");
    assert_eq!(
        rounds.len(),
        1,
        "the refused call opened no second round: {rounds:?}"
    );
}

/// **`--ticket-url` with no ticket is refused: nothing would draw the address.**
///
/// The dashboard draws the ticket's **id** as the link's text, so an address stored beside
/// a null id reaches no screen — it exists only in `review show`. A silent no-op is worse
/// than a refusal, because it looks like the flag worked.
#[test]
fn a_ticket_address_with_no_ticket_is_refused() {
    let desk = Desk::new();

    let document = desk.refused_json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--ticket-url",
        "https://linear.app/acme/issue/ABC-231",
    ]);
    assert_eq!(document["error"]["code"], "invalid_value");
    assert!(
        document["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("ticket_id"),
        "the refusal names the field that is missing: {document}"
    );

    // And it spends no round, like the refusal of a bad address: the pair is checked
    // before the review is opened.
    let found = desk.json(&["review", "find", "--repo", "acme/portal", "--pr", "412"]);
    assert!(
        found["reviews"].as_array().expect("the rounds").is_empty(),
        "the refused call opened no review: {found}"
    );

    // The address alone is accepted on a later call, once the record holds the id — which
    // is what a session does when it learns the address after opening the review.
    let review = desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--ticket",
        "ABC-231",
    ]);
    desk.run(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
        "--ticket-url",
        "https://linear.app/acme/issue/ABC-231",
    ]);
    let shown = desk.json(&["review", "show", "--review", &review]);
    assert_eq!(
        shown["review"]["ticket_url"],
        "https://linear.app/acme/issue/ABC-231"
    );
    assert_eq!(shown["review"]["round"], 1, "and spent no second round");
}
