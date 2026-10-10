//! `review-desk work list`, through the binary.
//!
//! **The shape asserted here is a published interface.** The Locus dispatcher is written
//! against these field names and these exit codes, so this file is about the document and
//! not only about the four kinds: the key set, the integer times, and that an empty answer
//! is exit 0 with an empty list rather than a failure.
//!
//! DEV-877 added the fifth kind, `nits_to_ticket`, and two fields: the state of the one
//! ticket covering a review's held-back findings, and when it was asked for.

mod common;

use common::{open_review, Desk};
use review_desk_store::{Actor, ReviewEvent};
use serde_json::{json, Value};

/// The items `work list` prints, with `--json`.
fn work(desk: &Desk, arguments: &[&str]) -> Vec<Value> {
    let document = desk.json(&[&["work", "list"], arguments].concat());
    document["work"].as_array().expect("work is a list").clone()
}

/// Write a brief through the command line, the way a session does.
fn put_brief(desk: &Desk, review: &str, brief: &Value) {
    let path = desk.dir().join(format!("brief-{review}.json"));
    std::fs::write(&path, brief.to_string()).expect("write the brief");
    desk.run(&[
        "brief",
        "put",
        "--review",
        review,
        "--file",
        path.to_str().expect("a path"),
    ]);
}

/// Write a draft through the command line, the way a session does.
fn put_draft(desk: &Desk, review: &str, body: &str) {
    let path = desk.dir().join(format!("draft-{review}.md"));
    std::fs::write(&path, body).expect("write the draft");
    desk.run(&[
        "draft",
        "put",
        "--review",
        review,
        "--file",
        path.to_str().expect("a path"),
    ]);
}

#[test]
fn an_empty_desk_answers_with_an_empty_list_and_exits_zero() {
    // **Nothing waiting is an answer, not a failure.** A dispatcher polling this every
    // minute would otherwise treat the ordinary case as an error every time.
    let desk = Desk::new();
    let document = desk.json(&["work", "list"]);

    assert_eq!(
        document,
        json!({ "work": [] }),
        "an empty list, and the key is still there"
    );

    let plain = desk.run(&["work", "list"]);
    assert_eq!(plain, "nothing is waiting on an agent");
}

#[test]
fn the_document_carries_exactly_the_fields_the_dispatcher_is_written_against() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    desk.run(&[
        "review",
        "link",
        "--review",
        &review,
        "--session",
        "allele-ae03f8ce",
        "--ledger",
        "ledger-412",
    ]);

    let items = work(&desk, &[]);
    assert_eq!(items.len(), 1);
    let item = &items[0];

    // The whole key set, checked as a set. A field added without being documented, or one
    // quietly renamed, breaks this rather than breaking the dispatcher a week later.
    let mut keys: Vec<&str> = item
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "entered_at",
            "head_sha",
            "kind",
            "ledger_id",
            "nit_ticket_requested_at",
            "nit_ticket_status",
            "note",
            "pr",
            "repo",
            "review_id",
            "round",
            "session_id",
            "sign_off",
        ]
    );

    assert_eq!(item["kind"], "review_requested");
    assert_eq!(item["review_id"], review.parse::<i64>().expect("an id"));
    assert_eq!(item["repo"], "acme/portal");
    assert_eq!(item["pr"], 412);
    assert_eq!(item["head_sha"], "3f9c2ab");
    assert_eq!(item["round"], 1);
    assert_eq!(item["session_id"], "allele-ae03f8ce");
    assert_eq!(item["ledger_id"], "ledger-412");
    assert_eq!(item["sign_off"], Value::Null, "layer 2 is not settled");
    assert_eq!(item["note"], Value::Null, "only a send-back has a note");
    assert!(
        item["entered_at"].is_i64(),
        "seconds since the epoch, as every other command's --json emits a time"
    );
}

#[test]
fn each_of_the_four_kinds_comes_back_from_a_seeded_database() {
    let desk = Desk::new();

    // One review per kind, so the four are asserted together and a change that collapses
    // two of them into one cannot pass.
    let requested = open_review(&desk, "acme/portal", "1", "aaa0001");

    let confirmed = open_review(&desk, "acme/portal", "2", "aaa0002");
    put_brief(
        &desk,
        &confirmed,
        &json!({ "problem": "the second problem" }),
    );

    let sent_back = open_review(&desk, "acme/portal", "3", "aaa0003");
    put_brief(
        &desk,
        &sent_back,
        &json!({ "problem": "the third problem" }),
    );
    put_draft(&desk, &sent_back, "the third draft");

    let approved = open_review(&desk, "acme/portal", "4", "aaa0004");
    put_brief(
        &desk,
        &approved,
        &json!({ "problem": "the fourth problem" }),
    );
    put_draft(&desk, &approved, "the fourth draft");

    let waiting = open_review(&desk, "acme/portal", "5", "aaa0005");
    put_brief(&desk, &waiting, &json!({ "problem": "the fifth problem" }));

    // **The developer's three acts, through the store.** There is no command for any of
    // them — only the developer signs off, and that happens in the dashboard — so a test
    // that exercises what comes *after* them has to perform them the way the dashboard's
    // API does.
    let store = desk.store();
    let id = |review: &str| review.parse::<i64>().expect("an id");
    store
        .confirm_brief(id(&confirmed), Actor::Developer)
        .expect("confirm the second");
    store
        .confirm_brief(id(&sent_back), Actor::Developer)
        .expect("confirm the third");
    store
        .send_back_draft(id(&sent_back), "cut F2; it is the same point as F1")
        .expect("send the third back");
    store
        .confirm_brief(id(&approved), Actor::Developer)
        .expect("confirm the fourth");
    store
        .approve_draft(id(&approved), ReviewEvent::RequestChanges, Actor::Developer)
        .expect("approve the fourth");

    let items = work(&desk, &[]);
    let kinds: Vec<(&str, i64)> = items
        .iter()
        .map(|item| {
            (
                item["kind"].as_str().expect("a kind"),
                item["review_id"].as_i64().expect("an id"),
            )
        })
        .collect();

    assert_eq!(
        kinds,
        vec![
            ("review_requested", id(&requested)),
            ("brief_settled", id(&confirmed)),
            ("draft_sent_back", id(&sent_back)),
            ("draft_approved", id(&approved)),
        ],
        "four kinds, and the review whose brief is waiting to be confirmed is not among \
         them: {items:#?}"
    );

    // The one that is waiting on the developer, named, so the absence above is specific.
    assert!(
        !items
            .iter()
            .any(|item| item["review_id"].as_i64() == Some(id(&waiting))),
        "a review waiting on the developer is not an agent's work"
    );

    // The send-back carries the note, and nothing else does.
    let notes: Vec<&Value> = items.iter().map(|item| &item["note"]).collect();
    assert_eq!(
        notes
            .iter()
            .filter(|note| !note.is_null())
            .collect::<Vec<_>>()
            .len(),
        1
    );
    assert_eq!(items[2]["note"], "cut F2; it is the same point as F1");
    assert_eq!(items[1]["sign_off"], "developer");

    // And it is readable as text, for a person running it by hand.
    let plain = desk.run(&["work", "list"]);
    assert!(plain.contains("review_requested acme/portal#1"), "{plain}");
    assert!(plain.contains("cut F2"), "{plain}");

    // Marking it posted takes the approved one off the list.
    desk.run(&[
        "draft",
        "posted",
        "--review",
        &approved,
        "--github-review",
        "gh-review-3312",
    ]);
    let after = work(&desk, &[]);
    assert_eq!(after.len(), 3, "the posted review is done with: {after:#?}");
}

#[test]
fn one_review_or_one_repository_at_a_time() {
    let desk = Desk::new();
    let portal = open_review(&desk, "acme/portal", "412", "aaa0001");
    let billing = open_review(&desk, "acme/billing", "12", "aaa0002");

    let one = work(&desk, &["--review", &portal]);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0]["review_id"], portal.parse::<i64>().unwrap());

    let repo = work(&desk, &["--repo", "acme/billing"]);
    assert_eq!(repo.len(), 1);
    assert_eq!(repo[0]["review_id"], billing.parse::<i64>().unwrap());

    // A repository with nothing waiting is the empty answer, and still exit 0.
    assert_eq!(
        work(&desk, &["--repo", "acme/nothing"]),
        Vec::<Value>::new()
    );

    // Both together, which is how a session watching its own review in a dispatcher that
    // serves one repository would ask.
    let both = work(&desk, &["--review", &portal, "--repo", "acme/portal"]);
    assert_eq!(both.len(), 1);
    assert!(work(&desk, &["--review", &portal, "--repo", "acme/billing"]).is_empty());
}

#[test]
fn an_unknown_review_is_refused_rather_than_answered_with_silence() {
    // A session watching its own review has either mistyped an id or lost one, and
    // "nothing is waiting" would be the wrong answer to both.
    let desk = Desk::new();
    let document = desk.refused_json(&["work", "list", "--review", "99"]);

    assert_eq!(document["error"]["code"], "not_found");
    assert_eq!(document["error"]["kind"], "refused");
    assert!(
        document["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("99"),
        "{document}"
    );
}

#[test]
fn a_ticket_the_developer_asked_for_is_an_item_an_agent_can_discharge() {
    // **The round trip DEV-877 needs**: the developer asks in the dashboard, `work list`
    // reports it, the session files the ticket and records it, and the item goes.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(r#"{"lens":"repetition","severity":"nit","claim":"a nit for the ticket"}"#)
        .assert()
        .success();
    desk.run(&[
        "finding",
        "set",
        "--review",
        &review,
        "--finding",
        "1",
        "--disposition",
        "held_back",
        "--reason",
        "a nit; it goes in the ticket",
    ]);

    // The developer's act, in the dashboard's place: there is no command for it.
    desk.store()
        .request_nit_ticket(id, None)
        .expect("the developer asks for the one ticket");

    let items = work(&desk, &[]);
    assert_eq!(items.len(), 1, "{items:#?}");
    assert_eq!(items[0]["kind"], "nits_to_ticket");
    assert_eq!(items[0]["nit_ticket_status"], "requested");
    assert!(
        items[0]["nit_ticket_requested_at"].is_i64(),
        "the time is an integer, as every other time in this document is: {items:#?}"
    );
    assert!(
        desk.run(&["work", "list"]).contains("[nits to ticket]"),
        "and the plain line says so too"
    );

    let filed = desk.json(&[
        "review",
        "nit-ticket",
        "--review",
        &review,
        "--ticket",
        "ABC-246",
    ]);
    assert_eq!(filed["nit_ticket"]["status"], "filed");
    assert_eq!(filed["nit_ticket"]["ticket_id"], "ABC-246");
    assert_eq!(filed["nit_ticket"]["finding_seqs"], json!([1]));

    let after = work(&desk, &[]);
    assert!(
        !after.iter().any(|item| item["kind"] == "nits_to_ticket"),
        "once the session has said which ticket it filed, the ticket is not waiting: \
         {after:#?}"
    );
    assert_eq!(
        after[0]["kind"], "review_requested",
        "and what is outstanding is what always was — this review has no brief"
    );
}
