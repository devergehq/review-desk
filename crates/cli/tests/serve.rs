//! `review-desk serve`: what it prints, and that the thing it printed is answering.
//!
//! The server itself is tested in `review-desk-server`. What is tested here is the
//! command: that it says where it is before it starts serving, that the address it names
//! is reachable, and that `--json` keeps its promise of one document.
//!
//! **The spawn is `common::serve`.** It lived here, and `stale_code_changed.rs` then needed
//! a server too — over a database the command line had seeded, which is a different
//! question from this file's. The helper moved rather than being copied, and the argument
//! for writing the child's output to a file rather than a pipe moved with it.

mod common;

use std::time::Duration;

use common::{get, serve, wait_for, Desk};

#[test]
fn serve_prints_its_url_and_answers_there() {
    let desk = Desk::new();
    let served = serve(&desk, false);
    let line = &served.line;

    assert!(
        line.contains("Review Desk is at http://127.0.0.1:"),
        "serve should say where it is; it said {line:?}"
    );

    let address = served.address();

    assert!(
        wait_for(&address),
        "nothing was listening at {address}, which serve had just printed"
    );

    let health = get(&address, "/api/v1/health");
    assert!(
        health.contains("200 OK") && health.contains("\"status\":\"ok\""),
        "the health endpoint should answer at the printed address: {health}"
    );

    let root = get(&address, "/");
    assert!(root.contains("200 OK"), "/ should answer: {root}");
}

#[test]
fn serve_binds_only_the_loopback_interface() {
    let desk = Desk::new();
    let served = serve(&desk, false);

    // The printed address is the whole promise: there is no flag to bind anywhere else,
    // and this asserts the command has not grown one.
    assert!(
        served.line.contains("http://127.0.0.1:"),
        "serve must bind 127.0.0.1; it printed {:?}",
        served.line
    );
}

#[test]
fn serve_with_json_prints_one_document() {
    let desk = Desk::new();
    let served = serve(&desk, true);
    let line = &served.line;

    let document: serde_json::Value =
        serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line:?}"));

    let serve = &document["serve"];
    assert!(
        serve["url"]
            .as_str()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:")),
        "the document should carry the URL: {document}"
    );
    assert_eq!(
        serve["database"],
        desk.db().display().to_string(),
        "the document should say which database is being served"
    );
    assert!(
        serve["front_end_embedded"].is_boolean(),
        "the document should say whether the dashboard is in this binary: {document}"
    );
}

#[test]
fn serve_refuses_a_port_already_taken() {
    let desk = Desk::new();
    let first = serve(&desk, true);
    let port = serde_json::from_str::<serde_json::Value>(&first.line).unwrap()["serve"]["address"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .to_string();

    // Exit 1, not 2: a port already in use is a request that was understood and refused,
    // not Review Desk being unable to run. The command line's exit codes rest on that
    // split and Locus's review worker treats 2 as a loud warning.
    let mut again = assert_cmd::Command::cargo_bin("review-desk").unwrap();
    let outcome = again
        .env("REVIEW_DESK_DB", desk.db())
        .args(["serve", "--port", &port])
        .timeout(Duration::from_secs(20))
        .assert()
        .failure();

    let code = outcome.get_output().status.code();
    assert_eq!(code, Some(1), "a taken port is a refusal, which is exit 1");

    drop(first);
}

#[test]
fn serve_is_in_the_help() {
    let desk = Desk::new();
    desk.command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("serve"));
}
