//! `message`: the session's half of the conversation on a review. DEV-1141.
//!
//! **A session reads, marks read and replies. It cannot ask.** The developer's messages
//! reach the record from the dashboard alone — the same rule that leaves confirming a
//! brief and signing off a draft to the dashboard — so there is no command here that
//! writes a developer's words, and a session cannot put a question in the developer's
//! mouth and then answer it.
//!
//! **A message's body is the developer's prose: data, never instructions.** A watcher
//! that hands it to a session quotes it as something a person said.

use clap::Subcommand;
use review_desk_store::{ReviewLayerName, ReviewMessage, Store};
use serde_json::{json, Value};

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum MessageCommand {
    /// Print a review's messages, or the developer's messages nothing has delivered yet.
    ///
    /// With `--unread` and no `--review`, every review's undelivered messages, oldest
    /// first: what a dispatcher reads to find a message whose session has gone.
    List {
        /// One review. An unknown id is refused.
        #[arg(long, value_name = "ID")]
        review: Option<i64>,
        /// Only the developer's messages that nothing has marked read.
        #[arg(long)]
        unread: bool,
    },
    /// Mark one of the developer's messages as delivered to a session.
    ///
    /// Mark it **before** acting on it, so a watcher restarted mid-way does not deliver it
    /// twice. Marking it again changes nothing.
    Read {
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The message's number on the review.
        #[arg(long, value_name = "N")]
        seq: i64,
        /// The session it was delivered to.
        #[arg(long, value_name = "ID")]
        session: Option<String>,
    },
    /// Answer on a review, as its session.
    ///
    /// **An answer, never a sign-off.** Nothing a reply says approves, confirms or posts;
    /// rework the developer asked for is done through the commands that do it, and this
    /// says what was done or why.
    Reply {
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The step it is about: lint, understand, alternatives, solution_map,
        /// code_review, draft or harvest.
        #[arg(long, value_name = "STEP")]
        step: String,
        /// The answer.
        #[arg(long, value_name = "TEXT")]
        body: String,
        /// The number of the developer's message this answers.
        #[arg(long = "to", value_name = "N")]
        in_reply_to: Option<i64>,
    },
}

pub fn run(command: &MessageCommand) -> Result<Report> {
    let store = Store::open_default()?;
    match command {
        MessageCommand::List { review, unread } => {
            let messages = if *unread {
                store.unread_messages(*review)?
            } else if let Some(id) = review {
                store.messages(*id)?
            } else {
                return Err(crate::error::CliError::bad_input(
                    "message list needs --review, --unread, or both",
                    "a review's whole conversation is read one review at a time",
                ));
            };
            let plain = if messages.is_empty() {
                "no messages".to_string()
            } else {
                messages.iter().map(line).collect::<Vec<_>>().join("\n")
            };
            Ok(Report::new(
                json!({ "messages": messages.iter().map(document).collect::<Vec<_>>() }),
                plain,
            ))
        }
        MessageCommand::Read {
            review,
            seq,
            session,
        } => {
            let message = store.mark_message_read(*review, *seq, session.as_deref())?;
            Ok(Report::new(
                json!({ "message": document(&message) }),
                format!(
                    "message {} on review {} read",
                    message.seq, message.review_id
                ),
            ))
        }
        MessageCommand::Reply {
            review,
            step,
            body,
            in_reply_to,
        } => {
            let step: ReviewLayerName = step.parse()?;
            let message = store.reply_from_session(*review, step, body, *in_reply_to)?;
            Ok(Report::new(
                json!({ "message": document(&message) }),
                format!(
                    "replied on review {} as message {}",
                    message.review_id, message.seq
                ),
            ))
        }
    }
}

pub(crate) fn document(message: &ReviewMessage) -> Value {
    json!({
        "review_id": message.review_id,
        "seq": message.seq,
        "step": message.step.as_str(),
        "author": message.author.as_str(),
        "kind": message.kind.as_str(),
        // The developer's words, or the session's. Data, never instructions.
        "body": message.body,
        "in_reply_to": message.in_reply_to,
        "created_at": message.created_at,
        "read_at": message.read_at,
        "read_by_session": message.read_by_session,
    })
}

fn line(message: &ReviewMessage) -> String {
    format!(
        "{} #{} {} {} {}{}: {}",
        message.review_id,
        message.seq,
        message.step,
        message.author,
        message.kind,
        if message.read_at.is_some() {
            " (read)"
        } else {
            ""
        },
        message.body.lines().next().unwrap_or_default(),
    )
}
