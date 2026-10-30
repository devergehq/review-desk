//! What the developer and a review's session say to each other. Migration 22, DEV-1141.

mod common;

use common::temp_store;
use review_desk_store::{MessageAuthor, MessageKind, ReviewLayerName, StoreError};

fn review(store: &review_desk_store::Store, pr: i64) -> i64 {
    store
        .open_review("acme/portal", pr, "c0ffee1")
        .expect("opening a review")
        .review
        .id
}

#[test]
fn a_question_waits_unread_until_something_delivers_it() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);

    let asked = store
        .ask_session(id, ReviewLayerName::Draft, "  Why is F13 a Should?  ")
        .expect("asking");
    assert_eq!(asked.seq, 1);
    assert_eq!(asked.author, MessageAuthor::Developer);
    assert_eq!(asked.kind, MessageKind::Ask);
    assert_eq!(asked.body, "Why is F13 a Should?");
    assert_eq!(store.unread_messages(Some(id)).expect("unread").len(), 1);

    let read = store
        .mark_message_read(id, 1, Some("allele-ae03f8ce"))
        .expect("marking read");
    assert!(read.read_at.is_some());
    assert_eq!(read.read_by_session.as_deref(), Some("allele-ae03f8ce"));
    assert!(store.unread_messages(Some(id)).expect("unread").is_empty());
}

#[test]
fn the_first_reading_stands() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);
    store
        .ask_session(id, ReviewLayerName::Draft, "Still there?")
        .expect("asking");
    let first = store.mark_message_read(id, 1, Some("s1")).expect("first");
    let again = store.mark_message_read(id, 1, Some("s2")).expect("again");
    assert_eq!(first, again);
}

#[test]
fn a_reply_names_the_question_it_answers_and_nothing_else() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);
    store
        .ask_session(id, ReviewLayerName::Draft, "Why?")
        .expect("asking");

    let reply = store
        .reply_from_session(id, ReviewLayerName::Draft, "Because.", Some(1))
        .expect("replying");
    assert_eq!(reply.seq, 2);
    assert_eq!(reply.author, MessageAuthor::Session);
    assert_eq!(reply.in_reply_to, Some(1));

    // Not to its own reply, and not to a message that is not there.
    for seq in [2, 99] {
        assert!(store
            .reply_from_session(id, ReviewLayerName::Draft, "And?", Some(seq))
            .is_err());
    }
}

#[test]
fn a_session_s_reply_cannot_be_marked_read() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);
    store
        .reply_from_session(id, ReviewLayerName::CodeReview, "Done.", None)
        .expect("replying");
    let refused = store.mark_message_read(id, 1, None).unwrap_err();
    assert!(matches!(
        refused,
        StoreError::InvalidValue { field: "seq", .. }
    ));
}

#[test]
fn an_empty_message_is_refused_and_writes_nothing() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);
    assert!(store
        .ask_session(id, ReviewLayerName::Draft, "   ")
        .is_err());
    assert!(store
        .reply_from_session(id, ReviewLayerName::Draft, "", None)
        .is_err());
    assert!(store.messages(id).expect("messages").is_empty());
}

#[test]
fn the_table_refuses_a_developer_reply_or_a_session_ask_whatever_writes_it() {
    let (dir, store) = temp_store();
    let id = review(&store, 514);
    drop(store);

    let conn = rusqlite::Connection::open(common::db_path(&dir)).expect("opening the file");
    for (author, kind) in [
        ("developer", "reply"),
        ("session", "ask"),
        ("session", "send_back"),
    ] {
        let wrote = conn.execute(
            "INSERT INTO review_messages (review_id, seq, step, author, kind, body, created_at)
             VALUES (?1, 1, 'draft', ?2, ?3, 'x', 1)",
            rusqlite::params![id, author, kind],
        );
        assert!(wrote.is_err(), "{author} {kind} was stored");
    }
}

#[test]
fn each_review_numbers_its_own_messages_and_unread_spans_them_all() {
    let (_dir, store) = temp_store();
    let one = review(&store, 514);
    let two = review(&store, 515);
    store
        .ask_session(one, ReviewLayerName::Draft, "a")
        .expect("a");
    store
        .ask_session(two, ReviewLayerName::Draft, "b")
        .expect("b");
    store
        .ask_session(one, ReviewLayerName::Draft, "c")
        .expect("c");

    assert_eq!(
        store
            .messages(one)
            .expect("messages")
            .iter()
            .map(|m| m.seq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(store.message(two, 1).expect("two's first").body, "b");
    assert_eq!(store.unread_messages(None).expect("all unread").len(), 3);
}

#[test]
fn a_send_back_note_is_kept_on_the_draft_step() {
    let (_dir, store) = temp_store();
    let id = review(&store, 514);
    let kept = store
        .record_send_back_message(id, "Regrade F13 to Blocker.")
        .expect("recording");
    assert_eq!(kept.kind, MessageKind::SendBack);
    assert_eq!(kept.step, ReviewLayerName::Draft);
}

#[test]
fn a_message_on_a_review_that_is_not_there_is_not_found() {
    let (_dir, store) = temp_store();
    assert!(matches!(
        store.ask_session(9001, ReviewLayerName::Draft, "hello"),
        Err(StoreError::NotFound { .. })
    ));
}
