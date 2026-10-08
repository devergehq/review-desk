//! The dashboard, embedded.
//!
//! `build.rs` writes the table this includes: every file of `web/.output/public`, or
//! nothing when that directory was not there at compile time. Both are legitimate
//! builds, and the server has to be honest about which one it is — DEV-861 requires
//! that `cargo build` works on a machine with no Node and that the binary then says so
//! at `/`, rather than serving a blank page or a 404 that looks like a bug.
//!
//! The front end is Nuxt in static mode with server-side rendering off, which decides
//! how routes map to files. `nuxt generate` writes `index.html` per route directory, so
//! `/rules` is `rules/index.html`; and because the app is a single page application, a
//! route that was never prerendered still has to load — the client router resolves it
//! once the page is up. That is what the fallback to the index is for, and it is why the
//! fallback is restricted to paths that do not look like files: a missing script must
//! 404, not quietly return HTML that the browser then fails to parse as JavaScript.

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// Whether this binary was built with a front end.
pub fn embedded() -> bool {
    !FILES.is_empty()
}

/// Every path the embedded site is served at.
///
/// Nuxt's asset file names carry a content hash and change on every build, so a test
/// that wants "some stylesheet" has to ask what was actually built rather than name one.
pub fn embedded_routes() -> Vec<&'static str> {
    FILES.iter().map(|(route, _)| *route).collect()
}

/// What to serve for a URL path.
#[derive(Debug, PartialEq, Eq)]
pub enum Served<'a> {
    /// A file, with the content type to serve it as.
    File { bytes: &'a [u8], mime: &'a str },
    /// Nothing matched and the path could not be a client-side route.
    Missing,
    /// This binary has no front end in it.
    NotBuilt,
}

/// Resolve a URL path against the embedded site.
pub fn resolve(path: &str) -> Served<'static> {
    if !embedded() {
        return Served::NotBuilt;
    }

    let trimmed = path.trim_start_matches('/');

    // `/` and `/reviews/17/` are directories; Nuxt wrote an index for each one, so each
    // path is tried as a file first and as a directory second.
    let as_file = if trimmed.is_empty() {
        "index.html".to_string()
    } else {
        trimmed.to_string()
    };
    let as_directory = format!("{}/index.html", trimmed.trim_end_matches('/'));

    for candidate in [as_file.as_str(), as_directory.as_str()] {
        if let Some((_, bytes)) = FILES.iter().find(|(route, _)| *route == candidate) {
            return Served::File {
                bytes,
                mime: mime_for(candidate),
            };
        }
    }

    // A path that looks like a file is a missing file. A path that does not is a route
    // the client router can resolve, so the index is the right answer: this is how a
    // deep link to a review that was never prerendered loads at all.
    if looks_like_a_file(trimmed) {
        return Served::Missing;
    }

    FILES
        .iter()
        .find(|(route, _)| *route == "index.html")
        .map_or(Served::Missing, |(_, bytes)| Served::File {
            bytes,
            mime: "text/html; charset=utf-8",
        })
}

/// Whether the last segment of a path has a file extension.
fn looks_like_a_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
}

/// The content type for a file name.
///
/// A short closed list rather than a MIME crate: this serves one known build, and a
/// wrong type here is visible the moment the page loads. `charset=utf-8` is stated on
/// every text type, because the generated HTML declares no `<meta charset>` fallback a
/// browser could guess from.
fn mime_for(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or_default();
    match extension {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/vnd.microsoft.icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The page a binary built with no front end serves at `/`.
///
/// It names the command that fixes it. A placeholder that only says something is missing
/// makes the reader go and find out how to build it; the build is two commands and they
/// fit on the page.
pub const NOT_BUILT_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Review Desk — front end not built</title>
<style>
  :root { color-scheme: light dark; }
  body {
    font: 16px/1.6 ui-sans-serif, system-ui, sans-serif;
    margin: 0; min-height: 100vh;
    display: grid; place-items: center;
    background: Canvas; color: CanvasText;
  }
  main { max-width: 34rem; padding: 2rem; }
  h1 { font-size: 1.25rem; margin: 0 0 1rem; }
  p { margin: 0 0 1rem; }
  pre {
    background: color-mix(in srgb, CanvasText 8%, Canvas);
    padding: 0.75rem 1rem; border-radius: 6px; overflow-x: auto;
  }
  code, pre { font-family: ui-monospace, monospace; font-size: 0.9em; }
</style>
</head>
<body>
<main>
  <h1>The front end was not built into this binary.</h1>
  <p>
    The JSON API is running and answering. This page is what is served at
    <code>/</code> when the binary was compiled without the dashboard's static
    build, which is what happens on a machine with no Node installed.
  </p>
  <p>To build a binary with the dashboard in it:</p>
  <pre>cd web &amp;&amp; pnpm install --frozen-lockfile &amp;&amp; pnpm run generate
cd .. &amp;&amp; cargo build --release</pre>
  <p>
    The API is unaffected either way. <code>GET /api/v1/health</code> answers from
    this binary now.
  </p>
</main>
</body>
</html>
"#;
