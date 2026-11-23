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
//! | [`Store::reply_from_session`] | the session, in prose | `review-desk message reply --body` |
//! | [`Store::answer_from_session`] | the session, as the contract | `review-desk message reply --answer` |
//! | [`Store::mark_message_read`] | whatever delivered it | `review-desk message read` |
//!
//! **No message approves, confirms or posts.** There is no kind that does and no column a
//! message could set that would. A send-back is recorded here as a message too, but it is
//! made by [`Store::send_back_draft`]'s own caller, through the draft's own rules; this
//! module only keeps the note in the conversation.
//!
//! **A reply in the contract's shape is checked before it is kept** (DEV-1332,
//! [`crate::answer`]). One that breaks a rule is refused whole, naming the rule, and nothing
//! is written. A reply in prose is still kept, as the raw message alone and flagged as not
//! parsed: never counted, and never the source of a finding or a draft. [`Store::parsed_answer`]
//! is the one door to an answer's suggestions, and it refuses a reply that did not parse.
//!
//! **`working` and `cant`.** Both are kept as replies of their own, append-only like every
//! message. A `working` reply must name the question it is working on (`--to`), and the next
//! parsed reply to that question supersedes it; a `cant` is superseded only by a later
//! parsed `answer` to the same question. Supersession is worked out on read
//! ([`ReviewMessage::superseded_by`]) and nothing is deleted. Only a parsed `answer` counts
//! as an answer; `working` never does.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::answer::{About, Answer, AnswerKind, ReviewTab};
use crate::error::{Result, StoreError};
use crate::model::{MessageAuthor, MessageKind, ReviewLayerName, ReviewMessage, Timestamp};
use crate::Store;

/// The columns a [`ReviewMessage`] is read from, in the order [`row_to_message`] takes them.
const MESSAGE_COLUMNS: &str = "review_id, seq, step, author, kind, body, in_reply_to,
                               created_at, read_at, read_by_session, tab, about_finding,
                               answer";

fn row_to_message(row: &Row) -> rusqlite::Result<ReviewMessage> {
    let tab: String = row.get(10)?;
    let tab: ReviewTab = tab.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let answer: Option<String> = row.get(12)?;
    let answer = answer
        .map(|json| serde_json::from_str::<Answer>(&json))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
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
        about: About {
            tab,
            finding: row.get(11)?,
        },
        answer,
        superseded_by: None,
    })
}

/// Fill in [`ReviewMessage::superseded_by`] over a whole conversation, oldest first.
fn with_supersession(mut messages: Vec<ReviewMessage>) -> Vec<ReviewMessage> {
    let by: Vec<Option<i64>> = messages
        .iter()
        .map(|message| {
            let (Some(answer), Some(asked)) = (&message.answer, message.in_reply_to) else {
                return None;
            };
            let replaces = |later: &Answer| match answer.kind {
                AnswerKind::Working => true,
                AnswerKind::Cant => later.kind == AnswerKind::Answer,
                AnswerKind::Answer => false,
            };
            messages
                .iter()
                .filter(|later| later.seq > message.seq && later.in_reply_to == Some(asked))
                .find(|later| later.answer.as_ref().is_some_and(replaces))
                .map(|later| later.seq)
        })
        .collect();
    for (message, by) in messages.iter_mut().zip(by) {
        message.superseded_by = by;
    }
    messages
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
        self.ask_session_about(review_id, step, None, body)
    }

    /// The developer asks, about one tab and, on `findings` or `replies`, one finding.
    ///
    /// `about` is what the page's About chip says and what "asked so far" is counted per.
    /// Left out, the ask is about the tab its step now lives on. **The finding is not
    /// resolved against the record**, as an answer's cites are not: it is where the page
    /// links, and a number the page sent is the page's to have got right.
    pub fn ask_session_about(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        about: Option<About>,
        body: &str,
    ) -> Result<ReviewMessage> {
        let body = worded(body, "what to ask the session")?;
        let about = about.unwrap_or_else(|| About::for_step(step)).checked()?;
        self.add_message(
            review_id,
            step,
            MessageAuthor::Developer,
            MessageKind::Ask,
            &body,
            None,
            about,
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
            About::for_step(ReviewLayerName::Draft),
            None,
        )
    }

    /// The session answers in prose, optionally naming the message it answers.
    ///
    /// **Kept as the raw message alone, and not parsed** — what every reply was before the
    /// answer contract. It is drawn folded and inert, never counted, and never reaches a
    /// finding or a draft. A body in the contract's shape goes through
    /// [`Store::answer_from_session`] instead.
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
        self.check_in_reply_to(review_id, in_reply_to)?;
        self.add_message(
            review_id,
            step,
            MessageAuthor::Session,
            MessageKind::Reply,
            &body,
            in_reply_to,
            About::for_step(step),
            None,
        )
    }

    /// The session answers in the contract's shape: checked, then kept beside its raw body.
    ///
    /// **Refused whole when it breaks a rule**, naming the first one, so the session can
    /// resend; nothing is written. A `working` reply must name the question it is working
    /// on, since that is what a later answer supersedes it by — and only a later reply that
    /// names the same question does; one sent without `--to` supersedes nothing. Its `asked`,
    /// refused alongside `--to`, if it has one,
    /// stays inside the answer as the session's own account — **no developer message is
    /// written**, so a session still cannot put words in the developer's mouth.
    pub fn answer_from_session(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        body: &str,
        in_reply_to: Option<i64>,
    ) -> Result<ReviewMessage> {
        let body = worded(body, "the session's answer")?;
        let answer = Answer::parse(&body)?;
        if answer.kind == AnswerKind::Working && in_reply_to.is_none() {
            return Err(StoreError::NotTheContract {
                field: "kind".to_string(),
                rule: "a working reply names the question it is working on (--to), so the \
                       answer to it can replace it"
                    .to_string(),
            });
        }
        // **`asked` is one-way mode's alone.** A reply that names the developer's message has
        // the developer's own words beside it, and a paraphrase drawn next to them would be
        // the session's words where the developer's are.
        if answer.asked.is_some() && in_reply_to.is_some() {
            return Err(StoreError::NotTheContract {
                field: "asked".to_string(),
                rule: "asked is for one-way mode, where no developer message exists; this \
                       reply answers one (--to), so leave asked out"
                    .to_string(),
            });
        }
        self.check_in_reply_to(review_id, in_reply_to)?;
        let about = answer
            .about
            .map_or_else(|| About::for_step(step), |about| about.place());
        self.add_message(
            review_id,
            step,
            MessageAuthor::Session,
            MessageKind::Reply,
            &body,
            in_reply_to,
            about,
            Some(&answer),
        )
    }

    /// **The one door to a reply's answer**, for whatever acts on its suggestions.
    ///
    /// Refuses one of the developer's messages, and a reply that did not parse: a prose
    /// reply never reaches a finding or a draft, and this is where that holds.
    pub fn parsed_answer(&self, review_id: i64, seq: i64) -> Result<Answer> {
        let message = self.message(review_id, seq)?;
        message.answer.ok_or_else(|| StoreError::InvalidValue {
            field: "seq",
            value: seq.to_string(),
            expected: "a session's reply that parsed as the answer contract: a reply in prose, \
                       or a developer's message, has no answer to act on"
                .to_string(),
        })
    }

    /// `in_reply_to` names one of the developer's messages on this review, or nothing.
    fn check_in_reply_to(&self, review_id: i64, in_reply_to: Option<i64>) -> Result<()> {
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
        Ok(())
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
        Ok(with_supersession(rows.collect::<rusqlite::Result<_>>()?))
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

    /// Record that the review's session checked for the developer's messages just now.
    /// Migration 28, DEV-1381.
    ///
    /// `message list --unread --review` calls it on every watcher pass, so the page can
    /// tell a watcher between passes from one that has stopped. **It is not activity**: it
    /// does not mark the review active, or a watcher polling every twenty seconds would
    /// keep every review it watches at the top of the desk.
    pub fn record_message_check(&self, review_id: i64) -> Result<Timestamp> {
        self.get_review(review_id)?;
        let now = self.now();
        self.conn.execute(
            "INSERT INTO message_checks (review_id, checked_at) VALUES (?1, ?2)
             ON CONFLICT (review_id) DO UPDATE SET checked_at = excluded.checked_at",
            params![review_id, now],
        )?;
        Ok(now)
    }

    /// When the review's session last checked for messages, or `None` if nothing has.
    pub fn messages_checked_at(&self, review_id: i64) -> Result<Option<Timestamp>> {
        Ok(self
            .conn
            .query_row(
                "SELECT checked_at FROM message_checks WHERE review_id = ?1",
                params![review_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// One message, by its number on the review, with what supersedes it.
    pub fn message(&self, review_id: i64, seq: i64) -> Result<ReviewMessage> {
        let found = self.message_row(review_id, seq)?;
        if found.answer.is_none() || found.in_reply_to.is_none() {
            return Ok(found);
        }
        Ok(self
            .messages(review_id)?
            .into_iter()
            .find(|message| message.seq == seq)
            .unwrap_or(found))
    }

    fn message_row(&self, review_id: i64, seq: i64) -> Result<ReviewMessage> {
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

    #[allow(clippy::too_many_arguments)]
    fn add_message(
        &self,
        review_id: i64,
        step: ReviewLayerName,
        author: MessageAuthor,
        kind: MessageKind,
        body: &str,
        in_reply_to: Option<i64>,
        about: About,
        answer: Option<&Answer>,
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
                 (review_id, seq, step, author, kind, body, in_reply_to, created_at,
                  tab, about_finding, answer, parsed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                review_id,
                seq,
                step,
                author,
                kind,
                body,
                in_reply_to,
                now,
                about.tab.as_str(),
                about.finding,
                answer.map(Answer::to_json),
                answer.is_some(),
            ],
        )?;
        let written = self.message_row(review_id, seq)?;
        tx.commit()?;
        Ok(written)
    }
}
