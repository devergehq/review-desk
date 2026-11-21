//! Posting a review through Review Desk, and the record of each attempt — DEV-1347.
//!
//! **Three steps, and the record comes first.** [`Store::begin_post`] checks that the draft
//! is still what the developer approved, makes the salt and the tag, and commits a `pending`
//! row — all before anything is sent. The caller then makes the one GitHub call, and reports
//! it with [`Store::finish_post`] or [`Store::fail_post`]. A crash between the two leaves the
//! row `pending`, and a `pending` row refuses the next post: not knowing whether a review
//! went up is a reason not to send it again.
//!
//! **Nothing here signs anything off.** A post needs a draft the developer has already
//! approved in the dashboard, and refuses one that has changed since; it never approves.

use std::io::Read;

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    DraftStatus, DraftThread, PostBasis, PostedTag, PostedTagStatus, Review, ReviewEvent,
    TagReading, Timestamp,
};
use crate::tag::{self, ApprovalStatement, Tag, APPROVAL_SCHEMA, TAG_MAX_BYTES};
use crate::Store;

const COLUMNS: &str = "id, review_id, github_review_id, status, statement, statement_hmac, \
                       line_hmac, go_ahead_line, go_ahead_at, session_id, basis, install_id, \
                       created_at, posted_at, failure";

/// What the session says about the go-ahead it is posting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRequest {
    /// The session posting.
    pub session_id: String,
    /// The line the developer typed to say go ahead, if the session has it.
    pub go_ahead_line: Option<String>,
    /// When they typed it.
    pub go_ahead_at: Option<Timestamp>,
    /// Whose word the post rests on. The caller decides it, from Allele, and falls to
    /// [`PostBasis::Desk`] on anything short of a confirmation.
    pub basis: PostBasis,
}

/// A post recorded and not yet sent: everything the one GitHub call needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPost {
    /// The `posted_tags` row to report the outcome against.
    pub tag_id: i64,
    pub review: Review,
    pub event: ReviewEvent,
    /// The approved body with the tag at its end — what is sent, byte for byte.
    pub body: String,
    pub threads: Vec<DraftThread>,
    /// The tag alone, as it ends `body`.
    pub tag: String,
}

impl Store {
    /// This database's install id: random, written once by migration 25.
    pub fn install_id(&self) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT install_id FROM installation WHERE id = 1",
            [],
            |row| row.get(0),
        )?)
    }

    /// Record a post of this review's approved draft, and hand back what to send.
    ///
    /// Refused, having written nothing, unless the draft is `approved` and is still the
    /// draft that was approved — the same body, the same threads, the same revision. Refused
    /// too while an earlier attempt is `pending`. A `failed` attempt does not stand in the
    /// way: it is known not to have gone up.
    pub fn begin_post(&self, review_id: i64, request: &PostRequest) -> Result<PendingPost> {
        let session_id = request.session_id.trim();
        if session_id.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "session_id",
                value: String::new(),
                expected: "the id of the session posting the review".to_string(),
            });
        }
        let line = request
            .go_ahead_line
            .as_deref()
            .map(tag::normalise_line)
            .filter(|line| !line.is_empty());

        self.refuse_if_dismissed(review_id, "posting its draft")?;
        let salt = salt()?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        let draft = self.get_draft(review_id)?;
        if draft.status != DraftStatus::Approved {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "only a draft the developer approved in the dashboard is posted",
            });
        }

        // **What was approved, compared with what is held now.** A draft approved before
        // DEV-1346 has no record of what was approved, and cannot be posted on the strength
        // of one: the developer approves it again.
        let approved = draft
            .approved_shown
            .clone()
            .ok_or(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "a draft approved since Review Desk records what was approved. Send it \
                     back and approve it again",
            })?;
        self.refuse_unless_draft_shown(review_id, &approved)
            .map_err(|error| match error {
                StoreError::ShownMoved {
                    what,
                    review_id,
                    moved,
                } => StoreError::ShownMoved {
                    what,
                    review_id,
                    moved: format!(
                        "it is not the draft that was approved ({moved}), so nothing was posted"
                    ),
                },
                other => other,
            })?;

        if let Some((tag_id, since)) = self
            .conn
            .query_row(
                "SELECT id, created_at FROM posted_tags WHERE review_id = ?1 AND status = 'pending'",
                params![review_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        {
            return Err(StoreError::PostInFlight {
                review_id,
                tag_id,
                since,
            });
        }

        // The hand-over holds the last two lines: an approved body its blocks no longer
        // compose to is refused there, and the threads are composed there.
        let handover = self.draft_handover(review_id)?;
        let review = handover.review;
        // **Every comment is pinned to the head, so every line must have been counted on it.**
        // A line with no recorded commit is let through, as the hand-over lets it through:
        // nothing backfills the column, and refusing every older finding would refuse every
        // post. One recorded on another commit is the case that can land silently wrong.
        if let Some(thread) = handover.threads.iter().find(|thread| {
            thread
                .line_sha
                .as_deref()
                .is_some_and(|sha| !sha.is_empty() && !same_commit(sha, &review.head_sha))
        }) {
            return Err(StoreError::ThreadOffHead {
                review_id,
                finding_seq: thread.finding_seq,
                line_sha: thread.line_sha.clone().unwrap_or_default(),
                head_sha: review.head_sha.clone(),
            });
        }
        let event = draft.event.ok_or(StoreError::DraftStatus {
            review_id,
            found: draft.status,
            wanted: "an approved draft with the event it is to be posted as",
        })?;

        let statement = ApprovalStatement {
            schema: APPROVAL_SCHEMA,
            review_id,
            repo: review.repo.clone(),
            pr: review.pr,
            head_sha: review.head_sha.clone(),
            round: review.round,
            event,
            body_sha256: approved.body_sha256,
            threads_sha256: approved.threads_sha256,
            draft_rev: approved.draft_rev,
        }
        .canonical();

        let install_id = self.install_id()?;
        let tag = Tag {
            install: install_id.clone(),
            basis: request.basis,
            statement_hmac: tag::statement_hmac(&salt, &statement),
            line_hmac: line.as_deref().map(|line| tag::line_hmac(&salt, line)),
        };
        let rendered = tag.render();
        debug_assert!(rendered.len() <= TAG_MAX_BYTES);
        let body = tag::tagged_body(&draft.body, &tag);
        // **What is sent must read back as this tag and no other.** A body that already holds
        // something parsing as a tag would post as two, and be rejected on the way back.
        if tag::parse(&body) != Ok(Some(tag.clone())) {
            return Err(StoreError::InvalidValue {
                field: "body",
                value: "a body that already carries a review-desk:posted tag".to_string(),
                expected: "a body with no tag of its own; the post adds the one tag".to_string(),
            });
        }

        let at = self.now();
        self.conn.execute(
            "INSERT INTO posted_tags
                 (review_id, status, salt, statement, statement_hmac, line_hmac, go_ahead_line,
                  go_ahead_at, session_id, basis, install_id, created_at)
             VALUES (?1, 'pending', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                review_id,
                salt.as_slice(),
                statement,
                tag.statement_hmac,
                tag.line_hmac,
                line,
                request.go_ahead_at,
                session_id,
                request.basis,
                install_id,
                at,
            ],
        )?;
        let tag_id = self.conn.last_insert_rowid();

        self.mark_review_active(review_id)?;
        tx.commit()?;

        Ok(PendingPost {
            tag_id,
            body,
            review,
            event,
            threads: handover.threads,
            tag: rendered,
        })
    }

    /// GitHub accepted the post: record the review it became, and mark the draft posted.
    ///
    /// One transaction, so the tag row and the draft cannot disagree about whether the
    /// review went up.
    pub fn finish_post(&self, tag_id: i64, github_review_id: &str) -> Result<PostedTag> {
        let github_review_id = github_review_id.trim();
        if github_review_id.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "github_review_id",
                value: String::new(),
                expected: "the id of the review posted on GitHub".to_string(),
            });
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let row = self.pending_tag(tag_id)?;
        self.conn.execute(
            "UPDATE posted_tags SET status = 'posted', github_review_id = ?2, posted_at = ?3
             WHERE id = ?1",
            params![tag_id, github_review_id, self.now()],
        )?;
        self.record_posted(row.review_id, github_review_id)?;
        tx.commit()?;
        self.get_posted_tag(tag_id)
    }

    /// The GitHub call failed: nothing went up, and the review may be posted again.
    pub fn fail_post(&self, tag_id: i64, failure: &str) -> Result<PostedTag> {
        let failure = match failure.trim() {
            "" => "the call failed and said nothing",
            failure => failure,
        };
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.pending_tag(tag_id)?;
        self.conn.execute(
            "UPDATE posted_tags SET status = 'failed', failure = ?2 WHERE id = ?1",
            params![tag_id, failure],
        )?;
        tx.commit()?;
        self.get_posted_tag(tag_id)
    }

    /// The attempt still in flight for this review, if there is one.
    pub fn pending_post(&self, review_id: i64) -> Result<Option<PostedTag>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM posted_tags WHERE review_id = ?1 AND status = 'pending'"
                ),
                params![review_id],
                row_to_posted_tag,
            )
            .optional()?)
    }

    /// One attempt, by its id.
    pub fn get_posted_tag(&self, tag_id: i64) -> Result<PostedTag> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM posted_tags WHERE id = ?1"),
                params![tag_id],
                row_to_posted_tag,
            )
            .optional()?
            .ok_or(StoreError::NotFound {
                entity: "posted tag",
                id: tag_id.to_string(),
            })
    }

    /// Every attempt to post this review, oldest first.
    pub fn posted_tags(&self, review_id: i64) -> Result<Vec<PostedTag>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM posted_tags WHERE review_id = ?1 ORDER BY id"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_posted_tag)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What a review body read back from GitHub says about how it was posted.
    ///
    /// **Held means the HMAC is recomputed, not merely found.** A tag is held when this
    /// database wrote it — the same install id — and a row it holds, keyed with that row's
    /// salt over that row's statement, gives the tag's `statement` and `basis`. Anything
    /// short of that is "this Mac has no record of it".
    pub fn read_tag(&self, body: &str) -> Result<TagReading> {
        let tag = match tag::parse(body) {
            Ok(Some(tag)) => tag,
            Ok(None) => return Ok(TagReading::NoTag),
            Err(reason) => return Ok(TagReading::Rejected { reason }),
        };
        if tag.install != self.install_id()? {
            return Ok(TagReading::NotHeld);
        }

        let mut statement = self.conn.prepare(
            "SELECT review_id, salt, statement, basis, line_hmac FROM posted_tags
             WHERE statement_hmac = ?1 AND status IN ('posted', 'pending')",
        )?;
        let rows = statement.query_map(params![tag.statement_hmac], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, PostBasis>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (review_id, salt, held, basis, line_hmac) = row?;
            if tag::statement_hmac(&salt, &held) == tag.statement_hmac
                && basis == tag.basis
                && line_hmac == tag.line_hmac
            {
                return Ok(TagReading::Held { basis, review_id });
            }
        }
        Ok(TagReading::NotHeld)
    }

    /// The attempt, which must still be `pending`.
    fn pending_tag(&self, tag_id: i64) -> Result<PostedTag> {
        let row = self.get_posted_tag(tag_id)?;
        if row.status != PostedTagStatus::Pending {
            return Err(StoreError::InvalidValue {
                field: "posted_tag_status",
                value: row.status.to_string(),
                expected: "pending: only an attempt still in flight has an outcome to report"
                    .to_string(),
            });
        }
        Ok(row)
    }
}

/// Whether two commits are the same one, when either may be abbreviated.
///
/// The rule `same_commit` in `crates/cli/src/commands/draft.rs` states, for the same pair:
/// a case-insensitive prefix match on the shorter, and blank matches nothing.
fn same_commit(one: &str, two: &str) -> bool {
    let shortest = one.len().min(two.len());
    shortest > 0
        && one
            .get(..shortest)
            .zip(two.get(..shortest))
            .is_some_and(|(one, two)| one.eq_ignore_ascii_case(two))
}

/// 32 bytes from the operating system.
///
/// `/dev/urandom` rather than a crate, as the server's sign-in token does it: it is the
/// operating system's own generator, and a salt has no need of anything else.
fn salt() -> Result<[u8; 32]> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(StoreError::NoRandomness)?;
    Ok(bytes)
}

fn row_to_posted_tag(row: &Row<'_>) -> rusqlite::Result<PostedTag> {
    Ok(PostedTag {
        id: row.get("id")?,
        review_id: row.get("review_id")?,
        github_review_id: row.get("github_review_id")?,
        status: row.get("status")?,
        statement: row.get("statement")?,
        statement_hmac: row.get("statement_hmac")?,
        line_hmac: row.get("line_hmac")?,
        go_ahead_line: row.get("go_ahead_line")?,
        go_ahead_at: row.get("go_ahead_at")?,
        session_id: row.get("session_id")?,
        basis: row.get("basis")?,
        install_id: row.get("install_id")?,
        created_at: row.get("created_at")?,
        posted_at: row.get("posted_at")?,
        failure: row.get("failure")?,
    })
}
