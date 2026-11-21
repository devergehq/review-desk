//! The answer contract, rule by rule — DEV-1332. The spec is `docs/answer-contract.md`.
//!
//! **One test per rule of the field table**, and one per suggestion type's carried fields.
//! Each refusal is checked for naming the field it broke, since that is what the session is
//! told when `message reply --answer` exits 1.

mod common;

use common::{db_path, temp_store};
use review_desk_store::{
    About, Answer, AnswerAbout, AnswerKind, MessageAuthor, ReviewLayerName, ReviewTab, Store,
    StoreError, SuggestionType,
};
use serde_json::{json, Value};
use tempfile::TempDir;

/// The doc's own example, which must parse.
fn example() -> Value {
    json!({
        "kind": "answer",
        "about": { "finding": "F9" },
        "headline": "Yes, the web path has the same hole: it calls the same policy with the same empty list.",
        "points": [
            { "basis": "ran",  "text": "A web export of a definition with no columns returned all 14 columns." },
            { "basis": "read", "text": "ExportController.php:41 passes the definition straight through." }
        ],
        "cites": ["F9", "app/Http/Controllers/ExportController.php:41", "criterion 3"],
        "not_sure": "Didn't check the CSV download button, which may use a third path.",
        "suggests": [
            { "type": "regrade", "finding": "F9", "from": "should", "to": "blocker",
              "why": "Both paths leak private columns." }
        ]
    })
}

/// `example()` with one field changed (or removed, for `Value::Null`).
fn with(field: &str, value: Value) -> Value {
    let mut answer = example();
    let object = answer.as_object_mut().unwrap();
    if value.is_null() {
        object.remove(field);
    } else {
        object.insert(field.to_string(), value);
    }
    answer
}

/// The field a refusal names, or a panic saying it was not refused.
fn refused(body: &Value) -> String {
    refused_text(&body.to_string())
}

fn refused_text(body: &str) -> String {
    match Answer::parse(body) {
        Err(StoreError::NotTheContract { field, .. }) => field,
        other => panic!("expected a contract refusal for {body}, got {other:?}"),
    }
}

fn parses(body: &Value) -> Answer {
    Answer::parse(&body.to_string()).unwrap_or_else(|error| panic!("{body}: {error}"))
}

fn long(n: usize) -> String {
    "x".repeat(n)
}

// -- the body ------------------------------------------------------------------------

#[test]
fn the_doc_s_example_parses() {
    let answer = parses(&example());
    assert_eq!(answer.kind, AnswerKind::Answer);
    assert_eq!(answer.cites.len(), 3);
    assert_eq!(
        answer.about,
        Some(AnswerAbout::Finding(review_desk_store::FindingNumber(9)))
    );
}

#[test]
fn a_body_that_is_not_json_is_refused() {
    assert_eq!(refused_text("Because nothing leaks."), "body");
}

#[test]
fn a_body_that_is_not_one_object_is_refused() {
    assert_eq!(refused(&json!([example()])), "body");
}

#[test]
fn a_field_the_contract_does_not_name_is_refused() {
    assert_eq!(refused(&with("severity", json!("blocker"))), "severity");
}

// -- kind ----------------------------------------------------------------------------

#[test]
fn kind_is_required() {
    assert_eq!(refused(&with("kind", Value::Null)), "kind");
}

#[test]
fn kind_is_answer_working_or_cant() {
    assert_eq!(refused(&with("kind", json!("approve"))), "kind");
}

// -- headline ------------------------------------------------------------------------

#[test]
fn headline_is_required() {
    assert_eq!(refused(&with("headline", Value::Null)), "headline");
    assert_eq!(refused(&with("headline", json!("   "))), "headline");
}

#[test]
fn headline_is_at_most_140_characters() {
    parses(&with("headline", json!(long(140))));
    assert_eq!(refused(&with("headline", json!(long(141)))), "headline");
}

#[test]
fn headline_counts_characters_not_bytes() {
    parses(&with("headline", json!("é".repeat(140))));
}

#[test]
fn headline_is_one_line() {
    assert_eq!(
        refused(&with("headline", json!("Yes.\nAnd another thing."))),
        "headline"
    );
}

// -- points --------------------------------------------------------------------------

#[test]
fn points_are_at_most_five() {
    let point = json!({ "basis": "read", "text": "A point." });
    parses(&with("points", json!(vec![point.clone(); 5])));
    assert_eq!(refused(&with("points", json!(vec![point; 6]))), "points");
}

#[test]
fn a_point_is_at_most_200_characters() {
    parses(&with(
        "points",
        json!([{ "basis": "ran", "text": long(200) }]),
    ));
    assert_eq!(
        refused(&with(
            "points",
            json!([{ "basis": "ran", "text": long(201) }])
        )),
        "points[0].text"
    );
}

#[test]
fn every_point_carries_a_basis() {
    assert_eq!(
        refused(&with("points", json!([{ "text": "A point." }]))),
        "points[0].basis"
    );
}

#[test]
fn a_basis_is_ran_read_or_told() {
    assert_eq!(
        refused(&with(
            "points",
            json!([{ "basis": "guessed", "text": "A point." }])
        )),
        "points[0].basis"
    );
    for basis in ["ran", "read", "told"] {
        parses(&with(
            "points",
            json!([{ "basis": basis, "text": "A point." }]),
        ));
    }
}

#[test]
fn a_point_has_only_a_basis_and_text() {
    assert_eq!(
        refused(&with(
            "points",
            json!([{ "basis": "ran", "text": "A point.", "cite": "F9" }])
        )),
        "points[0].cite"
    );
}

// -- cites ---------------------------------------------------------------------------

#[test]
fn an_answer_cites_at_least_one_thing() {
    assert_eq!(refused(&with("cites", Value::Null)), "cites");
    assert_eq!(refused(&with("cites", json!([]))), "cites");
}

#[test]
fn every_cite_is_a_string() {
    assert_eq!(refused(&with("cites", json!(["F9", 41]))), "cites[1]");
    assert_eq!(refused(&with("cites", json!("F9"))), "cites");
}

#[test]
fn a_cite_is_never_resolved_against_the_record() {
    // A SHA, a criterion and a finding that no record holds all pass: cites are checked for
    // being there, not for naming something.
    parses(&with(
        "cites",
        json!(["c0ffee1", "criterion 3", "F9999", "option C"]),
    ));
}

#[test]
fn a_reply_that_claims_nothing_needs_no_cite() {
    parses(
        &json!({ "kind": "cant", "headline": "No access to the staging database, so I can't run it." }),
    );
    parses(&json!({
        "kind": "working",
        "headline": "Checking the CSV path.",
        "activity": ["Reading ExportController.php"]
    }));
}

#[test]
fn a_cant_that_makes_a_point_must_cite() {
    let cant = json!({
        "kind": "cant",
        "headline": "I can't tell without the logs.",
        "points": [{ "basis": "read", "text": "The job logs nothing on a skip." }]
    });
    assert_eq!(refused(&cant), "cites");
}

// -- not_sure ------------------------------------------------------------------------

#[test]
fn not_sure_is_at_most_200_characters() {
    parses(&with("not_sure", json!(long(200))));
    assert_eq!(refused(&with("not_sure", json!(long(201)))), "not_sure");
}

#[test]
fn not_sure_is_one_string_not_a_list() {
    assert_eq!(refused(&with("not_sure", json!(["a", "b"]))), "not_sure");
}

// -- suggests ------------------------------------------------------------------------

fn suggesting(suggestion: Value) -> Value {
    with("suggests", json!([suggestion]))
}

#[test]
fn suggests_are_at_most_three() {
    let one = json!({ "type": "follow_up", "question": "Does the CSV button use it too?" });
    parses(&with("suggests", json!(vec![one.clone(); 3])));
    assert_eq!(refused(&with("suggests", json!(vec![one; 4]))), "suggests");
}

#[test]
fn a_suggestion_of_any_other_type_makes_the_whole_answer_not_parse() {
    for other in ["approve", "sign_off", "post", "merge", "NEW_FINDING"] {
        assert_eq!(
            refused(&suggesting(json!({ "type": other }))),
            "suggests[0].type",
            "{other}"
        );
    }
    assert_eq!(
        refused(&suggesting(json!({ "finding": "F9" }))),
        "suggests[0].type"
    );
}

#[test]
fn the_closed_list_is_seven_and_each_parses() {
    let every = [
        json!({ "type": "new_finding", "severity": "should", "title": "CSV export leaks columns",
                "where": "app/Http/Controllers/CsvController.php:12", "basis": "read" }),
        json!({ "type": "regrade", "finding": "F9", "from": "should", "to": "blocker", "why": "It leaks." }),
        json!({ "type": "add_to_draft", "finding": "F9" }),
        json!({ "type": "set_aside", "finding": "F4", "why": "The author fixed it in 3a1b." }),
        json!({ "type": "answer_question", "question": 3, "answer": "Yes, both paths are covered." }),
        json!({ "type": "draft_reply", "finding": "F3", "call": "raise_again", "words": "This still skips tenants." }),
        json!({ "type": "follow_up", "question": "Should the CSV path be checked too?" }),
    ];
    assert_eq!(SuggestionType::ALL.len(), every.len());
    for (suggestion, kind) in every.iter().zip(SuggestionType::ALL) {
        let answer = parses(&suggesting(suggestion.clone()));
        assert_eq!(answer.suggests[0].suggestion_type(), *kind);
        // And it reads back the way it was written.
        let stored: Answer = serde_json::from_str(&answer.to_json()).unwrap();
        assert_eq!(stored, answer);
    }
}

#[test]
fn a_suggestion_carries_no_field_its_type_does_not() {
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "set_aside", "finding": "F4", "why": "Fixed.", "to": "nit" })
        )),
        "suggests[0].to"
    );
}

#[test]
fn new_finding_carries_severity_title_where_and_basis() {
    let full = json!({ "type": "new_finding", "severity": "nit", "title": "Typo in the log line",
                       "where": "app/Jobs/BillingJob.php:9", "basis": "read" });
    parses(&suggesting(full.clone()));
    for (field, bad) in [
        ("severity", json!("critical")),
        ("title", json!(long(101))),
        ("where", Value::Null),
        ("basis", json!("felt")),
    ] {
        let mut one = full.clone();
        if bad.is_null() {
            one.as_object_mut().unwrap().remove(field);
        } else {
            one[field] = bad;
        }
        assert_eq!(refused(&suggesting(one)), format!("suggests[0].{field}"));
    }
}

#[test]
fn regrade_carries_a_finding_from_to_and_why() {
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "regrade", "finding": "9", "from": "should", "to": "blocker", "why": "x" })
        )),
        "suggests[0].finding"
    );
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "regrade", "finding": "F9", "to": "blocker", "why": "x" })
        )),
        "suggests[0].from"
    );
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "regrade", "finding": "F9", "from": "should", "to": "blocker" })
        )),
        "suggests[0].why"
    );
}

#[test]
fn a_regrade_changes_the_severity() {
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "regrade", "finding": "F9", "from": "should", "to": "should", "why": "x" })
        )),
        "suggests[0].to"
    );
}

#[test]
fn add_to_draft_carries_a_finding_or_one_sentence() {
    parses(&suggesting(
        json!({ "type": "add_to_draft", "text": "Both export paths share the policy." }),
    ));
    parses(&suggesting(
        json!({ "type": "add_to_draft", "finding": "F9", "text": "Say it leaks on web too." }),
    ));
    assert_eq!(
        refused(&suggesting(json!({ "type": "add_to_draft" }))),
        "suggests[0]"
    );
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "add_to_draft", "text": "One.\nTwo." })
        )),
        "suggests[0].text"
    );
}

#[test]
fn set_aside_carries_a_finding_and_why() {
    assert_eq!(
        refused(&suggesting(json!({ "type": "set_aside", "finding": "F4" }))),
        "suggests[0].why"
    );
}

#[test]
fn answer_question_names_one_of_the_five_and_answers_it() {
    for bad in [json!(0), json!(6), json!("3")] {
        assert_eq!(
            refused(&suggesting(
                json!({ "type": "answer_question", "question": bad, "answer": "Yes." })
            )),
            "suggests[0].question"
        );
    }
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "answer_question", "question": 2 })
        )),
        "suggests[0].answer"
    );
}

#[test]
fn draft_reply_carries_a_finding_a_call_and_the_words() {
    for call in ["raise_again", "accept_reason", "withdraw"] {
        parses(&suggesting(
            json!({ "type": "draft_reply", "finding": "F3", "call": call, "words": "Fine." }),
        ));
    }
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "draft_reply", "finding": "F3", "call": "R", "words": "Fine." })
        )),
        "suggests[0].call"
    );
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "draft_reply", "finding": "F3", "call": "withdraw", "words": long(601) })
        )),
        "suggests[0].words"
    );
}

#[test]
fn follow_up_carries_one_question() {
    assert_eq!(
        refused(&suggesting(
            json!({ "type": "follow_up", "question": long(201) })
        )),
        "suggests[0].question"
    );
}

// -- activity ------------------------------------------------------------------------

fn working(activity: Value) -> Value {
    json!({ "kind": "working", "headline": "Checking the CSV path.", "activity": activity })
}

#[test]
fn a_working_reply_says_what_it_is_doing() {
    assert_eq!(
        refused(&json!({ "kind": "working", "headline": "Checking." })),
        "activity"
    );
}

#[test]
fn activity_is_at_most_six_steps() {
    parses(&working(json!(vec!["Reading a file"; 6])));
    assert_eq!(
        refused(&working(json!(vec!["Reading a file"; 7]))),
        "activity"
    );
}

#[test]
fn an_activity_step_is_at_most_60_characters() {
    parses(&working(json!([long(60)])));
    assert_eq!(refused(&working(json!([long(61)]))), "activity[0]");
}

#[test]
fn activity_is_only_for_a_working_reply() {
    assert_eq!(refused(&with("activity", json!(["Reading"]))), "activity");
}

// -- about ---------------------------------------------------------------------------

#[test]
fn about_is_a_step_a_finding_or_a_reply() {
    let place = |about: Value| parses(&with("about", about)).about.unwrap().place();
    assert_eq!(
        place(json!({ "step": "draft" })),
        About {
            tab: ReviewTab::Draft,
            finding: None
        }
    );
    // An old step name is kept as the tab it now lives on.
    assert_eq!(
        place(json!({ "step": "code_review" })),
        About {
            tab: ReviewTab::Findings,
            finding: None
        }
    );
    assert_eq!(
        place(json!({ "finding": "F9" })),
        About {
            tab: ReviewTab::Findings,
            finding: Some(9)
        }
    );
    assert_eq!(
        place(json!({ "reply": "F3" })),
        About {
            tab: ReviewTab::Replies,
            finding: Some(3)
        }
    );
}

#[test]
fn about_is_about_one_thing() {
    assert_eq!(
        refused(&with("about", json!({ "finding": "F9", "step": "draft" }))),
        "about"
    );
    assert_eq!(
        refused(&with("about", json!({ "tab": "draft" }))),
        "about.tab"
    );
    assert_eq!(
        refused(&with("about", json!({ "step": "posting" }))),
        "about.step"
    );
    assert_eq!(
        refused(&with("about", json!({ "finding": "F0" }))),
        "about.finding"
    );
}

// -- asked ---------------------------------------------------------------------------

#[test]
fn asked_is_at_most_200_characters() {
    parses(&with("asked", json!(long(200))));
    assert_eq!(refused(&with("asked", json!(long(201)))), "asked");
}

// -- kept, or not --------------------------------------------------------------------

fn review(store: &Store) -> i64 {
    store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("opening a review")
        .review
        .id
}

#[test]
fn a_parsed_answer_is_kept_beside_its_raw_body() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let asked = store
        .ask_session(id, ReviewLayerName::CodeReview, "Same hole on web?")
        .unwrap();
    let body = example().to_string();
    let kept = store
        .answer_from_session(id, ReviewLayerName::CodeReview, &body, Some(asked.seq))
        .unwrap();
    assert!(kept.parsed());
    assert_eq!(kept.body, body, "the raw text is kept as sent");
    assert_eq!(kept.answer, Some(parses(&example())));
    assert_eq!(
        kept.about,
        About {
            tab: ReviewTab::Findings,
            finding: Some(9)
        },
        "the answer's own about places it"
    );
    assert_eq!(
        store.parsed_answer(id, kept.seq).unwrap(),
        parses(&example())
    );
}

#[test]
fn a_refused_answer_keeps_nothing() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let error = store
        .answer_from_session(id, ReviewLayerName::Draft, "Because.", None)
        .unwrap_err();
    assert_eq!(error.code(), "not_the_contract");
    assert!(error.is_refusal());
    assert!(store.messages(id).unwrap().is_empty());
}

#[test]
fn a_prose_reply_is_kept_only_as_a_raw_message() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let kept = store
        .reply_from_session(id, ReviewLayerName::Draft, "Because nothing leaks.", None)
        .unwrap();
    assert!(!kept.parsed());
    assert_eq!(kept.answer, None);
    // It never reaches a finding or a draft: the one door to an answer refuses it.
    assert!(matches!(
        store.parsed_answer(id, kept.seq),
        Err(StoreError::InvalidValue { field: "seq", .. })
    ));
    // Not even a body that happens to be the contract: `--body` is prose.
    let json_as_prose = store
        .reply_from_session(id, ReviewLayerName::Draft, &example().to_string(), None)
        .unwrap();
    assert!(!json_as_prose.parsed());
}

#[test]
fn asked_stays_inside_the_answer_and_writes_no_developer_message() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let body = with("asked", json!("Does the web path have the same hole?")).to_string();
    let kept = store
        .answer_from_session(id, ReviewLayerName::CodeReview, &body, None)
        .unwrap();
    assert_eq!(
        kept.answer.unwrap().asked.as_deref(),
        Some("Does the web path have the same hole?")
    );
    let messages = store.messages(id).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages.iter().all(|m| m.author == MessageAuthor::Session));
}

#[test]
fn a_working_reply_names_the_question_it_is_working_on() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let body = working(json!(["Reading the CSV controller"])).to_string();
    assert!(matches!(
        store.answer_from_session(id, ReviewLayerName::Draft, &body, None),
        Err(StoreError::NotTheContract { field, .. }) if field == "kind"
    ));
}

#[test]
fn working_is_superseded_by_the_next_parsed_reply_and_cant_only_by_an_answer() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let asked = store
        .ask_session(id, ReviewLayerName::CodeReview, "Same hole on web?")
        .unwrap()
        .seq;
    let other = store
        .ask_session(id, ReviewLayerName::CodeReview, "And the CSV button?")
        .unwrap()
        .seq;
    let step = ReviewLayerName::CodeReview;
    let w = |to| {
        store
            .answer_from_session(id, step, &working(json!(["Reading"])).to_string(), Some(to))
            .unwrap()
            .seq
    };
    let first_working = w(asked);
    // A prose reply supersedes nothing: it is never kept with the review as an answer.
    store
        .reply_from_session(id, step, "still on it", Some(asked))
        .unwrap();
    let cant = store
        .answer_from_session(
            id,
            step,
            &json!({ "kind": "cant", "headline": "No staging access." }).to_string(),
            Some(asked),
        )
        .unwrap()
        .seq;
    let working_on_other = w(other);
    let answer = store
        .answer_from_session(id, step, &example().to_string(), Some(asked))
        .unwrap()
        .seq;

    let by = |seq: i64| {
        store
            .messages(id)
            .unwrap()
            .into_iter()
            .find(|m| m.seq == seq)
            .unwrap()
            .superseded_by
    };
    assert_eq!(
        by(first_working),
        Some(cant),
        "the next parsed reply, of any kind"
    );
    assert_eq!(by(cant), Some(answer), "a cant, by the answer");
    assert_eq!(by(answer), None, "an answer stands");
    assert_eq!(
        by(working_on_other),
        None,
        "an answer to another question changes nothing"
    );
    assert_eq!(
        store.message(id, first_working).unwrap().superseded_by,
        Some(cant)
    );
}

// -- about on an ask -----------------------------------------------------------------

#[test]
fn an_ask_is_about_its_step_s_tab_unless_the_page_says_otherwise() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let plain = store
        .ask_session(id, ReviewLayerName::Alternatives, "Why option C?")
        .unwrap();
    assert_eq!(
        plain.about,
        About {
            tab: ReviewTab::Understanding,
            finding: None
        }
    );

    let about_f9 = About {
        tab: ReviewTab::Findings,
        finding: Some(9),
    };
    let pointed = store
        .ask_session_about(id, ReviewLayerName::CodeReview, Some(about_f9), "Web too?")
        .unwrap();
    assert_eq!(pointed.about, about_f9);
}

#[test]
fn an_ask_about_a_finding_is_on_findings_or_replies() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let wrong = About {
        tab: ReviewTab::Overview,
        finding: Some(9),
    };
    assert!(matches!(
        store.ask_session_about(id, ReviewLayerName::Draft, Some(wrong), "Hm?"),
        Err(StoreError::InvalidValue { field: "about", .. })
    ));
}

// -- migration 26 --------------------------------------------------------------------

#[test]
fn messages_from_before_the_contract_read_as_not_parsed_on_their_step_s_tab() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    {
        let old = Store::open_at_version(&path, 25).expect("open at 25");
        assert_eq!(old.schema_version().unwrap(), 25);
        // Written as a released build wrote them: the store's own writer is today's.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO reviews (id, repo, pr, head_sha, round, created_at)
                 VALUES (1, 'acme/portal', 514, 'c0ffee1', 1, 1000);",
        )
        .unwrap();
        for (i, step) in ReviewLayerName::ALL.iter().enumerate() {
            conn.execute(
                "INSERT INTO review_messages (review_id, seq, step, author, kind, body, created_at)
                 VALUES (1, ?1, ?2, 'developer', 'ask', 'asked before the contract', 1000)",
                rusqlite::params![i as i64 + 1, step.as_str()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO review_messages (review_id, seq, step, author, kind, body, created_at)
             VALUES (1, 8, 'draft', 'session', 'reply', '{\"kind\":\"answer\"}', 1000)",
            [],
        )
        .unwrap();
    }
    let store = Store::open(&path).expect("reopen migrates");
    let messages = store.messages(1).unwrap();
    assert_eq!(messages.len(), ReviewLayerName::ALL.len() + 1);
    for message in messages {
        // Not even an old reply whose prose happens to be JSON.
        assert!(!message.parsed(), "nothing before the contract parsed");
        assert_eq!(
            message.about,
            About::for_step(message.step),
            "the backfill and ReviewTab::for_step are the same table"
        );
    }
}

#[test]
fn the_record_refuses_a_parsed_flag_without_an_answer() {
    let (dir, store) = temp_store();
    let id = review(&store);
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .unwrap();
    let conn = rusqlite::Connection::open(db_path(&dir)).unwrap();
    for broken in [
        "UPDATE review_messages SET parsed = 1",
        "UPDATE review_messages SET answer = '{}', parsed = 1",
        "UPDATE review_messages SET tab = 'posting'",
        "UPDATE review_messages SET about_finding = 3",
    ] {
        assert!(conn.execute_batch(broken).is_err(), "{broken} was allowed");
    }
}

#[test]
fn asked_is_refused_on_a_reply_that_names_the_developer_s_message() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let asked = store
        .ask_session(id, ReviewLayerName::CodeReview, "Same hole on web?")
        .unwrap()
        .seq;
    let body = with("asked", json!("Does the web path leak too?")).to_string();
    assert!(matches!(
        store.answer_from_session(id, ReviewLayerName::CodeReview, &body, Some(asked)),
        Err(StoreError::NotTheContract { field, .. }) if field == "asked"
    ));
    assert_eq!(store.messages(id).unwrap().len(), 1, "nothing was kept");
}

#[test]
fn every_pairing_of_replies_supersedes_as_the_doc_says() {
    let working_body = working(json!(["Reading"])).to_string();
    let cant_body = json!({ "kind": "cant", "headline": "No staging access." }).to_string();
    let answer_body = example().to_string();
    // (earlier, later, does the later supersede the earlier)
    let cases = [
        (&working_body, &working_body, true),
        (&working_body, &answer_body, true),
        (&working_body, &cant_body, true),
        (&cant_body, &answer_body, true),
        (&cant_body, &cant_body, false),
        (&cant_body, &working_body, false),
        (&answer_body, &answer_body, false),
        (&answer_body, &working_body, false),
    ];
    for (earlier, later, supersedes) in cases {
        let (_dir, store) = temp_store();
        let id = review(&store);
        let step = ReviewLayerName::CodeReview;
        let asked = store.ask_session(id, step, "Web too?").unwrap().seq;
        let first = store
            .answer_from_session(id, step, earlier, Some(asked))
            .unwrap()
            .seq;
        let second = store
            .answer_from_session(id, step, later, Some(asked))
            .unwrap()
            .seq;
        assert_eq!(
            store.message(id, first).unwrap().superseded_by,
            supersedes.then_some(second),
            "{earlier} then {later}"
        );
    }
}

#[test]
fn a_reply_without_to_supersedes_nothing() {
    let (_dir, store) = temp_store();
    let id = review(&store);
    let step = ReviewLayerName::CodeReview;
    let asked = store.ask_session(id, step, "Web too?").unwrap().seq;
    let working_seq = store
        .answer_from_session(
            id,
            step,
            &working(json!(["Reading"])).to_string(),
            Some(asked),
        )
        .unwrap()
        .seq;
    store
        .answer_from_session(id, step, &example().to_string(), None)
        .unwrap();
    assert_eq!(store.message(id, working_seq).unwrap().superseded_by, None);
}
