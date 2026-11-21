//! Who may sign off: the developer's cookie, and the one-time token that buys it — DEV-1346.
//!
//! **Reading stays open and acting is the developer's.** Until this, being able to reach
//! the port was the whole authorisation, and `crate::guard` closed it to web pages and to
//! nothing else — so any process on the Mac, a review session included, could approve a
//! draft with one `curl`. Six operations now need a cookie that only the developer's
//! browser holds: confirming a brief, approving a draft, ticketing what is left and
//! approving, sending a draft back, dismissing a review and restoring one. They are the
//! ones [`crate::route::Operation::is_developer_act`] names, and the contract marks the
//! same six with `security: developerCookie`; `tests/developer.rs` holds the two lists
//! together.
//!
//! The way in is the same one Drafting Desk uses:
//!
//! 1. `review-desk serve` makes a **sign-in token** when it starts — 32 random bytes, held
//!    in memory and nowhere else — and prints it once, as the fragment of the dashboard's
//!    URL, and only to a terminal. A fragment is never sent to a server, so it is in no
//!    request log.
//! 2. The page reads the fragment, removes it from the address bar, and trades it at
//!    `POST /developer/sign-in` for a cookie: `HttpOnly`, so no script on the page can read
//!    it back, and `SameSite=Strict`, so no other site can make the browser send it.
//! 3. **The token is spent on that trade.** A copy left in the browser's history or a
//!    terminal's scrollback is worth nothing afterwards. The cookie is a second random value,
//!    not the token, and it lasts as long as this run of the server.
//!
//! ## The bar this sets, and no higher
//!
//! **It stops an agent approving with one `curl`. It does not stop an agent set on stealing
//! a cookie.** Code running as the developer can read the browser's cookie store, or the
//! terminal the token was printed to, and nothing a local server does can prevent that. What
//! this closes is the documented interface: an agent that can reach the API can no longer
//! sign off through it by accident, by a prompt injection that says "approve it", or by
//! simply being asked to. That is the honest-agent bar Drafting Desk states, and the README
//! says the same thing in the same words.

use std::collections::HashSet;
use std::io::Read;
use std::sync::{Arc, Mutex};

/// The cookie's name.
pub const COOKIE: &str = "rd_developer";

/// The environment variable the end-to-end test hands its token in through.
///
/// **Read only by a binary built with the `e2e` feature, and that feature cannot be built
/// in release** — see the `compile_error!` below. A shipped binary always makes its own
/// token, so nothing in its environment can choose one for it.
pub const E2E_TOKEN_VARIABLE: &str = "REVIEW_DESK_E2E_TOKEN";

// **The e2e feature is a debug-build feature, and this is what makes that a fact rather than a
// habit.** A release binary with it would take its sign-in token from the environment, which
// is a token any process that can set one could choose. `cargo build --release --features e2e`
// fails here instead.
#[cfg(all(feature = "e2e", not(debug_assertions)))]
compile_error!(
    "the `e2e` feature lets the environment choose the sign-in token, and is never built \
     in release: build the end-to-end test's binary in debug"
);

/// The token the end-to-end test hands in, when this binary was built for it.
#[cfg(feature = "e2e")]
pub fn e2e_token() -> Option<String> {
    std::env::var(E2E_TOKEN_VARIABLE)
        .ok()
        .filter(|token| token.len() >= 32)
}

/// The token the end-to-end test hands in: never, in a binary not built for it.
#[cfg(not(feature = "e2e"))]
pub fn e2e_token() -> Option<String> {
    None
}

/// 32 random bytes from the operating system, hex encoded.
///
/// `/dev/urandom` rather than a crate, as Drafting Desk does it: it is the operating
/// system's own generator on every platform this binary is built for, and reading it is
/// four lines.
pub fn new_secret() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Compare two secrets without stopping at the first byte that differs.
fn same_secret(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The sign-in token, until it is spent, and the cookies handed out for it.
#[derive(Debug, Clone)]
pub struct Developer {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// `None` once spent.
    token: Mutex<Option<String>>,
    /// Every cookie value handed out this run.
    cookies: Mutex<HashSet<String>>,
}

impl Developer {
    /// A developer who signs in with this token.
    pub fn signing_in_with(token: String) -> Self {
        Developer {
            inner: Arc::new(Inner {
                token: Mutex::new(Some(token)),
                cookies: Mutex::new(HashSet::new()),
            }),
        }
    }

    /// The token, while it is still unspent. For `review-desk serve` to print, once.
    pub fn token(&self) -> Option<String> {
        self.inner
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Trade the token for a cookie value, spending the token. `None` when the token is
    /// wrong or already spent.
    pub fn sign_in(&self, offered: &str) -> Option<std::io::Result<String>> {
        {
            let mut token = self
                .inner
                .token
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let matches = token
                .as_deref()
                .is_some_and(|held| same_secret(offered.as_bytes(), held.as_bytes()));
            if !matches {
                return None;
            }
            *token = None;
        }

        Some(new_secret().inspect(|cookie| {
            self.inner
                .cookies
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(cookie.clone());
        }))
    }

    /// Whether this cookie value is one this run handed out.
    pub fn recognises(&self, cookie: Option<&str>) -> bool {
        let Some(cookie) = cookie else { return false };
        self.inner
            .cookies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|held| same_secret(cookie.as_bytes(), held.as_bytes()))
    }
}

/// The `Set-Cookie` value for a cookie this run handed out.
///
/// No `Secure`: the dashboard is served over plain `http` on loopback, and a `Secure`
/// cookie would never be sent back there. No `Max-Age`: it is a session cookie, and the
/// server forgets it on restart anyway.
pub fn set_cookie(value: &str) -> String {
    format!("{COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/")
}

/// The developer cookie's value from a request's `Cookie` headers, if it sent one.
pub fn cookie_from(headers: &[(String, String)]) -> Option<String> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("cookie"))
        .flat_map(|(_, value)| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_is_spent_on_the_first_trade() {
        let developer = Developer::signing_in_with("a".repeat(64));
        let cookie = developer
            .sign_in(&"a".repeat(64))
            .expect("the token is right")
            .expect("randomness");
        assert!(developer.recognises(Some(&cookie)));
        assert!(developer.sign_in(&"a".repeat(64)).is_none(), "spent");
        assert_eq!(developer.token(), None);
    }

    #[test]
    fn a_wrong_token_does_not_spend_the_right_one() {
        let developer = Developer::signing_in_with("a".repeat(64));
        assert!(developer.sign_in(&"b".repeat(64)).is_none());
        assert!(developer.sign_in("").is_none());
        assert!(developer.sign_in(&"a".repeat(64)).is_some());
    }

    #[test]
    fn no_cookie_and_an_unknown_cookie_are_not_the_developer() {
        let developer = Developer::signing_in_with("a".repeat(64));
        assert!(!developer.recognises(None));
        assert!(!developer.recognises(Some("")));
        assert!(
            !developer.recognises(Some(&"a".repeat(64))),
            "the token is not a cookie"
        );
    }

    #[test]
    fn the_cookie_is_read_from_among_others() {
        let headers = vec![(
            "Cookie".to_string(),
            format!("theme=dark; {COOKIE}=abc123; other=1"),
        )];
        assert_eq!(cookie_from(&headers).as_deref(), Some("abc123"));
        assert_eq!(cookie_from(&[]), None);
    }

    #[test]
    fn a_binary_not_built_for_the_end_to_end_test_takes_no_token_from_the_environment() {
        // The release half of this is the `compile_error!` above; this is the other half,
        // for the build every other test runs: the variable is read only under the feature.
        if cfg!(feature = "e2e") {
            return;
        }
        std::env::set_var(E2E_TOKEN_VARIABLE, "c".repeat(64));
        assert_eq!(e2e_token(), None);
        std::env::remove_var(E2E_TOKEN_VARIABLE);
    }
}
