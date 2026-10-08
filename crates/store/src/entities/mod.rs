//! The typed create and read functions, one module per entity.
//!
//! Each module adds an `impl Store` block, so a caller sees one flat API on `Store`
//! rather than seven sub-objects.

mod brief;
mod draft;
mod finding;
mod layer;
mod outcome;
mod review;
mod rule;

use crate::error::{Result, StoreError};
use crate::model::Actor;

/// Refuse an actor other than the developer.
///
/// **Only the developer signs off** — confirming a brief and approving a draft are
/// theirs, decided on 9 October 2026. Those two functions take the actor as a required
/// argument so the record says whose assent was given, and there is exactly one value
/// it may hold. Taking the argument and then ignoring it would make the record's
/// `confirmed_by` a field nobody could trust.
///
/// Review Desk has no authentication — the server binds `127.0.0.1` and reaching the
/// port is the authorisation — so this is not a security control. It is the record
/// refusing to hold a claim that the harvester, or an agent, signed something off.
pub(crate) fn only_the_developer(by: Actor, act: &'static str) -> Result<()> {
    if by == Actor::Developer {
        return Ok(());
    }

    Err(StoreError::InvalidValue {
        field: "by",
        value: by.to_string(),
        expected: format!("developer; only the developer can {act}"),
    })
}
