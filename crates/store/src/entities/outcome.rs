//! Outcomes: what became of a finding, and who brought it about.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{Result, StoreError};
use crate::model::{now, FindingRef, FullReview, NewOutcome, Outcome};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "id, review_id, finding_seq, kind, actor, occurred_at, detail, \
                       harvest_key";

impl Store {
    /// Record an outcome against a finding.
    ///
    /// Fails with [`StoreError::UnknownParent`] if the finding does not exist.
    pub fn add_outcome(&self, finding: FindingRef, new: NewOutcome) -> Result<Outcome> {
        self.conn
            .execute(
                "INSERT INTO outcomes
                     (review_id, finding_seq, kind, actor, occurred_at, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    finding.review_id,
                    finding.seq,
                    new.kind,
                    new.actor,
                    new.occurred_at.unwrap_or_else(now),
                    new.detail,
                ],
            )
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    StoreError::UnknownParent {
                        entity: "outcome",
                        parent: "finding",
                        id: format!("{}/{}", finding.review_id, finding.seq),
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        self.get_outcome(self.conn.last_insert_rowid())
    }

    /// Record an outcome the harvester read off GitHub, keyed on the fact it came
    /// from, and do nothing if that fact has already been recorded.
    ///
    /// `Ok(None)` means the row was already there. The database decides that, through
    /// the unique index on (review, finding, kind, key) — not this function reading
    /// first and writing second, which two harvests running at once could both pass.
    ///
    /// This is separate from [`Store::add_outcome`] rather than a field on
    /// [`NewOutcome`] so that a caller recording an outcome by hand cannot set a key,
    /// and so that adding harvesting did not change the shape every other caller
    /// already builds.
    pub fn add_harvested_outcome(
        &self,
        finding: FindingRef,
        new: NewOutcome,
        harvest_key: &str,
    ) -> Result<Option<Outcome>> {
        let changed = self
            .conn
            .execute(
                "INSERT INTO outcomes
                     (review_id, finding_seq, kind, actor, occurred_at, detail, harvest_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (review_id, finding_seq, kind, harvest_key)
                     WHERE harvest_key IS NOT NULL
                 DO NOTHING",
                params![
                    finding.review_id,
                    finding.seq,
                    new.kind,
                    new.actor,
                    new.occurred_at.unwrap_or_else(now),
                    new.detail,
                    harvest_key,
                ],
            )
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    StoreError::UnknownParent {
                        entity: "outcome",
                        parent: "finding",
                        id: format!("{}/{}", finding.review_id, finding.seq),
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        if changed == 0 {
            return Ok(None);
        }

        self.get_outcome(self.conn.last_insert_rowid()).map(Some)
    }

    /// Read one outcome. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_outcome(&self, id: i64) -> Result<Outcome> {
        self.find_outcome(id)?.ok_or(StoreError::NotFound {
            entity: "outcome",
            id: id.to_string(),
        })
    }

    /// Read one outcome, or `None`.
    pub fn find_outcome(&self, id: i64) -> Result<Option<Outcome>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM outcomes WHERE id = ?1"),
                params![id],
                row_to_outcome,
            )
            .optional()?)
    }

    /// Every outcome recorded against one finding, oldest first.
    pub fn list_outcomes_for_finding(&self, finding: FindingRef) -> Result<Vec<Outcome>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM outcomes
             WHERE review_id = ?1 AND finding_seq = ?2
             ORDER BY occurred_at ASC, id ASC"
        ))?;
        let rows = statement.query_map(params![finding.review_id, finding.seq], row_to_outcome)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every outcome across a whole review, by finding then time.
    pub fn list_outcomes(&self, review_id: i64) -> Result<Vec<Outcome>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM outcomes
             WHERE review_id = ?1
             ORDER BY finding_seq ASC, occurred_at ASC, id ASC"
        ))?;
        let rows = statement.query_map(params![review_id], row_to_outcome)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A whole review in one read: the review, its brief, its findings and their
    /// outcomes.
    ///
    /// The four reads share one transaction, so they see one snapshot. Without it a
    /// writer landing between them could return outcomes citing a finding the findings
    /// list does not contain.
    pub fn full_review(&self, review_id: i64) -> Result<FullReview> {
        let tx = self.conn.unchecked_transaction()?;

        let full = FullReview {
            review: self.get_review(review_id)?,
            brief: self.find_brief(review_id)?,
            findings: self.list_findings(review_id)?,
            outcomes: self.list_outcomes(review_id)?,
        };

        tx.commit()?;
        Ok(full)
    }
}

/// Build an outcome from a row selecting [`COLUMNS`].
fn row_to_outcome(row: &Row<'_>) -> rusqlite::Result<Outcome> {
    Ok(Outcome {
        id: row.get("id")?,
        review_id: row.get("review_id")?,
        finding_seq: row.get("finding_seq")?,
        kind: row.get("kind")?,
        actor: row.get("actor")?,
        occurred_at: row.get("occurred_at")?,
        detail: row.get("detail")?,
        harvest_key: row.get("harvest_key")?,
    })
}
