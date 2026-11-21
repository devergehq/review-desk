//! `review-desk post`: the session posts the approved review through Review Desk — DEV-1347.
//!
//! **This is the first command that writes to GitHub**, and it writes one thing: the review
//! the developer approved in the dashboard, as one `POST pulls/<n>/reviews` through the
//! developer's own `gh` login. Review Desk stores no GitHub token.
//!
//! In order, and each step only if the one before it held:
//!
//! 1. The store rebuilds what was approved and refuses — posting nothing — unless the draft
//!    is approved and is still the draft that was approved. It refuses too while an earlier
//!    attempt is in flight.
//! 2. It commits a `pending` row holding the salt, the approval statement and the tag.
//! 3. This sends the body, **with the tag already at its end**, the threads, the event and
//!    `commit_id` pinned to the reviewed head, in one call.
//! 4. The answer is recorded: `posted` with the GitHub review's id, which marks the draft
//!    posted in the same transaction, or `failed` when GitHub certainly created nothing.
//!    Any other outcome stays `pending`, and the next `post` looks for that attempt's tag on
//!    the pull request before it sends anything.
//!
//! **It signs nothing off.** The draft was approved in the dashboard before this ran, and
//! `crates/cli/tests/signoff.rs` still holds that line over this file.
//!
//! **`draft handover` and `draft posted` still work**, so a session can still post with raw
//! `gh`. A review posted that way carries no tag, and reads as posted without going through
//! Review Desk.

use std::path::PathBuf;

use clap::Args;
use review_desk_harvest::gh::{Gh, POST_DEADLINE};
use review_desk_server::allele::GoAhead;
use review_desk_server::Allele;
use review_desk_store::{tag, PostBasis, PostRequest, Store};
use serde_json::json;

use crate::error::Result;
use crate::input::read_text;
use crate::report::Report;

#[derive(Debug, Args)]
pub struct PostArgs {
    /// The review's id.
    #[arg(value_name = "REVIEW")]
    review: i64,

    /// The id of the session posting it — the Allele session the developer said go ahead in.
    #[arg(long, value_name = "ID")]
    session: String,

    /// When the developer said go ahead, in Unix seconds.
    #[arg(long = "go-ahead-at", value_name = "TS")]
    go_ahead_at: i64,

    /// A file holding the line the developer typed to say go ahead, or `-` for standard
    /// input. Kept in the record and never posted: the tag carries only a salted HMAC of it.
    #[arg(long = "go-ahead-file", value_name = "PATH")]
    go_ahead_file: Option<PathBuf>,
}

/// How old a `pending` attempt has to be before `post` goes and looks for it on the pull
/// request, in seconds.
///
/// **Younger than this, it may still be in flight** in another process, and a lookup that
/// missed it would mark a live call failed and let a second one start. Every write is killed
/// at [`POST_DEADLINE`], so an attempt older than this has certainly finished one way or the
/// other; the assertion below keeps the two in that order.
const RECONCILE_AFTER: i64 = 120;
const _: () = assert!(RECONCILE_AFTER as u64 >= 2 * POST_DEADLINE.as_secs());

pub fn run(args: &PostArgs) -> Result<Report> {
    // Read before the database is opened, as every command does.
    let line = match &args.go_ahead_file {
        Some(path) => Some(read_text(path)?),
        None => None,
    };

    let store = Store::open_default()?;
    let review = store.get_review(args.review)?;

    // **The basis falls toward the weaker label.** `typed` only when there is a line, the
    // session posting is the session the review is linked to, and Allele says without
    // qualification that the developer typed that line there.
    //
    // **Allele is asked at its own socket and nowhere else.** No flag and no `HOME` can point
    // this at another one: the session whose word is being checked would be choosing who
    // checks it. `own_socket` reads the passwd entry, and `verify_go_ahead` also refuses a
    // socket that is not the owner's alone (0600).
    let allele = Allele::own_socket().map_or_else(Allele::none, Allele::at);
    let (basis, basis_because) = match line.as_deref().map(str::trim) {
        None | Some("") => (PostBasis::Desk, "no go-ahead line was given".to_string()),
        Some(_) if review.session_id.as_deref() != Some(args.session.trim()) => (
            PostBasis::Desk,
            "the session posting is not the session this review is linked to".to_string(),
        ),
        Some(text) => match allele.verify_go_ahead(&args.session, text, Some(args.go_ahead_at)) {
            GoAhead::Typed => (PostBasis::Typed, "Allele saw it typed".to_string()),
            GoAhead::Unconfirmed(why) => (PostBasis::Desk, why),
        },
    };

    let gh = Gh::new();

    // **An attempt whose outcome is unknown is settled before another is made.** It may be
    // up: look on the pull request for a review carrying its tag. Found, it is recorded as
    // posted and nothing is sent; not found, it is marked failed and this post goes ahead.
    // An attempt younger than [`RECONCILE_AFTER`] is left alone, and the store refuses.
    if let Some(earlier) = store.pending_post(args.review)? {
        if review_desk_store::now() - earlier.created_at >= RECONCILE_AFTER {
            let reviews = gh.review_bodies(&review.repo, review.pr)?;
            let found = reviews.into_iter().find(|(_, body)| {
                tag::parse(body).is_ok_and(|tag| {
                    tag.is_some_and(|tag| tag.statement_hmac == earlier.statement_hmac)
                })
            });
            match found {
                Some((github_review_id, _)) => {
                    let posted = store.finish_post(earlier.id, &github_review_id)?;
                    return Ok(Report::new(
                        json!({ "posted": posted, "reconciled": true }),
                        format!(
                            "review {} was already up on {}#{} as GitHub review {}, from an \
                             earlier attempt; nothing was sent",
                            posted.review_id, review.repo, review.pr, github_review_id
                        ),
                    ));
                }
                None => {
                    store.fail_post(
                        earlier.id,
                        "not on the pull request when the next post looked for it",
                    )?;
                }
            }
        }
    }

    let pending = store.begin_post(
        args.review,
        &PostRequest {
            session_id: args.session.clone(),
            go_ahead_line: line,
            go_ahead_at: Some(args.go_ahead_at),
            basis,
        },
    )?;

    let payload = json!({
        "commit_id": pending.review.head_sha,
        "event": pending.event.as_str().to_ascii_uppercase(),
        "body": pending.body,
        "comments": pending
            .threads
            .iter()
            .map(|thread| json!({
                "path": thread.path,
                "line": thread.line,
                "side": thread.side,
                "body": thread.body,
            }))
            .collect::<Vec<_>>(),
    });

    let posted = match gh.post_review(
        &pending.review.repo,
        pending.review.pr,
        &payload.to_string(),
    ) {
        Ok(github_review_id) => store.finish_post(pending.tag_id, &github_review_id)?,
        // **Only a call GitHub certainly refused is `failed`.** A 4xx, or `gh` missing or
        // logged out: nothing was created, and a retry is safe.
        Err(error) if error.certainly_not_sent() => {
            store.fail_post(pending.tag_id, &error.to_string())?;
            return Err(error.into());
        }
        // **Anything else may have created the review** — a 5xx, a timeout, an answer that
        // could not be read. The attempt stays `pending`, and the next post looks for it on
        // the pull request before it sends anything.
        Err(error) => return Err(error.into()),
    };

    Ok(Report::new(
        json!({ "posted": posted, "tag": pending.tag, "basis_because": basis_because }),
        format!(
            "review {} is posted on {}#{} as GitHub review {} (basis {}: {})",
            posted.review_id,
            pending.review.repo,
            pending.review.pr,
            posted.github_review_id.as_deref().unwrap_or_default(),
            posted.basis,
            basis_because,
        ),
    ))
}
