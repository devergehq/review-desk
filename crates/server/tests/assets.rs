//! The dashboard, served from the binary — or the page that says it is not in there.
//!
//! **Both are legitimate builds and both are tested here.** Which one these tests assert
//! depends on whether `web/.output/public` existed when the crate was compiled, which is
//! why `the_build_is_one_way_or_the_other` exists: it fails on a third state, so a test
//! run can never silently assert nothing. CI compiles and runs this file twice, once each
//! way, which is what makes both branches genuinely covered rather than one of them being
//! covered on whichever machine happened to have Node.

mod common;

use review_desk_server::assets;

#[test]
fn the_build_is_one_way_or_the_other() {
    // There is no third state. If `FILES` is non-empty it must contain an index, because
    // every route the dashboard has resolves to one.
    if assets::embedded() {
        assert!(
            matches!(assets::resolve("/"), assets::Served::File { .. }),
            "this binary has files embedded but nothing at /"
        );
    } else {
        assert!(matches!(assets::resolve("/"), assets::Served::NotBuilt));
    }
}

#[test]
fn the_root_is_served() {
    let harness = common::start_with_no_github();
    let answer = harness.request("GET", "/", None);

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(
        answer.header("Content-Type"),
        Some("text/html; charset=utf-8"),
        "whichever page is served at / is HTML"
    );

    let page = answer.text();
    if assets::embedded() {
        assert!(
            page.contains("<!DOCTYPE html>") || page.contains("<!doctype html>"),
            "the embedded index should be an HTML document: {}",
            &page[..page.len().min(200)]
        );
        assert!(
            !page.contains("front end was not built"),
            "a binary with a front end must not serve the placeholder"
        );
    } else {
        assert!(
            page.contains("The front end was not built into this binary."),
            "a binary with no front end has to say so at /: {}",
            &page[..page.len().min(400)]
        );
        assert!(
            page.contains("pnpm run generate"),
            "the placeholder should name the command that fixes it"
        );
    }
}

#[test]
fn the_api_answers_whether_or_not_the_front_end_is_built() {
    // The acceptance criterion is that a binary built with no Node serves the API. The
    // API must therefore be independent of the embedding, in both directions.
    let harness = common::start_with_no_github();
    let answer = harness.api("GET", "/health", None);

    assert_eq!(answer.status, 200);
    assert_eq!(
        answer.json()["front_end_embedded"],
        assets::embedded(),
        "health should report which kind of build this is"
    );
}

#[test]
fn a_prerendered_route_loads_on_a_direct_request() {
    if !assets::embedded() {
        return; // Asserted by the run CI makes with the front end built.
    }

    let harness = common::start_with_no_github();

    // Nuxt wrote `rules/index.html` for `/rules`. A static file server has to find it
    // from the directory path, which is the thing `web.yml` already checks for files and
    // this checks over HTTP.
    for path in ["/rules", "/rules/"] {
        let answer = harness.request("GET", path, None);
        assert_eq!(answer.status, 200, "{path}: {}", answer.text());
        assert!(
            answer.text().contains("<!DOCTYPE html>") || answer.text().contains("<!doctype html>"),
            "{path} should serve the prerendered page"
        );
    }
}

#[test]
fn a_client_side_route_that_was_never_prerendered_falls_back_to_the_index() {
    if !assets::embedded() {
        return; // Asserted by the run CI makes with the front end built.
    }

    let harness = common::start_with_no_github();

    // No review 9001 was prerendered — the prerender list comes from the sample data. The
    // app is a single page application with server-side rendering off, so the index has
    // to be served and the client router resolves the route once the page is up.
    let answer = harness.request("GET", "/reviews/9001", None);

    assert_eq!(answer.status, 200, "{}", answer.text());
    assert_eq!(
        answer.header("Content-Type"),
        Some("text/html; charset=utf-8")
    );
}

#[test]
fn a_missing_file_is_not_found_rather_than_the_index() {
    if !assets::embedded() {
        return;
    }

    let harness = common::start_with_no_github();

    // This matters more than it looks. Falling back to the index for a path that looks
    // like a file would answer a missing script with HTML, and the browser's error would
    // then be a parse failure somewhere else entirely.
    let answer = harness.request("GET", "/_nuxt/does-not-exist.js", None);
    assert_eq!(answer.status, 404, "{}", answer.text());
}

#[test]
fn assets_are_served_with_the_type_the_browser_needs() {
    if !assets::embedded() {
        return;
    }

    let harness = common::start_with_no_github();
    let mut checked = 0;

    for (route, expected) in [
        (".js", "text/javascript; charset=utf-8"),
        (".css", "text/css; charset=utf-8"),
    ] {
        // Picked out of what was actually built rather than hard-coded: Nuxt's file names
        // are hashed and change on every build.
        let Some(path) = first_embedded_with_extension(route) else {
            continue;
        };
        let answer = harness.request("GET", &format!("/{path}"), None);
        assert_eq!(answer.status, 200, "/{path}: {}", answer.text());
        assert_eq!(
            answer.header("Content-Type"),
            Some(expected),
            "/{path} must be served as {expected}, or the browser will not run it"
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "a built front end should have at least one script or stylesheet to check"
    );
}

#[test]
fn the_api_prefix_is_never_answered_by_the_front_end() {
    let harness = common::start_with_no_github();

    // A path under /api/v1 that no operation claims has to be a JSON 404, not the
    // dashboard's index: a client parsing JSON would otherwise get HTML.
    let answer = harness.api("GET", "/not-an-operation", None);

    assert_eq!(answer.status, 404);
    assert_eq!(
        answer.header("Content-Type"),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(answer.error_code(), "not_found");
}

/// The first embedded file with this extension, as it is served.
fn first_embedded_with_extension(extension: &str) -> Option<String> {
    assets::embedded_routes()
        .into_iter()
        .find(|route| route.ends_with(extension))
        .map(str::to_string)
}
