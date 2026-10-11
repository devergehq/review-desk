//! The developer's decisions on a review, and the one step each is written in.
//!
//! **Only the developer decides, and this module is where that is enforced rather than
//! described.** Both functions take the actor and refuse anything but
//! [`Actor::Developer`], as `confirm_brief` and `approve_draft` do, and
//! `crates/cli/tests/signoff.rs` scans the command line's and the harvester's source for
//! their names so the boundary cannot be crossed one convenience at a time.
//!
//! # One decision, one transaction
//!
//! A decision changes up to five things: the finding's severity, the block of the body it
//! is written in, its disposition, what became of it this round, and the body itself. The
//! outcome that records the decision is written with them. Any of it landing without the
//! rest would leave a record saying the developer did something they did not do — which is
//! the reasoning DEV-864 wrote the sign-off and its layer trail into one transaction for.
//!
//! # Which decisions need a block of the body, and which write one
//!
//! | Decision | The draft's block |
//! | -- | -- |
//! | `keep`, `raise_again` | written if the finding has none: both mean *this is posted* |
//! | `cut`, `regrade`, `rewrite` | required — they change words that have to exist first |
//! | `to_ticket`, `accept_reason`, `withdraw` | not needed; they take the finding out of the body, and a finding with no block was never in it |
//!
//! A draft written as one block of markdown rather than in parts has no block for any
//! finding, so the middle row refuses with [`StoreError::NoDraftSection`], which says so.
//! Every draft written before migration 9 is in that state.

use rusqlite::{params, Transaction, TransactionBehavior};

use crate::entities::{composition, only_the_developer};
use crate::error::{Result, StoreError};
use crate::model::{
    Actor, DecidedBy, Decision, DecisionMade, Disposition, Draft, Finding, FindingDecision,
    FindingRef, NitTicket, NitTicketStatus, Outcome, OutcomeKind, ReviewEvent, RoundStatus,
    Timestamp,
};
use crate::Store;

/// What the developer's decision came to.
struct Settled {
    /// Where the finding stands in this round's draft afterwards, or `None` to leave it.
    disposition: Option<Disposition>,
    /// What became of the finding this round, or `None` to leave it.
    round_status: Option<RoundStatus>,
    /// The outcome kind the decision is recorded as. Exactly one, always.
    outcome: OutcomeKind,
}

impl FindingDecision {
    /// What this decision settles, with no reference to the record.
    ///
    /// The whole mapping in one total function, so that "every decision leaves exactly one
    /// outcome" is readable rather than asserted across eight branches of a handler.
    fn settles(self) -> Settled {
        let settled = |disposition, round_status, outcome| Settled {
            disposition,
            round_status,
            outcome,
        };

        match self {
            FindingDecision::Keep => settled(Some(Disposition::InDraft), None, OutcomeKind::Kept),
            FindingDecision::Cut => settled(Some(Disposition::Suppressed), None, OutcomeKind::Cut),
            FindingDecision::Regrade => settled(None, None, OutcomeKind::Regraded),
            FindingDecision::Rewrite => settled(None, None, OutcomeKind::Rewritten),
            FindingDecision::ToTicket => settled(
                Some(Disposition::HeldBack),
                Some(RoundStatus::Ticketed),
                OutcomeKind::Ticketed,
            ),
            FindingDecision::RaiseAgain => settled(
                Some(Disposition::InDraft),
                Some(RoundStatus::StillOpen),
                OutcomeKind::RaisedAgain,
            ),
            // `rejected`, and the reason is the detail. That is exactly what
            // `RuleCounts::rejected_with_reason` counts — "author disagreed once, with a
            // reason you accepted" — so accepting the author's reason has to be the
            // outcome that count is built on, rather than a kind of its own that nothing
            // would read.
            FindingDecision::AcceptReason => settled(
                Some(Disposition::Suppressed),
                Some(RoundStatus::Accepted),
                OutcomeKind::Rejected,
            ),
            FindingDecision::Withdraw => settled(
                Some(Disposition::Suppressed),
                Some(RoundStatus::Withdrawn),
                OutcomeKind::Withdrawn,
            ),
        }
    }

    /// Whether this decision writes a block for a finding the draft has none for.
    fn writes_a_block(self) -> bool {
        matches!(self, FindingDecision::Keep | FindingDecision::RaiseAgain)
    }

    /// Whether this decision needs a block that is already there.
    fn needs_a_block(self) -> bool {
        matches!(
            self,
            FindingDecision::Cut | FindingDecision::Regrade | FindingDecision::Rewrite
        )
    }
}

impl Store {
    /// Record the developer's decision about one finding, and recompose the draft.
    ///
    /// `by` is required and must be [`Actor::Developer`]: see the module documentation.
    /// The finding, the draft and the outcome are read back inside the same transaction,
    /// so what the caller is handed is one instant of the record.
    ///
    /// Refused when the draft is approved or posted. An approved draft is what the review
    /// session reads and posts under the developer's name, and a decision that recomposed
    /// it underneath would change what gets posted.
    pub fn decide_finding(
        &self,
        review_id: i64,
        seq: i64,
        decision: &Decision,
        by: Actor,
    ) -> Result<DecisionMade> {
        only_the_developer(by, "decide on a finding")?;
        // Every refusal that can be computed from the request alone is computed before the
        // transaction opens, so a malformed decision never leaves half a change behind.
        check(decision)?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let finding = self.get_finding(review_id, seq)?;
        let draft = self.get_draft(review_id)?;

        if !draft.status.is_writable() {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "only a draft that is drafting, ready or sent back can be decided \
                         about; an approved draft is what the review session posts",
            });
        }

        let held = composition::finding_block(&tx, review_id, seq)?;
        if decision.decision.needs_a_block() && held.is_none() {
            return Err(StoreError::NoDraftSection { review_id, seq });
        }

        // **Keeping a finding this round has already settled is refused rather than
        // quietly reopened.** "Keep" means *post this as it stands*, and a finding carried
        // as fixed, accepted, withdrawn or ticketed is not posted as an open finding —
        // moving its round status to still open on a click labelled Keep would put a
        // decision in the record that the developer did not make. Raising it again is the
        // decision that says it is still open, and it is one button away.
        if decision.decision == FindingDecision::Keep && !finding.round_status.is_open() {
            return Err(StoreError::InvalidValue {
                field: "decision",
                value: format!("F{seq} has round status {}", finding.round_status),
                expected: "a finding whose round status is new or still open. Raise it \
                           again if this round is to say it is still there; what a settled \
                           finding came to is a prose block"
                    .to_string(),
            });
        }

        // The developer's own words belong to a rewrite once the block exists. Taken on a
        // keep as well, they would change what gets posted while the record said only
        // "kept" — which is the one thing this issue exists to stop.
        if held.is_some()
            && decision.decision != FindingDecision::Rewrite
            && (decision.title.is_some() || decision.text.is_some())
        {
            return Err(StoreError::InvalidValue {
                field: "text",
                value: format!("F{seq} already has a block in this draft"),
                expected: "no words on this decision; rewriting what gets posted is its \
                           own decision, so that the record says the developer rewrote it"
                    .to_string(),
            });
        }

        let settled = decision.decision.settles();
        let detail = detail_for(decision, &finding)?;
        // The words a new block would be written with, worked out before anything is
        // written — the handler's own rule. The transaction would roll a refusal back
        // either way; computing it here means a finding with nothing to say about it is
        // refused without a write having been attempted at all.
        let words = match (decision.decision.writes_a_block(), &held) {
            (true, None) => Some(words_for(decision, &finding)?),
            _ => None,
        };

        // The severity first, because the heading is composed from it: by the time the
        // body is recomposed below, the finding already holds the severity it is to be
        // posted at.
        if let Some(severity) = decision.severity {
            tx.execute(
                "UPDATE findings SET severity = ?3 WHERE review_id = ?1 AND seq = ?2",
                params![review_id, seq, severity],
            )?;
        }

        if let Some(disposition) = settled.disposition {
            tx.execute(
                "UPDATE findings SET disposition = ?3, disposition_reason = ?4
                 WHERE review_id = ?1 AND seq = ?2",
                params![review_id, seq, disposition, decision.note],
            )?;
        }

        if let Some(status) = settled.round_status {
            // **Recorded as decided by the developer**, which is what `DecidedBy` is a
            // separate set for: an agent decides most round statuses, and a decision made
            // in the dashboard is the developer's. The note is how they checked, in their
            // own words — for `accept_reason` it is the author's reason, which is the fact
            // the feedback loop is actually interested in.
            tx.execute(
                "UPDATE findings SET
                     round_status         = ?3,
                     round_status_by      = ?4,
                     round_status_checked = ?5,
                     round_status_at      = ?6
                 WHERE review_id = ?1 AND seq = ?2",
                params![
                    review_id,
                    seq,
                    status,
                    DecidedBy::Developer,
                    decision.note,
                    self.now()
                ],
            )?;
        }

        match (decision.decision, &words) {
            (FindingDecision::Rewrite, _) => {
                let rewritten = composition::rewrite_finding_block(
                    &tx,
                    review_id,
                    seq,
                    decision.title.as_deref(),
                    decision.text.as_deref(),
                )?;
                if !rewritten {
                    return Err(StoreError::NoDraftSection { review_id, seq });
                }
            }
            // `words` is `Some` exactly when this decision has a block to write and the
            // draft holds none, which is worked out above — so matching on it rather than
            // on the decision again is what keeps the two from disagreeing.
            (_, Some(words)) => {
                composition::add_finding_block(&tx, review_id, seq, words)?;
            }
            // A block the draft already holds, and a decision that does not rewrite its
            // words. Whether it posts is decided by the finding, which has just been
            // written.
            _ => {}
        }

        // The body last, so it is composed from everything the decision changed, and the
        // lint with it: after a decision the draft's lint reads as not checked, never as
        // passing.
        composition::recompose(&tx, review_id, composition::Lint::Clear, self.now())?;

        let outcome = write_outcome(
            &tx,
            FindingRef { review_id, seq },
            settled.outcome,
            detail.as_deref(),
            self.now(),
        )?;

        let made = DecisionMade {
            finding: self.get_finding(review_id, seq)?,
            draft: self.get_draft(review_id)?,
            outcome,
        };

        self.mark_review_active(review_id)?;
        tx.commit()?;
        Ok(made)
    }

    /// File what is left of the review as one ticket, and approve the draft, in one step.
    ///
    /// Behind "Ticket what is left, and approve". Every finding the draft would post is
    /// held back for the ticket and recorded as ticketed this round; the review's one nit
    /// ticket is asked for; the body is recomposed without them; and the draft is approved
    /// to be posted as `approve`. **A sign-off**, so `by` must be the developer.
    ///
    /// **Review Desk files no ticket.** It records the request, which `work list` reports
    /// as `nits_to_ticket` for the review session to act on — the same division as posting
    /// the review itself.
    ///
    /// Refused when the review has no open finding: there would be nothing to file, and
    /// the nits already held back are asked for on their own.
    pub fn ticket_rest_and_approve(
        &self,
        review_id: i64,
        note: Option<&str>,
        by: Actor,
    ) -> Result<TicketedAndApproved> {
        only_the_developer(by, "approve a draft")?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let draft = self.get_draft(review_id)?;
        if draft.status != crate::model::DraftStatus::Ready {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "only a draft that is ready can be approved",
            });
        }

        let open: Vec<Finding> = self
            .list_findings(review_id)?
            .into_iter()
            .filter(Finding::is_posted)
            .collect();

        if open.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "review_id",
                value: review_id.to_string(),
                expected: "a review with an open finding to file; sign the draft off \
                           instead, and ask for a ticket for the nits held back on its own"
                    .to_string(),
            });
        }

        let at = self.now();
        for finding in &open {
            tx.execute(
                "UPDATE findings SET
                     disposition          = ?3,
                     disposition_reason   = ?4,
                     round_status         = ?5,
                     round_status_by      = ?6,
                     round_status_checked = ?4,
                     round_status_at      = ?7
                 WHERE review_id = ?1 AND seq = ?2",
                params![
                    review_id,
                    finding.seq,
                    Disposition::HeldBack,
                    note,
                    RoundStatus::Ticketed,
                    DecidedBy::Developer,
                    at,
                ],
            )?;

            write_outcome(
                &tx,
                FindingRef {
                    review_id,
                    seq: finding.seq,
                },
                OutcomeKind::Ticketed,
                note,
                at,
            )?;
        }

        // The same statement `request_nit_ticket` runs, so the two cannot come to mean
        // different things, and the findings the ticket covers stay the ones held back
        // rather than a second list stored against it.
        super::review::write_nit_ticket(&tx, review_id, NitTicketStatus::Requested, None, at)?;

        composition::recompose(&tx, review_id, composition::Lint::Clear, self.now())?;

        // And the same statements `approve_draft` runs, layer 6 included. The event is not
        // the caller's to choose: this act is an approval, and a request for changes that
        // tickets everything it asked for would contradict itself.
        super::draft::write_approval(&tx, review_id, ReviewEvent::Approve, by, at)?;
        self.set_review_layer(
            review_id,
            crate::model::ReviewLayerName::Draft.number(),
            crate::model::ReviewLayerState::Done,
            Some(super::draft::APPROVED_DETAIL),
        )?;

        let settled = TicketedAndApproved {
            draft: self.get_draft(review_id)?,
            ticket: self.nit_ticket(review_id)?,
            ticketed: open.iter().map(|finding| finding.seq).collect(),
        };

        self.mark_review_active(review_id)?;
        tx.commit()?;
        Ok(settled)
    }
}

/// What "ticket what is left, and approve" came to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TicketedAndApproved {
    pub draft: Draft,
    pub ticket: NitTicket,
    /// The findings this act moved into the ticket, in number order. The ones that were
    /// already held back are in `ticket.finding_seqs` and not here: this is what changed.
    pub ticketed: Vec<i64>,
}

/// Refuse a decision whose parts do not go together, before anything is written.
fn check(decision: &Decision) -> Result<()> {
    let refuse = |field: &'static str, value: String, expected: &str| StoreError::InvalidValue {
        field,
        value,
        expected: expected.to_string(),
    };

    match decision.decision {
        FindingDecision::Regrade if decision.severity.is_none() => {
            return Err(refuse(
                "severity",
                "none given".to_string(),
                "the severity to post the finding at; regrading is the decision that \
                 changes one",
            ));
        }
        FindingDecision::Rewrite if decision.title.is_none() && decision.text.is_none() => {
            return Err(refuse(
                "text",
                "neither a title nor a paragraph".to_string(),
                "the heading's title, the paragraph, or both — in the developer's own \
                 words, which is what gets posted",
            ));
        }
        // And the words themselves, against the two rules the composition rests on. The
        // rewrite is the one path that writes a block's words through a statement of its
        // own rather than through `composition::write`, so it is the one path that has to
        // ask for itself. See `composition::validate_rewrite`.
        FindingDecision::Rewrite => {
            composition::validate_rewrite(decision.title.as_deref(), decision.text.as_deref())?;
        }
        FindingDecision::AcceptReason if blank(decision.note.as_deref()) => {
            return Err(refuse(
                "note",
                "no reason".to_string(),
                "the author's reason, in their words. A rejection with no reason is the \
                 one outcome the feedback loop cannot learn from, and it is what \
                 'rejected with a reason you accepted' counts",
            ));
        }
        _ => {}
    }

    // Words belong to a rewrite, and to the two decisions that may have to write a block
    // the draft has none of. **Refused rather than ignored** on the other five: an API that
    // accepts a field and does nothing with it is one a caller cannot tell they misused.
    if (decision.title.is_some() || decision.text.is_some())
        && !matches!(
            decision.decision,
            FindingDecision::Rewrite | FindingDecision::Keep | FindingDecision::RaiseAgain
        )
    {
        return Err(refuse(
            "text",
            format!("words on a {} decision", decision.decision),
            "no title and no text on this decision; rewriting what gets posted is its own \
             decision, and this one takes the finding out of the post",
        ));
    }

    if decision.severity.is_some() && decision.decision != FindingDecision::Regrade {
        return Err(refuse(
            "severity",
            decision
                .severity
                .map(|severity| severity.to_string())
                .unwrap_or_default(),
            "no severity on this decision; changing one is a regrade, so that the record \
             says the developer regraded it",
        ));
    }

    Ok(())
}

/// The words a decision is to write a new block with, and the agent's own beside them.
///
/// The agent's words are the finding's, in a stated order and with no invention: its claim
/// is the heading's title, and the paragraph is the consequence it recorded or, failing
/// that, the comment written for the author. A finding with neither is a refusal naming
/// what is missing rather than a block with an empty paragraph.
///
/// **The developer's words, where they gave any, are what the block posts — and the
/// agent's stay beside them.** That is the point of returning both: the developer may post
/// a held-back nit in their own words, and a record that stored those words as the agent's
/// would read as a block nobody had rewritten.
fn words_for(decision: &Decision, finding: &Finding) -> Result<composition::BlockWords> {
    // **The finding's own title, and its claim only when it has none.** The title is the one
    // line the code review page's row carries; the claim is what the review says is wrong in
    // the review's voice, and on the review board A8 was drawn from one ran to 56 words — a
    // heading nobody would post. Every finding written before migration 11 has no title,
    // which is what the fallback is for.
    let agent_title = finding
        .title
        .clone()
        .unwrap_or_else(|| finding.claim.clone());
    let agent_text = finding
        .why_it_matters
        .clone()
        .or_else(|| finding.inline_comment.clone())
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| StoreError::InvalidValue {
            field: "text",
            value: format!("F{} has no paragraph to post", finding.seq),
            expected: "what the review is to say about it; the finding records no \
                       consequence and no comment for the author, and nothing may invent \
                       one"
            .to_string(),
        })?;

    let title = match decision.title.as_deref().map(str::trim) {
        Some(title) if !title.is_empty() => title.to_string(),
        _ => agent_title.clone(),
    };
    let text = match decision.text.as_deref() {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => agent_text.clone(),
    };

    Ok(composition::BlockWords {
        title,
        text,
        agent_title,
        agent_text,
    })
}

/// What the outcome says beyond its kind.
///
/// The developer's note where they gave one, and for the two decisions that change words,
/// what changed. A regrade names both severities, which is what the Draft preview's "your
/// edits" list reads as "Blocker to Should".
fn detail_for(decision: &Decision, finding: &Finding) -> Result<Option<String>> {
    let note = decision
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());

    let detail = match decision.decision {
        FindingDecision::Regrade => {
            let to = decision.severity.ok_or_else(|| StoreError::InvalidValue {
                field: "severity",
                value: "none given".to_string(),
                expected: "the severity to post the finding at".to_string(),
            })?;
            let change = format!("{} to {}", finding.severity.label(), to.label());
            Some(match note {
                Some(note) => format!("{change}: {note}"),
                None => change,
            })
        }
        FindingDecision::Rewrite => {
            // Which part the developer replaced. **What it replaced is not repeated
            // here**: the block keeps the agent's own title and paragraph, which is one
            // copy of it rather than two that could disagree.
            let mut parts = Vec::new();
            if decision.title.is_some() {
                parts.push("the heading");
            }
            if decision.text.is_some() {
                parts.push("the paragraph");
            }
            let change = format!("rewrote {}", parts.join(" and "));
            Some(match note {
                Some(note) => format!("{change}: {note}"),
                None => change,
            })
        }
        // A `keep` or a `raise_again` may carry the developer's own words, for a finding
        // the draft holds no block for — the designs' "Post it" on a held-back nit. The
        // outcome is still the one its decision names, and its detail says whose words
        // were posted, because otherwise a decision that changed what gets posted would
        // leave a record that did not say so.
        _ if decision.title.is_some() || decision.text.is_some() => {
            let change = "posted in the developer's own words".to_string();
            Some(match note {
                Some(note) => format!("{change}: {note}"),
                None => change,
            })
        }
        _ => note.map(str::to_string),
    };

    Ok(detail)
}

/// Record the outcome inside the decision's own transaction.
///
/// Written through the transaction rather than through `Store::add_outcome`, which would
/// open a second one: the decision and the outcome that says who made it are one step.
///
/// **The time is passed in rather than read here**, because the store's clock lives on the
/// store and this is a free function — and because the decision and its outcome should
/// carry the same instant rather than two readings a tick apart.
fn write_outcome(
    tx: &Transaction<'_>,
    finding: FindingRef,
    kind: OutcomeKind,
    detail: Option<&str>,
    at: Timestamp,
) -> Result<Outcome> {
    tx.execute(
        "INSERT INTO outcomes (review_id, finding_seq, kind, actor, occurred_at, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            finding.review_id,
            finding.seq,
            kind,
            Actor::Developer,
            at,
            detail
        ],
    )?;

    Ok(Outcome {
        id: tx.last_insert_rowid(),
        review_id: finding.review_id,
        finding_seq: finding.seq,
        kind,
        actor: Actor::Developer,
        occurred_at: at,
        detail: detail.map(str::to_string),
        harvest_key: None,
    })
}

/// Whether a note was given at all.
fn blank(note: Option<&str>) -> bool {
    note.map(str::trim).unwrap_or_default().is_empty()
}

/// The outcome kinds a developer's decision can leave behind.
///
/// Not used by the decision path itself — [`FindingDecision::settles`] is the mapping —
/// but by the Draft preview's "your edits" list, which is every outcome of these kinds
/// with actor `developer`. It is here so that adding a decision and forgetting the screen
/// is a compile error rather than a list quietly missing a row.
pub const DEVELOPER_DECISIONS: [OutcomeKind; 8] = [
    OutcomeKind::Kept,
    OutcomeKind::Cut,
    OutcomeKind::Regraded,
    OutcomeKind::Rewritten,
    OutcomeKind::Ticketed,
    OutcomeKind::RaisedAgain,
    OutcomeKind::Rejected,
    OutcomeKind::Withdrawn,
];
