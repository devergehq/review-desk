//! Test scaffolding.
//!
//! Every store in these tests lives in a `TempDir`. Nothing here ever touches
//! `~/.review-desk/review-desk.db`, and `default_path_is_never_used` asserts it.
//!
//! Each integration test file compiles this module into its own binary, so a helper
//! that one of them does not use is dead code there. That is what the allow is for.
#![allow(dead_code)]

use std::path::PathBuf;

use review_desk_store::Store;
use tempfile::TempDir;

/// A store in a fresh temporary directory, migrated to the newest version.
///
/// The `TempDir` is returned alongside it and must be kept alive; dropping it deletes
/// the database.
pub fn temp_store() -> (TempDir, Store) {
    let dir = TempDir::new().expect("temporary directory");
    let store = Store::open(db_path(&dir)).expect("open store");
    (dir, store)
}

/// The database path inside a temporary directory.
pub fn db_path(dir: &TempDir) -> PathBuf {
    dir.path().join("review-desk.db")
}

/// Record a round as posted: the one thing that makes the next commit a round of its own.
///
/// **Since DEV-962 a new commit on an unposted round moves that round's head**, so a test
/// that needs two rounds has to post the first — which is what a real pull request does
/// between them. Recorded the way a session records it, with the id of the review it posted;
/// Review Desk posts nothing itself, and `review link --github-review` is the one way the
/// command line says a review was posted at all.
pub fn posted(store: &Store, review_id: i64) {
    store
        .set_review_links(
            review_id,
            review_desk_store::ReviewLinks {
                github_review_id: Some(format!("gh-review-{review_id}")),
                ..Default::default()
            },
        )
        .expect("record the round as posted");
}

/// A review to create, with everything but the identity left out.
pub fn new_review(repo: &str, pr: i64, head: &str, round: i64) -> review_desk_store::NewReview {
    review_desk_store::NewReview {
        repo: repo.to_string(),
        pr,
        head_sha: head.to_string(),
        round,
        ..Default::default()
    }
}
