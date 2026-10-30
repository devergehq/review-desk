//! `POST /reviews/{id}/fresh-session`: asked for only when Allele says the session is gone.
//! DEV-1142.

mod common;

use common::{fake_allele, seed, start_reading_sessions_from, start_with_no_github};

const GONE: &str = r#"{"result":"error","code":"bad_request","message":"no session with id x"}"#;
const RUNNING: &str = r#"{"result":"status","session":{"session_id":"allele-ae03f8ce","state":"running","state_age_secs":12}}"#;

#[test]
fn a_gone_session_can_be_replaced_and_the_request_is_on_the_session_read() {
    let (_dir, socket, _) = fake_allele(GONE);
    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);
    let path = format!("/reviews/{}/fresh-session", seeded.running_review_id);

    let asked = harness.api("POST", &path, None);
    assert_eq!(asked.status, 200, "{}", asked.text());
    assert!(asked.json()["fresh_session"]["requested_at"].is_string());
    assert!(asked.json()["fresh_session"]["declined_at"].is_null());

    let read = harness
        .api(
            "GET",
            &format!("/reviews/{}/session", seeded.running_review_id),
            None,
        )
        .json();
    assert!(read["fresh_session"]["requested_at"].is_string());
}

#[test]
fn a_running_session_is_not_replaced_and_nothing_is_recorded() {
    let (_dir, socket, _) = fake_allele(RUNNING);
    let harness = start_reading_sessions_from(&socket);
    let seeded = seed(&harness.store);

    let refused = harness.api(
        "POST",
        &format!("/reviews/{}/fresh-session", seeded.running_review_id),
        None,
    );
    assert_eq!(refused.status, 409);
    assert!(refused.text().contains("running"), "{}", refused.text());
    assert!(harness
        .store
        .fresh_session_request(seeded.running_review_id)
        .expect("read")
        .is_none());
}

#[test]
fn with_no_allele_or_no_session_nothing_is_asked() {
    let harness = start_with_no_github();
    let seeded = seed(&harness.store);
    for review in [seeded.running_review_id, seeded.review_id] {
        let refused = harness.api("POST", &format!("/reviews/{review}/fresh-session"), None);
        assert_eq!(refused.status, 409, "{}", refused.text());
        assert!(harness
            .store
            .fresh_session_request(review)
            .expect("read")
            .is_none());
    }
}
