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
    review_desk()
        .args(["review", "open"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unrecognized subcommand"));
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
