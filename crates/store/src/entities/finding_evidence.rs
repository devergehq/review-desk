//! What a finding is about: its hunk, the comment as it will post, the consequence, the
//! proof and the fix.
//!
//! **Its own module, for the reason the whole of DEV-874 is its own module: this text is
//! the only copy there is.** Review Desk does not fetch code — Patrick's rule — so the
//! hunk a finding carries was written once, by an agent, and nothing can get it back if
//! this file loses a tab or a trailing space. The rule for everything here is that it
//! goes in and comes out unaltered: no trim, no re-wrap, no normalising, and a line that
//! is one space long is one space long.
//!
//! Three tables, and one of them is interesting. The hunk's lines are rows rather than a
//! unified diff in a TEXT column because a diff makes the marker and the text share a
//! character position: a line whose own text begins with `+` is then indistinguishable
//! from an added line, and a line that is empty is indistinguishable from the end of the
//! hunk. Rows have no such coupling.
//!
//! The only text this module looks at before storing is a fix reference, which is a
//! pointer the page draws as one line. A blank one draws as a blank line, so it is
//! refused by name. Refusing is not normalising; nothing that is stored is altered.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};
use crate::model::{Finding, FindingHunk, HunkLine, NewFinding};
use crate::Store;

/// Refuse evidence the record cannot hold or the page cannot draw.
///
/// Called before the insert, so a finding is never half written. The three refusals are
/// each a thing an agent gets wrong: a hunk with nothing in it, a hunk whose file is
/// blank, and a comment anchored to a line the hunk does not draw.
pub(crate) fn validate(new: &NewFinding) -> Result<()> {
    if let Some(hunk) = &new.hunk {
        if hunk.file.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: "hunk.file",
                value: hunk.file.clone(),
                expected: "the file the hunk is from".to_string(),
            });
        }
        if hunk.start_line < 1 {
            return Err(StoreError::InvalidValue {
                field: "hunk.start_line",
                value: hunk.start_line.to_string(),
                expected: "the number the hunk's first numbered line carries, from 1".to_string(),
            });
        }
        if hunk.lines.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "hunk.lines",
                value: String::new(),
                expected: "at least one line; a hunk with no lines is a file name".to_string(),
            });
        }
    }

    if let Some(flagged) = new.flagged_line {
        if flagged < 1 {
            return Err(StoreError::InvalidValue {
                field: "flagged_line",
                value: flagged.to_string(),
                expected: "a line number, from 1".to_string(),
            });
        }

        // Checked only against a hunk that was given. A flagged line on its own is a
        // finding that says which line its comment is about and shows no code, which is
        // every finding written before this existed.
        if let Some(hunk) = &new.hunk {
            if !hunk.draws_line(flagged) {
                return Err(StoreError::InvalidValue {
                    field: "flagged_line",
                    value: flagged.to_string(),
                    expected: drawn_lines(hunk),
                });
            }
        }
    }

    for reference in &new.fix_references {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: "fix_references",
                value: reference.clone(),
                expected: "sibling code to point at, such as SendInvoiceEmail.php:44".to_string(),
            });
        }
    }

    Ok(())
}

/// What a hunk's flagged line could have been, for the refusal above.
///
/// A removed line carries no number, so the numbers a hunk draws run unbroken from
/// `start_line` — which is why this can name a range rather than a list, even for a hunk
/// of forty lines.
fn drawn_lines(hunk: &FindingHunk) -> String {
    let numbers: Vec<i64> = hunk
        .numbered_lines()
        .into_iter()
        .filter_map(|(number, _)| number)
        .collect();

    match (numbers.first(), numbers.last()) {
        (Some(first), Some(last)) if first == last => {
            format!("line {first}, the one line this hunk draws")
        }
        (Some(first), Some(last)) => format!("a line this hunk draws, {first} to {last}"),
        // Every line is a removal, so the hunk draws no numbers at all.
        _ => "a hunk with a line still in the file, which this one has none of".to_string(),
    }
}

/// Write a finding's hunk and fix references.
///
/// Takes the connection the caller's transaction is open on, so the hunk, its lines and
/// the finding itself are one step. A finding stored with the previous attempt's hunk
/// still attached would be a worse record than a finding with no hunk.
pub(crate) fn write(
    conn: &Connection,
    review_id: i64,
    seq: i64,
    new: &NewFinding,
) -> rusqlite::Result<()> {
    if let Some(hunk) = &new.hunk {
        conn.execute(
            "INSERT INTO finding_hunks (review_id, finding_seq, file, start_line)
             VALUES (?1, ?2, ?3, ?4)",
            params![review_id, seq, hunk.file, hunk.start_line],
        )?;

        // `position` from 1, assigned here and never from the rowid: the order the lines
        // are drawn in is the order they were given in, and that has to be stated.
        for (position, line) in (1i64..).zip(&hunk.lines) {
            conn.execute(
                "INSERT INTO finding_hunk_lines
                     (review_id, finding_seq, position, marker, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![review_id, seq, position, line.marker, line.text],
            )?;
        }
    }

    for (position, reference) in (1i64..).zip(&new.fix_references) {
        conn.execute(
            "INSERT INTO finding_fix_references
                 (review_id, finding_seq, position, reference)
             VALUES (?1, ?2, ?3, ?4)",
            params![review_id, seq, position, reference],
        )?;
    }

    Ok(())
}

impl Store {
    /// A finding's hunk, or `None`.
    pub(crate) fn finding_hunk(&self, review_id: i64, seq: i64) -> Result<Option<FindingHunk>> {
        let head: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT file, start_line FROM finding_hunks
                 WHERE review_id = ?1 AND finding_seq = ?2",
                params![review_id, seq],
                |row| Ok((row.get("file")?, row.get("start_line")?)),
            )
            .optional()?;

        let Some((file, start_line)) = head else {
            return Ok(None);
        };

        let mut statement = self.conn.prepare(
            "SELECT marker, text FROM finding_hunk_lines
             WHERE review_id = ?1 AND finding_seq = ?2 ORDER BY position ASC",
        )?;
        let lines = statement
            .query_map(params![review_id, seq], row_to_hunk_line)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Some(FindingHunk {
            file,
            start_line,
            lines,
        }))
    }

    /// A finding's fix references, in the order they were given.
    pub(crate) fn finding_fix_references(&self, review_id: i64, seq: i64) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT reference FROM finding_fix_references
             WHERE review_id = ?1 AND finding_seq = ?2 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id, seq], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Fill in one finding's hunk and fix references.
    ///
    /// Every read path goes through this. `row_to_finding` cannot do it — a row builder
    /// has no connection to read through, which is the arrangement `find_brief` already
    /// uses for a brief's options.
    pub(crate) fn with_evidence(&self, finding: Finding) -> Result<Finding> {
        Ok(Finding {
            hunk: self.finding_hunk(finding.review_id, finding.seq)?,
            fix_references: self.finding_fix_references(finding.review_id, finding.seq)?,
            ..finding
        })
    }

    /// Fill in the hunks and fix references of every finding of one review.
    ///
    /// Three queries for the whole review rather than two per finding. Not for the speed —
    /// this is one developer's SQLite file — but because `list_findings` is what serves the
    /// One review screen, and a read whose cost grows with the number of findings is a
    /// read that gets replaced later by one that does not go through the store.
    pub(crate) fn with_evidence_all(&self, findings: Vec<Finding>) -> Result<Vec<Finding>> {
        let Some(review_id) = findings.first().map(|finding| finding.review_id) else {
            return Ok(findings);
        };

        let mut heads = self.conn.prepare(
            "SELECT finding_seq, file, start_line FROM finding_hunks WHERE review_id = ?1",
        )?;
        let mut hunks: BTreeMap<i64, FindingHunk> = heads
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

        let mut lines = self.conn.prepare(
            "SELECT finding_seq, marker, text FROM finding_hunk_lines
             WHERE review_id = ?1 ORDER BY finding_seq ASC, position ASC",
        )?;
        for row in lines.query_map(params![review_id], |row| {
            Ok((row.get::<_, i64>("finding_seq")?, row_to_hunk_line(row)?))
        })? {
            let (seq, line) = row?;
            // A line whose hunk head is missing cannot happen: the head is written first,
            // in the same transaction. Dropped rather than unwrapped so that a database
            // somebody has been in by hand serves what it has.
            if let Some(hunk) = hunks.get_mut(&seq) {
                hunk.lines.push(line);
            }
        }

        let mut references = self.conn.prepare(
            "SELECT finding_seq, reference FROM finding_fix_references
             WHERE review_id = ?1 ORDER BY finding_seq ASC, position ASC",
        )?;
        let mut by_finding: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        for row in references.query_map(params![review_id], |row| {
            Ok((row.get::<_, i64>("finding_seq")?, row.get::<_, String>(1)?))
        })? {
            let (seq, reference) = row?;
            by_finding.entry(seq).or_default().push(reference);
        }

        Ok(findings
            .into_iter()
            .map(|finding| Finding {
                hunk: hunks.remove(&finding.seq),
                fix_references: by_finding.remove(&finding.seq).unwrap_or_default(),
                ..finding
            })
            .collect())
    }
}

/// One hunk line from a row selecting `marker, text`.
fn row_to_hunk_line(row: &rusqlite::Row<'_>) -> rusqlite::Result<HunkLine> {
    Ok(HunkLine {
        marker: row.get("marker")?,
        text: row.get("text")?,
    })
}
