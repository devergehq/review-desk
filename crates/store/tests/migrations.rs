//! What opening a database does: create, migrate, refuse.

mod common;

use common::{db_path, new_review, temp_store};
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

    // A database held deliberately at version 1, with a row in it.
    let review_id = {
        let old = Store::open_at_version(&path, 1).expect("open at version 1");
        assert_eq!(old.schema_version().unwrap(), 1);
        assert!(
            newest_version() > 1,
            "this test needs a later migration to migrate to"
        );

        let review = old
            .create_review(new_review("devergehq/review-desk", 7, "abc123", 1))
            .expect("create a review under the old schema");
        review.id
    };

    // Reopened without a cap, it migrates.
    let migrated = Store::open(&path).expect("reopen migrates");

    assert_eq!(
        migrated.schema_version().unwrap(),
        newest_version(),
        "the database was migrated to the newest version"
    );

    let review = migrated
        .get_review(review_id)
        .expect("the row written before the migration is still there");
    assert_eq!(review.repo, "devergehq/review-desk");
    assert_eq!(review.pr, 7);
    assert_eq!(review.head_sha, "abc123");
    assert_eq!(review.round, 1);
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
