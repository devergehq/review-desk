//! The twelve things the three approved boards draw, as the brief's own structure.
//!
//! **Patrick approved a page per step on 10 October 2026, and the brief could feed none of
//! them.** It held `problem`, `diagram` and `provenance` as free text and options carrying a
//! paragraph each way. The Understand board draws a headline, two flows, numbered problems
//! with their detail and a supporting table; the Alternatives board draws a verdict per
//! option per problem, who proposed each option and the blind pass a session reported; the
//! Solution board draws the change in parts, the choices the ticket left open and the places
//! the description and the code disagree.
//!
//! The types live here rather than in [`crate::model`] so this change occupies its own file,
//! for the reason [`crate::entities::provenance`] gives. The fields they hang off —
//! [`crate::model::Brief`] and [`crate::model::BriefOption`] — are in `model.rs`, because a
//! brief read in two calls would be the dashboard doing joins.
//!
//! ## One type per thing, in and out
//!
//! There is no `New…` type below. Every type here is the same shape going in and coming out,
//! which is [`crate::model::BriefOption`]'s own rule: the store assigns nothing a caller
//! would have to guess. Ordering is carried by **nesting** rather than by a number — a flow
//! is a list of rows and a row is a list of one to three boxes — so the `position` and `slot`
//! columns are the store's to derive from the order written, and no caller manages numbering.
//!
//! The one exception is [`BriefProblem::number`]. That is an identity, not an order: a flow
//! box marks a problem, an option is judged against one and a solution part fixes one. A
//! caller that could not choose it could not refer to it.
//!
//! ## No length is checked here
//!
//! The brief that prompted this ran 310 words where the board shows 85, and the store is not
//! where that is caught. Lengths are a writing rule and belong to the linter in Locus
//! (DEV-884). What is refused below is structural nonsense: a flow row of four boxes, a
//! verdict against a problem that does not exist, a heading over an empty table, a
//! disagreement with no finding behind it.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::model::{string_enum, BriefOption, NewBrief};
use crate::Store;

/// The most boxes one row of a flow may hold.
///
/// Three, because "rows of one to three boxes" is a property of the picture rather than a
/// taste: the page draws a row as a grid inside one column, and a fourth box is not a wider
/// row, it is a row that does not fit.
pub const MOST_BOXES_IN_A_FLOW_ROW: usize = 3;

string_enum! {
    /// Which flow a box belongs to.
    FlowSide as "side" {
        /// How it works today. The problem half.
        Before => "before",
        /// How it works after the change.
        After => "after",
    }
}

string_enum! {
    /// Who put an option forward.
    ///
    /// Three values, because an option reaches a brief from two places and may reach it from
    /// both: the ticket the author wrote before any work started, and a blind pass a session
    /// ran on the problem alone. **Review Desk runs neither**; it records what it is told.
    OptionProposer as "proposed_by" {
        /// On the ticket, or in the pull request's description.
        Ticket => "ticket",
        /// Only the blind pass came up with it.
        BlindPass => "blind_pass",
        /// Both arrived at it independently, which is the strongest signal on the board.
        Both => "both",
    }
}

string_enum! {
    /// What an option does about one numbered problem.
    ///
    /// **`NotAssessed` is a verdict, not the absence of one.** The approved board's option D
    /// is the case it exists for: the blind pass never came up with that option, so nothing
    /// scored it against the three problems. "Nobody judged this" and "the brief forgot to
    /// judge this" are different statements, and only the first can be written down.
    ProblemVerdict as "verdict" {
        /// The problem goes away.
        Fixed => "fixed",
        /// Some of it goes away. The line beside it says which part.
        Partly => "partly",
        /// The problem is still there afterwards.
        Stays => "stays",
        /// Nobody judged it. The only verdict that needs no reason.
        NotAssessed => "not_assessed",
    }
}

/// One box of a flow: a title and a short note, optionally marked with a problem number.
///
/// A box with no `problem` is a step of the flow that nothing is wrong with — the plain
/// boxes on the approved board, the ones that are neither red nor green.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefFlowBox {
    /// What the step is, in a few words.
    pub title: String,
    /// The line underneath it.
    pub note: Option<String>,
    /// The numbered problem this box is marked with, if any.
    pub problem: Option<i64>,
}

/// One row of a flow: one to three boxes, drawn side by side.
pub type BriefFlowRow = Vec<BriefFlowBox>;

/// One numbered problem, and the detail behind it.
///
/// `was_wrong` is required and `now_fixed` is not, which is what lets the problem half be
/// written before the change has been understood — DEV-883's order is understand, then the
/// blind pass, then the solution. A numbered problem with nothing wrong is not a problem; a
/// problem whose fix has not been described yet is step 2 doing its job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefProblem {
    /// The number the flows, the option verdicts and the solution parts refer to. From 1.
    pub number: i64,
    /// Two or three words naming it, for a heading. Absent on every problem written before
    /// DEV-892, and then the heading is the number alone.
    ///
    /// **Not a shortening of `was_wrong`.** A page with no label heads the column with the
    /// number and puts the sentence one click away; it never prints the sentence as a
    /// heading. The two are different kinds of thing, and a default would make them
    /// indistinguishable on the record.
    pub label: Option<String>,
    /// One line: what was wrong.
    pub was_wrong: String,
    /// One line: what it is now. Absent until the change has been described.
    pub now_fixed: Option<String>,
    /// What to head the detail with — "The shared session slot".
    pub detail_title: Option<String>,
    /// The named detail, before the change. One string per line of it.
    #[serde(default)]
    pub detail_before: Vec<String>,
    /// The named detail, after the change.
    #[serde(default)]
    pub detail_after: Vec<String>,
}

/// The one small supporting table: a heading, headed columns and rows of cells.
///
/// Every row holds exactly one cell per column. A ragged row would leave the page unable to
/// say which heading a trailing cell belongs under, which is the only thing a table is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefSupportTable {
    /// What to call it — "The idea: three questions, each with one owner".
    pub title: Option<String>,
    /// The column headings, left to right. At least one, or there is no table.
    pub columns: Vec<String>,
    /// The rows, each one cell per column. A cell may be empty; a blank is an answer.
    pub rows: Vec<Vec<String>>,
}

/// What one option does about one numbered problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefOptionVerdict {
    /// The numbered problem being judged.
    pub problem: i64,
    pub verdict: ProblemVerdict,
    /// One line of why. Required for every verdict but [`ProblemVerdict::NotAssessed`].
    pub why: Option<String>,
}

/// The blind pass a session reported running, as it reported it.
///
/// **Review Desk ran nothing.** `given` is the problem statement the session says it handed
/// over and `looked_up` is the session's own answer to "what else did you read" — the word
/// "nothing", or a list. Neither can be checked here, and holding them is not a claim that
/// they were.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefBlindPass {
    /// What the pass was given, verbatim.
    pub given: String,
    /// The key of the option it would pick. Absent when it recommended nothing.
    pub pick_key: Option<String>,
    /// Why it would pick that one. Present exactly when `pick_key` is.
    pub pick_why: Option<String>,
    /// What it says it looked up. The word "nothing", or a list.
    pub looked_up: String,
    /// What it would need to know that the statement did not say.
    #[serde(default)]
    pub questions: Vec<String>,
}

/// Whether a brief's blind pass did not run, ran and recommended nothing, or ran.
///
/// Three states, and they are **three different things to draw**: a board with no blind
/// column at all, a board saying a fresh pair of eyes found nothing worth proposing, and a
/// board with the pass's pick beside what was built. Derived rather than stored, so a brief
/// cannot hold a state that disagrees with its own options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlindPassState {
    /// No pass was recorded.
    NotRun,
    /// A pass was recorded and proposed no option of its own.
    RanAndFoundNothing,
    /// A pass was recorded and at least one option came from it.
    Ran,
}

/// One part of the change: a title, one line, its files, and the problems it fixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefPart {
    pub title: String,
    /// The one line under the title.
    pub summary: String,
    /// The numbered problems this part fixes. Empty for a part that fixes none — tests and
    /// generated types are a part of the change and fix nothing.
    #[serde(default)]
    pub fixes: Vec<i64>,
    /// The files, in the order they are to be drawn.
    #[serde(default)]
    pub files: Vec<BriefPartFile>,
}

/// One file of a part, with what the change did to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefPartFile {
    pub path: String,
    #[serde(default)]
    pub lines_added: i64,
    #[serde(default)]
    pub lines_removed: i64,
    /// Whether the pull request changed this file.
    ///
    /// **False is how a part claims a file outside the diff.** Two findings on the review of
    /// 10 October 2026 sat in files the pull request never touched, and a part that could
    /// not name them would have left those findings belonging to no part. A file outside the
    /// diff has no lines added or removed, which is what being outside it means.
    #[serde(default = "in_the_diff")]
    pub in_diff: bool,
}

/// What `in_diff` is when a caller does not mention it.
///
/// A function rather than serde's `bool` default, which is `false`. The ordinary file is one
/// the pull request changed, and a misspelled field must not quietly turn every file in a
/// part into a claim on code outside the diff.
fn in_the_diff() -> bool {
    true
}

/// One choice the ticket left open, and what the change chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefOpenChoice {
    /// What the ticket left open, or said and the change did otherwise.
    pub left_open: String,
    /// What the change chose.
    pub chosen: String,
    /// Why.
    pub why: String,
    /// Whether this departs from what the ticket said, rather than filling a gap in it.
    #[serde(default)]
    pub departs_from_ticket: bool,
    /// The finding that raises it, if one does. A choice is a choice, not a fault, so this
    /// is a pointer rather than a requirement.
    #[serde(default)]
    pub finding_seq: Option<i64>,
}

/// One place the description and the code disagree.
///
/// **The finding is required.** DEV-875's rule applies again here: a brief that says the
/// description claims one thing and the code does another, and raises nothing about it,
/// posts a review that never mentions the disagreement — and the author goes on reading a
/// description the record knows is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefDisagreement {
    pub description_says: String,
    pub code_does: String,
    /// The finding that says so. On this review.
    pub finding_seq: i64,
}

/// The problem half of a brief, and nothing else.
///
/// **This is what a blind pass may be shown.** Rows 2, 4 and 7 of DEV-882's table, and
/// the "was wrong" line of row 5: the before flow, the numbers marking it, what was wrong
/// with each, and how the system works today.
/// Nothing from the change half is here, and the absences are deliberate:
///
/// * **no headline** — the headline states the answer ("OAuth sign-in gets its own login
///   page"), which is the one sentence that would anchor a fresh pair of eyes hardest;
/// * **no after flow, and no `now_fixed`** — those are the change, which is row 5's other
///   half;
/// * **no detail** — row 5's detail is split before and after, and the before side travels
///   with the after side; splitting a reader's click would be a half-answer;
/// * **no `diagram`** — the superseded free-text field held both flows at once and nothing
///   can separate them, which is the whole reason the flows are rows now;
/// * **no options, no blind pass, no parts, no choices, no disagreements.**
///
/// `problem` is here because it is the problem half by name and by its own documentation, and
/// because it is the only thing a brief written the old way holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemStatement {
    pub review_id: i64,
    /// What problem the change is solving, as free text.
    pub problem: String,
    /// How the system works today, in plain words.
    pub how_it_works_today: Option<String>,
    /// The sub-caption above the before flow.
    pub flow_before_caption: Option<String>,
    /// The before flow, as rows of one to three boxes.
    pub flow_before: Vec<BriefFlowRow>,
    /// What is wrong, numbered. **`was_wrong` only.**
    pub problems: Vec<WhatIsWrong>,
}

/// One numbered problem, with what was wrong and nothing about the fix.
///
/// A type of its own rather than [`BriefProblem`] with fields left out, because leaving a
/// field out is a thing a later edit can undo by habit and a missing field is not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatIsWrong {
    pub number: i64,
    pub was_wrong: String,
}

impl Store {
    /// The problem half of the review's brief.
    ///
    /// Fails with [`StoreError::NotFound`] when the review has no brief: a problem statement
    /// assembled from no brief would be an empty document that reads like a brief with
    /// nothing wrong.
    ///
    /// **Built field by field from an explicit list, never by taking a brief and removing
    /// things.** A read that subtracts grows a leak every time the brief grows a field; a
    /// read that adds cannot.
    pub fn problem_statement(&self, review_id: i64) -> Result<ProblemStatement> {
        self.find_problem_statement(review_id)?
            .ok_or(StoreError::NotFound {
                entity: "brief",
                id: review_id.to_string(),
            })
    }

    /// The problem half, or `None` when the review has no brief.
    ///
    /// **One read of the brief, not two.** A caller that has to tell "no such review" from
    /// "no brief on it" would otherwise ask for the brief to find out and then ask again
    /// to read it — and reading a brief is a dozen statements, because it carries every
    /// row the three pages draw. The cheapest read in the contract should not be the most
    /// expensive one in the store.
    pub fn find_problem_statement(&self, review_id: i64) -> Result<Option<ProblemStatement>> {
        let Some(brief) = self.find_brief(review_id)? else {
            return Ok(None);
        };
        Ok(Some(ProblemStatement {
            review_id: brief.review_id,
            problem: brief.problem,
            how_it_works_today: brief.how_it_works_today,
            flow_before_caption: brief.flow_before_caption,
            flow_before: brief.flow_before,
            problems: brief
                .problems
                .into_iter()
                .map(|problem| WhatIsWrong {
                    number: problem.number,
                    was_wrong: problem.was_wrong,
                })
                .collect(),
        }))
    }

    /// Read one flow of the review's brief, as rows of boxes.
    pub(crate) fn brief_flow(&self, review_id: i64, side: FlowSide) -> Result<Vec<BriefFlowRow>> {
        let mut statement = self.conn.prepare(
            "SELECT position, title, note, problem FROM brief_flow_boxes
             WHERE review_id = ?1 AND side = ?2 ORDER BY position ASC, slot ASC",
        )?;
        let rows = statement.query_map(params![review_id, side], |row| {
            Ok((
                row.get::<_, i64>("position")?,
                BriefFlowBox {
                    title: row.get("title")?,
                    note: row.get("note")?,
                    problem: row.get("problem")?,
                },
            ))
        })?;

        // Grouped into rows on the way out. The position is what the slots were stored
        // under; the nesting is what the caller gave and what the page draws.
        let mut flow: Vec<BriefFlowRow> = Vec::new();
        let mut current: Option<i64> = None;
        for row in rows {
            let (position, flow_box) = row?;
            if current != Some(position) {
                flow.push(Vec::new());
                current = Some(position);
            }
            flow.last_mut()
                .expect("a row was just pushed")
                .push(flow_box);
        }
        Ok(flow)
    }

    /// The review's numbered problems, with their detail, in number order.
    pub(crate) fn brief_problems(&self, review_id: i64) -> Result<Vec<BriefProblem>> {
        let mut statement = self.conn.prepare(
            "SELECT number, label, was_wrong, now_fixed, detail_title FROM brief_problems
             WHERE review_id = ?1 ORDER BY number ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(BriefProblem {
                number: row.get("number")?,
                label: row.get("label")?,
                was_wrong: row.get("was_wrong")?,
                now_fixed: row.get("now_fixed")?,
                detail_title: row.get("detail_title")?,
                detail_before: Vec::new(),
                detail_after: Vec::new(),
            })
        })?;
        let mut problems = rows.collect::<rusqlite::Result<Vec<_>>>()?;

        for problem in &mut problems {
            problem.detail_before =
                self.brief_problem_detail(review_id, problem.number, "before")?;
            problem.detail_after = self.brief_problem_detail(review_id, problem.number, "after")?;
        }
        Ok(problems)
    }

    /// One side of one problem's named detail, in the order it was written.
    fn brief_problem_detail(&self, review_id: i64, number: i64, side: &str) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT text FROM brief_problem_detail
             WHERE review_id = ?1 AND number = ?2 AND side = ?3 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id, number, side], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The review's supporting table, or `None` when it has no columns.
    pub(crate) fn brief_support_table(
        &self,
        review_id: i64,
        title: Option<String>,
    ) -> Result<Option<BriefSupportTable>> {
        let mut headings = self.conn.prepare(
            "SELECT heading FROM brief_support_columns WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let columns = headings
            .query_map(params![review_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if columns.is_empty() {
            return Ok(None);
        }

        let mut cells = self.conn.prepare(
            "SELECT position, slot, text FROM brief_support_cells
             WHERE review_id = ?1 ORDER BY position ASC, slot ASC",
        )?;
        let read = cells.query_map(params![review_id], |row| {
            Ok((
                row.get::<_, i64>("position")?,
                row.get::<_, i64>("slot")?,
                row.get::<_, String>("text")?,
            ))
        })?;

        // One row per stored position, each already the full width: `put_brief` refuses a
        // ragged row, so a missing cell here would be a database edited behind the store.
        let mut rows: Vec<Vec<String>> = Vec::new();
        for cell in read {
            let (position, slot, text) = cell?;
            while rows.len() < position as usize {
                rows.push(vec![String::new(); columns.len()]);
            }
            if let Some(row) = rows.get_mut(position as usize - 1) {
                if let Some(place) = row.get_mut(slot as usize - 1) {
                    *place = text;
                }
            }
        }

        Ok(Some(BriefSupportTable {
            title,
            columns,
            rows,
        }))
    }

    /// Every option's verdicts, by option key.
    pub(crate) fn brief_option_verdicts(
        &self,
        review_id: i64,
        key: &str,
    ) -> Result<Vec<BriefOptionVerdict>> {
        let mut statement = self.conn.prepare(
            "SELECT problem, verdict, why FROM brief_option_verdicts
             WHERE review_id = ?1 AND key = ?2 ORDER BY problem ASC",
        )?;
        let rows = statement.query_map(params![review_id, key], |row| {
            Ok(BriefOptionVerdict {
                problem: row.get("problem")?,
                verdict: row.get("verdict")?,
                why: row.get("why")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The blind pass the review recorded, or `None` when none ran.
    pub(crate) fn brief_blind_pass(&self, review_id: i64) -> Result<Option<BriefBlindPass>> {
        let held = self
            .conn
            .query_row(
                "SELECT given, pick_key, pick_why, looked_up FROM brief_blind_pass
                 WHERE review_id = ?1",
                params![review_id],
                |row| {
                    Ok(BriefBlindPass {
                        given: row.get("given")?,
                        pick_key: row.get("pick_key")?,
                        pick_why: row.get("pick_why")?,
                        looked_up: row.get("looked_up")?,
                        questions: Vec::new(),
                    })
                },
            )
            .optional()?;

        match held {
            Some(pass) => {
                let mut statement = self.conn.prepare(
                    "SELECT text FROM brief_blind_questions WHERE review_id = ?1
                     ORDER BY position ASC",
                )?;
                let questions = statement
                    .query_map(params![review_id], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(Some(BriefBlindPass { questions, ..pass }))
            }
            None => Ok(None),
        }
    }

    /// The change in parts, in the order they were written.
    pub(crate) fn brief_parts(&self, review_id: i64) -> Result<Vec<BriefPart>> {
        let mut statement = self.conn.prepare(
            "SELECT position, title, summary FROM brief_parts
             WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok((
                row.get::<_, i64>("position")?,
                BriefPart {
                    title: row.get("title")?,
                    summary: row.get("summary")?,
                    fixes: Vec::new(),
                    files: Vec::new(),
                },
            ))
        })?;

        let mut parts = Vec::new();
        for row in rows {
            let (position, mut part) = row?;
            part.fixes = self.brief_part_problems(review_id, position)?;
            part.files = self.brief_part_files(review_id, position)?;
            parts.push(part);
        }
        Ok(parts)
    }

    /// The numbered problems one part fixes, in number order.
    fn brief_part_problems(&self, review_id: i64, part: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT problem FROM brief_part_problems WHERE review_id = ?1 AND part = ?2
             ORDER BY problem ASC",
        )?;
        let rows = statement.query_map(params![review_id, part], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One part's files, in the order they were written.
    fn brief_part_files(&self, review_id: i64, part: i64) -> Result<Vec<BriefPartFile>> {
        let mut statement = self.conn.prepare(
            "SELECT path, lines_added, lines_removed, in_diff FROM brief_part_files
             WHERE review_id = ?1 AND part = ?2 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id, part], row_to_part_file)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The choices the ticket left open, in the order they were written.
    pub(crate) fn brief_open_choices(&self, review_id: i64) -> Result<Vec<BriefOpenChoice>> {
        let mut statement = self.conn.prepare(
            "SELECT left_open, chosen, why, departs_from_ticket, finding_seq
             FROM brief_open_choices WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(BriefOpenChoice {
                left_open: row.get("left_open")?,
                chosen: row.get("chosen")?,
                why: row.get("why")?,
                departs_from_ticket: row.get("departs_from_ticket")?,
                finding_seq: row.get("finding_seq")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Where the description and the code disagree, in the order they were written.
    pub(crate) fn brief_disagreements(&self, review_id: i64) -> Result<Vec<BriefDisagreement>> {
        let mut statement = self.conn.prepare(
            "SELECT description_says, code_does, finding_seq FROM brief_disagreements
             WHERE review_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![review_id], |row| {
            Ok(BriefDisagreement {
                description_says: row.get("description_says")?,
                code_does: row.get("code_does")?,
                finding_seq: row.get("finding_seq")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn row_to_part_file(row: &Row<'_>) -> rusqlite::Result<BriefPartFile> {
    Ok(BriefPartFile {
        path: row.get("path")?,
        lines_added: row.get("lines_added")?,
        lines_removed: row.get("lines_removed")?,
        in_diff: row.get("in_diff")?,
    })
}

/// A refusal naming the field and what would have been accepted.
pub(crate) fn invalid(field: &'static str, value: impl Into<String>, expected: &str) -> StoreError {
    StoreError::InvalidValue {
        field,
        value: value.into(),
        expected: expected.to_string(),
    }
}

/// Check the whole structure of a brief about to be written.
///
/// **Pure, and called before the transaction opens**, which is the server's rule applied in
/// the store: a refusal computed after a write gives the caller an error for a request that
/// succeeded. Everything here is answerable from the document alone — a problem number is
/// checked against the problems *in the same document*, not against the ones already on the
/// record, because the write replaces them.
///
/// The one thing it cannot answer is whether a finding exists; that is read inside the
/// transaction by [`Store::check_brief_findings`].
pub(crate) fn validate_brief_structure(new: &NewBrief) -> Result<()> {
    let numbers = validate_problems(&new.problems)?;

    // The four free lines on the brief itself. Migration 10 adds them with `ALTER TABLE`,
    // and SQLite cannot put a CHECK on a table that already exists -- so this is the only
    // layer there is, the way `put_brief` is for `confirmation_not_required_because`.
    for (field, value) in [
        ("headline", new.headline.as_deref()),
        ("how_it_works_today", new.how_it_works_today.as_deref()),
        ("flow_before_caption", new.flow_before_caption.as_deref()),
        ("flow_after_caption", new.flow_after_caption.as_deref()),
    ] {
        optional(field, value, "the brief")?;
    }

    for (side, flow) in [
        ("flow_before", &new.flow_before),
        ("flow_after", &new.flow_after),
    ] {
        validate_flow(side, flow, &numbers)?;
    }

    // A caption over an empty flow is the same fault as a heading over an empty table:
    // the page draws the words and then a blank where the picture goes.
    for (field, caption, flow) in [
        (
            "flow_before_caption",
            new.flow_before_caption.as_deref(),
            &new.flow_before,
        ),
        (
            "flow_after_caption",
            new.flow_after_caption.as_deref(),
            &new.flow_after,
        ),
    ] {
        if let Some(caption) = caption {
            if flow.is_empty() {
                return Err(invalid(
                    field,
                    caption,
                    "a flow to caption; a caption over an empty flow draws the words and \
                     then a blank where the picture goes",
                ));
            }
        }
    }

    if let Some(table) = &new.support_table {
        validate_support_table(table)?;
    }

    for option in &new.options {
        validate_verdicts(option, &numbers)?;
    }

    match &new.blind_pass {
        Some(pass) => validate_blind_pass(pass, &new.options)?,
        // **The other direction, which is the one that was missing.** `blind_pass_state`
        // short-circuits on an absent pass and answers `not_run`, so an option claiming a
        // pass that is not on the record would make the Alternatives page badge an option
        // "Blind" with no blind column to put it in -- the disagreement between a brief
        // and its own options that the derived state exists to make impossible.
        None => {
            if let Some(option) = new.options.iter().find(|option| {
                matches!(
                    option.proposed_by,
                    OptionProposer::BlindPass | OptionProposer::Both
                )
            }) {
                return Err(invalid(
                    "options.proposed_by",
                    format!("option {}: {}", option.key, option.proposed_by),
                    "the blind pass that proposed it, in `blind_pass`, or `ticket`; an \
                     option from a pass this brief does not record is a pass nothing \
                     can show",
                ));
            }
        }
    }

    validate_parts(&new.parts, &numbers)?;

    for (position, choice) in new.open_choices.iter().enumerate() {
        let where_it_is = format!("open_choices[{}]", position + 1);
        required("open_choices.left_open", &choice.left_open, &where_it_is)?;
        required("open_choices.chosen", &choice.chosen, &where_it_is)?;
        required("open_choices.why", &choice.why, &where_it_is)?;
    }

    for (position, clash) in new.disagreements.iter().enumerate() {
        let where_it_is = format!("disagreements[{}]", position + 1);
        required(
            "disagreements.description_says",
            &clash.description_says,
            &where_it_is,
        )?;
        required("disagreements.code_does", &clash.code_does, &where_it_is)?;
    }

    Ok(())
}

/// The problem numbers, checked and collected.
fn validate_problems(problems: &[BriefProblem]) -> Result<Vec<i64>> {
    let mut numbers: Vec<i64> = Vec::with_capacity(problems.len());
    for problem in problems {
        if problem.number < 1 {
            return Err(invalid(
                "problems.number",
                problem.number.to_string(),
                "a number from 1; the flows, the option verdicts and the solution parts \
                 all refer to it",
            ));
        }
        if numbers.contains(&problem.number) {
            return Err(invalid(
                "problems.number",
                problem.number.to_string(),
                "one problem per number; two problems numbered the same would leave \
                 nothing able to say which one a red box is marked with",
            ));
        }
        numbers.push(problem.number);

        let at = format!("problem {}", problem.number);
        optional("problems.label", problem.label.as_deref(), &at)?;
        required("problems.was_wrong", &problem.was_wrong, &at)?;
        optional("problems.now_fixed", problem.now_fixed.as_deref(), &at)?;
        optional(
            "problems.detail_title",
            problem.detail_title.as_deref(),
            &at,
        )?;
        for line in problem.detail_before.iter().chain(&problem.detail_after) {
            required("problems.detail", line, &at)?;
        }
    }
    Ok(numbers)
}

/// Check one flow: its row widths, its boxes, and the problems they mark.
fn validate_flow(field: &'static str, flow: &[BriefFlowRow], numbers: &[i64]) -> Result<()> {
    let problem_field: &'static str = match field {
        "flow_before" => "flow_before.problem",
        _ => "flow_after.problem",
    };

    for (index, row) in flow.iter().enumerate() {
        let position = index + 1;
        if row.is_empty() {
            return Err(invalid(
                field,
                format!("row {position}"),
                "at least one box; an empty row is a gap the page draws as a gap",
            ));
        }
        if row.len() > MOST_BOXES_IN_A_FLOW_ROW {
            return Err(invalid(
                field,
                format!("row {position}: {} boxes", row.len()),
                "one to three boxes; a fourth does not fit the column the flow is drawn in",
            ));
        }

        for flow_box in row {
            let at = format!("{field} row {position}");
            required("flow.title", &flow_box.title, &at)?;
            optional("flow.note", flow_box.note.as_deref(), &at)?;
            if let Some(problem) = flow_box.problem {
                known_problem(problem_field, problem, numbers)?;
            }
        }
    }
    Ok(())
}

/// Check the supporting table is a grid with headings.
fn validate_support_table(table: &BriefSupportTable) -> Result<()> {
    optional("support_table.title", table.title.as_deref(), "the table")?;

    if table.columns.is_empty() {
        return Err(invalid(
            "support_table.columns",
            table.title.clone().unwrap_or_default(),
            "at least one column heading; a table with no columns is a heading over \
             nothing, so leave the table out instead",
        ));
    }
    for heading in &table.columns {
        required("support_table.columns", heading, "the table")?;
    }

    for (index, row) in table.rows.iter().enumerate() {
        if row.len() != table.columns.len() {
            return Err(invalid(
                "support_table.rows",
                format!("row {}: {} cells", index + 1, row.len()),
                &format!(
                    "{} cells, one per column heading; a ragged row leaves the page unable \
                     to say which heading a cell sits under",
                    table.columns.len()
                ),
            ));
        }
    }
    Ok(())
}

/// Check one option's verdicts against the numbered problems.
fn validate_verdicts(option: &BriefOption, numbers: &[i64]) -> Result<()> {
    let mut seen: Vec<i64> = Vec::with_capacity(option.verdicts.len());
    for verdict in &option.verdicts {
        known_problem("options.verdicts.problem", verdict.problem, numbers)?;
        if seen.contains(&verdict.problem) {
            return Err(invalid(
                "options.verdicts.problem",
                format!("option {}: problem {}", option.key, verdict.problem),
                "one verdict per problem; two verdicts on one problem would both answer \
                 the same cell of the matrix",
            ));
        }
        seen.push(verdict.problem);

        let at = format!("option {} on problem {}", option.key, verdict.problem);
        match (verdict.verdict, verdict.why.as_deref()) {
            (ProblemVerdict::NotAssessed, Some(why)) if why.trim().is_empty() => {
                return Err(invalid("options.verdicts.why", at, "nothing, or a reason"));
            }
            (ProblemVerdict::NotAssessed, _) => {}
            (_, None) => {
                return Err(invalid(
                    "options.verdicts.why",
                    at,
                    "one line of why; a verdict with no reason is a coloured dot",
                ));
            }
            (_, Some(why)) => required("options.verdicts.why", why, &at)?,
        }
    }
    Ok(())
}

/// Check the blind pass, and that its pick is one of the brief's own options.
fn validate_blind_pass(pass: &BriefBlindPass, options: &[BriefOption]) -> Result<()> {
    required("blind_pass.given", &pass.given, "the blind pass")?;
    required("blind_pass.looked_up", &pass.looked_up, "the blind pass")?;
    for question in &pass.questions {
        required("blind_pass.questions", question, "the blind pass")?;
    }

    match (pass.pick_key.as_deref(), pass.pick_why.as_deref()) {
        (None, None) => {}
        (Some(key), Some(why)) => {
            required("blind_pass.pick_key", key, "the blind pass")?;
            required("blind_pass.pick_why", why, "the blind pass")?;
            if !options.iter().any(|option| option.key == key) {
                return Err(invalid(
                    "blind_pass.pick_key",
                    key,
                    "the key of one of this brief's own options; a pick that is not on \
                     the option list is a title the page can say nothing else about",
                ));
            }
        }
        (Some(key), None) => {
            return Err(invalid(
                "blind_pass.pick_why",
                key,
                "why it would pick that one; a pick with no reason is the anchoring the \
                 pass exists to avoid",
            ));
        }
        (None, Some(why)) => {
            return Err(invalid(
                "blind_pass.pick_key",
                why,
                "the option the reason is about, or neither",
            ));
        }
    }
    Ok(())
}

/// Check the parts, their files and the problems they claim to fix.
fn validate_parts(parts: &[BriefPart], numbers: &[i64]) -> Result<()> {
    let mut paths: Vec<&str> = Vec::new();

    for (index, part) in parts.iter().enumerate() {
        let at = format!("part {}", index + 1);
        required("parts.title", &part.title, &at)?;
        required("parts.summary", &part.summary, &at)?;

        let mut fixes: Vec<i64> = Vec::with_capacity(part.fixes.len());
        for problem in &part.fixes {
            known_problem("parts.fixes", *problem, numbers)?;
            if fixes.contains(problem) {
                return Err(invalid(
                    "parts.fixes",
                    format!("{at}: problem {problem}"),
                    "each problem once; a part does not fix one problem twice",
                ));
            }
            fixes.push(*problem);
        }

        for file in &part.files {
            required("parts.files.path", &file.path, &at)?;
            for (field, lines) in [
                ("parts.files.lines_added", file.lines_added),
                ("parts.files.lines_removed", file.lines_removed),
            ] {
                if lines < 0 {
                    return Err(invalid(
                        field,
                        format!("{}: {lines}", file.path),
                        "a count of lines, which is never negative",
                    ));
                }
            }
            if !file.in_diff && (file.lines_added != 0 || file.lines_removed != 0) {
                return Err(invalid(
                    "parts.files.in_diff",
                    format!(
                        "{}: +{} -{}",
                        file.path, file.lines_added, file.lines_removed
                    ),
                    "no lines added or removed; a file outside the diff is one the pull \
                     request did not change, which is what makes it claimable at all",
                ));
            }
            if paths.contains(&file.path.as_str()) {
                return Err(invalid(
                    "parts.files.path",
                    file.path.clone(),
                    "one part per file; a path owned twice answers \"which part do this \
                     file's findings sit on\" twice",
                ));
            }
            paths.push(&file.path);
        }
    }
    Ok(())
}

/// Refuse a problem number that no problem in this brief carries.
///
/// **The number is in the refusal**, which is the whole of what makes it useful: a session
/// handed "a problem that is not there" and a flow of eleven boxes has to be told which one.
fn known_problem(field: &'static str, problem: i64, numbers: &[i64]) -> Result<()> {
    if numbers.contains(&problem) {
        return Ok(());
    }
    Err(invalid(
        field,
        format!("problem {problem}"),
        &if numbers.is_empty() {
            "a problem this brief numbers; it numbers none, so write the problems first".to_string()
        } else {
            format!(
                "a problem this brief numbers, which are {}",
                numbers
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
    ))
}

/// Refuse a required string that is blank.
fn required(field: &'static str, value: &str, at: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(invalid(field, at, "something rather than nothing"));
    }
    Ok(())
}

/// Refuse an optional string that is present and blank.
///
/// A field given as `""` is not the same as a field left out, and the difference matters:
/// leaving it out says there is none, and sending a blank says there is one and it is empty.
/// The second is what the database's own CHECK refuses; this names which field it was.
fn optional(field: &'static str, value: Option<&str>, at: &str) -> Result<()> {
    match value {
        Some(value) if value.trim().is_empty() => {
            Err(invalid(field, at, "something, or leave the field out"))
        }
        _ => Ok(()),
    }
}

impl Store {
    /// Check that every finding the brief points at is a finding of this review.
    ///
    /// Read inside the transaction, so a finding cannot be added or the review removed
    /// between the check and the insert — and checked here rather than left to the composite
    /// foreign key so the refusal names the finding instead of breaking.
    pub(crate) fn check_brief_findings(&self, review_id: i64, new: &NewBrief) -> Result<()> {
        let pointers = new
            .open_choices
            .iter()
            .filter_map(|choice| choice.finding_seq)
            .chain(new.disagreements.iter().map(|clash| clash.finding_seq));

        for seq in pointers {
            if self.find_finding(review_id, seq)?.is_none() {
                return Err(StoreError::UnknownParent {
                    entity: "brief",
                    parent: "finding",
                    id: format!("{review_id}/{seq}"),
                });
            }
        }
        Ok(())
    }

    /// Remove everything the structure holds, in reverse dependency order.
    ///
    /// Called before the options are replaced, because the verdicts and the blind pass's
    /// pick hang off them.
    pub(crate) fn clear_brief_structure(&self, review_id: i64) -> Result<()> {
        for table in [
            "brief_blind_questions",
            "brief_blind_pass",
            "brief_option_verdicts",
            "brief_part_problems",
            "brief_part_files",
            "brief_parts",
            "brief_problem_detail",
            "brief_flow_boxes",
            "brief_support_cells",
            "brief_support_columns",
            "brief_problems",
            "brief_open_choices",
            "brief_disagreements",
        ] {
            self.conn.execute(
                &format!("DELETE FROM {table} WHERE review_id = ?1"),
                params![review_id],
            )?;
        }
        Ok(())
    }

    /// Write the structure, in dependency order.
    ///
    /// Called after the options are inserted, inside the one transaction `put_brief` opens.
    /// **The whole brief is one record**: a flow whose problems were written and whose boxes
    /// were not is not a brief anybody wrote.
    pub(crate) fn write_brief_structure(&self, review_id: i64, new: &NewBrief) -> Result<()> {
        for problem in &new.problems {
            self.conn.execute(
                "INSERT INTO brief_problems
                     (review_id, number, label, was_wrong, now_fixed, detail_title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    review_id,
                    problem.number,
                    problem.label,
                    problem.was_wrong,
                    problem.now_fixed,
                    problem.detail_title,
                ],
            )?;
            for (side, lines) in [
                ("before", &problem.detail_before),
                ("after", &problem.detail_after),
            ] {
                for (index, text) in lines.iter().enumerate() {
                    self.conn.execute(
                        "INSERT INTO brief_problem_detail
                             (review_id, number, side, position, text)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![review_id, problem.number, side, index as i64 + 1, text],
                    )?;
                }
            }
        }

        for (side, flow) in [
            (FlowSide::Before, &new.flow_before),
            (FlowSide::After, &new.flow_after),
        ] {
            for (row, boxes) in flow.iter().enumerate() {
                for (slot, flow_box) in boxes.iter().enumerate() {
                    self.conn.execute(
                        "INSERT INTO brief_flow_boxes
                             (review_id, side, position, slot, title, note, problem)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            review_id,
                            side,
                            row as i64 + 1,
                            slot as i64 + 1,
                            flow_box.title,
                            flow_box.note,
                            flow_box.problem,
                        ],
                    )?;
                }
            }
        }

        if let Some(table) = &new.support_table {
            for (index, heading) in table.columns.iter().enumerate() {
                self.conn.execute(
                    "INSERT INTO brief_support_columns (review_id, position, heading)
                     VALUES (?1, ?2, ?3)",
                    params![review_id, index as i64 + 1, heading],
                )?;
            }
            for (row, cells) in table.rows.iter().enumerate() {
                for (slot, text) in cells.iter().enumerate() {
                    self.conn.execute(
                        "INSERT INTO brief_support_cells (review_id, position, slot, text)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![review_id, row as i64 + 1, slot as i64 + 1, text],
                    )?;
                }
            }
        }

        for option in &new.options {
            for verdict in &option.verdicts {
                self.conn.execute(
                    "INSERT INTO brief_option_verdicts
                         (review_id, key, problem, verdict, why)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        review_id,
                        option.key,
                        verdict.problem,
                        verdict.verdict,
                        verdict.why,
                    ],
                )?;
            }
        }

        if let Some(pass) = &new.blind_pass {
            self.conn.execute(
                "INSERT INTO brief_blind_pass
                     (review_id, given, pick_key, pick_why, looked_up)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    review_id,
                    pass.given,
                    pass.pick_key,
                    pass.pick_why,
                    pass.looked_up,
                ],
            )?;
            for (index, question) in pass.questions.iter().enumerate() {
                self.conn.execute(
                    "INSERT INTO brief_blind_questions (review_id, position, text)
                     VALUES (?1, ?2, ?3)",
                    params![review_id, index as i64 + 1, question],
                )?;
            }
        }

        for (index, part) in new.parts.iter().enumerate() {
            let position = index as i64 + 1;
            self.conn.execute(
                "INSERT INTO brief_parts (review_id, position, title, summary)
                 VALUES (?1, ?2, ?3, ?4)",
                params![review_id, position, part.title, part.summary],
            )?;
            for (file_index, file) in part.files.iter().enumerate() {
                self.conn.execute(
                    "INSERT INTO brief_part_files
                         (review_id, part, position, path, lines_added, lines_removed, in_diff)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        review_id,
                        position,
                        file_index as i64 + 1,
                        file.path,
                        file.lines_added,
                        file.lines_removed,
                        file.in_diff,
                    ],
                )?;
            }
            for problem in &part.fixes {
                self.conn.execute(
                    "INSERT INTO brief_part_problems (review_id, part, problem)
                     VALUES (?1, ?2, ?3)",
                    params![review_id, position, problem],
                )?;
            }
        }

        for (index, choice) in new.open_choices.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO brief_open_choices
                     (review_id, position, left_open, chosen, why, departs_from_ticket,
                      finding_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    review_id,
                    index as i64 + 1,
                    choice.left_open,
                    choice.chosen,
                    choice.why,
                    choice.departs_from_ticket,
                    choice.finding_seq,
                ],
            )?;
        }

        for (index, clash) in new.disagreements.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO brief_disagreements
                     (review_id, position, description_says, code_does, finding_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    review_id,
                    index as i64 + 1,
                    clash.description_says,
                    clash.code_does,
                    clash.finding_seq,
                ],
            )?;
        }

        Ok(())
    }
}
