//! The reads and updates the next issue's commands need.
//!
//! DEV-851 adds the command line; these tests check the store already answers what it
//! will ask, so that issue adds a surface rather than reshaping this crate.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, FindingRef, NewBrief, NewFinding, NewOutcome, NewRule, OutcomeKind,
    ReviewLinks, RuleScope, RuleStage, Severity, Store,
};

const REPO: &str = "devergehq/review-desk";

fn a_finding(claim: &str, severity: Severity) -> NewFinding {
    NewFinding {
        lens: "correctness".to_string(),
        severity,
        file: Some("src/lib.rs".to_string()),
        line: Some(1),
        claim: claim.to_string(),
        evidence: Some("the diff".to_string()),
        disposition: Disposition::InDraft,
        disposition_reason: None,
        rule_id: None,
    }
}

#[test]
fn a_head_commit_already_reviewed_is_found_rather_than_reviewed_again() {
    let (_dir, store) = temp_store();
    let first = store
        .create_review(new_review(REPO, 9, "head-one", 1))
        .unwrap();

    let found = store
        .find_review_by_head(REPO, 9, "head-one")
        .unwrap()
        .expect("the review of that head commit");

    assert_eq!(found.id, first.id, "the same review, not a new round");
    assert!(store
        .find_review_by_head(REPO, 9, "head-two")
        .unwrap()
        .is_none());
}

#[test]
fn the_next_round_follows_the_latest_one() {
    let (_dir, store) = temp_store();

    assert_eq!(
        store.latest_round(REPO, 9).unwrap(),
        None,
        "an unreviewed pull request is on no round"
    );

    store
        .create_review(new_review(REPO, 9, "head-one", 1))
        .unwrap();
    assert_eq!(store.latest_round(REPO, 9).unwrap(), Some(1));

    let next = store.latest_round(REPO, 9).unwrap().unwrap_or(0) + 1;
    let second = store
        .create_review(new_review(REPO, 9, "head-two", next))
        .unwrap();

    assert_eq!(second.round, 2);
    assert_eq!(store.latest_round(REPO, 9).unwrap(), Some(2));
    assert_eq!(
        store.latest_round(REPO, 10).unwrap(),
        None,
        "another pull request is unaffected"
    );
}

#[test]
fn linking_one_system_leaves_the_others_alone() {
    let (_dir, store) = temp_store();
    let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();

    let linked = store
        .set_review_links(
            review.id,
            ReviewLinks {
                session_id: Some("allele-1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(linked.session_id.as_deref(), Some("allele-1"));

    let linked = store
        .set_review_links(
            review.id,
            ReviewLinks {
                github_review_id: Some("gh-7".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        linked.session_id.as_deref(),
        Some("allele-1"),
        "setting the GitHub id did not clear the session id"
    );
    assert_eq!(linked.github_review_id.as_deref(), Some("gh-7"));
    assert_eq!(linked.ledger_id, None);
}

#[test]
fn a_brief_is_confirmed_once_and_replacing_it_undoes_that() {
    let (_dir, store) = temp_store();
    let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();

    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "first reading".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

    let confirmed = store.confirm_brief(review.id, Actor::Developer).unwrap();
    assert!(confirmed.is_confirmed());
    let at = confirmed.confirmed_at;

    let again = store.confirm_brief(review.id, Actor::Developer).unwrap();
    assert_eq!(
        again.confirmed_at, at,
        "confirming twice keeps the first time"
    );

    let replaced = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "second reading".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(replaced.problem, "second reading");
    assert!(
        !replaced.is_confirmed(),
        "the developer confirmed the old text, not this one"
    );
}

#[test]
fn findings_are_numbered_in_order_within_their_review() {
    let (_dir, store) = temp_store();
    let one = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();
    let two = store
        .create_review(new_review(REPO, 10, "head", 1))
        .unwrap();

    let a = store
        .add_finding(one.id, a_finding("first", Severity::Blocker))
        .unwrap();
    let b = store
        .add_finding(one.id, a_finding("second", Severity::Nit))
        .unwrap();
    let c = store
        .add_finding(one.id, a_finding("third", Severity::Question))
        .unwrap();
    let other = store
        .add_finding(two.id, a_finding("elsewhere", Severity::Should))
        .unwrap();

    assert_eq!((a.seq, b.seq, c.seq), (1, 2, 3));
    assert_eq!(other.seq, 1, "numbering restarts in another review");

    let listed = store.list_findings(one.id).unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(
        listed.iter().map(|f| f.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn a_disposition_is_changed_with_its_reason() {
    let (_dir, store) = temp_store();
    let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();
    let finding = store
        .add_finding(review.id, a_finding("a nit about naming", Severity::Nit))
        .unwrap();

    let changed = store
        .set_finding_disposition(
            review.id,
            finding.seq,
            Disposition::HeldBack,
            Some("Filed as a ticket instead".to_string()),
        )
        .unwrap();

    assert_eq!(changed.disposition, Disposition::HeldBack);
    assert_eq!(
        changed.disposition_reason.as_deref(),
        Some("Filed as a ticket instead")
    );

    let reread = store.get_finding(review.id, finding.seq).unwrap();
    assert_eq!(reread, changed);
}

#[test]
fn a_whole_review_reads_back_in_one_call() {
    let (_dir, store) = temp_store();
    let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();

    store
        .put_brief(
            review.id,
            NewBrief {
                problem: "the record is not kept".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    store.confirm_brief(review.id, Actor::Developer).unwrap();

    for claim in ["one", "two", "three"] {
        store
            .add_finding(review.id, a_finding(claim, Severity::Should))
            .unwrap();
    }
    store
        .add_outcome(
            FindingRef {
                review_id: review.id,
                seq: 2,
            },
            NewOutcome {
                kind: OutcomeKind::Rewritten,
                actor: Actor::Developer,
                detail: Some("Reworded to name the failure".to_string()),
                occurred_at: None,
            },
        )
        .unwrap();

    let full = store.full_review(review.id).unwrap();

    assert_eq!(full.review.id, review.id);
    assert!(full.brief.as_ref().unwrap().is_confirmed());
    assert_eq!(full.findings.len(), 3);
    assert_eq!(full.outcomes.len(), 1);
    assert_eq!(full.outcomes[0].finding_seq, 2);
    assert_eq!(full.outcomes[0].kind, OutcomeKind::Rewritten);

    // It has to survive a round trip through JSON, because that is how the next
    // issue's `--json` output is produced.
    let json = serde_json::to_string(&full).unwrap();
    let back: review_desk_store::FullReview = serde_json::from_str(&json).unwrap();
    assert_eq!(back, full);
}

#[test]
fn a_review_with_no_brief_still_reads_back() {
    let (_dir, store) = temp_store();
    let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();

    let full = store.full_review(review.id).unwrap();
    assert!(full.brief.is_none());
    assert!(full.findings.is_empty());
    assert!(full.outcomes.is_empty());
}

#[test]
fn rules_for_a_repository_are_the_global_ones_and_its_own() {
    let (_dir, store) = temp_store();

    let rule = |statement: &str, scope: RuleScope, stage: RuleStage| NewRule {
        statement: statement.to_string(),
        prevents: "a recurring failure".to_string(),
        scope,
        stage,
        origin_review_id: None,
        origin_findings: vec![],
        why_proposed: None,
        note: None,
    };

    store
        .create_rule(rule("global active", RuleScope::Global, RuleStage::Active))
        .unwrap();
    store
        .create_rule(rule("global lint", RuleScope::Global, RuleStage::Lint))
        .unwrap();
    store
        .create_rule(rule("global ci", RuleScope::Global, RuleStage::Ci))
        .unwrap();
    store
        .create_rule(rule(
            "global proposed",
            RuleScope::Global,
            RuleStage::Proposed,
        ))
        .unwrap();
    store
        .create_rule(rule(
            "global retired",
            RuleScope::Global,
            RuleStage::Retired,
        ))
        .unwrap();
    store
        .create_rule(rule(
            "ours active",
            RuleScope::Repo(REPO.to_string()),
            RuleStage::Active,
        ))
        .unwrap();
    store
        .create_rule(rule(
            "theirs active",
            RuleScope::Repo("devergehq/locus".to_string()),
            RuleStage::Active,
        ))
        .unwrap();

    let applying = store.rules_for_repo(REPO).unwrap();
    let statements: Vec<&str> = applying.iter().map(|r| r.statement.as_str()).collect();

    assert_eq!(
        statements,
        vec!["global active", "global lint", "global ci", "ours active"],
        "global rules first, then this repository's; only those in force"
    );
    assert!(
        !statements.contains(&"theirs active"),
        "another repository's rules do not apply"
    );
    assert!(!statements.contains(&"global proposed"));
    assert!(!statements.contains(&"global retired"));

    // Every rule is still there; only the applying view is filtered.
    assert_eq!(store.list_rules().unwrap().len(), 7);
}

#[test]
fn promoting_a_rule_brings_it_into_force() {
    let (_dir, store) = temp_store();
    let proposed = store
        .create_rule(NewRule {
            statement: "evidence or it is a preference".to_string(),
            prevents: "preference presented as a defect".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Proposed,
            origin_review_id: None,
            origin_findings: vec![],
            why_proposed: None,
            note: None,
        })
        .unwrap();

    assert!(store.rules_for_repo(REPO).unwrap().is_empty());

    let promoted = store
        .set_rule_stage(proposed.id, RuleStage::Active, None)
        .unwrap();
    assert_eq!(promoted.stage, RuleStage::Active);
    assert_eq!(store.rules_for_repo(REPO).unwrap().len(), 1);
}

#[test]
fn the_store_survives_being_closed_and_reopened() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("review-desk.db");

    let review_id = {
        let store = Store::open(&path).unwrap();
        let review = store.create_review(new_review(REPO, 9, "head", 1)).unwrap();
        store
            .add_finding(review.id, a_finding("persisted", Severity::Should))
            .unwrap();
        review.id
    };

    let store = Store::open(&path).unwrap();
    assert_eq!(store.list_findings(review_id).unwrap().len(), 1);
    assert_eq!(store.get_finding(review_id, 1).unwrap().claim, "persisted");
}

#[test]
fn opening_a_head_commit_already_reviewed_returns_it_rather_than_starting_a_round() {
    let (_dir, store) = temp_store();

    let first = store
        .open_review("devergehq/review-desk", 21, "aaa111")
        .expect("open");
    let again = store
        .open_review("devergehq/review-desk", 21, "aaa111")
        .expect("open again");

    assert!(first.created, "the first call creates the review");
    assert!(!again.created, "the second call returns what was there");
    assert_eq!(first.review.id, again.review.id);
    assert_eq!(first.review.round, 1);
    assert_eq!(again.review.round, 1);
}

#[test]
fn opening_a_new_head_commit_starts_the_next_round() {
    let (_dir, store) = temp_store();

    let round_one = store
        .open_review("devergehq/review-desk", 21, "aaa111")
        .expect("round one");
    let round_two = store
        .open_review("devergehq/review-desk", 21, "bbb222")
        .expect("round two");

    assert_eq!(round_one.review.round, 1);
    assert_eq!(round_two.review.round, 2);
    assert!(round_two.created);
    assert_ne!(round_one.review.id, round_two.review.id);
}

#[test]
fn an_earlier_head_commit_is_still_returned_once_later_rounds_exist() {
    // The whole point of keying on the head commit: a session that reopens the commit
    // it reviewed two rounds ago must be handed that round back, not handed a round
    // three it never asked for.
    let (_dir, store) = temp_store();

    let first = store
        .open_review("devergehq/review-desk", 21, "aaa111")
        .expect("round one");
    store
        .open_review("devergehq/review-desk", 21, "bbb222")
        .expect("round two");

    let reopened = store
        .open_review("devergehq/review-desk", 21, "aaa111")
        .expect("reopen round one");

    assert!(!reopened.created);
    assert_eq!(reopened.review.id, first.review.id);
    assert_eq!(reopened.review.round, 1);
    assert_eq!(
        store.latest_round("devergehq/review-desk", 21).unwrap(),
        Some(2),
        "reopening an old commit must not have started a round"
    );
}

#[test]
fn opening_a_review_of_a_malformed_repository_is_refused() {
    let (_dir, store) = temp_store();

    let error = store
        .open_review("not-a-repo", 21, "aaa111")
        .expect_err("a repository must be owner/repo");

    assert!(error.is_refusal());
    assert_eq!(error.code(), "invalid_value");
}

#[test]
fn a_head_commit_with_stray_whitespace_is_the_same_commit() {
    // Identity, not cosmetics. `find_review_by_head` compares under SQLite's binary
    // collation, so storing an untrimmed head commit would make `aaa111 ` a different
    // commit from `aaa111` — and because `latest_round` counts rounds per pull request,
    // the round it then started would be spent for good.
    let (_dir, store) = temp_store();

    let opened = store
        .open_review("devergehq/review-desk", 31, " aaa111 ")
        .expect("open");
    assert_eq!(opened.review.head_sha, "aaa111");

    let again = store
        .open_review(" devergehq/review-desk ", 31, "aaa111")
        .expect("open again");

    assert!(!again.created, "the same commit must not start a round");
    assert_eq!(again.review.id, opened.review.id);
    assert_eq!(
        store.latest_round("devergehq/review-desk", 31).unwrap(),
        Some(1)
    );
}

#[test]
fn one_repository_spelled_two_ways_is_one_repository() {
    // GitHub repository names are not case-sensitive identity. Under SQLite's binary
    // collation they would be: one pull request would get two parallel round sequences,
    // its findings would be split across two reviews that cannot see each other, and a
    // repository's own rules would be invisible under the other spelling.
    let (_dir, store) = temp_store();

    let opened = store
        .open_review("DevergeHQ/Review-Desk", 41, "aaa111")
        .expect("open");
    assert_eq!(opened.review.repo, "devergehq/review-desk");

    let again = store
        .open_review("devergehq/review-desk", 41, "aaa111")
        .expect("open again");

    assert!(!again.created, "the same repository must not start a round");
    assert_eq!(again.review.id, opened.review.id);
    assert_eq!(
        store.latest_round("devergehq/review-desk", 41).unwrap(),
        Some(1)
    );
}

#[test]
fn one_head_commit_spelled_two_ways_is_one_commit() {
    // A git object name is lowercase hexadecimal; `git rev-parse` never emits anything
    // else, so an uppercase one is a caller that normalised badly. Treating it as a
    // different commit would start a round for a commit already reviewed, and
    // `latest_round` counts per pull request, so that round number is spent for good.
    let (_dir, store) = temp_store();

    let opened = store
        .open_review("devergehq/review-desk", 42, "C0FFEE1")
        .expect("open");
    assert_eq!(opened.review.head_sha, "c0ffee1");

    let again = store
        .open_review("devergehq/review-desk", 42, "c0ffee1")
        .expect("open again");

    assert!(!again.created);
    assert_eq!(again.review.id, opened.review.id);
    assert_eq!(
        store.latest_round("devergehq/review-desk", 42).unwrap(),
        Some(1),
        "the same commit in another case must not have started a round"
    );
}

#[test]
fn a_rule_is_stored_under_the_name_a_lookup_will_use() {
    // `create_rule` checked a trimmed repository and inserted the raw one, so a rule
    // seeded with stray whitespace or another case could never be matched by
    // `rules_for_repo` — a rule in force that nothing can ever cite.
    let (_dir, store) = temp_store();

    store
        .create_rule(NewRule {
            statement: "the store is the only code that opens the database".to_string(),
            prevents: "two ways into the record".to_string(),
            scope: RuleScope::Repo(" DevergeHQ/Review-Desk ".to_string()),
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: Vec::new(),
            why_proposed: None,
            note: None,
        })
        .expect("seed rule");

    for spelling in [
        "devergehq/review-desk",
        " devergehq/review-desk ",
        "DevergeHQ/Review-Desk",
    ] {
        assert_eq!(
            store.rules_for_repo(spelling).unwrap().len(),
            1,
            "{spelling} must find the rule"
        );
    }
}

#[test]
fn a_brief_with_no_problem_in_it_is_refused() {
    // `problem` is required by serde, but an empty string satisfies that, and an empty
    // brief can still be confirmed — leaving a developer's assent to nothing on record.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/review-desk", 43, "aaa111", 1))
        .unwrap();

    let error = store
        .put_brief(
            review.id,
            NewBrief {
                problem: "   ".to_string(),
                ..Default::default()
            },
        )
        .expect_err("an empty problem is not a brief");

    assert!(error.is_refusal());
    assert_eq!(error.code(), "invalid_value");
}
