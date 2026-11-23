//! What a review's Allele session is doing, read from Allele's control socket.
//!
//! **The one other program this server talks to, and almost all of it is asking.** Review Desk
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
//! - **It reads, and it asks Allele to hand over one notice.** The requests are
//!   `sessions_status`, `prompts_verify` for `review-desk post` (DEV-1347), and
//!   `sessions_prompt` (DEV-1383), and nothing else. The session's `reply_to` address is
//!   read off the answer and dropped. Reversed on 23 November 2026 from "never writes into
//!   a session": with Allele present, Review Desk asks Allele to put a notice in front of
//!   the review's session when the developer asks a question, and Allele owns the delivery
//!   — Review Desk never speaks Claude Code's own protocol, and never starts, resumes or
//!   drives Claude. What it sends is a notice that points at the record, never the
//!   developer's words: see [`crate::handover`].
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

/// How long a handover may take, from connecting to Allele's answer — DEV-1383.
///
/// Fifteen seconds, not [`TIMEOUT`]'s one: Allele answers `sessions_prompt` only once it has
/// seen the session start on the notice, which takes as long as Claude takes to begin. It is
/// spent on the handover thread alone, never on a request someone is waiting for.
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(15);

/// The `allele.ext/1` contract a handover asks for.
const CONTRACT: &str = "allele.ext/1";

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

    /// Allele's socket under the developer's own home directory — the one place a go-ahead
    /// is checked (DEV-1347).
    ///
    /// **From the passwd entry, not `HOME`.** `HOME` is the caller's to set, and the caller
    /// is the session whose word is being checked: pointing it at a directory holding a
    /// socket of its own would be enough to have its go-ahead vouched for. A test build with
    /// `fake-allele` reads [`FAKE_ALLELE_HOME`] instead; a release build cannot have that.
    pub fn own_socket() -> Option<PathBuf> {
        #[cfg(feature = "fake-allele")]
        if let Some(home) = std::env::var_os(FAKE_ALLELE_HOME).filter(|home| !home.is_empty()) {
            return Some(Path::new(&home).join(".allele").join("control.sock"));
        }
        passwd_home().map(|home| home.join(".allele").join("control.sock"))
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

fn ask(socket: &Path, session_id: &str) -> Reading {
    let request = serde_json::json!({ "op": "sessions_status", "session_id": session_id });
    match exchange(socket, &request) {
        Ok(line) => answer(&line),
        Err(detail) => Reading::because(SessionStatus::Unreachable, detail),
    }
}

/// Send one request line and read one answer line, or say what went wrong reaching Allele.
fn exchange(socket: &Path, request: &serde_json::Value) -> Result<String, String> {
    let mut connection = Connection::open(socket, TIMEOUT)?;
    connection.ask(request)
}

/// The longest answer line read from Allele. Its answers are a few hundred bytes; anything
/// past this is not an answer, and reading it would let a broken socket grow a buffer forever.
const MOST_ANSWER_BYTES: usize = 64 * 1024;

/// One connection to Allele's socket, on which each request line is answered by one line.
///
/// **`timeout` is a deadline for the whole connection, not for each read** (DEV-1383 review,
/// F3): every request and answer on it shares the one budget, so an Allele that answers a
/// byte at a time, or a handover that says hello and then prompts, still costs at most
/// `timeout` from connecting.
#[cfg(unix)]
struct Connection {
    stream: std::os::unix::net::UnixStream,
    /// Bytes read past the end of the last answer line.
    pending: Vec<u8>,
    deadline: std::time::Instant,
    timeout: Duration,
}

#[cfg(unix)]
impl Connection {
    fn open(socket: &Path, timeout: Duration) -> Result<Self, String> {
        use std::os::unix::net::UnixStream;

        let deadline = std::time::Instant::now() + timeout;
        let stream = UnixStream::connect(socket)
            .map_err(|error| format!("Allele's socket refused the connection: {error}"))?;
        Ok(Connection {
            stream,
            pending: Vec::new(),
            deadline,
            timeout,
        })
    }

    /// What is left of the deadline, or the error that says it has passed.
    fn left(&self) -> Result<Duration, String> {
        let left = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(self.late());
        }
        Ok(left)
    }

    fn late(&self) -> String {
        format!("Allele did not answer within {} s", self.timeout.as_secs())
    }

    fn ask(&mut self, request: &serde_json::Value) -> Result<String, String> {
        use std::io::{Read, Write};

        // Both directions, so a socket that accepts and never reads cannot hold a write either.
        let left = self.left()?;
        self.stream
            .set_write_timeout(Some(left))
            .map_err(|error| error.to_string())?;
        writeln!(self.stream, "{request}")
            .map_err(|error| format!("could not ask Allele: {error}"))?;

        let mut chunk = [0_u8; 4096];
        loop {
            if let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                return String::from_utf8(line)
                    .map_err(|_| "Allele's answer was not text".to_string());
            }
            if self.pending.len() > MOST_ANSWER_BYTES {
                return Err(format!(
                    "Allele's answer ran past {MOST_ANSWER_BYTES} bytes without ending"
                ));
            }
            let left = self.left()?;
            self.stream
                .set_read_timeout(Some(left))
                .map_err(|error| error.to_string())?;
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err("Allele closed the connection without answering".to_string()),
                Ok(read) => self.pending.extend_from_slice(&chunk[..read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(self.late())
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(format!("could not read Allele's answer: {error}")),
            }
        }
    }
}

#[cfg(not(unix))]
struct Connection;

#[cfg(not(unix))]
impl Connection {
    fn open(_socket: &Path, _timeout: Duration) -> Result<Self, String> {
        Err("Allele's socket is a Unix socket, which this platform cannot open".to_string())
    }

    fn ask(&mut self, _request: &serde_json::Value) -> Result<String, String> {
        unreachable!("no connection is ever opened on this platform")
    }
}

/// Refuse a verifier that is not Allele's own kind of socket: a Unix socket only its owner
/// can open (mode 0600), as Allele creates it.
///
/// **Not a defence against a process running as the developer**, which can make such a
/// socket too, or write the database. What it rules out is a world-writable path or a plain
/// file standing in for Allele.
#[cfg(unix)]
fn owners_socket(socket: &Path) -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let metadata = std::fs::metadata(socket).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_socket() {
        return Err(format!("{} is not a socket", socket.display()));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o600 {
        return Err(format!(
            "{} is mode {mode:o}, and Allele's socket is 600",
            socket.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn owners_socket(_socket: &Path) -> Result<(), String> {
    Err("Allele's socket is a Unix socket, which this platform cannot open".to_string())
}

/// Where a `fake-allele` build looks for an Allele instead of the developer's home.
#[cfg(feature = "fake-allele")]
pub const FAKE_ALLELE_HOME: &str = "REVIEW_DESK_FAKE_ALLELE_HOME";

// **A test-only seam, and this is what keeps it one.** A release binary with it would let the
// environment choose the Allele that vouches for a go-ahead.
#[cfg(all(feature = "fake-allele", not(debug_assertions)))]
compile_error!(
    "the `fake-allele` feature lets the environment choose which Allele vouches for a \
     go-ahead, and is never built in release"
);

/// The current user's home directory, as the passwd entry names it.
#[cfg(unix)]
fn passwd_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    // SAFETY: `getpwuid_r` is the re-entrant lookup. It writes only into `entry` and into
    // `buffer`, whose length is passed with it; `found` is set to `&entry` on success and to
    // null otherwise. `pw_dir` points into `buffer`, which outlives the copy taken below.
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let status = libc::getpwuid_r(
            libc::geteuid(),
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut found,
        );
        if status != 0 || found.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        let dir = CStr::from_ptr(entry.pw_dir).to_bytes();
        (!dir.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(dir)))
    }
}

#[cfg(not(unix))]
fn passwd_home() -> Option<PathBuf> {
    None
}

/// Whether Allele confirms that the developer typed a go-ahead in a session — DEV-1347.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoAhead {
    /// Allele says, without qualification, that this line was typed in this session.
    Typed,
    /// Anything else, and why. The post's basis is `desk`.
    Unconfirmed(String),
}

impl Allele {
    /// Ask Allele whether `line` was typed by the developer into `session_id`.
    ///
    /// **Only an unqualified yes is [`GoAhead::Typed`].** Allele has no prompt check yet
    /// (allele-05-20, `prompts.verify`): today it answers this op with an error, which reads
    /// as unconfirmed — and so does no socket, a socket that does not answer, an answer that
    /// is not JSON, and an answer in any shape but `{"result": "prompt_verified", "typed":
    /// true}`. The basis a post records falls toward the weaker label on every one of
    /// them; it is never guessed upward.
    pub fn verify_go_ahead(&self, session_id: &str, line: &str, at: Option<i64>) -> GoAhead {
        let Some(socket) = self.socket.as_deref().filter(|socket| socket.exists()) else {
            return GoAhead::Unconfirmed("there is no Allele on this machine to ask".to_string());
        };
        if let Err(why) = owners_socket(socket) {
            return GoAhead::Unconfirmed(why);
        }
        let request = serde_json::json!({
            "op": "prompts_verify",
            "session_id": session_id,
            "text": line,
            "typed_at": at,
        });
        let line = match exchange(socket, &request) {
            Ok(line) => line,
            Err(detail) => return GoAhead::Unconfirmed(detail),
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            return GoAhead::Unconfirmed("Allele's answer was not JSON".to_string());
        };
        match (value["result"].as_str(), value["typed"].as_bool()) {
            (Some("prompt_verified"), Some(true)) => GoAhead::Typed,
            (Some("error"), _) => GoAhead::Unconfirmed(format!(
                "Allele has no prompt check it would answer: {} {}",
                value["code"].as_str().unwrap_or_default(),
                value["message"].as_str().unwrap_or_default()
            )),
            _ => GoAhead::Unconfirmed("Allele did not confirm the line was typed".to_string()),
        }
    }
}

/// What Allele said when asked to hand a notice to a session — DEV-1383.
///
/// The refusals are DEV-1382's codes, folded to what Review Desk does about each: try again
/// on the next sweep, hold off until the session's state changes, or note an Allele too old
/// to be asked. **[`Handed::Prompted`] and [`Handed::Uncertain`] are recorded**: the second
/// because the notice may well be in front of the session, so the two-minute spacing and the
/// cap of three have to apply to it too (DEV-1383 review, F1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handed {
    /// Allele put the notice in front of the session and saw it start.
    Prompted,
    /// `session_busy`: the session is working. The next sweep tries again.
    Busy,
    /// `session_awaiting_person`: a permission prompt or a menu is open, and only a person
    /// can answer it. Held until the session's state changes.
    AwaitingPerson,
    /// `session_not_live`, `not_dispatched` or `session_not_found`, by its code. Nothing
    /// Allele can deliver to; the watcher is the fallback. Held until the state changes.
    NotLive(String),
    /// An Allele that has no `sessions_prompt` to give: one from before `allele.ext/1`, one
    /// that does not know the `prompt` capability, or one that did not grant it.
    OlderAllele(String),
    /// The socket is there and could not be asked: it refused, or did not answer the
    /// `hello`. Nothing was sent to the session. The next sweep tries again.
    Unreachable(String),
    /// **The `sessions_prompt` was sent and what came back does not say it was not
    /// delivered**: `prompt_delivery_unconfirmed` (DEV-1382: pasted, but Allele did not see
    /// the session start), a code this does not know, an answer that is not one, or no
    /// answer before the deadline. Recorded as a handover, so it is not pasted again within
    /// two minutes, nor more than three times.
    Uncertain(String),
    /// Allele refused before anything reached the session, for a reason not named above —
    /// an Allele that is not one, or a `bad_request`. Nothing is recorded and the next sweep
    /// tries again.
    Refused(String),
}

impl Allele {
    /// Ask Allele to hand `notice` to `session_id`, or `None` when there is no Allele here.
    ///
    /// **Blocks for up to [`PROMPT_TIMEOUT`]**, so it is for the handover thread and nothing
    /// else. One connection: an `allele.ext/1` `hello` asking for `observe` and `prompt`,
    /// then the `sessions_prompt` itself.
    ///
    /// **Only to Allele's own kind of socket** — owner-only, as [`Allele::verify_go_ahead`]
    /// asks of one (DEV-1383 review, F4): this is the one request that puts anything in front
    /// of a session, and a listener that is not Allele could otherwise say `prompted` and
    /// spend a question's three handovers.
    pub fn prompt(&self, session_id: &str, notice: &str) -> Option<Handed> {
        let socket = self.socket.as_deref().filter(|socket| socket.exists())?;
        if let Err(why) = owners_socket(socket) {
            return Some(Handed::Refused(why));
        }
        Some(hand_over(socket, session_id, notice))
    }
}

fn hand_over(socket: &Path, session_id: &str, notice: &str) -> Handed {
    let mut connection = match Connection::open(socket, PROMPT_TIMEOUT) {
        Ok(connection) => connection,
        Err(detail) => return Handed::Unreachable(detail),
    };

    let hello = serde_json::json!({
        "op": "hello",
        "contract": CONTRACT,
        "client": { "id": "review-desk", "version": env!("CARGO_PKG_VERSION") },
        "capabilities": ["observe", "prompt"],
    });
    let greeted = match connection.ask(&hello).map(|line| parsed(&line)) {
        Ok(Some(value)) => value,
        Ok(None) => return Handed::Refused("Allele's answer to hello was not JSON".to_string()),
        Err(detail) => return Handed::Unreachable(detail),
    };
    match greeted["result"].as_str() {
        Some("hello") => {}
        // A pre-`ext/1` Allele answers `hello` with `bad_request`, and one that knows `ext/1`
        // but not the `prompt` capability cannot parse a hello asking for it.
        Some("error")
            if matches!(
                greeted["code"].as_str(),
                Some("bad_request" | "unsupported_contract")
            ) =>
        {
            return Handed::OlderAllele(said(&greeted))
        }
        Some("error") => return Handed::Refused(said(&greeted)),
        _ => return Handed::Refused("Allele's answer to hello was not a hello".to_string()),
    }
    let granted = greeted["granted"]
        .as_array()
        .is_some_and(|granted| granted.iter().any(|one| one == "prompt"));
    if !granted {
        return Handed::OlderAllele("Allele did not grant the prompt capability".to_string());
    }

    let request = serde_json::json!({
        "op": "sessions_prompt",
        "session_id": session_id,
        "prompt": notice,
    });
    // From here the notice may be in front of the session, so nothing below is "not sent".
    let answered = match connection.ask(&request).map(|line| parsed(&line)) {
        Ok(Some(value)) => value,
        Ok(None) => return Handed::Uncertain("Allele's answer was not JSON".to_string()),
        Err(detail) => return Handed::Uncertain(detail),
    };
    match answered["result"].as_str() {
        Some("prompted") => Handed::Prompted,
        Some("error") => refusal(&answered),
        _ => Handed::Uncertain("Allele's answer was not prompted or an error".to_string()),
    }
}

fn parsed(line: &str) -> Option<serde_json::Value> {
    serde_json::from_str(line.trim()).ok()
}

/// An error answer, as `code: message`.
fn said(value: &serde_json::Value) -> String {
    format!(
        "{}: {}",
        value["code"].as_str().unwrap_or_default(),
        value["message"].as_str().unwrap_or_default()
    )
}

/// One of DEV-1382's refusals, as what Review Desk does about it.
fn refusal(value: &serde_json::Value) -> Handed {
    let code = value["code"].as_str().unwrap_or_default();
    let message = value["message"].as_str().unwrap_or_default();
    match code {
        "session_busy" => Handed::Busy,
        "session_awaiting_person" => Handed::AwaitingPerson,
        "session_not_live" | "not_dispatched" | "session_not_found" => {
            Handed::NotLive(code.to_string())
        }
        // Today's Allele names an unknown id with `bad_request`; see `NOT_FOUND_MESSAGE`.
        "bad_request" if message.starts_with(NOT_FOUND_MESSAGE) => {
            Handed::NotLive("session_not_found".to_string())
        }
        // An `ext/1` Allele that granted `prompt` but has no such op parses the request and
        // refuses it whole.
        "capability_not_granted" => Handed::OlderAllele(said(value)),
        "bad_request" if message.contains("sessions_prompt") => Handed::OlderAllele(said(value)),
        // DEV-1382 answers `bad_request` before it writes anything: an empty or long prompt.
        "bad_request" => Handed::Refused(said(value)),
        // `prompt_delivery_unconfirmed`, and any code DEV-1382 did not name: it may have been
        // pasted.
        _ => Handed::Uncertain(said(value)),
    }
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

    /// The passwd entry names a home, and it is an absolute path — the one `own_socket`
    /// builds Allele's socket under whatever `HOME` says.
    #[test]
    fn the_passwd_entry_names_an_absolute_home() {
        let home = passwd_home().expect("the current user has a passwd entry");
        assert!(home.is_absolute(), "{}", home.display());
    }

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

    /// A socket that answers each line of one connection with the next of `replies`, and
    /// hands back every line it was sent.
    fn allele_conversing(
        replies: &'static [&'static str],
    ) -> (
        tempfile::TempDir,
        PathBuf,
        std::sync::mpsc::Receiver<Vec<String>>,
    ) {
        let dir = tempfile::Builder::new()
            .prefix("rd")
            .tempdir_in("/tmp")
            .expect("a temporary directory");
        let path = dir.path().join("a.sock");
        let listener = UnixListener::bind(&path).expect("binding a socket");
        owner_only(&path);
        let (sent, heard) = std::sync::mpsc::channel();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().expect("cloning"));
                let mut stream = stream;
                let mut lines = Vec::new();
                for reply in replies {
                    let mut asked = String::new();
                    if reader.read_line(&mut asked).unwrap_or(0) == 0 {
                        break;
                    }
                    lines.push(asked.trim().to_string());
                    let _ = writeln!(stream, "{reply}");
                }
                let _ = sent.send(lines);
            }
        });
        (dir, path, heard)
    }

    /// Mode 0600, as Allele makes its socket, which is what [`Allele::prompt`] asks for.
    fn owner_only(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("making the socket owner-only");
    }

    const HELLO: &str = r#"{"result":"hello","contract_versions":["allele.ext/1"],"epoch":"e1","granted":["observe","prompt"],"dispatch_limits":null}"#;

    #[test]
    fn a_prompt_says_hello_asking_for_prompt_then_sends_the_notice() {
        let (_dir, path, heard) = allele_conversing(&[
            HELLO,
            r#"{"result":"prompted","session":{"session_id":"9f2c","name":"Review","was_state":"response_ready","now_state":"running"}}"#,
        ]);
        let handed = Allele::at(&path).prompt("9f2c", "Review Desk: a notice");
        assert_eq!(handed, Some(Handed::Prompted));

        let lines = heard.recv().expect("what Allele was sent");
        let hello: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(hello["op"], "hello");
        assert_eq!(hello["contract"], "allele.ext/1");
        assert_eq!(hello["client"]["id"], "review-desk");
        assert_eq!(
            hello["capabilities"],
            serde_json::json!(["observe", "prompt"])
        );
        let prompt: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(
            prompt,
            serde_json::json!({
                "op": "sessions_prompt",
                "session_id": "9f2c",
                "prompt": "Review Desk: a notice",
            })
        );
    }

    #[test]
    fn each_refusal_reads_as_what_review_desk_does_about_it() {
        let cases = [
            (
                r#"{"result":"error","code":"session_busy","message":"running"}"#,
                Handed::Busy,
            ),
            (
                r#"{"result":"error","code":"session_awaiting_person","message":"menu"}"#,
                Handed::AwaitingPerson,
            ),
            (
                r#"{"result":"error","code":"session_not_live","message":"done"}"#,
                Handed::NotLive("session_not_live".to_string()),
            ),
            (
                r#"{"result":"error","code":"not_dispatched","message":"a person's"}"#,
                Handed::NotLive("not_dispatched".to_string()),
            ),
            (
                r#"{"result":"error","code":"session_not_found","message":"no session with id x"}"#,
                Handed::NotLive("session_not_found".to_string()),
            ),
            (
                r#"{"result":"error","code":"bad_request","message":"no session with id x"}"#,
                Handed::NotLive("session_not_found".to_string()),
            ),
            (
                r#"{"result":"error","code":"capability_not_granted","message":"prompt"}"#,
                Handed::OlderAllele("capability_not_granted: prompt".to_string()),
            ),
            (
                r#"{"result":"error","code":"prompt_delivery_unconfirmed","message":"never started"}"#,
                Handed::Uncertain("prompt_delivery_unconfirmed: never started".to_string()),
            ),
            (
                r#"{"result":"error","code":"a_code_allele_adds_later","message":"?"}"#,
                Handed::Uncertain("a_code_allele_adds_later: ?".to_string()),
            ),
            (
                r#"{"result":"error","code":"bad_request","message":"prompt starts with /"}"#,
                Handed::Refused("bad_request: prompt starts with /".to_string()),
            ),
            (
                "not json",
                Handed::Uncertain("Allele's answer was not JSON".to_string()),
            ),
        ];
        for (reply, expected) in cases {
            let replies: &'static [&'static str] = Box::leak(Box::new([HELLO, reply]));
            let (_dir, path, _heard) = allele_conversing(replies);
            assert_eq!(
                Allele::at(&path).prompt("s1", "n"),
                Some(expected),
                "{reply}"
            );
        }
    }

    #[test]
    fn an_allele_that_refuses_the_hello_is_older_and_is_sent_no_prompt() {
        for refusal in [
            // Before `ext/1`, `hello` is an op nobody knows.
            r#"{"result":"error","code":"bad_request","message":"could not parse request: unknown variant `hello`"}"#,
            // An `ext/1` Allele that does not know `prompt` cannot parse a hello asking for it.
            r#"{"result":"error","code":"bad_request","message":"could not parse hello: unknown variant `prompt`"}"#,
        ] {
            let replies: &'static [&'static str] = Box::leak(Box::new([refusal]));
            let (_dir, path, heard) = allele_conversing(replies);
            assert!(matches!(
                Allele::at(&path).prompt("s1", "n"),
                Some(Handed::OlderAllele(_))
            ));
            assert_eq!(heard.recv().unwrap().len(), 1, "nothing after the hello");
        }
    }

    #[test]
    fn an_allele_that_does_not_grant_prompt_is_older_and_is_sent_no_prompt() {
        let (_dir, path, heard) = allele_conversing(&[
            r#"{"result":"hello","contract_versions":["allele.ext/1"],"epoch":"e1","granted":["observe"],"dispatch_limits":null}"#,
        ]);
        assert!(matches!(
            Allele::at(&path).prompt("s1", "n"),
            Some(Handed::OlderAllele(_))
        ));
        assert_eq!(heard.recv().unwrap().len(), 1);
    }

    #[test]
    fn no_socket_file_is_no_prompt_attempted() {
        assert_eq!(
            Allele::at("/tmp/review-desk-no-such-allele.sock").prompt("s1", "n"),
            None
        );
        assert_eq!(Allele::none().prompt("s1", "n"), None);
    }

    /// F3 of the review: the fifteen seconds cover the whole handover, not each read.
    #[test]
    fn a_handover_that_is_answered_a_byte_at_a_time_still_ends_at_the_deadline() {
        let dir = tempfile::Builder::new()
            .prefix("rd")
            .tempdir_in("/tmp")
            .expect("a temporary directory");
        let path = dir.path().join("a.sock");
        let listener = UnixListener::bind(&path).expect("binding a socket");
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut stream = stream;
                // Never a newline, and never quiet for as long as one read's timeout.
                for _ in 0..200 {
                    if stream.write_all(b" ").is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let started = Instant::now();
        let mut connection =
            Connection::open(&path, Duration::from_millis(400)).expect("connecting");
        let answered = connection.ask(&serde_json::json!({ "op": "hello" }));
        assert!(answered.is_err(), "{answered:?}");
        assert!(
            started.elapsed() < Duration::from_millis(900),
            "took {:?}",
            started.elapsed()
        );
    }

    /// F4 of the review: a socket anyone can open is not Allele's, and is sent no notice.
    #[test]
    fn a_socket_that_is_not_owner_only_is_sent_no_notice() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path, heard) = allele_conversing(&[HELLO]);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(matches!(
            Allele::at(&path).prompt("s1", "n"),
            Some(Handed::Refused(_))
        ));
        drop(heard);
    }

    #[test]
    fn a_prompt_waits_longer_than_a_status_read() {
        assert_eq!(PROMPT_TIMEOUT, Duration::from_secs(15));
        assert_eq!(TIMEOUT, Duration::from_secs(1));
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
