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
//! ## Why `axum`, and why it is not `tiny_http` any more
//!
//! **This used to be `tiny_http`, and the argument for it was a good one that turned out
//! to be wrong.** It read: `rusqlite` is blocking and the store holds one `Connection`, so
//! an async runtime buys this server nothing and costs a thread hop around every handler;
//! `tiny_http` brings four crates of its own against twenty-seven, and the shape it offers
//! — a `Server` that several threads call `recv()` on — is exactly a small fixed worker
//! pool.
//!
//! Every clause of that is still true. What it missed is that a fixed pool of threads, each
//! of which parks on a keep-alive connection for as long as the browser holds it open, can
//! run out — and that the library's own pool did not merely queue when it ran out, it
//! *lost* the connection. A task could be left in its queue with every thread parked on a
//! socket and nothing able to pick it up, so the request on that connection was never
//! answered and never refused. Measured, on the dashboard's first page load: a silent
//! request in about one cold start in five at six connections, which is what Chromium
//! opens per origin. `tests/concurrency.rs` is that fault, written down.
//!
//! It could not be fixed from here. The pool is internal, the crate sets no socket read
//! timeout and exposes no knob for one, and its last release was October 2022. So the
//! choice was to own a fork of an abandoned HTTP server or to use a maintained one.
//!
//! **Twenty-three more crates is the real cost and it is not nothing.** Measured in this
//! workspace rather than guessed: the server's production dependency tree goes from 30
//! crates to 53. Twenty-seven arrive — `axum`, `tokio`, `hyper`, `tower` and what they
//! rest on — and four leave with `tiny_http`: `ascii`, `chunked_transfer` and `log`.
//!
//! What that buys is a
//! transport that does not lose requests, and a concurrency model with no fixed worker
//! count to get wrong: connections are futures on one thread, and the blocking work —
//! every call into the store — is handed to [`tokio::task::spawn_blocking`] at exactly one
//! place. The original argument against an async runtime was about hop cost, and the hop
//! is still there; it is simply worth paying, which a silent hang on first load is not.

#![deny(missing_debug_implementations)]

pub mod allele;
pub mod api;
pub mod assets;
pub mod error;
pub mod github;
pub mod guard;
pub mod hostname;
pub mod route;
pub mod store;
pub mod wire;

use std::future::{Future, IntoFuture};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Router;
use tokio::runtime::Builder;
use tokio::sync::Notify;

pub use crate::allele::Allele;
pub use crate::api::{Api, API_PREFIX};
pub use crate::error::{ApiError, Code};
pub use crate::github::Github;
pub use crate::hostname::{Hostname, HostnameError};
pub use crate::store::Records;

/// The port the contract's `servers` entry names: `http://127.0.0.1:7777/api/v1`.
pub const DEFAULT_PORT: u16 = 7777;

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

/// The store, the address it is answering on, and any name it also answers to, as every
/// request sees them.
///
/// The address is carried rather than looked up because the guard checks `Host` against
/// the address actually bound, and with port 0 that is not known until it is.
#[derive(Debug, Clone)]
struct Desk {
    api: Api,
    bound: SocketAddr,
    proxied: Option<Hostname>,
}

/// A listening server.
///
/// The socket is bound by [`Server::bind`] and the runtime does not exist yet. That
/// ordering is deliberate: [`Server::address`] has to be able to report the port before
/// anything is served, because port 0 means the operating system chooses it and both the
/// tests and `review-desk serve` print it.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    address: SocketAddr,
    api: Api,
    proxied: Option<Hostname>,
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

        let listener = TcpListener::bind(wanted).map_err(|detail| BindError::Listen {
            port,
            detail: detail.to_string(),
        })?;

        let address = listener.local_addr().unwrap_or(wanted);

        Ok(Server {
            listener,
            address,
            api: Api::new(records, github),
            // Answering to a name is asked for, never assumed.
            proxied: None,
        })
    }

    /// Also answer to one name a reverse proxy in front of this server forwards.
    ///
    /// `None` leaves the server answering only to the address it bound, which is what
    /// every caller that does not pass `--hostname` gets. See [`crate::hostname`] for
    /// what this widens and what it deliberately does not.
    pub fn also_answering_to(mut self, proxied: Option<Hostname>) -> Self {
        self.proxied = proxied;
        self
    }

    /// Read what each review's session is doing from this Allele.
    ///
    /// Not asked for, the server asks no Allele and every review's session reads as
    /// having none — which is what the tests get, and what `review-desk serve` gets on a
    /// machine whose Allele socket is not there. See [`crate::allele`].
    pub fn reading_sessions_from(mut self, allele: Allele) -> Self {
        self.api = self.api.reading_sessions_from(allele);
        self
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
    ///
    /// **Synchronous, and it owns its runtime.** Nothing above this — not `serve.rs`, not
    /// `main` — is async or knows that a runtime exists, which is what keeps the choice of
    /// transport a detail of this file.
    pub fn serve(self) {
        let runtime = runtime();
        runtime.block_on(run(
            self.listener,
            self.address,
            self.api,
            self.proxied,
            // Production stops when the process does.
            std::future::pending::<()>(),
        ));
    }

    /// Answer requests on a background thread, handing back a handle that stops them.
    ///
    /// This is how the tests drive a real server over a real socket: the acceptance
    /// criteria are about what a client sees on `127.0.0.1`, and a test that called the
    /// handlers directly would not have checked that.
    pub fn spawn(self) -> Running {
        let address = self.address;
        let stopping = Arc::new(Notify::new());
        let signal = Arc::clone(&stopping);

        let worker = thread::Builder::new()
            .name("review-desk-http".to_string())
            .spawn(move || {
                let runtime = runtime();
                runtime.block_on(run(
                    self.listener,
                    self.address,
                    self.api,
                    self.proxied,
                    async move {
                        signal.notified().await;
                    },
                ));
            })
            .expect("spawning the HTTP thread");

        Running {
            address,
            stopping,
            worker: Some(worker),
        }
    }
}

/// The runtime every server runs on.
///
/// **One thread, on purpose.** Connections are futures rather than threads, so one thread
/// serves as many at once as a browser cares to open — which is the whole point, since a
/// fixed count of serving threads is what the previous transport ran out of. Everything
/// that blocks goes to [`tokio::task::spawn_blocking`] instead, and that pool grows on
/// demand.
fn runtime() -> tokio::runtime::Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("building the HTTP runtime")
}

/// Serve `listener` until `stop` resolves.
async fn run(
    listener: TcpListener,
    bound: SocketAddr,
    api: Api,
    proxied: Option<Hostname>,
    stop: impl Future<Output = ()>,
) {
    listener
        .set_nonblocking(true)
        .expect("the listening socket could not be made non-blocking");
    let listener = tokio::net::TcpListener::from_std(listener)
        .expect("the listening socket could not join the runtime");

    // One handler for everything. The router does no matching, because `route.rs` already
    // does it from a method and a path and is the contract's own account of what the
    // routes are — a second table here would be a second thing to keep true.
    //
    // **There are no layers.** Not a logging one, not a compression one and emphatically
    // not a permissive CORS one: `guard.rs` is the only thing that decides who may talk
    // to this server, and middleware that answered first, or added headers of its own,
    // would be a second rule in a place nobody looks.
    let app = Router::new().fallback(answer).with_state(Desk {
        api,
        bound,
        proxied,
    });

    let serving = axum::serve(listener, app).into_future();

    tokio::select! {
        outcome = serving => {
            let _ = outcome;
        }
        () = stop => {}
    }
}

/// A server answering requests on its own thread.
#[derive(Debug)]
pub struct Running {
    address: SocketAddr,
    stopping: Arc<Notify>,
    worker: Option<thread::JoinHandle<()>>,
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
        // `notify_one` rather than `notify_waiters`, and the difference is a hang: a
        // handle dropped before the runtime thread has reached its `await` would lose a
        // notification sent to whoever is waiting *now*, and the join below would never
        // return. `notify_one` leaves a permit behind, so the signal cannot be early.
        self.stopping.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Answer one request: the API under [`API_PREFIX`], the embedded dashboard everywhere
/// else.
async fn answer(State(desk): State<Desk>, request: Request) -> Response {
    let (parts, body) = request.into_parts();

    let method = parts.method.as_str().to_string();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or_default().to_string();

    // Collected before the body is read, because the guard decides whether the body is
    // read at all. `into_parts` hands back the body as a stream that nothing has polled,
    // so "before" here is a fact about the code rather than a convention: the bytes are
    // still on the socket until `to_bytes` below, which only runs once the guard passes.
    let headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    // Before anything else, and before the body is touched: a request from somewhere that
    // is not this machine's own browser tab does not get to reach anything.
    //
    // **Above the API branch, not inside it.** It began inside, which left the static
    // branch unguarded — no data leak, since the dashboard bundle is public code, but one
    // rule for the whole server is a thing a reader can check and two rules is not.
    if let Err(refusal) = guard::check(desk.bound, desk.proxied.as_ref(), &method, &headers) {
        return json_response(refusal.status(), &refusal.document());
    }

    if let Some(rest) = api_path(&path) {
        let rest = rest.to_string();
        let body = match read_body(&parts, body).await {
            Ok(body) => body,
            Err(error) => return json_response(error.status(), &error.document()),
        };

        // The one place blocking work leaves the runtime. `Api::answer` takes the store's
        // mutex and talks to SQLite, so running it here would stall every other
        // connection this thread is serving.
        let api = desk.api.clone();
        let answered =
            tokio::task::spawn_blocking(move || api.answer(&method, &rest, &query, &body)).await;

        match answered {
            Ok((status, document)) => json_response(status, &document),
            // The blocking task panicked, which `Records` already turns into a refusal for
            // a poisoned lock. Reaching here means something else did, and the one honest
            // thing to say is that the request was not answered.
            Err(_) => {
                let failed = ApiError {
                    code: Code::StoreUnavailable,
                    message: "the request failed while it was being answered".to_string(),
                    field: None,
                    expected: None,
                };
                json_response(failed.status(), &failed.document())
            }
        }
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
    }
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

/// Read a request body, refusing one that claims to be larger than [`MAX_BODY`].
///
/// The stated length is checked before a byte is read, so a request that announces more
/// than the limit is refused rather than read and then thrown away. `to_bytes` enforces
/// the same limit again on what actually arrives, which is what catches a body that lied
/// about its length or sent none at all.
async fn read_body(parts: &axum::http::request::Parts, body: Body) -> Result<Vec<u8>, ApiError> {
    if let Some(length) = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        if length > MAX_BODY {
            return Err(ApiError::bad_request(format!(
                "the request body is {length} bytes, over the {MAX_BODY} byte limit"
            )));
        }
    }

    to_bytes(body, MAX_BODY as usize)
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| {
            ApiError::bad_request(format!("the request body could not be read: {error}"))
        })
}

/// The headers every response carries, whatever it is.
///
/// **Nothing may frame this server.** The guard stops another origin *talking* to the
/// API; it cannot stop one *embedding* the dashboard, because a click stolen through a
/// transparent iframe produces a request that genuinely comes from the dashboard's own
/// origin with the right content type — every check passes, because nothing is wrong with
/// the request. What is wrong is that the developer did not mean to make it.
///
/// That matters here for the same reason the guard does. The draft screen's approve
/// control posts to `/draft/approve`, and an approved draft is what a review session
/// publishes to GitHub under the developer's name. One stolen click on a draft at `ready`
/// publishes a review they never read, and the draft is immutable afterwards.
///
/// Both headers, not one: `X-Frame-Options` is what older browsers honour and
/// `frame-ancestors` is what the CSP-aware ones prefer. Neither costs anything here —
/// this server has no reason to be embedded anywhere, ever.
fn framing_headers() -> [(HeaderName, HeaderValue); 2] {
    [
        (header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        ),
    ]
}

/// One response, with the content type it was given and the headers everything carries.
///
/// Every response this server sends is built here, which is what makes "the framing
/// headers are on everything" one line rather than a rule to remember at each `return`.
fn respond(status: u16, body: Vec<u8>, mime: &str) -> Response {
    let mut response = Response::new(Body::from(body));

    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    for (name, value) in framing_headers() {
        headers.insert(name, value);
    }

    response
}

/// A JSON response, with the one header a JSON client needs.
fn json_response(status: u16, document: &serde_json::Value) -> Response {
    let body = serde_json::to_vec(document).unwrap_or_else(|_| {
        br#"{"code":"store_unavailable","message":"the response could not be serialised"}"#.to_vec()
    });

    respond(status, body, "application/json; charset=utf-8")
}

/// A response from the embedded dashboard, or the page that says there is none.
fn page_response(path: &str) -> Response {
    let (status, body, mime) = match assets::resolve(path) {
        assets::Served::File { bytes, mime } => (200, bytes.to_vec(), mime),
        // 200, not 404: the binary is working exactly as built, and a 404 here would
        // read as a bug in the server rather than a build without a front end.
        assets::Served::NotBuilt => (
            200,
            assets::NOT_BUILT_PAGE.as_bytes().to_vec(),
            "text/html; charset=utf-8",
        ),
        assets::Served::Missing => (404, b"not found".to_vec(), "text/plain; charset=utf-8"),
    };

    respond(status, body, mime)
}
