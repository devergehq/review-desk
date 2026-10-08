//! Briefs: one per review, what the review understood before reading the diff.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::entities::only_the_developer;
use crate::error::{Result, StoreError};
use crate::model::{now, Actor, Brief, BriefOption, NewBrief};
use crate::{is_foreign_key_violation, Store};

const COLUMNS: &str = "review_id, problem, diagram, option_map, approach_verdict, \
                       provenance, confirmed_at, confirmed_by, updated_at";

impl Store {
    /// Store the review's brief, replacing any brief already held.
    ///
    /// Replacing clears the confirmation: the developer confirmed the text that was
    /// there, not whatever replaces it. The options replace the options already held,
    /// for the same reason — a brief whose problem changed under half of its old
    /// option cards is not a brief anybody wrote.
    pub fn put_brief(&self, review_id: i64, new: NewBrief) -> Result<Brief> {
        // `problem` is required by serde, but an empty string satisfies that. A brief
        // with no problem in it can still be confirmed, and then the record holds a
        // developer's assent to nothing — which is worse than having no brief at all.
        if new.problem.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: "problem",
                value: new.problem,
                expected: "what problem the change is solving".to_string(),
            });
        }

        // Two spellings of one fact. `option_map` is the superseded form, kept so that
        // migration 4 destroys nothing a review already wrote; a caller that sends both
        // is refused rather than having one of them silently win.
        if new.option_map.is_some() && !new.options.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "option_map",
                value: new.option_map.unwrap_or_default(),
                expected: "either options or the superseded option_map, not both".to_string(),
            });
        }

        for option in &new.options {
            validate_option(option)?;
        }
        if let Some(duplicate) = first_duplicate_key(&new.options) {
            return Err(StoreError::InvalidValue {
                field: "options",
                value: duplicate,
                expected: "one option per key".to_string(),
            });
        }

        // The brief and its options are one record, so they are written in one step. A
        // brief stored with the previous round's option cards still attached would be a
        // worse record than no brief at all.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        let updated_at = now();
        self.conn
            .execute(
                "INSERT INTO briefs
                     (review_id, problem, diagram, option_map, approach_verdict,
                      provenance, confirmed_at, confirmed_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7)
                 ON CONFLICT (review_id) DO UPDATE SET
                     problem          = excluded.problem,
                     diagram          = excluded.diagram,
                     option_map       = excluded.option_map,
                     approach_verdict = excluded.approach_verdict,
                     provenance       = excluded.provenance,
                     confirmed_at     = NULL,
                     confirmed_by     = NULL,
                     updated_at       = excluded.updated_at",
                params![
                    review_id,
                    new.problem,
                    new.diagram,
                    new.option_map,
                    new.approach_verdict,
                    new.provenance,
                    updated_at,
                ],
            )
            .map_err(|error| unknown_review(error, review_id))?;

        self.conn.execute(
            "DELETE FROM brief_options WHERE review_id = ?1",
            params![review_id],
        )?;
        for option in &new.options {
            self.conn.execute(
                "INSERT INTO brief_options
                     (review_id, key, title, argument_for, argument_against, chosen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    review_id,
                    option.key,
                    option.title,
                    option.argument_for,
                    option.argument_against,
                    option.chosen,
                ],
            )?;
        }

        tx.commit()?;
        self.get_brief(review_id)
    }

    /// Mark the review's brief confirmed, if it is not already.
    ///
    /// Confirming twice keeps the first confirmation time and the first actor.
    ///
    /// `by` is required and must be [`Actor::Developer`]. **Confirming a brief is the
    /// developer's act** — decided on 9 October 2026 — and it is what releases the
    /// later layers, so the record has to say whose assent released them. There is no
    /// command-line command for this: it happens in the dashboard.
    pub fn confirm_brief(&self, review_id: i64, by: Actor) -> Result<Brief> {
        only_the_developer(by, "confirm a brief")?;

        let brief = self.get_brief(review_id)?;
        if brief.is_confirmed() {
            return Ok(brief);
        }

        self.conn.execute(
            "UPDATE briefs SET confirmed_at = ?2, confirmed_by = ?3 WHERE review_id = ?1",
            params![review_id, now(), by],
        )?;

        self.get_brief(review_id)
    }

    /// The alternatives the review considered, in key order.
    pub fn list_brief_options(&self, review_id: i64) -> Result<Vec<BriefOption>> {
        let mut statement = self.conn.prepare(
            "SELECT key, title, argument_for, argument_against, chosen
             FROM brief_options WHERE review_id = ?1 ORDER BY key ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(BriefOption {
                key: row.get("key")?,
                title: row.get("title")?,
                argument_for: row.get("argument_for")?,
                argument_against: row.get("argument_against")?,
                chosen: row.get("chosen")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Read the review's brief. Fails with [`StoreError::NotFound`] if there is none.
    pub fn get_brief(&self, review_id: i64) -> Result<Brief> {
        self.find_brief(review_id)?.ok_or(StoreError::NotFound {
            entity: "brief",
            id: review_id.to_string(),
        })
    }

    /// Read the review's brief, with its options, or `None`.
    pub fn find_brief(&self, review_id: i64) -> Result<Option<Brief>> {
        let held = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM briefs WHERE review_id = ?1"),
                params![review_id],
                row_to_brief,
            )
            .optional()?;

        match held {
            Some(brief) => Ok(Some(Brief {
                options: self.list_brief_options(review_id)?,
                ..brief
            })),
            None => Ok(None),
        }
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
        // Read separately, by the caller that builds the brief: a row builder has no
        // connection to read them through.
        options: Vec::new(),
        approach_verdict: row.get("approach_verdict")?,
        provenance: row.get("provenance")?,
        confirmed_at: row.get("confirmed_at")?,
        confirmed_by: row.get("confirmed_by")?,
        updated_at: row.get("updated_at")?,
    })
}

/// An option with an empty field is one of the cards on the One review screen with a
/// blank on it. The database refuses it too; this names which field it was.
fn validate_option(option: &BriefOption) -> Result<()> {
    for (field, value) in [
        ("key", &option.key),
        ("title", &option.title),
        ("argument_for", &option.argument_for),
        ("argument_against", &option.argument_against),
    ] {
        if value.trim().is_empty() {
            return Err(StoreError::InvalidValue {
                field: match field {
                    "key" => "options.key",
                    "title" => "options.title",
                    "argument_for" => "options.argument_for",
                    _ => "options.argument_against",
                },
                value: value.clone(),
                expected: "an option names itself and argues both ways".to_string(),
            });
        }
    }
    Ok(())
}

/// The first key given twice, if any.
///
/// The table's primary key would refuse the second row, but as a breakage rather than
/// as the refusal it is: two options under one letter means nothing could say which
/// one "B" is on the screen.
fn first_duplicate_key(options: &[BriefOption]) -> Option<String> {
    let mut seen: Vec<&str> = Vec::with_capacity(options.len());
    for option in options {
        if seen.contains(&option.key.as_str()) {
            return Some(option.key.clone());
        }
        seen.push(&option.key);
    }
    None
}
