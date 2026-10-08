//! Only the developer signs off.
//!
//! Decided by Patrick on 9 October 2026: confirming a brief and approving a draft
//! happen in the dashboard. The command line that agent sessions call has no command
//! for either, and this file is what keeps that true — the rest of the codebase could
//! drift back towards it one convenience at a time, and none of the other tests would
//! notice.

mod common;

use std::path::{Path, PathBuf};

use common::{open_review, Desk};
use review_desk_store::{Actor, DraftStatus, NewDraft, ReviewEvent};

/// The two store functions that sign something off.
const SIGN_OFF: [&str; 2] = ["confirm_brief", "approve_draft"];

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

    store.put_draft(id, NewDraft::ready("the body")).unwrap();
    let approved = store
        .approve_draft(id, ReviewEvent::Comment, Actor::Developer)
        .expect("the dashboard approves the draft through this function");
    assert_eq!(approved.status, DraftStatus::Approved);
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
