//! What a review's Allele session is doing, read from Allele's control socket.
//!
//! **The one thing this server asks another program, and it only ever asks.** Review Desk
//! records which Allele session has each review (`session_id`); Allele knows whether that
//! session is working, blocked on a permission prompt, or gone. Until now nothing joined
//! the two, so a stalled review could not say why and the draft page guessed from forty
//! seconds of silence. Decided on 30 October 2026 in DEV-1140: the server reads Allele's
//! socket, as an optional connection.
//!
//! Three rules, each of which is a test below:
//!
//! - **Absent is silent.** No socket file means no Allele on this machine, and the answer is
//!   `None` — the page draws nothing and is as it was before this existed. Review Desk
//!   still needs nothing else installed.
//! - **Broken is loud and blocks nothing.** A socket that refuses, or accepts and does not
//!   answer within [`TIMEOUT`], reads as [`SessionStatus::Unreachable`]. The request that
//!   asked costs at most that long, and nothing else on the page waits on it.
//! - **One direction.** The request is `sessions_status` and nothing else. The session's
//!   `reply_to` address is read off the answer and dropped: Review Desk reads from a
//!   session and never writes into one.
//!
//! The wire is Allele's (`src/dispatch/protocol.rs` there): one JSON object per line each
//! way, over a Unix socket at `~/.allele/control.sock`, mode 0600. Allele keeps those wire
//! types apart from its internal ones on purpose, so an outside reader is what they are for.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a read may take, from connecting to the last byte of the answer.
///
/// One second, because this is asked every five seconds by a page someone is looking at,
/// and Allele answers `sessions_status` from memory in well under a millisecond. Anything
/// slower than this is not an answer worth waiting for; it is a stuck Allele, and saying
/// so is the useful thing to show.
pub const TIMEOUT: Duration = Duration::from_secs(1);

/// The start of the message Allele sends for an id it has no record of.
///
/// **Matched as text, which is the one brittle thing in this file, and it is here alone.**
/// Allele answers an unknown id with `bad_request` — the same code as a malformed request —
/// so until it has a code of its own the message is the only way to tell "that session is
/// gone" from "Allele refused the question". Allele is to be asked for `session_not_found`;
/// both are accepted, so this keeps working on either side of that change.
const NOT_FOUND_MESSAGE: &str = "no session with id";

/// What the strip on a review page says about its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Allele says it is working.
    Running,
    /// **Blocked on a permission prompt.** Nobody is coming unless a person answers it.
    AwaitingInput,
    /// It finished its turn and is waiting for a prompt.
    Finished,
    /// Suspended or done in Allele, or Allele has no session with that id. Nothing will
    /// pick the review up.
    Gone,
    /// Allele answered with something this does not know how to read. Not a guess.
    Unknown,
    /// The socket is there and did not answer.
    Unreachable,
}

impl SessionStatus {
    /// The word on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Running => "running",
            SessionStatus::AwaitingInput => "awaiting_input",
            SessionStatus::Finished => "finished",
            SessionStatus::Gone => "gone",
            SessionStatus::Unknown => "unknown",
            SessionStatus::Unreachable => "unreachable",
        }
    }
}

/// One answer from Allele about one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub status: SessionStatus,
    /// Allele's own word for the state, as it sent it, when it sent one.
    ///
    /// Carried so the page can tell a suspended session from a done one, which the status
    /// folds together, and so an `unknown` says what Allele actually said.
    pub allele_state: Option<String>,
    /// Seconds since that state began, as Allele counts it.
    pub state_age_secs: Option<u64>,
    /// Why the status is what it is, when that is not obvious from the status: the error
    /// Allele sent, or what went wrong reaching it.
    pub detail: Option<String>,
}

impl Reading {
    fn of(status: SessionStatus) -> Self {
        Reading {
            status,
            allele_state: None,
            state_age_secs: None,
            detail: None,
        }
    }

    fn because(status: SessionStatus, detail: impl Into<String>) -> Self {
        Reading {
            detail: Some(detail.into()),
            ..Reading::of(status)
        }
    }
}

/// Where this server asks Allele, if anywhere.
#[derive(Debug, Clone, Default)]
pub struct Allele {
    socket: Option<PathBuf>,
}

impl Allele {
    /// Never ask. Every review reads as having no Allele, which is what the tests that are
    /// not about Allele want, and what a machine without it is.
    pub fn none() -> Self {
        Allele { socket: None }
    }

    /// Ask the socket at `path`. Its absence at the time of asking is not an error.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Allele {
            socket: Some(path.into()),
        }
    }

    /// Allele's own socket, `~/.allele/control.sock`, or none when there is no home.
    pub fn default_socket() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| Path::new(&home).join(".allele").join("control.sock"))
    }

    /// The socket this asks, if any.
    pub fn socket(&self) -> Option<&Path> {
        self.socket.as_deref()
    }

    /// What Allele says about `session_id`, or `None` when there is no Allele to ask.
    pub fn read(&self, session_id: &str) -> Option<Reading> {
        let socket = self.socket.as_deref()?;
        if !socket.exists() {
            return None;
        }
        Some(ask(socket, session_id))
    }
}

#[cfg(unix)]
fn ask(socket: &Path, session_id: &str) -> Reading {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error) => {
            return Reading::because(
                SessionStatus::Unreachable,
                format!("Allele's socket refused the connection: {error}"),
            )
        }
    };

    // Both directions, so a socket that accepts and never reads cannot hold a write either.
    let timed = stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)));
    if let Err(error) = timed {
        return Reading::because(SessionStatus::Unreachable, error.to_string());
    }

    let request = serde_json::json!({ "op": "sessions_status", "session_id": session_id });
    if let Err(error) = writeln!(stream, "{request}") {
        return Reading::because(
            SessionStatus::Unreachable,
            format!("could not ask Allele: {error}"),
        );
    }

    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(0) => Reading::because(
            SessionStatus::Unreachable,
            "Allele closed the connection without answering",
        ),
        Ok(_) => answer(&line),
        Err(error) => Reading::because(
            SessionStatus::Unreachable,
            format!(
                "Allele did not answer within {} s: {error}",
                TIMEOUT.as_secs()
            ),
        ),
    }
}

#[cfg(not(unix))]
fn ask(_socket: &Path, _session_id: &str) -> Reading {
    Reading::because(
        SessionStatus::Unreachable,
        "Allele's socket is a Unix socket, which this platform cannot open",
    )
}

/// Read one line of Allele's answer.
fn answer(line: &str) -> Reading {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return Reading::because(SessionStatus::Unknown, "Allele's answer was not JSON");
    };

    match value.get("result").and_then(|result| result.as_str()) {
        Some("status") => {
            let session = &value["session"];
            let state = session["state"].as_str().map(str::to_string);
            Reading {
                status: state.as_deref().map_or(SessionStatus::Unknown, status_of),
                state_age_secs: session["state_age_secs"].as_u64(),
                allele_state: state,
                detail: None,
            }
        }
        Some("error") => {
            let code = value["code"].as_str().unwrap_or_default();
            let message = value["message"].as_str().unwrap_or_default();
            if code == "session_not_found"
                || (code == "bad_request" && message.starts_with(NOT_FOUND_MESSAGE))
            {
                Reading::because(SessionStatus::Gone, "Allele has no session with this id")
            } else {
                Reading::because(
                    SessionStatus::Unknown,
                    format!("Allele answered {code}: {message}"),
                )
            }
        }
        _ => Reading::because(
            SessionStatus::Unknown,
            "Allele's answer was not a status or an error",
        ),
    }
}

/// Allele's `SessionState`, as the strip reads it.
///
/// A state this does not know is `Unknown`, never the nearest guess: Allele is free to add
/// states, and a new one drawn as "working" would be the very mistake the strip exists to
/// stop.
fn status_of(state: &str) -> SessionStatus {
    match state {
        "running" => SessionStatus::Running,
        "awaiting_input" => SessionStatus::AwaitingInput,
        "response_ready" | "idle" => SessionStatus::Finished,
        "suspended" | "done" => SessionStatus::Gone,
        _ => SessionStatus::Unknown,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;
    use std::time::Instant;

    use super::*;

    /// A socket that answers each connection with `reply`, once.
    ///
    /// The directory is short on purpose: a Unix socket path has a limit of about a
    /// hundred bytes, and a test's temporary directory can be most of that.
    fn allele_answering(reply: &'static str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix("rd")
            .tempdir_in("/tmp")
            .expect("a temporary directory");
        let path = dir.path().join("a.sock");
        let listener = UnixListener::bind(&path).expect("binding a socket");
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().expect("cloning"));
                let mut asked = String::new();
                let _ = reader.read_line(&mut asked);
                let mut stream = stream;
                let _ = writeln!(stream, "{reply}");
            }
        });
        (dir, path)
    }

    #[test]
    fn no_socket_file_is_no_allele_and_no_answer() {
        let allele = Allele::at("/tmp/review-desk-no-such-allele.sock");
        assert_eq!(allele.read("s1"), None);
        assert_eq!(Allele::none().read("s1"), None);
    }

    #[test]
    fn each_allele_state_reads_as_the_strip_draws_it() {
        let cases = [
            ("running", SessionStatus::Running),
            ("awaiting_input", SessionStatus::AwaitingInput),
            ("response_ready", SessionStatus::Finished),
            ("idle", SessionStatus::Finished),
            ("suspended", SessionStatus::Gone),
            ("done", SessionStatus::Gone),
            ("a_state_allele_adds_later", SessionStatus::Unknown),
        ];
        for (state, status) in cases {
            let line = format!(
                r#"{{"result":"status","session":{{"session_id":"s1","state":"{state}","state_age_secs":372,"reply_to":"uds:/tmp/x.sock"}}}}"#
            );
            let reading = answer(&line);
            assert_eq!(reading.status, status, "{state}");
            assert_eq!(reading.allele_state.as_deref(), Some(state));
            assert_eq!(reading.state_age_secs, Some(372));
        }
    }

    #[test]
    fn a_status_is_read_over_the_socket() {
        let (_dir, path) = allele_answering(
            r#"{"result":"status","session":{"session_id":"s1","state":"awaiting_input","state_age_secs":372}}"#,
        );
        let reading = Allele::at(&path).read("s1").expect("an answer");
        assert_eq!(reading.status, SessionStatus::AwaitingInput);
        assert_eq!(reading.state_age_secs, Some(372));
    }

    #[test]
    fn an_id_allele_has_never_heard_of_is_gone_under_either_code() {
        let today = answer(
            r#"{"result":"error","code":"bad_request","message":"no session with id nope"}"#,
        );
        assert_eq!(today.status, SessionStatus::Gone);

        let with_its_own_code = answer(
            r#"{"result":"error","code":"session_not_found","message":"no session with id nope"}"#,
        );
        assert_eq!(with_its_own_code.status, SessionStatus::Gone);
    }

    #[test]
    fn any_other_refusal_is_unknown_and_says_what_allele_said() {
        let reading = answer(r#"{"result":"error","code":"internal","message":"it broke"}"#);
        assert_eq!(reading.status, SessionStatus::Unknown);
        assert_eq!(
            reading.detail.as_deref(),
            Some("Allele answered internal: it broke")
        );
        assert_eq!(answer("not json").status, SessionStatus::Unknown);
    }

    #[test]
    fn a_socket_nobody_is_listening_on_is_unreachable() {
        let dir = tempfile::Builder::new()
            .prefix("rd")
            .tempdir_in("/tmp")
            .expect("a temporary directory");
        let path = dir.path().join("a.sock");
        drop(UnixListener::bind(&path).expect("binding a socket"));
        // The file is left behind and nothing accepts on it, which is what a crashed
        // Allele leaves.
        assert!(path.exists());
        let reading = Allele::at(&path).read("s1").expect("an answer");
        assert_eq!(reading.status, SessionStatus::Unreachable);
    }

    #[test]
    fn a_socket_that_never_answers_costs_at_most_the_timeout() {
        let dir = tempfile::Builder::new()
            .prefix("rd")
            .tempdir_in("/tmp")
            .expect("a temporary directory");
        let path = dir.path().join("a.sock");
        let listener = UnixListener::bind(&path).expect("binding a socket");
        let holder = thread::spawn(move || {
            // Accept, and hold the connection open without a word.
            let held = listener.accept();
            thread::sleep(Duration::from_secs(3));
            drop(held);
        });

        let started = Instant::now();
        let reading = Allele::at(&path).read("s1").expect("an answer");
        let took = started.elapsed();

        assert_eq!(reading.status, SessionStatus::Unreachable);
        assert!(took < TIMEOUT + Duration::from_millis(500), "took {took:?}");
        drop(holder);
    }
}
