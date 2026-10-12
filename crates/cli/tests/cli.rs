//! What the binary does when it is actually run.
//!
//! Every test either removes `REVIEW_DESK_DB` or points it at a temporary file, so no
//! test can read or write `~/.review-desk/review-desk.db`.

use std::path::MAIN_SEPARATOR;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// The binary under test, with the ambient database override cleared.
fn review_desk() -> Command {
    let mut command = Command::cargo_bin("review-desk").expect("the review-desk binary");
    command.env_remove("REVIEW_DESK_DB");
    command
}

#[test]
fn version_prints_the_crate_version() {
    review_desk()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn db_path_prints_the_default_location() {
    let expected = format!("{sep}.review-desk{sep}review-desk.db", sep = MAIN_SEPARATOR);

    review_desk()
        .args(["db", "path"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with(format!("{expected}\n")));
}

#[test]
fn db_path_honours_the_environment_variable() {
    let dir = TempDir::new().unwrap();
    let chosen = dir.path().join("somewhere-else.db");

    review_desk()
        .env("REVIEW_DESK_DB", &chosen)
        .args(["db", "path"])
        .assert()
        .success()
        .stdout(predicate::eq(format!("{}\n", chosen.display())));
}

#[test]
fn db_path_does_not_create_anything() {
    let dir = TempDir::new().unwrap();
    let chosen = dir.path().join("not-created.db");

    review_desk()
        .env("REVIEW_DESK_DB", &chosen)
        .args(["db", "path"])
        .assert()
        .success();

    assert!(
        !chosen.exists(),
        "printing the path must not open the database"
    );
}

#[test]
fn db_version_creates_the_database_and_prints_its_version() {
    let dir = TempDir::new().unwrap();
    let chosen = dir.path().join("fresh.db");

    review_desk()
        .env("REVIEW_DESK_DB", &chosen)
        .args(["db", "version"])
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "{}\n",
            review_desk_store::newest_version()
        )));

    assert!(chosen.exists());
}

#[test]
fn a_database_that_cannot_be_opened_exits_two() {
    // A directory is not a database. This is Review Desk being unable to run, not a
    // request being refused, so it is exit 2.
    let dir = TempDir::new().unwrap();

    review_desk()
        .env("REVIEW_DESK_DB", dir.path())
        .args(["db", "version"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("review-desk:"));
}

#[test]
fn an_unknown_command_is_refused_not_reported_as_broken() {
    // Exit 1, not 2. clap's own default is 2, which is the code this binary reserves
    // for being unable to run — a typo would have paged whoever treats 2 as broken.
    //
    // The names here are deliberate nonsense. This guard first used `review open`, which
    // DEV-851 then made a real command; a guard against unknown commands has to name
    // something no later slice will ever claim, or it quietly stops guarding.
    review_desk()
        .args(["not-a-command"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unrecognized subcommand"));

    review_desk()
        .args(["review", "not-a-command"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn a_missing_argument_is_refused_not_reported_as_broken() {
    // The case the old guard was really testing: a real command with an argument left
    // out. clap calls this a usage error and would exit 2 of its own accord.
    review_desk()
        .args(["review", "open", "--repo", "devergehq/review-desk"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--pr"));
}

#[test]
fn a_corrupt_database_exits_two() {
    // The other half of "a database that cannot be read": a file that opens fine and
    // turns out not to be a database. SQLite opens lazily, so this fails later and under
    // a different error than a path that is a directory — but it is still Review Desk
    // being unable to run, so it is still 2.
    let dir = TempDir::new().unwrap();
    let corrupt = dir.path().join("corrupt.db");
    std::fs::write(&corrupt, b"this is not a SQLite database, it is a note").unwrap();

    review_desk()
        .env("REVIEW_DESK_DB", &corrupt)
        .args(["db", "version"])
        .assert()
        .code(2);
}

#[test]
fn help_stays_text_even_when_json_is_asked_for() {
    // The one stated exception to "with --json, standard output is one JSON document":
    // --help and --version describe the command line rather than answering a question
    // about a record, and they are not failures. Pinned here so it stays a decision.
    review_desk()
        .args(["--help", "--json"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Usage:"));

    review_desk()
        .args(["--version", "--json"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_missing_subcommand_is_refused() {
    review_desk().arg("db").assert().code(1);
    review_desk().assert().code(1);
}

#[test]
fn help_and_version_are_not_errors() {
    review_desk().arg("--help").assert().code(0);
    review_desk().arg("--version").assert().code(0);
    review_desk().args(["db", "--help"]).assert().code(0);
}

/// **No command removes a review, and that is the decision rather than an omission.**
///
/// Patrick, 12 October 2026, on DEV-945: dismissing a review and deleting one are "the
/// developer's act, from the page, never a session's". The same rule is already why there
/// is no `brief confirm` and no `draft approve` — a session that could sign off could sign
/// off its own review — and it matters more here: a session that could dismiss a review
/// could take its own work out of the developer's sight, and a session that could delete
/// one could remove the record of it.
///
/// Written over the surface rather than against a list of names, because the failure this
/// guards against is a *new* command, and a test that named only today's would not see it.
/// Both halves are needed: the verbs are refused, and `--help` offers nothing that reads
/// like one.
#[test]
fn a_session_cannot_remove_a_review_from_the_command_line() {
    for verb in ["dismiss", "restore", "delete", "remove", "hide", "close"] {
        review_desk()
            .args(["review", verb, "--review", "1"])
            .assert()
            .code(1)
            .stderr(predicate::str::contains("unrecognized subcommand"));

        review_desk()
            .args([verb, "--review", "1"])
            .assert()
            .code(1)
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }

    // And nothing in the command line's own description of itself offers one. `review
    // --help` lists every subcommand of the group a removal would most naturally be added
    // to, so a command added there is caught here even under a name this test did not
    // think of.
    let listed = review_desk()
        .args(["review", "--help"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let listed = String::from_utf8(listed).expect("help is text");
    for word in ["dismiss", "restore", "delete", "remove"] {
        assert!(
            !listed.contains(word),
            "`review --help` offers {word:?}, and removing a review is the developer's \
             act in the dashboard: {listed}"
        );
    }
}
