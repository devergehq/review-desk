//! Briefs: one per review, what the review understood before reading the diff.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::entities::only_the_developer;
use crate::error::{Result, StoreError};
use crate::model::{now, Actor, Brief, BriefOption, NewBrief, ReviewLayerName, ReviewLayerState};
use crate::{is_foreign_key_violation, Store};

/// What layer 2 says once the developer has confirmed the brief.
///
/// The *when* is the layer row's own `updated_at`; this is the *who*. Only the developer
/// can confirm a brief, so there is one value it can take — and the row saying so is what
/// makes the trail readable without joining it back to the brief.
pub(crate) const CONFIRMED_DETAIL: &str = "confirmed by the developer";

const COLUMNS: &str = "review_id, problem, diagram, option_map, approach_verdict, \
                       provenance, confirmed_at, confirmed_by, confirmation_required, \
                       confirmation_not_required_because, updated_at";

impl Store {
    /// Store the review's brief, replacing any brief already held.
    ///
    /// Replacing clears the confirmation: the developer confirmed the text that was
    /// there, not whatever replaces it. The options replace the options already held,
    /// for the same reason — a brief whose problem changed under half of its old
    /// option cards is not a brief anybody wrote.
    ///
    /// # Confirmation
    ///
    /// `confirmation_required` defaults to true. Set false, with a reason, it records
    /// that the caller decided the developer's act at layer 2 was unnecessary — a small
    /// pull request, under a threshold the caller owns and Review Desk does not hold.
    /// Layer 2 is then set `done` in this same transaction, carrying the reason, because
    /// the brief's write is what settles it and a trail that still said "waiting on you"
    /// would be describing a wait nobody is in.
    ///
    /// **A brief already written as requiring confirmation cannot be rewritten as needing
    /// none.** That is the whole of the protection: the flag is safe to have because it
    /// can only ever be set before the developer was asked, not after. The other
    /// direction is allowed — asking for a confirmation that was not needed costs the
    /// developer a click and the record nothing — and it puts layer 2 back to
    /// `waiting_on_you`, undoing the only layer row this function ever wrote.
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

        let because = new
            .confirmation_not_required_because
            .as_deref()
            .map(str::trim)
            .filter(|because| !because.is_empty())
            .map(str::to_string);

        // Required when confirmation is not, and refused when it is. A brief that needed
        // no confirmation with nothing saying why would put layer 2 at `done` with no
        // record of who decided that or on what grounds, and the reason is the only thing
        // standing between this flag and an agent quietly excusing itself.
        match (new.confirmation_required, &because) {
            (false, None) => {
                return Err(StoreError::InvalidValue {
                    field: "confirmation_not_required_because",
                    value: String::new(),
                    expected: "why no confirmation is needed, such as the change being \
                               under the caller's own threshold"
                        .to_string(),
                })
            }
            (true, Some(because)) => {
                return Err(StoreError::InvalidValue {
                    field: "confirmation_not_required_because",
                    value: because.clone(),
                    expected: "nothing; this brief does require confirmation".to_string(),
                })
            }
            _ => {}
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

        // **Read inside the transaction, because this is a read-then-write rule.** Two
        // callers racing on one review would otherwise both see a brief that required
        // confirmation, both be allowed to proceed, and the second write would win —
        // which is the flip this refuses, arrived at by two legal-looking calls.
        let held = self.find_brief(review_id)?;
        if let Some(held) = &held {
            if held.confirmation_required && !new.confirmation_required {
                return Err(StoreError::InvalidValue {
                    field: "confirmation_required",
                    value: "false".to_string(),
                    expected: "true; this brief was written as requiring the developer's \
                               confirmation, and a confirmation already asked for cannot \
                               be withdrawn by rewriting the brief"
                        .to_string(),
                });
            }
        }

        let updated_at = now();
        self.conn
            .execute(
                "INSERT INTO briefs
                     (review_id, problem, diagram, option_map, approach_verdict,
                      provenance, confirmed_at, confirmed_by, confirmation_required,
                      confirmation_not_required_because, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?8, ?9)
                 ON CONFLICT (review_id) DO UPDATE SET
                     problem          = excluded.problem,
                     diagram          = excluded.diagram,
                     option_map       = excluded.option_map,
                     approach_verdict = excluded.approach_verdict,
                     provenance       = excluded.provenance,
                     confirmed_at     = NULL,
                     confirmed_by     = NULL,
                     confirmation_required = excluded.confirmation_required,
                     confirmation_not_required_because
                         = excluded.confirmation_not_required_because,
                     updated_at       = excluded.updated_at",
                params![
                    review_id,
                    new.problem,
                    new.diagram,
                    new.option_map,
                    new.approach_verdict,
                    new.provenance,
                    new.confirmation_required,
                    because,
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

        // **Layer 2 in the same transaction as the brief**, for the reason the options
        // are: a trail that disagrees with the record is worse than a trail that is
        // merely behind. Only these two transitions are written, and they are the two
        // this function causes:
        //
        // * recorded as needing no confirmation — layer 2 is finished with on write;
        // * recorded as needing one again, having needed none — the `done` above was
        //   this function's own, and leaving it would claim layer 2 was settled while
        //   the developer is being asked.
        //
        // An ordinary brief, required before and required now, touches no layer: what a
        // session's layers say about its own progress is the session's to write.
        let settled_before = held
            .as_ref()
            .is_some_and(|held| !held.confirmation_required);
        match (&because, settled_before) {
            (Some(because), _) => {
                self.set_review_layer(
                    review_id,
                    ReviewLayerName::Understand.number(),
                    ReviewLayerState::Done,
                    Some(&format!("confirmation not required: {because}")),
                )?;
            }
            (None, true) => {
                self.set_review_layer(
                    review_id,
                    ReviewLayerName::Understand.number(),
                    ReviewLayerState::WaitingOnYou,
                    Some("waiting on you"),
                )?;
            }
            (None, false) => {}
        }

        tx.commit()?;
        self.get_brief(review_id)
    }

    /// Mark the review's brief confirmed, if it is not already, and finish layer 2.
    ///
    /// Confirming twice keeps the first confirmation time and the first actor.
    ///
    /// `by` is required and must be [`Actor::Developer`]. **Confirming a brief is the
    /// developer's act** — decided on 9 October 2026 — and it is what releases the
    /// later layers, so the record has to say whose assent released them. There is no
    /// command-line command for this: it happens in the dashboard.
    ///
    /// **The confirmation and layer 2 are written in one transaction.** Until DEV-864
    /// this function wrote the confirmation and nothing else, and layer rows were the
    /// command line's to write — so the One review screen read "2 Understand · waiting
    /// on you" underneath a brief the developer had confirmed minutes earlier. The
    /// screen was faithful; the record was wrong. Two writes that can disagree are two
    /// writes that eventually will, so they are now one.
    ///
    /// Layer 2 is written on a repeat confirmation too, which is deliberate: a brief
    /// confirmed before this behaviour existed has a trail that still says it is waiting,
    /// and confirming again is the only call that can put it right.
    pub fn confirm_brief(&self, review_id: i64, by: Actor) -> Result<Brief> {
        only_the_developer(by, "confirm a brief")?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let brief = self.get_brief(review_id)?;

        if !brief.is_confirmed() {
            self.conn.execute(
                "UPDATE briefs SET confirmed_at = ?2, confirmed_by = ?3 WHERE review_id = ?1",
                params![review_id, now(), by],
            )?;
        }

        self.set_review_layer(
            review_id,
            ReviewLayerName::Understand.number(),
            ReviewLayerState::Done,
            Some(CONFIRMED_DETAIL),
        )?;

        tx.commit()?;
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
        confirmation_required: row.get("confirmation_required")?,
        confirmation_not_required_because: row.get("confirmation_not_required_because")?,
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
