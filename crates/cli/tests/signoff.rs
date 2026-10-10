//! Only the developer signs off, and only the developer decides.
//!
//! Decided by Patrick on 9 October 2026: confirming a brief and approving a draft
//! happen in the dashboard. The command line that agent sessions call has no command
//! for either, and this file is what keeps that true — the rest of the codebase could
//! drift back towards it one convenience at a time, and none of the other tests would
//! notice.
//!
//! **DEV-877 widened it from two functions to four.** Keeping, cutting, regrading and
//! rewriting a finding, the three choices on a held-back nit, the four replies to a
//! carried finding, and "ticket what is left, and approve" are all the developer's, made
//! through the dashboard's API and recorded as the developer. `decide_finding` and
//! `ticket_rest_and_approve` are the store functions behind them, and they are guarded by
//! name here exactly as the first two are.
//!
//! What the command line still may do is *write the draft* — including the blocks it is
//! composed of, with `draft put --sections` — and *record an outcome* with
//! `outcome add`. Neither is a decision: writing a draft cannot approve it, and an outcome
//! annotates a finding without moving it out of the draft, changing its severity or
//! recomposing the body. The tests below hold both of those lines.
//!
//! **DEV-882 added twelve rows to the brief and one read of it, and neither is a
//! decision.** A brief with a headline, two flows, numbered problems, a supporting table,
//! option verdicts, a blind pass, solution parts, open choices and disagreements on it is
//! still a brief the developer has not confirmed; and `brief problem-statement` is a read,
//! which cannot confirm anything by definition and is held to that here anyway. The store
//! function behind it, `problem_statement`, is deliberately **not** in `SIGN_OFF`: adding a
//! read to a list of things the command line may not call would stop an agent session
//! reading its own record.

mod common;

use std::path::{Path, PathBuf};

use common::{open_review, Desk};
use review_desk_store::{
    Actor, Decision, Disposition, DraftStatus, FindingDecision, NewDraft, NewDraftSection,
    OutcomeKind, ReviewEvent, RoundStatus, Severity,
};

/// The store functions only the developer may reach.
///
/// The first two sign something off; the second two are DEV-877's decisions, which are
/// sign-offs in everything but name — `ticket_rest_and_approve` approves a draft, and
/// `decide_finding` changes what gets posted under the developer's name.
const SIGN_OFF: [&str; 4] = [
    "confirm_brief",
    "approve_draft",
    "decide_finding",
    "ticket_rest_and_approve",
];

/// The crates whose source may not name them.
fn guarded_source_directories() -> [PathBuf; 2] {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf();
    [
        crates.join("cli").join("src"),
        crates.join("harvest").join("src"),
    ]
}

/// Every `.rs` file under a directory.
fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));

    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}

#[test]
fn no_source_file_in_the_cli_or_the_harvester_names_the_sign_off_functions() {
    // Comment lines are allowed to discuss the boundary — this file and
    // `commands/draft.rs` both do, and a rule nobody may explain is a rule nobody
    // will keep. Anything that is not a whole-line comment is code.
    let is_comment = |line: &str| {
        let line = line.trim_start();
        line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
    };

    let mut offences = Vec::new();
    let mut scanned = 0;

    for directory in guarded_source_directories() {
        let files = rust_files(&directory);
        assert!(
            !files.is_empty(),
            "{} has no Rust files, so this test is checking nothing",
            directory.display()
        );

        for file in files {
            scanned += 1;
            let source = std::fs::read_to_string(&file).expect("read the source");
            for (number, line) in source.lines().enumerate() {
                if is_comment(line) {
                    continue;
                }
                for function in SIGN_OFF {
                    if line.contains(function) {
                        offences.push(format!(
                            "{}:{} calls {function}: {}",
                            file.display(),
                            number + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        scanned >= 10,
        "only {scanned} files scanned; expected the two crates"
    );
    assert!(
        offences.is_empty(),
        "only the developer signs off, so no code in crates/cli or crates/harvest may \
         call the store's sign-off functions. Confirming a brief and approving a draft \
         happen in the dashboard.\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_sign_off_functions_are_still_there_to_be_called() {
    // Without this, the test above passes the day somebody renames them, and the
    // boundary it guards stops existing quietly.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");
    let store = desk.store();

    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(r#"{"problem":"a problem worth confirming"}"#)
        .assert()
        .success();

    let confirmed = store
        .confirm_brief(id, Actor::Developer)
        .expect("the dashboard confirms the brief through this function");
    assert!(confirmed.confirmed_at.is_some());
    assert_eq!(confirmed.confirmed_by, Some(Actor::Developer));

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            r#"{"lens":"correctness","severity":"should","claim":"a finding to decide about"}"#,
        )
        .assert()
        .success();

    store
        .put_draft(
            id,
            NewDraft::composed(vec![NewDraftSection::finding(
                1,
                "a finding to decide about",
                "and a paragraph about it",
            )]),
        )
        .unwrap();

    let made = store
        .decide_finding(id, 1, &Decision::of(FindingDecision::Cut), Actor::Developer)
        .expect("the dashboard cuts a finding through this function");
    assert_eq!(made.outcome.kind, OutcomeKind::Cut);
    assert!(!made.draft.body.contains("F1"));

    store
        .decide_finding(
            id,
            1,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("and keeps it again");
    let settled = store
        .ticket_rest_and_approve(id, None, Actor::Developer)
        .expect("the dashboard tickets what is left and approves through this function");
    assert_eq!(settled.draft.status, DraftStatus::Approved);
    assert_eq!(settled.draft.event, Some(ReviewEvent::Approve));

    let approved = store.get_draft(id).expect("the draft");
    assert_eq!(approved.status, DraftStatus::Approved);
    assert_eq!(approved.approved_by, Some(Actor::Developer));
}

#[test]
fn a_brief_with_every_row_on_it_is_still_the_developers_to_confirm() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");
    let store = desk.store();

    // Every row the three approved boards draw, written from the command line.
    desk.command()
        .args(["brief", "put", "--review", &review, "--file", "-"])
        .write_stdin(
            r#"{"problem": "a problem worth confirming",
                "headline": "the answer, in one sentence",
                "how_it_works_today": "how it works today, in plain words",
                "flow_before": [[{"title": "a before box", "problem": 1}]],
                "flow_after": [[{"title": "an after box", "problem": 1}]],
                "problems": [{"number": 1, "was_wrong": "what was wrong",
                              "now_fixed": "what it is now",
                              "detail_title": "the detail",
                              "detail_before": ["a line"], "detail_after": ["a line"]}],
                "support_table": {"title": "the idea", "columns": ["q"], "rows": [["a"]]},
                "options": [{"key": "A", "title": "an option", "argument_for": "for",
                             "argument_against": "against", "chosen": true,
                             "proposed_by": "both",
                             "verdicts": [{"problem": 1, "verdict": "fixed",
                                           "why": "it does"}]}],
                "blind_pass": {"given": "the problem, as handed over", "pick_key": "A",
                               "pick_why": "it answers it", "looked_up": "nothing",
                               "questions": ["a question"]},
                "parts": [{"title": "a part", "summary": "one line", "fixes": [1],
                           "files": [{"path": "a.php", "lines_added": 1}]}],
                "open_choices": [{"left_open": "something", "chosen": "something else",
                                  "why": "because"}]}"#,
        )
        .assert()
        .success();

    // **Nothing about writing it confirms it**, and that is the whole of the rule twelve
    // new rows could have quietly broken: a brief this complete looks finished.
    let written = store.get_brief(id).expect("the brief");
    assert!(
        !written.is_confirmed(),
        "writing a brief does not confirm it"
    );
    assert!(!written.is_settled());
    assert_eq!(written.confirmed_by, None);

    // Reading the problem half does not either. It is a read, and it is the read a session
    // hands to a blind pass — so it runs on exactly the briefs nobody has confirmed yet.
    desk.run(&["brief", "problem-statement", "--review", &review]);
    let after_reading = store.get_brief(id).expect("the brief");
    assert!(!after_reading.is_confirmed());
    assert_eq!(
        after_reading.updated_at, written.updated_at,
        "a read writes nothing"
    );

    // And there is still exactly one way to confirm it, which is the dashboard's.
    let refusal = desk.refused_json(&["brief", "confirm", "--review", &review]);
    assert!(
        refusal["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("dashboard")),
        "the refusal says where confirmation happens: {refusal}"
    );

    let confirmed = store
        .confirm_brief(id, Actor::Developer)
        .expect("the dashboard confirms it");
    assert_eq!(confirmed.confirmed_by, Some(Actor::Developer));
}

#[test]
fn nothing_on_the_command_line_can_decide_about_a_finding() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            r#"{"lens":"correctness","severity":"blocker","claim":"a finding in the draft"}"#,
        )
        .assert()
        .success();

    let store = desk.store();
    store
        .put_draft(
            id,
            NewDraft::composed(vec![NewDraftSection::finding(
                1,
                "a finding in the draft",
                "and a paragraph about it",
            )]),
        )
        .expect("a draft written in parts, which a session may do");

    // Every spelling of "decide this" a caller might reach for. Each must leave the
    // finding in the draft, its severity as it was, and the body still raising it.
    let attempts: [&[&str]; 8] = [
        &["finding", "decide", "--review", &review, "--finding", "1"],
        &["finding", "cut", "--review", &review, "--finding", "1"],
        &["finding", "keep", "--review", &review, "--finding", "1"],
        &["finding", "regrade", "--review", &review, "--finding", "1"],
        &["finding", "rewrite", "--review", &review, "--finding", "1"],
        &["draft", "decide", "--review", &review],
        &["draft", "ticket-and-approve", "--review", &review],
        &["draft", "approve", "--review", &review],
    ];

    for attempt in attempts {
        let code = desk
            .command()
            .args(attempt)
            .assert()
            .get_output()
            .status
            .code();
        // Exit 1 is Review Desk refusing and 2 is the argument parser refusing; which of
        // the two depends on whether the words happen to name a command that exists.
        // Neither is Review Desk being broken, and neither is success.
        assert!(
            code == Some(1) || code == Some(2),
            "{attempt:?} must be refused, not answered; it exited {code:?}"
        );
    }

    let finding = store.get_finding(id, 1).expect("the finding");
    assert_eq!(finding.disposition, Disposition::InDraft);
    assert_eq!(finding.severity, Severity::Blocker);
    assert_eq!(finding.round_status, RoundStatus::New);
    assert!(
        store
            .get_draft(id)
            .expect("the draft")
            .body
            .contains("### Blocker · F1 · a finding in the draft"),
        "and the body still raises it"
    );
    assert!(
        store.list_outcomes(id).expect("the outcomes").is_empty(),
        "and nothing was recorded as a decision"
    );
}

#[test]
fn the_command_line_may_write_the_draft_in_parts_without_deciding_anything() {
    // **The line this file is really about.** A session has to be able to write the blocks
    // the body is composed of — that is what makes the developer's decisions possible at
    // all — and writing them must not be able to decide one. So the draft it writes is
    // `ready`, every block posts, and no outcome exists.
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");

    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(r#"{"lens":"correctness","severity":"should","claim":"a finding"}"#)
        .assert()
        .success();

    let sections = desk.write_file(
        "sections.json",
        r#"[{"kind":"prose","text":"The approach fits the ticket."},
            {"kind":"finding","finding_seq":1,"title":"a finding","text":"a paragraph."}]"#,
    );

    let written = desk.json(&[
        "draft",
        "put",
        "--review",
        &review,
        "--sections",
        sections.to_str().expect("a path"),
    ]);

    assert_eq!(written["draft"]["status"], "ready");
    assert_eq!(
        written["draft"]["body"],
        "The approach fits the ticket.\n\n### Should · F1 · a finding\n\na paragraph.\n"
    );
    assert_eq!(written["sections"].as_array().expect("the blocks").len(), 2);

    let store = desk.store();
    assert!(
        store.list_outcomes(id).expect("the outcomes").is_empty(),
        "writing a draft is not deciding about one"
    );
    assert_eq!(
        store.get_draft(id).expect("the draft").status,
        DraftStatus::Ready,
        "and it cannot approve it either"
    );
}

#[test]
fn brief_confirm_exits_one_and_names_the_dashboard() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let output = desk
        .command()
        .args(["brief", "confirm", "--review", &review])
        .assert()
        .code(1);

    let message = String::from_utf8(output.get_output().stderr.clone()).expect("text");
    assert!(
        message.contains("dashboard"),
        "the refusal has to say where confirmation went: {message}"
    );
    assert!(
        message.contains("removed"),
        "and that the command is gone rather than broken: {message}"
    );
}

#[test]
fn brief_confirm_refused_with_json_is_still_one_document() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    let document = desk.refused_json(&["brief", "confirm", "--review", &review]);

    assert_eq!(document["error"]["code"], "bad_input");
    assert_eq!(document["error"]["kind"], "refused");
    assert!(document["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("dashboard"));
}

#[test]
fn nothing_on_the_command_line_can_approve_a_draft() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");

    desk.command()
        .args(["draft", "put", "--review", &review, "--file", "-"])
        .write_stdin("the body a session wrote")
        .assert()
        .success();

    // Every spelling of "approve this" a caller might reach for. Each must fail, and
    // the draft must still be waiting on the developer afterwards.
    let attempts: [&[&str]; 6] = [
        &["draft", "approve", "--review", &review],
        &["draft", "put", "--review", &review, "--status", "approved"],
        &["draft", "set", "--review", &review, "--status", "approved"],
        &[
            "draft",
            "posted",
            "--review",
            &review,
            "--github-review",
            "gh-1",
        ],
        &["brief", "confirm", "--review", &review],
        &[
            "review",
            "link",
            "--review",
            &review,
            "--github-review",
            "gh-1",
        ],
    ];

    for attempt in attempts {
        let code = desk
            .command()
            .args(attempt)
            .assert()
            .get_output()
            .status
            .code();
        // `review link` succeeds — it records an id, which is its job — so what is
        // asserted is the draft's status, not the exit code. The others are refused.
        assert!(
            code == Some(0) || code == Some(1),
            "{attempt:?} must not report Review Desk as broken, got {code:?}"
        );

        let status = desk.json(&["draft", "show", "--review", &review])["draft"]["status"]
            .as_str()
            .expect("a status")
            .to_string();
        assert_eq!(
            status, "ready",
            "{attempt:?} moved the draft to {status}; only the developer approves"
        );
    }
}

#[test]
fn a_draft_cannot_be_marked_posted_until_the_developer_has_approved_it() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "412", "3f9c2ab");
    let id: i64 = review.parse().expect("the review id");

    desk.command()
        .args(["draft", "put", "--review", &review, "--file", "-"])
        .write_stdin("the body")
        .assert()
        .success();

    let refused = desk.refused_json(&[
        "draft",
        "posted",
        "--review",
        &review,
        "--github-review",
        "gh-review-3312",
    ]);
    assert_eq!(refused["error"]["code"], "draft_status");
    assert!(refused["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("approved"));

    // The developer approves, in the dashboard's place, and only then does the session
    // get to report what it posted.
    desk.store()
        .approve_draft(id, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer approves");

    let shown = desk.json(&["draft", "show", "--review", &review]);
    assert_eq!(shown["draft"]["status"], "approved");
    assert_eq!(
        shown["draft"]["event"], "request_changes",
        "which is how a review session learns what to post it as"
    );

    let posted = desk.json(&[
        "draft",
        "posted",
        "--review",
        &review,
        "--github-review",
        "gh-review-3312",
    ]);
    assert_eq!(posted["draft"]["status"], "posted");
    assert_eq!(
        desk.json(&["review", "show", "--review", &review])["review"]["github_review_id"],
        "gh-review-3312"
    );
}
