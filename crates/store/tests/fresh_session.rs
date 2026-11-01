//! The developer's request for a fresh session. Migration 23, DEV-1142.

mod common;

use common::temp_store;
use review_desk_store::{ReviewLinks, StoreError, WorkFilter, WorkKind};

fn kinds(store: &review_desk_store::Store, id: i64) -> Vec<WorkKind> {
    store
        .work_list(&WorkFilter {
            review_id: Some(id),
            repo: None,
        })
        .expect("the work list")
        .into_iter()
        .map(|item| item.kind)
        .collect()
}

fn linked(store: &review_desk_store::Store, id: i64, session: &str) {
    store
        .set_review_links(
            id,
            ReviewLinks {
                session_id: Some(session.to_string()),
                ..Default::default()
            },
        )
        .expect("linking");
}

#[test]
fn a_declined_request_is_shown_and_is_no_longer_work() {
    let (_dir, store) = temp_store();
    let id = store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("open")
        .review
        .id;
    linked(&store, id, "allele-old");
    store.request_fresh_session(id).expect("asking");

    let declined = store
        .decline_fresh_session(id, "Allele says allele-old is running")
        .expect("declining");
    assert!(declined.declined_at.is_some());
    assert_eq!(
        declined.declined_reason.as_deref(),
        Some("Allele says allele-old is running")
    );
    assert!(!kinds(&store, id).contains(&WorkKind::FreshSessionRequested));
}

#[test]
fn asking_again_reopens_a_declined_request() {
    let (_dir, store) = temp_store();
    let id = store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("open")
        .review
        .id;
    store.request_fresh_session(id).expect("asking");
    store
        .decline_fresh_session(id, "still running")
        .expect("declining");
    let again = store.request_fresh_session(id).expect("asking again");
    assert!(again.declined_at.is_none());
    assert!(again.declined_reason.is_none());
}

#[test]
fn the_same_session_linked_again_does_not_answer_the_request() {
    let (_dir, store) = temp_store();
    let id = store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("open")
        .review
        .id;
    linked(&store, id, "allele-old");
    store.request_fresh_session(id).expect("asking");
    linked(&store, id, "allele-old");
    assert!(store.fresh_session_request(id).expect("read").is_some());
    linked(&store, id, "allele-new");
    assert!(store.fresh_session_request(id).expect("read").is_none());
}

#[test]
fn declining_needs_a_reason_and_a_request() {
    let (_dir, store) = temp_store();
    let id = store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("open")
        .review
        .id;
    assert!(matches!(
        store.decline_fresh_session(id, "nothing asked"),
        Err(StoreError::NotFound { .. })
    ));
    store.request_fresh_session(id).expect("asking");
    assert!(matches!(
        store.decline_fresh_session(id, "   "),
        Err(StoreError::InvalidValue { .. })
    ));
}

#[test]
fn a_dismissed_review_cannot_ask() {
    let (_dir, store) = temp_store();
    let id = store
        .open_review("acme/portal", 514, "c0ffee1")
        .expect("open")
        .review
        .id;
    store.dismiss_review(id, "not mine").expect("dismissed");
    assert!(store.request_fresh_session(id).is_err());
}
