//! Rules: what was learned, the failure it prevents, and how far it has graduated.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{now, FindingRef, NewRule, Rule, RuleException, RuleScope, RuleStage};
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

    /// Move a rule to another stage. The app proposes; only the developer promotes.
    pub fn set_rule_stage(&self, id: i64, stage: RuleStage) -> Result<Rule> {
        self.get_rule(id)?;
        self.conn.execute(
            "UPDATE rules SET stage = ?2 WHERE id = ?1",
            params![id, stage],
        )?;
        self.get_rule(id)
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
    IN_FORCE
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
