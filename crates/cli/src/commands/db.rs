//! `db path` and `db version`: where the record is and what shape it is in.

use clap::Subcommand;
use review_desk_store::{resolve_db_path, Store};
use serde_json::json;

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    /// Print the database path, honouring REVIEW_DESK_DB, without opening it.
    Path,
    /// Print the schema version of the database, creating it if it is absent.
    Version,
}

pub fn run(command: &DbCommand) -> Result<Report> {
    match command {
        DbCommand::Path => {
            let path = resolve_db_path()?;
            Ok(Report::new(
                json!({ "path": path.display().to_string() }),
                path.display().to_string(),
            ))
        }
        DbCommand::Version => {
            let store = Store::open_default()?;
            let version = store.schema_version()?;
            Ok(Report::new(
                json!({ "version": version }),
                version.to_string(),
            ))
        }
    }
}
