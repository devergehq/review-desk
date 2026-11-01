//! `work list` reports a fresh session the developer asked for, and a dispatcher declines
//! one with `review fresh-session --declined`. DEV-1142.

mod common;

use common::{open_review, Desk};

#[test]
fn a_requested_fresh_session_is_work_until_declined() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let id: i64 = review.parse().expect("an id");
    desk.store()
        .request_fresh_session(id)
        .expect("the developer asks");

    // It outranks `review_requested`: both start a session, and this one has the
    // dispatcher confirm with Allele that the old one is gone first.
    let work = desk.json(&["work", "list", "--review", &review]);
    assert_eq!(work["work"][0]["kind"], "fresh_session_requested");

    let declined = desk.json(&[
        "review",
        "fresh-session",
        "--review",
        &review,
        "--declined",
        "Allele says allele-old is running",
    ]);
    assert_eq!(
        declined["fresh_session"]["declined_reason"],
        "Allele says allele-old is running"
    );
    let work = desk.json(&["work", "list", "--review", &review]);
    assert_eq!(work["work"][0]["kind"], "review_requested");
}

#[test]
fn declining_with_no_request_is_refused() {
    let desk = Desk::new();
    let review = open_review(&desk, "acme/portal", "514", "c0ffee1");
    let refused = desk.refused_json(&[
        "review",
        "fresh-session",
        "--review",
        &review,
        "--declined",
        "nothing asked",
    ]);
    assert!(refused.to_string().contains("not_found"), "{refused}");
}
