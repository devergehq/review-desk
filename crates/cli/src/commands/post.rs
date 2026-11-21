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
//!    posted in the same transaction, or `failed` with what `gh` said.
//!
//! **It signs nothing off.** The draft was approved in the dashboard before this ran, and
//! `crates/cli/tests/signoff.rs` still holds that line over this file.
//!
//! **`draft handover` and `draft posted` still work**, so a session can still post with raw
//! `gh`. A review posted that way carries no tag, and reads as posted without going through
//! Review Desk.

use std::path::PathBuf;

use clap::Args;
use review_desk_harvest::gh::Gh;
use review_desk_harvest::HarvestError;
use review_desk_server::allele::GoAhead;
use review_desk_server::Allele;
use review_desk_store::{PostBasis, PostRequest, Store};
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

    /// Where to ask Allele whether that line was typed in that session. Defaults to
    /// `~/.allele/control.sock`. With no Allele, or an Allele that cannot say, the post's
    /// basis is `desk`.
    #[arg(long, value_name = "PATH")]
    allele_socket: Option<PathBuf>,
}

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
    let allele = args
        .allele_socket
        .clone()
        .or_else(Allele::default_socket)
        .map_or_else(Allele::none, Allele::at);
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

    let posted = match Gh::new().post_review(
        &pending.review.repo,
        pending.review.pr,
        &payload.to_string(),
    ) {
        Ok(github_review_id) => store.finish_post(pending.tag_id, &github_review_id)?,
        // **`gh` answered, and the answer could not be read.** The review may well be up, so
        // the attempt stays `pending` and the next post is refused: that is the one outcome
        // that must not be retried blind.
        Err(error @ HarvestError::Unreadable { .. }) => return Err(error.into()),
        Err(error) => {
            store.fail_post(pending.tag_id, &error.to_string())?;
            return Err(error.into());
        }
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
