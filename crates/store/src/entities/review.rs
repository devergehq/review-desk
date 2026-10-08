//! Reviews: one per pull request head commit per round.

use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{Result, StoreError};
use crate::model::{now, NewReview, OpenedReview, PrState, Review, ReviewLinks, Timestamp};
use crate::{is_unique_violation, normalise, Store};

const COLUMNS: &str = "id, repo, pr, head_sha, round, session_id, ledger_id, \
                       github_review_id, pr_state, pr_state_at, harvested_at, created_at";

impl Store {
    /// Create a review.
    ///
    /// A review is unique on repository, pull request and round; a second one fails
    /// with [`StoreError::DuplicateReview`] rather than an opaque SQLite error.
    pub fn create_review(&self, new: NewReview) -> Result<Review> {
        let new = validate(new)?;

        let created_at = now();
        self.conn
            .execute(
                "INSERT INTO reviews
                     (repo, pr, head_sha, round, session_id, ledger_id, github_review_id,
                      created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new.repo,
                    new.pr,
                    new.head_sha,
                    new.round,
                    new.session_id,
                    new.ledger_id,
                    new.github_review_id,
                    created_at,
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error) {
                    StoreError::DuplicateReview {
                        repo: new.repo.clone(),
                        pr: new.pr,
                        round: new.round,
                    }
                } else {
                    StoreError::Sqlite(error)
                }
            })?;

        self.get_review(self.conn.last_insert_rowid())
    }

    /// Open the review of this head commit: the one already on the record, or a new
    /// round if this commit has never been reviewed.
    ///
    /// A head commit already reviewed is handed back — the newest round for that commit,
    /// even when later rounds exist for other commits. A round is only ever started for
    /// a commit with no review.
    ///
    /// The lookup, the round arithmetic and the insert are one step, for the reason
    /// [`Store::add_finding`] gives: two sessions opening the same pull request at the
    /// same time on two different new commits would otherwise both read the same latest
    /// round, and the second would be refused as a duplicate of a round it never asked
    /// for.
    pub fn open_review(&self, repo: &str, pr: i64, head_sha: &str) -> Result<OpenedReview> {
        // Normalised before the lookup, through the same helper `create_review` uses
        // before the insert. A lookup keyed differently from the insert would miss and
        // then start a round for a commit already reviewed.
        let (repo, head_sha) = (normalise::repo(repo), normalise::head_sha(head_sha));

        // Looked for before any lock is taken. Reopening a commit already reviewed is
        // the common call — every session's first one — and putting it behind the write
        // lock would make every reopen queue behind every writer. Worse, losing that
        // queue for the whole busy timeout arrives as `StoreError::Sqlite`, which
        // `is_refusal` calls false, so an ordinary reopen under contention would exit 2:
        // the code the review worker treats as a loud warning.
        if let Some(review) = self.find_review_by_head(&repo, pr, &head_sha)? {
            return Ok(OpenedReview {
                review,
                created: false,
            });
        }

        // Only a commit with no review needs the write lock, and then the round
        // arithmetic and the insert have to be one step: two sessions opening the same
        // pull request on two different new commits would otherwise both read the same
        // latest round, and the loser would be refused as a duplicate of a round it
        // never asked for. IMMEDIATE for the reason `add_finding` gives — SQLite will
        // not upgrade a read lock, so a DEFERRED transaction fails outright instead of
        // waiting out the busy timeout.
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;

        // Checked again now the lock is held. The miss above was unsynchronised, so
        // another caller may have created this very review in between.
        if let Some(review) = self.find_review_by_head(&repo, pr, &head_sha)? {
            tx.commit()?;
            return Ok(OpenedReview {
                review,
                created: false,
            });
        }

        let round = self.latest_round(&repo, pr)?.unwrap_or(0) + 1;
        let review = self.create_review(NewReview {
            repo,
            pr,
            head_sha,
            round,
            ..Default::default()
        })?;

        tx.commit()?;
        Ok(OpenedReview {
            review,
            created: true,
        })
    }

    /// Read a review by id. Fails with [`StoreError::NotFound`] if it is not there.
    pub fn get_review(&self, id: i64) -> Result<Review> {
        self.find_review_by_id(id)?.ok_or(StoreError::NotFound {
            entity: "review",
            id: id.to_string(),
        })
    }

    /// Read a review by id, or `None`.
    pub fn find_review_by_id(&self, id: i64) -> Result<Option<Review>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM reviews WHERE id = ?1"),
                params![id],
                row_to_review,
            )
            .optional()?)
    }

    /// The review of this repository, pull request and round, if there is one.
    pub fn find_review(&self, repo: &str, pr: i64, round: i64) -> Result<Option<Review>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM reviews
                     WHERE repo = ?1 AND pr = ?2 AND round = ?3"
                ),
                params![repo, pr, round],
                row_to_review,
            )
            .optional()?)
    }

    /// The newest review of this repository, pull request and head commit, if there is
    /// one. A caller opening a review for a head commit it has already reviewed should
    /// be handed that review back rather than starting a round.
    pub fn find_review_by_head(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
    ) -> Result<Option<Review>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM reviews
                     WHERE repo = ?1 AND pr = ?2 AND head_sha = ?3
                     ORDER BY round DESC LIMIT 1"
                ),
                params![repo, pr, head_sha],
                row_to_review,
            )
            .optional()?)
    }

    /// The highest round reached on this pull request, or `None` if it has never been
    /// reviewed. The next round is this plus one.
    pub fn latest_round(&self, repo: &str, pr: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(round) FROM reviews WHERE repo = ?1 AND pr = ?2",
                params![repo, pr],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Every review of a pull request, oldest round first.
    pub fn list_reviews(&self, repo: &str, pr: i64) -> Result<Vec<Review>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reviews
             WHERE repo = ?1 AND pr = ?2 ORDER BY round ASC"
        ))?;
        let rows = statement.query_map(params![repo, pr], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Store the other systems' ids against a review.
    ///
    /// A field left `None` is left as it is, so one call can set the session id without
    /// clearing the GitHub review id.
    pub fn set_review_links(&self, id: i64, links: ReviewLinks) -> Result<Review> {
        // Make sure the review is there, so a bad id is NotFound rather than a silent
        // update of nothing.
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET
                 session_id       = COALESCE(?2, session_id),
                 ledger_id        = COALESCE(?3, ledger_id),
                 github_review_id = COALESCE(?4, github_review_id)
             WHERE id = ?1",
            params![
                id,
                links.session_id,
                links.ledger_id,
                links.github_review_id
            ],
        )?;

        self.get_review(id)
    }

    /// Record what the pull request was last seen to be, and when it reached that
    /// state.
    ///
    /// Writing the same state twice is the same as writing it once, which is what lets
    /// the harvester run again without special-casing a review it has already seen.
    pub fn set_review_pr_state(
        &self,
        id: i64,
        state: PrState,
        state_at: Option<Timestamp>,
    ) -> Result<Review> {
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET pr_state = ?2, pr_state_at = ?3 WHERE id = ?1",
            params![id, state, state_at],
        )?;

        self.get_review(id)
    }

    /// Record that the harvester has just read this review.
    pub fn mark_review_harvested(&self, id: i64, at: Timestamp) -> Result<Review> {
        self.get_review(id)?;

        self.conn.execute(
            "UPDATE reviews SET harvested_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;

        self.get_review(id)
    }

    /// The reviews whose pull request was open at their last harvest, oldest first.
    ///
    /// A review never harvested qualifies: its state is unknown, so it has to be
    /// looked at. A review whose pull request was merged or closed does not — there is
    /// nothing further to learn, and fetching it again would cost a GitHub call per
    /// run for ever.
    pub fn reviews_to_harvest(&self, repo: Option<&str>) -> Result<Vec<Review>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reviews
             WHERE (pr_state IS NULL OR pr_state = 'open')
               AND (?1 IS NULL OR repo = ?1)
             ORDER BY id ASC"
        ))?;
        let rows = statement.query_map(params![repo], row_to_review)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Check and normalise the identity the rest of the store keys on.
///
/// The store is the only way into the record, so it is the only place that can hold
/// this line. A review whose repository is empty or malformed would silently partition
/// the record: rules are scoped by the same string, and `rules_for_repo` would never
/// match it.
///
/// The repository and head commit come back canonical, through [`crate::normalise`],
/// which is where the argument for that lives. Checking one form and storing another is
/// the whole bug: it would make `abc ` — or `ABC` — a different commit from `abc`.
fn validate(new: NewReview) -> Result<NewReview> {
    let repo = normalise::repo(&new.repo);
    let repo = repo.as_str();
    let (owner, name) = repo.split_once('/').unwrap_or(("", ""));
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return Err(StoreError::InvalidValue {
            field: "repo",
            value: new.repo.clone(),
            expected: "a repository as owner/repo".to_string(),
        });
    }

    if new.pr < 1 {
        return Err(StoreError::InvalidValue {
            field: "pr",
            value: new.pr.to_string(),
            expected: "a pull request number of 1 or more".to_string(),
        });
    }

    if new.round < 1 {
        return Err(StoreError::InvalidValue {
            field: "round",
            value: new.round.to_string(),
            expected: "a round of 1 or more".to_string(),
        });
    }

    if new.head_sha.trim().is_empty() {
        return Err(StoreError::InvalidValue {
            field: "head_sha",
            value: new.head_sha.clone(),
            expected: "the head commit being reviewed".to_string(),
        });
    }

    Ok(NewReview {
        repo: repo.to_string(),
        head_sha: normalise::head_sha(&new.head_sha),
        ..new
    })
}

/// Build a review from a row selecting [`COLUMNS`].
fn row_to_review(row: &Row<'_>) -> rusqlite::Result<Review> {
    Ok(Review {
        id: row.get("id")?,
        repo: row.get("repo")?,
        pr: row.get("pr")?,
        head_sha: row.get("head_sha")?,
        round: row.get("round")?,
        session_id: row.get("session_id")?,
        ledger_id: row.get("ledger_id")?,
        github_review_id: row.get("github_review_id")?,
        pr_state: row.get("pr_state")?,
        pr_state_at: row.get("pr_state_at")?,
        harvested_at: row.get("harvested_at")?,
        created_at: row.get("created_at")?,
    })
}
