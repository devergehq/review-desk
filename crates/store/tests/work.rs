//! What a review needs an agent to do.
//!
//! **This is an interface, not an internal query.** The Locus dispatcher is written
//! against these four kinds, so the tests here are about the whole answer and not only
//! about the happy path: that a review has at most one kind, that one waiting on the
//! developer has none, that a review a session abandoned is still listed, and that
//! reading the list cannot change a record.

mod common;

use std::cell::RefCell;
use std::collections::BTreeSet;

use common::{db_path, new_review, temp_store};
use review_desk_store::{
    Actor, NewBrief, NewDraft, Review, ReviewEvent, ReviewLayerState, ReviewLinks, Store,
    WorkFilter, WorkItem, WorkKind,
};
use tempfile::TempDir;

/// Everything waiting, across every repository.
fn work(store: &Store) -> Vec<WorkItem> {
    store.work_list(&WorkFilter::default()).expect("the list")
}

/// The one item for a review, or `None` when nothing is waiting on an agent.
fn item_for(store: &Store, review_id: i64) -> Option<WorkItem> {
    let items = store
        .work_list(&WorkFilter {
            review_id: Some(review_id),
            ..Default::default()
        })
        .expect("the list");
    assert!(
        items.len() <= 1,
        "a review has at most one kind, got {items:?}"
    );
    items.into_iter().next()
}

/// The kind waiting on a review, or `None`.
fn kind_for(store: &Store, review_id: i64) -> Option<WorkKind> {
    item_for(store, review_id).map(|item| item.kind)
}

/// A review, opened the way `POST /reviews` opens one: nothing written yet.
fn requested(store: &Store, pr: i64, head: &str) -> Review {
    store
        .create_review(new_review("acme/portal", pr, head, 1))
        .expect("a review")
}

/// A brief requiring confirmation.
fn brief(problem: &str) -> NewBrief {
    NewBrief {
        problem: problem.to_string(),
        ..Default::default()
    }
}

/// A brief the caller recorded as needing none.
fn waived_brief(problem: &str, because: &str) -> NewBrief {
    NewBrief {
        problem: problem.to_string(),
        confirmation_required: false,
        confirmation_not_required_because: Some(because.to_string()),
        ..Default::default()
    }
}

#[test]
fn a_review_nothing_has_been_written_for_needs_an_agent_to_start() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 412, "3f9c2ab");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.kind, WorkKind::ReviewRequested);
    assert_eq!(item.review.id, review.id);
    assert_eq!(item.review.repo, "acme/portal");
    assert_eq!(item.review.pr, 412);
    assert_eq!(item.review.head_sha, "3f9c2ab");
    assert_eq!(item.review.round, 1);
    assert_eq!(
        item.entered_at, review.created_at,
        "it has needed an agent since it was opened"
    );
    assert_eq!(item.note, None);
    assert_eq!(
        item.sign_off, None,
        "layer 2 is not settled; there is no brief"
    );

    // **And once a brief is written it is no longer unstarted.** DEV-864 asks for this by
    // name, and it is what stops a dispatcher starting a second review of the same thing.
    store
        .put_brief(review.id, brief("the problem"))
        .expect("a brief");
    assert_ne!(kind_for(&store, review.id), Some(WorkKind::ReviewRequested));
}

#[test]
fn the_linked_session_and_ledger_are_reported_and_never_judged() {
    // Review Desk does not know whether a session is alive — `docs/brief.md` gives that
    // to Allele — so the item carries the ids and says nothing about them.
    let (_dir, store) = temp_store();
    let review = requested(&store, 413, "aaa1111");
    store
        .set_review_links(
            review.id,
            ReviewLinks {
                session_id: Some("allele-ae03f8ce".to_string()),
                ledger_id: Some("ledger-413".to_string()),
                ..Default::default()
            },
        )
        .expect("the links");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.review.session_id.as_deref(), Some("allele-ae03f8ce"));
    assert_eq!(item.review.ledger_id.as_deref(), Some("ledger-413"));
}

#[test]
fn a_confirmed_brief_tells_an_agent_to_carry_on_and_says_who_settled_it() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 414, "bbb2222");
    store
        .put_brief(review.id, brief("the problem"))
        .expect("a brief");

    // Written and not confirmed: the developer is being asked, and nothing an agent does
    // should pre-empt that.
    assert_eq!(
        kind_for(&store, review.id),
        None,
        "a review waiting on the developer is not an agent's work"
    );

    let confirmed = store
        .confirm_brief(review.id, Actor::Developer)
        .expect("the developer confirms it");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.kind, WorkKind::BriefSettled);
    assert_eq!(item.sign_off, Some(review_desk_store::SignOff::Developer));
    assert_eq!(item.entered_at, confirmed.confirmed_at.expect("a time"));
}

#[test]
fn a_waived_brief_is_marked_waived_rather_than_passing_as_confirmed() {
    // **The reason `sign_off` exists.** A brief recorded as needing no confirmation
    // produces the same kind as one the developer confirmed, and without this field a
    // dispatcher's ledger — and every log after it — would say the developer confirmed a
    // review nobody looked at.
    let (_dir, store) = temp_store();
    let review = requested(&store, 415, "ccc3333");
    let written = store
        .put_brief(
            review.id,
            waived_brief(
                "a small change",
                "38 changed lines, under the caller's threshold",
            ),
        )
        .expect("a waived brief");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.kind, WorkKind::BriefSettled);
    assert_eq!(
        item.sign_off,
        Some(review_desk_store::SignOff::Waived),
        "nobody confirmed this, and the item has to be able to say so"
    );
    assert_eq!(
        item.entered_at, written.updated_at,
        "a waived brief settled layer 2 the moment it was written"
    );
}

#[test]
fn a_sent_back_draft_carries_the_developers_note() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 416, "ddd4444");
    store
        .put_brief(review.id, brief("the problem"))
        .expect("a brief");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("confirmed");
    store
        .put_draft(review.id, NewDraft::ready("the body"))
        .expect("a draft");

    // Ready: the developer's turn.
    assert_eq!(kind_for(&store, review.id), None);

    let note = "F2 is the same point as F1; cut one of them";
    let sent_back = store.send_back_draft(review.id, note).expect("sent back");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.kind, WorkKind::DraftSentBack);
    assert_eq!(item.note.as_deref(), Some(note));
    assert_eq!(item.entered_at, sent_back.sent_back_at.expect("a time"));

    // **Revised, and sent back again, is a new item.** `(kind, review_id, entered_at)`
    // is the identity a caller keys on, so the time has to move or the developer's second
    // instruction is silently treated as already handled.
    store
        .put_draft(review.id, NewDraft::ready("a revised body"))
        .expect("revised");
    assert_eq!(kind_for(&store, review.id), None, "ready again");

    // Backdated so the second send-back is distinguishable from the first in a test that
    // runs inside one second.
    set_time(
        &store,
        "UPDATE drafts SET sent_back_at = 1 WHERE review_id = ?1",
        review.id,
    );
    let again = store
        .send_back_draft(review.id, "and the evidence on F3 is a guess")
        .expect("sent back again");
    assert_ne!(
        again.sent_back_at,
        Some(1),
        "the second send-back rewrote the time, so the item is a new one"
    );
}

#[test]
fn an_approved_draft_waits_to_be_posted_and_then_waits_for_nothing() {
    let (_dir, store) = temp_store();
    let review = requested(&store, 417, "eee5555");
    store
        .put_brief(review.id, brief("the problem"))
        .expect("a brief");
    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("confirmed");
    store
        .put_draft(review.id, NewDraft::ready("the body"))
        .expect("a draft");
    let approved = store
        .approve_draft(review.id, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("approved");

    let item = item_for(&store, review.id).expect("an item");
    assert_eq!(item.kind, WorkKind::DraftApproved);
    assert_eq!(item.entered_at, approved.approved_at.expect("a time"));
    assert_eq!(item.note, None, "the note is the send-back's, and only its");

    store
        .post_draft(review.id, "gh-review-3312")
        .expect("the session posts it");
    assert_eq!(
        kind_for(&store, review.id),
        None,
        "posted: layer 7 is the harvester's, and nothing is waiting on an agent"
    );
}

#[test]
fn a_review_a_session_abandoned_is_still_listed() {
    // **The defect an adversarial pass found before this shipped.** An earlier draft
    // asked what had *started* — `review_requested` wanted no layer past `pending` — so
    // a session dispatched for a review, which wrote layer 1 and then died, matched no
    // kind afterwards and was invisible for ever. Nothing here asks what has begun.
    let (_dir, store) = temp_store();

    let dead_at_layer_one = requested(&store, 420, "fff6666");
    store
        .set_review_layer(
            dead_at_layer_one.id,
            1,
            ReviewLayerState::Running,
            Some("linting"),
        )
        .expect("layer 1");
    assert_eq!(
        kind_for(&store, dead_at_layer_one.id),
        Some(WorkKind::ReviewRequested),
        "a layer left running is not a brief; an agent still has to start"
    );

    let failed = requested(&store, 421, "ggg7777");
    store
        .set_review_layer(
            failed.id,
            1,
            ReviewLayerState::Failed,
            Some("the lint crashed"),
        )
        .expect("layer 1");
    assert_eq!(
        kind_for(&store, failed.id),
        Some(WorkKind::ReviewRequested),
        "a failure that nothing reports is a failure nothing can retry"
    );

    let unfinished_draft = requested(&store, 422, "hhh8888");
    store
        .put_brief(unfinished_draft.id, brief("the problem"))
        .expect("a brief");
    store
        .confirm_brief(unfinished_draft.id, Actor::Developer)
        .expect("confirmed");
    store
        .put_draft(
            unfinished_draft.id,
            NewDraft {
                body: "half a draft".to_string(),
                status: review_desk_store::DraftStatus::Drafting,
                lint: None,
                sections: None,
            },
        )
        .expect("a drafting draft");
    assert_eq!(
        kind_for(&store, unfinished_draft.id),
        Some(WorkKind::BriefSettled),
        "a draft nobody finished has not been handed to the developer"
    );

    let skipped = requested(&store, 423, "iii9999");
    store
        .put_brief(skipped.id, brief("the problem"))
        .expect("a brief");
    store
        .confirm_brief(skipped.id, Actor::Developer)
        .expect("confirmed");
    store
        .set_review_layer(
            skipped.id,
            3,
            ReviewLayerState::Skipped,
            Some("no diagram needed"),
        )
        .expect("layer 3");
    assert_eq!(
        kind_for(&store, skipped.id),
        Some(WorkKind::BriefSettled),
        "a layer deliberately skipped must not suppress the review for ever"
    );
}

#[test]
fn a_round_superseded_by_a_new_head_commit_is_listed_by_nothing() {
    // A round nothing will move again would otherwise ask for an agent for ever, and
    // oldest-first would pin that dead round at the head of the queue in front of live
    // work.
    let (_dir, store) = temp_store();

    let first = store
        .open_review("acme/portal", 500, "1111aaa")
        .expect("round 1");
    assert_eq!(
        kind_for(&store, first.review.id),
        Some(WorkKind::ReviewRequested)
    );

    // **A finding, so that round 1 has been started.** Since DEV-945 a new commit on a
    // round nobody has started moves that round rather than superseding it, so without
    // this there would be no round 2 to supersede anything. A finding and no brief leaves
    // the kind exactly where it was, which is what this test needs round 1 to be.
    store
        .add_finding(
            first.review.id,
            review_desk_store::NewFinding {
                lens: "correctness".to_string(),
                severity: review_desk_store::Severity::Nit,
                claim: "round 1 was reviewed".to_string(),
                disposition: review_desk_store::Disposition::InDraft,
                ..Default::default()
            },
        )
        .expect("a finding");
    assert_eq!(
        kind_for(&store, first.review.id),
        Some(WorkKind::ReviewRequested),
        "a finding alone does not change what an agent is to do"
    );

    let second = store
        .open_review("acme/portal", 500, "2222bbb")
        .expect("round 2");
    assert_eq!(second.review.round, 2);

    assert_eq!(
        kind_for(&store, first.review.id),
        None,
        "round 1 is superseded and nothing will move it again"
    );
    assert_eq!(
        kind_for(&store, second.review.id),
        Some(WorkKind::ReviewRequested),
        "and round 2 is the live one"
    );
}

#[test]
fn the_list_is_oldest_first() {
    let (_dir, store) = temp_store();

    let first = requested(&store, 601, "aaa0001");
    let second = requested(&store, 602, "aaa0002");
    let third = requested(&store, 603, "aaa0003");

    // Backdated, because three reviews opened in one test are opened in one second.
    set_time(
        &store,
        "UPDATE reviews SET created_at = 300 WHERE id = ?1",
        first.id,
    );
    set_time(
        &store,
        "UPDATE reviews SET created_at = 100 WHERE id = ?1",
        second.id,
    );
    set_time(
        &store,
        "UPDATE reviews SET created_at = 200 WHERE id = ?1",
        third.id,
    );

    let order: Vec<i64> = work(&store).iter().map(|item| item.review.id).collect();
    assert_eq!(
        order,
        vec![second.id, third.id, first.id],
        "a dispatcher serving one item at a time serves what has waited longest"
    );
}

#[test]
fn one_repository_or_one_review_at_a_time() {
    let (_dir, store) = temp_store();
    let portal = requested(&store, 700, "aaa0010");
    let other = store
        .create_review(new_review("acme/billing", 12, "bbb0020", 1))
        .expect("another repository");

    let in_portal = store
        .work_list(&WorkFilter {
            repo: Some("acme/portal".to_string()),
            ..Default::default()
        })
        .expect("one repository");
    assert_eq!(
        in_portal
            .iter()
            .map(|item| item.review.id)
            .collect::<Vec<_>>(),
        vec![portal.id]
    );

    // Normalised the way the insert is, so a caller who typed the repository with capitals
    // is answered about the repository they named rather than told nothing is waiting.
    let shouted = store
        .work_list(&WorkFilter {
            repo: Some("  ACME/Portal  ".to_string()),
            ..Default::default()
        })
        .expect("one repository, shouted");
    assert_eq!(shouted.len(), 1, "{shouted:?}");

    // A repository the record has never heard of is an empty list, not a failure: there
    // is no "no such repository" to report, because a repository is not a record.
    let unknown = store
        .work_list(&WorkFilter {
            repo: Some("acme/nothing".to_string()),
            ..Default::default()
        })
        .expect("an unknown repository");
    assert!(unknown.is_empty());

    let one = store
        .work_list(&WorkFilter {
            review_id: Some(other.id),
            ..Default::default()
        })
        .expect("one review");
    assert_eq!(
        one.iter().map(|item| item.review.id).collect::<Vec<_>>(),
        vec![other.id]
    );

    assert_eq!(work(&store).len(), 2, "and unfiltered it is both");
}

#[test]
fn every_reachable_state_has_one_kind_or_none() {
    // **The property the five kinds rest on.** Five separate queries would be five
    // chances for two of them to match one review and hand an agent two contradictory
    // instructions. This walks a review through every state it can reach and asserts that
    // never happens, and that the states which should be silent are.
    let (_dir, store) = temp_store();
    let review = requested(&store, 800, "aaa0030");

    // What the walk actually reached. **It used to count `WorkKind::ALL` instead**, so the
    // closing assertion held however few states the walk visited — a kind added and never
    // walked passed it. Found while adding the fifth.
    let visited = RefCell::new(BTreeSet::new());
    let seen = |expected: Option<WorkKind>, what: &str| {
        let got = kind_for(&store, review.id);
        assert_eq!(got, expected, "{what}");
        if let Some(kind) = got {
            visited.borrow_mut().insert(kind.as_str());
        }
    };

    seen(
        Some(WorkKind::ReviewRequested),
        "opened and nothing written",
    );

    store
        .put_brief(review.id, brief("the problem"))
        .expect("a brief");
    seen(
        None,
        "a brief waiting to be confirmed is the developer's turn",
    );

    store
        .confirm_brief(review.id, Actor::Developer)
        .expect("confirmed");
    seen(Some(WorkKind::BriefSettled), "confirmed, and no draft yet");

    store
        .put_draft(review.id, NewDraft::ready("the body"))
        .expect("a draft");
    seen(None, "a ready draft is the developer's turn");

    store
        .send_back_draft(review.id, "cut F2")
        .expect("sent back");
    seen(Some(WorkKind::DraftSentBack), "sent back");

    store
        .put_draft(review.id, NewDraft::ready("a revised body"))
        .expect("revised");
    seen(None, "revised and ready: the developer's turn again");

    store
        .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
        .expect("approved");
    seen(Some(WorkKind::DraftApproved), "approved and not posted");

    store.post_draft(review.id, "gh-review-9").expect("posted");
    seen(None, "posted: nothing is waiting on an agent");

    store
        .request_nit_ticket(review.id, None)
        .expect("the developer asks for the one ticket");
    seen(
        Some(WorkKind::NitsToTicket),
        "a ticket the developer asked for and nobody has filed",
    );

    store
        .request_nit_ticket(review.id, Some("ABC-246"))
        .expect("the session files it");
    seen(None, "filed: nothing is waiting on an agent again");

    // Every kind was reached, so this test covers the set rather than part of it.
    let every: BTreeSet<&str> = WorkKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(
        visited.into_inner(),
        every,
        "every kind, and this walk visited each"
    );
}

#[test]
fn reading_the_list_changes_no_row_of_the_database() {
    // DEV-864 asks for this to be shown rather than asserted. It is compared on the rows
    // themselves and not on the file: opening a store migrates it, which is a write, and
    // a byte comparison would fail on that alone and prove nothing about the query.
    let dir = TempDir::new().expect("a directory");
    let path = db_path(&dir);

    {
        let store = Store::open(&path).expect("open and migrate");
        let review = store
            .create_review(new_review("acme/portal", 900, "aaa0040", 1))
            .expect("a review");
        store
            .put_brief(review.id, brief("the problem"))
            .expect("a brief");
        store
            .confirm_brief(review.id, Actor::Developer)
            .expect("confirmed");
        store
            .put_draft(review.id, NewDraft::ready("the body"))
            .expect("a draft");
        store
            .approve_draft(review.id, ReviewEvent::Comment, Actor::Developer)
            .expect("approved");
    }

    let store = Store::open(&path).expect("reopen, already migrated");
    let before = dump(&path);

    let items = work(&store);
    assert_eq!(items.len(), 1, "there is something to read: {items:?}");
    // Read a second time, and through both filters, so every code path the command can
    // take has run between the two dumps.
    work(&store);
    store
        .work_list(&WorkFilter {
            review_id: Some(items[0].review.id),
            repo: Some("acme/portal".to_string()),
        })
        .expect("filtered");

    assert_eq!(before, dump(&path), "work_list wrote something");
}

#[test]
fn the_work_query_contains_no_write_statement() {
    // "It is a read" is a claim about code, and code stops being like that one
    // convenience at a time. The command the dispatcher polls every minute is the last
    // place a write belongs, so the claim is checked rather than documented.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/entities/work.rs"),
    )
    .expect("the work module's source");

    let code: String = source
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !(line.starts_with("//") || line.starts_with("/*") || line.starts_with('*'))
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Each keyword with the space that follows it in real SQL, so `created_at` is not
    // read as a `CREATE`. That false positive is worth naming: a check that fires on the
    // name of a column gets switched off, and then it is checking nothing.
    let upper = code.to_uppercase();
    for statement in [
        "INSERT ", "UPDATE ", "DELETE ", "REPLACE ", "DROP ", "ALTER ", "CREATE ", "PRAGMA ",
    ] {
        assert!(
            !upper.contains(statement),
            "crates/store/src/entities/work.rs contains {statement:?}; the work list is \
             read-only and the dispatcher polls it"
        );
    }

    // And the other half, which catches a write that does not spell its verb here: in
    // rusqlite a read is `prepare` and `query_map`, and `execute` is how a write happens.
    assert!(
        !code.contains("execute"),
        "crates/store/src/entities/work.rs calls execute; a read uses prepare and query"
    );
    assert!(code.contains("SELECT"), "and it does read something");
}

/// Run one test-only statement against the database, for a time a test needs to control.
///
/// Through a second connection on purpose: the store has no function for backdating a
/// record and should not grow one. A test that needs three reviews to have been opened at
/// three different times, inside one second, is the only caller.
fn set_time(store: &Store, sql: &str, id: i64) {
    let conn = rusqlite::Connection::open(store.path()).expect("a second connection");
    conn.execute(sql, rusqlite::params![id]).expect("backdate");
}

/// Every row of every table, as text, for comparing a database with itself.
fn dump(path: &std::path::Path) -> String {
    let conn = rusqlite::Connection::open(path).expect("a read connection");
    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("the table list")
        .query_map([], |row| row.get(0))
        .expect("the table names")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("the table names");
    tables.retain(|name| !name.starts_with("sqlite_"));
    assert!(
        tables.len() > 5,
        "a dump of nothing proves nothing: {tables:?}"
    );

    let mut out = String::new();
    for table in tables {
        let mut statement = conn
            .prepare(&format!("SELECT * FROM \"{table}\""))
            .expect("select the table");
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut line = String::new();
                for index in 0..columns {
                    line.push_str(&format!("{:?}|", row.get_ref(index)?));
                }
                Ok(line)
            })
            .expect("the rows")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("the rows");
        out.push_str(&format!("{table}: {}\n", rows.join(" / ")));
    }
    out
}
