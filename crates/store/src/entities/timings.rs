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
use crate::model::{ReviewLayer, ReviewLayerName, ReviewTimings, Timestamp};
use crate::Store;

impl Store {
    /// How long one review has taken, with all seven layers and their passes.
    ///
    /// One read of the record per layer plus two rows, which is what keeps "each screen
    /// is one GET" true of the review page.
    pub fn review_timings(&self, review_id: i64) -> Result<ReviewTimings> {
        let review = self.get_review(review_id)?;
        let draft = self.find_draft(review_id)?;
        let layers = self.list_review_layers(review_id)?;

        Ok(timings_from(
            review_id,
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
    /// A review from before migration 11 is here too, with its figures absent. Leaving it
    /// out would make "how long does a review take" quietly a question about the reviews
    /// recorded since this change, which is a different question and not the one anybody
    /// asked.
    pub fn finished_review_timings(&self, repo: Option<&str>) -> Result<Vec<ReviewTimings>> {
        let repo = repo.map(crate::normalise::repo);
        let mut statement = self.conn.prepare(
            "SELECT v.id FROM reviews v
             JOIN drafts d ON d.review_id = v.id
             WHERE d.posted_at IS NOT NULL
               AND (?1 IS NULL OR v.repo = ?1)
             ORDER BY d.posted_at DESC, v.id DESC",
        )?;
        let ids = statement
            .query_map(params![repo], |row| row.get::<_, i64>("id"))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        ids.into_iter().map(|id| self.review_timings(id)).collect()
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

    // Every wait on the developer that has finished, over every layer: the brief's
    // confirmation at layer 2, and the draft's sign-off at layer 6. A wait still going is
    // left out for the reason `ReviewLayer::seconds` gives — a figure that grows on every
    // reload is not an answer to "how long did it wait".
    let waits: Vec<(Timestamp, i64)> = layers
        .iter()
        .flat_map(|layer| layer.runs.iter())
        .filter(|run| run.is_waiting_on_you())
        .filter_map(|run| run.finished_at.zip(run.seconds()))
        .collect();

    let waiting_on_you_seconds =
        (!waits.is_empty()).then(|| waits.iter().map(|(_, seconds)| seconds).sum::<i64>());

    // Requested to draft ready, less the waits that finished inside that span — which in
    // practice is the brief's confirmation. **Null until the draft is ready**: there is no
    // end to measure to, and measuring to now would make the figure grow while nothing
    // happened.
    let with_the_agent_seconds = draft_ready_at.map(|ready| {
        let waited: i64 = waits
            .iter()
            .filter(|(finished, _)| *finished <= ready)
            .map(|(_, seconds)| seconds)
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
    use crate::model::{ReviewLayerName, ReviewLayerRun, ReviewLayerState};

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
        // What migration 11 backfilled: one finish per layer, no starts anywhere.
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
