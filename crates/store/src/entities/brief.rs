//! Briefs: one per review, what the review understood before reading the diff.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{Result, StoreError};
use crate::model::{now, Brief, NewBrief};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, problem, diagram, option_map, approach_verdict, \
                       confirmed_at, updated_at";

impl Store {
    /// Store the review's brief, replacing any brief already held.
    ///
    /// Replacing clears the confirmation: the developer confirmed the text that was
    /// there, not whatever replaces it.
    pub fn put_brief(&self, review_id: i64, new: NewBrief) -> Result<Brief> {
        let updated_at = now();
        self.conn
            .execute(
                "INSERT INTO briefs
                     (review_id, problem, diagram, option_map, approach_verdict,
                      confirmed_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
                 ON CONFLICT (review_id) DO UPDATE SET
                     problem          = excluded.problem,
                     diagram          = excluded.diagram,
                     option_map       = excluded.option_map,
                     approach_verdict = excluded.approach_verdict,
                     confirmed_at     = NULL,
                     updated_at       = excluded.updated_at",
                params![
                    review_id,
                    new.problem,
                    new.diagram,
                    new.option_map,
                    new.approach_verdict,
                    updated_at,
                ],
            )
            .map_err(|error| unknown_review(error, review_id))?;

        self.get_brief(review_id)
    }

    /// Mark the review's brief confirmed by the developer, if it is not already.
    ///
    /// Confirming twice keeps the first confirmation time.
    pub fn confirm_brief(&self, review_id: i64) -> Result<Brief> {
        let brief = self.get_brief(review_id)?;
        if brief.is_confirmed() {
            return Ok(brief);
        }

        self.conn.execute(
            "UPDATE briefs SET confirmed_at = ?2 WHERE review_id = ?1",
            params![review_id, now()],
        )?;

        self.get_brief(review_id)
    }

    /// Read the review's brief. Fails with [`StoreError::NotFound`] if there is none.
    pub fn get_brief(&self, review_id: i64) -> Result<Brief> {
        self.find_brief(review_id)?.ok_or(StoreError::NotFound {
            entity: "brief",
            id: review_id.to_string(),
        })
    }

    /// Read the review's brief, or `None`.
    pub fn find_brief(&self, review_id: i64) -> Result<Option<Brief>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM briefs WHERE review_id = ?1"),
                params![review_id],
                row_to_brief,
            )
            .optional()?)
    }
}

/// A brief whose review is absent is a named failure, not a raw SQLite one.
fn unknown_review(error: rusqlite::Error, review_id: i64) -> StoreError {
    if is_foreign_key_violation(&error) {
        StoreError::UnknownParent {
            entity: "brief",
            parent: "review",
            id: review_id.to_string(),
        }
    } else {
        StoreError::Sqlite(error)
    }
}

/// Build a brief from a row selecting [`COLUMNS`].
fn row_to_brief(row: &Row<'_>) -> rusqlite::Result<Brief> {
    Ok(Brief {
        review_id: row.get("review_id")?,
        problem: row.get("problem")?,
        diagram: row.get("diagram")?,
        option_map: row.get("option_map")?,
        approach_verdict: row.get("approach_verdict")?,
        confirmed_at: row.get("confirmed_at")?,
        updated_at: row.get("updated_at")?,
    })
}
