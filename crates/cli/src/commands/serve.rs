//! `serve`: run the local HTTP server.
//!
//! Everything the server does lives in `review-desk-server`. This module turns arguments
//! into a bound listener and says where it is.
//!
//! It is the one command that prints for itself, and the reason is that it is the one
//! command that does not end. Every other command builds a [`Report`] and `main` prints
//! it on the way out; a report printed after `serve` returned would be printed when the
//! server stopped, which is far too late to tell anybody where it is. So the URL goes out
//! first — one JSON document with `--json`, one line without — and then the server runs.
//!
//! **And it is the one command that prints the sign-in link** — DEV-1346. Signing off needs
//! the developer's cookie, which the browser gets by trading a one-time token, and this
//! output is the only place that token ever appears: as the URL's fragment, only when the
//! output is a terminal, and never in `--json`. An agent that starts the server in its own
//! shell is not at a terminal, so it is told how the developer signs in rather than handed
//! the way to. See `crates/server/src/developer.rs` for what that does and does not stop.

use clap::Args;
use review_desk_harvest::gh::Gh;
use std::io::IsTerminal;
use std::path::PathBuf;

use review_desk_server::{Allele, Github, Hostname, Records, Server, DEFAULT_PORT};
use review_desk_store::Store;
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// The port to listen on. 0 asks the operating system for a free one.
    ///
    /// There is no flag for the address: the server binds 127.0.0.1 and nothing else.
    /// Reaching the port is enough to read the whole record — only the six sign-off acts
    /// need the developer's cookie — so a flag that could widen the bind is one typo away
    /// from publishing it.
    #[arg(long, value_name = "N", default_value_t = DEFAULT_PORT)]
    port: u16,

    /// One more hostname to answer to, forwarded by a reverse proxy on this machine.
    ///
    /// This is how the dashboard is reached at a name rather than at a port: the local
    /// proxy terminates TLS for, say, `app.review-desk.local` and forwards it here, and
    /// without being told that name the server refuses its own dashboard.
    ///
    /// **It does not widen the bind.** The listener is still 127.0.0.1 and nothing else;
    /// all this adds is one exact string to the `Host` and `Origin` checks, and only
    /// while it is typed. `crates/server/src/hostname.rs` says what that does and does
    /// not open.
    #[arg(long, value_name = "NAME", value_parser = Hostname::parse)]
    hostname: Option<Hostname>,

    /// Where to ask Allele what each review's session is doing.
    ///
    /// Defaults to Allele's own socket, `~/.allele/control.sock`. **Optional in every
    /// sense**: when nothing is there, the review pages draw no session strip and nothing
    /// else changes. The server asks `sessions_status`, and, when the developer asks a
    /// question, asks Allele to hand the session a notice pointing at it (DEV-1383).
    /// `crates/server/src/allele.rs` says what it asks and how it answers when Allele does
    /// not; `crates/server/src/handover.rs` says what is handed over.
    #[arg(long, value_name = "PATH")]
    allele_socket: Option<PathBuf>,
}

/// The sign-in link, to a person, or how a person gets one.
fn print_sign_in(server: &Server, hostname: Option<&Hostname>) {
    let Some(link) = server.sign_in_url() else {
        return;
    };
    if !std::io::stdout().is_terminal() {
        // Probably an agent's shell, or a log. The link is not printed.
        println!("  read-only here: to sign off, run `review-desk serve` in your own terminal");
        return;
    }
    println!("  sign in to sign off, once, in your browser: {link}");
    if let (Some(name), Some(token)) = (hostname, server.sign_in_token()) {
        // Either works, once: the cookie belongs to whichever host the browser signs in at.
        println!("  or through the proxy: https://{name}/#sign-in={token}");
    }
}

pub fn run(args: &ServeArgs, json: bool) -> Result<Report> {
    let store = Store::open_default()?;
    let database = store.path().display().to_string();
    let allele = args
        .allele_socket
        .clone()
        .or_else(Allele::default_socket)
        .map_or_else(Allele::none, Allele::at);
    let allele_socket = allele.socket().map(|path| path.display().to_string());

    let server = Server::bind(args.port, Records::new(store), Github::new(Gh::new()))
        .map_err(|error| CliError::bad_input("cannot start the server", error))?
        .also_answering_to(args.hostname.clone())
        .reading_sessions_from(allele.clone());

    let url = server.url();
    if json {
        println!(
            "{}",
            json!({
                "serve": {
                    "url": url,
                    "address": server.address().to_string(),
                    // Null unless `--hostname` was given, which is the usual case.
                    "hostname": args.hostname.as_ref().map(ToString::to_string),
                    "database": database,
                    // Where session state is asked for, present or not.
                    "allele_socket": allele_socket,
                    "front_end_embedded": review_desk_server::assets::embedded(),
                }
            })
        );
    } else {
        println!("Review Desk is at {url}");
        if let Some(hostname) = &args.hostname {
            println!("  also answering to https://{hostname}/ through this machine's proxy");
        }
        println!("  database: {database}");
        if let Some(socket) = &allele_socket {
            println!("  session state from Allele at {socket}, when it is running");
        }
        if !review_desk_server::assets::embedded() {
            println!("  the front end is not built into this binary; the API is serving");
        }
        print_sign_in(&server, args.hostname.as_ref());
    }

    server.serve();

    // Reached only if every worker thread has stopped, which is the process being told to
    // end. There is nothing left to report that has not been printed.
    Ok(Report::silent())
}
