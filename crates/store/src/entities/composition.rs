//! What the posted body is composed of: one block at a time.
//!
//! **The draft body is assembled, not edited.** Before DEV-877 the developer dropped a
//! finding from a review by deleting its paragraph out of one block of markdown, which is
//! slow and leaves Review Desk with no record of what they did — and what they did is the
//! strongest signal the feedback loop has. So a draft is now an ordered list of blocks:
//! the agent's summary sentence, each finding's heading and paragraph, the not-checked
//! line, and whatever else the house style puts around them.
//!
//! | Block | Composes to |
//! | -- | -- |
//! | `prose` | its text, exactly as written |
//! | `finding` | [`finding_heading`], a blank line, then its text — **while the finding is posted** |
//!
//! Blocks are joined by a blank line and the body ends in one newline. That is the whole
//! of the composition, and it is why it can reproduce what an agent writes today: only the
//! finding heading is composed, and everything the composition does not model is a prose
//! block carried through character for character.
//! `crates/store/tests/composition.rs` holds that claim against
//! `contract/examples/getDraft.response.json`, which is the one record of what an agent
//! wrote that this repository has.
//!
//! **The body is stored, not derived.** Every write and every decision recomposes it into
//! `drafts.body` in the same transaction. Composing it at read time would let a later
//! change to the record alter an approved draft underneath the session that is about to
//! post it — and `drafts.body` is the one thing in this database that is posted verbatim.

use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    now, Disposition, DraftSection, DraftSectionKind, Finding, NewDraftSection, RoundStatus,
    Severity,
};
use crate::{is_foreign_key_violation, Store};

/// One finding's heading, as the posted body carries it.
///
/// `### Blocker · F1 · A retried delivery is sent without an idempotency key`
///
/// **The one place this format lives.** It is composed from the finding's live severity
/// rather than stored, which is what makes regrading a finding change its heading instead
/// of leaving a second copy of the severity to disagree with it. The format is the one the
/// approved designs and `contract/examples/getDraft.response.json` already use, so a body
/// an agent wrote composes back to itself character for character.
pub fn finding_heading(severity: Severity, seq: i64, title: &str) -> String {
    format!("### {} · F{seq} · {title}", severity.label())
}

/// Whether a recomposition leaves the lint result behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lint {
    /// Clear it, and move `updated_at`. Every change to the body does this.
    Clear,
    /// Leave it, for a write that has just stored the lint result it was given.
    Keep,
}

impl Store {
    /// The blocks the review's draft body is composed of, in order.
    ///
    /// Every block, including the ones that do not post — a cut finding's block stays on
    /// the record, which is the difference between a decision and a deletion.
    pub fn draft_sections(&self, review_id: i64) -> Result<Vec<DraftSection>> {
        list(&self.conn, review_id)
    }

    /// Replace the blocks the review's draft is composed of, and recompose the body.
    ///
    /// For a session rewriting what the draft is made of without touching its status. The
    /// lint goes with the body it was run over: this is a new body, so it reads as not
    /// checked.
    pub fn put_draft_sections(
        &self,
        review_id: i64,
        sections: &[NewDraftSection],
    ) -> Result<Vec<DraftSection>> {
        validate(sections)?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let draft = self.get_draft(review_id)?;
        if !draft.status.is_writable() {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "only a draft that is drafting, ready or sent back can be rewritten",
            });
        }

        write(&tx, review_id, sections)?;
        recompose(&tx, review_id, Lint::Clear)?;
        tx.commit()?;
        self.draft_sections(review_id)
    }
}

/// Check a list of blocks before anything is written.
///
/// Everything that can be checked without the database is checked here, so a draft is
/// never left half rewritten: the shape of each kind, one block per finding, and the one
/// rule the composition rests on — a block's text neither begins nor ends with a newline,
/// because blocks are joined by exactly one blank line and a block that brought its own
/// would compose a body nobody wrote.
pub(crate) fn validate(sections: &[NewDraftSection]) -> Result<()> {
    let mut named: Vec<i64> = Vec::new();

    for (index, section) in sections.iter().enumerate() {
        let at = index + 1;
        let refuse = |value: String, expected: &str| StoreError::InvalidValue {
            field: "sections",
            value,
            expected: expected.to_string(),
        };

        match section.kind {
            DraftSectionKind::Prose => {
                if section.finding_seq.is_some() || section.title.is_some() {
                    return Err(refuse(
                        format!("block {at} is prose and names a finding"),
                        "a prose block carries text and nothing else; a block about a \
                         finding is kind finding",
                    ));
                }
                if section.text.trim().is_empty() {
                    return Err(refuse(
                        format!("block {at} is prose with no text"),
                        "the markdown this block posts; an empty block would compose a \
                         blank nobody can see to remove",
                    ));
                }
            }
            DraftSectionKind::Finding => {
                let Some(seq) = section.finding_seq else {
                    return Err(refuse(
                        format!("block {at} is about a finding and names none"),
                        "the finding's number, whose severity composes the heading",
                    ));
                };
                if section
                    .title
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err(refuse(
                        format!("block {at} is about F{seq} and has no title"),
                        "the heading's title — the part after the severity and the number",
                    ));
                }
                if named.contains(&seq) {
                    return Err(refuse(
                        format!("two blocks are about F{seq}"),
                        "one block per finding; the heading is composed from the finding, \
                         so a second block would repeat it",
                    ));
                }
                named.push(seq);
            }
        }

        if section.text.starts_with('\n') || section.text.ends_with('\n') {
            return Err(refuse(
                format!("block {at} begins or ends with a newline"),
                "the block's own text; blocks are joined by one blank line, and a block \
                 that brings its own would compose a body nobody wrote",
            ));
        }
    }

    Ok(())
}

/// What the review's blocks compose to.
///
/// Joined by a blank line, ending in one newline. A composition with nothing to post is
/// the empty string rather than a lone newline: a draft at `drafting` may hold no body at
/// all, and a draft every finding of which was cut is a body of prose or of nothing.
pub(crate) fn compose(conn: &Connection, review_id: i64) -> Result<String> {
    let posted: Vec<String> = list(conn, review_id)?.iter().filter_map(composed).collect();

    if posted.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("{}\n", posted.join("\n\n")))
}

/// One block's contribution to the body, or `None` when it does not post.
fn composed(section: &DraftSection) -> Option<String> {
    if !section.posted {
        return None;
    }

    match (section.kind, section.finding_seq, &section.title) {
        (DraftSectionKind::Prose, ..) => Some(section.text.clone()),
        (DraftSectionKind::Finding, Some(seq), Some(title)) => {
            let heading = finding_heading(section.severity?, seq, title);
            if section.text.is_empty() {
                Some(heading)
            } else {
                Some(format!("{heading}\n\n{}", section.text))
            }
        }
        // The schema refuses a finding block with no finding and the foreign key refuses
        // one naming a finding that is not there, so this arm is unreachable from a
        // database this build wrote. It drops the block rather than panicking: a body is
        // posted to GitHub under the developer's name, and half a heading is worse than a
        // block absent.
        (DraftSectionKind::Finding, ..) => None,
    }
}

/// Write the body the review's blocks compose to, and return it.
///
/// **The lint is cleared on every change to the body, and that is the rule rather than a
/// convenience.** The review-craft lint was run by the agent over the body it wrote; the
/// moment a decision changes that body the result is about a different document, and a
/// lint that still said "passes" would be the record vouching for words nobody checked.
pub(crate) fn recompose(tx: &Transaction<'_>, review_id: i64, lint: Lint) -> Result<String> {
    let body = compose(tx, review_id)?;

    match lint {
        Lint::Clear => tx.execute(
            "UPDATE drafts SET
                 body                     = ?2,
                 lint_passes              = NULL,
                 lint_visible_words       = NULL,
                 lint_word_budget         = NULL,
                 lint_findings_in_body    = NULL,
                 lint_findings_expected   = NULL,
                 lint_raised_before_on_pr = NULL,
                 lint_errors              = NULL,
                 lint_checked_at          = NULL,
                 updated_at               = ?3
             WHERE review_id = ?1",
            params![review_id, body, now()],
        )?,
        Lint::Keep => tx.execute(
            "UPDATE drafts SET body = ?2 WHERE review_id = ?1",
            params![review_id, body],
        )?,
    };

    Ok(body)
}

/// Replace every block of a draft with these, numbered from 1.
///
/// Checked, then written. The findings are checked inside the caller's transaction, so a
/// block about a finding that is not posted is a refusal naming it rather than a draft
/// that silently drops a paragraph the agent wrote.
pub(crate) fn write(
    tx: &Transaction<'_>,
    review_id: i64,
    sections: &[NewDraftSection],
) -> Result<()> {
    validate(sections)?;

    for section in sections {
        let Some(seq) = section.finding_seq else {
            continue;
        };
        let finding = finding_in(tx, review_id, seq)?;
        if !finding.is_posted() {
            return Err(StoreError::InvalidValue {
                field: "sections",
                value: format!(
                    "F{seq} is {} with round status {}, so this draft cannot raise it",
                    finding.disposition, finding.round_status
                ),
                expected: "a finding at disposition in_draft whose round status is new or \
                           still open. A fixed, accepted, withdrawn or ticketed finding is \
                           not posted as an open finding; what the round found about it is \
                           a prose block"
                    .to_string(),
            });
        }
    }

    replace(
        tx,
        review_id,
        &written_by_the_writer(tx, review_id, sections)?,
    )
}

/// Add one block for a finding the composition has none for, after the last finding block.
///
/// **Where it goes is the one thing this decides**, and it goes with the other findings
/// rather than at the end: the not-checked line is the last block of every draft the
/// designs draw, and a nit the developer chose to post belongs above it rather than after
/// it. With no finding block to follow it goes last — a draft of prose alone has nowhere
/// else to put it.
pub(crate) fn add_finding_block(
    tx: &Transaction<'_>,
    review_id: i64,
    seq: i64,
    title: &str,
    text: &str,
) -> Result<()> {
    let mut held = rows(tx, review_id)?;
    let after = held
        .iter()
        .rposition(|row| row.section.kind == DraftSectionKind::Finding)
        .map_or(held.len(), |index| index + 1);

    held.insert(
        after,
        Written {
            section: NewDraftSection::finding(seq, title, text),
            agent_title: Some(title.to_string()),
            agent_text: text.to_string(),
        },
    );

    // The whole list, not only the block being added: the rules are about the list — one
    // block per finding, and no block bringing its own blank line — and checking the new
    // one alone would leave a second block about this finding to the unique index, which
    // reports a broken database rather than a refusal naming the finding.
    let sections: Vec<NewDraftSection> = held.iter().map(|block| block.section.clone()).collect();
    validate(&sections)?;

    replace(tx, review_id, &held)
}

/// Replace one finding block's title and paragraph, keeping what the agent wrote.
///
/// `Ok(false)` when the draft holds no block for that finding, which the caller turns into
/// the refusal that says so. `agent_title` and `agent_text` are deliberately untouched:
/// they are what the rewrite replaced, and the second rewrite of a block replaced the
/// agent's words just as the first one did.
pub(crate) fn rewrite_finding_block(
    tx: &Transaction<'_>,
    review_id: i64,
    seq: i64,
    title: Option<&str>,
    text: Option<&str>,
) -> Result<bool> {
    let changed = tx.execute(
        "UPDATE draft_sections SET
             title = COALESCE(?3, title),
             text  = COALESCE(?4, text)
         WHERE review_id = ?1 AND finding_seq = ?2 AND kind = 'finding'",
        params![review_id, seq, title, text],
    )?;

    Ok(changed > 0)
}

/// The block about this finding, as it stands.
pub(crate) fn finding_block(
    conn: &Connection,
    review_id: i64,
    seq: i64,
) -> Result<Option<DraftSection>> {
    Ok(list(conn, review_id)?
        .into_iter()
        .find(|section| section.finding_seq == Some(seq)))
}

/// Pair each block being written with the words to keep as the writer's own.
///
/// **A block whose words did not change keeps what a rewrite replaced.** A whole-draft
/// write is normally the session writing the draft, and then what it sends *is* the
/// writer's words. But the dashboard writes the whole composition too — it is how the
/// developer changes a block of prose, because a prose block is about no finding and so
/// cannot carry an outcome — and it sends every other block back unchanged. Without this,
/// changing the summary sentence would quietly relabel a paragraph the developer had
/// rewritten as the words the agent wrote, and the one comparison the feedback loop asks
/// for would be lost to an edit somewhere else in the body.
///
/// Compared per field, so a session that rewrites a title and keeps the paragraph is the
/// author of the title and nothing more.
fn written_by_the_writer(
    conn: &Connection,
    review_id: i64,
    sections: &[NewDraftSection],
) -> Result<Vec<Written>> {
    let held = list(conn, review_id)?;

    Ok(sections
        .iter()
        .map(|section| {
            let earlier = section
                .finding_seq
                .and_then(|seq| held.iter().find(|block| block.finding_seq == Some(seq)));

            let (agent_title, agent_text) = match earlier {
                Some(block) => (
                    if block.title == section.title {
                        block.agent_title.clone()
                    } else {
                        section.title.clone()
                    },
                    if block.text == section.text {
                        block.agent_text.clone()
                    } else {
                        section.text.clone()
                    },
                ),
                None => (section.title.clone(), section.text.clone()),
            };

            Written {
                section: section.clone(),
                agent_title,
                agent_text,
            }
        })
        .collect())
}

/// One block as it is to be written: the words it holds, and the agent's own.
struct Written {
    section: NewDraftSection,
    agent_title: Option<String>,
    agent_text: String,
}

/// Every block of a draft, in order, each saying whether it posts.
fn list(conn: &Connection, review_id: i64) -> Result<Vec<DraftSection>> {
    let mut statement = conn.prepare(
        "SELECT s.position, s.kind, s.finding_seq, s.title, s.text, s.agent_title,
                s.agent_text, f.severity, f.disposition, f.round_status
         FROM draft_sections s
         LEFT JOIN findings f
             ON f.review_id = s.review_id AND f.seq = s.finding_seq
         WHERE s.review_id = ?1
         ORDER BY s.position ASC",
    )?;

    let rows = statement.query_map(params![review_id], |row| {
        let kind: DraftSectionKind = row.get("kind")?;
        let disposition: Option<Disposition> = row.get("disposition")?;
        let round_status: Option<RoundStatus> = row.get("round_status")?;

        // One rule, read through the block: a prose block always posts, and a finding
        // block posts while its finding does. `Finding::is_posted` is where that rule
        // lives; this is the same two columns, read without building a whole finding.
        let posted = match kind {
            DraftSectionKind::Prose => true,
            DraftSectionKind::Finding => matches!(
                (disposition, round_status),
                (Some(Disposition::InDraft), Some(status)) if status.is_open()
            ),
        };

        Ok(DraftSection {
            review_id,
            position: row.get("position")?,
            kind,
            finding_seq: row.get("finding_seq")?,
            title: row.get("title")?,
            text: row.get("text")?,
            agent_title: row.get("agent_title")?,
            agent_text: row.get("agent_text")?,
            severity: row.get("severity")?,
            posted,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The blocks a draft holds, in the form they would be written back in.
fn rows(conn: &Connection, review_id: i64) -> Result<Vec<Written>> {
    Ok(list(conn, review_id)?
        .into_iter()
        .map(|section| Written {
            agent_title: section.agent_title.clone(),
            agent_text: section.agent_text.clone(),
            section: NewDraftSection {
                kind: section.kind,
                finding_seq: section.finding_seq,
                title: section.title,
                text: section.text,
            },
        })
        .collect())
}

/// Write these blocks as the whole composition, numbered from 1.
///
/// Rewritten whole rather than shifted. The list is a handful of rows, SQLite will not
/// reorder a primary key in place without colliding partway through, and renumbering in
/// one pass is what keeps "the position is the store's to assign" true.
fn replace(tx: &Transaction<'_>, review_id: i64, blocks: &[Written]) -> Result<()> {
    tx.execute(
        "DELETE FROM draft_sections WHERE review_id = ?1",
        params![review_id],
    )?;

    for (index, block) in blocks.iter().enumerate() {
        tx.execute(
            "INSERT INTO draft_sections
                 (review_id, position, kind, finding_seq, title, text, agent_title,
                  agent_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                review_id,
                index as i64 + 1,
                block.section.kind,
                block.section.finding_seq,
                block.section.title,
                block.section.text,
                block.agent_title,
                block.agent_text,
            ],
        )
        .map_err(|error| unknown_parent(error, review_id))?;
    }

    Ok(())
}

/// One finding of this review, read inside the caller's transaction.
///
/// Read through the transaction rather than through `Store::find_finding`, so that what a
/// block is checked against is the same snapshot the block is written into.
fn finding_in(tx: &Transaction<'_>, review_id: i64, seq: i64) -> Result<Finding> {
    tx.query_row(
        &format!(
            "SELECT {} FROM findings WHERE review_id = ?1 AND seq = ?2",
            super::finding::COLUMNS
        ),
        params![review_id, seq],
        super::finding::row_to_finding,
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => StoreError::UnknownParent {
            entity: "draft section",
            parent: "finding",
            id: format!("{review_id}/{seq}"),
        },
        other => StoreError::Sqlite(other),
    })
}

/// A block whose draft or finding is absent is a named failure, not a raw SQLite one.
fn unknown_parent(error: rusqlite::Error, review_id: i64) -> StoreError {
    if is_foreign_key_violation(&error) {
        StoreError::UnknownParent {
            entity: "draft section",
            parent: "draft",
            id: review_id.to_string(),
        }
    } else {
        StoreError::Sqlite(error)
    }
}
