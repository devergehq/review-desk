//! A review's seven layers, and what each one is doing.
//!
//! One row per layer that has been written, and a read that completes the set. The
//! state is stored rather than derived: most of it looks derivable — layer 2 from the
//! brief's confirmation, layer 5 from the finding count — but layer 1's lint result has
//! nothing to derive from, and a derived state cannot tell a layer that was *skipped*
//! from one that has not started, which is the distinction "what was not checked"
//! exists to make.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{Result, StoreError};
use crate::model::{now, ReviewLayer, ReviewLayerName, ReviewLayerState};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, number, state, detail, updated_at";

impl Store {
    /// Record what one of a review's layers is doing, replacing what it said before.
    ///
    /// The detail is the short phrase the screen shows after the layer's name —
    /// "passed", "3 options", "6 findings". Writing the same state twice is the same as
    /// writing it once, except that the time moves, so a layer can report progress
    /// without the caller checking first.
    pub fn set_review_layer(
        &self,
        review_id: i64,
        number: i64,
        state: ReviewLayerState,
        detail: Option<&str>,
    ) -> Result<ReviewLayer> {
        // Checked before the write, so an eighth layer is refused with the valid range
        // named rather than caught by a CHECK constraint and reported as a breakage.
        ReviewLayerName::for_number(number)?;

        self.conn
            .execute(
                "INSERT INTO review_layers (review_id, number, state, detail, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (review_id, number) DO UPDATE SET
                     state      = excluded.state,
                     detail     = excluded.detail,
                     updated_at = excluded.updated_at",
                params![review_id, number, state, detail, now()],
            )
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    StoreError::UnknownParent {
                        entity: "review layer",
                        parent: "review",
                        id: review_id.to_string(),
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        self.get_review_layer(review_id, number)
    }

    /// Read one layer. Fails with [`StoreError::NotFound`] if nothing has written it.
    pub fn get_review_layer(&self, review_id: i64, number: i64) -> Result<ReviewLayer> {
        self.find_review_layer(review_id, number)?
            .ok_or(StoreError::NotFound {
                entity: "review layer",
                id: format!("{review_id}/{number}"),
            })
    }

    /// Read one layer, or `None` if nothing has written it.
    pub fn find_review_layer(&self, review_id: i64, number: i64) -> Result<Option<ReviewLayer>> {
        let held = self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM review_layers
                     WHERE review_id = ?1 AND number = ?2"
                ),
                params![review_id, number],
                row_to_layer,
            )
            .optional()?;

        match held {
            Some(layer) => Ok(Some(ReviewLayer {
                name: ReviewLayerName::for_number(layer.number)?,
                ..layer
            })),
            None => Ok(None),
        }
    }

    /// All seven layers of a review, in order.
    ///
    /// **A layer nothing has written comes back as `pending` with no time**, rather
    /// than being left out. A reader always has seven, which is what the four screens
    /// show, and no caller has to know that an absent row means not started.
    pub fn list_review_layers(&self, review_id: i64) -> Result<Vec<ReviewLayer>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM review_layers WHERE review_id = ?1 ORDER BY number ASC"
        ))?;
        let written = statement
            .query_map(params![review_id], row_to_layer)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut layers = Vec::with_capacity(ReviewLayerName::COUNT as usize);
        for number in 1..=ReviewLayerName::COUNT {
            let name = ReviewLayerName::for_number(number)?;
            match written.iter().find(|layer| layer.number == number) {
                Some(layer) => layers.push(ReviewLayer {
                    name,
                    ..layer.clone()
                }),
                None => layers.push(ReviewLayer {
                    review_id,
                    number,
                    name,
                    state: ReviewLayerState::Pending,
                    detail: None,
                    updated_at: None,
                }),
            }
        }

        Ok(layers)
    }
}

/// Build a layer from a row selecting [`COLUMNS`].
///
/// The name is a placeholder here and is set by the function that read the row. A row
/// builder cannot fail with a store error, and turning a number into a name can — a
/// number outside 1 to 7 is a row this build does not understand.
fn row_to_layer(row: &Row<'_>) -> rusqlite::Result<ReviewLayer> {
    Ok(ReviewLayer {
        review_id: row.get("review_id")?,
        number: row.get("number")?,
        name: ReviewLayerName::Lint,
        state: row.get("state")?,
        detail: row.get("detail")?,
        updated_at: row.get("updated_at")?,
    })
}
