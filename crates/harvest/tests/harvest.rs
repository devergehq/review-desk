//! What a harvest records, against responses captured from a real pull request.
//!
//! No test here reaches the network. Every GitHub read is replayed from
//! `tests/fixtures`, captured off `devergehq/locus#64` with the commands listed in that
//! directory's README, and parsed by the same code the live run parses with.
//!
//! The reviewed commit is `REVIEWED_HEAD`. The pull request has since moved to
//! `CURRENT_HEAD`, which is what makes the "code changed after" facts available.

use std::path::PathBuf;

use review_desk_harvest::recorded::Recorded;
use review_desk_harvest::{harvest, GitHub, Selection};
use review_desk_store::{
    Disposition, Finding, NewFinding, Outcome, OutcomeKind, PrState, Review, ReviewLayerState,
    Severity, Store,
};
use tempfile::TempDir;

/// The commit the review read.
const REVIEWED_HEAD: &str = "953513212a0e1400d7911b204150585661f5aab1";
/// The commit the pull request is at now.
const CURRENT_HEAD: &str = "46f6ea1645a9bbc9308f791411b2cac1a093367b";
/// The commit that last touched `skills/dispatcher/dispatcher.py`.
const LAST_TOUCHING_COMMIT: &str = "46f6ea1645a9bbc9308f791411b2cac1a093367b";

const PY: &str = "skills/dispatcher/dispatcher.py";
const MD: &str = "skills/dispatcher/workers/implement.md";

/// The thread roots in the recording, with the reply each thread holds.
const BLOCKER_COMMENT: &str = "4140231301";
const SHOULD_COMMENT: &str = "4140231304";
const PRE_EXISTING_COMMENT: &str = "4140274640";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn recording() -> Recorded {
    Recorded::from_dir(fixtures())
}

/// The recording with the pull request still open, for the step 7 the harvest writes
/// while it is. See `tests/fixtures/README.md`: an open response for a pull request that
/// merged in September cannot be captured, so this one is derived from the capture.
fn still_open() -> Recorded {
    Recorded::from_dir(fixtures()).with("pull_request", fixtures().join("pull_request-open.json"))
}

/// Step 7 of a review, as the record holds it.
fn step_seven(store: &Store, review_id: i64) -> review_desk_store::ReviewLayer {
    store.get_review_layer(review_id, 7).expect("step 7")
}

/// A review of `devergehq/locus#64` at the reviewed commit, with five findings:
///
/// | Finding | Where | Linked comment |
/// | -- | -- | -- |
/// | 1 | `dispatcher.py:873` — the file changed later, these lines did not | the blocker |
/// | 2 | `dispatcher.py:471` — removed later | the "should" |
/// | 3 | `dispatcher.py:1282` — removed later | none |
/// | 4 | `implement.md:100` — the file never changed | the pre-existing one |
/// | 5 | no file at all | none |
fn a_posted_review() -> (TempDir, Store, Review) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();

    let review = store
        .create_review(review_desk_store::NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: REVIEWED_HEAD.to_string(),
            round: 1,
            // Posted, which is what makes step 7 this review's: a sweep covers every
            // review whose pull request is open, and one whose draft is still being
            // written has no harvest step to move.
            github_review_id: Some("PRR_kwDOA1".to_string()),
            ..Default::default()
        })
        .unwrap();

    add(
        &store,
        review.id,
        Some(PY),
        Some(873),
        Some(BLOCKER_COMMENT),
    );
    add(&store, review.id, Some(PY), Some(471), Some(SHOULD_COMMENT));
    add(&store, review.id, Some(PY), Some(1282), None);
    add(
        &store,
        review.id,
        Some(MD),
        Some(100),
        Some(PRE_EXISTING_COMMENT),
    );
    add(&store, review.id, None, None, None);

    (dir, store, review)
}

fn add(
    store: &Store,
    review_id: i64,
    file: Option<&str>,
    line: Option<i64>,
    comment: Option<&str>,
) -> Finding {
    let finding = store
        .add_finding(
            review_id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                file: file.map(str::to_string),
                line,
                claim: format!(
                    "something about {}",
                    file.unwrap_or("the change as a whole")
                ),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                ..Default::default()
            },
        )
        .unwrap();

    if let Some(comment) = comment {
        store
            .set_finding_github_comment(review_id, finding.seq, comment)
            .unwrap();
    }

    finding
}

/// The outcomes of one kind against one finding.
fn outcomes_of(store: &Store, review_id: i64, seq: i64, kind: OutcomeKind) -> Vec<Outcome> {
    store
        .list_outcomes_for_finding(review_desk_store::FindingRef { review_id, seq })
        .unwrap()
        .into_iter()
        .filter(|outcome| outcome.kind == kind)
        .collect()
}

/// An outcome's detail, as JSON.
fn detail(outcome: &Outcome) -> serde_json::Value {
    serde_json::from_str(outcome.detail.as_deref().expect("a detail")).expect("detail is JSON")
}

#[test]
fn the_pull_request_being_merged_is_recorded_as_a_state_and_a_time() {
    let (_dir, store, review) = a_posted_review();

    let report = harvest(&store, &recording(), Selection::everything()).unwrap();

    assert_eq!(report.reviews.len(), 1);
    assert_eq!(report.reviews[0].pr_state, PrState::Merged);
    // 2026-09-30T10:54:42Z, as GitHub reported it.
    assert_eq!(report.reviews[0].pr_state_at, Some(1_790_765_682));

    let stored = store.get_review(review.id).unwrap();
    assert_eq!(stored.pr_state, Some(PrState::Merged));
    assert_eq!(stored.pr_state_at, Some(1_790_765_682));
    assert!(
        stored.harvested_at.is_some(),
        "the review remembers it has been harvested"
    );
    assert!(stored.is_settled());
}

#[test]
fn a_reply_is_recorded_with_its_author_and_its_text_unchanged() {
    let (_dir, store, review) = a_posted_review();

    harvest(&store, &recording(), Selection::everything()).unwrap();

    let replied = outcomes_of(&store, review.id, 1, OutcomeKind::Replied);
    assert_eq!(replied.len(), 1, "the blocker's thread holds one reply");

    // The body, taken out of the recording itself rather than retyped here: the test
    // asserts that what was stored is what GitHub sent, character for character.
    let raw = std::fs::read_to_string(fixtures().join("review_threads.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let thread = json["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["comments"]["nodes"][0]["databaseId"].as_i64() == Some(4140231301))
        .expect("the blocker's thread");
    let reply = &thread["comments"]["nodes"][1];

    let stored = detail(&replied[0]);
    assert_eq!(stored["body"], reply["body"]);
    assert_eq!(stored["login"], reply["author"]["login"]);
    assert_eq!(
        stored["comment_id"],
        reply["databaseId"].as_i64().unwrap().to_string()
    );
    assert_eq!(stored["in_reply_to"], BLOCKER_COMMENT);
    assert!(
        stored["body"]
            .as_str()
            .unwrap()
            .starts_with("**Right, and it was this PR's own leak"),
        "the reply is stored whole, not summarised: {}",
        stored["body"]
    );

    // The author of the pull request replied to their own review comment, so the
    // outcome is attributed to the author; the login is kept verbatim regardless.
    assert_eq!(replied[0].actor.as_str(), "author");
    // 2026-09-30T02:33:43Z, the reply's own time, not the time of the harvest.
    assert_eq!(replied[0].occurred_at, 1_790_735_623);
}

#[test]
fn the_harvester_does_not_say_what_a_reply_meant() {
    let (_dir, store, review) = a_posted_review();

    harvest(&store, &recording(), Selection::everything()).unwrap();

    let kinds: Vec<&str> = store
        .list_outcomes(review.id)
        .unwrap()
        .iter()
        .map(|outcome| outcome.kind.as_str())
        .collect();

    assert!(
        !kinds.contains(&"rejected") && !kinds.contains(&"kept"),
        "classifying a reply needs judgement, which this command does not have: {kinds:?}"
    );
}

#[test]
fn a_resolved_thread_is_recorded() {
    let (_dir, store, review) = a_posted_review();

    let report = harvest(&store, &recording(), Selection::everything()).unwrap();

    assert_eq!(
        report.reviews[0].threads_resolved_recorded, 3,
        "three of the five findings have a linked comment, and all three threads are resolved"
    );

    let resolved = outcomes_of(&store, review.id, 1, OutcomeKind::ThreadResolved);
    assert_eq!(resolved.len(), 1);
    let how = detail(&resolved[0]);
    assert_eq!(how["thread"], BLOCKER_COMMENT);
    assert_eq!(how["resolved_by"], "patrickdorival");
    assert_eq!(
        how["time_basis"], "observed",
        "GitHub does not report when a thread was resolved, and the record says so"
    );
}

#[test]
fn code_changed_after_tells_the_findings_own_lines_from_the_file_around_them() {
    let (_dir, store, review) = a_posted_review();

    harvest(&store, &recording(), Selection::everything()).unwrap();

    // Finding 1 sits at a line of dispatcher.py that no later commit touched.
    let file_only = outcomes_of(&store, review.id, 1, OutcomeKind::CodeChangedAfter);
    assert_eq!(file_only.len(), 1);
    let around_it = detail(&file_only[0]);
    assert_eq!(around_it["file"], PY);
    assert_eq!(around_it["line"], 873);
    assert_eq!(
        around_it["lines_changed"], false,
        "the file changed around the finding; its own lines did not"
    );
    assert_eq!(around_it["commit"], LAST_TOUCHING_COMMIT);
    assert_eq!(
        around_it["basis"],
        format!("compare {REVIEWED_HEAD}...{CURRENT_HEAD}")
    );

    // Finding 2 sits at a line that was removed.
    let lines_changed = outcomes_of(&store, review.id, 2, OutcomeKind::CodeChangedAfter);
    assert_eq!(lines_changed.len(), 1);
    assert_eq!(detail(&lines_changed[0])["lines_changed"], true);

    // Finding 4 points at a file no later commit touched at all.
    assert!(
        outcomes_of(&store, review.id, 4, OutcomeKind::CodeChangedAfter).is_empty(),
        "a file that did not change is not an outcome"
    );

    // Finding 5 names no file, so there is nothing to have changed.
    assert!(outcomes_of(&store, review.id, 5, OutcomeKind::CodeChangedAfter).is_empty());
}

#[test]
fn a_second_harvest_over_the_same_responses_writes_no_new_rows() {
    let (_dir, store, review) = a_posted_review();

    // **Both runs read the pull request as still open**, which is the case that actually
    // repeats: the sweep re-reads an open pull request every quarter of an hour, and the
    // claim being made is that `add_harvested_outcome` writes each fact once. It used to
    // use the merged recording and name the review explicitly, which stopped working when
    // DEV-957 gave `harvest_one` an early-out for a review that was settled and still is —
    // the second run no longer reads the threads at all, so it proved nothing about them.
    // That early-out has a test of its own below.
    let first = harvest(&store, &still_open(), Selection::everything()).unwrap();
    assert!(first.recorded_anything());
    let after_first = store.list_outcomes(review.id).unwrap();

    let second = harvest(&store, &still_open(), Selection::everything()).unwrap();

    assert!(
        !second.reviews[0].was_settled,
        "both runs read an open pull request, which is the state a sweep repeats on"
    );
    assert_eq!(second.reviews[0].replies_recorded, 0);
    assert_eq!(second.reviews[0].threads_resolved_recorded, 0);
    assert_eq!(second.reviews[0].code_changed_recorded, 0);
    assert_eq!(
        second.reviews[0].already_recorded,
        first.reviews[0].replies_recorded
            + first.reviews[0].threads_resolved_recorded
            + first.reviews[0].code_changed_recorded,
        "every fact the first run recorded, the second found already there"
    );
    assert!(!second.recorded_anything());

    let after_second = store.list_outcomes(review.id).unwrap();
    assert_eq!(
        after_first, after_second,
        "not one row added, changed or re-timed"
    );
}

#[test]
fn a_finding_with_no_linked_comment_is_skipped_for_replies_and_counted() {
    let (_dir, store, review) = a_posted_review();

    let report = harvest(&store, &recording(), Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].findings, 5);
    assert_eq!(
        report.reviews[0].findings_without_comment, 2,
        "findings 3 and 5 were never linked to a comment"
    );
    assert!(
        report
            .summary()
            .contains("2 findings have no linked comment"),
        "the run says how many were skipped: {}",
        report.summary()
    );

    // Finding 3 has no thread to read, so no reply and no resolution …
    assert!(outcomes_of(&store, review.id, 3, OutcomeKind::Replied).is_empty());
    assert!(outcomes_of(&store, review.id, 3, OutcomeKind::ThreadResolved).is_empty());
    // … and is still checked for a later change to its code.
    let changed = outcomes_of(&store, review.id, 3, OutcomeKind::CodeChangedAfter);
    assert_eq!(changed.len(), 1);
    assert_eq!(detail(&changed[0])["lines_changed"], true);
}

#[test]
fn a_linked_comment_github_has_no_thread_for_is_counted_not_fatal() {
    let (_dir, store, review) = a_posted_review();
    add(&store, review.id, Some(PY), Some(10), Some("999999999"));

    let report = harvest(&store, &recording(), Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].findings_with_missing_thread, 1);
    assert_eq!(
        report.reviews[0].replies_recorded, 3,
        "the other three threads were still read"
    );
}

#[test]
fn a_review_whose_pull_request_was_merged_is_not_fetched_again() {
    let (_dir, store, review) = a_posted_review();
    store
        .set_review_pr_state(review.id, PrState::Merged, Some(1_790_765_682))
        .unwrap();

    // A recording of nothing: any read at all would fail the run outright.
    let github = Recorded::nothing();
    let report = harvest(&store, &github, Selection::everything()).unwrap();

    assert!(report.reviews.is_empty(), "nothing was up for harvest");
    assert_eq!(github.calls(), 0, "GitHub was not asked anything");
}

/// A recording of this pull request **closed and not merged**, written for the test that
/// needs one.
///
/// The fixtures hold the capture (merged) and one derived open response; the sweep's
/// early return needs the third state, because the sweep keeps reading a `closed` pull
/// request and leaves a `merged` one alone. Derived the way `pull_request-open.json` was and
/// for the same stated reason: the real pull request merged in September and a closed
/// response for it cannot be captured, now or ever.
fn still_closed(dir: &TempDir) -> (Recorded, PathBuf) {
    let captured = std::fs::read_to_string(fixtures().join("pull_request.json")).unwrap();
    let mut document: serde_json::Value = serde_json::from_str(&captured).unwrap();
    document["state"] = serde_json::json!("closed");
    document["merged"] = serde_json::json!(false);
    document["merged_at"] = serde_json::Value::Null;

    let path = dir.path().join("pull_request-closed.json");
    std::fs::write(&path, serde_json::to_string(&document).unwrap()).unwrap();
    (
        Recorded::from_dir(fixtures()).with("pull_request", path.clone()),
        path,
    )
}

/// **A sweep that re-reads a closed pull request and finds it unchanged asks GitHub once.**
///
/// DEV-957 keeps the newest round of a `closed` pull request in the sweep so a reopen can
/// be seen at all. Without this early-out that cost the whole harvest every quarter of an
/// hour — the pull request, then every review thread, then a commit comparison and a commit
/// read per finding whose file moved — for ever, on a pull request nobody will touch again.
///
/// The question the re-read exists to ask is "has it changed", and the first call answers
/// it. Measured on `Recorded::calls`, because "it makes one request" is the claim.
///
/// **The sweep, not `--review`.** A run the developer narrowed is a direct request and reads
/// the review in full; `naming_a_review_reads_its_settled_pull_request_in_full` is that half.
#[test]
fn a_sweep_re_reading_an_unchanged_closed_pull_request_asks_github_once() {
    let (dir, store, review) = a_posted_review();
    let (closed, _) = still_closed(&dir);

    harvest(&store, &closed, Selection::everything()).unwrap();
    assert_eq!(
        store.get_review(review.id).unwrap().pr_state,
        Some(PrState::Closed),
        "the first run recorded the close"
    );

    let (again, _) = still_closed(&dir);
    let report = harvest(&store, &again, Selection::everything()).unwrap();

    assert_eq!(again.calls(), 1, "the pull request, and nothing else");
    assert!(report.reviews[0].was_settled);
    assert_eq!(
        report.reviews[0].pr_state,
        PrState::Closed,
        "and it says so"
    );
    assert!(!report.recorded_anything());

    // **And it does not touch the review.** `set_review_pr_state` and
    // `mark_review_harvested` both move `last_activity_at`, so a quarter-hourly re-read had
    // a finished row reading "last written to just now" beside "No round of it can be
    // posted". Writing the same state twice was already a no-op for the columns; the stamp
    // was not.
    let before = store.get_review(review.id).unwrap().last_activity_at;
    let (once_more, _) = still_closed(&dir);
    harvest(&store, &once_more, Selection::everything()).unwrap();
    assert_eq!(
        store.get_review(review.id).unwrap().last_activity_at,
        before,
        "a sweep that learned nothing wrote nothing"
    );
}

/// **A pull request closed and then merged is recorded as merged.**
///
/// The early return asks whether the state *changed*, not whether it is settled. Asking the
/// second question treated `closed` read back as `merged` as nothing to do — and because
/// every later sweep asked it again, the record kept `closed` and the close date for ever,
/// with the Finished row saying "the pull request closed on" about a merged one. GitHub
/// cannot un-merge, but it can merge a pull request that was closed and reopened, and
/// `reviews_to_harvest` keeps reading a closed one precisely so that is seen. Found by a
/// review of this branch.
#[test]
fn a_closed_pull_request_read_back_as_merged_is_recorded_as_merged() {
    let (_dir, store, review) = a_posted_review();

    // Closed first, through the derived-from-capture open response with the state moved.
    store
        .set_review_pr_state(review.id, PrState::Closed, Some(1_790_000_000))
        .unwrap();
    assert!(store.get_review(review.id).unwrap().is_settled());

    // The capture itself is the merged one, which is what the next sweep reads.
    let report = harvest(&store, &recording(), Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].pr_state, PrState::Merged);
    let after = store.get_review(review.id).unwrap();
    assert_eq!(
        after.pr_state,
        Some(PrState::Merged),
        "a settled state that moved is still a state that moved"
    );
    assert_ne!(
        after.pr_state_at,
        Some(1_790_000_000),
        "and the time moved with it"
    );
}

/// **`--review` reads a settled pull request in full, which the sweep does not.**
///
/// The early return is the sweep's. A merged pull request can still be replied to and have
/// its threads resolved, and the narrowed run is the only thing that can record them —
/// `reviews_to_harvest` leaves a merged review alone altogether. Applying the early return
/// to `--review` took that away and left the flag's own help promising it. Found by a
/// review of this branch.
#[test]
fn naming_a_review_reads_its_settled_pull_request_in_full() {
    let (_dir, store, review) = a_posted_review();

    // The first run records the merge and everything that came back with it.
    harvest(&store, &recording(), Selection::everything()).unwrap();
    assert!(store.get_review(review.id).unwrap().is_settled());

    let again = recording();
    let report = harvest(
        &store,
        &again,
        Selection {
            repo: None,
            review: Some(review.id),
        },
    )
    .unwrap();

    assert!(
        again.calls() > 1,
        "the threads were read, which is the point of asking for one review"
    );
    assert!(report.reviews[0].was_settled);
    assert!(
        report.reviews[0].already_recorded > 0,
        "and every fact it found was already on the record"
    );
    assert!(!report.recorded_anything());
}

/// **A reopen is still seen, which is the whole reason the re-read exists.**
#[test]
fn a_settled_pull_request_read_as_open_again_is_harvested_properly() {
    let (_dir, store, review) = a_posted_review();

    harvest(&store, &recording(), Selection::everything()).unwrap();
    assert!(store.get_review(review.id).unwrap().is_settled());

    let reopened = still_open();
    let report = harvest(
        &store,
        &reopened,
        Selection {
            repo: None,
            review: Some(review.id),
        },
    )
    .unwrap();

    assert_eq!(report.reviews[0].pr_state, PrState::Open);
    assert!(
        !store.get_review(review.id).unwrap().is_settled(),
        "and the record says it is open again"
    );
    assert!(
        reopened.calls() > 1,
        "a read that found it open falls through and harvests it properly"
    );
}

#[test]
fn narrowing_by_repository_leaves_another_repositorys_reviews_alone() {
    let (_dir, store, review) = a_posted_review();
    let github = Recorded::nothing();

    let report = harvest(
        &store,
        &github,
        Selection {
            repo: Some("devergehq/review-desk".to_string()),
            review: None,
        },
    )
    .unwrap();

    assert!(report.reviews.is_empty());
    assert_eq!(github.calls(), 0);
    assert_eq!(
        store.get_review(review.id).unwrap().harvested_at,
        None,
        "the locus review was not touched"
    );
}

#[test]
fn an_unknown_review_is_refused_not_reported_as_broken() {
    let (_dir, store, _review) = a_posted_review();

    let error = harvest(
        &store,
        &Recorded::nothing(),
        Selection {
            repo: None,
            review: Some(4_040),
        },
    )
    .expect_err("there is no review 4040");

    assert!(error.is_refusal(), "an unknown id is exit 1, not exit 2");
    // The store's own code, not a second word for the same failure: `harvest --review 99`
    // and `review show --review 99` are one refusal and report one name.
    assert_eq!(error.code(), "not_found");
}

#[test]
fn naming_a_review_and_the_wrong_repository_is_refused() {
    let (_dir, store, review) = a_posted_review();

    let error = harvest(
        &store,
        &Recorded::nothing(),
        Selection {
            repo: Some("devergehq/review-desk".to_string()),
            review: Some(review.id),
        },
    )
    .expect_err("the review is not in that repository");

    assert!(error.is_refusal());
}

#[test]
fn a_review_already_at_the_pull_requests_head_is_not_compared() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();
    let review = store
        .create_review(review_desk_store::NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            // Reviewed at the commit the pull request is still at.
            head_sha: CURRENT_HEAD.to_string(),
            round: 1,
            ..Default::default()
        })
        .unwrap();
    add(
        &store,
        review.id,
        Some(PY),
        Some(873),
        Some(BLOCKER_COMMENT),
    );

    // The recording holds no comparison, so asking for one would fail the run.
    let github = Recorded::from_dir(fixtures()).with("compare", "/nowhere/compare.json");
    let report = harvest(&store, &github, Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].code_changed_recorded, 0);
    assert_eq!(report.reviews[0].replies_recorded, 1);
}

#[test]
fn a_review_with_no_linked_findings_costs_no_thread_read() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();
    let review = store
        .create_review(review_desk_store::NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: REVIEWED_HEAD.to_string(),
            round: 1,
            ..Default::default()
        })
        .unwrap();
    add(&store, review.id, Some(PY), Some(471), None);

    let github = Recorded::from_dir(fixtures()).with("review_threads", "/nowhere/threads.json");
    let report = harvest(&store, &github, Selection::everything()).unwrap();

    assert_eq!(report.reviews[0].findings_without_comment, 1);
    assert_eq!(report.reviews[0].code_changed_recorded, 1);
}

#[test]
fn the_trait_is_the_only_way_the_harvester_reaches_github() {
    // Compile-time, not behavioural: if harvesting ever reached for `gh` directly, a
    // recording could not stand in for it and none of the tests above would mean
    // anything.
    fn takes_any(_github: &dyn GitHub) {}
    takes_any(&Recorded::nothing());
}

// -- the commits between two rounds (DEV-873) ----------------------------------------

/// The commits between two rounds, read through the same recorded responses the rest of
/// this file uses. `compare.json` was captured before DEV-873 read a commit message, which
/// makes it the regression case: an old recording must still parse.
#[test]
fn the_commits_between_two_rounds_are_read_with_their_summaries_and_files() {
    let github = recording();
    let commits = review_desk_harvest::commits_between(
        &github,
        "devergehq/locus",
        "9535132",
        "46f6ea1645a9bbc9308f791411b2cac1a093367b",
    )
    .expect("read the comparison");

    assert_eq!(commits.len(), 4, "the four commits the comparison holds");
    assert_eq!(
        commits[0].sha, "cf4f69c6466e3f6d1f83eb864c15e5be94f7db57",
        "oldest first, in GitHub's own order"
    );
    assert_eq!(
        commits[0].summary, commits[0].sha,
        "a recording captured before the message was read has no summary, so the sha \
         stands in for it — the commit is still in the pull request, and the store \
         refuses a blank summary"
    );
    assert!(
        commits.iter().any(|commit| !commit.files.is_empty()),
        "and the per-commit files were read: {commits:?}"
    );
}

#[test]
fn a_commit_message_is_read_as_its_first_line() {
    // The shape `gh api repos/OWNER/REPO/compare/BASE...HEAD` returns, with the message
    // DEV-873 reads. One line out of a message with a body, which is what the page shows.
    let json = r#"{"status":"ahead","commits":[{"sha":"a41c9e2",
                    "commit":{"message":"Send an idempotency key with each delivery\n\nThe endpoint cannot tell a retry from a new event otherwise.",
                              "committer":{"date":"2026-10-09T22:14:00Z"}}}],"files":[]}"#;

    let comparison = review_desk_harvest::wire::comparison(json).expect("read it");

    assert_eq!(comparison.commits.len(), 1);
    assert_eq!(
        comparison.commits[0].summary.as_deref(),
        Some("Send an idempotency key with each delivery"),
        "the first line, and not the body"
    );
    assert_eq!(comparison.commits[0].committed_at, Some(1_791_584_040));
}

#[test]
fn a_commit_whose_files_cannot_be_read_fails_the_call() {
    // **This asserted the opposite until reviewing DEV-873 showed why it was wrong.** It
    // used to keep the commit with an empty file list, on the argument that losing the
    // commit would take a line off "what changed since round 1". But `Finding.late` is
    // computed from exactly these paths, so a commit recorded with no files turns a
    // transient GitHub failure into "the previous round could have caught this and did
    // not" — a false accusation on the page. `round commits` replaces what it recorded, so
    // failing leaves the previous answer intact.
    let github = Recorded::nothing().with("compare", fixtures().join("compare.json"));

    let error = review_desk_harvest::commits_between(
        &github,
        "devergehq/locus",
        "9535132",
        "46f6ea1645a9bbc9308f791411b2cac1a093367b",
    )
    .expect_err("the comparison alone is not enough");

    assert!(
        !error.is_refusal(),
        "a GitHub read that failed is Review Desk unable to run, not a refused request"
    );
}

#[test]
fn a_range_longer_than_github_will_list_is_refused_rather_than_truncated() {
    // GitHub's comparison endpoint caps at 250 commits and says so in `total_commits`.
    // Recording the 250 it sent would make "what changed since round 1" a truncated list
    // that nothing says is truncated — and `late` is computed from that list.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("compare-long.json");
    std::fs::write(
        &path,
        r#"{"status":"ahead","total_commits":312,"commits":[
             {"sha":"a41c9e2","commit":{"message":"One of three hundred","committer":{"date":"2026-10-09T22:14:00Z"}}}
           ],"files":[]}"#,
    )
    .unwrap();
    let github = Recorded::nothing().with("compare", &path);

    let error = review_desk_harvest::commits_between(&github, "acme/portal", "3f9c2ab", "9d02e71")
        .expect_err("a truncated range is not an answer");

    assert_eq!(error.code(), "commit_range_too_long");
    let said = error.to_string();
    assert!(said.contains("312"), "it says how many there are: {said}");
    assert!(said.contains("--base"), "and the way out: {said}");
}

// -- what the harvest leaves on step 7 (DEV-933) -------------------------------------

#[test]
fn a_harvest_of_an_open_pull_request_puts_step_seven_at_running_with_what_came_back() {
    let (_dir, store, review) = a_posted_review();

    let report = harvest(&store, &still_open(), Selection::everything()).unwrap();

    let said = report.reviews[0]
        .step_seven
        .as_ref()
        .expect("a posted review's step 7");
    assert_eq!(said.state, ReviewLayerState::Running);
    assert_eq!(
        said.detail.as_deref(),
        Some("3 replies back"),
        "the strongest fact on the line the trail draws"
    );

    let step = step_seven(&store, review.id);
    assert_eq!(step.state, ReviewLayerState::Running);
    assert_eq!(step.detail.as_deref(), Some("3 replies back"));
    assert!(
        step.updated_at.is_some(),
        "and the step records when the harvest read it"
    );

    let facts: Vec<(&str, &str)> = step
        .facts
        .iter()
        .map(|fact| (fact.label.as_str(), fact.value.as_str()))
        .collect();
    assert_eq!(
        facts,
        vec![
            ("Pull request", "open"),
            ("Replies", "3"),
            ("Threads resolved", "3"),
            ("Code changed since", "3 findings"),
        ],
        "each count a fact of its own, as the other steps record theirs"
    );
}

#[test]
fn a_harvest_of_a_review_with_nothing_back_says_so_rather_than_waiting_for_itself() {
    // A review posted at the commit the pull request is still at, with no finding linked
    // to a comment: there is no thread to read and nothing to compare, so the harvest
    // reads the pull request and finds nothing.
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();
    let review = store
        .create_review(review_desk_store::NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: CURRENT_HEAD.to_string(),
            round: 1,
            github_review_id: Some("PRR_kwDOA1".to_string()),
            ..Default::default()
        })
        .unwrap();
    add(&store, review.id, Some(PY), Some(873), None);
    store
        .set_review_layer(
            review.id,
            7,
            review_desk_store::ReviewLayerState::Pending,
            Some("posted, waiting for the harvest"),
        )
        .unwrap();

    harvest(&store, &still_open(), Selection::everything()).unwrap();

    let step = step_seven(&store, review.id);
    assert_eq!(step.state, ReviewLayerState::Running);
    assert_eq!(
        step.detail.as_deref(),
        Some("nothing back yet"),
        "a harvest has read this review, so it no longer says it is waiting for one"
    );
    assert_eq!(
        step.facts
            .iter()
            .map(|fact| (fact.label.as_str(), fact.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("Pull request", "open")],
        "where the pull request stands, which is what gives a harvested step its card"
    );
}

#[test]
fn a_harvest_of_a_merged_pull_request_finishes_step_seven() {
    let (_dir, store, review) = a_posted_review();

    // The recording as captured: the pull request merged on 30 September 2026.
    harvest(&store, &recording(), Selection::everything()).unwrap();

    let step = step_seven(&store, review.id);
    assert_eq!(
        step.state,
        ReviewLayerState::Done,
        "the pull request has settled, so the harvest has nothing further to read"
    );
    assert_eq!(step.detail.as_deref(), Some("merged, 3 replies back"));
    assert_eq!(
        step.facts.first().map(|fact| fact.label.as_str()),
        Some("Pull request"),
        "and how it ended is the first thing on the card"
    );
    assert_eq!(step.facts[0].value, "merged");
}

#[test]
fn a_second_harvest_of_the_same_responses_only_moves_the_time_step_seven_was_read() {
    let (_dir, store, review) = a_posted_review();

    harvest(&store, &still_open(), Selection::everything()).unwrap();
    let after_first = step_seven(&store, review.id);

    harvest(&store, &still_open(), Selection::everything()).unwrap();
    let after_second = step_seven(&store, review.id);

    assert_eq!(after_second.detail, after_first.detail);
    assert_eq!(after_second.facts, after_first.facts);
    assert_eq!(
        after_second.runs.len(),
        1,
        "one pass, still open: a quarter-hourly harvest must not report ninety-six \
         passes a day: {:?}",
        after_second.runs
    );
    assert!(after_second.updated_at >= after_first.updated_at);
}

#[test]
fn a_harvest_of_a_review_nothing_has_posted_leaves_step_seven_alone() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("review-desk.db")).unwrap();
    let review = store
        .create_review(review_desk_store::NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: REVIEWED_HEAD.to_string(),
            round: 1,
            // Nothing posted: no GitHub review id.
            ..Default::default()
        })
        .unwrap();
    add(
        &store,
        review.id,
        Some(PY),
        Some(873),
        Some(BLOCKER_COMMENT),
    );

    let report = harvest(&store, &still_open(), Selection::everything()).unwrap();

    assert!(
        report.reviews[0].step_seven.is_none(),
        "the run says nothing about a step that is not this review's yet"
    );
    assert!(
        store.find_review_layer(review.id, 7).unwrap().is_none(),
        "and nothing was written for it: two steps of the trail in progress at once \
         would claim a feedback loop nobody had started"
    );
    assert_eq!(
        report.reviews[0].replies_recorded, 1,
        "the rest of the harvest ran as it always does"
    );
}

#[test]
fn the_run_says_what_it_left_step_seven_saying() {
    let (_dir, store, _review) = a_posted_review();

    let report = harvest(&store, &still_open(), Selection::everything()).unwrap();

    let line = report.lines().join("\n");
    assert!(
        line.contains("step 7 running: 3 replies back"),
        "the dispatcher runs this on a timer and its line is all a person sees: {line}"
    );
}
