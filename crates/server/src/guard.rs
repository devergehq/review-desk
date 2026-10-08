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
//! | `Host` is the loopback address and port actually bound | DNS rebinding: a name the attacker controls, resolved to 127.0.0.1 |
//! | `Origin`, when present, is this server's own | A page on another origin driving `fetch` or submitting a form |
//! | `Content-Type` is JSON on a write | A plain HTML form, which cannot send one |
//!
//! Each is load-bearing on its own. A form post carries an `Origin` today, so the second
//! covers it — but the third is what makes that a property of the request shape rather
//! than of browser behaviour that could change. **No CORS headers are sent**, by omission
//! and on purpose: there is no origin this API wants to be readable from.
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

/// The host-and-port forms that name this server.
///
/// `localhost` is allowed beside the literal address, and that is safe rather than a
/// convenience hole: DNS rebinding works by pointing a name *the attacker controls* at
/// 127.0.0.1, and the browser then sends that name — `evil.example` — in `Host`. It
/// cannot make the browser send `localhost`.
fn own_hosts(bound: SocketAddr) -> [String; 3] {
    let port = bound.port();
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
}

/// The origins that are this server.
fn own_origins(bound: SocketAddr) -> Vec<String> {
    own_hosts(bound)
        .iter()
        .map(|host| format!("http://{host}"))
        .collect()
}

/// Whether this method changes the record.
fn is_a_write(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

/// Check a request bound for the API.
///
/// Returns the refusal to serve, or `Ok(())`.
pub fn check(bound: SocketAddr, method: &str, headers: Headers<'_>) -> Result<(), ApiError> {
    // --- Host -----------------------------------------------------------------
    //
    // Absent is refused as firmly as wrong. HTTP/1.1 requires a Host, every browser
    // sends one, and treating "no Host" as "must be fine" would make the check optional
    // for anything that simply left it off.
    let host = header(headers, "host").unwrap_or_default();
    let hosts = own_hosts(bound);
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
        let origins = own_origins(bound);
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
    // Nothing that is not a browser sends one: `curl`, a script, a review session. A
    // browser always sends one on a cross-origin request, including a form submission,
    // so refusing the absent case would break every non-browser caller and stop no
    // attacker. This check is what covers the case anyway — a plain HTML form can send
    // only `application/x-www-form-urlencoded`, `multipart/form-data` or `text/plain`,
    // and none of them is JSON.
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
