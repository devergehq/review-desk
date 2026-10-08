//! Rules: what was learned, the failure it prevents, and how far it has graduated.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{
    now, FindingRef, NewRule, OutcomeKind, Rule, RuleCounts, RuleException, RuleScope, RuleStage,
    RulesSummary, Timestamp, ACTIVE_RULES_CAP,
};
use crate::{is_foreign_key_violation, normalise, Store};

const COLUMNS: &str = "id, statement, prevents, scope, scope_repo, stage, \
                       origin_review_id, why_proposed, note, last_cited_at, \
                       review_again_at, created_at, updated_at";

/// The stages at which a rule is in force. A proposed rule is not yet a rule, and a
/// retired one no longer is.
const IN_FORCE: &[RuleStage] = &[RuleStage::Active, RuleStage::Lint, RuleStage::Ci];

impl Store {
    /// Create a rule, with its links to the findings it came out of.
    pub fn create_rule(&self, new: NewRule) -> Result<Rule> {
        // A rule scoped to a repository that does not name one would be in force and
        // match nothing, because every lookup keys on the repository string. For the
        // same reason the name is stored canonical: checking a trimmed string and
        // inserting the raw one made a rule that `rules_for_repo` could never match.
        let new = match new.scope {
            RuleScope::Repo(repo) => {
                let repo = normalise::repo(&repo);
                if repo.is_empty() {
                    return Err(StoreError::InvalidValue {
                        field: "scope_repo",
                        value: repo,
                        expected: "a repository as owner/repo".to_string(),
                    });
                }
                NewRule {
                    scope: RuleScope::Repo(repo),
                    ..new
                }
            }
            RuleScope::Global => new,
        };

        // IMMEDIATE for the same reason as add_finding: the insert into rules and the
        // inserts into rule_findings are one step.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // One time for both columns, so a rule nothing has touched yet does not read as
        // having been changed a microsecond after it was created.
        let created_at = now();

        tx.execute(
            "INSERT INTO rules
                 (statement, prevents, scope, scope_repo, stage, origin_review_id,
                  why_proposed, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                new.statement,
                new.prevents,
                new.scope.as_str(),
                new.scope.repo(),
                new.stage,
                new.origin_review_id,
                new.why_proposed,
                new.note,
                created_at,
            ],
        )
        .map_err(|error| {
            if is_foreign_key_violation(&error) {
                StoreError::UnknownParent {
                    entity: "rule",
                    parent: "review",
                    id: new
                        .origin_review_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                }
            } else {
                StoreError::Sqlite(error)
            }
        })?;

        let id = tx.last_insert_rowid();

        // A rule cites a set of findings, so citing one twice means citing it once.
        // Without this, the duplicate breaks the link table's primary key and the
        // failure reaches the caller as a breakage rather than as the no-op it is.
        let mut cited = Vec::with_capacity(new.origin_findings.len());
        for finding in &new.origin_findings {
            if !cited.contains(finding) {
                cited.push(*finding);
            }
        }

        for finding in &cited {
            tx.execute(
                "INSERT INTO rule_findings (rule_id, review_id, finding_seq)
                 VALUES (?1, ?2, ?3)",
                params![id, finding.review_id, finding.seq],
            )
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    StoreError::UnknownParent {
                        entity: "rule",
                        parent: "finding",
                        id: format!("{}/{}", finding.review_id, finding.seq),
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;
        }

        tx.commit()?;
        self.get_rule(id)
    }

    /// Read a rule by id. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_rule(&self, id: i64) -> Result<Rule> {
        self.find_rule(id)?.ok_or(StoreError::NotFound {
            entity: "rule",
            id: id.to_string(),
        })
    }

    /// Read a rule by id, or `None`.
    pub fn find_rule(&self, id: i64) -> Result<Option<Rule>> {
        let raw = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM rules WHERE id = ?1"),
                params![id],
                row_to_raw_rule,
            )
            .optional()?;

        match raw {
            Some(raw) => Ok(Some(self.hydrate(raw)?)),
            None => Ok(None),
        }
    }

    /// The rules that apply to a repository: the global ones and that repository's own,
    /// and only those in force — active, lint or CI. Global rules come first.
    ///
    /// Proposed rules are excluded because the developer has not promoted them, and
    /// retired ones because nothing cites them any more.
    pub fn rules_for_repo(&self, repo: &str) -> Result<Vec<Rule>> {
        // Normalised through the same helper `create_rule` stores through. A lookup
        // keyed differently from the insert reports that a repository has learned
        // nothing, which is indistinguishable from the truth.
        let repo = normalise::repo(repo);

        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM rules
             WHERE (scope = 'global' OR (scope = 'repo' AND scope_repo = ?1))
               AND stage IN ({stages})
             ORDER BY CASE scope WHEN 'global' THEN 0 ELSE 1 END, id ASC",
            stages = in_force_sql_list()
        ))?;
        let raws = statement
            .query_map(params![repo], row_to_raw_rule)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        raws.into_iter().map(|raw| self.hydrate(raw)).collect()
    }

    /// Every rule, whatever its scope or stage, oldest first.
    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        let mut statement = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM rules ORDER BY id ASC"))?;
        let raws = statement
            .query_map([], row_to_raw_rule)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        raws.into_iter().map(|raw| self.hydrate(raw)).collect()
    }

    /// Move a rule to another stage, with the reason it moved. The app proposes; only
    /// the developer promotes.
    ///
    /// **The reason replaces the rule's `note`**, and `None` clears it. That is what
    /// the contract's own examples show: dismissing and retiring a rule put the reason
    /// in `note`, and promoting one — whose request body carries no reason — leaves
    /// `note` empty, because a line explaining a proposal does not survive the proposal
    /// being decided.
    ///
    /// The reason is an `Option` rather than a defaulted second function so that no
    /// call site can skip the decision without saying so. A dismissed proposal with no
    /// reason gets proposed again next week with no memory of having been told no.
    pub fn set_rule_stage(&self, id: i64, stage: RuleStage, reason: Option<&str>) -> Result<Rule> {
        self.get_rule(id)?;

        let reason = reason.map(str::trim).filter(|reason| !reason.is_empty());
        self.conn.execute(
            "UPDATE rules SET stage = ?2, note = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, stage, reason, now()],
        )?;
        self.get_rule(id)
    }

    /// Keep a rule for longer before asking about it again.
    ///
    /// `until` is a time, not a number of days: the store holds times, and turning
    /// "60 more days" into one is the caller's arithmetic. Until it passes, the rule is
    /// not shown as stale however long it has been since anything cited it.
    pub fn defer_rule(&self, id: i64, until: Timestamp, note: Option<&str>) -> Result<Rule> {
        self.get_rule(id)?;

        let note = note.map(str::trim).filter(|note| !note.is_empty());
        self.conn.execute(
            "UPDATE rules SET review_again_at = ?2, note = ?3, updated_at = ?4
             WHERE id = ?1",
            params![id, until, note, now()],
        )?;
        self.get_rule(id)
    }

    /// The rules at any of these stages, for one repository or for all of them.
    ///
    /// `repo` of `None` is every repository. A repository named gets the global rules
    /// and that repository's own, global ones first — the same scoping as
    /// [`Store::rules_for_repo`], which this does not replace: that one answers "what
    /// is in force here" for a review, and this one answers "what is there to decide
    /// on" for the Rules screen, which needs proposals too.
    ///
    /// Naming no stages is refused. A filter that permits nothing is a caller's
    /// mistake, and returning an empty list for it would read as a repository that has
    /// learned nothing.
    pub fn list_rules_at_stages(
        &self,
        repo: Option<&str>,
        stages: &[RuleStage],
    ) -> Result<Vec<Rule>> {
        if stages.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "stage",
                value: String::new(),
                expected: format!("at least one of: {}", RuleStage::valid_values()),
            });
        }

        // Normalised through the same helper `create_rule` stores through, for the
        // reason `rules_for_repo` gives: a lookup keyed differently from the insert
        // reports that a repository has learned nothing.
        let repo = repo.map(normalise::repo);

        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM rules
             WHERE (?1 IS NULL
                    OR scope = 'global'
                    OR (scope = 'repo' AND scope_repo = ?1))
               AND stage IN ({stages})
             ORDER BY CASE scope WHEN 'global' THEN 0 ELSE 1 END, id ASC",
            stages = sql_stage_list(stages)
        ))?;
        let raws = statement
            .query_map(params![repo], row_to_raw_rule)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        raws.into_iter().map(|raw| self.hydrate(raw)).collect()
    }

    /// A rule's counts, aggregated from the findings that cite it and what became of
    /// them.
    ///
    /// Five of the eight are measured here. The other three are `None` and always
    /// will be until something supplies them: two need a CI source Review Desk has
    /// not got, and the third needs a harvest of what human reviewers said. A measured
    /// zero and an absent source are different answers, so they are different values.
    ///
    /// **`raised` and `cited_by_findings` are not the same number.** `raised` is every
    /// finding citing the rule; `cited_by_findings` counts only the ones on a review
    /// other than the one the rule came out of — the Rules screen's "cited by 6 later
    /// findings", and what reconciles the contract's example, where R-09 is raised 7
    /// times and cited by 6.
    ///
    /// *Later* means a later review rather than a later finding, and that is a choice
    /// the contract leaves open. The other reading — every finding citing the rule
    /// except the ones it was created from — cannot be reached at all: a rule is
    /// created *from* findings that already exist, so none of them can have cited a
    /// rule that did not exist when they were written, and under that reading the two
    /// numbers could never differ.
    ///
    /// Which of these a screen shows differs by stage, and that is the server's
    /// decision to make. This answers what it can measure.
    pub fn rule_counts(&self, rule_id: i64) -> Result<RuleCounts> {
        self.get_rule(rule_id)?;

        let count = |sql: &str| -> Result<i64> {
            Ok(self
                .conn
                .query_row(sql, params![rule_id], |row| row.get(0))?)
        };

        let raised = count("SELECT COUNT(*) FROM findings WHERE rule_id = ?1")?;

        // A finding with two `kept` outcomes was kept once. The outcome is the fact;
        // the finding is what is being counted.
        let with_outcome = |kind: OutcomeKind, with_detail: bool| -> Result<i64> {
            let detail = if with_detail {
                "AND o.detail IS NOT NULL AND TRIM(o.detail) <> ''"
            } else {
                ""
            };
            Ok(self.conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM findings f
                     WHERE f.rule_id = ?1
                       AND EXISTS (
                           SELECT 1 FROM outcomes o
                           WHERE o.review_id = f.review_id
                             AND o.finding_seq = f.seq
                             AND o.kind = ?2
                             {detail}
                       )"
                ),
                params![rule_id, kind],
                |row| row.get(0),
            )?)
        };

        Ok(RuleCounts {
            raised: Some(raised),
            kept: Some(with_outcome(OutcomeKind::Kept, false)?),
            rejected_with_reason: Some(with_outcome(OutcomeKind::Rejected, true)?),
            cited_by_findings: Some(count(
                "SELECT COUNT(*) FROM findings f
                 JOIN rules r ON r.id = f.rule_id
                 WHERE f.rule_id = ?1
                   AND (r.origin_review_id IS NULL
                        OR f.review_id <> r.origin_review_id)",
            )?),
            exceptions: Some(count(
                "SELECT COUNT(*) FROM rule_exceptions WHERE rule_id = ?1",
            )?),
            // The three with no source. Listed rather than faked, and `None` rather
            // than 0: zero is a measurement and this is the absence of one.
            caught_by_check: None,
            raised_in_a_review_since: None,
            last_human_mention_at: None,
        })
    }

    /// The figures across the top of the Rules screen, for one repository or all of
    /// them.
    ///
    /// `window_days` is how far back the kept-findings figures look — "last 30 days,
    /// draft to posted" on the screen. `raised_twice_on_one_pr` is deliberately not
    /// windowed: a review that went in a circle is worth seeing whenever it happened,
    /// and the screen's caption puts no window on it.
    ///
    /// A finding counts as kept when a `kept` outcome says so. Deriving it from the
    /// dispositions instead would count every finding nobody has decided on yet as
    /// kept, which would make the figure flatter the review.
    pub fn rules_summary(&self, repo: Option<&str>, window_days: i64) -> Result<RulesSummary> {
        if window_days < 1 {
            return Err(StoreError::InvalidValue {
                field: "window_days",
                value: window_days.to_string(),
                expected: "a window of 1 day or more".to_string(),
            });
        }

        let repo = repo.map(normalise::repo);
        let since = now() - window_days * 86_400;

        let active_rules = self.list_rules_at_stages(repo.as_deref(), IN_FORCE)?.len() as i64;

        let findings_total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM findings f
             JOIN reviews r ON r.id = f.review_id
             WHERE f.created_at >= ?1 AND (?2 IS NULL OR r.repo = ?2)",
            params![since, repo],
            |row| row.get(0),
        )?;

        let findings_kept: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM findings f
             JOIN reviews r ON r.id = f.review_id
             WHERE f.created_at >= ?1
               AND (?2 IS NULL OR r.repo = ?2)
               AND EXISTS (
                   SELECT 1 FROM outcomes o
                   WHERE o.review_id = f.review_id
                     AND o.finding_seq = f.seq
                     AND o.kind = ?3
               )",
            params![since, repo, OutcomeKind::Kept],
            |row| row.get(0),
        )?;

        let raised_twice_on_one_pr: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT r.repo || '#' || r.pr) FROM reviews r
             WHERE (?1 IS NULL OR r.repo = ?1)
               AND EXISTS (
                   SELECT 1 FROM outcomes o
                   WHERE o.review_id = r.id AND o.kind = ?2
               )",
            params![repo, OutcomeKind::RaisedAgain],
            |row| row.get(0),
        )?;

        Ok(RulesSummary {
            active_rules,
            active_rules_cap: ACTIVE_RULES_CAP,
            // The replay consistency check does not exist, so there is nothing to
            // report rather than nothing to report *yet*.
            replay_matched: None,
            replay_total: None,
            findings_kept: Some(findings_kept),
            findings_total: Some(findings_total),
            findings_window_days: Some(window_days),
            raised_twice_on_one_pr: Some(raised_twice_on_one_pr),
        })
    }

    /// Record an accepted exception to a rule, with the reason it was accepted.
    ///
    /// The rule stays in force. An exception is the developer saying "not here, and
    /// here is why", which is a different act from retiring the rule — and the Rules
    /// screen shows it next to the stage, as "Repository rule · 1 exception".
    pub fn add_rule_exception(&self, rule_id: i64, reason: &str) -> Result<RuleException> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "reason",
                value: reason.to_string(),
                expected: "why the exception was accepted".to_string(),
            });
        }

        let created_at = now();
        self.conn
            .execute(
                "INSERT INTO rule_exceptions (rule_id, reason, created_at)
                 VALUES (?1, ?2, ?3)",
                params![rule_id, reason, created_at],
            )
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    StoreError::UnknownParent {
                        entity: "rule exception",
                        parent: "rule",
                        id: rule_id.to_string(),
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        let id = self.conn.last_insert_rowid();
        Ok(RuleException {
            id,
            rule_id,
            reason: reason.to_string(),
            created_at,
        })
    }

    /// The accepted exceptions to a rule, oldest first.
    pub fn list_rule_exceptions(&self, rule_id: i64) -> Result<Vec<RuleException>> {
        let mut statement = self.conn.prepare(
            "SELECT id, rule_id, reason, created_at FROM rule_exceptions
             WHERE rule_id = ?1 ORDER BY id ASC",
        )?;
        let rows = statement.query_map(params![rule_id], |row| {
            Ok(RuleException {
                id: row.get("id")?,
                rule_id: row.get("rule_id")?,
                reason: row.get("reason")?,
                created_at: row.get("created_at")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Turn stored columns into a rule, reading its finding links.
    fn hydrate(&self, raw: RawRule) -> Result<Rule> {
        let mut statement = self.conn.prepare(
            "SELECT review_id, finding_seq FROM rule_findings
             WHERE rule_id = ?1 ORDER BY review_id ASC, finding_seq ASC",
        )?;
        let origin_findings = statement
            .query_map(params![raw.id], |row| {
                Ok(FindingRef {
                    review_id: row.get("review_id")?,
                    seq: row.get("finding_seq")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Rule {
            id: raw.id,
            statement: raw.statement,
            prevents: raw.prevents,
            scope: RuleScope::from_columns(&raw.scope, raw.scope_repo)?,
            stage: raw.stage,
            origin_review_id: raw.origin_review_id,
            origin_findings,
            why_proposed: raw.why_proposed,
            note: raw.note,
            last_cited_at: raw.last_cited_at,
            review_again_at: raw.review_again_at,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}

/// [`IN_FORCE`] as a SQL list, so the stages are stated once and the query cannot
/// drift from the Rust constant. The values come from the constant, never from a caller.
fn in_force_sql_list() -> String {
    sql_stage_list(IN_FORCE)
}

/// Stages as a SQL list.
///
/// The text is formatted in rather than bound, as `in_force_sql_list` has always done:
/// a `RuleStage` can only be one of five values this crate declares, so there is no
/// caller string here to escape. A caller chooses *which* of them, never what they say.
fn sql_stage_list(stages: &[RuleStage]) -> String {
    stages
        .iter()
        .map(|stage| format!("'{}'", stage.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A rule's columns, before the scope is rebuilt and the links are read.
#[derive(Debug)]
struct RawRule {
    id: i64,
    statement: String,
    prevents: String,
    scope: String,
    scope_repo: Option<String>,
    stage: RuleStage,
    origin_review_id: Option<i64>,
    why_proposed: Option<String>,
    note: Option<String>,
    last_cited_at: Option<i64>,
    review_again_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

fn row_to_raw_rule(row: &Row<'_>) -> rusqlite::Result<RawRule> {
    Ok(RawRule {
        id: row.get("id")?,
        statement: row.get("statement")?,
        prevents: row.get("prevents")?,
        scope: row.get("scope")?,
        scope_repo: row.get("scope_repo")?,
        stage: row.get("stage")?,
        origin_review_id: row.get("origin_review_id")?,
        why_proposed: row.get("why_proposed")?,
        note: row.get("note")?,
        last_cited_at: row.get("last_cited_at")?,
        review_again_at: row.get("review_again_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_in_force_are_the_promoted_ones() {
        assert_eq!(
            IN_FORCE,
            &[RuleStage::Active, RuleStage::Lint, RuleStage::Ci],
            "a rule is in force once promoted, and not before or after"
        );
    }

    #[test]
    fn in_force_sql_list_quotes_every_stage() {
        assert_eq!(in_force_sql_list(), "'active', 'lint', 'ci'");
    }
}
