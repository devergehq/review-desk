//! What a request has to prove before it reaches a handler.
//!
//! **This server has no authentication: reachability is the authorisation.** That is the
//! contract's position and it is sound — right up to the point where something *else*
//! running on the developer's machine can reach it on their behalf. A web page open in
//! their browser is exactly that thing.
//!
//! It matters here more than it would on most local servers, because of what this API is.
//! Confirming a brief and approving a draft happen here and nowhere else, and an approved
//! draft is what a review session reads and posts to GitHub **under the developer's
//! name**. A page on `evil.example` that can `POST` to `127.0.0.1:7777` can approve a
//! draft the developer has not read. The decision this project rests on — only the
//! developer signs off — would then be enforced by nothing.
//!
//! Three checks, each closing a different route in:
//!
//! | Check | Closes |
//! |---|---|
//! | `Host` is the address actually bound, or the one host `--hostname` named | DNS rebinding: a name the attacker controls, resolved to 127.0.0.1 |
//! | `Origin`, when present, is this server's own | A page on another origin driving `fetch` or submitting a form |
//! | `Content-Type` is JSON on a write | A plain HTML form, which cannot send one |
//!
//! Each is load-bearing on its own. A form post carries an `Origin` today, so the second
//! covers it — but the third is what makes that a property of the request shape rather
//! than of browser behaviour that could change. **No CORS headers are sent**, by omission
//! and on purpose: there is no origin this API wants to be readable from.
//!
//! **One name may be added to the first check, and nothing may be added to the others.**
//! `review-desk serve --hostname app.review-desk.local` tells this module the single host a
//! reverse proxy in front of it forwards, which is how the dashboard is reached at a name
//! instead of at a port. It is an argument that has to be typed, not a list, an
//! environment variable or a build flag, and `crate::hostname` is where the difference is
//! argued. `crates/server/tests/cross_origin.rs` is the tripwire that keeps it that way.
//!
//! ## What this does not stop
//!
//! **Another process on the same machine.** A local program can set any header it likes,
//! so none of this is a defence against code already running as the developer. That is
//! the authorisation model the contract states, and closing it needs authentication,
//! which is out of scope for this version. What these checks buy is the narrower and more
//! valuable thing: a *web page* cannot reach the API, and a web page is the one attacker
//! that arrives without the developer installing anything.

use std::net::SocketAddr;

use crate::error::ApiError;
use crate::hostname::Hostname;

/// A request's headers, as name and value.
pub type Headers<'a> = &'a [(String, String)];

/// Find one header, case-insensitively. HTTP header names are not case-sensitive and a
/// check that assumed they were would be bypassed by changing one letter.
fn header<'a>(headers: Headers<'a>, wanted: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        .map(|(_, value)| value.as_str())
}

/// The host-and-port forms of the address the listener bound.
///
/// `localhost` is allowed beside the literal address, and that is safe rather than a
/// convenience hole: DNS rebinding works by pointing a name *the attacker controls* at
/// 127.0.0.1, and the browser then sends that name — `evil.example` — in `Host`. It
/// cannot make the browser send `localhost`.
fn loopback_hosts(bound: SocketAddr) -> [String; 3] {
    let port = bound.port();
    // Three exact strings, never a suffix match. The exactness does more work than it
    // looks: it also closes `0.0.0.0:{port}` — which routes to loopback and is the known
    // bypass for local servers — along with `[::ffff:127.0.0.1]`, `foo.localhost` and
    // `localhost.` with a trailing dot. `[::1]` cannot currently arrive, because the
    // listener binds IPv4 loopback only; it is listed so that binding IPv6 later does not
    // silently become a refusal.
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
}

/// The host-and-port forms that name this server.
///
/// The address bound, and — when `review-desk serve --hostname` named one — the single
/// host a reverse proxy in front of this server forwards. Still exact strings and still
/// never a suffix match, so naming `app.review-desk.local` does not make
/// `evil.app.review-desk.local` this server. See [`crate::hostname`] for why one named host
/// is not the configurable guard `cross_origin.rs` forbids.
fn own_hosts(bound: SocketAddr, proxied: Option<&Hostname>) -> Vec<String> {
    let mut hosts = loopback_hosts(bound).to_vec();
    if let Some(name) = proxied {
        hosts.push(name.host().to_string());
    }
    hosts
}

/// The origins that are this server.
///
/// The bound address is `http`, because that is what it serves. A named host is `https`
/// and only `https`: it is reached through the proxy, which terminates TLS and sends port
/// 80 to 443, so no browser sending that `Host` is on any other scheme.
fn own_origins(bound: SocketAddr, proxied: Option<&Hostname>) -> Vec<String> {
    let mut origins: Vec<String> = loopback_hosts(bound)
        .iter()
        .map(|host| format!("http://{host}"))
        .collect();
    if let Some(name) = proxied {
        origins.push(name.origin());
    }
    origins
}

/// Whether this method changes the record.
fn is_a_write(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

/// Check a request bound for the API.
///
/// `proxied` is the single host `review-desk serve --hostname` named, and is `None`
/// unless it was typed. It is the only thing this module is told beyond the address the
/// listener bound; everything else it decides from the request.
///
/// Returns the refusal to serve, or `Ok(())`.
pub fn check(
    bound: SocketAddr,
    proxied: Option<&Hostname>,
    method: &str,
    headers: Headers<'_>,
) -> Result<(), ApiError> {
    // --- Host -----------------------------------------------------------------
    //
    // Absent is refused as firmly as wrong. HTTP/1.1 requires a Host, every browser
    // sends one, and treating "no Host" as "must be fine" would make the check optional
    // for anything that simply left it off.
    let host = header(headers, "host").unwrap_or_default();
    let hosts = own_hosts(bound, proxied);
    if !hosts.iter().any(|own| host.eq_ignore_ascii_case(own)) {
        return Err(ApiError::bad_request(format!(
            "this server answers only to {}, and this request named {host:?}: \
             a name that resolves to the loopback address is not this server",
            hosts.join(", ")
        )));
    }

    // --- Origin ---------------------------------------------------------------
    //
    // Checked on every method, not only on writes. A cross-origin read could not be read
    // back anyway — no CORS headers are sent — but refusing it costs nothing and means
    // the rule is "no other origin talks to this API" rather than a list of verbs.
    if let Some(origin) = header(headers, "origin") {
        // "null" is what a sandboxed frame or a `file://` page sends. It names no origin
        // this server could recognise, so it is not this one.
        let origins = own_origins(bound, proxied);
        if !origins.iter().any(|own| origin.eq_ignore_ascii_case(own)) {
            return Err(ApiError::bad_request(format!(
                "this API answers only its own origin, and this request came from \
                 {origin:?}"
            )));
        }
    }

    // --- Content-Type on a write ----------------------------------------------
    //
    // **An absent Origin is allowed, and that is a decision rather than an oversight.**
    //
    // The guarantee is stronger than "a browser sends one when the origins differ", which
    // is what an earlier version of this comment claimed. The Fetch standard appends
    // `Origin` to **every request whose method is not GET or HEAD**, same-origin or not —
    // so a *write* with no `Origin` at all is not something a browser can be made to
    // produce. The value may be the string `null`, under `no-referrer` and under the
    // default `strict-origin-when-cross-origin` whenever an https page targets an http
    // URL — which is exactly the evil.example-to-127.0.0.1 case — but `null` is present,
    // and the allowlist above rejects it.
    //
    // What the allowance is really for is everything that is not a browser: `curl`, a
    // script, a review session. Refusing them would stop no attacker.
    //
    // This check then covers the form independently, and is the one that does not depend
    // on browser behaviour staying as it is: a `<form>` can send only
    // `application/x-www-form-urlencoded`, `multipart/form-data` or `text/plain`, and
    // none of them is JSON.
    if is_a_write(method) {
        let content_type = header(headers, "content-type").unwrap_or_default();
        let media_type = content_type.split(';').next().unwrap_or_default().trim();

        if !media_type.eq_ignore_ascii_case("application/json") {
            return Err(ApiError::invalid_value(
                "Content-Type",
                format!("a write to this API is JSON, and this request sent {content_type:?}"),
                "application/json",
            ));
        }
    }

    Ok(())
}
