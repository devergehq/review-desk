//! A review's seven layers, what each one is doing, and how long each has taken.
//!
//! One row per layer that has been written, and a read that completes the set. The
//! state is stored rather than derived: most of it looks derivable — layer 2 from the
//! brief's confirmation, layer 5 from the finding count — but layer 1's lint result has
//! nothing to derive from, and a derived state cannot tell a layer that was *skipped*
//! from one that has not started, which is the distinction "what was not checked"
//! exists to make.
//!
//! **Beside that row is an append-only record of every state the layer has entered**, and
//! that is DEV-890. The row above holds one `updated_at` and overwrites it, so until
//! migration 12 a layer's start was lost the moment it finished and Review Desk could not
//! say how long a review took. The passes a layer has made are derived from that record
//! and never stored: there is one copy of when each state was entered, so a total and the
//! times it was computed from cannot come to disagree.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    LayerFact, NewLayerFact, ReviewLayer, ReviewLayerName, ReviewLayerRun, ReviewLayerState,
    Timestamp,
};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, number, state, detail, updated_at";

impl Store {
    /// Record what one of a review's layers is doing, replacing what it said before.
    ///
    /// The detail is the short phrase the screen shows after the layer's name —
    /// "passed", "3 options", "6 findings". Writing the same state twice is the same as
    /// writing it once, except that the time moves, so a layer can report progress
    /// without the caller checking first.
    ///
    /// **A state that has changed is also appended to the layer's own record**, which is
    /// what lets a step that ran twice report both runs. A state that has not changed is
    /// not: see the comment on the insert.
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

        // One reading of the clock for both writes. Two would put the row's `updated_at`
        // and the state's `entered_at` a tick apart, and every duration measured from
        // either would then depend on which one a later reader picked.
        let at = self.now();

        self.conn
            .execute(
                "INSERT INTO review_layers (review_id, number, state, detail, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (review_id, number) DO UPDATE SET
                     state      = excluded.state,
                     detail     = excluded.detail,
                     updated_at = excluded.updated_at",
                params![review_id, number, state, detail, at],
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

        // **Appended only when the state has changed**, which is what keeps a pass a
        // pass. Writing the same state again is a supported call — it is how a layer
        // reports progress without checking first — and a row for each of those would
        // turn one pass that is still going into ten passes of a few seconds. Whatever
        // the caller said about the detail still lands, on the row above.
        //
        // **The test and the insert are one statement**, rather than a read and then a
        // write. `review-desk layer set` calls this with no transaction around it, one
        // process per invocation, so two sessions reporting the same step at the same
        // moment would otherwise both read the old state and both append — a pass of no
        // length that every total would then count. SQLite evaluates this as one write.
        //
        // The state tested is the newest one *in this record*, not the layer row's: they
        // agree by construction, and reading the record it is about leaves one source.
        self.conn.execute(
            "INSERT INTO review_layer_states (review_id, number, state, detail, entered_at)
             SELECT ?1, ?2, ?3, ?4, ?5
             WHERE NOT EXISTS (
                 SELECT 1 FROM review_layer_states newest
                 WHERE newest.review_id = ?1
                   AND newest.number = ?2
                   AND newest.state = ?3
                   AND newest.id = (
                       SELECT MAX(id) FROM review_layer_states
                       WHERE review_id = ?1 AND number = ?2
                   )
             )",
            params![review_id, number, state, detail, at],
        )?;

        // Plain statements, in whatever transaction the caller opened, for the reason
        // `layer_trail.rs` gives about the sign-offs: a layer write that is rolled back
        // has to take its state record and its activity with it.
        self.mark_review_active(review_id)?;

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
                runs: self.review_layer_runs(review_id, layer.number)?,
                facts: self.list_review_layer_facts(review_id, layer.number)?,
                ..layer
            })),
            None => Ok(None),
        }
    }

    /// Every pass one layer has made, oldest first.
    ///
    /// **Derived from the append-only record of its state changes, never stored.** A pass
    /// is one continuous occupancy of one working state, so the rule is one line per
    /// recorded state:
    ///
    /// | The state entered | What it does to the passes |
    /// |---|---|
    /// | `running`, `waiting_on_you` | closes the pass that was open, and opens a new one |
    /// | `done`, `failed`, `skipped` | closes the pass that was open — or, with none open, records a finish whose start nothing knows |
    /// | `pending` | closes the pass that was open and opens nothing: the layer is back to not started |
    ///
    /// The second half of the middle row is the whole of DEV-890's rule about reviews
    /// recorded before it. Migration 12 backfilled one state per layer from `updated_at`,
    /// which is the only time such a review holds, so a layer it had already finished
    /// comes out as a finish with no start — and every figure that would need that start
    /// reads as not recorded rather than as zero.
    pub fn review_layer_runs(&self, review_id: i64, number: i64) -> Result<Vec<ReviewLayerRun>> {
        let mut statement = self.conn.prepare(
            "SELECT state, detail, entered_at FROM review_layer_states
             WHERE review_id = ?1 AND number = ?2
             ORDER BY id ASC",
        )?;
        let changes = statement
            .query_map(params![review_id, number], |row| {
                Ok((
                    row.get::<_, ReviewLayerState>("state")?,
                    row.get::<_, Option<String>>("detail")?,
                    row.get::<_, Timestamp>("entered_at")?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(runs_from(&changes))
    }

    /// Record the facts one step came to, replacing whatever it recorded before.
    ///
    /// **The list is the statement**, which is the bargain every other ordered list on
    /// this record strikes — the sources, the criteria, the options. So the position is
    /// the store's to assign and no caller numbers anything, and an empty list is how a
    /// step takes its facts back off the record.
    ///
    /// **Separate from [`Store::set_review_layer`], and that is the point.** Setting a
    /// layer is the call a session makes to report progress, over and over, with whatever
    /// it knows at the time; facts that were cleared by a call that said nothing about
    /// them would be facts no session could keep. A step's facts change when the step has
    /// something new to say about them and not before.
    ///
    /// Refused: a pair with an empty label or an empty value, for the reason
    /// `put_review_sources` refuses a blank label — a field nobody filled in wearing the
    /// clothes of one somebody did. No length is refused; a label that runs long is a
    /// writing fault and belongs to the linter in Locus.
    pub fn put_review_layer_facts(
        &self,
        review_id: i64,
        number: i64,
        facts: Vec<NewLayerFact>,
    ) -> Result<Vec<LayerFact>> {
        // Checked before the write, so an eighth layer is refused with the valid range
        // named rather than caught by a foreign key and reported as a breakage.
        ReviewLayerName::for_number(number)?;

        let facts = facts
            .into_iter()
            .enumerate()
            .map(|(index, fact)| {
                Ok((
                    index as i64 + 1,
                    NewLayerFact {
                        label: required(&fact.label, "facts.label", "what this fact is called")?,
                        value: required(&fact.value, "facts.value", "what the fact says")?,
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // **The parent is the step, not the review**, and it is checked here so the
        // refusal names it. Facts about a step nothing has written are facts about
        // nothing, and the delete below would otherwise report success for one.
        if self.find_review_layer(review_id, number)?.is_none() {
            return Err(StoreError::UnknownParent {
                entity: "layer fact",
                parent: "review layer",
                id: format!("{review_id}/{number}"),
            });
        }

        self.conn.execute(
            "DELETE FROM review_layer_facts WHERE review_id = ?1 AND number = ?2",
            params![review_id, number],
        )?;
        for (position, fact) in &facts {
            self.conn.execute(
                "INSERT INTO review_layer_facts (review_id, number, position, label, value)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![review_id, number, position, fact.label, fact.value],
            )?;
        }

        self.mark_review_active(review_id)?;
        tx.commit()?;
        self.list_review_layer_facts(review_id, number)
    }

    /// The facts one step recorded, in the order they were written.
    pub fn list_review_layer_facts(&self, review_id: i64, number: i64) -> Result<Vec<LayerFact>> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, number, position, label, value FROM review_layer_facts
             WHERE review_id = ?1 AND number = ?2 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id, number], |row| {
            Ok(LayerFact {
                review_id: row.get("review_id")?,
                number: row.get("number")?,
                position: row.get("position")?,
                label: row.get("label")?,
                value: row.get("value")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// All seven layers of a review, in order.
    ///
    /// **A layer nothing has written comes back as `pending` with no time**, rather
    /// than being left out. A reader always has seven, which is what the five screens
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
                    runs: self.review_layer_runs(review_id, layer.number)?,
                    facts: self.list_review_layer_facts(review_id, layer.number)?,
                    ..layer.clone()
                }),
                None => layers.push(ReviewLayer {
                    review_id,
                    number,
                    name,
                    state: ReviewLayerState::Pending,
                    detail: None,
                    updated_at: None,
                    runs: Vec::new(),
                    facts: Vec::new(),
                }),
            }
        }

        Ok(layers)
    }
}

/// Build a layer from a row selecting [`COLUMNS`].
///
/// The name is a placeholder here and is set by the function that read the row, and so
/// are the passes. A row builder cannot fail with a store error, and both turning a
/// number into a name and reading a layer's state record can — a number outside 1 to 7 is
/// a row this build does not understand.
fn row_to_layer(row: &Row<'_>) -> rusqlite::Result<ReviewLayer> {
    Ok(ReviewLayer {
        review_id: row.get("review_id")?,
        number: row.get("number")?,
        name: ReviewLayerName::Lint,
        state: row.get("state")?,
        detail: row.get("detail")?,
        updated_at: row.get("updated_at")?,
        runs: Vec::new(),
        facts: Vec::new(),
    })
}

/// Pair a layer's recorded states into the passes it has made.
///
/// A free function over a list, rather than a method over a query, so the rule can be
/// tested without a database: the table in [`Store::review_layer_runs`] is the whole of
/// it, and the pairing is what is easy to get wrong rather than the SQL.
fn runs_from(changes: &[(ReviewLayerState, Option<String>, Timestamp)]) -> Vec<ReviewLayerRun> {
    let mut runs: Vec<ReviewLayerRun> = Vec::new();
    let mut open: Option<ReviewLayerRun> = None;

    for (state, detail, at) in changes {
        if let Some(mut going) = open.take() {
            going.ended_as = Some(*state);
            going.finished_at = Some(*at);
            runs.push(going);
        } else if state.is_finished() {
            // A layer finished with nothing before it. The finish is real and the start
            // was never recorded, which is what a review from before migration 12 knows
            // about a layer it had already finished.
            runs.push(ReviewLayerRun {
                state: None,
                started_at: None,
                ended_as: Some(*state),
                finished_at: Some(*at),
                detail: None,
            });
        }

        if state.is_being_worked() {
            open = Some(ReviewLayerRun {
                state: Some(*state),
                started_at: Some(*at),
                ended_as: None,
                finished_at: None,
                detail: detail.clone(),
            });
        }
    }

    runs.extend(open);
    runs
}

/// A required string, trimmed, or a refusal naming the field.
///
/// The same rule `provenance.rs` applies to a source's label, stated again here rather
/// than shared: these two modules are the two halves of DEV-917's record and a helper
/// moved to `model.rs` for two callers would be a third place to read before either makes
/// sense.
fn required(value: &str, field: &'static str, expected: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(StoreError::InvalidValue {
            field,
            value: value.to_string(),
            expected: expected.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReviewLayerState::{Done, Pending, Running, WaitingOnYou};

    /// A layer's recorded states, with times short enough to read on one line.
    fn changes(
        states: &[(ReviewLayerState, Timestamp)],
    ) -> Vec<(ReviewLayerState, Option<String>, Timestamp)> {
        states
            .iter()
            .map(|(state, at)| (*state, None, *at))
            .collect()
    }

    #[test]
    fn a_layer_worked_once_and_finished_is_one_measured_pass() {
        let runs = runs_from(&changes(&[(Running, 100), (Done, 160)]));

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].state, Some(Running));
        assert_eq!(runs[0].seconds(), Some(60));
        assert_eq!(runs[0].ended_as, Some(Done));
    }

    #[test]
    fn a_draft_sent_back_and_redrafted_is_two_passes_at_running() {
        // The sequence DEV-890 names: the agent drafts, the developer is waited on, the
        // draft comes back, the agent drafts again, and the developer signs it off.
        let runs = runs_from(&changes(&[
            (Running, 0),
            (WaitingOnYou, 60),
            (Running, 300),
            (WaitingOnYou, 420),
            (Done, 600),
        ]));

        let working: Vec<i64> = runs
            .iter()
            .filter(|run| run.state == Some(Running))
            .filter_map(ReviewLayerRun::seconds)
            .collect();
        assert_eq!(working, vec![60, 120], "both passes, not just the last");

        let waiting: Vec<i64> = runs
            .iter()
            .filter(|run| run.is_waiting_on_you())
            .filter_map(ReviewLayerRun::seconds)
            .collect();
        assert_eq!(
            waiting,
            vec![240, 180],
            "and both of the developer's own waits"
        );
    }

    #[test]
    fn a_layer_finished_before_anything_recorded_a_start_has_no_start() {
        // What migration 12 backfilled for a layer a review had already finished: one
        // state, carrying `updated_at`. The finish is real; nothing knows when it began.
        let runs = runs_from(&changes(&[(Done, 1_760_000_000)]));

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].started_at, None);
        assert_eq!(runs[0].finished_at, Some(1_760_000_000));
        assert_eq!(
            runs[0].seconds(),
            None,
            "not zero: a figure the record cannot answer is absent"
        );
    }

    #[test]
    fn a_layer_put_back_to_pending_closes_its_pass_and_opens_none() {
        let runs = runs_from(&changes(&[(Running, 0), (Pending, 50)]));

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].seconds(), Some(50));
        assert!(!runs[0].is_open(), "the pass ended when the layer did");
    }

    #[test]
    fn the_pass_going_on_now_has_no_finish_and_no_length() {
        let runs = runs_from(&changes(&[(Running, 0), (WaitingOnYou, 60)]));

        assert_eq!(runs.len(), 2);
        assert!(runs[1].is_open());
        assert_eq!(runs[1].seconds(), None);
    }
}
