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
    Disposition, Finding, NewFinding, Outcome, OutcomeKind, PrState, Review, Severity, Store,
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

    let first = harvest(&store, &recording(), Selection::everything()).unwrap();
    assert!(first.recorded_anything());
    let after_first = store.list_outcomes(review.id).unwrap();

    // The same review, named explicitly: the sweep would skip it now that its pull
    // request is settled, and a run that fetched nothing would prove nothing.
    let second = harvest(
        &store,
        &recording(),
        Selection {
            repo: None,
            review: Some(review.id),
        },
    )
    .unwrap();

    assert!(
        second.reviews[0].was_settled,
        "the second run really did re-read a settled pull request"
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
fn a_commit_whose_files_cannot_be_read_keeps_its_place_with_no_files() {
    // A recording that holds the comparison and not the commits. Losing the commit would
    // take a line off "what changed since round 1"; losing its files costs the `late`
    // flag, which is already three-valued for exactly this case.
    let github = Recorded::nothing().with("compare", fixtures().join("compare.json"));

    let commits = review_desk_harvest::commits_between(
        &github,
        "devergehq/locus",
        "9535132",
        "46f6ea1645a9bbc9308f791411b2cac1a093367b",
    )
    .expect("the comparison alone is enough");

    assert_eq!(commits.len(), 4);
    assert!(
        commits.iter().all(|commit| commit.files.is_empty()),
        "every commit is there, with no files"
    );
}
