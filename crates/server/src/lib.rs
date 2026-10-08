//! Review Desk's local HTTP server: the contract's JSON API and the embedded dashboard,
//! from one binary, on the loopback interface.
//!
//! ```no_run
//! use review_desk_harvest::gh::Gh;
//! use review_desk_server::{Records, Server, Github};
//! use review_desk_store::Store;
//!
//! let records = Records::new(Store::open_default()?);
//! let server = Server::bind(7777, records, Github::new(Gh::new()))?;
//! println!("Review Desk is at {}", server.url());
//! server.serve();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## It binds `127.0.0.1` and there is no flag to change that
//!
//! There is no authentication in this version: being able to reach the port is the
//! authorisation, which `contract/README.md` states and justifies. **That is sound only
//! while the bind holds** — on `0.0.0.0`, or through a tunnel, the whole record becomes
//! readable and writable by anyone who can reach it. So the address is a constant, not an
//! option: a flag that could widen it is one typo away from publishing the record, and
//! the version that adds authentication is the version that may add the flag.
//!
//! ## Why `tiny_http`
//!
//! `rusqlite` is blocking and the store holds one `Connection`, so an async runtime would
//! buy this server nothing and cost a thread hop around every handler. `tiny_http` is
//! four transitive crates against fifty for `axum` and `tokio`, and the shape it offers —
//! a `Server` several threads call `recv()` on — is exactly a small fixed worker pool.
//!
//! One worker would be wrong, not merely slow: a browser holds its connection open, so a
//! single accept loop would stop answering while a page sat idle.

#![deny(missing_debug_implementations)]

pub mod api;
pub mod assets;
pub mod error;
pub mod github;
pub mod guard;
pub mod route;
pub mod store;
pub mod wire;

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tiny_http::{Header, Request, Response, StatusCode};

pub use crate::api::{Api, API_PREFIX};
pub use crate::error::{ApiError, Code};
pub use crate::github::Github;
pub use crate::store::Records;

/// The port the contract's `servers` entry names: `http://127.0.0.1:7777/api/v1`.
pub const DEFAULT_PORT: u16 = 7777;

/// How many threads answer requests.
///
/// Small and fixed. One developer in one browser opens a handful of connections, and the
/// store is one connection behind a mutex, so more threads would queue on the same lock.
const WORKERS: usize = 4;

/// How long a worker waits for a request before checking whether it has been told to
/// stop.
///
/// A blocking accept cannot be interrupted reliably — `tiny_http`'s `unblock` wakes a
/// worker, not every worker, so four threads blocked on one listener do not all return.
/// Waiting with a timeout and re-checking a flag is what makes stopping deterministic,
/// which the tests need and a long-running server does not care about.
const ACCEPT_WAIT: Duration = Duration::from_millis(100);

/// The largest request body this server reads.
///
/// A draft body is the biggest thing the contract sends and is a review comment, so this
/// is generous by orders of magnitude. It exists so that a malformed `Content-Length`
/// cannot make the server allocate until the machine gives up.
const MAX_BODY: u64 = 4 * 1024 * 1024;

/// The server could not take the port.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// The port was refused — most often because something else already has it.
    #[error("cannot listen on 127.0.0.1:{port}: {detail}")]
    Listen { port: u16, detail: String },
}

/// A listening server.
pub struct Server {
    listener: Arc<tiny_http::Server>,
    address: SocketAddr,
    api: Api,
    stop: Arc<AtomicBool>,
}

// `tiny_http::Server` is not `Debug`, and the crate denies a missing one, so the address
// stands in for the listener: it is the only thing about it a reader wants anyway.
impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Take `port` on `127.0.0.1`.
    ///
    /// Port 0 asks the operating system for a free one, which is what the tests use so
    /// that two runs at once cannot collide; [`Server::address`] then reports what was
    /// actually bound.
    pub fn bind(port: u16, records: Records, github: Github) -> Result<Self, BindError> {
        // The only address this server ever binds. Written here, once.
        let wanted = SocketAddr::from((Ipv4Addr::LOCALHOST, port));

        let listener = tiny_http::Server::http(wanted).map_err(|detail| BindError::Listen {
            port,
            detail: detail.to_string(),
        })?;

        let address = listener.server_addr().to_ip().unwrap_or(wanted);

        Ok(Server {
            listener: Arc::new(listener),
            address,
            api: Api::new(records, github),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The address actually bound.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The URL to open, which is what `review-desk serve` prints.
    pub fn url(&self) -> String {
        format!("http://{}/", self.address)
    }

    /// Answer requests until the process ends.
    pub fn serve(self) {
        let workers = self.spawn_workers();
        for worker in workers {
            let _ = worker.join();
        }
    }

    /// Answer requests on background threads, handing back a handle that stops them.
    ///
    /// This is how the tests drive a real server over a real socket: the acceptance
    /// criteria are about what a client sees on `127.0.0.1`, and a test that called the
    /// handlers directly would not have checked that.
    pub fn spawn(self) -> Running {
        let address = self.address;
        let listener = Arc::clone(&self.listener);
        let stop = Arc::clone(&self.stop);
        let workers = self.spawn_workers();
        Running {
            listener,
            address,
            stop,
            workers,
        }
    }

    fn spawn_workers(self) -> Vec<thread::JoinHandle<()>> {
        let address = self.address;
        (0..WORKERS)
            .map(|worker| {
                let listener = Arc::clone(&self.listener);
                let stop = Arc::clone(&self.stop);
                let api = self.api.clone();
                thread::Builder::new()
                    .name(format!("review-desk-http-{worker}"))
                    .spawn(move || {
                        while !stop.load(Ordering::SeqCst) {
                            match listener.recv_timeout(ACCEPT_WAIT) {
                                Ok(Some(request)) => answer(&api, address, request),
                                // Nothing arrived inside the wait, so the flag is checked
                                // and the wait begins again.
                                Ok(None) => continue,
                                // The listener is gone. There is nothing left to answer.
                                Err(_) => break,
                            }
                        }
                    })
                    .expect("spawning an HTTP worker thread")
            })
            .collect()
    }
}

/// A server answering requests on its own threads.
pub struct Running {
    listener: Arc<tiny_http::Server>,
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("address", &self.address)
            .field("workers", &self.workers.len())
            .finish_non_exhaustive()
    }
}

impl Running {
    /// The address bound.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The base URL, with no trailing path.
    pub fn base(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // The flag first, then the nudge: a worker that is between waits sees the flag,
        // and one that is inside a wait returns from it within `ACCEPT_WAIT`. Joining is
        // therefore bounded, which is what makes a test that drops this handle finish.
        self.stop.store(true, Ordering::SeqCst);
        self.listener.unblock();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Answer one request: the API under [`API_PREFIX`], the embedded dashboard everywhere
/// else.
fn answer(api: &Api, bound: SocketAddr, mut request: Request) {
    let (path, query) = split_url(request.url());
    let path = path.to_string();
    let query = query.to_string();
    let method = request.method().as_str().to_string();

    // Collected before the body is read, because the guard decides whether the body is
    // read at all.
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|header| (header.field.to_string(), header.value.to_string()))
        .collect();

    let served = if let Some(rest) = api_path(&path) {
        // Before anything else, and before the body is touched: a request from somewhere
        // that is not this machine's own browser tab does not get to reach a handler.
        if let Err(refusal) = guard::check(bound, &method, &headers) {
            let _ = request.respond(json_response(refusal.status(), &refusal.document()));
            return;
        }

        let body = match read_body(&mut request) {
            Ok(body) => body,
            Err(error) => {
                let _ = request.respond(json_response(error.status(), &error.document()));
                return;
            }
        };
        let (status, document) = api.answer(&method, rest, &query, &body);
        json_response(status, &document)
    } else if method == "GET" || method == "HEAD" {
        page_response(&path)
    } else {
        // The dashboard is static files. Anything that is not a read of one is a mistake
        // about where the API lives, and saying so is more use than a bare 405.
        json_response(
            404,
            &ApiError::not_found(format!(
                "{method} {path} is not an endpoint; the API is under {API_PREFIX}"
            ))
            .document(),
        )
    };

    let _ = request.respond(served);
}

/// The part of a path after the API prefix, when the path is an API path at all.
///
/// The boundary matters more than it looks. A bare `strip_prefix` makes any path that
/// merely *starts with* `/api/v1` an API call, so `POST /api/v1reviews` would strip to
/// `reviews` and really create a review, and `GET /api/v1health` would answer the health
/// endpoint. The remainder has to be empty or start with a separator.
fn api_path(path: &str) -> Option<&str> {
    match path.strip_prefix(API_PREFIX) {
        Some("") => Some("/"),
        Some(rest) if rest.starts_with('/') => Some(rest),
        _ => None,
    }
}

/// The path and the query string, separately.
fn split_url(url: &str) -> (&str, &str) {
    url.split_once('?').unwrap_or((url, ""))
}

/// Read a request body, refusing one that claims to be larger than [`MAX_BODY`].
fn read_body(request: &mut Request) -> Result<Vec<u8>, ApiError> {
    if let Some(length) = request.body_length() {
        if length as u64 > MAX_BODY {
            return Err(ApiError::bad_request(format!(
                "the request body is {length} bytes, over the {MAX_BODY} byte limit"
            )));
        }
    }

    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY)
        .read_to_end(&mut body)
        .map_err(|error| {
            ApiError::bad_request(format!("the request body could not be read: {error}"))
        })?;
    Ok(body)
}

/// A JSON response, with the one header a JSON client needs.
fn json_response(status: u16, document: &serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(document).unwrap_or_else(|_| {
        br#"{"code":"store_unavailable","message":"the response could not be serialised"}"#.to_vec()
    });

    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
}

/// A response from the embedded dashboard, or the page that says there is none.
fn page_response(path: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    match assets::resolve(path) {
        assets::Served::File { bytes, mime } => Response::from_data(bytes.to_vec())
            .with_status_code(StatusCode(200))
            .with_header(header("Content-Type", mime)),
        // 200, not 404: the binary is working exactly as built, and a 404 here would
        // read as a bug in the server rather than a build without a front end.
        assets::Served::NotBuilt => Response::from_data(assets::NOT_BUILT_PAGE.as_bytes().to_vec())
            .with_status_code(StatusCode(200))
            .with_header(header("Content-Type", "text/html; charset=utf-8")),
        assets::Served::Missing => Response::from_data(b"not found".to_vec())
            .with_status_code(StatusCode(404))
            .with_header(header("Content-Type", "text/plain; charset=utf-8")),
    }
}

/// One header, or none if the value cannot be one.
fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .unwrap_or_else(|_| panic!("{name} is a valid header name"))
}
