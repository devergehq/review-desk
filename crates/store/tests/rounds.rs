//! A finding's identity across rounds, and what changed between them. DEV-873.
//!
//! The claims under test are the ones the issue's acceptance criteria make, and each test
//! is named after the claim rather than the function. What is being defended is that a
//! second-round review can say "two fixed, two still open, one new" — which needs a
//! finding to be the same finding in both rounds.

mod common;

use common::{posted, temp_store};
use review_desk_store::{
    DecidedBy, Disposition, FindingCarry, FindingHunk, HunkLine, HunkLineMarker, NewDraft,
    NewFinding, NewOutcome, NewReview, NewRoundCommit, OutcomeKind, ReviewEvent, RoundStatus,
    Severity, Store, StoreError, Verification,
};

const REPO: &str = "acme/portal";
const PR: i64 = 412;

/// A round of the one pull request every test here uses.
///
/// **Whatever round is already there is recorded as posted first.** Since DEV-962 a new
/// commit on an unposted round moves that round's head instead of opening the next one —
/// which is what a round *is* now — and every test in this file is about a finding, a status
/// or a commit list crossing from one round to another. So the rounds here are rounds: each
/// one posts, and then the next commit opens the next round, which is the sequence DEV-873
/// was written against and is still the one after a post.
///
/// Recorded the way a session records it, with the id of the review it posted. Review Desk
/// posts nothing itself.
fn round(store: &Store, head: &str) -> i64 {
    post_whatever_is_open(store);
    store
        .open_review(REPO, PR, head)
        .expect("open the round")
        .review
        .id
}

/// Record every round of this pull request as posted, so the next commit opens a round.
fn post_whatever_is_open(store: &Store) {
    for review in store.list_reviews(REPO, PR).expect("the rounds so far") {
        if review.github_review_id.is_none() {
            posted(store, review.id);
        }
    }
}

/// A finding on a review, and the number it was given.
fn raise(store: &Store, review: i64, claim: &str, file: Option<&str>) -> i64 {
    store
        .add_finding(
            review,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Should,
                file: file.map(str::to_string),
                line: None,
                claim: claim.to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                introduced_by_commit: None,
                introduced_by_seq: None,
                ..Default::default()
            },
        )
        .expect("raise a finding")
        .seq
}

/// A carry with a status and nothing else said about it.
fn as_(status: RoundStatus) -> FindingCarry {
    FindingCarry {
        status,
        by: DecidedBy::Agent,
        checked: None,
        commit: None,
    }
}

// -- a finding's number is its identity -----------------------------------------------

#[test]
fn a_carried_finding_keeps_the_number_it_had() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "no idempotency key", None);
    let f2 = raise(&store, first, "the backoff schedule has no ceiling", None);
    assert_eq!(f2, 2, "the second finding of round 1");

    let second = round(&store, "bbbbbbb");
    let carried = store
        .carry_finding(first, second, f2, &as_(RoundStatus::StillOpen))
        .expect("carry it forward");

    assert_eq!(
        carried.seq, 2,
        "F2 in round 1 is F2 in round 2, which is the whole of this issue"
    );
    assert_eq!(carried.review_id, second);
    assert_eq!(carried.continues_review_id, Some(first));
    assert_eq!(
        carried.claim, "the backoff schedule has no ceiling",
        "the claim comes with it, so round 2 can be read without reading round 1"
    );
}

#[test]
fn a_new_finding_takes_the_next_number_unused_on_the_pull_request() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    for n in 1..=5 {
        assert_eq!(raise(&store, first, &format!("finding {n}"), None), n);
    }

    let second = round(&store, "bbbbbbb");
    let fresh = raise(&store, second, "the key is different on every retry", None);

    assert_eq!(
        fresh, 6,
        "a number has to mean one finding on the pull request, not one per round"
    );
}

#[test]
fn a_number_restarts_for_a_different_pull_request() {
    let (_dir, store) = temp_store();
    let ours = round(&store, "aaaaaaa");
    raise(&store, ours, "ours", None);

    let theirs = store
        .create_review(NewReview {
            repo: REPO.to_string(),
            pr: 413,
            head_sha: "ccccccc".to_string(),
            round: 1,
            ..Default::default()
        })
        .expect("another pull request");

    assert_eq!(
        raise(&store, theirs.id, "theirs", None),
        1,
        "the pull request is the scope a number is an identity in"
    );
}

// -- opening a later round ------------------------------------------------------------

#[test]
fn opening_round_two_offers_round_ones_findings() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "no idempotency key", None);
    raise(&store, first, "no ceiling on the backoff", None);

    let second = round(&store, "bbbbbbb");
    let carryable = store
        .carryable(second)
        .expect("the carryable findings")
        .expect("round 2 has a round 1 behind it");

    assert_eq!(carryable.from_review_id, first);
    assert_eq!(carryable.from_round, 1);
    assert_eq!(
        carryable.findings.iter().map(|f| f.seq).collect::<Vec<_>>(),
        [1, 2],
        "both of round 1's findings, ready to be carried"
    );
}

#[test]
fn a_first_round_has_nothing_to_carry() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "the only finding", None);

    assert!(store.carryable(first).expect("ask anyway").is_none());
    assert!(store.previous_round(first).expect("ask anyway").is_none());
}

#[test]
fn a_finding_already_settled_is_still_offered_rather_than_hidden() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "went to a ticket", None);
    store
        .set_finding_round_status(first, f1, &as_(RoundStatus::Ticketed))
        .expect("ticket it");

    let second = round(&store, "bbbbbbb");
    let carryable = store.carryable(second).unwrap().unwrap();

    assert_eq!(
        carryable.findings[0].round_status,
        RoundStatus::Ticketed,
        "the page says \"went to a ticket in round 1, not part of this round\" in those \
         words, so the status comes with the finding rather than the finding being dropped"
    );
}

// -- how many rounds a finding has been open ------------------------------------------

#[test]
fn a_finding_carried_through_three_rounds_is_open_for_three_rounds() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "the retry delay ignores Retry-After", None);

    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("carry into round 2");

    let third = round(&store, "ccccccc");
    let carried = store
        .carry_finding(second, third, f1, &as_(RoundStatus::StillOpen))
        .expect("carry into round 3");

    assert_eq!(carried.seq, f1, "its number has not moved");
    assert_eq!(
        store.rounds_open(third, f1).expect("the rounds it is open"),
        3,
        "new in round 1 and still open in rounds 2 and 3"
    );
}

#[test]
fn a_finding_that_is_no_longer_open_is_open_for_no_rounds() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "no idempotency key", None);

    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::Fixed))
        .expect("carry it as fixed");

    assert_eq!(
        store.rounds_open(second, f1).unwrap(),
        0,
        "\"open for 2 rounds\" is a sentence about a finding that still is"
    );
}

#[test]
fn the_count_stops_at_the_round_the_finding_was_answered_in() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "a nit", None);

    // Accepted in round 2 — settled — and then raised again in round 3.
    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::Accepted))
        .unwrap();
    let third = round(&store, "ccccccc");
    store
        .carry_finding(second, third, f1, &as_(RoundStatus::StillOpen))
        .unwrap();

    assert_eq!(
        store.rounds_open(third, f1).unwrap(),
        1,
        "it has been open since round 3, not since round 1: round 2 answered it"
    );
}

// -- who decided, and how they checked ------------------------------------------------

#[test]
fn a_status_records_who_decided_it_and_how_they_checked() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "no idempotency key", None);

    let second = round(&store, "bbbbbbb");
    let carried = store
        .carry_finding(
            first,
            second,
            f1,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: Some("their new test fails with the fix reverted".to_string()),
                commit: Some("A41C9E2".to_string()),
            },
        )
        .expect("carry it as fixed");

    assert_eq!(carried.round_status, RoundStatus::Fixed);
    assert_eq!(
        carried.round_status_by,
        Some(DecidedBy::Agent),
        "an agent decided it, and the record must not say the developer did"
    );
    assert_eq!(
        carried.round_status_checked.as_deref(),
        Some("their new test fails with the fix reverted")
    );
    assert_eq!(
        carried.round_status_commit.as_deref(),
        Some("a41c9e2"),
        "a commit is identity, so it is stored in the form the record keys on"
    );
    assert!(carried.round_status_at.is_some());
}

#[test]
fn a_blank_note_is_no_note_rather_than_a_note_that_says_nothing() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "a finding", None);

    let changed = store
        .set_finding_round_status(
            first,
            f1,
            &FindingCarry {
                status: RoundStatus::Withdrawn,
                by: DecidedBy::Developer,
                checked: Some("   ".to_string()),
                commit: Some("".to_string()),
            },
        )
        .expect("withdraw it");

    assert_eq!(changed.round_status_checked, None);
    assert_eq!(changed.round_status_commit, None);
}

#[test]
fn carrying_the_same_finding_again_rewrites_the_status() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "the backoff has no ceiling", None);
    let second = round(&store, "bbbbbbb");

    store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("an agent leaves it open");

    let accepted = store
        .carry_finding(
            first,
            second,
            f1,
            &FindingCarry {
                status: RoundStatus::Accepted,
                by: DecidedBy::Developer,
                checked: Some("their reason: ABC-240 gives the dead state a screen".to_string()),
                commit: None,
            },
        )
        .expect("and the developer accepts the author's reason");

    assert_eq!(accepted.round_status, RoundStatus::Accepted);
    assert_eq!(accepted.round_status_by, Some(DecidedBy::Developer));
    assert_eq!(
        store.list_findings(second).unwrap().len(),
        1,
        "re-carrying rewrites the row rather than adding a second one"
    );
}

#[test]
fn a_carry_from_another_pull_request_is_refused() {
    let (_dir, store) = temp_store();
    let ours = round(&store, "aaaaaaa");
    let f1 = raise(&store, ours, "ours", None);

    let theirs = store
        .create_review(NewReview {
            repo: REPO.to_string(),
            pr: 999,
            head_sha: "ccccccc".to_string(),
            round: 2,
            ..Default::default()
        })
        .expect("a round of another pull request");

    match store.carry_finding(ours, theirs.id, f1, &as_(RoundStatus::StillOpen)) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "from_review_id"),
        other => panic!("a finding cannot cross pull requests: {other:?}"),
    }
}

#[test]
fn a_carry_that_does_not_run_backwards_is_refused() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "the finding", None);
    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .unwrap();

    // Round 2 into round 1 would make the chain a loop, and the consecutive-round count
    // walks that chain.
    match store.carry_finding(second, first, f1, &as_(RoundStatus::StillOpen)) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "from_review_id"),
        other => panic!("a chain has to run backwards: {other:?}"),
    }
}

// -- what a new finding is attributed to ----------------------------------------------

#[test]
fn a_new_finding_names_the_commit_and_the_finding_it_was_introduced_by() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(
        &store,
        first,
        "no idempotency key",
        Some("jobs/DeliverWebhook.php"),
    );

    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(
            first,
            second,
            f1,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: None,
                commit: Some("a41c9e2".to_string()),
            },
        )
        .unwrap();

    let introduced = store
        .add_finding(
            second,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(59),
                claim: "the idempotency key is different on every retry".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                introduced_by_commit: Some("A41C9E2".to_string()),
                introduced_by_seq: Some(f1),
                ..Default::default()
            },
        )
        .expect("the fix for F1 introduced it");

    assert_eq!(introduced.seq, 2, "the next number on the pull request");
    assert_eq!(
        introduced.introduced_by_commit.as_deref(),
        Some("a41c9e2"),
        "normalised, because a commit is identity"
    );
    assert_eq!(
        introduced.introduced_by_seq,
        Some(f1),
        "a number alone names the finding, because a number is an identity now"
    );
}

#[test]
fn an_attribution_naming_a_finding_nobody_raised_is_refused() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");

    let refused = store.add_finding(
        first,
        NewFinding {
            lens: "sibling diff".to_string(),
            severity: Severity::Blocker,
            file: None,
            line: None,
            claim: "introduced by a finding that does not exist".to_string(),
            evidence: None,
            disposition: Disposition::InDraft,
            disposition_reason: None,
            rule_id: None,
            introduced_by_commit: None,
            introduced_by_seq: Some(99),
            ..Default::default()
        },
    );

    match refused {
        Err(StoreError::UnknownParent { parent, id, .. }) => {
            assert_eq!(parent, "finding it was introduced by");
            assert_eq!(id, "99");
        }
        other => panic!("an attribution pointing at nothing is a refusal: {other:?}"),
    }
    assert!(
        store.list_findings(first).unwrap().is_empty(),
        "and nothing was written"
    );
}

// -- what changed between two rounds --------------------------------------------------

/// The three commits board A3 shows between round 1 and round 2.
fn three_commits() -> Vec<NewRoundCommit> {
    vec![
        NewRoundCommit {
            sha: "a41c9e2".to_string(),
            summary: "Send an idempotency key with each delivery\n\nLonger body, dropped."
                .to_string(),
            committed_at: Some(1_000),
            files: vec!["jobs/DeliverWebhook.php".to_string()],
        },
        NewRoundCommit {
            sha: "c7730bd".to_string(),
            summary: "Record a delivery as dead after the last attempt".to_string(),
            committed_at: Some(900),
            files: vec!["jobs/DeliverWebhook.php".to_string()],
        },
        NewRoundCommit {
            sha: "9d02e71".to_string(),
            summary: "Add a test for the dead-delivery record".to_string(),
            committed_at: Some(1_100),
            files: vec![
                "tests/WebhookRetryTest.php".to_string(),
                "tests/WebhookRetryTest.php".to_string(),
            ],
        },
    ]
}

#[test]
fn a_rounds_commits_keep_their_order_their_summary_and_their_files() {
    let (_dir, store) = temp_store();
    let second = round(&store, "9d02e71");

    let recorded = store
        .put_round_commits(second, &three_commits())
        .expect("record what changed");

    assert_eq!(
        recorded.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
        ["a41c9e2", "c7730bd", "9d02e71"],
        "GitHub's order, not a sort on the committer date — c7730bd's date is earlier \
         than a41c9e2's, as an amended commit's can be"
    );
    assert_eq!(recorded[0].position, 0);
    assert_eq!(
        recorded[0].summary, "Send an idempotency key with each delivery",
        "the first line and only the first: a paragraph does not go in a table cell"
    );
    assert_eq!(
        recorded[2].files,
        ["tests/WebhookRetryTest.php"],
        "deduplicated"
    );
}

#[test]
fn recording_a_rounds_commits_again_replaces_them() {
    let (_dir, store) = temp_store();
    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();

    let after = store
        .put_round_commits(
            second,
            &[NewRoundCommit {
                sha: "eeeeeee".to_string(),
                summary: "The only commit the pull request has after a force push".to_string(),
                committed_at: None,
                files: vec!["jobs/DeliverWebhook.php".to_string()],
            }],
        )
        .expect("re-read after a force push");

    assert_eq!(
        after.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
        ["eeeeeee"],
        "a second read must not leave behind a commit the pull request no longer has"
    );
}

#[test]
fn a_commit_with_no_summary_is_refused_rather_than_stored_blank() {
    let (_dir, store) = temp_store();
    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();

    let refused = store.put_round_commits(
        second,
        &[NewRoundCommit {
            sha: "eeeeeee".to_string(),
            summary: "   \n".to_string(),
            committed_at: None,
            files: Vec::new(),
        }],
    );

    match refused {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "summary"),
        other => panic!("a blank summary is a refusal: {other:?}"),
    }
    assert_eq!(
        store.list_round_commits(second).unwrap().len(),
        3,
        "and the refusal left the previous answer intact rather than half of a new one"
    );
}

#[test]
fn the_same_commit_twice_is_refused() {
    let (_dir, store) = temp_store();
    let second = round(&store, "9d02e71");

    let mut commits = three_commits();
    commits.push(commits[0].clone());

    match store.put_round_commits(second, &commits) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "sha"),
        other => panic!("one commit appears once: {other:?}"),
    }
}

// -- the rounds timeline ---------------------------------------------------------------

#[test]
fn the_timeline_reports_what_each_round_posted_and_what_came_back() {
    let (_dir, store) = temp_store();

    let first = round(&store, "3f9c2ab");
    let f1 = raise(
        &store,
        first,
        "no idempotency key",
        Some("jobs/DeliverWebhook.php"),
    );
    raise(&store, first, "the backoff has no ceiling", None);
    store
        .put_draft(first, NewDraft::ready("round one"))
        .unwrap();
    store
        .approve_draft(
            first,
            ReviewEvent::RequestChanges,
            review_desk_store::Actor::Developer,
        )
        .unwrap();
    store.post_draft(first, "gh-review-1").unwrap();

    // What came back: a reply the harvester read, and a thread resolved.
    for kind in [OutcomeKind::Replied, OutcomeKind::ThreadResolved] {
        store
            .add_outcome(
                review_desk_store::FindingRef {
                    review_id: first,
                    seq: f1,
                },
                NewOutcome {
                    kind,
                    actor: review_desk_store::Actor::Author,
                    detail: None,
                    occurred_at: None,
                },
            )
            .expect("record what came back");
    }

    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::Fixed))
        .unwrap();
    raise(&store, second, "the key is different on every retry", None);

    let rounds = store.rounds(REPO, PR).expect("the timeline");
    assert_eq!(rounds.len(), 2, "oldest first");

    let one = &rounds[0];
    assert_eq!(one.round, 1);
    assert_eq!(one.event, Some(ReviewEvent::RequestChanges));
    assert!(one.posted_at.is_some(), "round 1 was posted");
    assert_eq!(one.findings, 2);
    assert_eq!(one.replies, 1);
    assert_eq!(one.threads_resolved, 1);
    assert_eq!(
        one.commits, 0,
        "nothing was recorded as changing before round 1; the three commits belong to \
         the round that reviewed them"
    );
    assert_eq!(one.statuses.new, 2);

    let two = &rounds[1];
    assert_eq!(two.round, 2);
    assert_eq!(two.posted_at, None, "round 2 has not been posted");
    assert_eq!(two.event, None);
    assert_eq!(two.commits, 3);
    assert_eq!(two.findings, 2);
    assert_eq!(two.statuses.fixed, 1);
    assert_eq!(
        two.statuses.new, 1,
        "one fixed and one new, which is the sentence the page could not say before"
    );
    assert_eq!(two.replies, 0, "nothing has come back for round 2 yet");
}

#[test]
fn a_pull_request_with_no_reviews_has_no_timeline() {
    let (_dir, store) = temp_store();
    assert!(store.rounds(REPO, 1).expect("ask anyway").is_empty());
}

#[test]
fn the_timeline_is_found_whatever_case_the_repository_was_typed_in() {
    let (_dir, store) = temp_store();
    round(&store, "aaaaaaa");

    assert_eq!(
        store.rounds("Acme/Portal", PR).expect("the timeline").len(),
        1,
        "the repository is normalised on the way in, so it has to be on the way out too"
    );
}

// -- the late flag ---------------------------------------------------------------------

#[test]
fn a_new_finding_on_code_that_did_not_change_is_late() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "round one", None);

    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();
    let changed = raise(
        &store,
        second,
        "on code the commits touched",
        Some("jobs/DeliverWebhook.php"),
    );
    let untouched = raise(
        &store,
        second,
        "on code nothing touched since round 1",
        Some("config/webhooks.php"),
    );

    let found = store
        .findings_in_round(second)
        .expect("the round's findings");
    let late = |seq: i64| {
        found
            .iter()
            .find(|f| f.finding.seq == seq)
            .expect("the finding")
            .late
    };

    assert_eq!(late(changed), Some(false));
    assert_eq!(
        late(untouched),
        Some(true),
        "round 1 could have caught it and did not"
    );
}

#[test]
fn late_is_unanswerable_rather_than_false_when_nothing_can_answer_it() {
    let (_dir, store) = temp_store();

    // A first round: nothing to be late against.
    let first = round(&store, "aaaaaaa");
    let only = raise(&store, first, "round one", Some("jobs/DeliverWebhook.php"));
    assert_eq!(store.findings_in_round(first).unwrap()[0].late, None);

    let second = round(&store, "9d02e71");

    // A round whose commits have not been recorded: nothing to compare against.
    let unknown = raise(
        &store,
        second,
        "no commits read yet",
        Some("config/webhooks.php"),
    );
    assert_eq!(
        store
            .findings_in_round(second)
            .unwrap()
            .iter()
            .find(|f| f.finding.seq == unknown)
            .unwrap()
            .late,
        None,
        "\"nobody looked\" must not read as \"this was not missed\""
    );

    store.put_round_commits(second, &three_commits()).unwrap();

    // A finding that is not new: it was not raised in this round.
    store
        .carry_finding(first, second, only, &as_(RoundStatus::StillOpen))
        .unwrap();
    // And a new finding that names no file: no code to compare.
    let nowhere = raise(&store, second, "a claim about the whole change", None);

    let found = store.findings_in_round(second).unwrap();
    let late = |seq: i64| {
        found
            .iter()
            .find(|f| f.finding.seq == seq)
            .expect("the finding")
            .late
    };
    assert_eq!(
        late(only),
        None,
        "a carried finding is not late, it is open"
    );
    assert_eq!(late(nowhere), None);
}

#[test]
fn a_finding_reports_the_round_it_was_carried_from() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "round one", None);
    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .unwrap();
    let fresh = raise(&store, second, "raised in round 2", None);

    let found = store.findings_in_round(second).unwrap();
    let carried = found.iter().find(|f| f.finding.seq == f1).unwrap();
    let raised = found.iter().find(|f| f.finding.seq == fresh).unwrap();

    assert_eq!(carried.continues_round, Some(1), "\"raised in round 1\"");
    assert_eq!(raised.continues_round, None);
}

// -- what a carry copies, and what it does not -----------------------------------------

/// Every column of `findings`, and what carrying a finding does with it.
///
/// **This test exists to fail when a column is added to `findings` and nobody decides
/// whether a carried finding keeps it.** A carry writes an explicit column list — it has
/// to, because the row it writes differs from the row it reads in exactly the round
/// columns — so a column added by a later migration is silently *not* carried, and a
/// carried F2 would quietly lose whatever that column held. The three sets below are the
/// three answers available, and the assertion is that every column has one of them.
///
/// It does not cover child tables keyed on `(review_id, finding_seq)`. A table added
/// beside `findings` has to be carried by hand and named in [`CARRIED_TABLES`].
const CARRIED: &[&str] = &[
    // What the finding *is*. A carried finding whose claim or evidence vanished would be
    // a different finding wearing the same number.
    "lens",
    "severity",
    "file",
    "line",
    // **The commit that line was measured on, and it is carried for the same reason the
    // line is.** DEV-978: a carry copies a number, and a number is an address only
    // against the commit it was counted on — so carrying one without the other is what
    // made a round-two link point at round two's commit with round one's numbering.
    // Nothing here re-measures, so nothing here may re-stamp.
    "line_sha",
    "claim",
    // The one line the code review page's row carries. A carried finding that lost it
    // would read as a different finding in round 2 — the row would fall back to the
    // claim and the page would look as though nobody had ever titled it.
    "title",
    "evidence",
    // What is to become of it in the draft. Carried as it stood, and changed from there
    // with `finding set` — a carry must not quietly redecide a disposition.
    "disposition",
    "disposition_reason",
    // The rule it was raised under. Still the ruling it was raised under.
    "rule_id",
    // Where it came from. **Carried, and this was wrong the first time.** "Introduced by
    // a41c9e2, the commit that fixed F1" is a fact about the finding's origin and stays
    // true in every later round; dropping it on the carry made the record forget, by
    // round 3, that a fix had caused F5 at all.
    "introduced_by_commit",
    "introduced_by_seq",
    // What the finding is *about*, added by DEV-874 and carried for the same reason as the
    // claim: **these are the only copy there is.** Review Desk does not fetch code, so a
    // carried finding that lost its comment, its proof or its fix would be a different
    // finding wearing the same number, and nothing could get them back. This test is how
    // they were found — it named all seven the moment DEV-874 merged.
    "flagged_line",
    "inline_comment",
    "why_it_matters",
    "proof",
    "proof_note",
    "verification",
    "suggested_fix",
];

/// Columns the carry writes itself, because they are about this round rather than the
/// finding.
const SET_BY_THE_CARRY: &[&str] = &[
    "review_id",
    "seq",
    "continues_review_id",
    "round_status",
    "round_status_by",
    "round_status_checked",
    "round_status_at",
    "round_status_commit",
    "created_at",
];

/// Columns a carried finding deliberately does not get, each with its reason.
const NOT_CARRIED: &[(&str, &str)] = &[(
    "github_comment_id",
    "a round posts its own comments, and `findings_by_github_comment` is UNIQUE — \
         copying it would make the carry itself fail",
)];

/// Child tables of `findings` a carry copies, each with its reason.
///
/// Copied with `INSERT ... SELECT`, so the text never passes through this process: the hunk
/// is the only copy of that code there is, and a round trip through Rust is a chance to
/// lose a tab or a line that is nothing but a space.
const CARRIED_TABLES: &[(&str, &str)] = &[
    (
        "finding_hunks",
        "the code the finding is about, as it stood at the earlier round's head commit",
    ),
    (
        "finding_hunk_lines",
        "its lines, in the order they are drawn",
    ),
    (
        "finding_fix_references",
        "the sibling code the fix should look like",
    ),
];

#[test]
fn every_column_of_a_finding_is_decided_about_when_it_is_carried() {
    let (dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "a finding", None);
    drop(store);

    let conn = rusqlite::Connection::open(common::db_path(&dir)).expect("open the database");
    let mut statement = conn
        .prepare("SELECT name FROM pragma_table_info('findings')")
        .expect("read the columns");
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("the column names")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("the column names");

    assert!(
        columns.len() > 10,
        "the columns were not read at all: {columns:?}"
    );

    for column in &columns {
        let decided = CARRIED.contains(&column.as_str())
            || SET_BY_THE_CARRY.contains(&column.as_str())
            || NOT_CARRIED.iter().any(|(name, _)| name == column);

        assert!(
            decided,
            "`findings.{column}` is new and nothing says whether a carried finding keeps \
             it. Add it to CARRIED in crates/store/src/entities/round.rs's INSERT and to \
             CARRIED here, or to NOT_CARRIED with the reason. A column left out is a \
             carried finding quietly losing what it held — see DEV-873."
        );
    }

    // And the three sets are about columns that exist, so a renamed column fails here
    // rather than leaving a stale entry nobody notices.
    for listed in CARRIED
        .iter()
        .chain(SET_BY_THE_CARRY.iter())
        .chain(NOT_CARRIED.iter().map(|(name, _)| name))
    {
        assert!(
            columns.iter().any(|column| column == listed),
            "`findings.{listed}` is listed here and is not a column any more"
        );
    }

    // Every table keyed on a finding is either carried or is `findings` itself. A fourth
    // one added beside them has to be decided about the same way the columns are.
    let mut statement = conn
        .prepare(
            "SELECT name FROM pragma_table_list
             WHERE schema = 'main' AND type = 'table' AND name LIKE 'finding%'",
        )
        .expect("read the tables");
    let tables: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("the table names")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("the table names");

    for table in &tables {
        if table == "findings" {
            continue;
        }
        assert!(
            CARRIED_TABLES.iter().any(|(name, _)| name == table),
            "`{table}` hangs off a finding and nothing says whether a carry copies it. \
             Add it to `carry_evidence_tables` and to CARRIED_TABLES with the reason — see \
             DEV-873."
        );
    }
    assert_eq!(
        tables.len(),
        CARRIED_TABLES.len() + 1,
        "the tables read were {tables:?}, which is not `findings` plus the carried ones"
    );
}

#[test]
fn a_carry_copies_what_the_finding_is_and_not_the_comment_it_was_posted_as() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(
        &store,
        first,
        "no idempotency key",
        Some("jobs/DeliverWebhook.php"),
    );
    store
        .set_finding_disposition(
            first,
            f1,
            Disposition::HeldBack,
            Some("a nit, for the one ticket".to_string()),
        )
        .expect("hold it back");
    store
        .set_finding_github_comment(first, f1, "4140231301")
        .expect("link the comment it was posted as");

    let earlier = store.get_finding(first, f1).expect("the earlier finding");
    let second = round(&store, "bbbbbbb");
    let carried = store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("carry it");

    // What the finding is, carried.
    assert_eq!(carried.lens, earlier.lens);
    assert_eq!(carried.severity, earlier.severity);
    assert_eq!(carried.file, earlier.file);
    assert_eq!(carried.line, earlier.line);
    assert_eq!(carried.claim, earlier.claim);
    assert_eq!(carried.evidence, earlier.evidence);
    assert_eq!(carried.disposition, earlier.disposition);
    assert_eq!(carried.disposition_reason, earlier.disposition_reason);
    assert_eq!(carried.rule_id, earlier.rule_id);

    // And the comment it was posted as, not carried: a round posts its own comments, and
    // the unique index would refuse the row if this were copied.
    assert_eq!(
        carried.github_comment_id, None,
        "round 2 has not posted anything yet, and one comment belongs to one finding"
    );
    assert_eq!(
        store
            .get_finding(first, f1)
            .unwrap()
            .github_comment_id
            .as_deref(),
        Some("4140231301"),
        "and round 1 still holds its own"
    );
}

#[test]
fn a_carried_finding_still_says_which_fix_introduced_it() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(
        &store,
        first,
        "no idempotency key",
        Some("jobs/DeliverWebhook.php"),
    );

    let second = round(&store, "bbbbbbb");
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::Fixed))
        .unwrap();
    let f2 = store
        .add_finding(
            second,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: Some("jobs/DeliverWebhook.php".to_string()),
                line: Some(59),
                claim: "the key is different on every retry".to_string(),
                evidence: None,
                disposition: Disposition::InDraft,
                disposition_reason: None,
                rule_id: None,
                introduced_by_commit: Some("a41c9e2".to_string()),
                introduced_by_seq: Some(f1),
                ..Default::default()
            },
        )
        .expect("the fix for F1 introduced it")
        .seq;

    // Carried twice more. **Found by driving three rounds through the binary by hand**:
    // the attribution used to be dropped on the carry, so by round 3 the record had
    // forgotten that a fix caused F2 — and "introduced by a fix" is a figure the page
    // shows across the whole pull request.
    let third = round(&store, "ccccccc");
    store
        .carry_finding(second, third, f2, &as_(RoundStatus::Fixed))
        .unwrap();
    let fourth = round(&store, "ddddddd");
    let still = store
        .carry_finding(third, fourth, f2, &as_(RoundStatus::Fixed))
        .unwrap();

    assert_eq!(
        still.introduced_by_commit.as_deref(),
        Some("a41c9e2"),
        "where a finding came from stays true in every later round"
    );
    assert_eq!(still.introduced_by_seq, Some(f1));
}

#[test]
fn two_rounds_written_before_the_migration_are_not_joined_by_their_numbers() {
    // **The case a wrong inference would be visible in.** Before migration 7 each round
    // numbered its findings from 1, so a pull request reviewed twice holds two findings
    // called F1 that have nothing to do with each other. Deriving the chain from a
    // matching number — rather than from the link `carry_finding` writes — would join
    // them, and then report the second as "open for 2 rounds" and the first as the
    // finding it continues.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("review-desk.db");

    {
        // Version 6, the one immediately before this change, so the rows below are
        // written under the schema that actually preceded it rather than an older one.
        let old = Store::open_at_version(&path, 6).expect("a database at version 6");
        assert_eq!(old.schema_version().unwrap(), 6);
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO reviews (id, repo, pr, head_sha, round, created_at)
                 VALUES (1, 'acme/portal', 412, 'aaaaaaa', 1, 1000),
                        (2, 'acme/portal', 412, 'bbbbbbb', 2, 2000);
             INSERT INTO findings
                 (review_id, seq, lens, severity, claim, disposition, created_at)
                 VALUES (1, 1, 'correctness', 'blocker', 'round one''s own F1',
                         'in_draft', 1001),
                        (2, 1, 'clarity', 'nit', 'round two''s unrelated F1',
                         'in_draft', 2001);",
        )
        .expect("two rounds, each numbering from 1");
    }

    let store = Store::open(&path).expect("reopen migrates");

    let second = store.get_finding(2, 1).expect("round two's F1");
    assert_eq!(second.claim, "round two's unrelated F1");
    assert_eq!(
        second.continues_review_id, None,
        "it continues nothing, because nothing said it did"
    );
    assert_eq!(
        store.rounds_open(2, 1).unwrap(),
        1,
        "open since round 2, not since round 1: the two F1s are different findings"
    );

    // And round 1's findings are still *offered* for carrying, which is a different thing
    // from being linked: an agent may decide the two are the same finding and carry it.
    let carryable = store.carryable(2).unwrap().expect("round 1 is behind it");
    assert_eq!(carryable.findings.len(), 1);
    assert_eq!(carryable.findings[0].claim, "round one's own F1");

    // The next new finding on the pull request takes 2, not 1 — the numbers already used
    // are read across the pull request even though the old rows were numbered per round.
    let fresh = raise(&store, 2, "the first finding numbered the new way", None);
    assert_eq!(
        fresh, 2,
        "one finding, one number, from here on; the old rows are left as they are"
    );
}

#[test]
fn late_is_unanswerable_when_a_rounds_commits_were_read_but_their_files_were_not() {
    // **A GitHub read that failed halfway used to produce a false accusation.** The gate
    // was "are there commits", not "is anything known about what they touched", so a
    // round whose commit list arrived with no file lists made every new finding read as
    // "the previous round could have caught this and did not".
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "round one", None);

    let second = round(&store, "9d02e71");
    store
        .put_round_commits(
            second,
            &[NewRoundCommit {
                sha: "a41c9e2".to_string(),
                summary: "Send an idempotency key with each delivery".to_string(),
                committed_at: None,
                files: Vec::new(),
            }],
        )
        .expect("a commit whose files nobody could read");

    let seq = raise(
        &store,
        second,
        "a new finding",
        Some("jobs/DeliverWebhook.php"),
    );
    assert_eq!(
        store.findings_in_round(second).unwrap()[..]
            .iter()
            .find(|f| f.finding.seq == seq)
            .unwrap()
            .late,
        None,
        "nothing is known about what changed, so nothing can be said about what was missed"
    );
}

#[test]
fn deciding_a_new_findings_status_does_not_erase_that_it_was_raised_late() {
    // **`round_status` is a mutable axis and was being used as a proxy for "raised in this
    // round".** Accepting the author's reason on a finding raised in round 2 used to flip
    // its lateness to null, losing the one fact DEV-873 added about it.
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    raise(&store, first, "round one", None);

    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();
    let seq = raise(
        &store,
        second,
        "on untouched code",
        Some("config/webhooks.php"),
    );

    let late_of = |store: &Store| {
        store.findings_in_round(second).unwrap()[..]
            .iter()
            .find(|f| f.finding.seq == seq)
            .unwrap()
            .late
    };

    assert_eq!(
        late_of(&store),
        Some(true),
        "raised in round 2 on untouched code"
    );

    store
        .set_finding_round_status(
            second,
            seq,
            &FindingCarry {
                status: RoundStatus::Accepted,
                by: DecidedBy::Developer,
                checked: Some("Their reason stands.".to_string()),
                commit: None,
            },
        )
        .expect("the developer accepts the author's reason");

    assert_eq!(
        late_of(&store),
        Some(true),
        "deciding what became of it does not change when it was raised"
    );
}

#[test]
fn a_finding_carried_with_the_status_new_is_still_not_judged_for_lateness() {
    // The converse. `new` is a legal status to carry with, and a carried finding was not
    // raised in this round whatever status it holds.
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(
        &store,
        first,
        "on untouched code",
        Some("config/webhooks.php"),
    );

    let second = round(&store, "9d02e71");
    store.put_round_commits(second, &three_commits()).unwrap();
    store
        .carry_finding(first, second, f1, &as_(RoundStatus::New))
        .expect("carried, oddly, as new");

    assert_eq!(
        store.findings_in_round(second).unwrap()[0].late,
        None,
        "it came from round 1, so round 1 cannot have missed it"
    );
}

#[test]
fn a_commit_that_is_a_word_rather_than_an_object_name_is_refused() {
    // **The API serves these columns as `^[0-9a-f]{7,40}$`**, and `--commit HEAD` is a
    // plausible thing for a person or an agent to type. Unchecked, it was a response that
    // failed its own contract — found by reviewing DEV-873.
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "a finding", None);

    for typed in ["HEAD", "main", "a41c9e", "zzzzzzz"] {
        match store.set_finding_round_status(
            first,
            f1,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: None,
                commit: Some(typed.to_string()),
            },
        ) {
            Err(StoreError::InvalidValue {
                field, expected, ..
            }) => {
                assert_eq!(field, "commit");
                assert!(expected.contains("hexadecimal"), "{expected}");
            }
            other => panic!("`{typed}` is not a commit: {other:?}"),
        }
    }

    // And a real abbreviation is taken, in either case.
    let fixed = store
        .set_finding_round_status(
            first,
            f1,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: None,
                commit: Some("A41C9E2".to_string()),
            },
        )
        .expect("seven hexadecimal characters is what git abbreviates to");
    assert_eq!(fixed.round_status_commit.as_deref(), Some("a41c9e2"));
}

#[test]
fn an_attributed_commit_that_is_a_word_is_refused_too() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");

    let refused = store.add_finding(
        first,
        NewFinding {
            lens: "l".to_string(),
            severity: Severity::Should,
            file: None,
            line: None,
            claim: "attributed to a branch name".to_string(),
            evidence: None,
            disposition: Disposition::InDraft,
            disposition_reason: None,
            rule_id: None,
            introduced_by_commit: Some("HEAD~1".to_string()),
            introduced_by_seq: None,
            ..Default::default()
        },
    );

    match refused {
        Err(StoreError::InvalidValue { field, .. }) => {
            assert_eq!(field, "introduced_by_commit")
        }
        other => panic!("a branch name is not a commit: {other:?}"),
    }
    assert!(
        store.list_findings(first).unwrap().is_empty(),
        "and nothing was written"
    );
}

#[test]
fn a_rounds_commit_that_is_not_an_object_name_is_refused() {
    let (_dir, store) = temp_store();
    let second = round(&store, "9d02e71");

    match store.put_round_commits(
        second,
        &[NewRoundCommit {
            sha: "not-a-sha".to_string(),
            summary: "Whatever this is".to_string(),
            committed_at: None,
            files: Vec::new(),
        }],
    ) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "sha"),
        other => panic!("the record keys on object names: {other:?}"),
    }
}

// -- what DEV-874 added, carried --------------------------------------------------------

/// A finding carrying everything DEV-874 added, with the awkward text that module exists
/// for: a tab, a trailing double space, a line that is nothing but spaces, a removal in the
/// middle, and a proof with a newline and a non-ASCII character in it.
fn a_finding_with_its_code_and_its_proof(claim: &str) -> NewFinding {
    NewFinding {
        lens: "sibling diff".to_string(),
        severity: Severity::Blocker,
        file: Some("jobs/DeliverWebhook.php".to_string()),
        line: Some(58),
        claim: claim.to_string(),
        hunk: Some(FindingHunk {
            file: "jobs/DeliverWebhook.php".to_string(),
            start_line: 56,
            lines: vec![
                HunkLine {
                    marker: HunkLineMarker::Unchanged,
                    text: "{".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Removed,
                    text: "    $client->post($this->endpoint->url);".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Added,
                    text: "\t$response = $client->post($this->endpoint->url);  ".to_string(),
                },
                HunkLine {
                    marker: HunkLineMarker::Added,
                    text: "   ".to_string(),
                },
            ],
        }),
        flagged_line: Some(57),
        inline_comment: Some("This retry re-sends the payload with no key.".to_string()),
        why_it_matters: Some("The customer is charged twice.".to_string()),
        proof: Some("attempt 1 key: 6f1c…e2\nattempt 2 key: b83a…07".to_string()),
        proof_note: Some("A throwaway test, not committed.".to_string()),
        verification: Verification::Reproduced,
        suggested_fix: Some("Send the delivery id as the key.".to_string()),
        fix_references: vec!["SendInvoiceEmail.php:44".to_string()],
        ..Default::default()
    }
}

#[test]
fn a_carried_finding_keeps_the_code_and_the_proof_it_was_about() {
    // **DEV-874 merged after this was written, and the guard test above named all seven of
    // its columns and all three of its tables.** They are carried for the same reason the
    // claim is: Review Desk does not fetch code, so a carried finding that lost its hunk,
    // its comment or its proof would be a different finding wearing the same number, and
    // nothing could get them back.
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = store
        .add_finding(
            first,
            a_finding_with_its_code_and_its_proof("no idempotency key"),
        )
        .expect("raise it with everything")
        .seq;

    let earlier = store.get_finding(first, f1).expect("read it back");
    let second = round(&store, "bbbbbbb");
    let carried = store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("carry it forward");

    assert_eq!(carried.flagged_line, earlier.flagged_line);
    assert_eq!(carried.inline_comment, earlier.inline_comment);
    assert_eq!(carried.why_it_matters, earlier.why_it_matters);
    assert_eq!(carried.proof, earlier.proof);
    assert_eq!(carried.proof_note, earlier.proof_note);
    assert_eq!(carried.verification, earlier.verification);
    assert_eq!(carried.suggested_fix, earlier.suggested_fix);
    assert_eq!(carried.fix_references, earlier.fix_references);

    // **The hunk byte for byte, which is the point of copying it in SQL.** The tab, the
    // trailing double space and the line that is nothing but spaces are what
    // `finding_evidence` exists to protect, and a round trip through this process is where
    // they would be lost.
    let hunk = carried.hunk.as_ref().expect("the hunk came with it");
    assert_eq!(hunk, earlier.hunk.as_ref().unwrap());
    assert_eq!(hunk.start_line, 56);
    assert_eq!(
        hunk.lines[2].text,
        "\t$response = $client->post($this->endpoint->url);  "
    );
    assert_eq!(hunk.lines[3].text, "   ");
    assert_eq!(
        hunk.numbered_lines()
            .iter()
            .map(|(number, _)| *number)
            .collect::<Vec<_>>(),
        [Some(56), None, Some(57), Some(58)],
        "and it still numbers from 56 with the removal taking no number"
    );
    assert!(
        hunk.draws_line(57),
        "so the flagged line is still a line the hunk draws"
    );
}

#[test]
fn carrying_a_finding_again_replaces_its_hunk_rather_than_doubling_it() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = store
        .add_finding(first, a_finding_with_its_code_and_its_proof("no key"))
        .unwrap()
        .seq;
    let second = round(&store, "bbbbbbb");

    store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .unwrap();
    let again = store
        .carry_finding(first, second, f1, &as_(RoundStatus::Accepted))
        .expect("the developer accepts the reason");

    let hunk = again.hunk.as_ref().expect("still there");
    assert_eq!(
        hunk.lines.len(),
        4,
        "re-carrying replaces the hunk; doubling it would renumber every line after the \
         first copy"
    );
    assert_eq!(
        again.fix_references,
        ["SendInvoiceEmail.php:44"],
        "and the references are replaced, not appended"
    );
}

#[test]
fn a_finding_with_no_hunk_carries_without_one() {
    let (_dir, store) = temp_store();
    let first = round(&store, "aaaaaaa");
    let f1 = raise(&store, first, "a claim about the whole change", None);
    let second = round(&store, "bbbbbbb");

    let carried = store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("every one of DEV-874's fields is optional and independent");

    assert_eq!(carried.hunk, None);
    assert_eq!(carried.proof, None);
    assert_eq!(carried.verification, Verification::Unknown);
    assert!(carried.fix_references.is_empty());
}

// -- the commit a line was measured on (DEV-978) ---------------------------------------

/// A finding with a line and the commit that line was counted on.
fn raise_measured(store: &Store, review: i64, claim: &str, line: i64, line_sha: &str) -> i64 {
    store
        .add_finding(
            review,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Should,
                file: Some("app/Export.php".to_string()),
                line: Some(line),
                line_sha: Some(line_sha.to_string()),
                claim: claim.to_string(),
                ..Default::default()
            },
        )
        .expect("raise a finding")
        .seq
}

#[test]
fn a_carried_finding_keeps_the_commit_its_line_was_measured_on() {
    let (_dir, store) = temp_store();
    let first = round(&store, "a1b2c3d");
    let f1 = raise_measured(&store, first, "the export is unbounded", 40, "a1b2c3d");

    let second = round(&store, "e4f5a6b");
    let carried = store
        .carry_finding(first, second, f1, &as_(RoundStatus::StillOpen))
        .expect("carry it forward");

    assert_eq!(
        carried.line,
        Some(40),
        "the carry copies the line, which it always did"
    );
    assert_eq!(
        carried.line_sha.as_deref(),
        Some("a1b2c3d"),
        "and the commit that line was counted on goes with it. Round two's head is \
         e4f5a6b, and pairing it with round one's line 40 is the whole of DEV-978"
    );
}

#[test]
fn a_finding_measured_on_the_new_head_says_so_rather_than_inheriting_the_old_one() {
    let (_dir, store) = temp_store();
    let first = round(&store, "a1b2c3d");
    raise_measured(&store, first, "the export is unbounded", 40, "a1b2c3d");

    let second = round(&store, "e4f5a6b");
    let f2 = raise_measured(&store, second, "the retry has no ceiling", 58, "e4f5a6b");
    let raised = store.get_finding(second, f2).expect("read it back");

    assert_eq!(
        raised.line_sha.as_deref(),
        Some("e4f5a6b"),
        "a line read on this round's head is measured on this round's head"
    );
}

#[test]
fn a_finding_whose_line_nobody_measured_says_nothing_rather_than_guessing() {
    let (_dir, store) = temp_store();
    let only = round(&store, "a1b2c3d");
    let f1 = raise(&store, only, "the description is wrong", None);

    assert_eq!(
        store.get_finding(only, f1).expect("read it back").line_sha,
        None,
        "the review's head is where the review is, not where a line was counted. \
         DEV-962 moves it under a session that is still reading, so defaulting to it \
         would have the record assert a commit nobody measured anything on"
    );
}

#[test]
fn a_measured_commit_that_is_a_word_is_refused() {
    let (_dir, store) = temp_store();
    let only = round(&store, "a1b2c3d");

    let refused = store.add_finding(
        only,
        NewFinding {
            lens: "sibling diff".to_string(),
            severity: Severity::Should,
            file: Some("app/Export.php".to_string()),
            line: Some(40),
            line_sha: Some("HEAD".to_string()),
            claim: "the export is unbounded".to_string(),
            ..Default::default()
        },
    );

    let message = refused.expect_err("`--line-sha HEAD` is a plausible thing to type");
    assert!(
        message.to_string().contains("line_sha"),
        "the refusal names the field: {message}"
    );
}

#[test]
fn a_blank_measured_commit_is_nothing_rather_than_a_link_to_nowhere() {
    let (_dir, store) = temp_store();
    let only = round(&store, "a1b2c3d");

    let raised = store
        .add_finding(
            only,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Should,
                file: Some("app/Export.php".to_string()),
                line: Some(40),
                line_sha: Some("   ".to_string()),
                claim: "the export is unbounded".to_string(),
                ..Default::default()
            },
        )
        .expect("whitespace is nothing said, not a refusal");

    assert_eq!(
        raised.line_sha, None,
        "an empty commit would compose `blob//app/Export.php`, which is a link to nothing"
    );
}
