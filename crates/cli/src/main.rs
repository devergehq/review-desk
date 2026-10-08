//! The `review-desk` binary.
//!
//! This issue gives it only what proves the store works: `--version`, and `db path` to
//! say where the database lives. The commands that write and read review records are
//! DEV-851; they will sit on the same store crate.
//!
//! The binary never opens the database file itself. It asks `review-desk-store`.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use review_desk_store::{resolve_db_path, Store};

/// Exit code for a request that was understood and refused.
const EXIT_REFUSED: u8 = 1;

/// Exit code for Review Desk being unable to run.
const EXIT_BROKEN: u8 = 2;

#[derive(Debug, Parser)]
#[command(
    name = "review-desk",
    version,
    about = "The record of an agent-assisted code review."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect the database.
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DbCommand {
    /// Print the database path, honouring REVIEW_DESK_DB.
    Path,
    /// Print the schema version of the database, creating it if it is absent.
    Version,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("review-desk: {error}");
            if error.is_refusal() {
                ExitCode::from(EXIT_REFUSED)
            } else {
                ExitCode::from(EXIT_BROKEN)
            }
        }
    }
}

fn run(cli: Cli) -> review_desk_store::Result<()> {
    match cli.command {
        Command::Db { command } => match command {
            DbCommand::Path => {
                println!("{}", resolve_db_path()?.display());
                Ok(())
            }
            DbCommand::Version => {
                let store = Store::open_default()?;
                println!("{}", store.schema_version()?);
                Ok(())
            }
        },
    }
}
