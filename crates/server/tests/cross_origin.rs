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

/// The name the local reverse proxy forwards, as `--hostname` would be given it.
///
/// A subdomain, because macOS sends a bare `.local` apex to mDNS whatever
/// `/etc/resolver` says. No port: the proxy terminates TLS on 443 and a browser sends no port for the default
/// one, so this is the exact `Host` that arrives.
const PROXIED: &str = "app.review-desk.local";

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
fn no_write_this_contract_declares_is_reachable_from_another_origin() {
    // **Walked from `Operation::ALL` rather than listed.** The two tests either side of
    // this one name the writes the project rests on; this one is what makes an operation
    // added later guarded by default. The guard runs before any handler, so the body does
    // not have to be one the operation would accept: a refusal naming the origin is proof
    // the request never reached the record.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    let mut writes = 0;
    for operation in review_desk_server::route::Operation::ALL {
        let route = operation
            .route()
            .replace("/reviews/1", &format!("/reviews/{}", seeded.review_id))
            .replace("/rules/1/", &format!("/rules/{}/", seeded.rule_id));
        let (method, path) = route.split_once(' ').expect("a method and a path");
        if method == "GET" {
            continue;
        }
        writes += 1;

        let answer = harness.raw(
            method,
            &format!("/api/v1{path}"),
            &foreign_fetch(&host),
            Some("{}"),
        );

        assert_eq!(
            answer.status,
            400,
            "{} ({}) answered a page on another origin: {}",
            operation.route(),
            operation.id(),
            answer.text()
        );
        assert!(
            answer.json()["message"]
                .as_str()
                .expect("a message")
                .contains("evil.example"),
            "{} was refused for some other reason than its origin: {}",
            operation.route(),
            answer.text()
        );
    }

    assert!(writes >= 13, "only {writes} writes were walked");
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "and no write landed"
    );
    assert!(
        harness
            .store
            .list_outcomes(seeded.review_id)
            .unwrap()
            .iter()
            .all(|outcome| outcome.kind != review_desk_store::OutcomeKind::Ticketed),
        "nor any decision"
    );
}

#[test]
fn a_foreign_origin_cannot_decide_about_a_finding() {
    // The same argument as approving a draft, and for the same reason it is a test of its
    // own: cutting a finding changes what gets posted under the developer's name, and
    // "ticket what is left, and approve" approves.
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    let cut = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/findings/1/decide", seeded.review_id),
        &foreign_fetch(&host),
        Some(r#"{"decision":"cut"}"#),
    );
    assert!(cut.status >= 400, "{}", cut.text());

    let ticket = harness.raw(
        "POST",
        &format!(
            "/api/v1/reviews/{}/draft/ticket-and-approve",
            seeded.review_id
        ),
        &foreign_fetch(&host),
        Some("{}"),
    );
    assert!(ticket.status >= 400, "{}", ticket.text());

    // The assertions the file exists for: the record did not move.
    let finding = harness.store.get_finding(seeded.review_id, 1).unwrap();
    assert_eq!(finding.disposition, review_desk_store::Disposition::InDraft);
    let draft = harness.store.get_draft(seeded.review_id).unwrap();
    assert_eq!(draft.status, DraftStatus::Ready);
    // F1 is anchored, so since DEV-929 it is a row in the index and a thread on the code
    // rather than a section — and a cut would take the row with it.
    assert!(
        draft.body.contains("| 🔴 Blocker · F1 |"),
        "a page on https://evil.example recomposed the body"
    );
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
fn a_null_origin_is_refused() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);
    let host = harness
        .running
        .base()
        .trim_start_matches("http://")
        .to_string();

    // `null` is what a browser sends from a sandboxed frame, from a `file://` page, and —
    // the case that matters — from an https page targeting an http URL under the default
    // `strict-origin-when-cross-origin` policy. That is exactly evil.example reaching
    // 127.0.0.1. It is *present* and it is not this server's origin, so it is refused;
    // asserted rather than inferred from the allowlist being an allowlist.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &[
            ("Host", host.as_str()),
            ("Origin", "null"),
            ("Content-Type", "application/json"),
        ],
        Some(r#"{"event":"approve"}"#),
    );

    assert!(answer.status >= 400, "{}", answer.text());
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "an Origin of null approved the draft"
    );
}

#[test]
fn the_dashboard_is_guarded_too_not_only_the_api() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);
    let port = harness.running.address().port();

    // The guard began inside the API branch, which left the static branch unguarded. No
    // data leaks that way — the dashboard bundle is public code — but one rule covering
    // the whole server is a thing a reader can check and two rules is not.
    let answer = harness.raw(
        "GET",
        "/",
        &[("Host", &format!("evil.example:{port}"))],
        None,
    );

    assert!(
        answer.status >= 400,
        "a rebound hostname was served the dashboard: {}",
        answer.status
    );
}

#[test]
fn nothing_may_frame_this_server() {
    let harness = common::start_with_no_github();
    let seeded = common::seed(&harness.store);

    // The guard stops another origin *talking* to the API. It cannot stop one *embedding*
    // the dashboard: a click stolen through a transparent iframe produces a request that
    // genuinely comes from the dashboard's own origin with the right content type, so
    // every check passes. One stolen click on the draft screen publishes a review the
    // developer never read, under their name, and the draft is immutable afterwards.
    for (method, path, body) in [
        ("GET", "/".to_string(), None),
        ("GET", format!("/reviews/{}/draft", seeded.review_id), None),
        ("GET", "/api/v1/inbox".to_string(), None),
        ("GET", "/does-not-exist".to_string(), None),
    ] {
        let answer = harness.request(method, &path, body);
        assert_eq!(
            answer.header("X-Frame-Options"),
            Some("DENY"),
            "{path} can be framed: no X-Frame-Options"
        );
        assert_eq!(
            answer.header("Content-Security-Policy"),
            Some("frame-ancestors 'none'"),
            "{path} can be framed: no frame-ancestors"
        );
    }
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

// --- The one host `--hostname` may name ----------------------------------------------
//
// `review-desk serve --hostname review-desk.local` is how the dashboard is reached at a
// name instead of at a port: the machine's reverse proxy forwards that `Host`, and until
// the server is told the name it refuses its own dashboard. These tests are the boundary
// of what naming it widens. `crates/server/src/hostname.rs` argues why it is not the
// configurable guard `the_guard_has_no_way_to_be_turned_off` forbids; this is where that
// argument is checked rather than asserted.

#[test]
fn a_named_hostname_is_answered() {
    let harness = common::start_answering_to(PROXIED);
    common::seed(&harness.store);

    let answer = harness.raw("GET", "/api/v1/inbox", &[("Host", PROXIED)], None);

    assert_eq!(
        answer.status,
        200,
        "the server refused the name it was told to answer to: {}",
        answer.text()
    );
}

#[test]
fn a_hostname_that_was_not_named_is_still_refused() {
    let harness = common::start_with_no_github();
    common::seed(&harness.store);

    // The default. Nothing is widened by the flag existing — only by it being typed.
    let answer = harness.raw("GET", "/api/v1/inbox", &[("Host", PROXIED)], None);

    assert!(
        answer.status >= 400,
        "a server told no hostname answered to one: {}",
        answer.text()
    );
}

#[test]
fn naming_one_hostname_does_not_name_its_neighbours() {
    let harness = common::start_answering_to(PROXIED);
    let seeded = common::seed(&harness.store);

    // Exact, never a suffix or a prefix match. Both halves matter: a suffix match would
    // make every name an attacker can register under a `.local` they control into this
    // server, and a prefix match would hand them `review-desk.local.evil.example`.
    for host in [
        "evil.app.review-desk.local",
        "app.review-desk.local.evil.example",
        "xapp.review-desk.local",
        "app.review-desk.localx",
        "app.review-desk.local.",
        "evil.example",
    ] {
        let answer = harness.raw(
            "POST",
            &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
            &[("Host", host), ("Content-Type", "application/json")],
            Some(r#"{"event":"approve"}"#),
        );

        assert!(
            answer.status >= 400,
            "Host {host:?} reached a handler on a server that named only {PROXIED:?}: {}",
            answer.text()
        );
    }

    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "a neighbour of the named hostname approved the draft"
    );
}

#[test]
fn the_named_hostnames_own_origin_can_write() {
    let harness = common::start_answering_to(PROXIED);
    let seeded = common::seed(&harness.store);

    // What the dashboard really sends when it is served through the proxy. If this is
    // refused then the name is useless: browsing works and signing off does not.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/brief/confirm", seeded.review_id),
        &[
            ("Host", PROXIED),
            ("Origin", "https://app.review-desk.local"),
            ("Content-Type", "application/json"),
        ],
        None,
    );

    assert_eq!(answer.status, 200, "{}", answer.text());
}

#[test]
fn the_named_hostname_is_https_and_not_http() {
    let harness = common::start_answering_to(PROXIED);
    let seeded = common::seed(&harness.store);

    // The proxy terminates TLS and redirects port 80 to 443, so nothing is ever served
    // on `http://review-desk.local` and no browser can be on that origin. Accepting it
    // would widen the allowlist to an origin that does not exist.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &[
            ("Host", PROXIED),
            ("Origin", "http://app.review-desk.local"),
            ("Content-Type", "application/json"),
        ],
        Some(r#"{"event":"approve"}"#),
    );

    assert!(answer.status >= 400, "{}", answer.text());
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "an http origin on the named hostname approved the draft"
    );
}

#[test]
fn a_foreign_origin_through_the_named_hostname_is_still_refused() {
    let harness = common::start_answering_to(PROXIED);
    let seeded = common::seed(&harness.store);

    // **This is the property that chose this design.** A proxy can be made to serve the
    // name by rewriting `Host` and `Origin` instead, and `web/nuxt.config.ts` does
    // exactly that for the dev server — but rewriting `Origin` launders it, so a page on
    // any origin arrives wearing the dashboard's and `web/dev-proxy.ts` has to filter the
    // real one back out. Naming the host rewrites nothing: a foreign `Origin` arrives as
    // itself and the check that was already here refuses it.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &[
            ("Host", PROXIED),
            ("Origin", "https://evil.example"),
            ("Content-Type", "application/json"),
        ],
        Some(r#"{"event":"approve"}"#),
    );

    assert!(answer.status >= 400, "{}", answer.text());
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "a foreign origin approved the draft through the named hostname"
    );
}

#[test]
fn a_form_post_through_the_named_hostname_is_still_refused() {
    let harness = common::start_answering_to(PROXIED);
    let seeded = common::seed(&harness.store);

    // The third check is not weakened by the first one being widened. A form cannot send
    // JSON whatever `Host` it is sent to.
    let answer = harness.raw(
        "POST",
        &format!("/api/v1/reviews/{}/draft/approve", seeded.review_id),
        &[
            ("Host", PROXIED),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ],
        Some("event=approve"),
    );

    assert!(answer.status >= 400, "{}", answer.text());
    assert_eq!(
        harness.store.get_draft(seeded.review_id).unwrap().status,
        DraftStatus::Ready,
        "a form post approved the draft through the named hostname"
    );
}

/// **The guard takes no configuration, and DEV-862 is where that was first put under
/// pressure.**
///
/// That issue points the dashboard at this API, which means a `nuxt dev` server on
/// `localhost:3000` now talks to it — and the obvious way to make that work is a
/// development bypass: an allowed-origins list, an environment variable, a `--dev` flag.
/// Any of them would be one mistake away from being left on in a build that signs off
/// reviews, and the sign-off decision would then rest on a configuration value.
///
/// It was made to work the other way round. `web/nuxt.config.ts` has the proxy state what
/// it is really talking to — `changeOrigin` rewrites `Host`, and `headers.origin` replaces
/// the browser's `Origin` with this server's own — so what arrives is a request that
/// genuinely names this server. The guard is unchanged and knows nothing about that proxy;
/// it would refuse that dev server exactly as it refuses any other origin.
///
/// **Serving the dashboard at a hostname moved this line, and it is worth being exact
/// about where it now is.** `review-desk serve --hostname review-desk.local` lets the
/// developer add one host to the first check, because the machine's reverse proxy forwards
/// that `Host` and the server otherwise refuses its own dashboard. That is a widening, and
/// the reason it is not the bypass this test was written against is the shape of it: one
/// name, of a type that parsed it, passed as an argument that has to be typed. There is no
/// list, no environment variable, no build flag and no default — and because nothing is
/// rewritten anywhere, a foreign `Origin` arriving on that host is still refused by the
/// second check. `crates/server/src/hostname.rs` makes the argument; the tests above it
/// check it; this one holds the shape.
///
/// So the needles below are unchanged, and the test now reads `hostname.rs` as well —
/// a bypass that moved into the module next door would otherwise pass. The signature is
/// still pinned, and still pinned to carry at most one name rather than a collection.
#[test]
fn the_guard_has_no_way_to_be_turned_off() {
    let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Both files, because the guard's policy is only as fixed as the narrowest thing it
    // is allowed to be told, and that is now defined one module over.
    for module in ["src/guard.rs", "src/hostname.rs"] {
        let source = std::fs::read_to_string(crate_root.join(module)).expect("reading the module");

        for (needle, what) in [
            ("env::var", "a read of the environment"),
            ("env!(", "a compile-time environment variable"),
            (
                "option_env!",
                "an optional compile-time environment variable",
            ),
            ("cfg!(debug_assertions)", "a debug-build branch"),
            ("#[cfg(debug_assertions)]", "a debug-build branch"),
            ("cfg!(feature", "a Cargo feature branch"),
            ("allowed_origins", "a configurable origin list"),
            ("allow_origin", "a configurable origin list"),
            ("Access-Control-Allow", "a CORS header"),
        ] {
            assert!(
                !source.contains(needle),
                "crates/server/{module} contains {needle:?} — {what}. The guard takes no \
                 configuration: the only things it is told are the address the listener \
                 actually bound and at most one hostname the developer typed, and \
                 everything else it decides from the request. A development loop that \
                 needed this relaxed would be a development loop that could sign off a \
                 review from any page the developer has open."
            );
        }
    }

    let guard =
        std::fs::read_to_string(crate_root.join("src/guard.rs")).expect("reading the guard");

    // Whitespace is collapsed so that the assertion is about the signature rather than
    // about where rustfmt chose to wrap it.
    let collapsed = guard.split_whitespace().collect::<Vec<_>>().join(" ");

    // And the things it *is* told stay the bound address and one optional name, rather
    // than becoming a list of hosts somebody can add to.
    assert!(
        collapsed.contains(
            "pub fn check( bound: SocketAddr, proxied: Option<&Hostname>, method: &str, \
             headers: Headers<'_>, )"
        ),
        "the guard's signature changed; it takes the bound address, at most one hostname, \
         the method and the headers, and nothing that could carry a policy"
    );

    for (needle, what) in [
        ("Vec<Hostname>", "a list of hostnames"),
        ("&[Hostname]", "a list of hostnames"),
        ("Vec<&Hostname>", "a list of hostnames"),
    ] {
        assert!(
            !collapsed.contains(needle),
            "crates/server/src/guard.rs contains {needle:?} — {what}. One name is a name \
             the developer typed; a list is a configuration file waiting to happen."
        );
    }
}
