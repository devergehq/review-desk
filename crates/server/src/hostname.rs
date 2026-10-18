//! The one name, besides the address it bound, that this server will answer to.
//!
//! **This exists because the dashboard is reached through this machine's local reverse
//! proxy**, at a name dnsmasq resolves to the loopback address. The proxy forwards the
//! request with `Host: app.review-desk.local`, and `guard.rs` answers only to the address the
//! listener actually bound — so until it is told the name, the server refuses its own
//! dashboard.
//!
//! The other shape was available and is worth naming, because it is what the development
//! loop uses. `web/nuxt.config.ts` has the proxy *state* this server's own `Host` and
//! `Origin`, and the guard is left knowing nothing about it. A reverse proxy could do the
//! same here. It was not done that way because rewriting `Origin` in a proxy **launders**
//! it: a page on any origin at all would arrive wearing the dashboard's, and
//! `web/dev-proxy.ts` exists only to compensate for that. Telling the server one name it
//! may answer to needs no compensation — nothing is rewritten, so a foreign `Origin`
//! arrives as itself and is refused by the check that was already there.
//!
//! **Why this is not the bypass `cross_origin.rs` forbids.** That test's subject is a
//! guard that can be *configured*: an allowed-origins list, an environment variable, a
//! `--dev` flag — each of them one mistake away from being left on in a build that signs
//! off reviews. This is one name, parsed into this type, passed as an argument that has
//! to be typed. There is no environment variable, no build flag, no list, and no default:
//! the guard's allowlist is still a fixed set of exact strings, and all that changes is
//! that the developer may add one of them. DNS rebinding is untouched, because rebinding
//! turns on a name *the attacker controls* and an attacker cannot make a browser send
//! `app.review-desk.local`.

use std::fmt;

/// A name that cannot be a host this server answers to.
#[derive(Debug, thiserror::Error)]
#[error("{raw:?} cannot be a hostname this server answers to: {detail}")]
pub struct HostnameError {
    raw: String,
    detail: &'static str,
}

/// One host — and optionally one port — that this server answers to beside its address.
///
/// Parsed once, here, so that everything downstream holds a name that has already been
/// checked. The guard compares it as an exact string and never as a suffix, which is what
/// keeps `evil.review-desk.local` from being this server because `review-desk.local` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hostname {
    /// The `Host` header this names, lowercased: `review-desk.local`, or with a port when
    /// one was given.
    host: String,
}

impl Hostname {
    /// Read a hostname, or say why it is not one.
    ///
    /// **Nothing is trimmed or repaired.** A name with a space in it, or a scheme, or a
    /// path, is a mistake at the command line, and a mistake that is silently cleaned up
    /// is a mistake that reaches the guard as something the developer did not type.
    pub fn parse(raw: &str) -> Result<Self, HostnameError> {
        let refuse = |detail: &'static str| {
            Err(HostnameError {
                raw: raw.to_string(),
                detail,
            })
        };

        if raw.is_empty() {
            return refuse("it is empty");
        }
        if raw.contains("://") {
            return refuse("it is a URL, and this is a host with an optional port");
        }
        if raw.contains('/') {
            return refuse("it has a path");
        }
        if raw.contains('@') {
            return refuse("it has a userinfo part");
        }
        if raw.chars().any(char::is_whitespace) {
            return refuse("it has whitespace in it");
        }
        if raw.contains('*') {
            return refuse(
                "it is a wildcard, and the guard matches one exact name rather than a pattern",
            );
        }
        if raw.contains('[') || raw.contains(']') {
            return refuse(
                "it is an IPv6 literal; the loopback address the listener bound is already \
                 answered to and needs no naming",
            );
        }

        // Host and port are split here rather than by a URL parser, because what is being
        // built is the `Host` header's own text and a parser would hand back a structure
        // that has to be printed back into one.
        let (host, port) = match raw.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (raw, None),
        };

        if host.is_empty() {
            return refuse("it has a port but no host");
        }
        if host.ends_with('.') {
            return refuse(
                "it ends in a dot; the guard matches exactly, so the trailing-dot form and \
                 the bare form are two different names and only one can be meant",
            );
        }
        if host.contains("..") {
            return refuse("it has an empty label");
        }
        if host.starts_with('.') {
            return refuse("it starts with a dot");
        }
        for label in host.split('.') {
            if label.starts_with('-') || label.ends_with('-') {
                return refuse("a label starts or ends with a hyphen");
            }
            if !label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
            {
                return refuse("a label has something other than letters, digits and hyphens");
            }
        }

        if let Some(port) = port {
            if port.is_empty() {
                return refuse("it ends in a colon with no port");
            }
            if port.contains(':') {
                return refuse("it has more than one port");
            }
            if port.parse::<u16>().is_err() {
                return refuse("the port is not a number between 0 and 65535");
            }
        }

        Ok(Hostname {
            host: raw.to_ascii_lowercase(),
        })
    }

    /// The `Host` header this name is, exactly as it must arrive.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The origin a page served under this name has.
    ///
    /// **`https` and only `https`.** This name exists to be served through the local
    /// reverse proxy, which terminates TLS and redirects port 80 to 443, so that is the
    /// one scheme a browser can ever be on when it sends this `Host`. Accepting
    /// `http://` as well would widen the allowlist to an origin nothing serves.
    pub fn origin(&self) -> String {
        format!("https://{}", self.host)
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(&self.host)
    }
}

#[cfg(test)]
mod tests {
    use super::Hostname;

    #[test]
    fn a_plain_name_is_read_and_lowercased() {
        let name = Hostname::parse("Review-Desk.local").expect("a hostname");
        assert_eq!(name.host(), "review-desk.local");
        assert_eq!(name.origin(), "https://review-desk.local");
    }

    #[test]
    fn a_port_is_kept_because_the_header_carries_it() {
        let name = Hostname::parse("review-desk.local:8443").expect("a hostname");
        assert_eq!(name.host(), "review-desk.local:8443");
        assert_eq!(name.origin(), "https://review-desk.local:8443");
    }

    #[test]
    fn the_shapes_that_are_not_a_hostname_are_refused() {
        for raw in [
            "",
            "https://review-desk.local",
            "review-desk.local/dashboard",
            "user@review-desk.local",
            "review desk.local",
            "*.review-desk.local",
            "[::1]",
            ":7777",
            "review-desk.local.",
            "review-desk..local",
            ".review-desk.local",
            "-review-desk.local",
            "review-desk.local:",
            "review-desk.local:70000",
            "review-desk.local:http",
            "review_desk.local",
        ] {
            assert!(
                Hostname::parse(raw).is_err(),
                "{raw:?} was accepted as a hostname"
            );
        }
    }
}
