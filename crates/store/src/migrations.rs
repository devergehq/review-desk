//! Forward-only schema migrations.
//!
//! Each migration is a `(version, name, sql)` triple in `MIGRATIONS`, ordered by
//! version with no gaps. Opening a database applies every migration above its current
//! `PRAGMA user_version`, each in its own transaction, and records the new version. A
//! migration that has shipped is never edited; the next change is a new entry.
//!
//! The split is not arbitrary: version 1 is the review record, version 2 is the
//! learning layer the project brief treats as a later concern, version 3 is what the
//! harvester needs to read a posted review's outcomes back from GitHub, version 4
//! is what the local API contract needs — the draft, the per-layer status, the
//! structured option map, rule exceptions, and the twelve columns the four screens
//! read — and version 5 is the one fact a brief could not hold: whether the developer
//! has to confirm it at all.

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
        OR (scope = 'repo' AND scope_repo IS NOT NULL AND scope_repo <> '')
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
    Migration {
        version: 3,
        name: "harvest",
        sql: r#"
-- The comment a finding was posted as. Set by `review-desk finding link`, and the
-- only handle the harvester has on a finding's thread.
ALTER TABLE findings ADD COLUMN github_comment_id TEXT;

-- One GitHub comment is one finding. The index is partial because most findings have
-- no linked comment, and SQLite would otherwise have to be trusted to treat every one
-- of those NULLs as distinct.
CREATE UNIQUE INDEX findings_by_github_comment
    ON findings (github_comment_id)
    WHERE github_comment_id IS NOT NULL;

-- What the pull request was last seen to be, when it reached that state, and when the
-- harvester last looked. A review whose pull request is settled is not fetched again.
ALTER TABLE reviews ADD COLUMN pr_state     TEXT;
ALTER TABLE reviews ADD COLUMN pr_state_at  INTEGER;
ALTER TABLE reviews ADD COLUMN harvested_at INTEGER;

CREATE INDEX reviews_by_pr_state ON reviews (pr_state);

-- The identity of the GitHub fact an outcome was harvested from: a reply comment's id,
-- a thread's root comment id, a commit. It is what makes a second harvest write
-- nothing: the index below refuses the row rather than the code remembering not to
-- write it.
--
-- The index is partial so that an outcome recorded by hand, which has no key, is never
-- caught by it. Two developers may record "kept" against one finding twice; two
-- harvests of one reply may not.
ALTER TABLE outcomes ADD COLUMN harvest_key TEXT;

CREATE UNIQUE INDEX outcomes_by_harvest_key
    ON outcomes (review_id, finding_seq, kind, harvest_key)
    WHERE harvest_key IS NOT NULL;
"#,
    },
    Migration {
        version: 4,
        name: "contract",
        sql: r#"
-- The draft review: one per review, replaced in place. An approved draft is what the
-- review session reads and posts, which is why editing one is refused rather than
-- merged: the characters that get posted must be the characters that were approved.
CREATE TABLE drafts (
    review_id                INTEGER PRIMARY KEY REFERENCES reviews (id) ON DELETE CASCADE,
    body                     TEXT    NOT NULL,
    status                   TEXT    NOT NULL,
    event                    TEXT,
    approved_at              INTEGER,
    -- Who approved it. Only the developer signs off, so this is the record of whose
    -- assent was given; the store refuses any other actor.
    approved_by              TEXT,
    sent_back_note           TEXT,
    sent_back_at             INTEGER,
    posted_at                INTEGER,
    lint_passes              INTEGER,
    lint_visible_words       INTEGER,
    lint_word_budget         INTEGER,
    lint_findings_in_body    INTEGER,
    lint_findings_expected   INTEGER,
    lint_raised_before_on_pr INTEGER,
    lint_errors              TEXT,
    lint_checked_at          INTEGER,
    updated_at               INTEGER NOT NULL,
    CHECK (status IN ('drafting', 'ready', 'approved', 'sent_back', 'posted')),
    CHECK (event IS NULL OR event IN ('comment', 'request_changes', 'approve')),
    -- The state machine, enforced by the database rather than remembered by the code.
    -- An approved or posted draft names the event it is to be posted as, who approved
    -- it and when; anything earlier names none of them.
    CHECK (
        (status IN ('approved', 'posted'))
        = (event IS NOT NULL AND approved_at IS NOT NULL AND approved_by IS NOT NULL)
    ),
    -- One direction only. A draft that was sent back and then reworked is `ready`
    -- again and keeps the note, because what the developer asked for is part of the
    -- record even once it has been done.
    CHECK (status <> 'sent_back' OR (sent_back_at IS NOT NULL AND sent_back_note IS NOT NULL)),
    CHECK ((status = 'posted') = (posted_at IS NOT NULL))
);

-- The Inbox groups on this, across every repository.
CREATE INDEX drafts_by_status ON drafts (status);

-- One row per layer that ran. Stored rather than derived: layer 1's lint result has
-- nowhere to be derived from, and a derived status cannot tell a layer that was
-- skipped from one that has not run — which is the distinction "what was not checked"
-- exists to make.
--
-- The layer's *name* is not a column. It is fixed by its number, so storing both
-- would let a row say layer 5 is called `lint`.
CREATE TABLE review_layers (
    review_id  INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    number     INTEGER NOT NULL,
    state      TEXT    NOT NULL,
    detail     TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (review_id, number),
    CHECK (number BETWEEN 1 AND 7),
    CHECK (state IN ('pending', 'running', 'done', 'waiting_on_you', 'failed', 'skipped'))
);

CREATE INDEX review_layers_by_state ON review_layers (state, number);

-- The alternatives considered, written before the diff was read. One row per option,
-- replacing `briefs.option_map` as the thing a screen reads: an opaque string cannot
-- serve a card with a title, an argument each way and a badge.
--
-- Hung off the brief, not the review: an option map with no brief is a set of
-- arguments about nothing.
CREATE TABLE brief_options (
    review_id        INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    key              TEXT    NOT NULL,
    title            TEXT    NOT NULL,
    argument_for     TEXT    NOT NULL,
    argument_against TEXT    NOT NULL,
    chosen           INTEGER NOT NULL,
    PRIMARY KEY (review_id, key),
    CHECK (key <> ''),
    CHECK (title <> '' AND argument_for <> '' AND argument_against <> ''),
    CHECK (chosen IN (0, 1))
);

-- An accepted exception to a rule, with the reason it was accepted. A rule with an
-- exception is still in force; the screen says so next to its stage.
CREATE TABLE rule_exceptions (
    id         INTEGER PRIMARY KEY,
    rule_id    INTEGER NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    reason     TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK (reason <> '')
);

CREATE INDEX rule_exceptions_by_rule ON rule_exceptions (rule_id);

-- The pull request's own facts. The store had nowhere to put any of them, so every
-- screen that shows a review showed its number and not its name.
ALTER TABLE reviews ADD COLUMN title         TEXT;
ALTER TABLE reviews ADD COLUMN ticket_id     TEXT;
ALTER TABLE reviews ADD COLUMN changed_lines INTEGER;

-- What the review did not look at, in the reviewer's own words. The one section of a
-- review with no adversary, so it is stored rather than derived.
ALTER TABLE reviews ADD COLUMN not_checked   TEXT;

-- The one ticket covering every nit held back from this review's draft. The findings
-- it covers are not stored against it: they are every finding on the review at
-- disposition `held_back`, so the set cannot drift from the dispositions.
--
-- SQLite cannot add a CHECK to an existing table, so the closed set is enforced on the
-- way in and on the way out by `NitTicketStatus` rather than by the schema. The
-- default is what gives every row that already exists an honest value.
ALTER TABLE reviews ADD COLUMN nit_ticket_status       TEXT NOT NULL DEFAULT 'none';
ALTER TABLE reviews ADD COLUMN nit_ticket_id           TEXT;
ALTER TABLE reviews ADD COLUMN nit_ticket_requested_at INTEGER;
ALTER TABLE reviews ADD COLUMN nit_ticket_filed_at     INTEGER;

-- Where the problem statement came from and what the developer changed about it.
ALTER TABLE briefs ADD COLUMN provenance   TEXT;

-- Who confirmed the brief. Only the developer signs off; this is the record of it.
ALTER TABLE briefs ADD COLUMN confirmed_by TEXT;

-- The rule a finding was raised under. The store recorded rule-to-finding and never
-- finding-to-rule, so "cited by 6 later findings" and a rule's staleness could not be
-- answered at all. The whole feedback loop runs through this column.
--
-- SET NULL rather than CASCADE: deleting a rule must not delete the findings that
-- cited it. The finding was still raised; it just no longer cites anything.
ALTER TABLE findings ADD COLUMN rule_id INTEGER REFERENCES rules (id) ON DELETE SET NULL;

-- Partial, because most findings cite no rule.
CREATE INDEX findings_by_rule ON findings (rule_id) WHERE rule_id IS NOT NULL;

-- The proposal's justification, and the free line the screen shows under the
-- statement. `note` is also where the reason for a stage change lands, so a dismissed
-- proposal is not proposed again next week with no memory of having been told no.
ALTER TABLE rules ADD COLUMN why_proposed    TEXT;
ALTER TABLE rules ADD COLUMN note            TEXT;

-- When a finding last cited the rule. Stored rather than scanned on every Rules load;
-- it cannot drift, because a finding is never deleted, only dispositioned.
ALTER TABLE rules ADD COLUMN last_cited_at   INTEGER;

-- Set by "Keep 60 more days". Without it a deferred rule comes back as stale on the
-- next load, and the developer's decision to keep it is lost.
ALTER TABLE rules ADD COLUMN review_again_at INTEGER;

-- A rule had `created_at` only, so a promotion, dismissal or deferral left no trace of
-- when it happened. The default makes the column NOT NULL on a table that already has
-- rows; the backfill then gives those rows the only honest answer there is, which is
-- when they were created. 0 is not a time any rule was created at.
ALTER TABLE rules ADD COLUMN updated_at      INTEGER NOT NULL DEFAULT 0;
UPDATE rules SET updated_at = created_at WHERE updated_at = 0;
"#,
    },
    Migration {
        version: 5,
        name: "optional-confirmation",
        sql: r#"
-- Whether the developer has to confirm this brief at all.
--
-- **This is the one place an agent decides a developer step is unnecessary**, so the
-- default is the strict answer and it is what every brief already written gets: a brief
-- stored before this migration was stored under a rule that said the developer confirms,
-- and backfilling it to "not required" would retroactively excuse confirmations that
-- were actually waited for.
--
-- The caller owns the threshold. Patrick's decision of 9 October 2026 is that a brief is
-- confirmed only on pull requests changing more than 200 lines, and Review Desk does not
-- hold that number: it records that the caller said no confirmation was needed, and the
-- reason the caller gave.
ALTER TABLE briefs ADD COLUMN confirmation_required INTEGER NOT NULL DEFAULT 1;

-- Why the caller said so, in the caller's own words. Required when confirmation is not
-- required and refused when it is, which SQLite cannot express as a CHECK on a table
-- that already exists — so `put_brief` enforces it on the way in, the way
-- `NitTicketStatus` does for the column migration 4 added to `reviews`.
--
-- A brief that needs no confirmation with no reason attached would put layer 2 at `done`
-- with nothing on the record saying who decided that or why, which is the state this
-- whole field exists to avoid.
ALTER TABLE briefs ADD COLUMN confirmation_not_required_because TEXT;
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
///
/// The version is read again inside the transaction. Two processes opening a fresh
/// database at the same moment both see version 0 outside it; without the second read
/// the loser would run a migration that has just been applied and fail on a table that
/// already exists.
fn apply(conn: &Connection, migration: &Migration) -> Result<()> {
    let run = || -> rusqlite::Result<bool> {
        // IMMEDIATE takes the write lock before the read, so the version this reads
        // cannot change under it and a competing process waits rather than racing.
        conn.execute_batch("BEGIN IMMEDIATE")?;

        let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current >= migration.version {
            // Someone else applied it while we were waiting for the lock.
            conn.execute_batch("COMMIT")?;
            return Ok(false);
        }

        conn.execute_batch(migration.sql)?;
        // PRAGMA will not take a bound parameter, so the version is formatted in. It
        // comes from the const table above, never from a caller.
        conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))?;
        conn.execute_batch("COMMIT")?;
        Ok(true)
    };

    run().map(|_| ()).map_err(|source| {
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
