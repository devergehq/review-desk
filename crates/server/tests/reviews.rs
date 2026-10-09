//! The three operations the store on `master` can answer in full.
//!
//! `POST /reviews` carries most of the weight, because it is the one operation that reads
//! GitHub and DEV-861 is specific about the order it does things in: the request is
//! checked first, so a machine with no `gh` reports the real problem; and if the read
//! fails, nothing is created.

mod common;

use review_desk_store::Disposition;

#[test]
fn a_link_alone_creates_a_review_from_a_recorded_github_response() {
    let harness = common::start(common::recorded_github());

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["repo"], "devergehq/locus");
    assert_eq!(review["pr"], 64);
    assert_eq!(
        review["head_sha"],
        common::RECORDED_HEAD_SHA,
        "the head commit has to be the one GitHub reported, resolved before creation"
    );
    assert_eq!(review["round"], 1);

    // **The title, from the same read.** A review started from the Inbox has nothing but
    // a pasted link, so if this read does not resolve a title the review has none — and a
    // title is what both the Inbox and the One review screen list a review under. Until
    // DEV-862 the `GitHub` trait carried no title and this was null for every review the
    // screen started.
    assert_eq!(
        review["title"],
        common::RECORDED_TITLE,
        "the title has to be the one GitHub reported: {}",
        answer.text()
    );
    assert_eq!(
        review["changed_lines"],
        common::RECORDED_CHANGED_LINES,
        "changed lines are additions plus deletions, as GitHub counted them: {}",
        answer.text()
    );

    // No network was involved: the only GitHub this server had was a directory of files.
    let stored = harness
        .store
        .find_review_by_id(review["id"].as_i64().unwrap())
        .unwrap()
        .expect("the review is on the record");
    assert_eq!(stored.head_sha, common::RECORDED_HEAD_SHA);
    assert_eq!(
        stored.title.as_deref(),
        Some(common::RECORDED_TITLE),
        "and it is on the record, not only in the answer"
    );
    assert_eq!(stored.changed_lines, Some(common::RECORDED_CHANGED_LINES));
}

#[test]
fn the_review_a_link_starts_turns_up_in_the_inbox_under_its_title() {
    // The acceptance criterion is about the screen, not the handler: "starting a review
    // from a link in the inbox creates a review that has a title". The Inbox reads
    // `GET /inbox`, and a brief is what puts a review in it, so this follows the review
    // the whole way to the group the screen renders.
    let harness = common::start(common::recorded_github());

    let started = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let review_id = started.json()["id"].as_i64().expect("a review id");

    // A review waits in "Confirm the problem" once a session has written its brief,
    // which is the state the Inbox lists.
    harness
        .store
        .put_brief(
            review_id,
            review_desk_store::NewBrief {
                problem: "Dispatcher watchers never exit.".to_string(),
                ..Default::default()
            },
        )
        .expect("writing a brief");

    let inbox = harness.api("GET", "/inbox", None);
    assert_eq!(inbox.status, 200, "{}", inbox.text());

    let waiting = &inbox.json()["briefs_to_confirm"][0];
    assert_eq!(waiting["review_id"].as_i64(), Some(review_id));
    assert_eq!(
        waiting["pull_request"]["title"],
        common::RECORDED_TITLE,
        "the Inbox lists a review by its title, and the link is all the screen had: {}",
        inbox.text()
    );
}

#[test]
fn a_caller_that_supplies_a_title_keeps_it_and_costs_no_read() {
    // The bargain the contract offers: pass what you know and save the server a read.
    // A caller that passes a head commit gets no read at all, so it also gets no resolved
    // title — which is why the one it passed has to be kept rather than overwritten.
    let (github, calls) = common::counted(common::recorded_github());
    let harness = common::start(github);

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(
            r#"{"pr_url":"https://github.com/devergehq/locus/pull/64",
                "head_sha":"9535132","title":"A title the caller already knew",
                "changed_lines":7}"#,
        ),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["head_sha"], "9535132");
    assert_eq!(review["title"], "A title the caller already knew");
    assert_eq!(review["changed_lines"], 7);
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a caller that supplied the head commit must cost no GitHub read"
    );
}

#[test]
fn pasting_the_same_link_twice_does_not_overwrite_what_a_session_recorded() {
    // **`POST /reviews` deduplicates**, so a developer pasting the same link twice reaches
    // this handler a second time with the review already on the record.
    //
    // The read must not replace what is there. A session opens round 2 with the delta it
    // actually reviewed — `review open --changed-lines 180` — and GitHub's figure is the
    // whole pull request's additions plus deletions, 1100. `changed_lines` is what the
    // draft's word budget is computed from, so replacing one with the other is not
    // cosmetic; and the contract says a title you pass is one nothing then overwrites.
    let harness = common::start(common::recorded_github());

    let session = harness.api(
        "POST",
        "/reviews",
        Some(&format!(
            r#"{{"pr_url":"https://github.com/devergehq/locus/pull/64",
                     "head_sha":"{}","title":"The delta this round reviewed",
                     "changed_lines":180}}"#,
            common::RECORDED_HEAD_SHA
        )),
    );
    assert_eq!(session.status, 201, "{}", session.text());
    let review_id = session.json()["id"].as_i64().expect("a review id");

    // The developer pastes the link. No head commit, so GitHub is read — and the review
    // for that head commit already exists.
    let pasted = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );
    assert_eq!(pasted.status, 201, "{}", pasted.text());
    assert_eq!(
        pasted.json()["id"].as_i64(),
        Some(review_id),
        "the same head commit has to hand back the same review"
    );

    assert_eq!(
        pasted.json()["title"],
        "The delta this round reviewed",
        "a title the caller recorded is not replaced by GitHub's: {}",
        pasted.text()
    );
    assert_eq!(
        pasted.json()["changed_lines"],
        180,
        "180 is the delta this round reviewed; {} is the whole pull request: {}",
        common::RECORDED_CHANGED_LINES,
        pasted.text()
    );

    // And it is the record that says so, not only the answer.
    let stored = harness
        .store
        .find_review_by_id(review_id)
        .unwrap()
        .expect("the review is on the record");
    assert_eq!(
        stored.title.as_deref(),
        Some("The delta this round reviewed")
    );
    assert_eq!(stored.changed_lines, Some(180));
}

#[test]
fn a_resolved_fact_still_fills_a_gap_the_record_has_no_answer_for() {
    // The other half of the same rule. A review opened with no title — `review open` does
    // not require one — gains GitHub's when the developer pastes the link, because filling
    // a gap is the whole reason the read was widened.
    let harness = common::start(common::recorded_github());

    let opened = harness.api(
        "POST",
        "/reviews",
        Some(&format!(
            r#"{{"pr_url":"https://github.com/devergehq/locus/pull/64","head_sha":"{}"}}"#,
            common::RECORDED_HEAD_SHA
        )),
    );
    assert_eq!(opened.status, 201, "{}", opened.text());
    assert!(
        opened.json()["title"].is_null(),
        "no read was made, so no title"
    );

    let pasted = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );
    assert_eq!(
        pasted.json()["title"],
        common::RECORDED_TITLE,
        "{}",
        pasted.text()
    );
    assert_eq!(
        pasted.json()["changed_lines"],
        common::RECORDED_CHANGED_LINES
    );
}

#[test]
fn a_review_whose_title_cannot_be_read_is_created_with_none() {
    // A recording captured before DEV-862 widened the read carries no title, and an
    // installed `gh` could answer the same way. A review with no title is a review the
    // screen shows without one; it is not a reason to refuse to start the review, because
    // the head commit — the one fact the record cannot do without — was resolved.
    let directory = tempfile::tempdir().expect("a temporary directory");
    let narrowed = directory.path().join("pull_request.json");
    std::fs::write(
        &narrowed,
        format!(
            r#"{{"number":64,"state":"closed","merged":true,
                 "merged_at":"2026-09-30T10:54:42Z","closed_at":"2026-09-30T10:54:42Z",
                 "head":{{"sha":"{}"}},"user":{{"login":"patrickdorival"}}}}"#,
            common::RECORDED_HEAD_SHA
        ),
    )
    .expect("writing a narrowed recording");

    let harness = common::start(
        review_desk_harvest::recorded::Recorded::nothing().with("pull_request", &narrowed),
    );

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/devergehq/locus/pull/64"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["head_sha"], common::RECORDED_HEAD_SHA);
    assert!(
        review["title"].is_null(),
        "a title GitHub did not send is null, not an empty string: {}",
        answer.text()
    );
    assert!(review["changed_lines"].is_null());
}

#[test]
fn a_malformed_link_is_refused_without_reading_github() {
    // The reader behind the counter is the recorded one, so it *would* have answered. A
    // non-zero count therefore means the handler reached for GitHub before it had
    // finished checking the request — which is the test DEV-861 asks for: one that fails
    // if the GitHub trait is called.
    let (github, calls) = common::counted(common::recorded_github());
    let harness = common::start(github);

    for (link, why) in [
        (
            "https://github.com/acme/portal/pulls/412",
            "no /pull/ segment",
        ),
        ("https://github.com/acme/portal/pull/", "no number"),
        ("https://github.com/acme/portal/pull/zero", "not a number"),
        (
            "https://github.com/portal/pull/412",
            "the host would become the owner",
        ),
        ("not a url at all", "not a link"),
        ("https://github.com/acme/portal/pull/0", "number below 1"),
        (
            "ftp://github.com/acme/portal/pull/412",
            "not an http scheme",
        ),
        ("github.com/acme/portal/pull/412", "no scheme"),
        (
            "https://github.com/acme/portal/pull/412/../../evil/pull/9",
            "a path that is not the shape",
        ),
    ] {
        let body = format!(r#"{{"pr_url":{}}}"#, serde_json::to_string(link).unwrap());
        let answer = harness.api("POST", "/reviews", Some(&body));

        assert_eq!(
            answer.status,
            400,
            "{link:?} ({why}) should be refused as a bad link, not answered {}: {}",
            answer.status,
            answer.text()
        );
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "pr_url");
        assert!(
            answer.json()["expected"].is_string(),
            "invalid_value has to say what it would have accepted"
        );
    }

    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a malformed link must be refused before any GitHub read is made"
    );
}

/// The same assertion, made where the call count can actually be read.
///
/// The server owns its GitHub reader once it is built, so the count is checked by giving
/// the server a reader that has nothing recorded and asserting the refusal is about the
/// link rather than about GitHub. A read would have produced `github_unavailable`.
#[test]
fn a_malformed_link_never_reports_github_as_the_problem() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/issues/412"}"#),
    );

    assert_eq!(answer.error_code(), "invalid_value");
    assert_ne!(
        answer.error_code(),
        "github_unavailable",
        "a machine with no gh must be told its link is wrong, not that gh is missing"
    );
}

#[test]
fn github_being_unavailable_returns_the_contracts_code_and_creates_nothing() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412"}"#),
    );

    assert_eq!(answer.status, 502, "{}", answer.text());
    assert_eq!(answer.error_code(), "github_unavailable");

    // Nothing was created. A review with no resolvable head could not be deduplicated or
    // given a round, which is the reason the contract resolves the head first.
    assert!(
        harness
            .store
            .list_reviews("acme/portal", 412)
            .unwrap()
            .is_empty(),
        "a failed GitHub read must leave no review behind"
    );
}

#[test]
fn a_supplied_head_commit_needs_no_github_read() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    assert_eq!(answer.json()["head_sha"], "3f9c2ab");
}

#[test]
fn the_same_head_commit_returns_the_same_review() {
    let harness = common::start_with_no_github();
    let body = r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab"}"#;

    let first = harness.api("POST", "/reviews", Some(body));
    let again = harness.api("POST", "/reviews", Some(body));

    assert_eq!(first.status, 201);
    assert_eq!(again.status, 201);
    assert_eq!(
        first.json()["id"],
        again.json()["id"],
        "a second click of \"Understand it first\" must not open a second review"
    );
    assert_eq!(again.json()["round"], 1);
}

#[test]
fn a_bad_head_commit_is_refused() {
    let harness = common::start_with_no_github();

    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"NOTAHASH"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "invalid_value");
    assert_eq!(answer.json()["field"], "head_sha");
}

#[test]
fn the_facts_a_caller_supplies_are_stored_rather_than_dropped() {
    let harness = common::start_with_no_github();

    // Before DEV-860 the store had no column for any of these and this operation refused
    // them rather than accepting and dropping them. It has columns now, so the test is
    // the one that was always wanted: they come back.
    let answer = harness.api(
        "POST",
        "/reviews",
        Some(
            r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_sha":"3f9c2ab",
                "title":"Retry failed webhook deliveries","ticket_id":"ABC-231",
                "changed_lines":214}"#,
        ),
    );

    assert_eq!(answer.status, 201, "{}", answer.text());
    let review = answer.json();
    assert_eq!(review["title"], "Retry failed webhook deliveries");
    assert_eq!(review["ticket_id"], "ABC-231");
    assert_eq!(review["changed_lines"], 214);

    let stored = harness
        .store
        .find_review_by_id(review["id"].as_i64().unwrap())
        .unwrap()
        .expect("the review is on the record");
    assert_eq!(
        stored.title.as_deref(),
        Some("Retry failed webhook deliveries")
    );
    assert_eq!(stored.ticket_id.as_deref(), Some("ABC-231"));
    assert_eq!(stored.changed_lines, Some(214));
}

#[test]
fn a_link_on_another_host_is_refused() {
    let harness = common::start_with_no_github();

    // A review is recorded as owner/repo with no host, and the only GitHub this server
    // can read is github.com. Accepting this would open a review against a GitHub
    // repository of the same name, which is a different project.
    for link in [
        "https://gitlab.com/acme/portal/pull/412",
        "https://bitbucket.org/acme/portal/pull/412",
        "https://github.example.com/acme/portal/pull/412",
    ] {
        let body = format!(r#"{{"pr_url":{}}}"#, serde_json::to_string(link).unwrap());
        let answer = harness.api("POST", "/reviews", Some(&body));

        assert_eq!(answer.status, 400, "{link}: {}", answer.text());
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "pr_url");
    }

    assert!(
        harness
            .store
            .list_reviews("acme/portal", 412)
            .unwrap()
            .is_empty(),
        "a refused link must create nothing"
    );
}

#[test]
fn an_unknown_field_in_the_body_is_refused() {
    let harness = common::start_with_no_github();

    // The contract sets additionalProperties: false, and a misspelled `head_sha` would
    // otherwise be a GitHub read the caller thought they had saved.
    let answer = harness.api(
        "POST",
        "/reviews",
        Some(r#"{"pr_url":"https://github.com/acme/portal/pull/412","head_shaa":"3f9c2ab"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "bad_request");
}

#[test]
fn confirming_a_brief_sets_the_confirmation_and_says_so() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", seeded.review_id),
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let brief = answer.json();
    assert_eq!(brief["review_id"], seeded.review_id);
    assert_eq!(brief["confirmed"], true);
    assert!(
        brief["confirmed_at"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z')),
        "the confirmation time is RFC 3339 in UTC: {brief}"
    );

    let stored = harness.store.get_brief(seeded.review_id).unwrap();
    assert!(stored.is_confirmed(), "the store holds the confirmation");
}

#[test]
fn confirming_a_confirmed_brief_returns_it_unchanged() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let path = format!("/reviews/{}/brief/confirm", seeded.review_id);

    let first = harness.api("POST", &path, None);
    let again = harness.api("POST", &path, None);

    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(
        first.json()["confirmed_at"],
        again.json()["confirmed_at"],
        "confirming twice keeps the first confirmation time"
    );
}

#[test]
fn confirming_a_brief_that_was_never_written_is_a_conflict() {
    let harness = common::start_with_no_github();
    let opened = harness
        .store
        .open_review("acme/portal", 99, "abc1234")
        .unwrap();

    let answer = harness.api(
        "POST",
        &format!("/reviews/{}/brief/confirm", opened.review.id),
        None,
    );

    // 409 and not 404: the review exists, and the contract gives the operation both.
    assert_eq!(answer.status, 409, "{}", answer.text());
    assert_eq!(answer.error_code(), "conflict");
}

#[test]
fn a_findings_disposition_moves_with_its_reason() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/2", seeded.review_id),
        Some(r#"{"disposition":"held_back","reason":"a nit; it goes in the ticket"}"#),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    let finding = answer.json();
    assert_eq!(finding["seq"], 2);
    assert_eq!(
        finding["key"], "F2",
        "the display key is derived by the server"
    );
    assert_eq!(finding["disposition"], "held_back");
    assert_eq!(
        finding["disposition_reason"],
        "a nit; it goes in the ticket"
    );

    let stored = harness.store.get_finding(seeded.review_id, 2).unwrap();
    assert_eq!(stored.disposition, Disposition::HeldBack);
}

#[test]
fn holding_a_finding_back_with_no_reason_is_refused_and_changes_nothing() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    for body in [
        r#"{"disposition":"held_back"}"#,
        r#"{"disposition":"suppressed","reason":"   "}"#,
    ] {
        let answer = harness.api(
            "PATCH",
            &format!("/reviews/{}/findings/1", seeded.review_id),
            Some(body),
        );

        assert_eq!(answer.status, 400, "{}", answer.text());
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "reason");

        // The refusal is computed before the write, so the finding is where it was.
        let stored = harness.store.get_finding(seeded.review_id, 1).unwrap();
        assert_eq!(
            stored.disposition,
            Disposition::InDraft,
            "a refused disposition change must leave the finding alone"
        );
    }
}

#[test]
fn moving_a_finding_back_into_the_draft_needs_no_reason() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/1", seeded.review_id),
        Some(r#"{"disposition":"in_draft"}"#),
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
}

#[test]
fn an_unknown_disposition_names_the_ones_that_exist() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/1", seeded.review_id),
        Some(r#"{"disposition":"wontfix"}"#),
    );

    assert_eq!(answer.status, 400);
    assert_eq!(answer.error_code(), "invalid_value");
    let expected = answer.json()["expected"].as_str().unwrap().to_string();
    assert!(
        expected.contains("in_draft") && expected.contains("held_back"),
        "a refusal has to say what it would have accepted; it said {expected:?}"
    );
}

#[test]
fn an_unknown_finding_is_not_found() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    let answer = harness.api(
        "PATCH",
        &format!("/reviews/{}/findings/99", seeded.review_id),
        Some(r#"{"disposition":"in_draft"}"#),
    );

    assert_eq!(answer.status, 404);
    assert_eq!(answer.error_code(), "not_found");
}

#[test]
fn a_brief_needing_no_confirmation_is_not_in_the_developers_queue() {
    // **DEV-864's second part, from the API's side.** `confirmation_required: false` is
    // the one place an agent decides a developer step was unnecessary, and the Inbox's
    // "Confirm the problem" group exists to ask for exactly that act — so a brief
    // recorded as needing no confirmation must not be in it. The brief is still served on
    // the One review screen, with the two fields that let the screen say which state it
    // is in.
    let harness = common::start_with_no_github();

    let asked = harness
        .store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("a review")
        .review
        .id;
    harness
        .store
        .put_brief(
            asked,
            review_desk_store::NewBrief {
                problem: "A large change nobody has read yet.".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief requiring confirmation");

    let waived = harness
        .store
        .open_review("acme/portal", 413, "c0ffee1")
        .expect("another review")
        .review
        .id;
    harness
        .store
        .put_brief(
            waived,
            review_desk_store::NewBrief {
                problem: "A log line says null where it means not measured.".to_string(),
                confirmation_required: false,
                confirmation_not_required_because: Some(
                    "38 changed lines, under this caller's 200-line threshold".to_string(),
                ),
                ..Default::default()
            },
        )
        .expect("a brief needing no confirmation");

    let inbox = harness.api("GET", "/inbox", None);
    assert_eq!(inbox.status, 200, "{}", inbox.text());

    let queued: Vec<i64> = inbox.json()["briefs_to_confirm"]
        .as_array()
        .expect("the group")
        .iter()
        .map(|item| item["review_id"].as_i64().expect("a review id"))
        .collect();
    assert_eq!(
        queued,
        vec![asked],
        "only the brief that is actually waiting on the developer: {}",
        inbox.text()
    );

    // And the One review screen can tell the two states apart, which is what the screen
    // needs in order to show a brief that is finished with and that nobody signed.
    let shown = harness.api("GET", &format!("/reviews/{waived}"), None);
    assert_eq!(shown.status, 200, "{}", shown.text());
    let brief = &shown.json()["brief"];
    assert_eq!(brief["confirmed"], false, "nobody confirmed it");
    assert_eq!(brief["confirmation_required"], false);
    assert_eq!(
        brief["confirmation_not_required_because"],
        "38 changed lines, under this caller's 200-line threshold"
    );

    let required = harness.api("GET", &format!("/reviews/{asked}"), None);
    assert_eq!(required.json()["brief"]["confirmation_required"], true);
    assert_eq!(
        required.json()["brief"]["confirmation_not_required_because"],
        serde_json::Value::Null
    );
}
