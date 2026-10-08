//! The queries that are not about one pull request: what is waiting on the developer
//! anywhere, and which repositories there are.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, DraftStatus, FindingRef, NewDraft, NewFinding, NewOutcome, NewRule,
    OutcomeKind, Review, ReviewEvent, ReviewLayerState, RuleScope, RuleStage, Severity, Store,
};

/// A review on a repository, at round 1.
fn seed(store: &Store, repo: &str, pr: i64, head: &str) -> Review {
    store
        .create_review(new_review(repo, pr, head, 1))
        .expect("create review")
}

/// A brief on a review, unconfirmed.
fn brief(store: &Store, review: i64) {
    store
        .put_brief(
            review,
            review_desk_store::NewBrief {
                problem: "a problem worth confirming".to_string(),
                ..Default::default()
            },
        )
        .expect("put brief");
}

fn ids(reviews: &[Review]) -> Vec<i64> {
    reviews.iter().map(|review| review.id).collect()
}

#[test]
fn briefs_waiting_to_be_confirmed_are_found_across_every_repository() {
    let (_dir, store) = temp_store();
    let portal = seed(&store, "acme/portal", 412, "3f9c2ab");
    let billing = seed(&store, "acme/billing-api", 88, "bbbbbbb");
    let confirmed = seed(&store, "acme/portal", 407, "ccccccc");
    let no_brief = seed(&store, "acme/portal", 409, "ddddddd");

    brief(&store, portal.id);
    brief(&store, billing.id);
    brief(&store, confirmed.id);
    store
        .confirm_brief(confirmed.id, Actor::Developer)
        .expect("confirm");

    let waiting = store.reviews_with_unconfirmed_brief(None).unwrap();

    assert_eq!(
        ids(&waiting),
        [billing.id, portal.id],
        "newest first, across repositories"
    );
    assert!(
        !ids(&waiting).contains(&no_brief.id),
        "a review with no brief has nothing to confirm"
    );
    assert!(!ids(&waiting).contains(&confirmed.id));

    assert_eq!(
        ids(&store
            .reviews_with_unconfirmed_brief(Some("ACME/Portal"))
            .unwrap()),
        [portal.id],
        "and one repository can be asked about, by any spelling of its name"
    );
}

#[test]
fn drafts_are_found_by_the_status_they_are_waiting_at() {
    let (_dir, store) = temp_store();
    let ready = seed(&store, "acme/portal", 412, "3f9c2ab");
    let approved = seed(&store, "acme/portal", 407, "ccccccc");
    let drafting = seed(&store, "acme/billing-api", 88, "bbbbbbb");

    store.put_draft(ready.id, NewDraft::ready("ready")).unwrap();
    store
        .put_draft(approved.id, NewDraft::ready("approved"))
        .unwrap();
    store
        .approve_draft(approved.id, ReviewEvent::Comment, Actor::Developer)
        .unwrap();
    store
        .put_draft(
            drafting.id,
            NewDraft {
                body: "half".to_string(),
                status: DraftStatus::Drafting,
                lint: None,
            },
        )
        .unwrap();

    assert_eq!(
        ids(&store
            .reviews_with_draft_status(DraftStatus::Ready, None)
            .unwrap()),
        [ready.id],
        "the Inbox's approve-a-draft group"
    );
    assert_eq!(
        ids(&store
            .reviews_with_draft_status(DraftStatus::Approved, None)
            .unwrap()),
        [approved.id],
        "and what a review session polls to learn it may post"
    );
    assert_eq!(
        ids(&store
            .reviews_with_draft_status(DraftStatus::Posted, None)
            .unwrap()),
        [] as [i64; 0]
    );
}

#[test]
fn a_review_an_author_or_the_harvester_acted_on_is_found() {
    let (_dir, store) = temp_store();
    let replied = seed(&store, "acme/portal", 398, "aaaaaaa");
    let quiet = seed(&store, "acme/portal", 412, "3f9c2ab");
    let only_mine = seed(&store, "acme/portal", 407, "ccccccc");

    let finding = |store: &Store, review: i64| {
        store
            .add_finding(
                review,
                NewFinding {
                    lens: "l".to_string(),
                    severity: Severity::Should,
                    file: None,
                    line: None,
                    claim: "c".to_string(),
                    evidence: None,
                    disposition: Disposition::InDraft,
                    disposition_reason: None,
                    rule_id: None,
                },
            )
            .unwrap()
            .seq
    };
    let outcome = |store: &Store, review: i64, seq: i64, actor: Actor, at: i64| {
        store
            .add_outcome(
                FindingRef {
                    review_id: review,
                    seq,
                },
                NewOutcome {
                    kind: OutcomeKind::Rejected,
                    actor,
                    detail: Some("the export is chunked upstream".to_string()),
                    occurred_at: Some(at),
                },
            )
            .unwrap();
    };

    let seq = finding(&store, replied.id);
    outcome(&store, replied.id, seq, Actor::Author, 2_000);
    let seq = finding(&store, only_mine.id);
    outcome(&store, only_mine.id, seq, Actor::Developer, 2_000);
    finding(&store, quiet.id);

    let acted = store.reviews_with_author_activity(None, None).unwrap();
    assert_eq!(
        ids(&acted),
        [replied.id],
        "the developer's own outcomes are not the author replying"
    );

    assert!(
        store
            .reviews_with_author_activity(None, Some(3_000))
            .unwrap()
            .is_empty(),
        "and the window is a window"
    );

    // The harvester counts too: it is how most of this arrives.
    let seq = finding(&store, quiet.id);
    outcome(&store, quiet.id, seq, Actor::Harvester, 5_000);
    assert_eq!(
        ids(&store
            .reviews_with_author_activity(None, Some(3_000))
            .unwrap()),
        [quiet.id]
    );
}

#[test]
fn a_review_in_progress_reports_the_layer_it_got_to() {
    let (_dir, store) = temp_store();
    let running = seed(&store, "acme/portal", 409, "ddddddd");
    let waiting = seed(&store, "acme/portal", 412, "3f9c2ab");
    let finished = seed(&store, "acme/billing-api", 88, "bbbbbbb");

    // A session that left layer 2 running and got as far as layer 5.
    store
        .set_review_layer(running.id, 2, ReviewLayerState::Running, Some("reading"))
        .unwrap();
    store
        .set_review_layer(running.id, 5, ReviewLayerState::Running, Some("6 findings"))
        .unwrap();
    store
        .set_review_layer(
            waiting.id,
            6,
            ReviewLayerState::WaitingOnYou,
            Some("waiting"),
        )
        .unwrap();
    store
        .set_review_layer(finished.id, 7, ReviewLayerState::Done, Some("harvested"))
        .unwrap();

    let in_progress = store.reviews_in_progress(None).unwrap();

    assert_eq!(
        in_progress.len(),
        1,
        "waiting on the developer is not progress — those reviews are the other groups"
    );
    assert_eq!(in_progress[0].review.id, running.id);
    assert_eq!(
        in_progress[0].layer.number, 5,
        "the furthest running layer, not the first one left running"
    );
    assert_eq!(in_progress[0].layer.detail.as_deref(), Some("6 findings"));
    assert_eq!(
        in_progress[0].layer.name,
        review_desk_store::ReviewLayerName::CodeReview
    );
}

#[test]
fn filed_nit_tickets_are_found_with_the_nits_they_cover() {
    let (_dir, store) = temp_store();
    let filed = seed(&store, "acme/portal", 412, "3f9c2ab");
    let asked = seed(&store, "acme/portal", 407, "ccccccc");

    for review in [filed.id, asked.id] {
        for claim in ["a nit", "another nit"] {
            store
                .add_finding(
                    review,
                    NewFinding {
                        lens: "repetition".to_string(),
                        severity: Severity::Nit,
                        file: None,
                        line: None,
                        claim: claim.to_string(),
                        evidence: None,
                        disposition: Disposition::HeldBack,
                        disposition_reason: Some("one ticket for all of them".to_string()),
                        rule_id: None,
                    },
                )
                .unwrap();
        }
    }
    store.request_nit_ticket(filed.id, Some("ABC-240")).unwrap();
    store.request_nit_ticket(asked.id, None).unwrap();

    let tickets = store.filed_nit_tickets(None, None).unwrap();

    assert_eq!(tickets.len(), 1, "asked for is not filed");
    assert_eq!(tickets[0].review_id, filed.id);
    assert_eq!(tickets[0].ticket_id.as_deref(), Some("ABC-240"));
    assert_eq!(
        tickets[0].finding_seqs,
        [1, 2],
        "and it covers both of that review's held-back nits"
    );
}

#[test]
fn the_repository_list_counts_open_reviews_and_that_repositorys_rules() {
    let (_dir, store) = temp_store();
    let posted = seed(&store, "acme/portal", 407, "ccccccc");
    seed(&store, "acme/portal", 412, "3f9c2ab");
    seed(&store, "acme/billing-api", 88, "bbbbbbb");

    store
        .set_review_links(
            posted.id,
            review_desk_store::ReviewLinks {
                github_review_id: Some("gh-review-3312".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    let rule = |repo: Option<&str>, stage: RuleStage| NewRule {
        statement: "a rule".to_string(),
        prevents: "a failure".to_string(),
        scope: match repo {
            Some(repo) => RuleScope::Repo(repo.to_string()),
            None => RuleScope::Global,
        },
        stage,
        origin_review_id: None,
        origin_findings: vec![],
        why_proposed: None,
        note: None,
    };
    store
        .create_rule(rule(Some("acme/portal"), RuleStage::Active))
        .unwrap();
    store
        .create_rule(rule(Some("acme/portal"), RuleStage::Ci))
        .unwrap();
    store
        .create_rule(rule(Some("acme/portal"), RuleStage::Proposed))
        .unwrap();
    store.create_rule(rule(None, RuleStage::Active)).unwrap();
    // A repository known only by a rule of its own still belongs in the picker.
    store
        .create_rule(rule(Some("acme/docs"), RuleStage::Active))
        .unwrap();

    let repos = store.list_repos().unwrap();

    assert_eq!(
        repos.iter().map(|r| r.repo.as_str()).collect::<Vec<_>>(),
        ["acme/billing-api", "acme/docs", "acme/portal"],
        "alphabetical: a picker ordered by a count rearranges itself as work lands"
    );

    let portal = &repos[2];
    assert_eq!(
        portal.open_reviews, 1,
        "a review with a posted GitHub review is not open"
    );
    assert_eq!(
        portal.active_rules, 2,
        "in force and scoped to this repository: not the proposal, not the global one"
    );

    let docs = &repos[1];
    assert_eq!(docs.open_reviews, 0);
    assert_eq!(docs.active_rules, 1);
}
