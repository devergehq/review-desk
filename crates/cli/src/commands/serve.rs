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

use clap::Args;
use review_desk_harvest::gh::Gh;
use review_desk_server::{Github, Hostname, Records, Server, DEFAULT_PORT};
use review_desk_store::Store;
use serde_json::json;

use crate::error::{CliError, Result};
use crate::report::Report;

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// The port to listen on. 0 asks the operating system for a free one.
    ///
    /// There is no flag for the address: the server binds 127.0.0.1 and nothing else.
    /// This version has no authentication — reachability is the authorisation — so a flag
    /// that could widen the bind is one typo away from publishing the whole record.
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
}

pub fn run(args: &ServeArgs, json: bool) -> Result<Report> {
    let store = Store::open_default()?;
    let database = store.path().display().to_string();

    let server = Server::bind(args.port, Records::new(store), Github::new(Gh::new()))
        .map_err(|error| CliError::bad_input("cannot start the server", error))?
        .also_answering_to(args.hostname.clone());

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
        if !review_desk_server::assets::embedded() {
            println!("  the front end is not built into this binary; the API is serving");
        }
    }

    server.serve();

    // Reached only if every worker thread has stopped, which is the process being told to
    // end. There is nothing left to report that has not been printed.
    Ok(Report::silent())
}
