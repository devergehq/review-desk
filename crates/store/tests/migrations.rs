//! What opening a database does: create, migrate, refuse.

mod common;

use common::{db_path, temp_store};
use review_desk_store::{newest_version, Store, StoreError};
use tempfile::TempDir;

#[test]
fn opening_an_absent_path_creates_the_database_at_the_newest_version() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    assert!(!path.exists(), "the database should not exist yet");

    let store = Store::open(&path).expect("open creates the database");

    assert!(path.exists(), "the database file was created");
    assert_eq!(
        store.schema_version().unwrap(),
        newest_version(),
        "a fresh database lands on the newest migration"
    );
    assert!(newest_version() > 0, "there is at least one migration");
}

#[test]
fn opening_creates_missing_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("deeper").join("rd.db");

    let store = Store::open(&path).expect("open creates the directories");

    assert!(path.exists());
    assert_eq!(store.schema_version().unwrap(), newest_version());
}

#[test]
fn an_older_database_is_migrated_forward_and_keeps_its_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let previous = newest_version() - 1;
    assert!(previous >= 1, "this test needs a migration to come from");

    // A database held deliberately at the previous version, with rows written the way
    // the previous version of the code wrote them — in its own SQL, not through
    // today's typed functions. Those select the columns the newest schema has, so a
    // migration that adds one makes them unable to speak an older schema at all. That
    // is the store's rule, not a defect: one way into the record, at one version.
    {
        let old = Store::open_at_version(&path, previous).expect("open at the previous version");
        assert_eq!(old.schema_version().unwrap(), previous);
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO reviews (id, repo, pr, head_sha, round, created_at)
                 VALUES (1, 'devergehq/review-desk', 7, 'abc123', 1, 1000);
             INSERT INTO findings
                 (review_id, seq, lens, severity, claim, disposition, created_at)
                 VALUES (1, 1, 'correctness', 'should', 'the old claim', 'in_draft', 1001);
             INSERT INTO outcomes (review_id, finding_seq, kind, actor, occurred_at)
                 VALUES (1, 1, 'kept', 'developer', 1002);",
        )
        .expect("write rows under the old schema");
    }

    // Reopened without a cap, it migrates.
    let migrated = Store::open(&path).expect("reopen migrates");

    assert_eq!(
        migrated.schema_version().unwrap(),
        newest_version(),
        "the database was migrated to the newest version"
    );

    let review = migrated
        .get_review(1)
        .expect("the row written before the migration is still there");
    assert_eq!(review.repo, "devergehq/review-desk");
    assert_eq!(review.pr, 7);
    assert_eq!(review.head_sha, "abc123");
    assert_eq!(review.round, 1);
    assert_eq!(review.pr_state, None, "the new columns start empty");
    assert_eq!(review.pr_state_at, None);
    assert_eq!(review.harvested_at, None);

    let finding = migrated.get_finding(1, 1).expect("the finding survived");
    assert_eq!(finding.claim, "the old claim");
    assert_eq!(finding.github_comment_id, None);

    let outcomes = migrated.list_outcomes(1).expect("the outcome survived");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].kind.as_str(), "kept");
    assert_eq!(outcomes[0].harvest_key, None);
}

#[test]
fn migrating_is_forward_only_and_reopening_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    let first = Store::open(&path).unwrap();
    let version = first.schema_version().unwrap();
    drop(first);

    let second = Store::open(&path).unwrap();
    assert_eq!(second.schema_version().unwrap(), version);

    // Asking for an older target does not roll anything back.
    let third = Store::open_at_version(&path, 1).unwrap();
    assert_eq!(
        third.schema_version().unwrap(),
        version,
        "a lower target leaves a newer database alone"
    );
}

#[test]
fn a_database_from_the_future_is_refused() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    Store::open(&path).unwrap();

    // Pretend a later build has been here.
    let future = newest_version() + 5;
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(&format!("PRAGMA user_version = {future}"))
            .unwrap();
    }

    match Store::open(&path) {
        Err(StoreError::FromTheFuture { found, known }) => {
            assert_eq!(found, future);
            assert_eq!(known, newest_version());
        }
        other => panic!("expected FromTheFuture, got {other:?}"),
    }
}

#[test]
fn foreign_keys_are_enforced_on_the_connection() {
    // SQLite leaves foreign keys off by default, which would make the store's parent
    // checks claims that nothing enforces.
    let (_dir, store) = temp_store();
    assert!(store.foreign_keys_enforced().unwrap());
}

#[test]
fn default_path_is_never_used_by_these_tests() {
    let (dir, store) = temp_store();
    let default = review_desk_store::default_db_path().expect("a home directory");

    assert!(store.path().starts_with(dir.path()));
    assert_ne!(store.path(), default.as_path());
}

#[test]
fn the_default_path_sits_under_the_home_directory() {
    let path = review_desk_store::default_db_path().expect("a home directory");
    assert!(path.ends_with("review-desk.db"));
    assert!(path.to_string_lossy().contains(".review-desk"));
}
