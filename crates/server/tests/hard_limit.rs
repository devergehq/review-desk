//! **Review Desk never starts, resumes or drives Claude.** DEV-1335, for DEV-1331's hard limit.
//!
//! Not a behaviour any other test could catch, because the failure is code that exists, not
//! code that runs: a `claude` invocation behind a flag nobody sets in CI would pass every
//! other test here. So this reads the source under `crates/` and `web/` and fails on any of:
//!
//! | Pattern | Where | Why it is refused |
//! | -- | -- | -- |
//! | the print flag, short or long | everything | running Claude headless |
//! | the streaming JSON format's name | everything | driving Claude over a pipe |
//! | `Command::new(` | shipped code, `crates/*/src` | spawning any process; `gh` is the one there is |
//! | the dot-directory Claude keeps its files in | everything | reading the developer's Claude files or login |
//!
//! **This file excludes itself by path**: [`THIS_FILE`] is this file's location, made from
//! `CARGO_MANIFEST_DIR` rather than typed, and the walk skips exactly that path. It names
//! the patterns, so it would otherwise find them. A renamed copy is not skipped, and fails.
//! The patterns are also assembled from pieces below, so a grep for them over the repository
//! does not land here either.
//!
//! Test code under `crates/*/tests` is outside the `Command::new(` rule: it spawns the binary
//! under test, and `hostname`/`ifconfig` to find an address to bind. None of it ships.

use std::path::{Path, PathBuf};

/// This file, which names every pattern and is skipped by path.
fn this_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("hard_limit.rs")
}

const THIS_FILE: &str = "crates/server/tests/hard_limit.rs";

/// The repository's root, two up from this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the repository root")
}

/// Directories that are built, installed or recorded, never written as source.
const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".nuxt",
    ".output",
    "dist",
    "node_modules",
    "playwright-report",
    "target",
    "test-results",
];

/// One thing that may not appear.
struct Refused {
    pattern: String,
    why: &'static str,
    /// Only files under these root-relative prefixes are read for it; empty is everywhere.
    only_under: &'static [&'static str],
    /// Root-relative files allowed to hold it, each with the reason.
    allowed: &'static [(&'static str, &'static str)],
}

fn refused() -> Vec<Refused> {
    let claude = "claude";
    vec![
        Refused {
            pattern: format!("{claude} -p"),
            why: "runs Claude headless",
            only_under: &[],
            allowed: &[],
        },
        Refused {
            pattern: format!("{claude} --{}", "print"),
            why: "runs Claude headless",
            only_under: &[],
            allowed: &[],
        },
        Refused {
            pattern: format!("stream-{}", "json"),
            why: "drives Claude over a pipe",
            only_under: &[],
            allowed: &[],
        },
        Refused {
            pattern: format!("Command::{}(", "new"),
            why: "spawns a process from shipped code",
            only_under: &[
                "crates/cli/src/",
                "crates/harvest/src/",
                "crates/server/src/",
                "crates/store/src/",
            ],
            allowed: &[(
                "crates/harvest/src/gh.rs",
                "the harvest reads GitHub through `gh`, DEV-1335's one allowed process",
            )],
        },
        Refused {
            pattern: format!(".{claude}/"),
            why: "reaches into the developer's Claude files or login",
            only_under: &[],
            allowed: &[(
                "crates/harvest/tests/fixtures/compare.json",
                "a recorded GitHub diff of another repository's code, kept as data",
            )],
        },
    ]
}

/// Every file under `directory`, skipping what [`SKIPPED_DIRECTORIES`] names.
fn files(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let name = entry.file_name();
            if !SKIPPED_DIRECTORIES.iter().any(|skip| name == *skip) {
                files(&path, into);
            }
        } else if kind.is_file() {
            into.push(path);
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// What the walk found: `path: pattern (why)`, one per line.
fn breaches(root: &Path) -> Vec<String> {
    let me = this_file().canonicalize().expect("this test's own file");
    let mut all = Vec::new();
    files(&root.join("crates"), &mut all);
    files(&root.join("web"), &mut all);
    assert!(
        all.len() > 50,
        "the walk found {} files; it is not reading the source",
        all.len()
    );

    let rules = refused();
    let mut found = Vec::new();
    for path in all {
        if path.canonicalize().is_ok_and(|path| path == me) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        for rule in &rules {
            if !rule.only_under.is_empty()
                && !rule
                    .only_under
                    .iter()
                    .any(|under| relative.starts_with(under))
            {
                continue;
            }
            if rule.allowed.iter().any(|(allowed, _)| *allowed == relative) {
                continue;
            }
            if contains(&bytes, rule.pattern.as_bytes()) {
                found.push(format!("{relative}: `{}` ({})", rule.pattern, rule.why));
            }
        }
    }
    found
}

#[test]
fn nothing_under_crates_or_web_starts_resumes_or_drives_claude() {
    let found = breaches(&root());
    assert!(
        found.is_empty(),
        "Review Desk never starts, resumes or drives Claude (DEV-1331). Found:\n{}",
        found.join("\n")
    );
}

#[test]
fn this_file_is_where_it_says_it_is() {
    assert!(
        this_file().is_file(),
        "{THIS_FILE} has moved; the walk skips it by that path"
    );
    assert!(this_file().ends_with(THIS_FILE));
}

#[test]
fn every_allowed_file_still_exists() {
    let root = root();
    for rule in refused() {
        for (allowed, why) in rule.allowed {
            assert!(
                root.join(allowed).is_file(),
                "{allowed} is allowed `{}` because {why}, and is gone; drop it from the list",
                rule.pattern
            );
        }
    }
}

/// The walk would catch each pattern, so a rule that matches nothing is not a rule that passes.
#[test]
fn each_pattern_is_caught_in_a_file_that_holds_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for (index, rule) in refused().iter().enumerate() {
        let under = rule.only_under.first().copied().unwrap_or("web/");
        let path = directory
            .path()
            .join(under)
            .join(format!("caught{index}.txt"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("let x = \"{}\";\n", rule.pattern)).unwrap();
    }
    for filler in 0..60 {
        let path = directory
            .path()
            .join("web")
            .join(format!("filler{filler}.ts"));
        std::fs::write(path, "export {}\n").unwrap();
    }
    let root = directory.path().canonicalize().unwrap();
    let found = breaches(&root);
    assert_eq!(found.len(), refused().len(), "{found:#?}");
}
