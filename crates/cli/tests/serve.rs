//! `review-desk serve`: what it prints, and that the thing it printed is answering.
//!
//! The server itself is tested in `review-desk-server`. What is tested here is the
//! command: that it says where it is before it starts serving, that the address it names
//! is reachable, and that `--json` keeps its promise of one document.
//!
//! This spawns the binary rather than going through `assert_cmd`, because `serve` does
//! not exit: the test reads what it printed, uses it, and kills it.
//!
//! **Its output goes to a file, not a pipe, and that is not a style choice.** The first
//! version piped stdout, read one line and dropped the reader — which closes the read end,
//! so the child's *next* `println!` fails with `EPIPE` and panics, killing the server the
//! test had just started. `serve` prints two lines with a front end embedded and three
//! without, so the window was narrow on a developer's machine and wide on CI: it failed
//! once locally in about twenty runs, and then on CI, where there is no front end. A file
//! has no reader to close.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::Desk;
use tempfile::TempDir;

/// A running `review-desk serve`, and the first line it printed.
#[derive(Debug)]
struct Served {
    child: Child,
    line: String,
    /// Holds the file its output went to. Dropping it removes the file.
    _output: TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start `review-desk serve` on a free port and read the line it prints.
fn serve(desk: &Desk, json: bool) -> Served {
    let output = TempDir::new().expect("a directory for the child's output");
    let path = output.path().join("serve.out");
    let file = std::fs::File::create(&path).expect("the child's output file");

    let binary = assert_cmd::cargo::cargo_bin("review-desk");
    let mut command = Command::new(binary);
    command
        .env("REVIEW_DESK_DB", desk.db())
        // Port 0 so two runs of the test suite cannot collide on a port.
        .args(["serve", "--port", "0"])
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped());
    if json {
        command.arg("--json");
    }

    let child = command.spawn().expect("starting review-desk serve");

    // Wait for the first line to land. The file is written by the child and read here;
    // nothing the test does can interrupt the child's writing.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut line = String::new();
    while Instant::now() < deadline {
        let printed = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some((first, _)) = printed.split_once('\n') {
            line = first.to_string();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        !line.is_empty(),
        "serve printed nothing in 20 seconds; it should say where it is before it serves"
    );

    Served {
        child,
        line,
        _output: output,
    }
}

/// Wait until the address *answers*, or give up.
///
/// A full request, not a bare connect. Connecting and dropping the socket leaves a
/// worker reading a request that never arrives, which on a loaded machine is one of four
/// workers tied up for no reason — and "something accepted the connection" is a weaker
/// claim than "the server answered" anyway.
fn wait_for(address: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() && get(address, "/api/v1/health").contains("200 OK")
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// One GET, returning the body.
fn get(address: &str, path: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("connecting to the served address");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    socket.write_all(request.as_bytes()).expect("the request");
    let mut answer = String::new();
    socket.read_to_string(&mut answer).expect("the answer");
    answer
}

#[test]
fn serve_prints_its_url_and_answers_there() {
    let desk = Desk::new();
    let served = serve(&desk, false);
    let line = &served.line;

    assert!(
        line.contains("Review Desk is at http://127.0.0.1:"),
        "serve should say where it is; it said {line:?}"
    );

    let address = line
        .trim()
        .rsplit("http://")
        .next()
        .expect("a URL in the line")
        .trim_end_matches('/')
        .to_string();

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
