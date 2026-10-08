//! What every test here needs: a server on a temporary database, and a client.
//!
//! The client is twenty lines of `TcpStream` rather than an HTTP crate. Every acceptance
//! criterion in DEV-861 is about what something on `127.0.0.1` sees, so the tests speak
//! to a real socket; and a server whose whole argument is dependency weight should not
//! pull a client library in to test itself.

// Every test binary in this directory compiles the whole module, so each one warns about
// whichever helpers it does not happen to use.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use review_desk_harvest::recorded::Recorded;
use review_desk_harvest::{CommitFiles, Comparison, GitHub, PullRequest, ReviewThread};
use review_desk_server::{Github, Records, Running, Server};
use review_desk_store::{Disposition, NewBrief, NewFinding, Severity, Store};
use serde_json::Value;
use tempfile::TempDir;

/// GitHub responses captured off a real pull request.
///
/// `crates/harvest/tests/fixtures` is used rather than copied. They were captured from
/// `devergehq/locus#64` and they are parsed by `harvest::wire`, which is the same code
/// the live `gh` path goes through — a second copy would be one more thing to keep
/// identical for no gain.
pub fn recorded_github() -> Recorded {
    Recorded::from_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../harvest/tests/fixtures"))
}

/// The head commit `pull_request.json` reports, which is what a resolved review holds.
pub const RECORDED_HEAD_SHA: &str = "46f6ea1645a9bbc9308f791411b2cac1a093367b";

/// Any GitHub reader, with its reads counted.
///
/// This is what makes "refused without a GitHub read" an assertion rather than a hope:
/// the inner reader is one that *would* have answered, so a non-zero count means the
/// handler reached for GitHub before it finished checking the request.
#[derive(Debug)]
pub struct Counted<G> {
    inner: G,
    calls: Arc<AtomicUsize>,
}

/// Wrap a reader, handing back the counter to assert on.
pub fn counted<G: GitHub>(inner: G) -> (Counted<G>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Counted {
            inner,
            calls: Arc::clone(&calls),
        },
        calls,
    )
}

impl<G: GitHub> GitHub for Counted<G> {
    fn pull_request(&self, repo: &str, pr: i64) -> review_desk_harvest::Result<PullRequest> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.pull_request(repo, pr)
    }

    fn review_threads(
        &self,
        repo: &str,
        pr: i64,
    ) -> review_desk_harvest::Result<Vec<ReviewThread>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.review_threads(repo, pr)
    }

    fn compare(
        &self,
        repo: &str,
        base: &str,
        head: &str,
    ) -> review_desk_harvest::Result<Comparison> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.compare(repo, base, head)
    }

    fn commit(&self, repo: &str, sha: &str) -> review_desk_harvest::Result<CommitFiles> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.commit(repo, sha)
    }
}

/// A running server, its database, and the directory both die with.
#[derive(Debug)]
pub struct Harness {
    pub running: Running,
    pub store: Store,
    _directory: TempDir,
}

/// Start a server on a fresh database, reading GitHub through `github`.
pub fn start(github: impl review_desk_harvest::GitHub + Send + 'static) -> Harness {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("review-desk.db");

    // Two handles on the same file: one the server answers from, one the test seeds and
    // inspects with. That is what lets a test assert a refusal wrote nothing.
    let served = Store::open(&path).expect("opening the served database");
    let store = Store::open(&path).expect("opening the test's handle on the database");

    // Port 0, so tests can run at the same time without choosing ports by hand.
    let server = Server::bind(0, Records::new(served), Github::new(github))
        .expect("binding a free port on 127.0.0.1");

    Harness {
        running: server.spawn(),
        store,
        _directory: directory,
    }
}

/// Start a server whose GitHub has nothing recorded, so every read fails.
pub fn start_with_no_github() -> Harness {
    start(Recorded::nothing())
}

impl Harness {
    /// A request to the API, under the contract's `/api/v1`.
    pub fn api(&self, method: &str, path: &str, body: Option<&str>) -> Answer {
        self.request(method, &format!("/api/v1{path}"), body)
    }

    /// A request to anything.
    pub fn request(&self, method: &str, path: &str, body: Option<&str>) -> Answer {
        request(&self.running.base(), method, path, body)
    }
}

/// What came back.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Answer {
    /// The body as JSON. Fails the test if it is not.
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "expected JSON, got {error}: {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// One header's value, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The error code in the contract's error shape.
    ///
    /// The body is the `Error` object itself: every error response in `openapi.yaml` is
    /// `$ref: Error`, and the examples under `contract/examples/errors/` are bare.
    pub fn error_code(&self) -> String {
        self.json()["code"]
            .as_str()
            .unwrap_or_else(|| panic!("no code in {}", self.text()))
            .to_string()
    }
}

/// Make one HTTP/1.1 request and read the whole answer.
///
/// `Connection: close` is sent so the server closes the socket when it has finished,
/// which makes "read to the end" the right way to read the body and keeps the client
/// from having to parse chunked encoding or honour keep-alive.
pub fn request(base: &str, method: &str, path: &str, body: Option<&str>) -> Answer {
    let address = base.trim_start_matches("http://");
    let mut socket = TcpStream::connect(address).expect("connecting to the server");

    let body = body.unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {address}\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         \r\n{body}",
        body.len()
    );
    socket
        .write_all(request.as_bytes())
        .expect("writing the request");

    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).expect("reading the answer");
    parse(&raw)
}

/// Split a raw HTTP answer into a status, headers and a body.
fn parse(raw: &[u8]) -> Answer {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("an answer with a header block");
    let head = String::from_utf8_lossy(&raw[..split]);
    let body = raw[split + 4..].to_vec();

    let mut lines = head.lines();
    let status_line = lines.next().expect("a status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status code in {status_line:?}"));

    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();

    Answer {
        status,
        headers,
        body,
    }
}

/// A review with a brief and two findings, as the record would hold after layers 2 and 5.
#[derive(Debug)]
pub struct Seeded {
    pub review_id: i64,
}

// The `..Default::default()` on `NewBrief` has no effect today and is kept deliberately:
// DEV-860 adds `options` and `provenance` to that struct, and the spread is what keeps
// this seed compiling across that rebase. Clippy is right that it does nothing yet.
#[allow(clippy::needless_update)]
pub fn seed(store: &Store) -> Seeded {
    let opened = store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("opening a review");
    let review_id = opened.review.id;

    store
        .put_brief(
            review_id,
            NewBrief {
                problem: "Failed webhook deliveries are never retried.".to_string(),
                diagram: Some("flowchart LR\n  A[Event] --> B[Deliver]".to_string()),
                option_map: Some("A: retry inside the request. B: a queue.".to_string()),
                approach_verdict: Some("The queue is right for this volume.".to_string()),
                // Spread, not listed: DEV-860 adds `options` and `provenance` to this
                // struct, and it derives Default, so this seed survives that rebase.
                ..Default::default()
            },
        )
        .expect("writing a brief");

    store
        .add_finding(
            review_id,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(58),
                claim: "A retried delivery is sent without an idempotency key.".to_string(),
                evidence: Some("throwaway test, 2 deliveries for 1 event".to_string()),
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .expect("writing a finding");

    store
        .add_finding(
            review_id,
            NewFinding {
                lens: "repetition".to_string(),
                severity: Severity::Nit,
                file: None,
                line: None,
                claim: "Backoff schedule repeated in two files.".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
            },
        )
        .expect("writing a second finding");

    Seeded { review_id }
}
