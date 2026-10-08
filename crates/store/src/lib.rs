//! The Review Desk record store.
//!
//! **This crate is the only code that opens the database file.** The binary, and every
//! later caller — the local HTTP API, the dashboard, the harvester — goes through the
//! typed functions here. That is the project's first rule of interaction: one way in.
//!
//! ```no_run
//! use review_desk_store::{NewReview, Store};
//!
//! let store = Store::open("/tmp/review-desk.db")?;
//! let review = store.create_review(NewReview {
//!     repo: "devergehq/review-desk".to_string(),
//!     pr: 1,
//!     head_sha: "c0ffee".to_string(),
//!     round: 1,
//!     ..Default::default()
//! })?;
//! println!("review {}", review.id);
//! # Ok::<(), review_desk_store::StoreError>(())
//! ```

#![deny(missing_debug_implementations)]

mod entities;
pub mod error;
pub mod migrations;
pub mod model;
mod normalise;
pub mod path;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub use crate::error::{Result, StoreError};
pub use crate::migrations::newest_version;
pub use crate::model::{
    now, Actor, Brief, Disposition, Finding, FindingRef, FullReview, NewBrief, NewFinding,
    NewOutcome, NewReview, NewRule, OpenedReview, Outcome, OutcomeKind, PrState, Review,
    ReviewLinks, Rule, RuleScope, RuleStage, Severity, Timestamp,
};
pub use crate::path::{default_db_path, resolve_db_path, DB_ENV_VAR};

/// An open Review Desk database, migrated to a known schema version.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    /// Open the database at `path`, creating it if it is not there, and apply every
    /// pending migration.
    ///
    /// A missing parent directory is created. A database newer than this build knows
    /// about is refused rather than guessed at.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_at_version(path, migrations::newest_version())
    }

    /// Open the database the environment points at: `REVIEW_DESK_DB` if set, else
    /// `~/.review-desk/review-desk.db`.
    pub fn open_default() -> Result<Self> {
        Self::open(resolve_db_path()?)
    }

    /// Open the database and migrate only as far as `target`.
    ///
    /// This exists so that a test, or a recovery, can hold a database at an older
    /// version deliberately. Ordinary callers want [`Store::open`].
    ///
    /// The typed functions speak the newest schema only: once a migration adds a
    /// column, a read of that entity against an older database fails on the missing
    /// column. A database held back is for inspecting and migrating, not for writing
    /// through.
    pub fn open_at_version(path: impl AsRef<Path>, target: i64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| StoreError::Open {
                    path: path.clone(),
                    source: Box::new(source),
                })?;
            }
        }

        let conn = Connection::open(&path).map_err(|source| StoreError::Open {
            path: path.clone(),
            source: Box::new(source),
        })?;

        // Foreign keys are off by default in SQLite, which would make "a finding
        // cannot belong to a review that is not there" a claim nothing enforces.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        migrations::migrate_to(&conn, target)?;

        Ok(Self { conn, path })
    }

    /// Where this database is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version this database is at.
    pub fn schema_version(&self) -> Result<i64> {
        migrations::applied_version(&self.conn)
    }

    /// Whether this connection is enforcing foreign keys.
    pub fn foreign_keys_enforced(&self) -> Result<bool> {
        let on: i64 = self
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        Ok(on == 1)
    }
}

/// SQLite's extended result codes for the constraint failures the store gives names to.
mod constraint {
    pub const UNIQUE: i32 = 2067;
    pub const PRIMARY_KEY: i32 = 1555;
    pub const FOREIGN_KEY: i32 = 787;
}

/// Which constraint a SQLite error broke, if it broke one.
fn broken_constraint(error: &rusqlite::Error) -> Option<i32> {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => match failure.extended_code {
            constraint::UNIQUE | constraint::PRIMARY_KEY | constraint::FOREIGN_KEY => {
                Some(failure.extended_code)
            }
            _ => None,
        },
        _ => None,
    }
}

/// True when this error is SQLite refusing a duplicate.
fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        broken_constraint(error),
        Some(constraint::UNIQUE) | Some(constraint::PRIMARY_KEY)
    )
}

/// True when this error is SQLite refusing a row whose parent is absent.
fn is_foreign_key_violation(error: &rusqlite::Error) -> bool {
    broken_constraint(error) == Some(constraint::FOREIGN_KEY)
}
