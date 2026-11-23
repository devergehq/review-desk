//! Which of the developer's questions may be handed to the review's session through Allele,
//! and what a handover records. Migration 28, DEV-1383.

mod common;

use common::db_path;
use review_desk_store::{
    Clock, ReviewLayerName, ReviewLinks, Store, HANDOVER_AGAIN_AFTER_SECS, HANDOVER_AT_MOST,
};
use tempfile::TempDir;

const START: i64 = 1_763_900_000;

fn store_at(clock: &Clock) -> (TempDir, Store) {
    let dir = TempDir::new().expect("temporary directory");
    let store = Store::open_with_clock(db_path(&dir), clock.clone()).expect("open store");
    (dir, store)
}

/// A review on `pr`, linked to `session` when one is given.
fn review(store: &Store, pr: i64, session: Option<&str>) -> i64 {
    let id = store
        .open_review("acme/portal", pr, "c0ffee1")
        .expect("opening a review")
        .review
        .id;
    if let Some(session) = session {
        store
            .set_review_links(
                id,
                ReviewLinks {
                    session_id: Some(session.to_string()),
                    ..Default::default()
                },
            )
            .expect("linking the session");
    }
    id
}

fn handed_over(store: &Store) -> Vec<(i64, i64, String)> {
    store
        .messages_to_hand_over()
        .expect("reading what to hand over")
        .into_iter()
        .map(|one| (one.message.review_id, one.message.seq, one.session_id))
        .collect()
}

#[test]
fn an_unread_ask_on_a_review_with_a_session_is_handed_over() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    let asked = store
        .ask_session(id, ReviewLayerName::Draft, "Why is F13 a Should?")
        .expect("asking");

    assert_eq!(asked.handed_over_at, None);
    assert_eq!(asked.handed_over_count, 0);
    assert_eq!(handed_over(&store), vec![(id, 1, "9f2c".to_string())]);
    assert!(store.message_to_hand_over(id, 1).unwrap().is_some());
}

#[test]
fn a_review_with_no_session_has_nothing_to_hand_over_to() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, None);
    store
        .ask_session(id, ReviewLayerName::Draft, "Anyone there?")
        .expect("asking");
    assert!(handed_over(&store).is_empty());
    assert_eq!(store.message_to_hand_over(id, 1).unwrap(), None);
}

#[test]
fn a_message_marked_read_is_never_handed_over_again() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");
    store.record_handover(id, 1).expect("handed over");
    store
        .mark_message_read(id, 1, Some("9f2c"))
        .expect("marking read");

    clock.advance(HANDOVER_AGAIN_AFTER_SECS * 10);
    assert!(handed_over(&store).is_empty());
}

#[test]
fn a_send_back_is_never_handed_over_and_cannot_be_recorded_as_one() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    let note = store
        .record_send_back_message(id, "Regrade F13.")
        .expect("keeping the note");
    assert!(handed_over(&store).is_empty());
    assert!(store.record_handover(id, note.seq).is_err());
    assert_eq!(store.message(id, note.seq).unwrap().handed_over_count, 0);
}

#[test]
fn a_reply_is_never_handed_over() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .reply_from_session(id, ReviewLayerName::Draft, "Done.", None)
        .expect("replying");
    assert!(handed_over(&store).is_empty());
}

#[test]
fn a_handover_records_when_and_how_many_times() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");

    let once = store.record_handover(id, 1).expect("handed over");
    assert_eq!(once.handed_over_at, Some(START));
    assert_eq!(once.handed_over_count, 1);
    assert_eq!(once.read_at, None, "a handover is not a reading");

    clock.advance(HANDOVER_AGAIN_AFTER_SECS + 1);
    let twice = store.record_handover(id, 1).expect("handed over again");
    assert_eq!(
        twice.handed_over_at,
        Some(START + HANDOVER_AGAIN_AFTER_SECS + 1)
    );
    assert_eq!(twice.handed_over_count, 2);
}

#[test]
fn an_unread_message_is_handed_over_again_no_sooner_than_two_minutes_later() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");
    store.record_handover(id, 1).expect("handed over");

    assert!(handed_over(&store).is_empty(), "just handed over");
    clock.advance(HANDOVER_AGAIN_AFTER_SECS);
    assert!(
        handed_over(&store).is_empty(),
        "exactly two minutes is not more than two minutes"
    );
    clock.advance(1);
    assert_eq!(handed_over(&store).len(), 1);
}

#[test]
fn a_message_is_handed_over_at_most_three_times() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");

    for _ in 0..HANDOVER_AT_MOST {
        assert_eq!(handed_over(&store).len(), 1);
        store.record_handover(id, 1).expect("handed over");
        clock.advance(HANDOVER_AGAIN_AFTER_SECS + 1);
    }
    assert!(handed_over(&store).is_empty());
    assert_eq!(
        store.message(id, 1).unwrap().handed_over_count,
        HANDOVER_AT_MOST
    );
}

#[test]
fn oldest_first_across_reviews() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let first = review(&store, 514, Some("s-one"));
    let second = review(&store, 515, Some("s-two"));
    store
        .ask_session(second, ReviewLayerName::Draft, "Earlier")
        .expect("asking");
    clock.advance(5);
    store
        .ask_session(first, ReviewLayerName::Draft, "Later")
        .expect("asking");

    assert_eq!(
        handed_over(&store),
        vec![
            (second, 1, "s-two".to_string()),
            (first, 1, "s-one".to_string())
        ]
    );
}

/// Migration 28 adds two columns, and what a machine upgrading has to get is every message it
/// already held reading as never handed over — and the count's floor enforced.
#[test]
fn a_message_from_before_the_handover_reads_as_never_handed_over() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    const FROM: i64 = 27;
    {
        let old = Store::open_at_version(&path, FROM).expect("open at 27");
        assert_eq!(old.schema_version().unwrap(), FROM);
        let id = old
            .open_review("acme/portal", 514, "c0ffee1")
            .expect("a review")
            .review
            .id;
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO review_messages (review_id, seq, step, author, kind, body, created_at)
                 VALUES (?1, 1, 'draft', 'developer', 'ask', 'Why?', 1000)",
            [id],
        )
        .expect("write the row a released build left");
    }

    let migrated = Store::open(&path).expect("reopen migrates");
    let message = migrated.messages(1).expect("messages").remove(0);
    assert_eq!(message.handed_over_at, None);
    assert_eq!(message.handed_over_count, 0);

    let conn = rusqlite::Connection::open(&path).unwrap();
    assert!(
        conn.execute_batch("UPDATE review_messages SET handed_over_count = -1")
            .is_err(),
        "a count below nought is refused by the record itself"
    );
}

#[test]
fn a_question_on_a_dismissed_review_is_in_front_of_nobody() {
    let clock = Clock::fixed(START);
    let (_dir, store) = store_at(&clock);
    let id = review(&store, 514, Some("9f2c"));
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");
    store
        .dismiss_review(id, "opened by mistake")
        .expect("dismissing");
    assert!(handed_over(&store).is_empty());
    assert_eq!(store.message_to_hand_over(id, 1).unwrap(), None);
}
