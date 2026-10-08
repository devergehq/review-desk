//! Where the database lives.
//!
//! Default `~/.review-desk/review-desk.db`, overridden by `REVIEW_DESK_DB`. Resolution
//! lives in the store crate because the database file is the store's business; the
//! binary asks rather than deciding for itself.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::error::{Result, StoreError};

/// The environment variable that overrides the database location.
pub const DB_ENV_VAR: &str = "REVIEW_DESK_DB";

/// The directory the default database sits in, under the home directory.
const DEFAULT_DIR: &str = ".review-desk";

/// The default database file name.
const DEFAULT_FILE: &str = "review-desk.db";

/// The database path to use: `REVIEW_DESK_DB` if it is set and not empty, else the
/// default under the home directory.
pub fn resolve_db_path() -> Result<PathBuf> {
    match std::env::var_os(DB_ENV_VAR) {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => default_db_path(),
    }
}

/// `~/.review-desk/review-desk.db`, ignoring any override.
pub fn default_db_path() -> Result<PathBuf> {
    let home = home_dir().ok_or(StoreError::NoHome)?;
    Ok(PathBuf::from(home).join(DEFAULT_DIR).join(DEFAULT_FILE))
}

/// The home directory, from `HOME` or, on Windows, `USERPROFILE`.
fn home_dir() -> Option<OsString> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
}
