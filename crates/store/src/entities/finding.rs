//! Findings: what a review found, each with a sequence number stable within its review.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{now, Disposition, Finding, NewFinding};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, seq, lens, severity, file, line, claim, evidence, \
                       disposition, disposition_reason, rule_id, github_comment_id, \
                       created_at";

impl Store {
    /// Add a finding to a review, assigning the next sequence number within it.
    ///
    /// Fails with [`StoreError::UnknownParent`] if the review does not exist; the
    /// database enforces that, not this function.
    ///
    /// A finding that names the rule it was raised under also moves that rule's
    /// `last_cited_at`. **That column is stored rather than scanned for** — the choice
    /// S12 asks for — and it cannot drift, because a finding is never deleted, only
    /// dispositioned, so the newest citation only ever moves forward.
    pub fn add_finding(&self, review_id: i64, new: NewFinding) -> Result<Finding> {
        // The read that picks the sequence number and the insert that claims it must
        // be one step. A DEFERRED transaction would take a read lock and then try to
        // upgrade it on the insert, and SQLite refuses that upgrade immediately with
        // "database is locked" rather than waiting out the busy timeout — it cannot
        // wait without risking deadlock. IMMEDIATE takes the write lock up front, so a
        // second caller queues on the busy handler instead of failing.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // Checked inside the transaction, so a rule that is not there is named as the
        // missing parent it is. The foreign key would refuse the row too, but it
        // arrives as one violation among several and the message would blame the
        // review.
        if let Some(rule_id) = new.rule_id {
            if self.find_rule(rule_id)?.is_none() {
                return Err(StoreError::UnknownParent {
                    entity: "finding",
                    parent: "rule",
                    id: rule_id.to_string(),
                });
            }
        }

        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM findings WHERE review_id = ?1",
            params![review_id],
            |row| row.get(0),
        )?;

        let created_at = now();
        tx.execute(
            "INSERT INTO findings
                 (review_id, seq, lens, severity, file, line, claim, evidence,
                  disposition, disposition_reason, rule_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                new.rule_id,
                created_at,
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

        if let Some(rule_id) = new.rule_id {
            // MAX, not an assignment: findings can be added in any order, and a rule's
            // last citation is the newest one it has, not the newest one written.
            tx.execute(
                "UPDATE rules
                 SET last_cited_at = MAX(COALESCE(last_cited_at, 0), ?2),
                     updated_at    = ?2
                 WHERE id = ?1",
                params![rule_id, created_at],
            )?;
        }

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

    /// Record the GitHub review comment a finding was posted as.
    ///
    /// Linking the same finding to the same comment again changes nothing, so the
    /// command can be re-run. Linking a comment that another finding already holds is
    /// refused: a comment belongs to one finding, and the harvester would otherwise
    /// record the same reply against two of them.
    pub fn set_finding_github_comment(
        &self,
        review_id: i64,
        seq: i64,
        comment_id: &str,
    ) -> Result<Finding> {
        let comment_id = comment_id.trim();
        if comment_id.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "github_comment_id",
                value: comment_id.to_string(),
                expected: "the id of the GitHub review comment the finding was posted as"
                    .to_string(),
            });
        }

        // Checked first, so an unknown finding is NotFound rather than a no-op update.
        self.get_finding(review_id, seq)?;

        if let Some(holder) = self.find_finding_by_github_comment(comment_id)? {
            if (holder.review_id, holder.seq) != (review_id, seq) {
                return Err(StoreError::DuplicateGithubComment {
                    comment_id: comment_id.to_string(),
                    review_id: holder.review_id,
                    seq: holder.seq,
                });
            }
        }

        self.conn.execute(
            "UPDATE findings SET github_comment_id = ?3 WHERE review_id = ?1 AND seq = ?2",
            params![review_id, seq, comment_id],
        )?;

        self.get_finding(review_id, seq)
    }

    /// The finding posted as this GitHub review comment, if any.
    pub fn find_finding_by_github_comment(&self, comment_id: &str) -> Result<Option<Finding>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM findings WHERE github_comment_id = ?1"),
                params![comment_id],
                row_to_finding,
            )
            .optional()?)
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
        rule_id: row.get("rule_id")?,
        github_comment_id: row.get("github_comment_id")?,
        created_at: row.get("created_at")?,
    })
}
