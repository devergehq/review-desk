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
    Actor, AuthorResponse, DraftStatus, LeftTheDesk, NitTicket, NitTicketStatus, OutcomeKind,
    PostedReview, RepoSummary, Review, ReviewAtLayer, ReviewAtStep, ReviewLayerName,
    ReviewLayerState, ReviewStep, ReviewsShown, RuleStage, Timestamp,
};
use crate::{normalise, Store};

/// The reviews' columns, as `entities::review` selects them, qualified for the joins
/// below.
pub(super) const REVIEW_COLUMNS: &str = "v.id, v.repo, v.pr, v.head_sha, v.round, v.title, \
                              v.ticket_id, v.ticket_url, v.changed_lines, v.not_checked, \
                              v.session_id, \
                              v.ledger_id, v.github_review_id, v.pr_state, v.pr_state_at, \
                              v.harvested_at, v.nit_ticket_status, v.nit_ticket_id, \
                              v.nit_ticket_requested_at, v.nit_ticket_filed_at, \
                              v.dismissed_at, v.dismissed_reason, v.last_activity_at, \
                              v.created_at";

/// What the pull request a review belongs to was last seen to be, as a SQL expression.
///
/// **The state is the pull request's, and the record keeps a copy of it per round.** Only
/// the round a harvest read gets its copy written, so round 1 of a merged pull request
/// still says `open` — or says nothing at all, which is every round nobody has harvested.
/// Reading `v.pr_state` alone therefore answers a question about one round's last reading
/// rather than about the pull request, and that was the whole of DEV-957: five rounds of
/// already-merged pull requests sat in "drafts to approve" because each of those rounds
/// had never been read.
///
/// **So it is the highest-numbered round that holds a state.** Not "any round that holds
/// a settled one": a pull request closed, reopened and reviewed again would be outvoted
/// for ever by the stale `closed` on an early round. And not the newest round's own
/// column: Patrick's #9568 has four rounds, one posted and read, three never read, so the
/// newest round's column is null on a pull request that merged a week ago.
///
/// Within one sweep the rounds are read oldest id first, so the highest round's reading is
/// the freshest one there is. The two readings can only disagree in the direction that
/// makes this right: GitHub cannot un-merge a pull request, and a `closed` that a later
/// round reads as `open` is a reopen, which is exactly what should win.
///
/// **`COALESCE` to `open`, and that is load-bearing rather than tidy.** A pull request no
/// harvest has looked at yields null here, and `null NOT IN (…)` is null in SQL — so a
/// `WHERE` built on the bare subquery would drop every review of every unharvested pull
/// request out of the queue, which is every review on a fresh database. Unlooked-at reads
/// as open, which is the rule [`Store::reviews_to_harvest`] has always used and says so.
macro_rules! pr_state {
    () => {
        "COALESCE((SELECT settled.pr_state FROM reviews settled
                   WHERE settled.repo = v.repo AND settled.pr = v.pr
                     AND settled.pr_state IS NOT NULL
                   ORDER BY settled.round DESC LIMIT 1), 'open')"
    };
}

/// Whether the pull request this review belongs to is finished with.
///
/// Written once so that the clause below and [`Store::reviews_at_step`]'s filter cannot
/// disagree about what finished means. The two words are checked against
/// [`PrState::is_settled`] by `the_words_in_the_finished_clause_are_the_settled_states`,
/// because a state renamed in the enum would leave this compiling and counting nothing.
pub(super) const FINISHED: &str = concat!(pr_state!(), " IN ('merged', 'closed')");

/// **Every query in this module leaves out a review that has left the desk, and this is
/// the clause that does it.** Two ways of leaving, and the same exclusion.
///
/// A dismissal is the developer saying they will not act on a review again, so it leaves
/// the queue — every group on it — as well as the work list and the Reviews list's default
/// view. Written once and pasted into each query rather than each query spelling it out,
/// because a group added later that forgot it would put a dismissed review back in front of
/// the developer and nothing would fail.
///
/// **A merged or closed pull request is the other way, and it is not a dismissal.**
/// DEV-957: on 13 October 2026 the queue held five drafts on pull requests that had
/// already merged, none of which could ever be posted, and Patrick asked whether they
/// should be dismissed or gone. Neither — finished. Dismissal stays the developer's act
/// for a review they decline; finishing is what the record already knew and nothing acted
/// on. Both belong here for the same reason the paragraph above gives, and finishing
/// belongs here *especially*: it is derived rather than stored, so a query spelling it out
/// for itself would be a second definition of when a review is over.
///
/// **Two queries here do something else, and both say so.** [`Store::reviews_at_step`]
/// takes a [`ReviewsShown`] instead, because the Reviews list is the page both filters
/// live on. And [`Store::list_repos`] applies the clause to the count it draws but *not*
/// to the set of repositories it lists: a repository whose only review has left the desk
/// has to stay in the picker, or neither the dismissed nor the finished view could be
/// reached for it. `every_query_that_lists_reviews_says_what_it_does_about_the_ones_that_have_left`
/// is what holds the rest to the rule.
pub(super) const IN_PLAY: &str = concat!(
    "v.dismissed_at IS NULL AND ",
    pr_state!(),
    " NOT IN ('merged', 'closed')"
);

/// The newest round of the pull request this review belongs to, as a SQL expression.
///
/// **An unposted round superseded by a newer one can never be the review that posts**, so
/// it is not work waiting on anybody. DEV-957's comment is the measurement: the queue
/// listed #9541 twice, round 1 and round 2 both "Draft ready", and only round 2 could ever
/// be posted — the author had pushed past round 1's commit, and a draft written against a
/// commit nobody is looking at any more is a draft with nowhere to go.
///
/// **It counts every round, dismissed or finished.** [`Store::work_list`] carries that
/// argument word for word and it holds here: filtering the subquery as well would make
/// round 1 the live round again the moment round 2 was dismissed, and round 1 is the one
/// with nothing that can move its state.
pub(super) const NEWEST_ROUND: &str = "v.round = (SELECT MAX(round) FROM reviews
                   WHERE repo = v.repo AND pr = v.pr)";

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
    ///
    /// **Nor a round a newer round has superseded**, which is [`NEWEST_ROUND`] and its
    /// argument: confirming the problem of a round whose commit the author has pushed past
    /// asks a session to carry on reviewing a commit nobody is looking at.
    pub fn reviews_with_unconfirmed_brief(&self, repo: Option<&str>) -> Result<Vec<Review>> {
        let repo = repo.map(normalise::repo);
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS} FROM reviews v
             JOIN briefs b ON b.review_id = v.id
             WHERE b.confirmed_at IS NULL
               AND b.confirmation_required = 1
               AND {IN_PLAY}
               AND {NEWEST_ROUND}
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
    ///
    /// **Only the newest round of a pull request**, which is [`NEWEST_ROUND`] and is the
    /// defect DEV-957's comment measured: #9541 drew two rows, round 1 and round 2 both
    /// "Draft ready", and signing round 1 off would have asked a session to post a review
    /// of a commit the author has pushed past. A superseded *posted* draft is not in this
    /// group anyway — its status is `posted`, not `ready` — so the restriction takes out
    /// exactly the rounds that can never post and nothing else.
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
               AND {IN_PLAY}
               AND {NEWEST_ROUND}
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
             WHERE {IN_PLAY}
               AND (?1 IS NULL OR v.repo = ?1)
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
    /// **A merged or closed pull request is not here, and [`IN_PLAY`] is now what says
    /// so.** This query carried its own `pr_state` test, and that test read the round's
    /// own column — which is written only for the round a harvest happened to read. So a
    /// pull request merged a week ago whose newest round nobody had harvested was still
    /// "waiting on the author", with no author left to wait for: DEV-957. The derived
    /// clause answers about the pull request rather than about one round's last reading,
    /// and an unlooked-at pull request still reads as open, which was the one thing the
    /// column test got right.
    ///
    /// **Only the newest round of a pull request**, which is [`NEWEST_ROUND`] and its
    /// argument: nothing moves a superseded round's state again, so nothing could retract
    /// it. Without this, a pull request whose author pushed a fix — the very event this
    /// group waits for — keeps its posted round here for ever, and a pull request on
    /// round four draws four rows each claiming its author owes a reply.
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
               AND {IN_PLAY}
               AND {NEWEST_ROUND}
               AND (?5 IS NULL OR v.repo = ?5)
             ORDER BY d.posted_at DESC, v.id DESC"
        ))?;
        let rows = statement.query_map(
            params![
                OutcomeKind::Replied,
                OutcomeKind::ThreadResolved,
                OutcomeKind::CodeChangedAfter,
                Actor::Author,
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
    /// **`shown` is the one filter in this module, and the default is the reviews in
    /// play.** A review is off the desk two ways — the developer dismissed it, or its
    /// pull request merged or closed — and the page has a position for each:
    /// [`ReviewsShown::Dismissed`] is the filter DEV-945 asks for,
    /// [`ReviewsShown::Finished`] is DEV-957's, and [`ReviewsShown::Everything`] is the
    /// whole record on one page.
    ///
    /// **Each position is a bound comparison rather than a clause built by hand.** Two
    /// `Option<bool>` parameters, one per way off the desk, each compared against the
    /// column or the derived state: `None` asks nothing about that one. So the four
    /// positions are four pairs of answers and not four queries, and a position added
    /// later cannot forget a clause the others have.
    ///
    /// **Dismissed outranks finished, and the single-row answer says so too.** A review
    /// the developer dismissed and whose pull request later merged is under **Dismissed**
    /// and not under **Finished** — the issue's own rule, that a dismissed review which
    /// then merges stays dismissed — and [`LeftTheDesk`] carries the same order so the
    /// filter and the row's own words cannot disagree.
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
    /// **A harvest writes step 7 for a review it was posted on, and that review reports
    /// it.** `post_draft` writes the step `pending`, and since DEV-933
    /// `record_harvest_step` writes what the harvest found: `running` while the pull
    /// request is open, `done` once it has settled.
    ///
    /// **A `harvested_at` does not by itself mean step 7.** A review read before DEV-933
    /// holds the stamp and no step — which is every harvested review until that change,
    /// and is what `contract/examples/listReviews.response.json` still shows. And the
    /// sweep reads every review whose pull request is open, including one still being
    /// drafted, where `record_harvest_step` answers `None` rather than claim a loop
    /// nobody has started. Either way the step reported is the one the record holds.
    ///
    /// A *session* may write it too: `set_review_layer` takes any step and any state, and
    /// the seed behind `contract/examples/listReviews.response.json` writes step 7 `done`.
    /// What this query will not do is derive one from `harvested_at`, which would be
    /// inventing a step state — DEV-864 settled that as the writer's to state, and
    /// `record_harvest_step` states it.
    ///
    /// One statement. The step comes off a `LEFT JOIN` to the layer the subquery picks,
    /// rather than seven reads per review: this is a list of the whole record, and a
    /// read per step per review is what turns it into a page that gets slower the longer
    /// the developer uses Review Desk.
    pub fn reviews_at_step(
        &self,
        repo: Option<&str>,
        shown: ReviewsShown,
    ) -> Result<Vec<ReviewAtStep>> {
        let repo = repo.map(normalise::repo);
        // One answer per way off the desk. `Some(true)` asks for only those, `Some(false)`
        // for only the rest, `None` asks nothing. **Dismissed is asked first and
        // `Finished` pairs it with `Some(false)`**, which is what keeps a dismissed review
        // whose pull request merged out of the Finished view: it stays dismissed.
        let (only_dismissed, only_finished) = match shown {
            ReviewsShown::InPlay => (Some(false), Some(false)),
            ReviewsShown::Dismissed => (Some(true), None),
            ReviewsShown::Finished => (Some(false), Some(true)),
            ReviewsShown::Everything => (None, None),
        };
        let mut statement = self.conn.prepare(&format!(
            "SELECT {REVIEW_COLUMNS},
                 l.number AS step_number,
                 l.state  AS step_state,
                 l.detail AS step_detail,
                 ({FINISHED}) AS is_finished,
                 (SELECT settled.pr_state_at FROM reviews settled
                  WHERE settled.repo = v.repo AND settled.pr = v.pr
                    AND settled.pr_state IS NOT NULL
                  ORDER BY settled.round DESC LIMIT 1) AS finished_at,
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
               AND (?3 IS NULL OR (v.dismissed_at IS NOT NULL) = ?3)
               AND (?4 IS NULL OR ({FINISHED}) = ?4)
             ORDER BY v.id DESC"
        ))?;
        let rows = statement
            .query_map(
                params![
                    ReviewLayerState::Pending,
                    repo,
                    only_dismissed,
                    only_finished
                ],
                |row| {
                    Ok((
                        row_to_review(row)?,
                        row.get::<_, i64>("rounds_on_pr")?,
                        row.get::<_, Option<i64>>("step_number")?,
                        row.get::<_, Option<ReviewLayerState>>("step_state")?,
                        row.get::<_, Option<String>>("step_detail")?,
                        row.get::<_, bool>("is_finished")?,
                        row.get::<_, Option<Timestamp>>("finished_at")?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(review, rounds_on_pr, number, state, detail, is_finished, finished_at)| {
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
                    // **Dismissed before finished, which is the order [`LeftTheDesk`] states
                    // and the same order the filter above uses.** A review the developer
                    // dismissed and whose pull request later merged says dismissed: the
                    // dismissal is an act somebody took and the merge is something that
                    // happened to it, and the issue settles it that way.
                    let left_the_desk = if review.is_dismissed() {
                        Some(LeftTheDesk::Dismissed)
                    } else if is_finished {
                        Some(LeftTheDesk::Finished)
                    } else {
                        None
                    };
                    Ok(ReviewAtStep {
                        review,
                        rounds_on_pr,
                        step,
                        left_the_desk,
                        // Only on a finished row. A dismissed review whose pull request also
                        // merged is a dismissed row, and a date for a thing the row does not
                        // say happened would be a fact with nothing to attach to.
                        finished_at: (left_the_desk == Some(LeftTheDesk::Finished))
                            .then_some(finished_at)
                            .flatten(),
                    })
                },
            )
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
               AND {IN_PLAY}
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
        // **A dismissed review's filed ticket is still listed, and that is a decision
        // this branch reversed.** The clause went in with the queue's other groups, and a
        // review of it put the two readings side by side: this is "handled without you",
        // a log of what happened in the window, and `finished_review_timings` is exempted
        // on exactly that ground — dismissing a review afterwards does not unmeasure the
        // time it took, and it does not unfile the ticket either. The groups that drop a
        // dismissed review are the ones asking what to do next; a figure about the week's
        // work that shrank when the developer tidied up would be a worse answer than the
        // one it replaced.
        let mut statement = self.conn.prepare(
            "SELECT v.id FROM reviews v
             WHERE v.nit_ticket_status = ?1
               AND (?2 IS NULL OR v.repo = ?2)
               AND (?3 IS NULL OR v.nit_ticket_filed_at >= ?3)
             ORDER BY v.id DESC",
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
                  WHERE v.repo = k.repo AND v.github_review_id IS NULL
                    AND {IN_PLAY}) AS open_reviews,
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
        ticket_url: row.get("ticket_url")?,
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
        dismissed_at: row.get("dismissed_at")?,
        dismissed_reason: row.get("dismissed_reason")?,
        last_activity_at: row.get("last_activity_at")?,
        created_at: row.get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PrState;

    /// **The two words in the finished clause are the two settled states, and nothing
    /// holds that but this.**
    ///
    /// `rules_for_repo` and `harvested_so_far` build their lists of words out of the enum
    /// for this reason, and the clause above cannot: it is a `const` that
    /// [`IN_PLAY`] is concatenated from, so the words have to be literals. A state renamed
    /// in `PrState` would leave the SQL compiling and matching nothing — every merged pull
    /// request quietly back in the queue, which is the whole of DEV-957 undone with no test
    /// failing. So the words are read back out of the clause and checked against the enum.
    #[test]
    fn the_words_in_the_finished_clause_are_the_settled_states() {
        let settled: Vec<&str> = PrState::ALL
            .iter()
            .filter(|state| state.is_settled())
            .map(|state| state.as_str())
            .collect();
        assert_eq!(
            settled,
            vec!["merged", "closed"],
            "the settled states, which is what the clause has to name"
        );

        let listed = FINISHED
            .rsplit_once("IN (")
            .expect("the clause ends in an IN list")
            .1;
        for state in &settled {
            assert!(
                listed.contains(&format!("'{state}'")),
                "{state} is settled and the finished clause does not name it: {listed}"
            );
        }
        for state in PrState::ALL.iter().filter(|state| !state.is_settled()) {
            assert!(
                !listed.contains(&format!("'{}'", state.as_str())),
                "{} is not settled and the finished clause names it: {listed}",
                state.as_str()
            );
        }
    }

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
