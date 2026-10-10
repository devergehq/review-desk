//! How long a review took, and how long each of its seven steps took.
//!
//! **Nothing here is stored.** Every figure is derived from the append-only record of
//! each layer's state changes, which `entities::layer` writes, and from the two times the
//! draft already held. That is deliberate: a stored total and the times it was computed
//! from are two records of one fact, and this repository has already paid for one of
//! those — a screen that could render two coverage records side by side could render a
//! disagreement, and so could a page showing a total next to the runs it came from.
//!
//! The four questions DEV-890 exists to answer, and where each is answered:
//!
//! | Question | Here |
//! |---|---|
//! | How long was the review with the agent? | [`ReviewTimings::with_the_agent_seconds`] |
//! | How long did it wait on the developer? | [`ReviewTimings::waiting_on_you_seconds`] |
//! | How long did each step take? | [`crate::ReviewLayer::seconds`], over every pass |
//! | Is the session still alive? | [`ReviewTimings::last_activity_at`] |

use rusqlite::params;

use crate::error::Result;
use crate::model::{
    Review, ReviewLayer, ReviewLayerName, ReviewLayerRun, ReviewTimings, Timestamp,
};
use crate::Store;

impl Store {
    /// How long one review has taken, with all seven layers and their passes.
    ///
    /// One read of the record per layer plus two rows, which is what keeps "each screen
    /// is one GET" true of the review page.
    pub fn review_timings(&self, review_id: i64) -> Result<ReviewTimings> {
        self.timings_of(&self.get_review(review_id)?)
    }

    /// The same, for a review already read.
    ///
    /// The list below reads its reviews in one query and would otherwise read each of them
    /// again by id, which is a second read of a row it is holding.
    fn timings_of(&self, review: &Review) -> Result<ReviewTimings> {
        let draft = self.find_draft(review.id)?;
        let layers = self.list_review_layers(review.id)?;

        Ok(timings_from(
            review.id,
            review.created_at,
            review.last_activity_at,
            draft.as_ref().and_then(|draft| draft.approved_at),
            draft.as_ref().and_then(|draft| draft.posted_at),
            layers,
        ))
    }

    /// Every finished review's timings, newest first.
    ///
    /// **Finished means posted**: a session has reported the review on GitHub, so nothing
    /// further will move its figures. It is the set statistics should be computed over,
    /// and the reason this read exists at all — DEV-890 makes the numbers available in one
    /// read and deliberately builds no statistics page over them.
    ///
    /// A review from before migration 12 is here too, with its figures absent. Leaving it
    /// out would make "how long does a review take" quietly a question about the reviews
    /// recorded since this change, which is a different question and not the one anybody
    /// asked.
    pub fn finished_review_timings(
        &self,
        repo: Option<&str>,
    ) -> Result<Vec<(Review, ReviewTimings)>> {
        let repo = repo.map(crate::normalise::repo);
        // The reviews themselves, not their ids: every caller wants both, and the join is
        // already selecting the rows.
        let mut statement = self.conn.prepare(&format!(
            "SELECT {columns} FROM reviews v
             JOIN drafts d ON d.review_id = v.id
             WHERE d.posted_at IS NOT NULL
               AND (?1 IS NULL OR v.repo = ?1)
             ORDER BY d.posted_at DESC, v.id DESC",
            columns = super::inbox::REVIEW_COLUMNS,
        ))?;
        let reviews = statement
            .query_map(params![repo], super::inbox::row_to_review)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        reviews
            .into_iter()
            .map(|review| {
                let timings = self.timings_of(&review)?;
                Ok((review, timings))
            })
            .collect()
    }

    /// When this review's draft was last ready for the developer.
    ///
    /// The queue's "ready 20 min ago", which nothing could answer before: the draft holds
    /// `approved_at`, `sent_back_at` and `posted_at`, and had no time for the one state
    /// the developer is actually waiting in. It is layer 6 entering `waiting_on_you`, so
    /// it comes out of the same record as everything else here rather than from a column
    /// that could disagree with the trail.
    pub fn draft_ready_at(&self, review_id: i64) -> Result<Option<Timestamp>> {
        Ok(last_draft_ready(&self.list_review_layers(review_id)?))
    }
}

/// When the draft was last ready for the developer.
///
/// Layer 6 entering `waiting_on_you`, and the **last** time it did: a draft sent back and
/// redrafted is ready again, and the one that matters is the one the developer was last
/// shown.
fn last_draft_ready(layers: &[ReviewLayer]) -> Option<Timestamp> {
    layers
        .iter()
        .find(|layer| layer.name == ReviewLayerName::Draft)?
        .runs
        .iter()
        .rfind(|run| run.is_waiting_on_you())
        .and_then(|run| run.started_at)
}

/// Build the timings from what the record holds.
///
/// A free function over its inputs rather than a method, so the arithmetic can be tested
/// without a database — which is where the two figures that subtract one span from
/// another are easy to get wrong.
fn timings_from(
    review_id: i64,
    requested_at: Timestamp,
    last_activity_at: Option<Timestamp>,
    signed_off_at: Option<Timestamp>,
    posted_at: Option<Timestamp>,
    layers: Vec<ReviewLayer>,
) -> ReviewTimings {
    let draft_ready_at = last_draft_ready(&layers);

    // Every wait on the developer, over every layer: the brief's confirmation at layer 2,
    // and the draft's sign-off at layer 6.
    let waits: Vec<&ReviewLayerRun> = layers
        .iter()
        .flat_map(|layer| layer.runs.iter())
        .filter(|run| run.is_waiting_on_you())
        .collect();

    // The total counts the waits that have **finished**, for the reason
    // `ReviewLayer::seconds` gives: a figure that grows on every reload is not an answer
    // to "how long did it wait".
    let finished: Vec<i64> = waits.iter().filter_map(|run| run.seconds()).collect();
    let waiting_on_you_seconds = (!finished.is_empty()).then(|| finished.iter().sum::<i64>());

    // Requested to draft ready, less the developer's own time **inside that span**.
    //
    // **The overlap, not the finished waits.** A session may draft before the brief is
    // confirmed — the Inbox supports exactly that, and says so with `blocks_later_layers`
    // — so a wait on the developer can still be open when the draft becomes ready. Its
    // own length is unknown, but the part of it that fell inside this span is not: it ran
    // from where it started to the moment the draft was ready. Counting only finished
    // waits would charge the developer's time to the agent on the one path where the two
    // overlap.
    //
    // **Null until the draft is ready**: there is no end to measure to, and measuring to
    // now would make the figure grow while nothing happened.
    let with_the_agent_seconds = draft_ready_at.map(|ready| {
        let waited: i64 = waits
            .iter()
            .filter_map(|run| {
                let from = run.started_at?.max(requested_at);
                let to = run.finished_at.unwrap_or(ready).min(ready);
                Some((to - from).max(0))
            })
            .sum();
        // Never negative. Two writes a clock moved backwards between would otherwise
        // serve a duration that cannot have happened.
        (ready - requested_at - waited).max(0)
    });

    ReviewTimings {
        review_id,
        requested_at,
        draft_ready_at,
        signed_off_at,
        posted_at,
        with_the_agent_seconds,
        waiting_on_you_seconds,
        last_activity_at,
        layers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ReviewLayerName, ReviewLayerState};

    /// A layer carrying the passes a test wants, with the rest of it left plain.
    fn layer(number: i64, runs: Vec<ReviewLayerRun>) -> ReviewLayer {
        ReviewLayer {
            review_id: 1,
            number,
            name: ReviewLayerName::for_number(number).expect("a layer number"),
            state: ReviewLayerState::Done,
            detail: None,
            updated_at: None,
            runs,
        }
    }

    /// A finished pass in one state.
    fn pass(state: ReviewLayerState, from: Timestamp, to: Timestamp) -> ReviewLayerRun {
        ReviewLayerRun {
            state: Some(state),
            started_at: Some(from),
            ended_as: Some(ReviewLayerState::Done),
            finished_at: Some(to),
            detail: None,
        }
    }

    /// A pass that is still going.
    fn going(state: ReviewLayerState, from: Timestamp) -> ReviewLayerRun {
        ReviewLayerRun {
            state: Some(state),
            started_at: Some(from),
            ended_as: None,
            finished_at: None,
            detail: None,
        }
    }

    #[test]
    fn time_with_the_agent_leaves_out_the_developers_own_wait() {
        // Requested at 0. The brief waited on the developer for 600 seconds and was
        // confirmed at 900; the draft was ready at 1,800. So 1,800 seconds passed and 600
        // of them were the developer's.
        let timings = timings_from(
            1,
            0,
            None,
            None,
            None,
            vec![
                layer(2, vec![pass(ReviewLayerState::WaitingOnYou, 300, 900)]),
                layer(6, vec![going(ReviewLayerState::WaitingOnYou, 1_800)]),
            ],
        );

        assert_eq!(timings.draft_ready_at, Some(1_800));
        assert_eq!(timings.with_the_agent_seconds, Some(1_200));
        assert_eq!(
            timings.waiting_on_you_seconds,
            Some(600),
            "the wait still going is not counted: it has no length yet"
        );
    }

    #[test]
    fn a_review_whose_draft_is_not_ready_has_no_figure_for_the_agent() {
        let timings = timings_from(
            1,
            0,
            None,
            None,
            None,
            vec![layer(5, vec![going(ReviewLayerState::Running, 60)])],
        );

        assert_eq!(timings.draft_ready_at, None);
        assert_eq!(
            timings.with_the_agent_seconds, None,
            "not the time so far: a total that grows on every reload is not a total"
        );
        assert_eq!(timings.waiting_on_you_seconds, None);
    }

    #[test]
    fn a_review_from_before_this_change_reports_nothing_rather_than_nought() {
        // What migration 12 backfilled: one finish per layer, no starts anywhere.
        let finished = |number| {
            layer(
                number,
                vec![ReviewLayerRun {
                    state: None,
                    started_at: None,
                    ended_as: Some(ReviewLayerState::Done),
                    finished_at: Some(1_760_000_000),
                    detail: None,
                }],
            )
        };
        let timings = timings_from(
            1,
            1_759_000_000,
            None,
            Some(1_760_000_100),
            Some(1_760_000_200),
            (1..=7).map(finished).collect(),
        );

        assert_eq!(timings.draft_ready_at, None, "no start was ever recorded");
        assert_eq!(timings.with_the_agent_seconds, None);
        assert_eq!(timings.waiting_on_you_seconds, None);
        assert!(
            timings.layers.iter().all(|layer| layer.seconds().is_none()),
            "and no step claims a length either"
        );
        assert_eq!(
            timings.signed_off_at,
            Some(1_760_000_100),
            "what the draft did record is still served"
        );
    }

    #[test]
    fn a_wait_still_open_when_the_draft_was_ready_is_not_the_agents_time() {
        // **The path the Inbox supports**: a session drafts before the brief is confirmed,
        // so the developer's wait at step 2 is still open when the draft becomes ready.
        // Requested at 0, the developer waited from 600, the draft was ready at 1,800 and
        // nobody has confirmed anything — so 1,200 of those 1,800 seconds were theirs.
        let timings = timings_from(
            1,
            0,
            None,
            None,
            None,
            vec![
                layer(2, vec![going(ReviewLayerState::WaitingOnYou, 600)]),
                layer(6, vec![going(ReviewLayerState::WaitingOnYou, 1_800)]),
            ],
        );

        assert_eq!(timings.draft_ready_at, Some(1_800));
        assert_eq!(
            timings.with_the_agent_seconds,
            Some(600),
            "the 1,200 seconds the developer has had it are not the agent's"
        );
        assert_eq!(
            timings.waiting_on_you_seconds, None,
            "and no wait has finished, so there is no total to give"
        );
    }

    #[test]
    fn the_draft_ready_time_is_the_last_one_a_redraft_produced() {
        let timings = timings_from(
            1,
            0,
            None,
            None,
            None,
            vec![layer(
                6,
                vec![
                    pass(ReviewLayerState::Running, 0, 100),
                    pass(ReviewLayerState::WaitingOnYou, 100, 200),
                    pass(ReviewLayerState::Running, 200, 300),
                    going(ReviewLayerState::WaitingOnYou, 300),
                ],
            )],
        );

        assert_eq!(
            timings.draft_ready_at,
            Some(300),
            "the draft the developer is looking at now, not the one they sent back"
        );
        assert_eq!(
            timings.with_the_agent_seconds,
            Some(200),
            "300 seconds, less the 100 the developer had it"
        );
    }
}
