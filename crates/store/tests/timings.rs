//! When each of a review's seven steps started and finished.
//!
//! **Every test here opens the store with a clock it can move**, which is the only way a
//! duration can be asserted at all: a test for "a step that ran twice reports both runs
//! and their total" would otherwise have to sleep for the total. See
//! `review_desk_store::clock` for why that is a value the store is opened with rather than
//! an argument on every write — a write that took a time could backdate a step, and
//! `no_write_takes_a_time_for_a_step` below is the assertion that none does.

mod common;

use common::{db_path, new_review};
use review_desk_store::{
    newest_version, Actor, Clock, Decision, DraftStatus, FindingDecision, NewBrief, NewDraft,
    NewFinding, NewOutcome, NewReviewSource, NewRule, OutcomeKind, PrState, Review, ReviewEvent,
    ReviewFacts, ReviewLayerState, ReviewLinks, Severity, SourceKind, Store, Timestamp,
};
use tempfile::TempDir;

/// A Friday afternoon in October 2026, as seconds since the epoch. Any instant would do;
/// a real one keeps the figures in the failure messages readable.
const START: Timestamp = 1_760_000_000;

/// A store whose clock the test holds, in a fresh temporary directory.
fn stopped_store() -> (TempDir, Store, Clock) {
    let dir = TempDir::new().expect("temporary directory");
    let clock = Clock::fixed(START);
    let store = Store::open_with_clock(db_path(&dir), clock.clone()).expect("open store");
    (dir, store, clock)
}

/// A review with a brief, both written at the clock's current time.
fn opened(store: &Store, pr: i64) -> Review {
    let review = store
        .create_review(new_review("acme/portal", pr, "3f9c2ab", 1))
        .expect("a review");
    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "a webhook delivery that fails once is never tried again".to_string(),
                ..Default::default()
            },
        )
        .expect("a brief");
    review
}

#[test]
fn a_step_worked_on_and_finished_reports_how_long_it_took() {
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 412);

    store
        .set_review_layer(review.id, 5, ReviewLayerState::Running, Some("reading"))
        .expect("the session starts the code review");
    clock.advance(17 * 60);
    store
        .set_review_layer(review.id, 5, ReviewLayerState::Done, Some("3 findings"))
        .expect("and finishes it");

    let layer = store.get_review_layer(review.id, 5).expect("the layer");
    assert_eq!(layer.runs.len(), 1);
    assert_eq!(layer.runs[0].started_at, Some(START));
    assert_eq!(layer.runs[0].finished_at, Some(START + 17 * 60));
    assert_eq!(layer.runs[0].ended_as, Some(ReviewLayerState::Done));
    assert_eq!(layer.seconds(), Some(17 * 60), "in whole seconds");
}

#[test]
fn reporting_the_same_state_again_does_not_start_a_second_run() {
    // **`set_review_layer` is deliberately idempotent** — "writing the same state twice is
    // the same as writing it once, except that the time moves" — so a session reporting
    // progress is one run that is still going, not one run per report. This is the
    // assertion that fails if a row is ever appended on every write.
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 413);

    for reported in ["reading the diff", "3 files in", "9 files in"] {
        store
            .set_review_layer(review.id, 5, ReviewLayerState::Running, Some(reported))
            .expect("progress");
        clock.advance(120);
    }

    let layer = store.get_review_layer(review.id, 5).expect("the layer");
    assert_eq!(
        layer.runs.len(),
        1,
        "one run, three reports: {:?}",
        layer.runs
    );
    assert_eq!(
        layer.runs[0].started_at,
        Some(START),
        "from the first report"
    );
    assert_eq!(
        layer.detail.as_deref(),
        Some("9 files in"),
        "and the latest detail still landed on the row"
    );
    assert_eq!(layer.seconds(), None, "a run still going has no length yet");
    assert_eq!(layer.in_state_since(), Some(START));
}

#[test]
fn a_draft_sent_back_and_redrafted_reports_both_runs_and_their_total() {
    // The sequence the issue names, through the functions that really write it: the
    // session drafts, the developer sends it back, the session drafts again, and the
    // developer signs it off.
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 414);

    store
        .set_review_layer(review.id, 6, ReviewLayerState::Running, Some("drafting"))
        .expect("the session starts the draft");
    clock.advance(10 * 60);
    store
        .put_draft(review.id, NewDraft::ready("the first draft"))
        .expect("the draft is ready");

    clock.advance(5 * 60);
    store
        .send_back_draft(review.id, "F2 is the same point as F1; cut one of them")
        .expect("the developer sends it back");

    clock.advance(6 * 60);
    store
        .put_draft(review.id, NewDraft::ready("the second draft"))
        .expect("the draft is ready again");

    clock.advance(60);
    store
        .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("the developer signs it off");

    let layer = store.get_review_layer(review.id, 6).expect("layer 6");
    let working: Vec<i64> = layer
        .runs
        .iter()
        .filter(|run| run.state == Some(ReviewLayerState::Running))
        .filter_map(|run| run.seconds())
        .collect();
    assert_eq!(
        working,
        vec![10 * 60, 6 * 60],
        "both runs of the draft, not the last one: {:?}",
        layer.runs
    );
    assert_eq!(
        layer.seconds(),
        Some((10 + 5 + 6 + 1) * 60),
        "and the step's total is every run it made, the developer's own waits included"
    );
}

#[test]
fn the_review_splits_its_time_between_the_agent_and_the_developer() {
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 415);

    // Layer 2 waits on the developer for ten minutes.
    clock.advance(4 * 60);
    store
        .set_review_layer(review.id, 2, ReviewLayerState::WaitingOnYou, None)
        .expect("the brief is ready");
    clock.advance(10 * 60);
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer confirms it");

    // Then the session works for twenty minutes and the draft is ready.
    clock.advance(20 * 60);
    store
        .put_draft(review.id, NewDraft::ready("the draft"))
        .expect("the draft is ready");

    let timings = store.review_timings(review.id).expect("the timings");
    assert_eq!(timings.requested_at, START);
    assert_eq!(timings.draft_ready_at, Some(START + 34 * 60));
    assert_eq!(
        timings.waiting_on_you_seconds,
        Some(10 * 60),
        "the brief's confirmation"
    );
    assert_eq!(
        timings.with_the_agent_seconds,
        Some(24 * 60),
        "thirty-four minutes to draft ready, less the ten the developer had it"
    );

    // And once the developer signs it off, their second wait is counted too.
    clock.advance(3 * 60);
    store
        .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("signed off");
    let timings = store.review_timings(review.id).expect("the timings again");
    assert_eq!(timings.waiting_on_you_seconds, Some(13 * 60));
    assert_eq!(
        timings.with_the_agent_seconds,
        Some(24 * 60),
        "which does not change what the agent took: that span is closed"
    );
    assert_eq!(timings.signed_off_at, Some(START + 37 * 60));
}

#[test]
fn a_review_nothing_has_drafted_has_no_figure_for_the_agent() {
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 416);
    clock.advance(9 * 60);
    store
        .set_review_layer(review.id, 5, ReviewLayerState::Running, None)
        .expect("the session is reading");

    let timings = store.review_timings(review.id).expect("the timings");
    assert_eq!(timings.draft_ready_at, None);
    assert_eq!(
        timings.with_the_agent_seconds, None,
        "not nine minutes: the span has no end yet, and a total that grows on every \
         reload is not a total"
    );
    assert_eq!(
        timings.waiting_on_you_seconds, None,
        "and nobody has waited"
    );
}

#[test]
fn the_finished_review_list_is_one_read_and_holds_the_totals() {
    let (_dir, store, clock) = stopped_store();

    // One review carried all the way to posted, and one left with the developer.
    let posted = opened(&store, 417);
    clock.advance(60);
    store
        .put_draft(posted.id, NewDraft::ready("the draft"))
        .expect("ready");
    clock.advance(60);
    store
        .approve_draft(posted.id, ReviewEvent::Comment, Actor::Developer)
        .expect("signed off");
    clock.advance(30);
    store
        .post_draft(posted.id, "gh-review-3312")
        .expect("the session posts it");

    let waiting = opened(&store, 418);
    store
        .put_draft(waiting.id, NewDraft::ready("a draft nobody has signed"))
        .expect("ready");

    let finished = store
        .finished_review_timings(None)
        .expect("the finished reviews");
    assert_eq!(
        finished
            .iter()
            .map(|(review, _)| review.id)
            .collect::<Vec<_>>(),
        vec![posted.id],
        "finished means posted: a draft waiting to be signed off is not a measurement yet"
    );
    let (review, timings) = &finished[0];
    assert_eq!(
        review.title.as_deref(),
        None,
        "the review comes back with the figures, so no caller reads it twice"
    );
    assert_eq!(timings.review_id, review.id);
    assert_eq!(timings.with_the_agent_seconds, Some(60));
    assert_eq!(timings.waiting_on_you_seconds, Some(60));
    assert_eq!(timings.posted_at, Some(START + 150));
    assert_eq!(
        timings.layers.len(),
        7,
        "all seven steps, so a per-step figure can be computed from this read alone"
    );

    // And it is filtered by repository, like every other list in the store.
    assert!(store
        .finished_review_timings(Some("other/repo"))
        .expect("another repository")
        .is_empty());
}

#[test]
fn a_write_moves_the_last_activity_time_and_a_read_does_not() {
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 419);
    assert_eq!(
        store.get_review(review.id).unwrap().last_activity_at,
        Some(START),
        "opening a review is itself a write"
    );

    clock.advance(600);
    for _ in 0..3 {
        store.review_timings(review.id).expect("a read");
        store.full_review(review.id).expect("another read");
        store.list_review_layers(review.id).expect("and another");
    }
    assert_eq!(
        store.get_review(review.id).unwrap().last_activity_at,
        Some(START),
        "reading the record is not activity on it"
    );

    store
        .set_review_layer(review.id, 5, ReviewLayerState::Running, None)
        .expect("a write");
    assert_eq!(
        store.get_review(review.id).unwrap().last_activity_at,
        Some(START + 600)
    );
}

#[test]
fn review_writes_move_the_last_activity_time() {
    // **Every write the command line can make, one after another.** A write added later
    // that forgets to move the activity time makes the queue say a live session has been
    // silent for an hour, and nothing but a list like this one catches it: there is no
    // type that makes a write to a review announce itself.
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 420);
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "a retried delivery carries no idempotency key".to_string(),
                // The comment and the consequence, because the developer's decision
                // further down refuses a finding with nothing to post.
                inline_comment: Some("A retry sends the event again with a new id.".to_string()),
                why_it_matters: Some("The customer receives the delivery twice.".to_string()),
                ..Default::default()
            },
        )
        .expect("a finding");

    let rule = store
        .create_rule(NewRule {
            statement: "a retry carries the key of the delivery it repeats".to_string(),
            prevents: "a duplicate delivery".to_string(),
            scope: review_desk_store::RuleScope::Global,
            stage: review_desk_store::RuleStage::Proposed,
            origin_review_id: Some(review.id),
            origin_findings: Vec::new(),
            why_proposed: None,
            note: None,
        })
        .expect("a rule");

    type Write<'a> = (&'a str, Box<dyn Fn() + 'a>);
    let id = review.id;
    let writes: Vec<Write> = vec![
        (
            "review facts",
            Box::new(|| {
                store
                    .set_review_facts(
                        id,
                        &ReviewFacts {
                            title: Some("Retry failed webhook deliveries".to_string()),
                            ..Default::default()
                        },
                    )
                    .map(|_| ())
                    .expect("facts")
            }),
        ),
        (
            "review links",
            Box::new(|| {
                store
                    .set_review_links(
                        id,
                        ReviewLinks {
                            session_id: Some("ses_1".to_string()),
                            ..Default::default()
                        },
                    )
                    .map(|_| ())
                    .expect("links")
            }),
        ),
        (
            "the brief",
            Box::new(|| {
                store
                    .put_brief(
                        id,
                        NewBrief {
                            problem: "a second reading of the same change".to_string(),
                            ..Default::default()
                        },
                    )
                    .map(|_| ())
                    .expect("a brief")
            }),
        ),
        (
            "the brief's confirmation",
            Box::new(|| {
                store
                    .confirm_brief(id, Actor::Developer)
                    .map(|_| ())
                    .expect("confirmed")
            }),
        ),
        (
            "a finding's disposition",
            Box::new(|| {
                store
                    .set_finding_disposition(id, 1, review_desk_store::Disposition::InDraft, None)
                    .map(|_| ())
                    .expect("a disposition")
            }),
        ),
        (
            "a finding's round status",
            Box::new(|| {
                store
                    .set_finding_round_status(
                        id,
                        1,
                        &review_desk_store::FindingCarry {
                            status: review_desk_store::RoundStatus::StillOpen,
                            by: review_desk_store::DecidedBy::Agent,
                            checked: Some("still there at the same line".to_string()),
                            commit: None,
                        },
                    )
                    .map(|_| ())
                    .expect("a status")
            }),
        ),
        (
            "a finding's GitHub comment",
            Box::new(|| {
                store
                    .set_finding_github_comment(id, 1, "gh-comment-1")
                    .map(|_| ())
                    .expect("a comment id")
            }),
        ),
        (
            "an outcome",
            Box::new(|| {
                store
                    .add_outcome(
                        review_desk_store::FindingRef {
                            review_id: id,
                            seq: 1,
                        },
                        NewOutcome {
                            kind: OutcomeKind::Replied,
                            actor: Actor::Author,
                            detail: Some("the key is added in the next commit".to_string()),
                            occurred_at: None,
                        },
                    )
                    .map(|_| ())
                    .expect("an outcome")
            }),
        ),
        (
            "the review's sources",
            Box::new(|| {
                store
                    .put_review_sources(
                        id,
                        vec![NewReviewSource {
                            kind: SourceKind::Ticket,
                            label: "ABC-231".to_string(),
                            link: None,
                        }],
                    )
                    .map(|_| ())
                    .expect("sources")
            }),
        ),
        (
            "the draft",
            Box::new(|| {
                store
                    .put_draft(id, NewDraft::ready("### Blocker · F1 · the finding"))
                    .map(|_| ())
                    .expect("a draft")
            }),
        ),
        (
            "a finding's decision",
            Box::new(|| {
                store
                    .decide_finding(
                        id,
                        1,
                        &Decision {
                            decision: FindingDecision::Keep,
                            severity: None,
                            title: None,
                            text: None,
                            note: None,
                        },
                        Actor::Developer,
                    )
                    .map(|_| ())
                    .expect("a decision")
            }),
        ),
        (
            "the draft's approval",
            Box::new(|| {
                store
                    .approve_draft(id, ReviewEvent::Comment, Actor::Developer)
                    .map(|_| ())
                    .expect("approved")
            }),
        ),
        (
            "the posted review",
            Box::new(|| {
                store
                    .post_draft(id, "gh-review-3312")
                    .map(|_| ())
                    .expect("posted")
            }),
        ),
        (
            "the ticket for the nits",
            Box::new(|| {
                store
                    .request_nit_ticket(id, None)
                    .map(|_| ())
                    .expect("a ticket asked for")
            }),
        ),
        (
            "the pull request's state",
            Box::new(|| {
                store
                    .set_review_pr_state(id, PrState::Merged, Some(START))
                    .map(|_| ())
                    .expect("merged")
            }),
        ),
        (
            "the harvest",
            Box::new(|| {
                store
                    .mark_review_harvested(id, START)
                    .map(|_| ())
                    .expect("harvested")
            }),
        ),
        (
            "a layer",
            Box::new(|| {
                store
                    .set_review_layer(id, 7, ReviewLayerState::Done, Some("1 outcome"))
                    .map(|_| ())
                    .expect("a layer")
            }),
        ),
    ];

    for (what, write) in writes {
        let before = store.get_review(id).unwrap().last_activity_at;
        let at = clock.advance(60);
        write();
        assert_eq!(
            store.get_review(id).unwrap().last_activity_at,
            Some(at),
            "writing {what} has to move the review's last-activity time (it was {before:?})"
        );
    }

    // A rule is not a review, and promoting one is not activity on the review it came
    // from: the developer deciding a rule says nothing about whether that review's
    // session is alive.
    let before = store.get_review(id).unwrap().last_activity_at;
    clock.advance(60);
    store
        .set_rule_stage(rule.id, review_desk_store::RuleStage::Active, None)
        .expect("promoted");
    assert_eq!(
        store.get_review(id).unwrap().last_activity_at,
        before,
        "a rule's stage is not activity on the review it was proposed from"
    );
}

#[test]
fn no_write_takes_a_time_for_a_step() {
    // **The assertion that the store stamps the clock.** There is no argument to pass, so
    // what a test can do is prove that a caller holding a time of its own cannot get it
    // into the record: the store's clock is stopped at START, the test's own clock reads a
    // year later, and the record says START.
    //
    // The command line and the HTTP API are checked in their own crates —
    // `crates/cli/tests/review.rs` tries the arguments, and
    // `crates/server/tests/reviews.rs` sends the fields.
    let (_dir, store, _clock) = stopped_store();
    let review = opened(&store, 421);
    let a_year_later = START + 365 * 24 * 60 * 60;

    store
        .set_review_layer(review.id, 5, ReviewLayerState::Running, Some("reading"))
        .expect("running");
    store
        .set_review_layer(
            review.id,
            5,
            ReviewLayerState::Done,
            // The nearest thing to supplying a time: writing one into the only free-text
            // field the call takes. It lands as text and moves no figure.
            Some("3 findings, at 2027-10-09T12:00:00Z"),
        )
        .expect("done");

    let layer = store.get_review_layer(review.id, 5).expect("the layer");
    assert_eq!(layer.runs[0].finished_at, Some(START));
    assert_eq!(layer.seconds(), Some(0));
    assert!(
        layer.runs[0].finished_at < Some(a_year_later),
        "nothing a caller wrote reached a timestamp"
    );
}

#[test]
fn a_review_from_before_this_change_is_never_given_a_made_up_start() {
    // **Migration 11's backfill, against a review that really predates it.** The only time
    // such a review holds for a step is `review_layers.updated_at`, and the honest reading
    // of it is that the step entered the state it is in then. So a step it had already
    // finished has a finish and no start, and every figure that would need the start reads
    // as not recorded — never as zero, which would say the review took no time at all.
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    const FROM: i64 = 10;
    assert!(
        newest_version() > FROM,
        "this test needs the step-timings migration to come from version 10"
    );

    {
        let old = Store::open_at_version(&path, FROM).expect("open at version 10");
        assert_eq!(old.schema_version().unwrap(), FROM);
    }
    {
        // Written in its own SQL: today's typed functions speak the newest schema only.
        // One review posted a week ago, with the trail a session left behind it.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO reviews (id, repo, pr, head_sha, round, created_at)
                 VALUES (1, 'acme/portal', 412, '3f9c2ab', 1, 1759000000);
             INSERT INTO review_layers (review_id, number, state, detail, updated_at)
                 VALUES (1, 1, 'done', 'passed', 1759000100),
                        (1, 2, 'done', 'confirmed by the developer', 1759000200),
                        (1, 5, 'done', '3 findings', 1759000300),
                        (1, 6, 'done', 'approved by the developer', 1759000400),
                        (1, 7, 'running', 'harvesting', 1759000500);
             INSERT INTO drafts
                 (review_id, body, status, event, approved_at, approved_by, posted_at,
                  updated_at)
                 VALUES (1, 'the body', 'posted', 'comment', 1759000400, 'developer',
                         1759000450, 1759000450);",
        )
        .expect("write a review under the old schema");
    }

    let migrated = Store::open(&path).expect("reopen migrates");
    assert_eq!(migrated.schema_version().unwrap(), newest_version());

    let timings = migrated.review_timings(1).expect("the timings");
    assert_eq!(
        timings.last_activity_at, None,
        "nothing has written since the record started keeping this, and that is not zero"
    );
    assert_eq!(timings.draft_ready_at, None);
    assert_eq!(timings.with_the_agent_seconds, None);
    assert_eq!(timings.waiting_on_you_seconds, None);
    assert_eq!(
        timings.posted_at,
        Some(1759000450),
        "what the draft did record is still served"
    );

    let finished: Vec<_> = timings
        .layers
        .iter()
        .filter(|layer| layer.state == ReviewLayerState::Done)
        .collect();
    assert_eq!(finished.len(), 4);
    for layer in &finished {
        assert_eq!(layer.runs.len(), 1, "one state, from `updated_at`");
        assert_eq!(
            layer.runs[0].started_at, None,
            "step {} was given a start it never had",
            layer.number
        );
        assert_eq!(layer.runs[0].finished_at, layer.updated_at);
        assert_eq!(
            layer.seconds(),
            None,
            "step {} claims a length nothing measured",
            layer.number
        );
    }

    // The layer that was still running when the migration ran keeps the one thing
    // `updated_at` does say about it: when it entered the state it is in.
    let harvest = timings
        .layers
        .iter()
        .find(|layer| layer.number == 7)
        .expect("layer 7");
    assert_eq!(harvest.in_state_since(), Some(1759000500));
    assert_eq!(harvest.seconds(), None, "and it has not finished");

    // A step nothing ever wrote has no runs at all, which is a third answer again: not
    // started, as against finished-with-no-start and still-going.
    let alternatives = timings
        .layers
        .iter()
        .find(|layer| layer.number == 3)
        .expect("layer 3");
    assert_eq!(alternatives.state, ReviewLayerState::Pending);
    assert!(alternatives.runs.is_empty());
    assert_eq!(alternatives.seconds(), None);

    // And it is in the finished list, with its figures absent. Leaving it out would make
    // "how long does a review take" quietly a question about reviews recorded since this
    // change, which is a different question.
    let listed = migrated
        .finished_review_timings(None)
        .expect("the finished reviews");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1.with_the_agent_seconds, None);

    // A write now starts the record, from this change onwards.
    migrated
        .set_review_layer(1, 7, ReviewLayerState::Done, Some("3 outcomes"))
        .expect("the harvester finishes");
    let timings = migrated.review_timings(1).expect("the timings again");
    assert!(timings.last_activity_at.is_some());
    let harvest = timings
        .layers
        .iter()
        .find(|layer| layer.number == 7)
        .expect("layer 7");
    assert!(
        harvest.seconds().is_some(),
        "the start it did have was recorded, so this pass can be measured"
    );
}

#[test]
fn the_draft_ready_time_is_the_one_the_developer_is_looking_at() {
    let (_dir, store, clock) = stopped_store();
    let review = opened(&store, 422);

    store
        .put_draft(review.id, NewDraft::ready("the first draft"))
        .expect("ready");
    clock.advance(300);
    store
        .send_back_draft(review.id, "cut F2")
        .expect("sent back");
    clock.advance(300);
    store
        .put_draft(review.id, NewDraft::ready("the second draft"))
        .expect("ready again");

    assert_eq!(
        store.draft_ready_at(review.id).expect("the time"),
        Some(START + 600),
        "the draft in front of the developer now, not the one they sent back"
    );
    assert_eq!(
        store.get_draft(review.id).unwrap().status,
        DraftStatus::Ready
    );
}
