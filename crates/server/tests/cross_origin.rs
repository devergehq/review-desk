//! A web page open in the developer's browser must not be able to reach this API.
//!
//! **This is a test file about the sign-off decision, not about HTTP hygiene.** Confirming
//! a brief and approving a draft happen through this API and nowhere else, and an approved
//! draft is posted to GitHub under the developer's name by a review session. If a page on
//! another origin can approve a draft, then "only the developer can sign off" — the
//! decision the whole project rests on — is enforced by nothing.
//!
//! Every test here fails if its check is removed from `guard.rs`.

mod common;

use review_desk_store::DraftStatus;

/// The headers a browser on another origin would really send for a `fetch` with a JSON
/// body: its own `Origin`, and the `Host` of the server it is talking to.
fn foreign_fetch(host: &str) -> Vec<(&str, &str)> {
    vec![
        ("Host", host),
        ("Origin", "https://evil.example"),
        ("Content-Type", "application/json"),
    ]
}

#[test]
fn a_foreign_origin_cannot_approve_a_draft() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &foreign_fetch(&host),
        Some(r#"{"event":"approve"}"#),
    );

    assert!(
        answer.status >= 400,
        "a page on another origin approved a draft: {} {}",
        answer.status,
        answer.text()
    );

    // The refusal is only worth anything if the record did not move. This is the
    // assertion the whole file exists for.
    let draft = harness.store.get_draft(seeded.review_id).unwrap();
    assert_eq!(
        draft.status,
        DraftStatus::Ready,
        "the draft was approved by a request from https://evil.example"
    );
    assert!(draft.approved_at.is_none(), "an approval time was recorded");
}

#[test]
fn a_foreign_origin_cannot_confirm_a_brief() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/brief/confirm", seeded.review_id),
        &foreign_fetch(&host),
        None,
    );

    assert!(answer.status >= 400, "{}", answer.text());
    assert!(
        !harness
            .store
            .get_brief(seeded.review_id)
            .unwrap()
            .is_confirmed(),
        "the brief was confirmed by a request from https://evil.example"
    );
}

#[test]
fn a_foreign_origin_cannot_read_the_record_either() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    // Checked on reads as well as writes. The browser would refuse to hand the body back
    // anyway, since no CORS headers are sent — but the rule worth having is "no other
    // origin talks to this API", not a list of verbs.
    let answer = harness.raw("GET", "/api/v1/inbox", &foreign_fetch(&host), None);
    assert!(answer.status >= 400, "{}", answer.text());
}

#[test]
fn no_permissive_cors_header_is_ever_sent() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    // There is no origin this API wants to be readable from, so there is no header
    // saying there is. Asserted rather than assumed: one of these added "to make the
    // dashboard work" would undo every test in this file.
    for (method, path, body) in [
        ("GET", "/inbox", None),
        ("GET", "/rules", None),
        ("POST", "/reviews", Some(r#"{"pr_url":"nonsense"}"#)),
    ] {
        let answer = harness.api(method, path, body);
        for header in [
            "Access-Control-Allow-Origin",
            "Access-Control-Allow-Credentials",
            "Access-Control-Allow-Methods",
            "Access-Control-Allow-Headers",
        ] {
            assert_eq!(
                answer.header(header),
                None,
                "{method} {path} sent {header}, which would open the API to another origin"
            );
        }
    }
}

#[test]
fn a_rebound_hostname_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let port = harness.running.address().port();

    // DNS rebinding: a name the attacker controls, made to resolve to 127.0.0.1. The
    // connection really does arrive here, and the browser sends the attacker's name in
    // `Host` — which is the one thing that distinguishes it from the developer's own tab.
    for host in [
        format!("evil.example:{port}"),
        format!("rebind.attacker.test:{port}"),
        // A bare loopback address on the wrong port is not this server either.
        format!("127.0.0.1:{}", port.wrapping_add(1)),
    ] {
        let answer = harness.raw(
            "POST",
            &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
            &[
                ("Host", host.as_str()),
                ("Content-Type", "application/json"),
            ],
            Some(r#"{"event":"approve"}"#),
        );

        assert!(
            answer.status >= 400,
            "Host {host:?} reached a handler: {}",
            answer.text()
        );
    }

    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "a rebound hostname approved the draft"
    );
}

#[test]
fn an_absent_host_is_refused() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    // Absent is refused as firmly as wrong. HTTP/1.1 requires a Host, and treating its
    // absence as "must be fine" would make the check optional for anything that left it
    // off.
    let answer = harness.raw(
        "GET",
        "/api/v1/inbox",
        &[("Content-Type", "application/json")],
        None,
    );
    assert!(answer.status >= 400, "{}", answer.text());
}

#[test]
fn localhost_is_still_a_name_this_server_answers_to() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);
    let port = harness.running.address().port();

    // Allowing `localhost` is not a hole. Rebinding points a name *the attacker controls*
    // at 127.0.0.1; the browser then sends that name, not `localhost`. Refusing it would
    // only break the developer typing http://localhost:7777 into their own browser.
    let answer = harness.raw(
        "GET",
        "/api/v1/inbox",
        &[
            ("Host", &format!("localhost:{port}")),
            ("Content-Type", "application/json"),
        ],
        None,
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
}

#[test]
fn a_plain_html_form_cannot_make_a_write() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    // The three content types a `<form>` can send, and nothing else. This is the check
    // that does not depend on browser behaviour: even a form that somehow carried no
    // `Origin` could not shape a request this API accepts.
    for content_type in [
        "application/x-www-form-urlencoded",
        "multipart/form-data; boundary=----x",
        "text/plain;charset=UTF-8",
    ] {
        let answer = harness.raw(
            "POST",
            &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
            &[("Host", host.as_str()), ("Content-Type", content_type)],
            Some(r#"{"event":"approve"}"#),
        );

        assert_eq!(
            answer.status,
            400,
            "a form sending {content_type} made a write: {}",
            answer.text()
        );
        assert_eq!(answer.error_code(), "invalid_value");
        assert_eq!(answer.json()["field"], "Content-Type");
    }

    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "a form post approved the draft"
    );
}

#[test]
fn a_write_with_no_content_type_at_all_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/brief/confirm", seeded.review_id),
        &[("Host", host.as_str())],
        None,
    );

    assert_eq!(answer.status, 400, "{}", answer.text());
}

#[test]
fn a_caller_that_is_not_a_browser_still_works() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    // **An absent `Origin` is allowed, deliberately.** Nothing that is not a browser sends
    // one — curl, a script, a review session — and a browser always sends one on a
    // cross-origin request, including a form submission. Refusing the absent case would
    // break every non-browser caller and stop no attacker; the content-type check is what
    // covers the form.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/brief/confirm", seeded.review_id),
        &[
            ("Host", host.as_str()),
            ("Content-Type", "application/json"),
        ],
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert!(harness
        .store
        .get_brief(seeded.review_id)
        .unwrap()
        .is_confirmed());
}

#[test]
fn the_dashboards_own_origin_is_accepted() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let base = harness.running.base();
    let host = base.trim_start_matches("http://").to_string();

    // The checks are worthless if they also block the dashboard, which is the only client
    // this API has. A page served from this binary sends exactly these.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/brief/confirm", seeded.review_id),
        &[
            ("Host", host.as_str()),
            ("Origin", base.as_str()),
            ("Content-Type", "application/json"),
        ],
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
}

#[test]
fn a_header_name_in_another_case_is_still_the_same_header() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    // HTTP header names are not case-sensitive. A check that assumed they were would be
    // bypassed by changing one letter, which is the cheapest possible attack.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &[
            ("hOsT", host.as_str()),
            ("ORIGIN", "https://evil.example"),
            ("content-TYPE", "application/json"),
        ],
        Some(r#"{"event":"approve"}"#),
    );

    assert!(
        answer.status >= 400,
        "a differently-cased Origin header slipped past the check: {}",
        answer.text()
    );
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready
    );
}
