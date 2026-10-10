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
use std::time::Duration;

use review_desk_harvest::recorded::Recorded;
use review_desk_harvest::{CommitFiles, Comparison, GitHub, PullRequest, ReviewThread};
use review_desk_server::{Github, Records, Running, Server};
use review_desk_store::{
    BriefOption, Disposition, DraftLint, DraftStatus, FindingHunk, HunkLine, HunkLineMarker,
    NewBrief, NewDraft, NewFinding, NewOutcome, NewRule, OutcomeKind, ReviewLayerState, RuleScope,
    RuleStage, Severity, Store, Verification,
};
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

/// The title `pull_request.json` reports, verbatim.
///
/// `devergehq/locus#64`'s own title, as `gh` printed it. Written out here rather than read
/// from the file so that a test asserting the title can be read without opening a fixture
/// — and so that re-capturing the fixture against a different pull request fails loudly
/// instead of silently asserting whatever the new one is called.
pub const RECORDED_TITLE: &str =
    "DEV-795: give dispatcher watchers an exit condition, and a way to see the ones that have none";

/// Additions plus deletions, as `pull_request.json` reports them: 1097 and 3.
pub const RECORDED_CHANGED_LINES: i64 = 1100;

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

    /// A request with the headers stated exactly, and nothing added.
    ///
    /// This is what lets a test be a browser on another origin, or a form post, or a
    /// client that left the `Host` off — none of which the ordinary helper can be,
    /// because it always sends a correct set.
    pub fn raw(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Answer {
        raw_request(&self.running.base(), method, path, headers, body)
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
    raw_request(
        base,
        method,
        path,
        &[("Host", address), ("Content-Type", "application/json")],
        body,
    )
}

/// One HTTP/1.1 request with exactly these headers, plus the two the transport needs.
///
/// `Connection: close` is always sent so the server closes the socket when it has
/// finished, which makes "read to the end" the right way to read the body; and
/// `Content-Length` is computed, because a test that lies about it is testing the lie.
pub fn raw_request(
    base: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Answer {
    let address = base.trim_start_matches("http://");
    let mut socket = TcpStream::connect(address).expect("connecting to the server");

    let body = body.unwrap_or_default();
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Connection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    ));

    if body.len() <= WRITE_BLIND {
        // One write, as before, and no cost: everything the contract sends is small.
        socket
            .write_all(format!("{head}{body}").as_bytes())
            .expect("writing the request");
    } else {
        // **A big body is sent while watching for an answer, and the body-size limit is
        // why.** A server that refuses a request for the length it announced answers from
        // the head alone — which is the entire point of having a limit — and then closes.
        // Writing on past that gets a broken pipe, and the write after *that* earns a
        // reset, which throws away the receive buffer and with it the very answer the test
        // came for. RFC 9110 asks a client sending a large body to watch for an early
        // error response rather than plough on; this one now does.
        //
        // It took a change of transport to notice. The previous one drained a body it had
        // already refused, so writing blind always worked and nobody had to decide whether
        // reading five megabytes in order to discard them was wanted. It is not.
        socket
            .write_all(head.as_bytes())
            .expect("writing the request head");

        if let Some(early) = answered_already(&mut socket) {
            return parse(&early);
        }

        // Not checked: by here the server has had its chance to refuse and did not, but a
        // refusal landing mid-write is still its right rather than this client's business.
        let _ = socket.write_all(body.as_bytes());
    }

    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).expect("reading the answer");
    parse(&raw)
}

/// A body this size or smaller is written in one go, head and all.
///
/// Comfortably over anything the contract sends and comfortably under the server's limit,
/// so the careful path below is taken only by the test that deliberately goes over.
const WRITE_BLIND: usize = 64 * 1024;

/// Whatever the server has already said, if it has said anything.
///
/// Used between the head and a large body. `None` means nothing arrived in the window and
/// the body should be sent; the read timeout is cleared either way, so the caller is
/// handed back a socket that blocks normally.
fn answered_already(socket: &mut TcpStream) -> Option<Vec<u8>> {
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();

    let mut first = [0_u8; 4096];
    let read = socket.read(&mut first);
    socket.set_read_timeout(None).ok();

    match read {
        Ok(got) if got > 0 => {
            let mut raw = first[..got].to_vec();
            let _ = socket.read_to_end(&mut raw);
            Some(raw)
        }
        _ => None,
    }
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

/// A record with enough in it that every one of the sixteen operations has something
/// real to answer with.
///
/// Built through the store's public API rather than by writing SQL, so it cannot drift
/// from what the command line would have produced.
#[derive(Debug)]
pub struct Seeded {
    pub review_id: i64,
    /// A second review, on another repository, so cross-repository queries are exercised
    /// by more than one row.
    pub other_review_id: i64,
    pub rule_id: i64,
    pub proposed_rule_id: i64,
    /// A review a session is still working on, for the Inbox's "In progress".
    pub running_review_id: i64,
}

#[allow(clippy::needless_update)]
pub fn seed(store: &Store) -> Seeded {
    let review_id = store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("opening a review")
        .review
        .id;

    store
        .set_review_facts(
            review_id,
            &review_desk_store::ReviewFacts {
                title: Some("Retry failed webhook deliveries".to_string()),
                ticket_id: Some("ABC-231".to_string()),
                changed_lines: Some(214),
                not_checked: Some(
                    "The admin screens, which this change does not touch.".to_string(),
                ),
            },
        )
        .expect("setting the review's facts");

    store
        .put_brief(
            review_id,
            NewBrief {
                problem: "Failed webhook deliveries are never retried.".to_string(),
                diagram: Some("flowchart LR\n  A[Event] --> B[Deliver]".to_string()),
                options: vec![
                    BriefOption {
                        key: "A".to_string(),
                        title: "Retry inside the request".to_string(),
                        argument_for: "Smallest change, no new tables.".to_string(),
                        argument_against: "Holds the request open.".to_string(),
                        chosen: false,
                    },
                    BriefOption {
                        key: "B".to_string(),
                        title: "Queue with backoff".to_string(),
                        argument_for: "Meets the 24-hour window.".to_string(),
                        argument_against: "Needs an idempotency key.".to_string(),
                        chosen: true,
                    },
                ],
                approach_verdict: Some("B is the right shape for this ticket.".to_string()),
                provenance: Some("from the ticket and the code".to_string()),
                ..Default::default()
            },
        )
        .expect("writing a brief");

    for number in 1..=7 {
        let state = match number {
            1 => ReviewLayerState::Done,
            2 => ReviewLayerState::WaitingOnYou,
            3..=5 => ReviewLayerState::Done,
            6 => ReviewLayerState::WaitingOnYou,
            _ => ReviewLayerState::Pending,
        };
        store
            .set_review_layer(review_id, number, state, Some("seeded"))
            .expect("writing a layer");
    }

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
                rule_id: None,
                // The first finding carries everything DEV-874 added, including a hunk
                // whose middle line is a removal and whose fourth is blank — so the
                // conformance test validates a filled `FindingHunk` and the awkward text
                // goes over the wire rather than only into the database.
                hunk: Some(FindingHunk {
                    file: "jobs/DeliverWebhook.php".to_string(),
                    start_line: 56,
                    lines: vec![
                        HunkLine {
                            marker: HunkLineMarker::Unchanged,
                            text: "{".to_string(),
                        },
                        HunkLine {
                            marker: HunkLineMarker::Removed,
                            text: "    $client->post($this->endpoint->url);".to_string(),
                        },
                        HunkLine {
                            marker: HunkLineMarker::Added,
                            text: "\t$response = $client->post($this->endpoint->url);  "
                                .to_string(),
                        },
                        HunkLine {
                            marker: HunkLineMarker::Added,
                            text: "   ".to_string(),
                        },
                    ],
                }),
                flagged_line: Some(57),
                inline_comment: Some(
                    "This retry re-sends the payload with no idempotency key.".to_string(),
                ),
                why_it_matters: Some(
                    "A customer whose endpoint answers slowly is charged twice.".to_string(),
                ),
                proof: Some("deliveries for evt_1: 2, expected 1\nnaïve ✓".to_string()),
                proof_note: Some("A throwaway test, not committed.".to_string()),
                verification: Verification::Reproduced,
                suggested_fix: Some("Send the delivery id as the key.".to_string()),
                fix_references: vec!["SendInvoiceEmail.php:44".to_string()],
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
                disposition: Disposition::HeldBack,
                disposition_reason: Some("a nit; it goes in the ticket".to_string()),
                rule_id: None,
                ..Default::default()
            },
        )
        .expect("writing a second finding");

    store
        .put_draft(
            review_id,
            NewDraft {
                body: "## Review\n\nOne blocker.".to_string(),
                status: DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(118),
                    word_budget: Some(150),
                    findings_in_body: Some(1),
                    findings_expected: Some(1),
                    raised_before_on_pr: Some(0),
                    errors: None,
                    checked_at: Some(review_desk_store::now()),
                }),
            },
        )
        .expect("writing a draft");

    // A developer edit, which is what the Draft preview screen lists.
    store
        .add_outcome(
            review_desk_store::FindingRef { review_id, seq: 2 },
            NewOutcome {
                kind: OutcomeKind::Cut,
                actor: review_desk_store::Actor::Developer,
                detail: Some("rename suggestion".to_string()),
                occurred_at: None,
            },
        )
        .expect("writing a developer outcome");

    let rule_id = store
        .create_rule(NewRule {
            statement: "A retry must carry an idempotency key.".to_string(),
            prevents: "One event delivered twice.".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: Some(review_id),
            origin_findings: vec![review_desk_store::FindingRef { review_id, seq: 1 }],
            why_proposed: None,
            note: Some("author disagreed once, with a reason you accepted".to_string()),
        })
        .expect("writing a rule")
        .id;

    let proposed_rule_id = store
        .create_rule(NewRule {
            statement: "Do not raise rename-for-clarity suggestions.".to_string(),
            prevents: "Nits that generate more nits.".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Proposed,
            origin_review_id: None,
            origin_findings: Vec::new(),
            why_proposed: Some("you cut this from 5 of the last 5 drafts".to_string()),
            note: None,
        })
        .expect("writing a proposed rule")
        .id;

    // A review a session is still working on. In progress means a layer is `running` —
    // a layer waiting on the developer is the other groups' business — so the Inbox's
    // "In progress" group is empty without one of these.
    let running_id = store
        .open_review("acme/billing-api", 93, "bb28cd0")
        .expect("opening a running review")
        .review
        .id;
    store
        .set_review_links(
            running_id,
            review_desk_store::ReviewLinks {
                session_id: Some("allele-ae03f8ce".to_string()),
                ..Default::default()
            },
        )
        .expect("linking the session");
    store
        .set_review_layer(
            running_id,
            5,
            ReviewLayerState::Running,
            Some("code review"),
        )
        .expect("writing a running layer");

    // A second review, on another repository, so the cross-repository queries and the
    // repository list have more than one row to be right about.
    let other_review_id = store
        .open_review("acme/billing-api", 88, "aa17bc9")
        .expect("opening a second review")
        .review
        .id;
    store
        .put_brief(
            other_review_id,
            NewBrief {
                problem: "Invoice totals round per line.".to_string(),
                ..Default::default()
            },
        )
        .expect("writing a second brief");

    Seeded {
        review_id,
        other_review_id,
        rule_id,
        proposed_rule_id,
        running_review_id: running_id,
    }
}
