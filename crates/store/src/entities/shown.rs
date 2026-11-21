//! What the developer was shown, and the refusal when it has moved — DEV-1346.
//!
//! **Every sign-off is an act on what was on the screen, not on what the record holds when
//! the request lands.** The page is served a [`DraftShown`] with the draft and a hash with
//! the brief, and it sends them back with the act. The store compares them here, inside the
//! act's own transaction, and refuses with [`StoreError::ShownMoved`] — having written
//! nothing — when they differ.
//!
//! The hashes are computed here and nowhere else, so the page, the API and a later post
//! (DEV-1347) cannot come to disagree about how a draft is hashed.

use sha2::{Digest, Sha256};

use crate::error::{Result, StoreError};
use crate::model::{Brief, Draft, DraftShown, DraftThread};
use crate::Store;

/// SHA-256 of `bytes`, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The JSON a value is hashed over.
///
/// `serde_json` writes a struct's fields in declaration order and the lists in it come out
/// of `ORDER BY`, so the same record gives the same bytes. A field added to the type
/// changes every hash — which refuses a sign-off shown by the previous build, and that is
/// the safe direction to be wrong in.
///
/// **Infallible for what it is given.** `serde_json` fails only on a map whose keys are not
/// strings or on a `Serialize` that reports an error itself, and neither a brief nor a
/// thread has either. Were it to fail, the empty serialisation hashes to a value no page was
/// ever served, so the act is refused rather than allowed.
fn hashed_json<T: serde::Serialize>(value: &T) -> String {
    sha256_hex(&serde_json::to_vec(value).unwrap_or_default())
}

/// What a page that serves this draft and these threads is showing.
///
/// **Computed from the values being served, never read again.** A page that serves a draft
/// it read in one call and a `shown` it read in another can be handed the hashes of a draft
/// written between the two — body A on screen, B's `shown` in hand — and the approval then
/// matches B. Taken from what is served, a mixed read can only produce a `shown` that matches
/// no state of the record, so it fails closed as a `409`. Found by the self-review of
/// DEV-1346, with a probe that approved a body the page never drew.
pub fn shown_of(draft: &Draft, threads: &[DraftThread]) -> DraftShown {
    DraftShown {
        body_sha256: sha256_hex(draft.body.as_bytes()),
        threads_sha256: hashed_json(&threads),
        draft_rev: draft.rev,
    }
}

impl Store {
    /// What a developer reading this draft now is being shown.
    ///
    /// `threads_sha256` is taken over the threads as [`Store::draft_threads`] composes
    /// them, which is what posts beside the body. **For a page, use [`shown_of`] over the
    /// draft and threads actually served**; this reads both again, and is for the store's
    /// own comparison inside a transaction, where nothing can write between the two reads.
    pub fn draft_shown(&self, review_id: i64) -> Result<DraftShown> {
        let draft = self.get_draft(review_id)?;
        let threads = self.draft_threads(review_id)?;
        Ok(shown_of(&draft, &threads))
    }

    /// The hash of the brief a developer reading it now is being shown.
    ///
    /// **Taken without the confirmation**, so confirming twice from the same page is still
    /// one confirmation rather than a refusal: whether a brief is confirmed is not part of
    /// what the developer is confirming.
    pub fn brief_sha256(&self, review_id: i64) -> Result<String> {
        Ok(brief_hash(&self.get_brief(review_id)?))
    }

    /// Refuse unless the draft is still what `shown` says the developer saw.
    ///
    /// Called inside the act's transaction, before anything is written.
    pub(crate) fn refuse_unless_draft_shown(
        &self,
        review_id: i64,
        shown: &DraftShown,
    ) -> Result<()> {
        let now = self.draft_shown(review_id)?;
        let mut moved = Vec::new();
        if now.draft_rev != shown.draft_rev {
            moved.push(format!(
                "it is revision {} and you were shown revision {}",
                now.draft_rev, shown.draft_rev
            ));
        }
        if now.body_sha256 != shown.body_sha256 {
            moved.push("the body is not the one you were shown".to_string());
        }
        if now.threads_sha256 != shown.threads_sha256 {
            moved.push("the comments on the code are not the ones you were shown".to_string());
        }
        if moved.is_empty() {
            return Ok(());
        }
        Err(StoreError::ShownMoved {
            what: "draft",
            review_id,
            moved: moved.join("; "),
        })
    }

    /// Refuse unless the brief is still the one whose hash the developer was shown.
    pub(crate) fn refuse_unless_brief_shown(&self, review_id: i64, shown: &str) -> Result<()> {
        if self.brief_sha256(review_id)? == shown {
            return Ok(());
        }
        Err(StoreError::ShownMoved {
            what: "brief",
            review_id,
            moved: "the brief is not the one you were shown".to_string(),
        })
    }
}

/// A brief's hash, with the confirmation taken off it.
///
/// Public and pure, so the API can serve it beside every brief it serves without a second
/// read: it is a function of the brief and nothing else.
pub fn brief_hash(brief: &Brief) -> String {
    let mut unconfirmed = brief.clone();
    unconfirmed.confirmed_at = None;
    unconfirmed.confirmed_by = None;
    hashed_json(&unconfirmed)
}
