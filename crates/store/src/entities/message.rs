//! What the developer and a review's session say to each other. Migration 22.
//!
//! **The conversation is kept with the review, and Review Desk delivers none of it.**
//! DEV-1140 (option A): the developer writes from the dashboard, the record keeps it, the
//! review worker's watcher in Locus reads [`Store::unread_messages`] and hands it to the
//! session, and the session answers with `review-desk message reply`. Review Desk never
//! writes into a session, so it still depends on nothing.
//!
//! Three verbs, split by who may use them, which is the whole of the safety argument:
//!
//! | Verb | Who | Through |
//! |---|---|---|
//! | [`Store::ask_session`] | the developer | the dashboard only — no command writes a developer's words |
//! | [`Store::reply_from_session`] | the session | `review-desk message reply` |
//! | [`Store::mark_message_read`] | whatever delivered it | `review-desk message read` |
//!
//! **No message approves, confirms or posts.** There is no kind that does and no column a
//! message could set that would. A send-back is recorded here as a message too, but it is
//! made by [`Store::send_back_draft`]'s own caller, through the draft's own rules; this
//! module only keeps the note in the conversation.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{MessageAuthor, MessageKind, ReviewLayerName, ReviewMessage, Timestamp};
use crate::Store;

/// The columns a [`ReviewMessage`] is read from, in the order [`row_to_message`] takes them.
const MESSAGE_COLUMNS: &str = "review_id, seq, step, author, kind, body, in_reply_to,
                               created_at, read_at, read_by_session";

fn row_to_message(row: &Row) -> rusqlite::Result<ReviewMessage> {
    Ok(ReviewMessage {
        review_id: row.get(0)?,
        seq: row.get(1)?,
        step: row.get(2)?,
        author: row.get(3)?,
        kind: row.get(4)?,
        body: row.get(5)?,
        in_reply_to: row.get(6)?,
        created_at: row.get(7)?,
        read_at: row.get(8)?,
        read_by_session: row.get(9)?,
    })
}

/// A body with something in it, or the refusal that says what was missing.
fn worded(body: &str, expected: &str) -> Result<String> {
    let body = body.trim();
    if body.is_empty() {
        return Err(StoreError::InvalidValue {
            field: "body",
            value: String::new(),
            expected: expected.to_string(),
        });
    }
    Ok(body.to_string())
}

impl Store {
    /// The developer asks the review's session something, on one step of the review.
    ///
    /// **The developer's act, and there is no command for it.** Like confirming a brief and
    /// signing off a draft, the developer's words reach the record from the dashboard alone,
    /// so a session cannot write a question and then answer it as if it had been asked.
    pub fn ask_session(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        body: &str,
    ) -> Result<ReviewMessage> {
        let body = worded(body, "what to ask the session")?;
        self.add_message(
            review_id,
            step,
            MessageAuthor::Developer,
            MessageKind::Ask,
            &body,
            None,
        )
    }

    /// Keep the note a draft was sent back with in the conversation, on the draft step.
    ///
    /// **Recording, not sending.** The send-back itself is [`Store::send_back_draft`], with
    /// its own refusals; the caller makes it first and records the note here after it has
    /// succeeded, so a refused send-back leaves no message saying it happened.
    pub fn record_send_back_message(&self, review_id: i64, note: &str) -> Result<ReviewMessage> {
        let note = worded(note, "what the session is to change")?;
        self.add_message(
            review_id,
            ReviewLayerName::Draft,
            MessageAuthor::Developer,
            MessageKind::SendBack,
            &note,
            None,
        )
    }

    /// The session answers, optionally naming the message it answers.
    ///
    /// `in_reply_to` must be one of the developer's messages on this review: a session
    /// answering its own reply, or a number that is not there, is refused rather than
    /// stored as a thread that points nowhere.
    pub fn reply_from_session(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        body: &str,
        in_reply_to: Option<i64>,
    ) -> Result<ReviewMessage> {
        let body = worded(body, "the session's answer")?;
        if let Some(seq) = in_reply_to {
            let asked = self.message(review_id, seq)?;
            if asked.author != MessageAuthor::Developer {
                return Err(StoreError::InvalidValue {
                    field: "in_reply_to",
                    value: seq.to_string(),
                    expected: "the number of one of the developer's messages on this review"
                        .to_string(),
                });
            }
        }
        self.add_message(
            review_id,
            step,
            MessageAuthor::Session,
            MessageKind::Reply,
            &body,
            in_reply_to,
        )
    }

    /// Mark one of the developer's messages as delivered to a session.
    ///
    /// **The first reading stands.** Marking a message read twice — a watcher restarted
    /// between delivering it and the session acting — changes nothing and answers the
    /// message as it was first read, so a delivery is never recorded as later than it was.
    pub fn mark_message_read(
        &self,
        review_id: i64,
        seq: i64,
        session_id: Option<&str>,
    ) -> Result<ReviewMessage> {
        let message = self.message(review_id, seq)?;
        if message.author != MessageAuthor::Developer {
            return Err(StoreError::InvalidValue {
                field: "seq",
                value: seq.to_string(),
                expected: "one of the developer's messages: a session's reply is read by being \
                           on the page, not by being marked"
                    .to_string(),
            });
        }
        if message.read_at.is_some() {
            return Ok(message);
        }
        let session = session_id.map(str::trim).filter(|id| !id.is_empty());
        self.conn.execute(
            "UPDATE review_messages SET read_at = ?1, read_by_session = ?2
              WHERE review_id = ?3 AND seq = ?4 AND read_at IS NULL",
            params![self.now(), session, review_id, seq],
        )?;
        self.message(review_id, seq)
    }

    /// Every message on a review, oldest first.
    pub fn messages(&self, review_id: i64) -> Result<Vec<ReviewMessage>> {
        self.get_review(review_id)?;
        let mut statement = self.conn.prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM review_messages WHERE review_id = ?1 ORDER BY seq"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_message)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The developer's messages nothing has delivered yet, on one review or on all of them.
    ///
    /// What a watcher reads on every tick. Oldest first across reviews, so a caller serving
    /// one at a time serves what has waited longest.
    pub fn unread_messages(&self, review_id: Option<i64>) -> Result<Vec<ReviewMessage>> {
        if let Some(id) = review_id {
            self.get_review(id)?;
        }
        let mut statement = self.conn.prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM review_messages
              WHERE author = 'developer' AND read_at IS NULL
                AND (?1 IS NULL OR review_id = ?1)
              ORDER BY created_at, review_id, seq"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_message)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One message, by its number on the review.
    pub fn message(&self, review_id: i64, seq: i64) -> Result<ReviewMessage> {
        self.get_review(review_id)?;
        self.conn
            .query_row(
                &format!(
                    "SELECT {MESSAGE_COLUMNS} FROM review_messages
                      WHERE review_id = ?1 AND seq = ?2"
                ),
                params![review_id, seq],
                row_to_message,
            )
            .optional()?
            .ok_or(StoreError::NotFound {
                entity: "message",
                id: format!("{seq} on review {review_id}"),
            })
    }

    fn add_message(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        author: MessageAuthor,
        kind: MessageKind,
        body: &str,
        in_reply_to: Option<i64>,
    ) -> Result<ReviewMessage> {
        self.get_review(review_id)?;
        let now: Timestamp = self.now();
        // IMMEDIATE, so two writers cannot both read the same highest `seq` and collide on
        // the unique key — the developer asking while the session replies is the ordinary
        // case, not a contrived one.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let seq: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM review_messages WHERE review_id = ?1",
            params![review_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO review_messages
                 (review_id, seq, step, author, kind, body, in_reply_to, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![review_id, seq, step, author, kind, body, in_reply_to, now],
        )?;
        let written = self.message(review_id, seq)?;
        tx.commit()?;
        Ok(written)
    }
}
