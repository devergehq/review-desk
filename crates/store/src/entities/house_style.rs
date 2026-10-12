//! What GitHub receives: the posted body's shape, and the threads that go with it.
//!
//! **The draft is blocks; what posts is composed from them.** DEV-877 made the body a list
//! of blocks so that a finding can be cut, regraded or rewritten, and DEV-891 made every
//! posted finding a block of its own. Then the posted body *was* those blocks joined end to
//! end: the review on tc-portal#9585 ran to 309 lines with no index, no severity glyph and
//! two inline threads. DEV-929 keeps the blocks and composes what GitHub receives from the
//! same record, in the shape `house-style.md` asks for:
//!
//! | The body, in order | Where it comes from |
//! | -- | -- |
//! | the verdict line, the `**Method**` line, `### Problem fit` | `prose` blocks, character for character |
//! | an index: one row per posted finding, in severity order | composed here |
//! | a finding with a line inside the diff | **not in the body** — it is a thread on that line |
//! | a finding with nowhere to anchor | its `finding` block, as a section |
//! | the folded provenance, with `Suppressed` | a `prose` block, character for character |
//!
//! **The index goes where the findings begin**, which in a draft written in the house
//! style's order is under the verdict line and `### Problem fit` — the blocks before the
//! first `finding` block are carried through ahead of it, and the blocks after the last one
//! follow the sections. Nothing here reads a prose block to decide that: the position of
//! the first finding block is the whole rule.
//!
//! **A row links to the code, not to its thread.** `house-style.md`'s Mechanics link each
//! row to its thread's `html_url`, which needs the thread to exist first — and the worker
//! posts the body and the threads in one call, on characters the developer has already
//! signed off, so no thread URL can be in the approved body. The link is to the line at
//! the head commit reviewed, and the thread sits on that same line. A finding with no file
//! links to the pull request: `review_lint.py`'s `index.rows_linked` fails a row with no
//! link at all.
//!
//! **Which side, and whose line numbers.** Always GitHub's `RIGHT`, always the new file's
//! numbering. A removed line carries no number in this record — see
//! [`crate::FindingHunk::numbered_lines`] — and `finding_evidence::validate` refuses a
//! `flagged_line` the hunk does not draw, so a flagged line is always a line still in the
//! file. Where a finding is about a line the change deleted, the worker's brief says to
//! flag the line that replaced it and to say in the comment that the code at fault is gone.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use crate::entities::composition;
use crate::error::{Result, StoreError};
use crate::model::{
    DraftHandover, DraftSection, DraftSectionKind, DraftStatus, DraftThread, FindingAnchor,
    FindingHunk, HunkLine, HunkLineMarker, RoundStatus, Severity,
};
use crate::Store;

/// The heading the index sits under.
///
/// **One table, not the house style's two.** The second one is "Resolved in this PR", and a
/// finding this round resolved is not posted as an open finding at all — `Finding::is_posted`
/// refuses it a block, and what the round found about it is prose the session writes. So
/// every row this composes is an open one, and the heading says so rather than leaving a
/// reader to infer it. `review_lint.py`'s `rows()` reads an index from a `### Open…`
/// heading, which is the same rule from the other side.
pub(crate) const INDEX_HEADING: &str = "### Open — needs a decision";

/// The index's header row and its rule, which never change.
///
/// Four columns, which is `review_lint.py`'s budget for a body table: the severity, where
/// in the code, the finding, and where it stands. The first column has no header because
/// its cell is a glyph and a word, and a header over it would say "Severity" twice.
const INDEX_HEAD: &str = "| | Where | Finding | Status |\n| -- | -- | -- | -- |";

/// The widest a table cell may be before GitHub squeezes it, from `review_lint.py`.
///
/// 140 characters of *shown* text — what the cell renders as, without a link's target or
/// its backticks. A title longer than that is clamped at a word boundary here rather than
/// posted into a column GitHub breaks between letters; the whole title is still in the
/// thread or the section the row points at.
const CELL_PROSE: usize = 140;

/// The longest unbroken token a cell may carry, from `review_lint.py`.
///
/// GitHub breaks a longer one between letters — "Process InvoiceU pdatesAc tion.php" in the
/// review that prompted the check. A `basename:line` link is well under it; the paths that
/// cross it are long ones, and the tail is the part that identifies the file.
const CELL_TOKEN: usize = 40;

/// Where the review is, so that a row can link to the code it is about.
pub(crate) struct Place {
    pub repo: String,
    pub pr: i64,
    pub head_sha: String,
}

/// What the shape needs to know about one finding that posts.
///
/// Read from the record rather than taken from a caller: the severity and the title are on
/// the block already — they are what the heading is composed from — and this is the rest,
/// which decides where the finding goes and what its row says.
pub(crate) struct Posting {
    pub file: Option<String>,
    pub line: Option<i64>,
    pub status: RoundStatus,
    pub anchor: Option<FindingAnchor>,
}

impl Store {
    /// What the review session posts: the approved body, and the threads on the code.
    ///
    /// **Refused until the developer has signed the draft off, and the store is where that
    /// is enforced rather than promised.** Nothing on the command line can approve a draft
    /// — `put_draft` accepts only `drafting` and `ready` — so a hand-over that refuses any
    /// other status is the one route from Review Desk to a pull request, and it opens only
    /// after the developer has acted.
    ///
    /// **A posted draft hands over too**, which is not a weakening: posting is the step
    /// after this one, and a session that posted and then failed to report it needs to read
    /// the same payload again to check GitHub for its own review. The statuses this refuses
    /// are the three before the sign-off.
    pub fn draft_handover(&self, review_id: i64) -> Result<DraftHandover> {
        let draft = self.get_draft(review_id)?;
        if !matches!(draft.status, DraftStatus::Approved | DraftStatus::Posted) {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "only an approved draft is handed over. The developer signs off in \
                         the dashboard, and nothing else can: a body and its threads are \
                         posted under their name",
            });
        }

        // **An approved body the blocks no longer compose to is refused, loudly.** A draft
        // approved before DEV-929 was composed in the old shape — every finding a section,
        // no index — and the threads are composed from the record *now*. Handing the two
        // over together would post each anchored finding twice, as a section and as a
        // comment, on somebody's pull request under the developer's name.
        //
        // Checked on an approved draft and not on a posted one. After posting, the record
        // is allowed to move on — the harvest reads outcomes back and a finding's round
        // status changes with them — and the body that was posted is the stored one, which
        // is what a session retrying step 2's "is it already up" check has to match on.
        if draft.status == DraftStatus::Approved && self.draft_body_is_stale(review_id)? {
            return Err(StoreError::DraftStatus {
                review_id,
                found: draft.status,
                wanted: "a draft whose body is what its blocks compose to. This one was \
                             approved in an older shape, so posting it with the threads \
                             would carry each anchored finding twice. Take the sign-off \
                             back, send it back with a note, and the session writes it \
                             again",
            });
        }

        let review = self.get_review(review_id)?;
        let threads = self.draft_threads(review_id)?;

        Ok(DraftHandover {
            review,
            draft,
            threads,
        })
    }

    /// Whether the stored body is no longer what the draft's blocks compose to.
    ///
    /// **The one state the hand-over cannot serve, and the page has to be able to say so.**
    /// Every write keeps the two in step, so this is false on a draft written by this
    /// build; what it catches is a draft approved before the composition changed shape,
    /// which an upgrade leaves behind. The way out is the developer's: take the sign-off
    /// back, send it back with a note, and the session writes it again.
    pub fn draft_body_is_stale(&self, review_id: i64) -> Result<bool> {
        let draft = self.get_draft(review_id)?;
        let composed = composition::compose(&self.conn, review_id)?;

        Ok(composed.trim_end_matches('\n') != draft.body.trim_end_matches('\n'))
    }

    /// The inline threads this review's draft hands over, in severity order.
    ///
    /// **Derived, never stored, and that is the opposite choice from the body.** The body is
    /// written into `drafts.body` on every change because it is posted verbatim and must not
    /// move under the session about to post it. A thread is composed from the same blocks and
    /// the same findings, and both are frozen once the draft is approved — `put_draft` and
    /// every decision refuse an approved draft — so there is nothing for a second copy to
    /// protect against and one more thing for it to disagree with.
    pub fn draft_threads(&self, review_id: i64) -> Result<Vec<DraftThread>> {
        let blocks = self.draft_sections(review_id)?;
        let postings = postings(&self.conn, review_id)?;
        Ok(threads(&blocks, &postings))
    }
}

/// Where the review's pull request is, for the links a row carries.
pub(crate) fn place(conn: &Connection, review_id: i64) -> Result<Place> {
    Ok(conn.query_row(
        "SELECT repo, pr, head_sha FROM reviews WHERE id = ?1",
        params![review_id],
        |row| {
            Ok(Place {
                repo: row.get("repo")?,
                pr: row.get("pr")?,
                head_sha: row.get("head_sha")?,
            })
        },
    )?)
}

/// What every finding of the review is, by number.
///
/// Three queries for the whole review, as `with_evidence_all` does and for the same reason:
/// this runs inside the transaction of every write that recomposes a body, and a read whose
/// cost grows with the number of findings is one that gets replaced later by something that
/// does not go through the store. Only the hunk's **markers** are read — the text of a line
/// decides nothing here, and the numbers a hunk draws come from the markers alone.
pub(crate) fn postings(conn: &Connection, review_id: i64) -> Result<BTreeMap<i64, Posting>> {
    let mut hunks: BTreeMap<i64, FindingHunk> = conn
        .prepare("SELECT finding_seq, file, start_line FROM finding_hunks WHERE review_id = ?1")?
        .query_map(params![review_id], |row| {
            Ok((
                row.get::<_, i64>("finding_seq")?,
                FindingHunk {
                    file: row.get("file")?,
                    start_line: row.get("start_line")?,
                    lines: Vec::new(),
                },
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;

    let mut lines = conn.prepare(
        "SELECT finding_seq, marker FROM finding_hunk_lines
         WHERE review_id = ?1 ORDER BY finding_seq ASC, position ASC",
    )?;
    for row in lines.query_map(params![review_id], |row| {
        Ok((
            row.get::<_, i64>("finding_seq")?,
            row.get::<_, HunkLineMarker>("marker")?,
        ))
    })? {
        let (seq, marker) = row?;
        if let Some(hunk) = hunks.get_mut(&seq) {
            // The text is not read, and a line here stands for a line there: the numbering
            // and the "did the diff draw this" reading are both about markers.
            hunk.lines.push(HunkLine {
                marker,
                text: String::new(),
            });
        }
    }

    let mut findings = conn.prepare(
        "SELECT seq, file, line, flagged_line, round_status FROM findings WHERE review_id = ?1",
    )?;
    let rows = findings.query_map(params![review_id], |row| {
        Ok((
            row.get::<_, i64>("seq")?,
            row.get::<_, Option<String>>("file")?,
            row.get::<_, Option<i64>>("line")?,
            row.get::<_, Option<i64>>("flagged_line")?,
            row.get::<_, RoundStatus>("round_status")?,
        ))
    })?;

    let mut held = BTreeMap::new();
    for row in rows {
        let (seq, file, line, flagged_line, status) = row?;
        let anchor = anchor(file.as_deref(), flagged_line, hunks.get(&seq));
        held.insert(
            seq,
            Posting {
                file,
                line,
                status,
                anchor,
            },
        );
    }

    Ok(held)
}

/// Where GitHub can hang this finding's comment, from the four facts the record holds.
///
/// The same rule as [`crate::Finding::anchor`], reading the columns rather than a built
/// `Finding`: a file, a flagged line, a hunk, and a hunk that both came from the diff and
/// draws that line. Stated twice because the two callers have different things in hand —
/// this one is inside a write's transaction and never builds a finding — and held together
/// by `anchoring_agrees_with_the_findings_own_reading` in `tests/house_style.rs`.
fn anchor(
    file: Option<&str>,
    flagged_line: Option<i64>,
    hunk: Option<&FindingHunk>,
) -> Option<FindingAnchor> {
    file?;
    let line = flagged_line?;
    let hunk = hunk?;
    if !hunk.drawn_by_the_diff() || !hunk.draws_line(line) {
        return None;
    }

    Some(FindingAnchor {
        path: hunk.file.clone(),
        line,
    })
}

/// The body these blocks compose to, in the house style's shape.
///
/// Joined by a blank line and ending in one newline, as the blocks always were. A
/// composition with nothing to post is the empty string rather than a lone newline: a draft
/// at `drafting` may hold no body at all.
pub(crate) fn body(
    place: &Place,
    blocks: &[DraftSection],
    postings: &BTreeMap<i64, Posting>,
) -> String {
    let index = index(place, blocks, postings);
    let mut parts: Vec<String> = Vec::new();
    let mut indexed = false;

    for block in blocks {
        if !block.posted {
            continue;
        }

        match block.kind {
            DraftSectionKind::Prose => parts.push(block.text.clone()),
            DraftSectionKind::Finding => {
                // The index goes where the findings begin, once.
                if !indexed {
                    if let Some(table) = &index {
                        parts.push(table.clone());
                    }
                    indexed = true;
                }

                // A finding with a line inside the diff is a row and a thread, and the body
                // does not carry it twice.
                if block
                    .finding_seq
                    .and_then(|seq| postings.get(&seq))
                    .is_some_and(|posting| posting.anchor.is_some())
                {
                    continue;
                }

                if let Some(section) = section(block) {
                    parts.push(section);
                }
            }
        }
    }

    if parts.is_empty() {
        return String::new();
    }
    format!("{}\n", parts.join("\n\n"))
}

/// One finding's section: the composed heading, a blank line, then the block's text.
///
/// `None` for a finding block the schema should have refused — no finding, no title or no
/// severity. It drops the block rather than panicking: a body is posted to GitHub under the
/// developer's name, and half a heading is worse than a block absent.
fn section(block: &DraftSection) -> Option<String> {
    let seq = block.finding_seq?;
    let title = block.title.as_deref()?;
    let heading = super::composition::finding_heading(block.severity?, seq, title);

    if block.text.is_empty() {
        Some(heading)
    } else {
        Some(format!("{heading}\n\n{}", block.text))
    }
}

/// The index: one row per posted finding, most serious first, then by number.
///
/// `None` when the draft posts no finding, which is a draft of prose alone — and then the
/// composition is exactly what it was before DEV-929, down to the character.
fn index(
    place: &Place,
    blocks: &[DraftSection],
    postings: &BTreeMap<i64, Posting>,
) -> Option<String> {
    let mut rows: Vec<(u8, i64, String)> = blocks
        .iter()
        .filter(|block| block.posted && block.kind == DraftSectionKind::Finding)
        .filter_map(|block| {
            let seq = block.finding_seq?;
            let severity = block.severity?;
            // **The title is asked for here as well**, because a row is a claim that the
            // finding posts somewhere and a block with no title posts nowhere: `section`
            // and `threads` both drop one. A row with an empty Finding cell would point at
            // a section that is not in the body.
            let title = block.title.as_deref()?;
            let posting = postings.get(&seq)?;
            Some((
                severity.rank(),
                seq,
                row(place, severity, seq, title, posting),
            ))
        })
        .collect();

    if rows.is_empty() {
        return None;
    }

    rows.sort_by_key(|(rank, seq, _)| (*rank, *seq));
    let rows: Vec<&str> = rows.iter().map(|(_, _, row)| row.as_str()).collect();

    Some(format!(
        "{INDEX_HEADING}\n\n{INDEX_HEAD}\n{}",
        rows.join("\n")
    ))
}

/// One index row.
///
/// The glyph and the word together, which is what the house style's index carries and what
/// its "Casing and formatting" rule asks for: the word carries the severity and the glyph
/// is beside it. The number is folded into the same cell rather than given a column of its
/// own — a fifth column is the one that leaves the page.
fn row(place: &Place, severity: Severity, seq: i64, title: &str, posting: &Posting) -> String {
    let severity_cell = format!("{} {} · F{seq}", severity.glyph(), severity.label());
    let where_cell = match &posting.anchor {
        Some(anchor) => code_link(place, &anchor.path, Some(anchor.line)),
        None => match &posting.file {
            Some(file) => code_link(place, file, posting.line),
            // Nothing in the code to point at: a correction to the description, a missing
            // ticket, a wrong figure. The pull request is where such a finding is about.
            None => format!(
                "[the pull request](https://github.com/{}/pull/{})",
                place.repo, place.pr
            ),
        },
    };
    let finding_cell = cell(title);
    let status_cell = format!(
        "{} · {}",
        standing(posting.status),
        if posting.anchor.is_some() {
            "on the code"
        } else {
            "in the body"
        }
    );

    format!("| {severity_cell} | {where_cell} | {finding_cell} | {status_cell} |")
}

/// Where the finding stands this round, in the words the chip and the row share.
fn standing(status: RoundStatus) -> &'static str {
    match status {
        RoundStatus::StillOpen => "Still open",
        // Every other status is settled and gets no block, so `new` is what reaches here.
        _ => "Open",
    }
}

/// A link to the line at the head commit reviewed, with `basename:line` as its text.
fn code_link(place: &Place, path: &str, line: Option<i64>) -> String {
    let base = format!(
        "https://github.com/{}/blob/{}/{}",
        place.repo, place.head_sha, path
    );
    let name = path.rsplit('/').next().unwrap_or(path);
    let (target, text) = match line {
        Some(line) => (format!("{base}#L{line}"), format!("{name}:{line}")),
        None => (base, name.to_string()),
    };

    format!("[`{}`]({})", scannable(&text), link(&target))
}

/// A link target markdown can carry: wrapped in `<>` when it holds anything that would end
/// it early.
///
/// A path with a space in it is rare and legal, and `[text](a b)` is not a link at all.
fn link(target: &str) -> String {
    if target.contains(' ') || target.contains('(') || target.contains(')') {
        format!("<{target}>")
    } else {
        target.to_string()
    }
}

/// A path short enough for a cell, keeping the end of it.
///
/// GitHub breaks a token over [`CELL_TOKEN`] between letters, and what identifies a file is
/// its name rather than the directories above it — so what is dropped is the front, and the
/// ellipsis says so.
fn scannable(text: &str) -> String {
    if text.chars().count() <= CELL_TOKEN {
        return text.to_string();
    }

    let tail: String = text
        .chars()
        .skip(text.chars().count() - (CELL_TOKEN - 1))
        .collect();
    format!("…{tail}")
}

/// A cell's text: on one line, its pipes escaped, clamped to what GitHub can draw.
///
/// The clamp is at a word boundary, and it stops short of an unclosed backtick: a cell
/// ending mid-span swallows the words after it. The whole title is still in the section's
/// heading or on the thread, which is what the row points at.
fn cell(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ").replace('|', "\\|");
    if shown(&flat).chars().count() <= CELL_PROSE {
        return flat;
    }

    let mut kept: Vec<&str> = Vec::new();
    for word in flat.split_whitespace() {
        let mut candidate = kept.clone();
        candidate.push(word);
        // Two characters for the ellipsis and the space before it.
        if shown(&candidate.join(" ")).chars().count() + 2 > CELL_PROSE {
            break;
        }
        kept.push(word);
    }

    while kept
        .iter()
        .map(|word| word.matches('`').count())
        .sum::<usize>()
        % 2
        == 1
    {
        kept.pop();
    }

    // **A clamp that kept nothing keeps the start of the text instead.** One word longer
    // than the whole cell leaves the loop above with an empty list, and a row reading `…`
    // says less about the finding than a sentence cut mid-word does. Cut on a character
    // boundary, which is all there is to cut on.
    if kept.is_empty() {
        let start: String = flat.chars().take(CELL_PROSE - 2).collect();
        // **And not ending inside a code span.** A cell with an odd number of backticks has
        // an open span in it, and a table row is split on the pipes *outside* code spans —
        // so one unclosed backtick can swallow the two cells after it.
        let start = match start.matches('`').count() % 2 {
            1 => start[..start.rfind('`').unwrap_or(0)].to_string(),
            _ => start,
        };
        return format!("{} …", start.trim_end());
    }

    format!("{} …", kept.join(" "))
}

/// What a cell renders as: no backticks and no emphasis, so the clamp measures what a
/// reader sees rather than the markup around it. `review_lint.py`'s `shown()`, for the one
/// case a title can hold.
fn shown(text: &str) -> String {
    text.replace(['`', '*', '_'], "")
}

/// The threads the draft hands over, most serious first, then by number.
pub(crate) fn threads(
    blocks: &[DraftSection],
    postings: &BTreeMap<i64, Posting>,
) -> Vec<DraftThread> {
    let mut held: Vec<DraftThread> = blocks
        .iter()
        .filter(|block| block.posted && block.kind == DraftSectionKind::Finding)
        .filter_map(|block| {
            let seq = block.finding_seq?;
            let severity = block.severity?;
            let title = block.title.clone()?;
            let posting = postings.get(&seq)?;
            let anchor = posting.anchor.clone()?;

            let key = format!("F{seq}");
            Some(DraftThread {
                finding_seq: seq,
                severity,
                body: thread_body(
                    severity,
                    &key,
                    &title,
                    standing(posting.status),
                    &block.text,
                ),
                finding_key: key,
                title,
                path: anchor.path,
                line: anchor.line,
                side: FindingAnchor::SIDE.to_string(),
            })
        })
        .collect();

    held.sort_by_key(|thread| (thread.severity.rank(), thread.finding_seq));
    held
}

/// One thread, as the house style shapes it.
///
/// A blocker, a should and a question sit **inside a GitHub alert**, which draws a coloured
/// left rail and carries the severity before a word is read; the alert is the one its
/// severity picks. A nit takes the lighter shape with no rail and no labels, because four
/// labels on a one-line nit is why nits read as heavy as blockers — and the contrast
/// between the two shapes is the point.
///
/// **The block's text is carried through, blockquoted and otherwise untouched.** It is the
/// text the developer read and signed off, and the `in diff` tag it opens with is prose no
/// part of this record carries, so nothing here lifts it into the title line. The title
/// line is composed: the severity, the finding's number, the title, and where the finding
/// stands as a code span, which is what `review_lint.py`'s `threads.disposition_top` reads.
fn thread_body(severity: Severity, key: &str, title: &str, standing: &str, text: &str) -> String {
    let title_line = format!("**{} · {key}** — {title} · `{standing}`", severity.label());
    // **Outside the rail, on its own line.** The house style puts the `agent:` marker
    // outside the alert because it is provenance rather than finding, and this is the same
    // kind of thing: it is how the comment is matched back, not part of what is claimed.
    let marker = thread_marker(key);

    let Some(alert) = alert(severity) else {
        // The nit's shape: the glyph does the work the rail does above, and the text follows
        // on the next line, as `house-style.md`'s own example has it.
        return if text.is_empty() {
            format!("{} {title_line}\n\n{marker}", severity.glyph())
        } else {
            format!("{} {title_line}\n{text}\n\n{marker}", severity.glyph())
        };
    };

    let mut lines = vec![format!("> [!{alert}]"), format!("> {title_line}")];
    if !text.is_empty() {
        lines.push(">".to_string());
        lines.extend(text.lines().map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        }));
    }
    lines.push(String::new());
    lines.push(marker);

    lines.join("\n")
}

/// The line a composed comment ends with, so a session can match it back to its finding.
///
/// **The path and the line are not a key.** A Should and a Nit on one line is ordinary, and
/// GitHub's `pulls/<n>/reviews` answers with the review rather than with the comments it
/// made — so a session reading the comments back and pairing them on `(path, line)` would
/// link the wrong comment to the wrong finding, silently. Measured by the DEV-929 worker
/// half. An HTML comment is invisible on GitHub and comes back verbatim in the comment's
/// `body`, which is what makes it a join rather than a hope.
pub fn thread_marker(key: &str) -> String {
    format!("<!-- review-desk:{key} -->")
}

/// Which alert a severity picks, and `None` for the one that takes no rail.
///
/// `house-style.md`: Blocker to `[!CAUTION]`, Should to `[!WARNING]`, Question to `[!NOTE]`.
/// `review_lint.py` holds the same three in its `ALERT` map and fails a nit that has a rail.
fn alert(severity: Severity) -> Option<&'static str> {
    match severity {
        Severity::Blocker => Some("CAUTION"),
        Severity::Should => Some("WARNING"),
        Severity::Question => Some("NOTE"),
        Severity::Nit => None,
    }
}
