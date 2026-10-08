//! Forward-only schema migrations.
//!
//! Each migration is a `(version, name, sql)` triple in `MIGRATIONS`, ordered by
//! version with no gaps. Opening a database applies every migration above its current
//! `PRAGMA user_version`, each in its own transaction, and records the new version. A
//! migration that has shipped is never edited; the next change is a new entry.
//!
//! The split below is not arbitrary: version 1 is the review record, version 2 is the
//! learning layer the project brief treats as a later concern.

use rusqlite::Connection;

use crate::error::{Result, StoreError};

/// One migration.
#[derive(Debug)]
pub struct Migration {
    /// The `user_version` the database holds once this has been applied.
    pub version: i64,
    /// A short name, used in error messages.
    pub name: &'static str,
    /// The statements to run. Executed as a batch inside one transaction.
    pub sql: &'static str,
}

/// Every migration, in order. Append only.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "review-record",
        sql: r#"
CREATE TABLE reviews (
    id               INTEGER PRIMARY KEY,
    repo             TEXT    NOT NULL,
    pr               INTEGER NOT NULL,
    head_sha         TEXT    NOT NULL,
    round            INTEGER NOT NULL,
    session_id       TEXT,
    ledger_id        TEXT,
    github_review_id TEXT,
    created_at       INTEGER NOT NULL,
    UNIQUE (repo, pr, round)
);

CREATE INDEX reviews_by_head ON reviews (repo, pr, head_sha);

CREATE TABLE briefs (
    review_id        INTEGER PRIMARY KEY REFERENCES reviews (id) ON DELETE CASCADE,
    problem          TEXT    NOT NULL,
    diagram          TEXT,
    option_map       TEXT,
    approach_verdict TEXT,
    confirmed_at     INTEGER,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE findings (
    review_id          INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    seq                INTEGER NOT NULL,
    lens               TEXT    NOT NULL,
    severity           TEXT    NOT NULL,
    file               TEXT,
    line               INTEGER,
    claim              TEXT    NOT NULL,
    evidence           TEXT,
    disposition        TEXT    NOT NULL,
    disposition_reason TEXT,
    created_at         INTEGER NOT NULL,
    PRIMARY KEY (review_id, seq)
);

CREATE TABLE outcomes (
    id          INTEGER PRIMARY KEY,
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    actor       TEXT    NOT NULL,
    occurred_at INTEGER NOT NULL,
    detail      TEXT,
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);

CREATE INDEX outcomes_by_finding ON outcomes (review_id, finding_seq);
"#,
    },
    Migration {
        version: 2,
        name: "rules",
        sql: r#"
CREATE TABLE rules (
    id               INTEGER PRIMARY KEY,
    statement        TEXT    NOT NULL,
    prevents         TEXT    NOT NULL,
    scope            TEXT    NOT NULL,
    scope_repo       TEXT,
    stage            TEXT    NOT NULL,
    origin_review_id INTEGER REFERENCES reviews (id) ON DELETE SET NULL,
    created_at       INTEGER NOT NULL,
    CHECK (
        (scope = 'global' AND scope_repo IS NULL)
        OR (scope = 'repo' AND scope_repo IS NOT NULL)
    )
);

CREATE INDEX rules_by_scope ON rules (scope, scope_repo, stage);

CREATE TABLE rule_findings (
    rule_id     INTEGER NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    PRIMARY KEY (rule_id, review_id, finding_seq),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);
"#,
    },
];

/// The version a freshly created database lands on.
pub fn newest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// The version this database currently holds.
pub fn applied_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// Apply every migration above the current version, up to and including `target`.
///
/// Forward-only: a database already at or above `target` is left alone. A database
/// beyond the newest migration this build knows is refused rather than guessed at.
pub fn migrate_to(conn: &Connection, target: i64) -> Result<i64> {
    let from = applied_version(conn)?;
    if from > newest_version() {
        return Err(StoreError::FromTheFuture {
            found: from,
            known: newest_version(),
        });
    }

    for migration in MIGRATIONS {
        if migration.version <= from || migration.version > target {
            continue;
        }
        apply(conn, migration)?;
    }

    applied_version(conn)
}

/// Apply one migration and record its version, atomically.
fn apply(conn: &Connection, migration: &Migration) -> Result<()> {
    let run = || -> rusqlite::Result<()> {
        conn.execute_batch("BEGIN")?;
        conn.execute_batch(migration.sql)?;
        // PRAGMA will not take a bound parameter, so the version is formatted in. It
        // comes from the const table above, never from a caller.
        conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))?;
        conn.execute_batch("COMMIT")?;
        Ok(())
    };

    run().map_err(|source| {
        let _ = conn.execute_batch("ROLLBACK");
        StoreError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_ordered_from_one_with_no_gaps() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                migration.version,
                index as i64 + 1,
                "migration {:?} is out of order",
                migration.name
            );
        }
    }

    #[test]
    fn newest_version_is_the_last_migration() {
        assert_eq!(newest_version(), MIGRATIONS.len() as i64);
    }
}
