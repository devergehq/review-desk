//! Test scaffolding for the binary.
//!
//! Every test gets its own database in a `TempDir` and points `REVIEW_DESK_DB` at it, so
//! no test can read or write `~/.review-desk/review-desk.db`.
//!
//! Each integration test file compiles this module into its own binary, so a helper one
//! of them does not use is dead code there. That is what the allow is for.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use review_desk_store::Store;
use serde_json::Value;
use tempfile::TempDir;

/// A Review Desk with a database of its own.
#[derive(Debug)]
pub struct Desk {
    dir: TempDir,
}

impl Desk {
    pub fn new() -> Self {
        Desk {
            dir: TempDir::new().expect("temporary directory"),
        }
    }

    /// Where this desk's database is.
    pub fn db(&self) -> PathBuf {
        self.dir.path().join("review-desk.db")
    }

    /// This desk's directory, for a test that has to write a file to hand to a command.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The binary, pointed at this desk's database.
    pub fn command(&self) -> Command {
        let mut command = Command::cargo_bin("review-desk").expect("the review-desk binary");
        command.env("REVIEW_DESK_DB", self.db());
        command
    }

    /// Run a command that must succeed, and hand back its standard output, trimmed.
    pub fn run(&self, arguments: &[&str]) -> String {
        let output = self.command().args(arguments).assert().success();
        String::from_utf8(output.get_output().stdout.clone())
            .expect("output is text")
            .trim()
            .to_string()
    }

    /// Run a command that must succeed with `--json`, and hand back its one document.
    pub fn json(&self, arguments: &[&str]) -> Value {
        let text = self.run(&[arguments, &["--json"]].concat());
        serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("{arguments:?} did not print one JSON document: {error}\n{text}")
        })
    }

    /// Run a command that must be refused with `--json`, and hand back its one document.
    pub fn refused_json(&self, arguments: &[&str]) -> Value {
        let output = self
            .command()
            .args(arguments)
            .arg("--json")
            .assert()
            .code(1);
        let text = String::from_utf8(output.get_output().stdout.clone()).expect("output is text");
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("a refusal must still be one document: {error}\n{text}"))
    }

    /// Write a file in this desk's directory and hand back its path, for a command that
    /// takes a document.
    pub fn write_file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, contents).expect("write the document");
        path
    }

    /// A store on this desk's database, for seeding what no command creates — rules,
    /// which DEV-851 deliberately leaves out of the command line.
    pub fn store(&self) -> Store {
        Store::open(self.db()).expect("open the store")
    }
}

/// Open a review and hand back its id, the way a caller would: by reading the one line
/// `review open` prints.
pub fn open_review(desk: &Desk, repo: &str, pr: &str, head: &str) -> String {
    desk.run(&["review", "open", "--repo", repo, "--pr", pr, "--head", head])
}

/// Record a review as posted, so that a new head commit on its pull request opens the next
/// round rather than moving this round's head.
///
/// **DEV-962: a round is a posted review.** `review open` moves an unposted round to a new
/// commit — Patrick's queue held one pull request twice, round 1 and round 2, over a round 1
/// he had never posted — so a test that wants a second round has to have posted the first.
/// `review link --github-review` is the one way the command line says a review was posted,
/// and Review Desk posts nothing itself.
pub fn post_round(desk: &Desk, review: &str) {
    desk.command()
        .args([
            "review",
            "link",
            "--review",
            review,
            "--github-review",
            &format!("gh-review-{review}"),
        ])
        .assert()
        .success();
}

/// Make a review count as started: somebody has begun on it.
///
/// **One finding is the cheapest honest way to say a session got that far**, and it is what
/// `ReviewWork` asks for — a brief, a finding or a draft. Since DEV-962 this no longer
/// decides whether a new commit opens a round; [`post_round`] does. What it is still for is
/// a test about work a delete would lose, or about `review find`'s `started`.
pub fn start_review(desk: &Desk, review: &str) {
    desk.command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(
            serde_json::json!({
                "lens": "correctness",
                "severity": "nit",
                "claim": "a session reviewed this round",
            })
            .to_string(),
        )
        .assert()
        .success();
}

/* ------------------------------- A served desk ------------------------------- */

/// A running `review-desk serve`, and the first line it printed.
///
/// **Here rather than in `tests/serve.rs`, because two test files need a server now.**
/// That file tests the *command* — what it prints, and that the address it printed answers
/// — and `stale_code_changed.rs` tests an *answer* the server gives over a database the
/// command line seeded. One spawn helper, because a second copy is a second set of
/// decisions about pipes and deadlines to keep identical.
///
/// **Its output goes to a file, not a pipe, and that is not a style choice.** The first
/// version of this piped stdout, read one line and dropped the reader — which closes the
/// read end, so the child's *next* `println!` fails with `EPIPE` and panics, killing the
/// server the test had just started. `serve` prints two lines with a front end embedded
/// and three without, so the window was narrow on a developer's machine and wide on CI: it
/// failed once locally in about twenty runs, and then on CI, where there is no front end.
/// A file has no reader to close.
#[derive(Debug)]
pub struct Served {
    child: Child,
    /// The first line it printed, which is where it says it is.
    pub line: String,
    /// Holds the file its output went to. Dropping it removes the file.
    _output: TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    /// The address it is answering on, as `host:port`.
    ///
    /// Read out of the line it printed, in either form: `--json` prints one document
    /// carrying `serve.address`, and without it the line names the URL in prose. A test
    /// that wants the address should not also have to know which form it asked for.
    pub fn address(&self) -> String {
        if let Ok(document) = serde_json::from_str::<Value>(&self.line) {
            if let Some(address) = document["serve"]["address"].as_str() {
                return address.to_string();
            }
        }
        self.line
            .trim()
            .rsplit("http://")
            .next()
            .expect("a URL in the line serve printed")
            .trim_end_matches('/')
            .to_string()
    }
}

/// Start `review-desk serve` on a free port and read the line it prints.
pub fn serve(desk: &Desk, json: bool) -> Served {
    let output = TempDir::new().expect("a directory for the child's output");
    let path = output.path().join("serve.out");
    let file = std::fs::File::create(&path).expect("the child's output file");

    let binary = assert_cmd::cargo::cargo_bin("review-desk");
    let mut command = std::process::Command::new(binary);
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
/// A full request, not a bare connect. Connecting and dropping the socket leaves a worker
/// reading a request that never arrives, which on a loaded machine is one of four workers
/// tied up for no reason — and "something accepted the connection" is a weaker claim than
/// "the server answered" anyway.
pub fn wait_for(address: &str) -> bool {
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

/// One GET, returning the whole answer: status line, headers and body.
pub fn get(address: &str, path: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("connecting to the served address");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    socket.write_all(request.as_bytes()).expect("the request");
    let mut answer = String::new();
    socket.read_to_string(&mut answer).expect("the answer");
    answer
}

/// One GET that must answer 200 with one JSON document, and hand that document back.
pub fn get_json(address: &str, path: &str) -> Value {
    let answer = get(address, path);
    assert!(
        answer.contains("200 OK"),
        "GET {path} should answer 200: {answer}"
    );
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_else(|| panic!("GET {path} answered with no body: {answer}"));
    serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("GET {path} did not answer one document: {error}\n{body}"))
}
