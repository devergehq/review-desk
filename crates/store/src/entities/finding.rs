//! Findings: what a review found, each with a sequence number stable within its review.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{Result, StoreError};
use crate::model::{now, Disposition, Finding, NewFinding};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, seq, lens, severity, file, line, claim, evidence, \
                       disposition, disposition_reason, created_at";

impl Store {
    /// Add a finding to a review, assigning the next sequence number within it.
    ///
    /// Fails with [`StoreError::UnknownParent`] if the review does not exist; the
    /// database enforces that, not this function.
    pub fn add_finding(&self, review_id: i64, new: NewFinding) -> Result<Finding> {
        // One transaction, so two callers adding findings at once cannot be handed the
        // same sequence number.
        let tx = self.conn.unchecked_transaction()?;

        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM findings WHERE review_id = ?1",
            params![review_id],
            |row| row.get(0),
        )?;

        tx.execute(
            "INSERT INTO findings
                 (review_id, seq, lens, severity, file, line, claim, evidence,
                  disposition, disposition_reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                review_id,
                seq,
                new.lens,
                new.severity,
                new.file,
                new.line,
                new.claim,
                new.evidence,
                new.disposition,
                new.disposition_reason,
                now(),
            ],
        )
        .map_err(|error| {
            if is_foreign_key_violation(&error) {
                StoreError::UnknownParent {
                    entity: "finding",
                    parent: "review",
                    id: review_id.to_string(),
                }
            } else {
                StoreError::Sqlite(error)
            }
        })?;

        tx.commit()?;
        self.get_finding(review_id, seq)
    }

    /// Read one finding. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_finding(&self, review_id: i64, seq: i64) -> Result<Finding> {
        self.find_finding(review_id, seq)?
            .ok_or(StoreError::NotFound {
                entity: "finding",
                id: format!("{review_id}/{seq}"),
            })
    }

    /// Read one finding, or `None`.
    pub fn find_finding(&self, review_id: i64, seq: i64) -> Result<Option<Finding>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM findings WHERE review_id = ?1 AND seq = ?2"),
                params![review_id, seq],
                row_to_finding,
            )
            .optional()?)
    }

    /// Every finding of a review, in sequence order.
    pub fn list_findings(&self, review_id: i64) -> Result<Vec<Finding>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM findings WHERE review_id = ?1 ORDER BY seq ASC"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_finding)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Change a finding's disposition, with the reason it changed.
    pub fn set_finding_disposition(
        &self,
        review_id: i64,
        seq: i64,
        disposition: Disposition,
        reason: Option<String>,
    ) -> Result<Finding> {
        // Checked first, so an unknown finding is NotFound rather than a no-op update.
        self.get_finding(review_id, seq)?;

        self.conn.execute(
            "UPDATE findings SET disposition = ?3, disposition_reason = ?4
             WHERE review_id = ?1 AND seq = ?2",
            params![review_id, seq, disposition, reason],
        )?;

        self.get_finding(review_id, seq)
    }
}

/// Build a finding from a row selecting [`COLUMNS`].
fn row_to_finding(row: &Row<'_>) -> rusqlite::Result<Finding> {
    Ok(Finding {
        review_id: row.get("review_id")?,
        seq: row.get("seq")?,
        lens: row.get("lens")?,
        severity: row.get("severity")?,
        file: row.get("file")?,
        line: row.get("line")?,
        claim: row.get("claim")?,
        evidence: row.get("evidence")?,
        disposition: row.get("disposition")?,
        disposition_reason: row.get("disposition_reason")?,
        created_at: row.get("created_at")?,
    })
}
