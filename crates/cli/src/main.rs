//! The `review-desk` binary: the commands an agent session calls to record a review and
//! to read the rules for a repository.
//!
//! The binary never opens the database file itself. It asks `review-desk-store`, which
//! is the only code that does.
//!
//! Three things hold across every command, and they are all decided in this file:
//!
//! * **Exit 0 is done, 1 is refused, 2 is broken.** A caller must be able to tell a
//!   request that was understood and refused from Review Desk being unable to run
//!   without reading message text. Locus's review worker treats 2 as a loud warning that
//!   never blocks the review, so a typo must not land there.
//! * **With `--json`, standard output is one JSON document and nothing else** — on a
//!   refusal as much as on success, which is why failures are printed here rather than
//!   left to each command.
//! * **Without `--json`, standard output is for the answer and standard error is for the
//!   failure.** Nothing is written to both.

mod commands;
mod error;
mod input;
mod report;

use std::process::ExitCode;

use clap::Parser;

use crate::commands::Cli;
use crate::error::error_document;

/// Exit code for a request that was understood and refused.
const EXIT_REFUSED: u8 = 1;

/// Exit code for Review Desk being unable to run.
const EXIT_BROKEN: u8 = 2;

fn main() -> ExitCode {
    // Whether to answer in JSON has to be settled before the arguments parse, because a
    // usage error is itself an answer and the contract covers every answer. Once they do
    // parse, `cli.json` is what counts.
    let json_was_asked_for = json_in_raw_arguments();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return usage(error, json_was_asked_for),
    };

    match commands::run(&cli) {
        Ok(report) => {
            report.print(cli.json);
            ExitCode::SUCCESS
        }
        Err(error) => {
            if cli.json {
                // Standard output, not standard error: a caller that asked for JSON gets
                // one document wherever it is reading, and the exit code says which kind
                // of failure it is.
                println!("{}", error.document());
            } else {
                eprintln!("review-desk: {error}");
            }

            if error.is_refusal() {
                ExitCode::from(EXIT_REFUSED)
            } else {
                ExitCode::from(EXIT_BROKEN)
            }
        }
    }
}

/// Report an argument that clap would not accept.
///
/// clap exits the process itself with code 2 on a usage error, which is the code this
/// binary reserves for being unable to run. A typo is a refused request, so it is 1 —
/// otherwise a mistyped flag would page whoever treats 2 as broken.
fn usage(error: clap::Error, json: bool) -> ExitCode {
    // Help and version are not failures. clap reports them through the same type and
    // sends them to standard output, and they stay text even when `--json` is given:
    // they describe the command line rather than answering a question about a record.
    if !error.use_stderr() {
        let _ = error.print();
        return ExitCode::SUCCESS;
    }

    if json {
        println!(
            "{}",
            error_document("usage", error.render().to_string().trim_end(), true)
        );
    } else {
        let _ = error.print();
    }

    ExitCode::from(EXIT_REFUSED)
}

/// Whether `--json` is among the raw arguments.
///
/// Only used to choose the shape of a usage error, so the one thing it can get wrong — a
/// flag's *value* being the text `--json` — can only change how an error the arguments
/// were going to cause anyway is formatted.
fn json_in_raw_arguments() -> bool {
    std::env::args_os().any(|argument| argument == "--json")
}
