//! **The only module in this crate that calls `review-desk-store`.**
//!
//! Two reasons it is one file. The project's first rule of interaction is that the store
//! crate is the only code that opens the database, and a server with store calls spread
//! across a dozen handlers makes that rule hard to check. The second is immediate:
//! DEV-860 is changing the store's signatures right now — a new migration, new queries,
//! and `confirm_brief` gaining a required actor argument. Keeping every call in one
//! module makes that rebase a change to one file instead of a change to the server.
//!
//! The wrappers are deliberately thin. They add exactly two things the handlers need and
//! the store does not owe them: a lock around the one connection, and the contract's
//! refusals for states the store expresses as an absent row.

use std::sync::{Arc, Mutex};

use review_desk_store::{
    Brief, Disposition, Finding, OpenedReview, Review, Store, StoreError, Timestamp,
};

use crate::error::{ApiError, Code};

/// The open database, shared by every worker thread.
///
/// One connection behind a mutex, rather than a connection per thread. SQLite serialises
/// writes anyway, this server has one user, and a pool would mean the store crate handing
/// out connections — which is a change to the store, and DEV-861 is not allowed one.
#[derive(Debug, Clone)]
pub struct Records {
    store: Arc<Mutex<Store>>,
}

impl Records {
    /// Wrap an open store.
    pub fn new(store: Store) -> Self {
        Records {
            store: Arc::new(Mutex::new(store)),
        }
    }

    /// Run one piece of work against the store.
    ///
    /// A poisoned lock means a worker thread panicked while holding it, so what the
    /// database holds is unknown. That is Review Desk being unable to run rather than a
    /// refusal, which is what `store_unavailable` means.
    fn with<T>(&self, work: impl FnOnce(&Store) -> Result<T, StoreError>) -> Result<T, ApiError> {
        let store = self.store.lock().map_err(|_| ApiError {
            code: Code::StoreUnavailable,
            message: "the database lock was left poisoned by a failed request".to_string(),
            field: None,
            expected: None,
        })?;
        work(&store).map_err(ApiError::from)
    }

    /// The schema version, for the health endpoint. The cheapest honest proof that the
    /// database is open and migrated, rather than that the process is merely running.
    pub fn schema_version(&self) -> Result<i64, ApiError> {
        self.with(|store| store.schema_version())
    }

    /// Open the review for this head commit: the one already on the record, or a new
    /// round. `review open` in DEV-851 is the same call, so the two surfaces cannot
    /// disagree about what "the same review" is.
    pub fn open_review(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
    ) -> Result<OpenedReview, ApiError> {
        self.with(|store| store.open_review(repo, pr, head_sha))
    }

    /// One review by id, as a refusal when it is not there.
    pub fn review(&self, id: i64) -> Result<Review, ApiError> {
        self.with(|store| store.find_review_by_id(id))?
            .ok_or_else(|| ApiError::not_found(format!("no review with id {id}")))
    }

    /// The review's brief, or a `conflict`.
    ///
    /// The contract gives `confirmBrief` a 409 for "the review has no brief to confirm",
    /// which the store reports as an absent row. A caller that asked to confirm a brief
    /// that was never written has made a request about a review that does exist, so 404
    /// would be the wrong answer and the contract says as much.
    pub fn brief_to_confirm(&self, review_id: i64) -> Result<Brief, ApiError> {
        self.with(|store| store.find_brief(review_id))?
            .ok_or_else(|| {
                ApiError::conflict(format!("review {review_id} has no brief to confirm"))
            })
    }

    /// Confirm the brief. The developer's act at layer 2.
    ///
    /// **This is a sign-off endpoint, and only the developer can sign off** — Patrick,
    /// 9 October 2026. On `master` the store records the confirmation time and nothing
    /// about who made it; there is no actor column and no argument to pass one to.
    /// DEV-860 adds the argument, and the only value this server will ever pass is
    /// `Actor::Developer`: this API is the sole surface where a brief can be confirmed,
    /// and the command line agents use has no command for it.
    pub fn confirm_brief(&self, review_id: i64) -> Result<Brief, ApiError> {
        self.with(|store| store.confirm_brief(review_id))
    }

    /// One finding by review and sequence number.
    pub fn finding(&self, review_id: i64, seq: i64) -> Result<Finding, ApiError> {
        self.with(|store| store.find_finding(review_id, seq))?
            .ok_or_else(|| ApiError::not_found(format!("review {review_id} has no finding {seq}")))
    }

    /// Move a finding between the draft, held back and suppressed.
    pub fn set_finding_disposition(
        &self,
        review_id: i64,
        seq: i64,
        disposition: Disposition,
        reason: Option<String>,
    ) -> Result<Finding, ApiError> {
        self.with(|store| store.set_finding_disposition(review_id, seq, disposition, reason))
    }
}

/// The current time, as the store counts it. Handlers do not read the clock themselves.
pub fn now() -> Timestamp {
    review_desk_store::now()
}
