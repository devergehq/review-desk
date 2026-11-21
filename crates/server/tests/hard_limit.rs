//! **Review Desk never starts, resumes or drives Claude.** DEV-1335, for DEV-1331's hard limit.
//!
//! Not a behaviour any other test could catch, because the failure is code that exists, not
//! code that runs: a Claude invocation behind a flag nobody sets in CI would pass every other
//! test here. So this reads every file under `crates/` and `web/` and fails on any of:
//!
//! | Rule | Where | Why it is refused |
//! | -- | -- | -- |
//! | Claude's print flag, short or long, after its name | everywhere | running Claude headless |
//! | the streaming JSON format's name | everywhere | driving Claude over a pipe |
//! | Claude's name as a quoted program: the bare name, a command line starting with it, or a path ending in it | everywhere | starting Claude, whatever the call looks like |
//! | Claude's dot-directory as a path component | everywhere | reading the developer's Claude files or login |
//! | a process API: `std`/`tokio` `process::Command` however imported or aliased, `Command::new`, `posix_spawn`, `exec*` | shipped Rust, `crates/*/src` | spawning anything; `gh` is the one there is, pinned by count |
//! | a process API: `child_process`, `execa`, `Bun.spawn`, `Deno.Command` | shipped web code, `web/app` and `web/nuxt.config.ts` | the dashboard spawns nothing |
//!
//! The code rules are matched on the file with **all whitespace removed**, so a space before a
//! parenthesis or a line break inside an import changes nothing; the prose rules are matched
//! with whitespace collapsed to one space.
//!
//! **This file excludes itself by path**: [`this_file`] is this file's location, made from
//! `CARGO_MANIFEST_DIR` rather than typed, and the walk skips exactly that path. It names the
//! patterns, so it would otherwise find them. A renamed copy is not skipped, and fails. The
//! patterns are also assembled from pieces, so a grep for them over the repository does not land
//! here either.
//!
//! **What is not read, by root-relative path, never by a directory's name** — a source
//! directory called `dist` deep in `web/app` is read like any other: the build's own output
//! (`target`, `web/.nuxt`, `web/.output`, `web/dist`), installed packages (`web/node_modules`),
//! test output, and the `.json` files in `crates/harvest/tests/fixtures`, which are GitHub's
//! answers about other repositories — another project's source, recorded as data. Anything
//! else in that directory is read like any source, so code cannot hide there by its
//! directory. A symlinked file is read through
//! its link; a symlinked directory anywhere but those paths fails the walk rather than being
//! skipped.
//!
//! Test code (`crates/*/tests`, `web/test`, `web/e2e`, `web/scripts`) is outside the two
//! process-API rules: it spawns the binary under test, the type generator, and `hostname` or
//! `ifconfig` to find an address to bind. None of it ships, and the other four rules still
//! apply to it.

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

/// Root-relative paths that are built, installed, recorded output or recorded data.
const NOT_READ: &[&str] = &[
    ".git",
    "target",
    "web/.nuxt",
    "web/.output",
    "web/dist",
    "web/node_modules",
    "web/playwright-report",
    "web/test-results",
];

/// Recorded data: the `.json` files directly in this directory are not read; nothing else is skipped.
const RECORDED: &str = "crates/harvest/tests/fixtures";

fn is_recorded(relative: &str) -> bool {
    relative
        .strip_prefix(RECORDED)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| !name.contains('/') && name.ends_with(".json"))
}

/// One file, three ways: as written, whitespace collapsed to one space, and whitespace gone.
struct Text {
    raw: String,
    collapsed: String,
    stripped: String,
}

impl Text {
    fn of(bytes: &[u8]) -> Self {
        let raw = String::from_utf8_lossy(bytes).into_owned();
        let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let stripped = raw.split_whitespace().collect::<String>();
        Text {
            raw,
            collapsed,
            stripped,
        }
    }
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

const CLAUDE: &str = concat!("cla", "ude");

fn print_flag(text: &Text) -> usize {
    count(&text.collapsed, &format!("{CLAUDE} -p"))
        + count(&text.collapsed, &format!("{CLAUDE} --{}", "print"))
}

fn stream_json(text: &Text) -> usize {
    count(&text.raw, &format!("stream-{}", "json"))
}

/// The name as a program: `"claude"`, `"claude …"`, `"…/claude"`, in any of three quotes.
fn quoted_program(text: &Text) -> usize {
    ['"', '\'', '`']
        .iter()
        .map(|quote| {
            count(&text.raw, &format!("{quote}{CLAUDE}{quote}"))
                + count(&text.raw, &format!("{quote}{CLAUDE} "))
                + count(&text.raw, &format!("/{CLAUDE}{quote}"))
        })
        .sum()
}

/// `.claude` where it starts a path component: not `docs.claude.com`, but `~/.claude`,
/// `".claude"`, `/.claude/` and `.join(".claude")`.
fn dot_directory(text: &Text) -> usize {
    let needle = format!(".{CLAUDE}");
    text.raw
        .match_indices(&needle)
        .filter(|(at, _)| {
            let before = text.raw[..*at].chars().next_back();
            let after = text.raw[at + needle.len()..].chars().next();
            let starts_a_component =
                before.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '.');
            let ends_a_component =
                after.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-');
            starts_a_component && ends_a_component
        })
        .count()
}

/// Any way shipped Rust reaches a process API, whitespace removed.
fn rust_process(text: &Text) -> usize {
    let stripped = &text.stripped;
    // `process::Command` in a path, an import or an alias, and `Command` inside a
    // `process::{…}` import list.
    let in_lists = stripped
        .match_indices("process::{")
        .filter(|(at, _)| {
            let list = &stripped[at + "process::{".len()..];
            let list = &list[..list.find('}').unwrap_or(list.len())];
            list.split(',')
                .any(|item| item == "Command" || item.starts_with("Commandas"))
        })
        .count();
    count(stripped, "process::Command")
        + in_lists
        + count(stripped, "Command::new(")
        + count(stripped, "posix_spawn")
        + count(stripped, "libc::exec")
        + count(stripped, "libc::fork")
        + count(stripped, "nix::unistd::exec")
}

/// Any way the dashboard's shipped code reaches a process API.
fn web_process(text: &Text) -> usize {
    count(&text.raw, &format!("child_{}", "process"))
        + count(&text.raw, &format!("exe{}", "ca"))
        + count(&text.stripped, "Bun.spawn")
        + count(&text.stripped, "Deno.Command")
}

/// One thing that may not appear.
struct Refused {
    name: &'static str,
    why: &'static str,
    found: fn(&Text) -> usize,
    /// Only files under these root-relative prefixes are read for it; empty is everywhere.
    only_under: &'static [&'static str],
    /// Root-relative files allowed to hold it, how many times, and why.
    allowed: &'static [(&'static str, usize, &'static str)],
}

const SHIPPED_RUST: &[&str] = &[
    "crates/cli/src/",
    "crates/harvest/src/",
    "crates/server/src/",
    "crates/store/src/",
];

const SHIPPED_WEB: &[&str] = &["web/app/", "web/nuxt.config.ts"];

fn refused() -> Vec<Refused> {
    vec![
        Refused {
            name: "Claude's print flag",
            why: "runs Claude headless",
            found: print_flag,
            only_under: &[],
            allowed: &[],
        },
        Refused {
            name: "the streaming JSON format",
            why: "drives Claude over a pipe",
            found: stream_json,
            only_under: &[],
            allowed: &[],
        },
        Refused {
            name: "Claude's name as a quoted program",
            why: "starts Claude",
            found: quoted_program,
            only_under: &[],
            allowed: &[],
        },
        Refused {
            name: "Claude's dot-directory",
            why: "reaches into the developer's Claude files or login",
            found: dot_directory,
            only_under: &[],
            allowed: &[],
        },
        Refused {
            name: "a process API in shipped Rust",
            why: "spawns a process",
            found: rust_process,
            only_under: SHIPPED_RUST,
            // The `use std::process::{Command, Stdio}` and the one `Command::new(&self.program)`:
            // two, and a third anywhere in the file fails.
            allowed: &[(
                "crates/harvest/src/gh.rs",
                2,
                "the harvest reads GitHub through `gh`, the one process Review Desk runs",
            )],
        },
        Refused {
            name: "a process API in the dashboard",
            why: "spawns a process from the dashboard",
            found: web_process,
            only_under: SHIPPED_WEB,
            allowed: &[],
        },
    ]
}

/// Every file under `directory` that is read, or the symlinked directories that stopped it.
fn files(root: &Path, directory: &Path, into: &mut Vec<PathBuf>, linked: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let relative = relative(root, &path);
        if NOT_READ.contains(&relative.as_str()) || is_recorded(&relative) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            // Followed for a file; a directory reached through a link is reported, not read.
            match std::fs::metadata(&path) {
                Ok(target) if target.is_file() => into.push(path),
                Ok(target) if target.is_dir() => linked.push(relative),
                _ => {}
            }
        } else if kind.is_dir() {
            files(root, &path, into, linked);
        } else if kind.is_file() {
            into.push(path);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// What the walk found: `path: name ×n (why)`, one per line.
fn breaches(root: &Path) -> Vec<String> {
    let me = this_file().canonicalize().expect("this test's own file");
    let mut all = Vec::new();
    let mut linked = Vec::new();
    files(root, &root.join("crates"), &mut all, &mut linked);
    files(root, &root.join("web"), &mut all, &mut linked);

    let mut found: Vec<String> = linked
        .into_iter()
        .map(|path| format!("{path}: a symlinked directory, which this walk does not read"))
        .collect();
    let rules = refused();
    for path in all {
        if path.canonicalize().is_ok_and(|path| path == me) {
            continue;
        }
        let relative = relative(root, &path);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = Text::of(&bytes);
        for rule in &rules {
            if !rule.only_under.is_empty()
                && !rule
                    .only_under
                    .iter()
                    .any(|under| relative.starts_with(under))
            {
                continue;
            }
            let n = (rule.found)(&text);
            let allowed = rule
                .allowed
                .iter()
                .find(|(allowed, _, _)| *allowed == relative)
                .map_or(0, |(_, times, _)| *times);
            if n > allowed {
                found.push(format!("{relative}: {} ×{n} ({})", rule.name, rule.why));
            }
        }
    }
    found
}

#[test]
fn nothing_under_crates_or_web_starts_resumes_or_drives_claude() {
    let root = root();
    let mut read = Vec::new();
    files(&root, &root.join("crates"), &mut read, &mut Vec::new());
    files(&root, &root.join("web"), &mut read, &mut Vec::new());
    assert!(
        read.len() > 50,
        "the walk found {} files; it is not reading the source",
        read.len()
    );

    let found = breaches(&root);
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
        for (allowed, _, why) in rule.allowed {
            assert!(
                root.join(allowed).is_file(),
                "{allowed} is allowed {} because {why}, and is gone; drop it from the list",
                rule.name
            );
        }
    }
}

/// Each way of writing it is caught, including the six a reviewer planted that the first
/// version of this test passed, and a second process in the one file allowed one.
#[test]
fn every_way_of_writing_it_is_caught() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().canonicalize().unwrap();
    let plant = |path: &str, body: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    };
    let c = CLAUDE;
    let cases = [
        ("crates/server/src/a.rs", format!("use std::process::Command as Cmd;\nfn a() {{ Cmd::new(\"{c}\").args([\"-p\", \"x\"]); }}\n")),
        ("crates/server/src/b.rs", "fn b() { std::process::Command::new (\"x\"); }\n".to_string()),
        ("crates/server/src/c.rs", format!("fn c() {{ home_dir().join(\".{c}\"); }}\n")),
        ("crates/server/src/d.rs", "use std::process::{\n    Stdio,\n    Command,\n};\n".to_string()),
        ("crates/store/src/e.rs", "use tokio::process::Command;\n".to_string()),
        ("web/app/probe.ts", format!("import {{ spawn }} from 'node:child_{}'\nspawn('x', [])\n", "process")),
        ("web/app/home.ts", format!("const at = '~/.{c}'\n")),
        ("web/app/dist/p.ts", format!("const run = '{c} -p'\n")),
        ("web/app/resume.ts", format!("const run = `{c} --resume`\n")),
        ("web/app/bin.ts", format!("const bin = '/usr/local/bin/{c}'\n")),
        ("web/e2e/drive.ts", format!("const how = 'stream-{}'\n", "json")),
        (
            "crates/harvest/tests/fixtures/hidden.rs",
            format!("fn h() {{ let _ = \"{c}\"; }}\n"),
        ),
        ("crates/cli/tests/f.rs", format!("const FLAG: &str = \"{c}  --{}\";\n", "print")),
        (
            "crates/harvest/src/gh.rs",
            "use std::process::{Command, Stdio};\nfn a() { Command::new(&self.program); }\nfn b() { Command::new(\"x\"); }\n".to_string(),
        ),
    ];
    for (path, body) in &cases {
        plant(path, body);
    }
    // Things that are not breaches: a domain, the product's name in prose, a CLI subcommand.
    plant("web/app/fine.ts", &format!("const docs = 'https://docs.{c}.com'\nconst who = 'Your own Claude Code session'\nenum MessageCommand {{}}\n"));
    plant("web/node_modules/x/index.js", &format!("spawn('{c}')\n"));
    plant(
        &format!("{RECORDED}/blob.json"),
        &format!("{{\"text\": \"~/.{c}/\"}}\n"),
    );

    let found = breaches(&root);
    for (path, _) in &cases {
        assert!(
            found
                .iter()
                .any(|line| line.starts_with(&format!("{path}:"))),
            "{path} was not caught; found {found:#?}"
        );
    }
    for path in [
        "web/app/fine.ts",
        "web/node_modules",
        "crates/harvest/tests/fixtures/blob.json",
    ] {
        assert!(
            !found.iter().any(|line| line.starts_with(path)),
            "{path} should not be caught; found {found:#?}"
        );
    }
}

/// A symlinked directory is reported, not silently skipped; a symlinked file is read.
#[cfg(unix)]
#[test]
fn a_symlink_is_read_or_reported() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().canonicalize().unwrap();
    let elsewhere = tempfile::tempdir().expect("another directory");
    let outside = elsewhere.path().join("x.ts");
    std::fs::write(&outside, format!("const how = 'stream-{}'\n", "json")).unwrap();
    std::fs::create_dir_all(root.join("web/app")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("web/app/linked.ts")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.join("web/app/linked-dir")).unwrap();

    let found = breaches(&root);
    assert!(
        found
            .iter()
            .any(|line| line.starts_with("web/app/linked.ts:")),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|line| line.starts_with("web/app/linked-dir:")),
        "{found:#?}"
    );
}
