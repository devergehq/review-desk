//! The queries that are not about one pull request.
//!
//! Everything else in this crate is scoped to a review, a pull request or a
//! repository, because that is what a review session asks about. The Inbox asks the
//! opposite question — what is waiting on the developer, anywhere — and the Rules
//! screen asks which repositories there are at all. Neither could be answered without
//! the caller reading every review and filtering it, which is the join the dashboard
//! is not supposed to do.
//!
//! **These return reviews, not Inbox items.** The groups on that screen carry different
//! fields and are rendered differently, and deciding what each one shows is the server's
//! job — including, since DEV-930, whether a figure is a measurement or the absence of
//! one. What the store owns is which reviews belong in each group.

use rusqlite::{params, Row};

use crate::error::Result;
use crate::model::{
    Actor, AuthorResponse, DraftStatus, NitTicket, NitTicketStatus, OutcomeKind, PostedReview,
    PrState, RepoSummary, Review, ReviewAtLayer, ReviewAtStep, ReviewLayerName, ReviewLayerState,
    ReviewStep, RuleStage, Timestamp,
};
use crate::{normalise, Store};

/// The reviews' columns, as `entities::review` selects them, qualified for the joins
/// below.
pub(super) const REVIEW_COLUMNS: &str = "v.id, v.repo, v.pr, v.head_sha, v.round, v.title, \
                              v.ticket_id, v.changed_lines, v.not_checked, v.session_id, \
                              v.ledger_id, v.github_review_id, v.pr_state, v.pr_state_at, \
                              v.harvested_at, v.nit_ticket_status, v.nit_ticket_id, \
                              v.nit_ticket_requested_at, v.nit_ticket_filed_at, \
                              v.last_activity_at, v.created_at";

impl Store {
    /// Every review whose brief is written, needs confirming, and has not been
    /// confirmed; newest first.
    ///
    /// The Inbox's "Confirm the problem". A review with no brief at all is not here:
    /// there is nothing to confirm, and the session has not reached layer 2.
    ///
    /// **Nor is a brief recorded as needing no confirmation.** The group is the
    /// developer's queue, and a brief the caller said needed no confirmation is not in
    /// anybody's queue — listing it would be asking for the act the field exists to say
    /// was unnecessary. It is still on the One review screen, where the developer can
    /// read it and still send the draft back.
    pub fn reviews_with_unconfirmed_brief(&self, repo: Option<&str>) -> Result<Vec<Review>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS} FROM reviews v
             JOIN briefs b ON b.review_id = v.id
             WHERE b.confirmed_at IS NULL
               AND b.confirmation_required = 1
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

    /// Every posted review whose pull request has not been merged or closed, newest
    /// posting first, with what has come back on it.
    ///
    /// The Inbox's "Waiting on the author", and the group the dashboard had none of: a
    /// review the developer signed off and the session posted left every group here, so
    /// the queue could only say the whole thing was not measured and the review was
    /// reachable by typing its id. DEV-930.
    ///
    /// **Posted is the draft reported posted, or the review carrying a GitHub review
    /// id.** Two columns in this schema answer "posted" and they are written by
    /// different commands: `post_draft` writes both in one transaction, but
    /// `review link --github-review` writes only the second and requires no draft at
    /// all — and [`Store::list_repos`] has long counted *that* one as the review no
    /// longer being open. A review posted through `review link` was in no group on any
    /// page, which is the complaint this query exists to answer, so both count.
    ///
    /// **Open means open or unlooked-at**, the rule [`Store::reviews_to_harvest`] uses
    /// and for the same reason: a review nobody has harvested has no recorded state, so
    /// the honest answer is that nothing is known. A pull request merged or closed at
    /// its last harvest is finished with — there is no author left to wait for.
    ///
    /// **Only the newest round of a pull request**, which is [`Store::work_list`]'s
    /// restriction and its argument word for word: nothing moves a superseded round's
    /// state again, so nothing could retract it. Without this, a pull request whose
    /// author pushed a fix — the very event this group waits for — keeps its posted
    /// round here for ever, and a pull request on round four draws four rows each
    /// claiming its author owes a reply. `reviews_to_harvest` can afford to ignore that
    /// because harvesting a dead round costs one wasted call; a group the developer
    /// reads cannot.
    ///
    /// **The counts are of the author's own acts, so they read the actor and not only
    /// the kind.** `review_desk_harvest`'s `actor_for` maps a reply from any login that
    /// is not the pull request's author to [`Actor::Developer`] — which is who usually
    /// resolves a review thread, since on most repositories the author cannot — so
    /// counting `replied` rows alone credits the author with the developer's own
    /// replies. `code_changed_after` is the exception and is not counted by actor at all:
    /// the harvester attributes it to nobody, because it is an observation about the code
    /// rather than an act by a person.
    ///
    /// **It is counted `DISTINCT` by finding instead.** The harvester keys that outcome on
    /// the commit it saw the change in — `code_changed:<sha>` — so a finding whose file is
    /// touched by three successive pushes holds three rows, and counting the rows gives a
    /// figure about findings that can exceed the findings it is about.
    ///
    /// **`findings` and `findings_linked` are here so that a nought can be told from an
    /// absence.** The harvester reads a thread through a finding's
    /// `github_comment_id`, skips thread reading altogether when no finding carries
    /// one, and marks the review harvested regardless — so `harvested_at` alone cannot
    /// say whether a reply could have been seen. A review with no linked finding has
    /// nothing a reply could attach to, and zero replies is then the absence of a
    /// measurement rather than one. Deciding what to *show* is still the caller's, as
    /// this module's header says; what the store owes is the facts that decision needs.
    pub fn posted_reviews_awaiting_author(&self, repo: Option<&str>) -> Result<Vec<PostedReview>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS},
                 d.posted_at AS posted_at,
                 d.event     AS posted_as,
                 (SELECT COUNT(*) FROM findings f
                  WHERE f.review_id = v.id) AS findings,
                 (SELECT COUNT(*) FROM findings f
                  WHERE f.review_id = v.id
                    AND f.github_comment_id IS NOT NULL) AS findings_linked,
                 (SELECT COUNT(*) FROM outcomes o
                  WHERE o.review_id = v.id
                    AND o.kind = ?1 AND o.actor = ?4) AS replies,
                 (SELECT COUNT(*) FROM outcomes o
                  WHERE o.review_id = v.id
                    AND o.kind = ?2 AND o.actor = ?4) AS threads_resolved,
                 (SELECT COUNT(DISTINCT o.finding_seq) FROM outcomes o
                  WHERE o.review_id = v.id AND o.kind = ?3) AS code_changed_after
             FROM reviews v
             LEFT JOIN drafts d ON d.review_id = v.id
             WHERE (d.posted_at IS NOT NULL OR v.github_review_id IS NOT NULL)
               AND (v.pr_state IS NULL OR v.pr_state = ?5)
               AND v.round = (
                   SELECT MAX(round) FROM reviews
                   WHERE repo = v.repo AND pr = v.pr
               )
               AND (?6 IS NULL OR v.repo = ?6)
             ORDER BY d.posted_at DESC, v.id DESC"
        ))?;
        let rows = statement.query_map(
            params![
                OutcomeKind::Replied,
                OutcomeKind::ThreadResolved,
                OutcomeKind::CodeChangedAfter,
                Actor::Author,
                PrState::Open,
                repo
            ],
            |row| {
                Ok(PostedReview {
                    review: row_to_review(row)?,
                    posted_at: row.get("posted_at")?,
                    posted_as: row.get("posted_as")?,
                    findings: row.get("findings")?,
                    findings_linked: row.get("findings_linked")?,
                    response: AuthorResponse {
                        replies: row.get("replies")?,
                        threads_resolved: row.get("threads_resolved")?,
                        code_changed_after: row.get("code_changed_after")?,
                    },
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every review on the record, newest first, with the step it has reached.
    ///
    /// The Reviews list: the one place every review is named, whatever became of it, so
    /// that a review which has left the queue is still somewhere a reader can find it.
    ///
    /// **Newest first is by id**, which is the order every other query in this module
    /// serves and the order reviews were opened in. Deliberately not by
    /// `last_activity_at`: that is null for every review from before DEV-890, so a sort
    /// on it would put them in whatever order SQLite felt like. It is a column the list
    /// draws, not the column it is ordered by.
    ///
    /// **The step is the highest-numbered one that is not `pending`** — see
    /// [`ReviewAtStep::step`] for why that is not the same as the last step written, and
    /// why a review with none gets `None` rather than step 1.
    ///
    /// **The state and the note come back with the number, and a caller must draw them.**
    /// When the furthest step is finished, the number alone names a step the review has
    /// *left*: a posted review's is step 6, and a cell reading "Draft" tells a reader it
    /// still needs signing off while the queue says it is with the author. "Draft,
    /// approved by you" says the same record and misleads nobody.
    ///
    /// **Nothing in the harvest path writes step 7**, which is a gap in the record rather
    /// than in this query. `post_draft` writes the harvest step as `pending` and
    /// `review_desk_harvest` writes no step at all, so a harvest that ran leaves the step
    /// it ran as untouched — and a review that has been harvested reports step 6.
    ///
    /// A *session* may still write it: `set_review_layer` takes any step and any state,
    /// and the seed behind `contract/examples/listReviews.response.json` writes step 7
    /// `done`. So this query does report `Harvest` when the record holds it. What it will
    /// not do is derive one from `harvested_at`, which would be inventing a step state —
    /// DEV-864 settled that as the session's to write.
    ///
    /// One statement. The step comes off a `LEFT JOIN` to the layer the subquery picks,
    /// rather than seven reads per review: this is a list of the whole record, and a
    /// read per step per review is what turns it into a page that gets slower the longer
    /// the developer uses Review Desk.
    pub fn reviews_at_step(&self, repo: Option<&str>) -> Result<Vec<ReviewAtStep>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS},
                 l.number AS step_number,
                 l.state  AS step_state,
                 l.detail AS step_detail,
                 (SELECT MAX(round) FROM reviews r
                  WHERE r.repo = v.repo AND r.pr = v.pr) AS rounds_on_pr
             FROM reviews v
             LEFT JOIN review_layers l
                 ON l.review_id = v.id
                AND l.number = (
                    SELECT MAX(number) FROM review_layers
                    WHERE review_id = v.id AND state <> ?1
                )
             WHERE (?2 IS NULL OR v.repo = ?2)
             ORDER BY v.id DESC"
        ))?;
        let rows = statement
            .query_map(params![ReviewLayerState::Pending, repo], |row| {
                Ok((
                    row_to_review(row)?,
                    row.get::<_, i64>("rounds_on_pr")?,
                    row.get::<_, Option<i64>>("step_number")?,
                    row.get::<_, Option<ReviewLayerState>>("step_state")?,
                    row.get::<_, Option<String>>("step_detail")?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|(review, rounds_on_pr, number, state, detail)| {
                // Both or neither. The join either matched the layer the subquery named,
                // in which case the row is `NOT NULL` in both columns because the table
                // is, or it matched nothing at all. A number without a state would be a
                // row this build does not understand, and `for_number` is the only part
                // that can fail.
                let step = match (number, state) {
                    (Some(number), Some(state)) => Some(ReviewStep {
                        number,
                        name: ReviewLayerName::for_number(number)?,
                        state,
                        detail,
                    }),
                    _ => None,
                };
                Ok(ReviewAtStep {
                    review,
                    rounds_on_pr,
                    step,
                })
            })
            .collect()
    }

    /// Every review a session is still working on, with the layer it is on.
    ///
    /// **In progress means a layer is `running`.** A layer `waiting_on_you` is not
    /// progress — it is the developer's turn, and those reviews are what the other
    /// groups are made of, so counting them here would show every waiting review twice.
    ///
    /// **Step 7 is not a session's work, and is left out.** A harvest sets it `running`
    /// for as long as the pull request stays open (DEV-933), which is a quarter-hourly
    /// read of GitHub rather than anybody working — so counting it would put every posted
    /// review under "With the agent · nothing for you to do" for weeks, and a review whose
    /// author had replied would be listed twice on one page, which is the double-listing
    /// the paragraph above exists to prevent.
    ///
    /// The layer reported is the highest-numbered running one *of the six*: a session that
    /// left an earlier layer running is further on than that layer suggests, and the
    /// screen's "Layer 5 of 7" should say how far it got.
    pub fn reviews_in_progress(&self, repo: Option<&str>) -> Result<Vec<ReviewAtLayer>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS}, l.number, l.state, l.detail, l.updated_at
             FROM reviews v
             JOIN review_layers l ON l.review_id = v.id
             WHERE l.state = ?1
               AND l.number <> ?3
               AND (?2 IS NULL OR v.repo = ?2)
               AND l.number = (
                   SELECT MAX(number) FROM review_layers
                   WHERE review_id = v.id AND state = ?1 AND number <> ?3
               )
             ORDER BY v.id DESC"
        ))?;
        let rows = statement
            .query_map(
                params![
                    ReviewLayerState::Running,
                    repo,
                    ReviewLayerName::Harvest.number()
                ],
                |row| Ok((row_to_review(row)?, row.get::<_, i64>("number")?)),
            )?
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
///
/// `entities::work` joins the same way and reads reviews through this rather than keeping
/// a third copy of the list, which is the very drift that test exists to catch.
pub(super) fn row_to_review(row: &Row<'_>) -> rusqlite::Result<Review> {
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
        last_activity_at: row.get("last_activity_at")?,
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
