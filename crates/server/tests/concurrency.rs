//! Every request is answered, however many connections arrive at once.
//!
//! **This test exists because the server used to lose requests, silently and forever.**
//!
//! A browser opens several connections to one origin — Chromium up to six — and asks for
//! the dashboard's assets down all of them at once. Against the previous HTTP library
//! that burst could leave a connection accepted by the operating system and read by
//! nobody: the request on it was never answered and never refused, so `load` never fired
//! and the dashboard sat blank with no error on screen. Measured at 6 cold starts in 30
//! with six connections, which is roughly one developer in five opening the dashboard for
//! the first time and seeing nothing.
//!
//! Two properties of that failure are what this test is shaped around.
//!
//! **It needs a cold server.** A pool that has already grown escapes it, so a server that
//! has answered anything at all is the wrong server to ask. Every trial binds a new one.
//!
//! **A healthy server is still answering.** A fresh connection always got its reply, so
//! `/health` responded throughout and every liveness check passed while the page hung.
//! That is why this asks the questions *down the connections it already opened* rather
//! than opening another one to see whether the server is up — the latter was green all
//! along and is the reason the fault took four theories to find.
//!
//! It is a probabilistic fault, so one trial proves nothing: [`TRIALS`] of them is what
//! makes a red run reliable. On a server that does not strand connections the whole file
//! is a few hundred milliseconds, because every request is answered immediately and
//! nothing ever waits for [`PATIENCE`].

mod common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, SockAddr, Socket, Type};

/// How many cold starts each connection count is tried.
///
/// Twenty, because the fault is a race and not a certainty. At six connections it struck
/// about half of cold starts and at five about a third, so twenty trials put the chance
/// of a buggy server passing this file at around one in a million — and a test that only
/// *sometimes* catches the bug it was written for is worse than none, because the first
/// green run retires it.
const TRIALS: usize = 20;

/// How long one request may take before it counts as unanswered.
///
/// Generous beyond any honest answer. Every one of these is a `GET` of a fixed string
/// over the loopback interface and lands in single-digit milliseconds; two seconds is
/// here so that a slow continuous-integration runner cannot fail this file by being slow,
/// and nothing else. The fault it looks for is unbounded — those requests are never
/// answered, not answered late — so no finite patience makes this test flaky in the
/// direction of a false red.
const PATIENCE: Duration = Duration::from_secs(2);

/// How long a new server is left alone before the burst.
///
/// Long enough for it to have finished starting, which is the state every real request
/// arrives in and the only state in which the fault can happen at all. Measured: 250
/// in-process trials without this wait found nothing, because a server asked instantly
/// grows to meet the connections instead of queueing them.
const SETTLE: Duration = Duration::from_millis(250);

/// Open `connections` at once and ask each of them a question.
///
/// **Three properties of this shape are load-bearing, and the first two versions of this
/// file got them wrong and were green against the library they were written to indict.**
///
/// *The connections are opened in one tight burst from a single thread*, which is why
/// this reaches for a non-blocking socket rather than `TcpStream::connect`. The fault is
/// decided at the moment connections are accepted, and the accepting side grows to meet
/// connections that arrive far enough apart. A version that opened them from a thread
/// each behind a `Barrier` found nothing in eighty bursts: waking six threads is itself
/// slow enough, and scattered enough, to miss the window. A browser opens its sockets
/// from one event loop, and so does this.
///
/// *Every connection is established before any request is sent.* A connection answered
/// the instant it opens frees whatever was serving it, and that freed capacity is exactly
/// what would pick up a stranded connection. A browser opens its connections and then
/// asks, too.
///
/// *The requests are keep-alive*, so nothing is freed by a connection closing.
fn all_answered(address: SocketAddr, connections: usize) -> Result<(), String> {
    let target = SockAddr::from(address);

    let mut sockets = Vec::with_capacity(connections);
    for index in 0..connections {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, None)
            .map_err(|error| format!("socket {index} could not be made: {error}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|error| format!("socket {index} would not go non-blocking: {error}"))?;
        socket.set_tcp_nodelay(true).ok();

        // Deliberately not checked. A non-blocking connect is *expected* to report that it
        // has not finished yet, and the only answer that matters is whether a reply
        // arrives further down. Waiting here for the handshake is the thing that makes the
        // burst too loose to measure anything.
        let _ = socket.connect(&target);
        sockets.push(socket);
    }

    // `Host` is the address actually bound, so the guard accepts these; what is under test
    // is what the transport does with them, not what the guard thinks of them.
    let request =
        format!("GET /api/v1/health HTTP/1.1\r\nHost: {address}\r\nConnection: keep-alive\r\n\r\n");

    // Only now, with every connection already on its way, is each handshake waited for.
    // Writing to a socket whose non-blocking connect has not finished is refused outright
    // rather than waited for, so this is needed -- but it happens *after* the burst, which
    // is what keeps the burst a burst.
    let deadline = Instant::now() + PATIENCE;
    for (index, socket) in sockets.iter().enumerate() {
        while socket.peer_addr().is_err() {
            if Instant::now() > deadline {
                return Err(format!("connection {index} never finished opening"));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    let mut streams: Vec<TcpStream> = Vec::with_capacity(connections);
    for socket in sockets {
        socket.set_nonblocking(false).ok();
        let stream = TcpStream::from(socket);
        stream.set_read_timeout(Some(PATIENCE)).ok();
        streams.push(stream);
    }

    for (index, stream) in streams.iter_mut().enumerate() {
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("request {index} could not be sent: {error}"))?;
    }

    let mut silent = Vec::new();
    for (index, stream) in streams.iter_mut().enumerate() {
        // Any byte at all is an answer. What the response *says* is every other test's
        // business; this one only asks whether one arrived.
        let mut first = [0_u8; 1];
        if !matches!(stream.read(&mut first), Ok(1)) {
            silent.push(index);
        }
    }

    if silent.is_empty() {
        return Ok(());
    }

    Err(format!(
        "{} of {connections} requests went unanswered for {PATIENCE:?} (connections {silent:?})",
        silent.len(),
    ))
}

/// Try `connections` at once against [`TRIALS`] servers that have never answered anything.
fn none_are_lost(connections: usize) {
    for trial in 1..=TRIALS {
        // A new server every trial. The fault only reaches a pool that is still cold, so
        // reusing one would be measuring the wrong thing and would pass on a server that
        // loses a developer's first page load.
        let desk = common::start_with_no_github();

        // Let the server settle before asking it anything. This is not a politeness: an
        // accepting side that has only just started has not finished standing its
        // threads up, so it takes the "grow to meet them" path and the contention never
        // happens. A developer's server has always been running before they open a
        // browser at it, and a version of this file without this wait was green against
        // a library that loses one first page load in ten.
        thread::sleep(SETTLE);

        if let Err(detail) = all_answered(desk.running.address(), connections) {
            panic!("cold start {trial} of {TRIALS}, {connections} connections at once: {detail}");
        }
    }
}

#[test]
fn five_connections_at_once_all_get_answers() {
    none_are_lost(5);
}

/// Six is the number that matters: it is what Chromium opens per origin.
#[test]
fn six_connections_at_once_all_get_answers() {
    none_are_lost(6);
}

#[test]
fn ten_connections_at_once_all_get_answers() {
    none_are_lost(10);
}
