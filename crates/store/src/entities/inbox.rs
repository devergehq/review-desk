//! The queries that are not about one pull request.
//!
//! Everything else in this crate is scoped to a review, a pull request or a
//! repository, because that is what a review session asks about. The Inbox asks the
//! opposite question — what is waiting on the developer, anywhere — and the Rules
//! screen asks which repositories there are at all. Neither could be answered without
//! the caller reading every review and filtering it, which is the join the dashboard
//! is not supposed to do.
//!
//! **These return reviews, not Inbox items.** The five groups on that screen carry
//! different fields and are rendered differently, and deciding what each one shows is
//! the server's job. What the store owns is which reviews belong in each group.

use rusqlite::{params, Row};

use crate::error::Result;
use crate::model::{
    Actor, DraftStatus, NitTicket, NitTicketStatus, RepoSummary, Review, ReviewAtLayer,
    ReviewLayerState, RuleStage, Timestamp,
};
use crate::{normalise, Store};

/// The reviews' columns, as `entities::review` selects them, qualified for the joins
/// below.
const REVIEW_COLUMNS: &str = "v.id, v.repo, v.pr, v.head_sha, v.round, v.title, \
                              v.ticket_id, v.changed_lines, v.not_checked, v.session_id, \
                              v.ledger_id, v.github_review_id, v.pr_state, v.pr_state_at, \
                              v.harvested_at, v.nit_ticket_status, v.nit_ticket_id, \
                              v.nit_ticket_requested_at, v.nit_ticket_filed_at, \
                              v.created_at";

impl Store {
    /// Every review whose brief is written and not yet confirmed, newest first.
    ///
    /// The Inbox's "Confirm the problem". A review with no brief at all is not here:
    /// there is nothing to confirm, and the session has not reached layer 2.
    pub fn reviews_with_unconfirmed_brief(&self, repo: Option<&str>) -> Result<Vec<Review>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS} FROM reviews v
             JOIN briefs b ON b.review_id = v.id
             WHERE b.confirmed_at IS NULL
               AND (?1 IS NULL OR v.repo = ?1)
             ORDER BY v.id DESC"
        ))?;
        let rows = statement.query_map(params![repo], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every review whose draft is at this status, newest first.
    ///
    /// `ready` is the Inbox's "Approve a draft". `approved` is what a review session
    /// looks for to learn that it may post — the question the project brief leaves
    /// open, answered by polling until something better exists.
    pub fn reviews_with_draft_status(
        &self,
        status: DraftStatus,
        repo: Option<&str>,
    ) -> Result<Vec<Review>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS} FROM reviews v
             JOIN drafts d ON d.review_id = v.id
             WHERE d.status = ?1
               AND (?2 IS NULL OR v.repo = ?2)
             ORDER BY v.id DESC"
        ))?;
        let rows = statement.query_map(params![status, repo], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every review an author or the harvester has recorded something against, newest
    /// first.
    ///
    /// The Inbox's "Author replied". Both actors, because the harvester is how most of
    /// it arrives: it records the author's reply, the thread being resolved and the
    /// code changing afterwards, and the screen shows all three as the author having
    /// acted. `since` limits it to recent activity; `None` is all of it.
    pub fn reviews_with_author_activity(
        &self,
        repo: Option<&str>,
        since: Option<Timestamp>,
    ) -> Result<Vec<Review>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS} FROM reviews v
             WHERE (?1 IS NULL OR v.repo = ?1)
               AND EXISTS (
                   SELECT 1 FROM outcomes o
                   WHERE o.review_id = v.id
                     AND o.actor IN (?2, ?3)
                     AND (?4 IS NULL OR o.occurred_at >= ?4)
               )
             ORDER BY v.id DESC"
        ))?;
        let rows = statement.query_map(
            params![repo, Actor::Author, Actor::Harvester, since],
            row_to_review,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every review a session is still working on, with the layer it is on.
    ///
    /// **In progress means a layer is `running`.** A layer `waiting_on_you` is not
    /// progress — it is the developer's turn, and those reviews are what the other
    /// groups are made of, so counting them here would show every waiting review twice.
    ///
    /// The layer reported is the highest-numbered running one: a session that left an
    /// earlier layer running is further on than that layer suggests, and the screen's
    /// "Layer 5 of 7" should say how far it got.
    pub fn reviews_in_progress(&self, repo: Option<&str>) -> Result<Vec<ReviewAtLayer>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS}, l.number, l.state, l.detail, l.updated_at
             FROM reviews v
             JOIN review_layers l ON l.review_id = v.id
             WHERE l.state = ?1
               AND (?2 IS NULL OR v.repo = ?2)
               AND l.number = (
                   SELECT MAX(number) FROM review_layers
                   WHERE review_id = v.id AND state = ?1
               )
             ORDER BY v.id DESC"
        ))?;
        let rows = statement
            .query_map(params![ReviewLayerState::Running, repo], |row| {
                Ok((row_to_review(row)?, row.get::<_, i64>("number")?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|(review, number)| {
                Ok(ReviewAtLayer {
                    layer: self.get_review_layer(review.id, number)?,
                    review,
                })
            })
            .collect()
    }

    /// The nit tickets filed since a time, newest review first.
    ///
    /// The Inbox's "11 nits filed as 3 tickets for an agent": the tickets are these,
    /// and the nits are their `finding_seqs`.
    pub fn filed_nit_tickets(
        &self,
        repo: Option<&str>,
        since: Option<Timestamp>,
    ) -> Result<Vec<NitTicket>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(
            "SELECT id FROM reviews
             WHERE nit_ticket_status = ?1
               AND (?2 IS NULL OR repo = ?2)
               AND (?3 IS NULL OR nit_ticket_filed_at >= ?3)
             ORDER BY id DESC",
        )?;
        let ids = statement
            .query_map(params![NitTicketStatus::Filed, repo, since], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        ids.into_iter().map(|id| self.nit_ticket(id)).collect()
    }

    /// Every repository the record knows about, alphabetically, with what it holds.
    ///
    /// A repository counts if it has a review or a rule of its own. Alphabetical
    /// because this fills a picker, and a picker ordered by a count rearranges itself
    /// as work lands.
    ///
    /// `open_reviews` is reviews with no posted GitHub review: the review's
    /// `github_review_id` is the record of its having been posted. `active_rules`
    /// counts the rules in force **scoped to that repository**, not the global ones,
    /// which is what the contract's description of the field says.
    pub fn list_repos(&self) -> Result<Vec<RepoSummary>> {
        let in_force = RuleStage::ALL
            .iter()
            .filter(|stage| matches!(stage, RuleStage::Active | RuleStage::Lint | RuleStage::Ci))
            .map(|stage| format!("'{}'", stage.as_str()))
            .collect::<Vec<_>>()
            .join(", ");

        let mut statement = self.conn.prepare(&format!(
            "WITH known AS (
                 SELECT repo FROM reviews
                 UNION
                 SELECT scope_repo FROM rules
                 WHERE scope = 'repo' AND scope_repo IS NOT NULL
             )
             SELECT
                 k.repo AS repo,
                 (SELECT COUNT(*) FROM reviews v
                  WHERE v.repo = k.repo AND v.github_review_id IS NULL) AS open_reviews,
                 (SELECT COUNT(*) FROM rules u
                  WHERE u.scope = 'repo' AND u.scope_repo = k.repo
                    AND u.stage IN ({in_force})) AS active_rules
             FROM known k
             ORDER BY k.repo ASC"
        ))?;
        let rows = statement.query_map([], |row| {
            Ok(RepoSummary {
                repo: row.get("repo")?,
                open_reviews: row.get("open_reviews")?,
                active_rules: row.get("active_rules")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Build a review from a row selecting [`REVIEW_COLUMNS`].
///
/// The same columns `entities::review` reads, aliased for the joins here. The two lists
/// are checked against each other by `the_two_review_column_lists_agree`, because a
/// column added to one and not the other would make a review read through the Inbox a
/// different record from the same review read by id.
fn row_to_review(row: &Row<'_>) -> rusqlite::Result<Review> {
    Ok(Review {
        id: row.get("id")?,
        repo: row.get("repo")?,
        pr: row.get("pr")?,
        head_sha: row.get("head_sha")?,
        round: row.get("round")?,
        title: row.get("title")?,
        ticket_id: row.get("ticket_id")?,
        changed_lines: row.get("changed_lines")?,
        not_checked: row.get("not_checked")?,
        session_id: row.get("session_id")?,
        ledger_id: row.get("ledger_id")?,
        github_review_id: row.get("github_review_id")?,
        pr_state: row.get("pr_state")?,
        pr_state_at: row.get("pr_state_at")?,
        harvested_at: row.get("harvested_at")?,
        nit_ticket_status: row.get("nit_ticket_status")?,
        nit_ticket_id: row.get("nit_ticket_id")?,
        nit_ticket_requested_at: row.get("nit_ticket_requested_at")?,
        nit_ticket_filed_at: row.get("nit_ticket_filed_at")?,
        created_at: row.get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A column added to one list and not the other would make a review read through
    /// the Inbox a different record from the same review read by id — the same class of
    /// bug as a lookup normalised differently from the insert.
    #[test]
    fn the_two_review_column_lists_agree() {
        let unqualify = |columns: &str| {
            columns
                .split(',')
                .map(|column| column.trim().trim_start_matches("v.").to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            unqualify(REVIEW_COLUMNS),
            unqualify(crate::entities::review::COLUMNS),
            "the Inbox's review columns and entities::review's must be the same set, \
             in the same order"
        );
    }
}
