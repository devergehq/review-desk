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
//! | `finding` with nowhere to anchor | [`finding_heading`], a blank line, then its text — **while the finding is posted** |
//! | `finding` with a line inside the diff | an index row and an inline **thread**, not a section |
//!
//! Blocks are joined by a blank line and the body ends in one newline. **Since DEV-929 the
//! body is also shaped**: an index of every posted finding sits where the findings begin,
//! and a finding GitHub can anchor is lifted out of the body into a thread on the line it
//! concerns. [`super::house_style`] is where that shape and its reasons live. What stays
//! true here is that a prose block is carried through character for character — which is
//! what lets a mermaid fence survive — and that a finding's heading is composed from its
//! live severity rather than stored, so a regrade changes it.
//!
//! **The body is stored, not derived.** Every write and every decision recomposes it into
//! `drafts.body` in the same transaction. Composing it at read time would let a later
//! change to the record alter an approved draft underneath the session that is about to
//! post it — and `drafts.body` is the one thing in this database that is posted verbatim.

use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use crate::entities::house_style;
use crate::error::{Result, StoreError};
use crate::model::{
    Disposition, DraftSection, DraftSectionKind, Finding, NewDraftSection, RoundStatus, Severity,
    Timestamp,
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
        recompose(&tx, review_id, Lint::Clear, self.now())?;
        self.mark_review_active(review_id)?;
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
                // **The title is not checked here, and that is a division rather than an
                // omission.** A finding block may state no title: the finding owns it, and
                // resolving it needs a read. This function is called before the
                // transaction opens — `put_draft`'s own comment says why, and it was found
                // sitting inside one by the review of DEV-877 — so the title rule lives in
                // `resolve_titles`, which has the transaction it needs. What stays here is
                // a *stated* title that is blank, which needs nothing read.
                if section
                    .title
                    .as_deref()
                    .is_some_and(|title| title.trim().is_empty())
                {
                    return Err(refuse(
                        format!("block {at} is about F{seq} and states a blank title"),
                        "the heading's title — the part after the severity and the number \
                         — or no title at all, which takes the finding's own",
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

/// What the review's blocks compose to: the body GitHub receives.
///
/// **The blocks are the draft; the shape is the house style's.** Until DEV-929 this was the
/// posted blocks joined end to end, which is how a real review came out as
/// 309 lines with no index and two threads. The joining is still here — a prose block is
/// carried through character for character and a finding block still composes its own
/// heading — and what sits between them is now an index, with each anchored finding lifted
/// out of the body and handed over as a thread instead. [`super::house_style`] holds that
/// shape and the reason for each part of it.
///
/// A composition with nothing to post is the empty string rather than a lone newline: a
/// draft at `drafting` may hold no body at all.
pub(crate) fn compose(conn: &Connection, review_id: i64) -> Result<String> {
    let blocks = list(conn, review_id)?;
    let place = house_style::place(conn, review_id)?;
    let postings = house_style::postings(conn, review_id)?;

    Ok(house_style::body(&place, &blocks, &postings))
}

/// Write the body the review's blocks compose to, and return it.
///
/// **The lint is cleared on every change to the body, and that is the rule rather than a
/// convenience.** The review-craft lint was run by the agent over the body it wrote; the
/// moment a decision changes that body the result is about a different document, and a
/// lint that still said "passes" would be the record vouching for words nobody checked.
pub(crate) fn recompose(
    tx: &Transaction<'_>,
    review_id: i64,
    lint: Lint,
    at: Timestamp,
) -> Result<String> {
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
            params![review_id, body, at],
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
///
/// **The check is on the blocks the caller is adding or changing, and not on the ones it
/// is handing back.** What the refusal is for is a session writing a draft that raises a
/// finding the composition will drop — and adding a block, or giving an existing one new
/// words, is that claim. Handing back a block the store itself just served is not.
///
/// Refusing those too, which is what this did until the review of DEV-877 found it, made
/// the first decision that took a finding out of the post the last prose edit the
/// developer could make: `draft_sections` serves a cut finding's block deliberately, the
/// dashboard writes the whole composition back to change the summary sentence, and the
/// block the store had just handed it was the block that failed the write. Every recovery
/// was worse — dropping the block loses what a rewrite replaced, and raising the finding
/// again to edit it writes outcomes nobody decided.
pub(crate) fn write(
    tx: &Transaction<'_>,
    review_id: i64,
    sections: &[NewDraftSection],
) -> Result<()> {
    // **The finding owns its title, and this is where that is true rather than described.**
    //
    // A finding block's title had to be stated, so the same words lived twice — once on the
    // finding and once on every block about it — and two copies of a title are two titles
    // that can disagree. A block written with none now takes the finding's, falling back to
    // its claim for a finding nobody has titled, which is every finding written before
    // migration 11.
    //
    // **It does not weaken what DEV-877 guarantees, and that was checked rather than
    // assumed.** A caller that states a title still gets exactly that title, so a body with
    // no decisions in it still equals what the agent wrote character for character; and
    // `agent_title` is set from the resolved words either way, so a rewrite still keeps what
    // it replaced. What is *not* done here is refusing a stated title that differs from the
    // finding's: `rewrite` exists to make them differ, and a heading the posted review
    // carries is not required to be the one line a row of the code review page carries.
    let resolved = resolve_titles(tx, review_id, sections)?;
    let sections = resolved.as_slice();

    validate(sections)?;

    let blocks = written_by_the_writer(tx, review_id, sections)?;
    for block in &blocks {
        let Some(seq) = block.section.finding_seq else {
            continue;
        };
        if !block.claims_the_finding_is_posted {
            continue;
        }
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

    replace(tx, review_id, &blocks)
}

/// Give every finding block with no title of its own the finding's.
///
/// Read inside the caller's transaction, so a block cannot take a title from a finding that
/// is changing underneath it. A block about a finding that is not there is left alone: the
/// foreign key refuses it a moment later and naming the missing finding is that error's job,
/// not this one's.
fn resolve_titles(
    tx: &Transaction<'_>,
    review_id: i64,
    sections: &[NewDraftSection],
) -> Result<Vec<NewDraftSection>> {
    let mut resolved = Vec::with_capacity(sections.len());

    for section in sections {
        let needs_a_title = section.kind == DraftSectionKind::Finding && section.title.is_none();
        let Some(seq) = section.finding_seq.filter(|_| needs_a_title) else {
            resolved.push(section.clone());
            continue;
        };

        let Ok(found) = finding_in(tx, review_id, seq) else {
            // A block about a finding that is not there. Left alone: the foreign key
            // refuses it a moment later and naming the missing finding is that error's
            // job, not this one's.
            resolved.push(section.clone());
            continue;
        };

        // The finding's title, else its claim. A finding with neither is a finding whose
        // claim is the empty string, and a heading reading `### Nit · F1 ·` is posted to
        // GitHub verbatim — so it is refused here rather than composed.
        let title = found.title.unwrap_or(found.claim);
        if title.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: "sections",
                value: format!("F{seq} has no title and no claim to fall back on"),
                expected: "a title on the block, or a finding with a title or a claim to \
                           head it with; a heading with nothing after the number is a \
                           finding the posted review does not state"
                    .to_string(),
            });
        }

        resolved.push(NewDraftSection {
            title: Some(title),
            ..section.clone()
        });
    }

    Ok(resolved)
}

/// Refuse the words a rewrite would put in a block.
///
/// **The one write path that did not run [`validate`].** A rewrite replaces a block's title
/// and paragraph through a statement of its own, so until the review of DEV-877 found it,
/// the two rules the composition rests on were enforced on every path except the decision
/// whose purpose is to change those words — and a body with three blank lines in it, or a
/// heading reading `### Blocker · F1 ·`, is posted to GitHub verbatim.
///
/// Checked without the database, so the caller can refuse before its transaction opens.
pub(crate) fn validate_rewrite(title: Option<&str>, text: Option<&str>) -> Result<()> {
    if let Some(title) = title {
        if title.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: "title",
                value: format!("{title:?}"),
                expected: "the heading's title — the part after the severity and the \
                           number. A heading with nothing after the number is a finding \
                           the posted review does not state"
                    .to_string(),
            });
        }
    }

    if let Some(text) = text {
        if text.starts_with('\n') || text.ends_with('\n') {
            return Err(StoreError::InvalidValue {
                field: "text",
                value: "a paragraph that begins or ends with a newline".to_string(),
                expected: "the paragraph's own text; blocks are joined by one blank line, \
                           and a block that brings its own composes a body nobody wrote"
                    .to_string(),
            });
        }
    }

    Ok(())
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
    words: &BlockWords,
) -> Result<()> {
    let mut held = rows(tx, review_id)?;
    let after = held
        .iter()
        .rposition(|row| row.section.kind == DraftSectionKind::Finding)
        .map_or(held.len(), |index| index + 1);

    held.insert(
        after,
        Written {
            section: NewDraftSection::finding(seq, &words.title, &words.text),
            agent_title: Some(words.agent_title.clone()),
            agent_text: words.agent_text.clone(),
            claims_the_finding_is_posted: true,
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

            let (agent_title, agent_text, changed) = match earlier {
                Some(block) => {
                    let same_title = block.title == section.title;
                    let same_text = block.text == section.text;
                    (
                        if same_title {
                            block.agent_title.clone()
                        } else {
                            section.title.clone()
                        },
                        if same_text {
                            block.agent_text.clone()
                        } else {
                            section.text.clone()
                        },
                        !(same_title && same_text),
                    )
                }
                // No block for this finding yet, so writing one is a claim that it posts.
                None => (section.title.clone(), section.text.clone(), true),
            };

            Written {
                section: section.clone(),
                agent_title,
                agent_text,
                claims_the_finding_is_posted: section.finding_seq.is_some() && changed,
            }
        })
        .collect())
}

/// The words a new block is written with: the ones it posts, and the writer's own.
///
/// **Two pairs rather than one**, because the developer may post a held-back nit in their
/// own words — the designs' "Post it" — and the agent's words then still exist: they are
/// the finding's claim and its consequence, which is what the store would have composed
/// the block from. Writing the developer's words into both, which is what this did until
/// the review of DEV-877 found it, made the record say the agent wrote them and left
/// [`DraftSection::rewritten`] reading false about a block the developer had written.
#[derive(Debug)]
pub(crate) struct BlockWords {
    pub title: String,
    pub text: String,
    pub agent_title: String,
    pub agent_text: String,
}

/// One block as it is to be written: the words it holds, and the agent's own.
struct Written {
    section: NewDraftSection,
    agent_title: Option<String>,
    agent_text: String,
    /// Whether writing this block is a claim that its finding is posted — true when it is
    /// being added, or given words it did not have. See [`write`].
    claims_the_finding_is_posted: bool,
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
            // Handed back as it stands, so it claims nothing new.
            claims_the_finding_is_posted: false,
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
