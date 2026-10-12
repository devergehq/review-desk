//! The queries that are not about one pull request: what is waiting on the developer
//! anywhere, and which repositories there are.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{
    Actor, DismissedReviews, Disposition, DraftStatus, FindingRef, NewDraft, NewFinding,
    NewOutcome, NewRule, OutcomeKind, PrState, Review, ReviewEvent, ReviewLayerName,
    ReviewLayerState, RuleScope, RuleStage, Severity, Store,
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
                sections: None,
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
                    ..Default::default()
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
                        ..Default::default()
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

/* --------------------------- posted, and still open --------------------------- */

/// A review posted as this event, with a draft behind it. Returns its id.
fn post(store: &Store, review: i64, event: ReviewEvent, github_review: &str) {
    store
        .put_draft(review, NewDraft::ready("a body worth posting"))
        .expect("put draft");
    store
        .approve_draft(review, event, Actor::Developer)
        .expect("approve");
    store
        .post_draft(review, github_review)
        .expect("report posted");
}

#[test]
fn a_posted_review_whose_pull_request_is_open_is_waiting_on_the_author() {
    let (_dir, store) = temp_store();
    let open = seed(&store, "acme/portal", 398, "aaaaaaa");
    let never_looked = seed(&store, "acme/portal", 399, "eeeeeee");
    let merged = seed(&store, "acme/portal", 400, "fffffff");
    let closed = seed(&store, "acme/billing-api", 88, "bbbbbbb");
    let not_posted = seed(&store, "acme/portal", 412, "3f9c2ab");

    post(&store, open.id, ReviewEvent::RequestChanges, "PRR_open");
    post(
        &store,
        never_looked.id,
        ReviewEvent::Comment,
        "PRR_unlooked",
    );
    post(&store, merged.id, ReviewEvent::Approve, "PRR_merged");
    post(&store, closed.id, ReviewEvent::Comment, "PRR_closed");
    store
        .put_draft(not_posted.id, NewDraft::ready("ready, not posted"))
        .unwrap();

    store
        .set_review_pr_state(open.id, PrState::Open, None)
        .unwrap();
    store
        .set_review_pr_state(merged.id, PrState::Merged, Some(9_000))
        .unwrap();
    store
        .set_review_pr_state(closed.id, PrState::Closed, Some(9_000))
        .unwrap();
    store.mark_review_harvested(open.id, 8_000).unwrap();

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let found: Vec<i64> = waiting.iter().map(|item| item.review.id).collect();

    assert!(
        found.contains(&open.id),
        "a posted review, pull request open"
    );
    assert!(
        found.contains(&never_looked.id),
        "and one nobody has harvested: its state is unknown, which is why it is here"
    );
    assert!(
        !found.contains(&merged.id),
        "a merged pull request has no author left to wait for"
    );
    assert!(!found.contains(&closed.id), "nor has a closed one");
    assert!(
        !found.contains(&not_posted.id),
        "and a draft that was never posted is not waiting on anybody"
    );
    assert_eq!(found.len(), 2);

    assert_eq!(
        ids_of(
            &store
                .posted_reviews_awaiting_author(Some("ACME/Portal"))
                .unwrap()
        ),
        found,
        "one repository can be asked about, by any spelling of its name"
    );
    assert!(
        store
            .posted_reviews_awaiting_author(Some("acme/billing-api"))
            .unwrap()
            .is_empty(),
        "and the only posted review there is closed"
    );
}

/// The review ids of a posted-review list, in the order it came back.
fn ids_of(items: &[review_desk_store::PostedReview]) -> Vec<i64> {
    items.iter().map(|item| item.review.id).collect()
}

#[test]
fn a_posted_review_carries_when_it_was_posted_and_as_what() {
    let (_dir, store) = temp_store();
    let review = seed(&store, "acme/portal", 398, "aaaaaaa");
    post(&store, review.id, ReviewEvent::RequestChanges, "PRR_kwDOA1");

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let item = waiting.first().expect("the review is in the group");

    assert_eq!(item.posted_as, Some(ReviewEvent::RequestChanges));
    assert!(
        item.posted_at.is_some(),
        "the draft's own posted_at is what put it here, so it is always there"
    );
    assert_eq!(
        item.review.github_review_id.as_deref(),
        Some("PRR_kwDOA1"),
        "and the review it became on GitHub, for a link out"
    );
    assert!(
        item.review.harvested_at.is_none(),
        "nothing has harvested it, which is what makes the counts below unmeasured"
    );
    assert_eq!(item.response, review_desk_store::AuthorResponse::default());
}

#[test]
fn a_posted_review_carries_what_the_harvester_recorded() {
    let (_dir, store) = temp_store();
    let review = seed(&store, "acme/portal", 398, "aaaaaaa");

    let seq = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "boundary".to_string(),
                severity: Severity::Should,
                claim: "the retry writes the wrong bucket".to_string(),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .unwrap()
        .seq;
    post(&store, review.id, ReviewEvent::RequestChanges, "PRR_kwDOA1");

    let recorded = |kind: OutcomeKind, actor: Actor, at: i64| {
        store
            .add_outcome(
                FindingRef {
                    review_id: review.id,
                    seq,
                },
                NewOutcome {
                    kind,
                    actor,
                    detail: None,
                    occurred_at: Some(at),
                },
            )
            .unwrap();
    };
    recorded(OutcomeKind::Replied, Actor::Author, 1_000);
    recorded(OutcomeKind::Replied, Actor::Author, 1_100);
    recorded(OutcomeKind::ThreadResolved, Actor::Author, 1_200);
    // The harvester's own observation about the code, which is nobody's act.
    recorded(OutcomeKind::CodeChangedAfter, Actor::Harvester, 1_300);
    // **The same finding, seen changed again on a later commit.** The harvester keys this
    // outcome on the commit it saw the change in, so a finding whose file is touched by
    // three pushes holds three rows — and a count of the rows would say three findings
    // changed on a review that holds one.
    recorded(OutcomeKind::CodeChangedAfter, Actor::Harvester, 1_350);
    // A kind that is not the author answering, so the counts cannot be a count of
    // outcomes.
    recorded(OutcomeKind::Kept, Actor::Developer, 1_400);
    store.mark_review_harvested(review.id, 2_000).unwrap();

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let item = waiting.first().expect("the review is in the group");

    assert_eq!(item.response.replies, 2);
    assert_eq!(item.response.threads_resolved, 1);
    assert_eq!(
        item.response.code_changed_after, 1,
        "one finding, however many commits it was seen changed in"
    );
    assert_eq!(
        item.review.harvested_at,
        Some(2_000),
        "and something has looked, which is what makes those three figures"
    );
}

#[test]
fn only_the_authors_own_replies_and_resolutions_are_counted_as_theirs() {
    let (_dir, store) = temp_store();
    let review = seed(&store, "acme/portal", 398, "aaaaaaa");
    let seq = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "boundary".to_string(),
                severity: Severity::Should,
                claim: "the retry writes the wrong bucket".to_string(),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .unwrap()
        .seq;
    post(&store, review.id, ReviewEvent::RequestChanges, "PRR_kwDOA1");

    // **What the harvester writes when the reply is not the author's.** `actor_for` maps
    // any login that is not the pull request's author to `developer` — and on most
    // repositories the author cannot resolve a review thread, so the reviewer does.
    for (kind, actor) in [
        (OutcomeKind::Replied, Actor::Developer),
        (OutcomeKind::ThreadResolved, Actor::Developer),
    ] {
        store
            .add_outcome(
                FindingRef {
                    review_id: review.id,
                    seq,
                },
                NewOutcome {
                    kind,
                    actor,
                    detail: None,
                    occurred_at: Some(1_000),
                },
            )
            .unwrap();
    }
    store.mark_review_harvested(review.id, 2_000).unwrap();

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let item = waiting.first().expect("the review is in the group");

    assert_eq!(
        (item.response.replies, item.response.threads_resolved),
        (0, 0),
        "the developer's own reply and their own resolution are not the author answering"
    );
}

#[test]
fn a_posted_review_says_how_many_of_its_findings_a_reply_could_attach_to() {
    let (_dir, store) = temp_store();
    let review = seed(&store, "acme/portal", 398, "aaaaaaa");
    let finding = |claim: &str| {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "boundary".to_string(),
                    severity: Severity::Should,
                    claim: claim.to_string(),
                    disposition: Disposition::InDraft,
                    ..Default::default()
                },
            )
            .unwrap()
            .seq
    };
    let linked = finding("the retry writes the wrong bucket");
    finding("the backoff has no ceiling");
    post(&store, review.id, ReviewEvent::RequestChanges, "PRR_kwDOA1");
    store
        .set_finding_github_comment(review.id, linked, "IC_kwDOA1")
        .expect("the comment the finding was posted as");
    store.mark_review_harvested(review.id, 2_000).unwrap();

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let item = waiting.first().expect("the review is in the group");

    assert_eq!(item.findings, 2);
    assert_eq!(
        item.findings_linked, 1,
        "the harvester reads a thread through this, so it is what says a reply could \
         have been seen at all"
    );
}

#[test]
fn a_superseded_round_stops_waiting_on_the_author() {
    let (_dir, store) = temp_store();
    // Round 1 was posted. The author then pushed — which is the event this group waits
    // for — so a session opened round 2 on the new head commit.
    let first = store
        .create_review(new_review("acme/portal", 398, "aaaaaaa", 1))
        .expect("round 1");
    post(&store, first.id, ReviewEvent::RequestChanges, "PRR_round1");

    assert_eq!(
        ids_of(&store.posted_reviews_awaiting_author(None).unwrap()),
        [first.id],
        "while it is the only round, it is the one waiting"
    );

    let second = store
        .create_review(new_review("acme/portal", 398, "eeeeeee", 2))
        .expect("round 2");

    assert!(
        store
            .posted_reviews_awaiting_author(None)
            .unwrap()
            .is_empty(),
        "round 1's wait ended at the commit that opened round 2, and nothing would ever \
         move its state again to retract it"
    );

    post(&store, second.id, ReviewEvent::Comment, "PRR_round2");
    assert_eq!(
        ids_of(&store.posted_reviews_awaiting_author(None).unwrap()),
        [second.id],
        "and one pull request is one row, whatever round it is on"
    );
}

#[test]
fn a_review_posted_through_review_link_is_still_a_posted_review() {
    let (_dir, store) = temp_store();
    let review = seed(&store, "acme/portal", 398, "aaaaaaa");

    // **No draft at all.** `review link --github-review` records the review as posted
    // without a draft transition, and `list_repos` has always counted that as the review
    // no longer being open — so a group that read `posted_at` alone left it in no group
    // on any page, which is the complaint DEV-930 was opened about.
    store
        .set_review_links(
            review.id,
            review_desk_store::ReviewLinks {
                github_review_id: Some("PRR_linked".to_string()),
                ..Default::default()
            },
        )
        .expect("the posted review's id");

    let waiting = store.posted_reviews_awaiting_author(None).unwrap();
    let item = waiting.first().expect("the review is in the group");
    assert_eq!(item.review.id, review.id);
    assert!(
        item.posted_at.is_none(),
        "it is posted and nothing holds the time, which the item says rather than guesses"
    );
    assert!(item.posted_as.is_none());
}

#[test]
fn posted_reviews_come_back_newest_posting_first() {
    let (_dir, store) = temp_store();
    let first = seed(&store, "acme/portal", 398, "aaaaaaa");
    let second = seed(&store, "acme/portal", 399, "eeeeeee");

    post(&store, first.id, ReviewEvent::Comment, "PRR_first");
    post(&store, second.id, ReviewEvent::Comment, "PRR_second");

    assert_eq!(
        ids_of(&store.posted_reviews_awaiting_author(None).unwrap()),
        [second.id, first.id],
        "the newest posting leads"
    );
}

/* ------------------------------ every review ------------------------------ */

#[test]
fn every_review_is_listed_newest_first_with_the_step_it_reached() {
    let (_dir, store) = temp_store();
    let first = seed(&store, "acme/portal", 398, "aaaaaaa");
    let second = seed(&store, "acme/billing-api", 88, "bbbbbbb");
    let untouched = seed(&store, "acme/portal", 412, "3f9c2ab");

    // Step 6 is waiting and step 7 is pending, which is every review a session has
    // drafted: the furthest step that has *started* is 6, and a rule that read the last
    // step written would say 7.
    store
        .set_review_layer(first.id, 1, ReviewLayerState::Done, Some("passed"))
        .unwrap();
    store
        .set_review_layer(first.id, 6, ReviewLayerState::WaitingOnYou, Some("yours"))
        .unwrap();
    store
        .set_review_layer(
            first.id,
            7,
            ReviewLayerState::Pending,
            Some("after posting"),
        )
        .unwrap();
    store
        .set_review_layer(second.id, 3, ReviewLayerState::Skipped, None)
        .unwrap();

    let listed = store
        .reviews_at_step(None, DismissedReviews::default())
        .unwrap();
    assert_eq!(
        listed.iter().map(|at| at.review.id).collect::<Vec<_>>(),
        [untouched.id, second.id, first.id],
        "newest first, by the order they were opened"
    );

    let step_of = |id: i64| {
        listed
            .iter()
            .find(|at| at.review.id == id)
            .expect("the review is listed")
            .step
            .clone()
    };

    let step = step_of(first.id).expect("a step");
    assert_eq!(step.number, 6, "the furthest step that is not pending");
    assert_eq!(step.name, ReviewLayerName::Draft);
    assert_eq!(step.state, ReviewLayerState::WaitingOnYou);
    assert_eq!(step.detail.as_deref(), Some("yours"));

    let step = step_of(second.id).expect("a step");
    assert_eq!(
        (step.number, step.state, step.detail),
        (3, ReviewLayerState::Skipped, None),
        "a step deliberately not run is a step the review got to"
    );

    assert!(
        step_of(untouched.id).is_none(),
        "a review nothing has reported on is at no step, which is not step 1 pending"
    );
}

#[test]
fn a_listed_review_says_which_round_of_how_many_it_is() {
    let (_dir, store) = temp_store();
    let first = store
        .create_review(new_review("acme/portal", 398, "aaaaaaa", 1))
        .expect("round 1");
    let third = store
        .create_review(new_review("acme/portal", 398, "ccccccc", 3))
        .expect("round 3");
    let alone = seed(&store, "acme/billing-api", 88, "bbbbbbb");

    let listed = store
        .reviews_at_step(None, DismissedReviews::default())
        .unwrap();
    let rounds = |id: i64| {
        listed
            .iter()
            .find(|at| at.review.id == id)
            .expect("the review is listed")
            .rounds_on_pr
    };

    // "round 1 of 3" rather than a bare 1: a round number alone, in a column beside the
    // repository and the pull request, reads as a count of rework against their author.
    assert_eq!((rounds(first.id), first.round), (3, 1));
    assert_eq!((rounds(third.id), third.round), (3, 3));
    assert_eq!(
        (rounds(alone.id), alone.round),
        (1, 1),
        "and a pull request reviewed once is round 1 of 1"
    );
}

#[test]
fn the_reviews_list_can_be_asked_about_one_repository() {
    let (_dir, store) = temp_store();
    let portal = seed(&store, "acme/portal", 398, "aaaaaaa");
    seed(&store, "acme/billing-api", 88, "bbbbbbb");

    assert_eq!(
        store
            .reviews_at_step(Some("ACME/Portal"), DismissedReviews::default())
            .unwrap()
            .iter()
            .map(|at| at.review.id)
            .collect::<Vec<_>>(),
        [portal.id],
        "by any spelling of its name, as every other filter here accepts"
    );
    assert!(
        store
            .reviews_at_step(Some("acme/nothing"), DismissedReviews::default())
            .unwrap()
            .is_empty(),
        "and a repository with no reviews is empty rather than everything"
    );
}
