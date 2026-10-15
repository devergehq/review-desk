//! The queries that are not about one pull request: what is waiting on the developer
//! anywhere, and which repositories there are.

mod common;

use common::{db_path, new_review, temp_store};
use review_desk_store::{
    Actor, Disposition, DraftStatus, FindingRef, NewDraft, NewFinding, NewOutcome, NewRule,
    OutOfPlay, OutcomeKind, PrState, Review, ReviewEvent, ReviewLayerName, ReviewLayerState,
    ReviewsShown, RuleScope, RuleStage, Severity, Store,
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

/// The pull requests `newest_rounds_in_play` reports, as `(pr, round)` pairs.
///
/// By pull request and round rather than by review id, because what this query answers is
/// which round of which pull request is the live one — an assertion on ids would pass over
/// a query that returned the right rows for the wrong reason.
fn on_the_desk(store: &Store, repo: Option<&str>) -> Vec<(i64, i64)> {
    let mut found: Vec<(i64, i64)> = store
        .newest_rounds_in_play(repo)
        .expect("the pull requests on the desk")
        .iter()
        .map(|review| (review.pr, review.round))
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn the_desk_holds_one_round_per_pull_request_and_it_is_the_newest() {
    // **The candidates for "Requested, not started", and the whole of what the store
    // answers about that group.** DEV-994: the group is these minus the pull requests the
    // five groups above were served for, and the subtraction is the server's because it
    // has to be made against what was served.
    let (_dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 412, "3f9c2ab");
    // Round 1 has to post before a new commit opens round 2 rather than moving this head —
    // DEV-962, which `common::posted` carries.
    common::posted(&store, one.id);
    let two = store
        .open_review("acme/portal", 412, "d4e5f64")
        .expect("round 2")
        .review;
    assert_eq!(two.round, 2, "a posted round is superseded, not moved");
    seed(&store, "acme/billing-api", 88, "bbbbbbb");

    assert_eq!(
        on_the_desk(&store, None),
        vec![(88, 1), (412, 2)],
        "one round per pull request, and the newest of them"
    );
    assert_eq!(
        on_the_desk(&store, Some("acme/billing-api")),
        vec![(88, 1)],
        "and one repository's, for a caller that asked about one"
    );
    assert_eq!(
        on_the_desk(&store, Some("ACME/Billing-API")),
        vec![(88, 1)],
        "normalised, so a caller who typed the repository differently is answered"
    );
}

#[test]
fn a_round_the_developer_dismissed_takes_its_pull_request_off_the_desk() {
    // **The one place the invariant stops, and it stops on purpose.** DEV-994's rule is
    // "in play *and not dismissed* means exactly one group": a dismissal is the developer
    // saying they will not act on this round, so a group that put the pull request back in
    // front of them under another heading would undo their own act.
    //
    // **And no earlier round stands in for it**, which is `NEWEST_ROUND` counting every
    // round whatever became of it. Round 1 is superseded and nothing can move its state
    // again; promoting it here would put a dead round in a group about work waiting to
    // start.
    let (_dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 412, "3f9c2ab");
    common::posted(&store, one.id);
    let two = store
        .open_review("acme/portal", 412, "d4e5f64")
        .expect("round 2")
        .review;
    assert_eq!(on_the_desk(&store, None), vec![(412, 2)]);

    store
        .dismiss_review(two.id, "not worth a review")
        .expect("the developer dismissed the round");

    assert_eq!(
        on_the_desk(&store, None),
        Vec::<(i64, i64)>::new(),
        "the pull request is off the desk, and round 1 is not promoted into its place"
    );

    store.restore_review(two.id).expect("restored");
    assert_eq!(
        on_the_desk(&store, None),
        vec![(412, 2)],
        "and it comes back, because a restore is the dismissal taken back"
    );
}

#[test]
fn a_finished_or_out_of_play_pull_request_is_not_on_the_desk() {
    // The other two ways off, both through `IN_PLAY` and both asserted here rather than
    // trusted: a query that pasted the clause and a query that spelled half of it compile
    // the same.
    //
    // **Out of play is the one that undoes itself** — DEV-962, a pull request converted to
    // draft or whose review request was withdrawn — so it is asserted in both directions.
    let (_dir, store) = temp_store();
    let merged = seed(&store, "acme/portal", 401, "9d41c0e");
    seed(&store, "acme/portal", 415, "eeeeeee");
    seed(&store, "acme/portal", 412, "3f9c2ab");

    store
        .set_review_pr_state(merged.id, PrState::Merged, None)
        .expect("the pull request merged");
    store
        .set_pr_out_of_play("acme/portal", 415, Some(OutOfPlay::Draft))
        .expect("converted to a draft");

    assert_eq!(
        on_the_desk(&store, None),
        vec![(412, 1)],
        "a merged pull request is over and one out of play is paused; neither is waiting"
    );

    store
        .set_pr_out_of_play("acme/portal", 415, None)
        .expect("marked ready again");
    assert_eq!(
        on_the_desk(&store, None),
        vec![(412, 1), (415, 1)],
        "and the paused one returns, which is the only one of the three that does"
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
        .reviews_at_step(None, ReviewsShown::default())
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
        .reviews_at_step(None, ReviewsShown::default())
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
            .reviews_at_step(Some("ACME/Portal"), ReviewsShown::default())
            .unwrap()
            .iter()
            .map(|at| at.review.id)
            .collect::<Vec<_>>(),
        [portal.id],
        "by any spelling of its name, as every other filter here accepts"
    );
    assert!(
        store
            .reviews_at_step(Some("acme/nothing"), ReviewsShown::default())
            .unwrap()
            .is_empty(),
        "and a repository with no reviews is empty rather than everything"
    );
}

/* ------------- a code change a later posted round has already reviewed ------------- */

/*
 * DEV-983. On 14 October 2026 Patrick's queue drew a two-round pull request as "round 1 ·
 * The code under F1 changed · See what changed" and opened round 1, though round 2 had been
 * posted the day before — and the change it reported was round 2's own head. A round that
 * posted is a review that happened, so the code it read has been looked at.
 *
 * The fixtures here are the three the issue asks for, built the way the harvester builds
 * them: the detail is the JSON `harvest_code_changes` writes, carrying the commit the change
 * is at and the `compare A...B` the comparison it was read from.
 */

/// The detail the harvester writes for a code change, at this commit and this comparison.
fn code_change_detail(file: &str, commit: &str, from: &str, to: &str) -> String {
    serde_json::json!({
        "file": file,
        "line": 1122,
        "lines_changed": false,
        "file_status": "modified",
        "commit": commit,
        "basis": format!("compare {from}...{to}"),
    })
    .to_string()
}

/// A finding with a file, which is what a code change is recorded against.
fn finding_on(store: &Store, review: i64, file: &str) -> i64 {
    store
        .add_finding(
            review,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "the export reads the tenant from the request".to_string(),
                file: Some(file.to_string()),
                line: Some(1122),
                disposition: Disposition::InDraft,
                ..Default::default()
            },
        )
        .expect("add finding")
        .seq
}

/// Record a code change on a finding, with the detail the harvester would have written.
fn code_changed(store: &Store, review: i64, seq: i64, detail: String, at: i64) {
    store
        .add_outcome(
            FindingRef {
                review_id: review,
                seq,
            },
            NewOutcome {
                kind: OutcomeKind::CodeChangedAfter,
                actor: Actor::Harvester,
                detail: Some(detail),
                occurred_at: Some(at),
            },
        )
        .expect("record the code change");
}

/// Move a round's opened time, in the database's own SQL.
///
/// **A test about `replied` needs the reply to land either side of the round that answers
/// it**, and `NewReview` carries no `created_at`: `create_review` writes `now()`. The store
/// has no function for moving a review's creation time, and it should not grow one so that
/// a test can pass — so this does it the way `tests/rules.rs` ages a finding past its
/// window, in SQL. The round's own facts are untouched; only when it opened moves.
fn opened_at(dir: &tempfile::TempDir, review: i64, at: i64) {
    let conn = rusqlite::Connection::open(db_path(dir)).expect("open the database");
    let moved = conn
        .execute(
            "UPDATE reviews SET created_at = ?1 WHERE id = ?2",
            rusqlite::params![at, review],
        )
        .expect("move the round's opened time");
    assert_eq!(moved, 1, "one round, moved");
}

/// Record an author's reply on a finding, at this time.
fn replied(store: &Store, review: i64, seq: i64, at: i64) {
    store
        .add_outcome(
            FindingRef {
                review_id: review,
                seq,
            },
            NewOutcome {
                kind: OutcomeKind::Replied,
                actor: Actor::Author,
                detail: Some("the retry passes the tenant now".to_string()),
                occurred_at: Some(at),
            },
        )
        .expect("record the reply");
}

/// Round 1 posted at `A`, the code changed `A` to `B`, round 2 posted at `B`.
///
/// The issue's first fixture. Both rounds are posted, which is also what lets round 2 exist
/// at all: since DEV-962 a new commit on an *unposted* round moves that round's head.
fn two_posted_rounds(store: &Store) -> (Review, Review, i64) {
    let one = seed(store, "acme/portal", 631, "c1d2e31");
    let seq = finding_on(store, one.id, "app/Exports/ScheduledExport.php");
    post(store, one.id, ReviewEvent::RequestChanges, "PRR_r1");

    let two = store
        .create_review(new_review("acme/portal", 631, "d2e3f42", 2))
        .expect("round 2");
    post(store, two.id, ReviewEvent::RequestChanges, "PRR_r2");

    (one, two, seq)
}

#[test]
fn a_code_change_a_later_posted_round_read_is_not_author_activity() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "d2e3f42",
            "c1d2e31",
            "d2e3f42",
        ),
        1_000,
    );

    assert!(
        !ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "round 2 posted at the commit this change is at, so nobody has to look again"
    );
    assert!(
        store.author_activity(one.id).unwrap().is_empty(),
        "and the item would have no event to name"
    );
    assert_eq!(
        store.list_outcomes(one.id).unwrap().len(),
        1,
        "nothing is deleted: the What happened page still has it"
    );
}

#[test]
fn a_change_after_the_posted_rounds_head_is_still_author_activity() {
    let (_dir, store) = temp_store();
    let (one, two, seq) = two_posted_rounds(&store);
    let later = finding_on(&store, two.id, "app/Exports/ScheduledExport.php");

    // The same push, seen from both rounds: round 1 compares its own head to `e3f4a53`,
    // round 2 compares `d2e3f42` to it. Neither comparison ends at a posted round's head.
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "e3f4a53",
            "c1d2e31",
            "e3f4a53",
        ),
        2_000,
    );
    code_changed(
        &store,
        two.id,
        later,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "e3f4a53",
            "d2e3f42",
            "e3f4a53",
        ),
        2_000,
    );

    let acted = ids(&store.reviews_with_author_activity(None, None).unwrap());
    assert!(
        acted.contains(&two.id),
        "a change nothing has reviewed counts, on the newest round"
    );
    assert_eq!(
        store.author_activity(two.id).unwrap().len(),
        1,
        "and it is the newest round's own event"
    );
}

#[test]
fn a_change_whose_comparison_ends_at_a_posted_head_is_already_reviewed() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    // **The "at or before" case.** The author pushed twice before round 2 opened, so the
    // commit that touched the file is `9a8b7c6` — a commit no round's head records — while
    // the comparison it was found in ends at `d2e3f42`, which round 2 was posted on. Every
    // change in that comparison is inside round 2's reading.
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "9a8b7c6",
            "c1d2e31",
            "d2e3f42",
        ),
        1_500,
    );

    assert!(
        !ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "the comparison ends at a posted round's head, so the change is at or before it"
    );
}

#[test]
fn a_commit_the_record_does_not_recognise_keeps_its_item() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    // Neither commit is a round's head. Ancestry is not in this record, so nothing here can
    // say whether round 2 contains `9a8b7c6` — and an unknown commit errs toward showing.
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "9a8b7c6",
            "c1d2e31",
            "7f6e5d4",
        ),
        1_500,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "an unrecognised commit is shown, not hidden"
    );
    assert_eq!(store.author_activity(one.id).unwrap().len(), 1);
}

#[test]
fn a_detail_that_is_not_json_keeps_its_item_rather_than_failing_the_query() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    // `outcome add --detail "the author pushed a fix"` is a legal call. SQLite's
    // `json_extract` raises an error over malformed JSON rather than returning null, so an
    // unguarded clause would empty the whole queue over one row of prose.
    code_changed(
        &store,
        one.id,
        seq,
        "the author pushed a fix".to_string(),
        1_500,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "prose in detail says nothing about a commit, so the change is shown"
    );
}

/*
 * A reply a later posted round has already answered (DEV-990)
 * ----------------------------------------------------------
 *
 * The same rule, for what the record cannot ask a commit question about. On 15 October 2026
 * a pull request showed "The author replied to F18 · replied to round 2" with "Read the
 * reply", though round 3 had opened eight minutes after those replies and posted
 * twenty-nine minutes later: round 3 is the answer to the reply.
 *
 * Three cases, which are the three the issue asks for, and each turns on *when*: the round
 * that answers has to have opened after the reply, and it has to have posted.
 */

#[test]
fn a_reply_a_later_posted_round_opened_after_is_not_author_activity() {
    let (dir, store) = temp_store();
    let (one, two, seq) = two_posted_rounds(&store);

    // The shape the issue records: the author replies, and then the round that answers the
    // reply opens and posts. Round 2's session had the reply in front of it, because it was
    // there to read when the round opened.
    replied(&store, one.id, seq, 1_000);
    opened_at(&dir, two.id, 2_000);

    assert!(
        !ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "round 2 opened after the reply and posted, so the reply has been answered"
    );
    assert!(
        store.author_activity(one.id).unwrap().is_empty(),
        "and the item would have no event to name"
    );
    assert_eq!(
        store.list_outcomes(one.id).unwrap().len(),
        1,
        "nothing is deleted: round 1's What happened page still lists the reply"
    );
    assert!(
        ids_of(&store.posted_reviews_awaiting_author(None).unwrap()).contains(&two.id),
        "and the pull request is waiting on its author, which is the issue's own criterion"
    );
}

#[test]
fn a_reply_that_arrived_after_the_later_round_opened_is_still_author_activity() {
    let (dir, store) = temp_store();
    let (one, two, seq) = two_posted_rounds(&store);

    // The author answers a round-1 thread after round 2 had already opened. That round's
    // session may never have seen it, so it stays in the queue — the safe direction.
    opened_at(&dir, two.id, 2_000);
    replied(&store, one.id, seq, 3_000);
    // And a code change round 2 has already read, beside it: the two halves of the clause
    // answer about one review at once.
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail(
            "app/Exports/ScheduledExport.php",
            "d2e3f42",
            "c1d2e31",
            "d2e3f42",
        ),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a reply nobody's later round was open to read is the developer's to read"
    );
    let acted = store.author_activity(one.id).unwrap();
    assert_eq!(
        acted.iter().map(|outcome| outcome.kind).collect::<Vec<_>>(),
        vec![OutcomeKind::Replied],
        "the reply survives and the already-reviewed change does not"
    );
}

#[test]
fn an_unposted_later_round_does_not_hide_an_earlier_rounds_reply() {
    let (dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 671, "c1d2e31");
    let seq = finding_on(&store, one.id, "app/Jobs/RunExport.php");
    post(&store, one.id, ReviewEvent::RequestChanges, "PRR_r1");

    // Round 2 opened after the reply and has posted nothing. A round that has not posted is
    // a review that has not happened, so it has answered nothing.
    replied(&store, one.id, seq, 1_000);
    let two = store
        .create_review(new_review("acme/portal", 671, "d2e3f42", 2))
        .unwrap();
    opened_at(&dir, two.id, 2_000);

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a round that has not posted has answered nothing"
    );
    assert_eq!(
        store.author_activity(one.id).unwrap().len(),
        1,
        "and the reply is the event the item names"
    );
}

#[test]
fn a_thread_the_author_resolved_is_answered_by_a_later_posted_round_too() {
    let (dir, store) = temp_store();
    let (one, two, seq) = two_posted_rounds(&store);

    // **Not only `replied`.** The rule is about what the record cannot ask a commit
    // question of, and resolving a thread is the other thing the harvester writes for the
    // author. `InboxReplyItem` says so in the contract, so something has to hold it to that.
    store
        .add_outcome(
            FindingRef {
                review_id: one.id,
                seq,
            },
            NewOutcome {
                kind: OutcomeKind::ThreadResolved,
                actor: Actor::Harvester,
                detail: None,
                occurred_at: Some(1_000),
            },
        )
        .expect("record the resolution");
    opened_at(&dir, two.id, 2_000);

    assert!(
        !ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "round 2 opened after the thread was resolved and posted, so it read that too"
    );
    assert_eq!(
        store.list_outcomes(one.id).unwrap().len(),
        1,
        "and nothing is deleted"
    );
}

#[test]
fn a_reply_on_the_newest_round_is_never_already_answered() {
    let (dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 681, "c1d2e31");
    let seq = finding_on(&store, one.id, "app/Jobs/RunExport.php");
    post(&store, one.id, ReviewEvent::RequestChanges, "PRR_only");

    // The round the reply is on opened before the reply arrived, which is every reply there
    // is. Nothing may read a round's *own* opening as having answered what came after it;
    // the clause asks for a round numbered above this one, and there is none.
    opened_at(&dir, one.id, 2_000);
    replied(&store, one.id, seq, 3_000);

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a round's own opening is not a later round answering it"
    );
}

#[test]
fn a_newest_rounds_own_change_is_never_already_reviewed() {
    let (_dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 641, "c1d2e31");
    let seq = finding_on(&store, one.id, "app/Jobs/RunExport.php");
    post(&store, one.id, ReviewEvent::RequestChanges, "PRR_only");

    // A detail naming this round's own head. There is no later round, so there is nothing
    // that could have reviewed it — the clause asks for a round numbered above this one.
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail("app/Jobs/RunExport.php", "c1d2e31", "c1d2e31", "c1d2e31"),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a round's own head is not a later round's reading"
    );
}

#[test]
fn an_unposted_later_round_does_not_make_an_earlier_rounds_change_reviewed() {
    let (_dir, store) = temp_store();
    let one = seed(&store, "acme/portal", 651, "c1d2e31");
    let seq = finding_on(&store, one.id, "app/Jobs/RunExport.php");
    post(&store, one.id, ReviewEvent::RequestChanges, "PRR_r1");

    // Round 2 exists at `d2e3f42` and has posted nothing. Nobody has reviewed that commit,
    // so the change at it is still the developer's to see.
    store
        .create_review(new_review("acme/portal", 651, "d2e3f42", 2))
        .unwrap();
    code_changed(
        &store,
        one.id,
        seq,
        code_change_detail("app/Jobs/RunExport.php", "d2e3f42", "c1d2e31", "d2e3f42"),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a round that has not posted has reviewed nothing"
    );
}

#[test]
fn a_commit_written_as_a_number_is_not_taken_for_a_round_head() {
    let (_dir, store) = temp_store();
    // A pull request whose round 2 head is all digits, which about one abbreviated commit
    // in twenty-seven is.
    let one = store
        .create_review(new_review("acme/portal", 661, "1234560", 1))
        .unwrap();
    let seq = finding_on(&store, one.id, "app/Jobs/RunExport.php");
    post(&store, one.id, ReviewEvent::RequestChanges, "PRR_r1");
    let two = store
        .create_review(new_review("acme/portal", 661, "1234567", 2))
        .unwrap();
    post(&store, two.id, ReviewEvent::RequestChanges, "PRR_r2");

    // `outcome add --detail '{"commit": 1234567}'` is a legal call, and `head_sha` has
    // `TEXT` affinity — so SQLite converts the integer operand to text before comparing and
    // `'1234567' = 1234567` is true. A number nobody meant as a commit would then hide an
    // event, which is the one direction this rule must not be wrong in.
    code_changed(
        &store,
        one.id,
        seq,
        serde_json::json!({
            "file": "app/Jobs/RunExport.php",
            "line": 1122,
            "commit": 1_234_567,
            // The basis ends nowhere a round posted, so the only thing that could hide
            // this event is the number above — which is what the test is about.
            "basis": "compare 1234560...9f8e7d6",
        })
        .to_string(),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a commit has to be written as a string to name one"
    );
}

#[test]
fn text_that_merely_contains_three_dots_is_not_a_comparison() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    // `detail` is free text, and this is a legal document. Its tail is round 2's head, so
    // a clause that took any `...` for a comparison would hide a change on the strength of
    // a sentence claiming nothing about what any review read. `compare ` is the one prefix
    // the harvester writes.
    code_changed(
        &store,
        one.id,
        seq,
        serde_json::json!({
            "file": "app/Exports/ScheduledExport.php",
            "commit": "9a8b7c6",
            "basis": "annotation...d2e3f42",
        })
        .to_string(),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "a basis has to say it is a comparison to be read as one"
    );
}

#[test]
fn a_basis_spelled_in_capitals_is_not_a_comparison() {
    let (_dir, store) = temp_store();
    let (one, _two, seq) = two_posted_rounds(&store);

    // SQLite's `LIKE` is case-insensitive over ASCII, so a clause written with it accepted
    // `COMPARE …` — which the dashboard's sample backend rejects, because `startsWith` is
    // not. The two backends disagreeing in the direction that hides is the fault; `=` over
    // `substr` compares with the binary collation.
    code_changed(
        &store,
        one.id,
        seq,
        serde_json::json!({
            "file": "app/Exports/ScheduledExport.php",
            "commit": "9a8b7c6",
            "basis": "COMPARE c1d2e31...d2e3f42",
        })
        .to_string(),
        1_000,
    );

    assert!(
        ids(&store.reviews_with_author_activity(None, None).unwrap()).contains(&one.id),
        "the prefix the harvester writes is lower case, and so is the one this reads"
    );
}
