//! `GET /reviews/{id}/session`: what a review's Allele session is doing, over a real
//! socket on both sides.
//!
//! Allele is stood in for by a Unix socket that answers one line, the way Allele's own
//! control socket does. `src/allele.rs` tests the reading of every answer; these test that
//! the server asks only when it should and serves what it was told.

mod common;

use std::sync::atomic::Ordering;

use common::{fake_allele, seed, start, start_reading_sessions_from};
use review_desk_server::allele::TIMEOUT;

const AWAITING: &str = r#"{"result":"status","session":{"session_id":"allele-ae03f8ce","name":"Review","project":"acme","state":"awaiting_input","state_age_secs":372,"dispatched":true,"depth":1,"reply_to":"uds:/tmp/cc-socks/1.sock","group":null}}"#;

#[test]
fn a_session_blocked_on_a_prompt_is_served_as_awaiting_input_with_its_age() {
    let (_dir, socket, asked) = fake_allele(AWAITING);
    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);

    let answer = harness.api(
        "GET",
        &format!("/reviews/{}/session", seeded.running_review_id),
        None,
    );
    assert_eq!(answer.status, 200);
    let body = answer.json();
    assert_eq!(body["session_id"], "allele-ae03f8ce");
    assert_eq!(body["reading"]["status"], "awaiting_input");
    assert_eq!(body["reading"]["allele_state"], "awaiting_input");
    assert_eq!(body["reading"]["state_age_secs"], 372);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[test]
fn the_session_s_own_address_is_never_served() {
    let (_dir, socket, _asked) = fake_allele(AWAITING);
    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);

    let text = harness
        .api(
            "GET",
            &format!("/reviews/{}/session", seeded.running_review_id),
            None,
        )
        .text();
    assert!(!text.contains("uds:"), "{text}");
    assert!(!text.contains("reply_to"), "{text}");
}

#[test]
fn a_review_with_no_session_asks_allele_nothing() {
    let (_dir, socket, asked) = fake_allele(AWAITING);
    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);

    // The first seeded review records no session.
    let answer = harness.api(
        "GET",
        &format!("/reviews/{}/session", seeded.review_id),
        None,
    );
    assert_eq!(answer.status, 200);
    let body = answer.json();
    assert!(body["session_id"].is_null());
    assert!(body["reading"].is_null());
    assert_eq!(asked.load(Ordering::SeqCst), 0, "Allele was asked");
}

#[test]
fn with_no_allele_the_reading_is_null_and_nothing_else_changes() {
    let harness = start(review_desk_harvest::recorded::Recorded::nothing());
    let seeded = seed(&harness.store);

    let answer = harness.api(
        "GET",
        &format!("/reviews/{}/session", seeded.running_review_id),
        None,
    );
    assert_eq!(answer.status, 200);
    let body = answer.json();
    assert_eq!(body["session_id"], "allele-ae03f8ce");
    assert!(body["reading"].is_null());
}

#[test]
fn an_allele_that_never_answers_is_unreachable_within_the_timeout() {
    use std::os::unix::net::UnixListener;
    use std::time::{Duration, Instant};

    let dir = tempfile::Builder::new()
        .prefix("rd")
        .tempdir_in("/tmp")
        .expect("a temporary directory");
    let socket = dir.path().join("a.sock");
    let listener = UnixListener::bind(&socket).expect("binding a socket");
    let _holder = std::thread::spawn(move || {
        let held = listener.accept();
        std::thread::sleep(Duration::from_secs(4));
        drop(held);
    });

    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);

    let started = Instant::now();
    let answer = harness.api(
        "GET",
        &format!("/reviews/{}/session", seeded.running_review_id),
        None,
    );
    let took = started.elapsed();

    assert_eq!(answer.status, 200);
    assert_eq!(answer.json()["reading"]["status"], "unreachable");
    assert!(took < TIMEOUT + Duration::from_secs(1), "took {took:?}");
}

#[test]
fn an_unknown_review_is_not_found_and_asks_allele_nothing() {
    let (_dir, socket, asked) = fake_allele(AWAITING);
    let harness = start_reading_sessions_from(&socket);

    let answer = harness.api("GET", "/reviews/9001/session", None);
    assert_eq!(answer.status, 404);
    assert_eq!(answer.error_code(), "not_found");
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}
