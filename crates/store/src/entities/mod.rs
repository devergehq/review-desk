//! The typed create and read functions, one module per entity.
//!
//! Each module adds an `impl Store` block, so a caller sees one flat API on `Store`
//! rather than one sub-object per table.

mod brief;
pub(crate) mod brief_structure;
pub(crate) mod composition;
pub(crate) mod decision;
pub(crate) mod draft;
mod finding;
mod finding_evidence;
mod fresh_session;
mod harvest;
pub(crate) mod house_style;
pub(crate) mod inbox;
mod layer;
mod message;
pub(crate) mod outcome;
pub(crate) mod provenance;
mod request;
pub(crate) mod review;
mod round;
mod rule;
mod tidy;
mod timings;
mod work;

use crate::error::{Result, StoreError};
use crate::model::Actor;
use crate::normalise;

/// Check and normalise the pull request a record is keyed on, and hand back the
/// repository in the form the record spells it.
///
/// **One copy, because there are now two records keyed on a pull request.** Every table
/// here hung off `reviews (id)` until DEV-996 added `review_requests`, which is keyed on
/// `(repo, pr)` and on nothing else — so the rule that was inside
/// [`crate::entities::review`]'s own `validate` is here instead. Two copies of it is the
/// bug it prevents: a request filed under `Globex/portal` against rounds filed under
/// `globex/portal` is two pull requests to the record and one to GitHub, so the queue
/// would draw the round's row and the request's row for one review and the subtraction
/// that is supposed to stop that would never see them as the same thing.
///
/// Checking one form and storing another is the same bug wearing a hat, which is why the
/// canonical spelling is what comes back rather than the caller's.
pub(crate) fn pull_request_on_the_record(repo: &str, pr: i64) -> Result<String> {
    let normalised = normalise::repo(repo);
    // **The characters and the segments, through the one copy of that rule.** This checked
    // only that both halves were non-empty, so `acme/portal#x` was stored — and a header
    // then built `https://github.com/acme/portal#x/pull/412`, which GitHub serves as the
    // repository page rather than the pull request: a link whose text and destination name
    // two different things, which is the defect DEV-936 exists to remove, reached through
    // the one field nobody thought was free text.
    //
    // The first fix for that wrote the character class here and missed `..`, which
    // `wire::is_repo_name` had refused all along and said why. `normalise::is_repo_name` is
    // now the only copy, and it carries both the rule and the reason.
    if !normalise::is_repo_name(&normalised) {
        return Err(StoreError::InvalidValue {
            field: "repo",
            value: repo.to_string(),
            expected: concat!(
                "a repository as owner/repo, in letters, digits, dot, underscore and ",
                "hyphen, and neither half `.` nor `..`",
            )
            .to_string(),
        });
    }

    if pr < 1 {
        return Err(StoreError::InvalidValue {
            field: "pr",
            value: pr.to_string(),
            expected: "a pull request number of 1 or more".to_string(),
        });
    }

    Ok(normalised)
}

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
