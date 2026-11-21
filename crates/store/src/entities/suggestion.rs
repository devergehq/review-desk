//! The developer's answers to what a review's session suggested — DEV-1334, under DEV-1331.
//!
//! **The session proposes; the developer decides.** An answer's suggestions change nothing
//! until the developer accepts one, and accepting writes exactly what the page's own control
//! writes: a decision on a finding through [`Store::decide_finding_prompted`], which is
//! [`Store::decide_finding`] with the prompt recorded beside it, or a finding through
//! [`Store::add_finding_from_conversation`]. This module adds that one developer-side call
//! for adding a finding, the dismissal, and the check that ties an act to the suggestion
//! that prompted it.
//!
//! **Whose act it is.** Patrick, 21 November 2026: "An accepted suggestion stays the
//! developer's decision. A new `prompted_by` field holds the sequence number of the message
//! that suggested it. No session's name goes on a developer's decision." So every call here
//! refuses any actor but the developer, an acceptance's outcome is the developer's like any
//! other, and nothing records [`crate::DecidedBy::Agent`].
//!
//! | Act | What it writes |
//! | -- | -- |
//! | Accept a `new_finding`, or *Turn into a finding* | a finding with `prompted_by`, and a row here |
//! | Accept a `regrade`, `add_to_draft`, `set_aside` or `draft_reply` | the decision and its outcome, and a row here naming the outcome |
//! | Accept a `follow_up` | nothing: it fills the ask box, and only the developer sends |
//! | Dismiss any of them | a row here, with the developer's reason when they give one |

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::answer::{Accepting, AnswerKind, Basis, Suggestion, LINE_MAX, TITLE_MAX};
use crate::entities::only_the_developer;
use crate::error::{Result, StoreError};
use crate::model::{
    Actor, DeveloperFinding, Disposition, Finding, FindingDecision, NewFinding, PromptedBy,
    Severity, SuggestionAnswer, SuggestionVerdict, Timestamp, Verification,
};
use crate::Store;

const COLUMNS: &str = "review_id, message_seq, suggestion, verdict, reason, finding_seq, \
                       outcome_id, answered_at";

/// The lens a finding the developer adds from the conversation is filed under.
pub const CONVERSATION_LENS: &str = "conversation";

fn row_to_answer(row: &Row<'_>) -> rusqlite::Result<SuggestionAnswer> {
    Ok(SuggestionAnswer {
        review_id: row.get(0)?,
        message: row.get(1)?,
        suggestion: row.get(2)?,
        verdict: row.get(3)?,
        reason: row.get(4)?,
        finding_seq: row.get(5)?,
        outcome_id: row.get(6)?,
        answered_at: row.get(7)?,
    })
}

/// How a suggestion is named in a refusal: its place, or the whole answer.
fn which(suggestion: Option<i64>) -> String {
    suggestion.map_or_else(|| "behind the whole answer".to_string(), |n| n.to_string())
}

impl Store {
    /// **Add a finding the developer chose to add from the conversation** — the one
    /// developer-side call that adds a finding (Patrick, 21 November 2026, Q1).
    ///
    /// Behind Accept on a `new_finding` suggestion (`prompted.suggestion` names it) and
    /// *Turn into a finding* on an answer (`prompted.suggestion` is `None`). The finding is
    /// numbered and stored by the same insert as a session's ([`Store::add_finding`]),
    /// starts in *To post* like any other, and carries `prompted_by`. It is given no block
    /// of the draft: the S6 card's own words, "it goes into To post; you decide it like any
    /// other", so Post on Findings is what puts it in the body.
    ///
    /// Refused when the draft is approved or posted, as every decision is: an approved draft
    /// is what the session posts under the developer's name. Refused when the prompt has
    /// already been answered, so a double click adds one finding.
    pub fn add_finding_from_conversation(
        &self,
        review_id: i64,
        finding: &DeveloperFinding,
        prompted: PromptedBy,
        by: Actor,
    ) -> Result<Finding> {
        only_the_developer(by, "add a finding from the conversation")?;
        let new = new_finding(finding)?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.check_prompt(review_id, prompted, |suggestion| {
            matches!(
                suggestion.suggestion_type().accepting(),
                Accepting::AddsAFinding
            )
        })?;
        // **One answer adds one finding**, whichever door was used first: Accept on its
        // `new_finding` and *Turn into a finding* would otherwise each add one (self-review
        // round 2, S1). The suggestion's own place is already refused as answered above.
        if let Some(added) = self
            .suggestion_answers(review_id)?
            .into_iter()
            .find(|one| one.message == prompted.message && one.finding_seq.is_some())
        {
            return Err(StoreError::SuggestionAnswered {
                review_id,
                message: prompted.message,
                which: which(prompted.suggestion),
                answered: if added.suggestion.is_none() {
                    "covered: the whole answer was turned into a finding"
                } else {
                    "covered: a suggestion on this answer already added a finding"
                },
            });
        }
        // *Turn into a finding* is offered on an answer, not on word that the session is
        // still working or on why it can't: neither says anything to raise.
        if prompted.suggestion.is_none()
            && self.parsed_answer(review_id, prompted.message)?.kind != AnswerKind::Answer
        {
            return Err(StoreError::InvalidValue {
                field: "prompted_by",
                value: format!("message {}", prompted.message),
                expected: "a reply whose kind is answer; a working or cant reply has nothing \
                           to turn into a finding"
                    .to_string(),
            });
        }
        if let Some(draft) = self.find_draft(review_id)? {
            if !draft.status.is_writable() {
                return Err(StoreError::DraftStatus {
                    review_id,
                    found: draft.status,
                    wanted: "only a draft that is drafting, ready or sent back can take a new \
                             finding; an approved draft is what the review session posts",
                });
            }
        }

        let seq = self.insert_finding(&tx, review_id, &new, Some(prompted.message))?;
        write_accepted(&tx, review_id, prompted, Some(seq), None, self.now())?;
        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.get_finding(review_id, seq)
    }

    /// Dismiss one suggestion, with the developer's reason when they give one.
    ///
    /// **One click, and the reason after it.** Dismissing a suggestion already dismissed
    /// keeps the dismissal and replaces the reason when one is given, so the card can take a reason after
    /// the click rather than asking for one before it. An accepted suggestion is not
    /// dismissed: the decision it prompted is in the record.
    pub fn dismiss_suggestion(
        &self,
        review_id: i64,
        message: i64,
        suggestion: i64,
        reason: Option<&str>,
        by: Actor,
    ) -> Result<SuggestionAnswer> {
        only_the_developer(by, "dismiss a suggestion")?;
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        if let Some(reason) = reason {
            if reason.chars().count() > LINE_MAX {
                return Err(StoreError::InvalidValue {
                    field: "reason",
                    value: format!("{} characters", reason.chars().count()),
                    expected: format!("at most {LINE_MAX} characters"),
                });
            }
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let prompted = PromptedBy {
            message,
            suggestion: Some(suggestion),
        };
        self.suggestion_at(review_id, prompted)?;
        match self.suggestion_answer(review_id, prompted)? {
            Some(held) if held.verdict == SuggestionVerdict::Accepted => {
                return Err(StoreError::SuggestionAnswered {
                    review_id,
                    message,
                    which: which(Some(suggestion)),
                    answered: "accepted",
                });
            }
            Some(_) => {
                // A reason said again replaces the last; no reason keeps it (self-review F5).
                tx.execute(
                    "UPDATE suggestion_answers SET reason = COALESCE(?4, reason)
                      WHERE review_id = ?1 AND message_seq = ?2 AND suggestion = ?3",
                    params![review_id, message, suggestion, reason],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO suggestion_answers
                         (review_id, message_seq, suggestion, verdict, reason, answered_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        review_id,
                        message,
                        suggestion,
                        SuggestionVerdict::Dismissed,
                        reason,
                        self.now()
                    ],
                )?;
            }
        }
        let answered = self
            .suggestion_answer(review_id, prompted)?
            .expect("the row was written in this transaction");
        tx.commit()?;
        Ok(answered)
    }

    /// Every answer the developer has given to a suggestion on one review, oldest first.
    pub fn suggestion_answers(&self, review_id: i64) -> Result<Vec<SuggestionAnswer>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM suggestion_answers WHERE review_id = ?1
              ORDER BY message_seq, COALESCE(suggestion, 0)"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_answer)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The answer to one prompt, if it has one.
    fn suggestion_answer(
        &self,
        review_id: i64,
        prompted: PromptedBy,
    ) -> Result<Option<SuggestionAnswer>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM suggestion_answers
                      WHERE review_id = ?1 AND message_seq = ?2
                        AND COALESCE(suggestion, 0) = COALESCE(?3, 0)"
                ),
                params![review_id, prompted.message, prompted.suggestion],
                row_to_answer,
            )
            .optional()?)
    }

    /// The suggestion a prompt names, read through the one door to a reply's answer.
    ///
    /// `None` for a prompt that names the whole answer. Refused when the message is not a
    /// parsed reply ([`Store::parsed_answer`]) or has no suggestion at that place.
    fn suggestion_at(&self, review_id: i64, prompted: PromptedBy) -> Result<Option<Suggestion>> {
        let answer = self.parsed_answer(review_id, prompted.message)?;
        let Some(place) = prompted.suggestion else {
            return Ok(None);
        };
        usize::try_from(place)
            .ok()
            .and_then(|place| place.checked_sub(1))
            .and_then(|index| answer.suggests.get(index))
            .cloned()
            .map(Some)
            .ok_or_else(|| StoreError::InvalidValue {
                field: "suggestion",
                value: place.to_string(),
                expected: format!(
                    "the place of one of message {}'s {} suggestions, from 1",
                    prompted.message,
                    answer.suggests.len()
                ),
            })
    }

    /// **The check every acceptance passes before it writes anything.**
    ///
    /// The message is a parsed reply, the suggestion is one it makes and is of a kind this
    /// act can accept, and nothing has answered it yet. A prompt naming the whole answer
    /// (*Turn into a finding*) passes the kind check: it is not a suggestion.
    pub(crate) fn check_prompt(
        &self,
        review_id: i64,
        prompted: PromptedBy,
        fits: impl Fn(&Suggestion) -> bool,
    ) -> Result<Option<Suggestion>> {
        let suggestion = self.suggestion_at(review_id, prompted)?;
        if let Some(suggestion) = &suggestion {
            if !fits(suggestion) {
                return Err(StoreError::InvalidValue {
                    field: "prompted_by",
                    value: format!(
                        "suggestion {} of message {} is a {}",
                        which(prompted.suggestion),
                        prompted.message,
                        suggestion.suggestion_type()
                    ),
                    expected: "the suggestion this act accepts: see `SuggestionType::accepting`"
                        .to_string(),
                });
            }
        }
        if let Some(held) = self.suggestion_answer(review_id, prompted)? {
            return Err(StoreError::SuggestionAnswered {
                review_id,
                message: prompted.message,
                which: which(prompted.suggestion),
                answered: held.verdict.as_str(),
            });
        }
        Ok(suggestion)
    }

    /// The check a decision prompted by a suggestion passes: the suggestion is one whose
    /// acceptance is this decision, on this finding. Run inside the decision's transaction.
    pub(crate) fn check_decision_prompt(
        &self,
        review_id: i64,
        seq: i64,
        decision: FindingDecision,
        severity: Option<Severity>,
        prompted: PromptedBy,
    ) -> Result<()> {
        if prompted.suggestion.is_none() {
            return Err(StoreError::InvalidValue {
                field: "prompted_by",
                value: format!("message {} as a whole", prompted.message),
                expected: "a decision is prompted by one suggestion; name its place".to_string(),
            });
        }
        self.check_prompt(review_id, prompted, |suggestion| {
            let decides = match suggestion.suggestion_type().accepting() {
                Accepting::Decides(decisions) => decisions.contains(&decision),
                _ => false,
            };
            // A regrade is accepted at the severity it suggested and no other, so the line
            // the card draws from the suggestion is what the record holds (self-review F4).
            // A `draft_reply` deliberately takes any of its three calls.
            let at_its_severity = match suggestion {
                Suggestion::Regrade { to, .. } => severity == Some(*to),
                _ => true,
            };
            decides && at_its_severity && suggestion.finding() == Some(seq)
        })?;
        Ok(())
    }
}

/// Record an acceptance beside what it wrote, in the caller's transaction.
pub(crate) fn write_accepted(
    tx: &Transaction<'_>,
    review_id: i64,
    prompted: PromptedBy,
    finding_seq: Option<i64>,
    outcome_id: Option<i64>,
    at: Timestamp,
) -> Result<()> {
    tx.execute(
        "INSERT INTO suggestion_answers
             (review_id, message_seq, suggestion, verdict, finding_seq, outcome_id, answered_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            review_id,
            prompted.message,
            prompted.suggestion,
            SuggestionVerdict::Accepted,
            finding_seq,
            outcome_id,
            at
        ],
    )?;
    Ok(())
}

/// The finding a [`DeveloperFinding`] is stored as.
///
/// `where` is split into a file and a line when it reads as `path:41`, and kept whole as
/// the file otherwise, which is what the findings list draws under the title. The title is
/// the claim as well, and the one sentence the developer has for why it matters, so Post
/// can write its block without inventing a paragraph. `basis` is how it was checked.
fn new_finding(finding: &DeveloperFinding) -> Result<NewFinding> {
    let title = finding.title.trim();
    let at = finding.at.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(StoreError::InvalidValue {
            field: "title",
            value: format!("{} characters", title.chars().count()),
            expected: format!("a title of 1 to {TITLE_MAX} characters"),
        });
    }
    if at.is_empty() || at.chars().count() > LINE_MAX {
        return Err(StoreError::InvalidValue {
            field: "where",
            value: format!("{} characters", at.chars().count()),
            expected: format!("where it is, in 1 to {LINE_MAX} characters"),
        });
    }
    let (file, line) = match at.rsplit_once(':') {
        Some((path, digits))
            if !path.is_empty()
                && !path.contains(char::is_whitespace)
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (
                path.to_string(),
                digits.parse::<i64>().ok().filter(|n| *n >= 1),
            )
        }
        _ => (at.to_string(), None),
    };
    Ok(NewFinding {
        lens: CONVERSATION_LENS.to_string(),
        severity: finding.severity,
        file: Some(file),
        line,
        claim: title.to_string(),
        title: Some(title.to_string()),
        why_it_matters: Some(title.to_string()),
        disposition: Disposition::InDraft,
        verification: match finding.basis {
            Basis::Ran => Verification::Reproduced,
            Basis::Read => Verification::ReadNotRun,
            Basis::Told => Verification::Unknown,
        },
        ..NewFinding::default()
    })
}
