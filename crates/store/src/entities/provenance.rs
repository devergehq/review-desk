//! Where a review's understanding came from: its sources, the ticket's criteria, the
//! description lint's individual results and the per-file change map.
//!
//! **All of it is written by the agent through the command line.** Review Desk reads no
//! ticket and no pull request description; what is here is what a review said it read.
//!
//! The four records share one shape and one rule, which is why they share a module. Each
//! is an ordered list belonging to one review, and each is written by **replacing the
//! whole list** — the same bargain [`crate::Store::put_brief`] strikes with its options.
//! Two consequences follow and both are the point: the position is the store's to assign,
//! so no caller manages numbering; and the whole coverage record changes in one
//! transaction, so nothing can read half of it.
//!
//! The model types live here rather than in [`crate::model`] so that this change occupies
//! its own file. Three changes to the store were in flight at once on 10 October 2026 and
//! a shared file is where they would have collided.

use rusqlite::{params, Row, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::model::string_enum;
use crate::Store;

string_enum! {
    /// What kind of thing a review read the problem from.
    ///
    /// Three values, because three are what a review has to read from: the ticket, the
    /// author's own description of the change, and what was said on the pull request
    /// before this round. The code is not among them — a review always reads the code,
    /// so saying so carries nothing.
    SourceKind as "kind" {
        /// The ticket, named by whatever the agent calls it. Review Desk does not fetch it.
        Ticket => "ticket",
        /// The pull request's description, as the author wrote it.
        Description => "description",
        /// What was said on the pull request before this round of review.
        Comment => "comment",
    }
}

string_enum! {
    /// Whether the change does what the ticket asked for.
    ///
    /// Three values and no fourth. "Partly" was considered and refused: a criterion half
    /// met is a criterion not met, and the finding says which half.
    CriterionStatus as "status" {
        /// The change does this, and the pointer says where.
        Met => "met",
        /// The change does not do this. **A finding has to say so** — see
        /// [`Store::put_acceptance_criteria`].
        NotMet => "not_met",
        /// This pull request was never going to do it, and the note says what will.
        NotInThisPr => "not_in_this_pr",
    }
}

string_enum! {
    /// How a file in the change was looked at.
    ///
    /// The free sentence beside it says what was done; this is the part a reader can
    /// count, and the part that makes an unreviewed area impossible to overlook.
    FileCheck as "checked" {
        /// Every line of the change to this file was read.
        ReadInFull => "read_in_full",
        /// Some of it was read. The sentence beside it says which part, and why that was
        /// enough.
        ReadInPart => "read_in_part",
        /// The change was executed, not only read — a test run, a script run.
        Run => "run",
        /// **Not reviewed.** The row exists so that the gap is on the record rather than
        /// absent from it, and it must carry the reason.
        NotReviewed => "not_reviewed",
    }
}

string_enum! {
    /// What one check of the pull request description reported.
    ///
    /// `warn` is here because the review-craft description lint has warnings that do not
    /// fail it, and a warning folded into `pass` is a warning nobody sees.
    DescriptionLintStatus as "status" {
        Pass => "pass",
        Warn => "warn",
        Fail => "fail",
    }
}

/// One thing the review read the problem from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSource {
    pub review_id: i64,
    /// Where it comes in the list, from 1. Assigned by the store from the order written.
    pub position: i64,
    pub kind: SourceKind,
    /// What to call it on screen — "ABC-231 · Retry webhook deliveries for 24 hours",
    /// "The pull request description", "2 earlier comments on the pull request".
    pub label: String,
    /// Where to follow it to. Opaque: Review Desk never fetches it.
    pub link: Option<String>,
}

/// A source to record. Unknown fields are refused, for the reason
/// [`crate::model::NewBrief`] gives: a misspelled `link` is a source with no link, which
/// is worse than a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewReviewSource {
    pub kind: SourceKind,
    pub label: String,
    #[serde(default)]
    pub link: Option<String>,
}

/// One thing the ticket asks for, and whether the change does it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub review_id: i64,
    /// Where it comes in the list, from 1. Assigned by the store.
    pub position: i64,
    /// What the ticket asks for, in the ticket's own terms.
    pub text: String,
    pub status: CriterionStatus,
    /// The finding that says this is not met. Required when the status is `not_met`.
    pub finding_seq: Option<i64>,
    /// The file that satisfies it.
    pub file: Option<String>,
    /// The line in that file. Never set without a file.
    pub line: Option<i64>,
    /// Anything else that answers for it — "the description says it follows in ABC-240".
    pub note: Option<String>,
}

impl AcceptanceCriterion {
    /// Whether this criterion points anywhere at all.
    ///
    /// A criterion with no pointer is a claim with nothing behind it. It is allowed for
    /// `met` and `not_in_this_pr`, where a reader can at least go and look, and refused
    /// for `not_met`.
    pub fn has_pointer(&self) -> bool {
        self.finding_seq.is_some() || self.file.is_some() || self.note.is_some()
    }
}

/// A criterion to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewAcceptanceCriterion {
    pub text: String,
    pub status: CriterionStatus,
    #[serde(default)]
    pub finding_seq: Option<i64>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
}

/// One file the change touched, or one area it did not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub review_id: i64,
    /// Where it comes in the table, from 1. Assigned by the store.
    pub position: i64,
    /// The path, or a pattern for an area — `resources/admin/**`.
    pub path: String,
    /// What changed in it, in one line.
    pub summary: String,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub checked: FileCheck,
    /// How it was checked, in the reviewer's own words. The reason, when it was not
    /// reviewed.
    pub how_checked: Option<String>,
}

/// A file entry to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewFileChange {
    pub path: String,
    pub summary: String,
    /// Defaults to 0, which is what an area that was not changed has.
    #[serde(default)]
    pub lines_added: i64,
    #[serde(default)]
    pub lines_removed: i64,
    pub checked: FileCheck,
    #[serde(default)]
    pub how_checked: Option<String>,
}

/// One check the description lint reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptionLintResult {
    pub review_id: i64,
    /// Where it comes in the list, from 1. Assigned by the store.
    pub position: i64,
    /// What was checked — "Description length", "Working notes", "Template".
    pub label: String,
    pub status: DescriptionLintStatus,
    /// What it found — "1,840 of 2,568 characters", "All 5 headings present".
    pub detail: Option<String>,
}

/// A description lint result to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewDescriptionLintResult {
    pub label: String,
    pub status: DescriptionLintStatus,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Everything a review recorded about where its understanding came from, in one read.
///
/// One type and one call, because the page shows all four at once and a reader that had
/// to make four calls would be the dashboard doing joins — which the contract's "each
/// screen is one GET" exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewProvenance {
    pub review_id: i64,
    /// What the review read, in the order it was written.
    pub sources: Vec<ReviewSource>,
    /// What the ticket asks for, in the order it was written.
    pub criteria: Vec<AcceptanceCriterion>,
    /// The change map, in the order it was written.
    pub files: Vec<FileChange>,
    /// The description lint's individual results, in the order they were written.
    pub description_lint: Vec<DescriptionLintResult>,
}

impl ReviewProvenance {
    /// Whether the review recorded none of this.
    ///
    /// **What "reads as it does today" means.** A review written before this change, or
    /// by a session that records none of it, is empty here, and every surface serves the
    /// empty lists rather than inventing an entry.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.criteria.is_empty()
            && self.files.is_empty()
            && self.description_lint.is_empty()
    }

    /// Whether the description lint reported no failure, or `None` when it has not run.
    ///
    /// **Derived, never stored.** A stored verdict could sit above a list that
    /// contradicts it, which is the one thing a page showing both must not be able to do.
    /// A warning does not fail it, matching the draft lint's own rule.
    pub fn description_lint_passes(&self) -> Option<bool> {
        if self.description_lint.is_empty() {
            return None;
        }
        Some(
            !self
                .description_lint
                .iter()
                .any(|result| result.status == DescriptionLintStatus::Fail),
        )
    }

    /// The areas the review recorded as not reviewed.
    pub fn not_reviewed(&self) -> impl Iterator<Item = &FileChange> {
        self.files
            .iter()
            .filter(|file| file.checked == FileCheck::NotReviewed)
    }
}

impl Store {
    /// Record what the review read the problem from, replacing the whole list.
    ///
    /// An empty list clears it, which is the only way to take a source back off the
    /// record: the list is the statement, so removing an entry is restating the list.
    pub fn put_review_sources(
        &self,
        review_id: i64,
        sources: Vec<NewReviewSource>,
    ) -> Result<Vec<ReviewSource>> {
        let sources = sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| {
                let label = required(&source.label, "sources.label", "what to call this source")?;
                Ok((
                    index as i64 + 1,
                    NewReviewSource {
                        kind: source.kind,
                        label,
                        link: optional(source.link),
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.require_review(review_id, "review source")?;

        self.conn.execute(
            "DELETE FROM review_sources WHERE review_id = ?1",
            params![review_id],
        )?;
        for (position, source) in &sources {
            self.conn.execute(
                "INSERT INTO review_sources (review_id, position, kind, label, link)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![review_id, position, source.kind, source.label, source.link],
            )?;
        }

        tx.commit()?;
        self.list_review_sources(review_id)
    }

    /// What the review read the problem from, in the order it was written.
    pub fn list_review_sources(&self, review_id: i64) -> Result<Vec<ReviewSource>> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, position, kind, label, link FROM review_sources
             WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(ReviewSource {
                review_id: row.get("review_id")?,
                position: row.get("position")?,
                kind: row.get("kind")?,
                label: row.get("label")?,
                link: row.get("link")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Record what the ticket asks for, replacing the whole list.
    ///
    /// # What is refused, and why each one
    ///
    /// * **A criterion with no text.** A status against nothing is not a criterion.
    /// * **More than one pointer.** A criterion pointing at a finding *and* a file leaves
    ///   nothing able to say which the screen follows.
    /// * **A line with no file.** Unlike a finding, where a line alone is still worth
    ///   saying, a criterion's pointer is somewhere to go.
    /// * **`not_met` with no finding.** DEV-875's one rule. A review that says the change
    ///   fails a criterion and raises no finding about it posts a review that never
    ///   mentions the failure, and the ticket is then marked unmet by a record the author
    ///   never sees.
    /// * **A finding that is not on this review.** Checked here so the refusal names the
    ///   finding; the composite foreign key would refuse the row too, but as a breakage.
    ///
    /// **The write order this implies is deliberate.** A `not_met` criterion cannot be
    /// written before the finding exists, so the criteria are written — or rewritten — at
    /// layer 5 or later. That is the method: layer 2 understands the problem and layer 5
    /// reads the diff, so a review that knew at layer 2 that a criterion was unmet had
    /// read the diff out of order.
    pub fn put_acceptance_criteria(
        &self,
        review_id: i64,
        criteria: Vec<NewAcceptanceCriterion>,
    ) -> Result<Vec<AcceptanceCriterion>> {
        let criteria = criteria
            .into_iter()
            .enumerate()
            .map(|(index, criterion)| {
                let position = index as i64 + 1;
                validate_criterion(criterion, position).map(|criterion| (position, criterion))
            })
            .collect::<Result<Vec<_>>>()?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.require_review(review_id, "acceptance criterion")?;

        // Inside the transaction, so a finding cannot be added or the review removed
        // between the check and the insert.
        for (_, criterion) in &criteria {
            if let Some(seq) = criterion.finding_seq {
                if self.find_finding(review_id, seq)?.is_none() {
                    return Err(StoreError::UnknownParent {
                        entity: "acceptance criterion",
                        parent: "finding",
                        id: format!("{review_id}/{seq}"),
                    });
                }
            }
        }

        self.conn.execute(
            "DELETE FROM review_criteria WHERE review_id = ?1",
            params![review_id],
        )?;
        for (position, criterion) in &criteria {
            self.conn.execute(
                "INSERT INTO review_criteria
                     (review_id, position, text, status, finding_seq, file, line, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    review_id,
                    position,
                    criterion.text,
                    criterion.status,
                    criterion.finding_seq,
                    criterion.file,
                    criterion.line,
                    criterion.note,
                ],
            )?;
        }

        tx.commit()?;
        self.list_acceptance_criteria(review_id)
    }

    /// What the ticket asks for, in the order it was written.
    pub fn list_acceptance_criteria(&self, review_id: i64) -> Result<Vec<AcceptanceCriterion>> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, position, text, status, finding_seq, file, line, note
             FROM review_criteria WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], row_to_criterion)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Record the change map, replacing the whole table.
    ///
    /// One entry per path. The same path twice is refused rather than having the second
    /// win: two rows for one file would put two different coverage claims on one screen.
    /// An entry recorded as `not_reviewed` must say why.
    pub fn put_file_changes(
        &self,
        review_id: i64,
        files: Vec<NewFileChange>,
    ) -> Result<Vec<FileChange>> {
        let files = files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let position = index as i64 + 1;
                validate_file(file, position).map(|file| (position, file))
            })
            .collect::<Result<Vec<_>>>()?;

        if let Some(path) = first_duplicate_path(&files) {
            return Err(StoreError::InvalidValue {
                field: "files.path",
                value: path,
                expected: "one entry per path; two rows for one file are two coverage \
                           claims about it"
                    .to_string(),
            });
        }

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.require_review(review_id, "file change")?;

        self.conn.execute(
            "DELETE FROM review_files WHERE review_id = ?1",
            params![review_id],
        )?;
        for (position, file) in &files {
            self.conn.execute(
                "INSERT INTO review_files
                     (review_id, position, path, summary, lines_added, lines_removed,
                      checked, how_checked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    review_id,
                    position,
                    file.path,
                    file.summary,
                    file.lines_added,
                    file.lines_removed,
                    file.checked,
                    file.how_checked,
                ],
            )?;
        }

        tx.commit()?;
        self.list_file_changes(review_id)
    }

    /// The change map, in the order it was written.
    pub fn list_file_changes(&self, review_id: i64) -> Result<Vec<FileChange>> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, position, path, summary, lines_added, lines_removed,
                    checked, how_checked
             FROM review_files WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], row_to_file)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The sequence numbers of the review's findings on this path, in order.
    ///
    /// **This is the join the change map does not store.** A finding listed against a file
    /// exists and names that file because it is read from the findings themselves, so the
    /// agreement DEV-875 asks for holds by construction rather than by a check that could
    /// be forgotten.
    pub fn findings_on_path(&self, review_id: i64, path: &str) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT seq FROM findings
             WHERE review_id = ?1 AND file = ?2 ORDER BY seq ASC",
        )?;
        let rows = statement.query_map(params![review_id, path], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Record the description lint's individual results, replacing the whole list.
    ///
    /// The overall verdict is not taken: it is "no result failed", derived by
    /// [`ReviewProvenance::description_lint_passes`].
    pub fn put_description_lint(
        &self,
        review_id: i64,
        results: Vec<NewDescriptionLintResult>,
    ) -> Result<Vec<DescriptionLintResult>> {
        let results = results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let label = required(
                    &result.label,
                    "description_lint.label",
                    "what this check looked at",
                )?;
                Ok((
                    index as i64 + 1,
                    NewDescriptionLintResult {
                        label,
                        status: result.status,
                        detail: optional(result.detail),
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.require_review(review_id, "description lint result")?;

        self.conn.execute(
            "DELETE FROM description_lint_results WHERE review_id = ?1",
            params![review_id],
        )?;
        for (position, result) in &results {
            self.conn.execute(
                "INSERT INTO description_lint_results
                     (review_id, position, label, status, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    review_id,
                    position,
                    result.label,
                    result.status,
                    result.detail
                ],
            )?;
        }

        tx.commit()?;
        self.list_description_lint(review_id)
    }

    /// The description lint's individual results, in the order they were written.
    pub fn list_description_lint(&self, review_id: i64) -> Result<Vec<DescriptionLintResult>> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, position, label, status, detail
             FROM description_lint_results WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(DescriptionLintResult {
                review_id: row.get("review_id")?,
                position: row.get("position")?,
                label: row.get("label")?,
                status: row.get("status")?,
                detail: row.get("detail")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Everything the review recorded about where its understanding came from, in one
    /// read.
    ///
    /// A review that recorded none of it comes back with four empty lists rather than a
    /// failure: absent is silent. **A review that is not there is a failure**, and the
    /// difference matters — the four lists are read with plain queries that answer
    /// "nothing" for an id nobody has, so without this check `provenance show --review 99`
    /// printed an empty record for a review that does not exist and exited 0.
    pub fn review_provenance(&self, review_id: i64) -> Result<ReviewProvenance> {
        self.get_review(review_id)?;

        Ok(ReviewProvenance {
            review_id,
            sources: self.list_review_sources(review_id)?,
            criteria: self.list_acceptance_criteria(review_id)?,
            files: self.list_file_changes(review_id)?,
            description_lint: self.list_description_lint(review_id)?,
        })
    }

    /// Refuse a write whose review is not there, naming it as the missing parent.
    ///
    /// Every write here deletes before it inserts, so the foreign key would never fire on
    /// a list being *cleared* for a review that does not exist — the delete would match
    /// nothing and the call would report success for a review nobody has.
    fn require_review(&self, review_id: i64, entity: &'static str) -> Result<()> {
        if self.find_review_by_id(review_id)?.is_none() {
            return Err(StoreError::UnknownParent {
                entity,
                parent: "review",
                id: review_id.to_string(),
            });
        }
        Ok(())
    }
}

/// A required string, trimmed, or a refusal naming the field.
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

/// An optional string, trimmed, with an empty one treated as absent.
///
/// A caller sending `""` means "nothing", and the schema refuses an empty string so that
/// the two cannot both be on the record meaning the same thing.
fn optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Check one criterion, and hand back its canonical form.
///
/// `position` is in every refusal. An agent handed "a criterion recorded as not met with
/// no finding" and a list of five has to be told which one.
fn validate_criterion(
    criterion: NewAcceptanceCriterion,
    position: i64,
) -> Result<NewAcceptanceCriterion> {
    let text = required(
        &criterion.text,
        "criteria.text",
        &format!("what criterion {position} asks for, in the ticket's own terms"),
    )?;
    let file = optional(criterion.file);
    let note = optional(criterion.note);

    let pointers = [
        criterion.finding_seq.is_some(),
        file.is_some(),
        note.is_some(),
    ]
    .iter()
    .filter(|given| **given)
    .count();
    if pointers > 1 {
        return Err(StoreError::InvalidValue {
            field: "criteria.finding_seq",
            value: format!("criterion {position}: {text}"),
            expected: "one pointer: a finding, a file and line, or a note — not two, \
                       because nothing could say which one a screen follows"
                .to_string(),
        });
    }

    if criterion.line.is_some() && file.is_none() {
        return Err(StoreError::InvalidValue {
            field: "criteria.line",
            value: format!(
                "criterion {position} line {}",
                criterion.line.unwrap_or_default()
            ),
            expected: "a file to go with the line; a criterion's pointer is somewhere to go"
                .to_string(),
        });
    }

    if criterion.status == CriterionStatus::NotMet && criterion.finding_seq.is_none() {
        return Err(StoreError::InvalidValue {
            field: "criteria.finding_seq",
            value: format!("criterion {position}: {text}"),
            expected: "the sequence number of the finding that says so; a criterion \
                       recorded as not met with no finding posts a review that never \
                       mentions the failure, so raise the finding first"
                .to_string(),
        });
    }

    Ok(NewAcceptanceCriterion {
        text,
        file,
        note,
        ..criterion
    })
}

/// Check one file entry, and hand back its canonical form.
fn validate_file(file: NewFileChange, position: i64) -> Result<NewFileChange> {
    let path = required(
        &file.path,
        "files.path",
        &format!("entry {position}'s path, or a pattern for an area"),
    )?;
    let summary = required(
        &file.summary,
        "files.summary",
        &format!("what changed in {path}, in one line"),
    )?;
    let how_checked = optional(file.how_checked);

    for (field, lines) in [
        ("files.lines_added", file.lines_added),
        ("files.lines_removed", file.lines_removed),
    ] {
        if lines < 0 {
            return Err(StoreError::InvalidValue {
                field,
                value: format!("{path}: {lines}"),
                expected: "a count of lines, which is never negative".to_string(),
            });
        }
    }

    // The gap on the record is the whole reason the row exists, and a gap with no reason
    // is the "not checked" sentence again with extra steps.
    if file.checked == FileCheck::NotReviewed && how_checked.is_none() {
        return Err(StoreError::InvalidValue {
            field: "files.how_checked",
            value: path,
            expected: "why this was not reviewed, such as the change not touching it".to_string(),
        });
    }

    Ok(NewFileChange {
        path,
        summary,
        how_checked,
        ..file
    })
}

/// The first path given twice, if any.
///
/// The table's primary key would refuse the second row, but as a breakage rather than as
/// the refusal it is.
fn first_duplicate_path(files: &[(i64, NewFileChange)]) -> Option<String> {
    let mut seen: Vec<&str> = Vec::with_capacity(files.len());
    for (_, file) in files {
        if seen.contains(&file.path.as_str()) {
            return Some(file.path.clone());
        }
        seen.push(&file.path);
    }
    None
}

fn row_to_criterion(row: &Row<'_>) -> rusqlite::Result<AcceptanceCriterion> {
    Ok(AcceptanceCriterion {
        review_id: row.get("review_id")?,
        position: row.get("position")?,
        text: row.get("text")?,
        status: row.get("status")?,
        finding_seq: row.get("finding_seq")?,
        file: row.get("file")?,
        line: row.get("line")?,
        note: row.get("note")?,
    })
}

fn row_to_file(row: &Row<'_>) -> rusqlite::Result<FileChange> {
    Ok(FileChange {
        review_id: row.get("review_id")?,
        position: row.get("position")?,
        path: row.get("path")?,
        summary: row.get("summary")?,
        lines_added: row.get("lines_added")?,
        lines_removed: row.get("lines_removed")?,
        checked: row.get("checked")?,
        how_checked: row.get("how_checked")?,
    })
}
