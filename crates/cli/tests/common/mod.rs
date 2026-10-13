//! Test scaffolding for the binary.
//!
//! Every test gets its own database in a `TempDir` and points `REVIEW_DESK_DB` at it, so
//! no test can read or write `~/.review-desk/review-desk.db`.
//!
//! Each integration test file compiles this module into its own binary, so a helper one
//! of them does not use is dead code there. That is what the allow is for.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use review_desk_store::Store;
use serde_json::Value;
use tempfile::TempDir;

/// A Review Desk with a database of its own.
#[derive(Debug)]
pub struct Desk {
    dir: TempDir,
}

impl Desk {
    pub fn new() -> Self {
        Desk {
            dir: TempDir::new().expect("temporary directory"),
        }
    }

    /// Where this desk's database is.
    pub fn db(&self) -> PathBuf {
        self.dir.path().join("review-desk.db")
    }

    /// This desk's directory, for a test that has to write a file to hand to a command.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The binary, pointed at this desk's database.
    pub fn command(&self) -> Command {
        let mut command = Command::cargo_bin("review-desk").expect("the review-desk binary");
        command.env("REVIEW_DESK_DB", self.db());
        command
    }

    /// Run a command that must succeed, and hand back its standard output, trimmed.
    pub fn run(&self, arguments: &[&str]) -> String {
        let output = self.command().args(arguments).assert().success();
        String::from_utf8(output.get_output().stdout.clone())
            .expect("output is text")
            .trim()
            .to_string()
    }

    /// Run a command that must succeed with `--json`, and hand back its one document.
    pub fn json(&self, arguments: &[&str]) -> Value {
        let text = self.run(&[arguments, &["--json"]].concat());
        serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("{arguments:?} did not print one JSON document: {error}\n{text}")
        })
    }

    /// Run a command that must be refused with `--json`, and hand back its one document.
    pub fn refused_json(&self, arguments: &[&str]) -> Value {
        let output = self
            .command()
            .args(arguments)
            .arg("--json")
            .assert()
            .code(1);
        let text = String::from_utf8(output.get_output().stdout.clone()).expect("output is text");
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("a refusal must still be one document: {error}\n{text}"))
    }

    /// Write a file in this desk's directory and hand back its path, for a command that
    /// takes a document.
    pub fn write_file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, contents).expect("write the document");
        path
    }

    /// A store on this desk's database, for seeding what no command creates — rules,
    /// which DEV-851 deliberately leaves out of the command line.
    pub fn store(&self) -> Store {
        Store::open(self.db()).expect("open the store")
    }
}

/// Open a review and hand back its id, the way a caller would: by reading the one line
/// `review open` prints.
pub fn open_review(desk: &Desk, repo: &str, pr: &str, head: &str) -> String {
    desk.run(&["review", "open", "--repo", repo, "--pr", pr, "--head", head])
}

/// Record a review as posted, so that a new head commit on its pull request opens the next
/// round rather than moving this round's head.
///
/// **DEV-962: a round is a posted review.** `review open` moves an unposted round to a new
/// commit — Patrick's queue held one pull request twice, round 1 and round 2, over a round 1
/// he had never posted — so a test that wants a second round has to have posted the first.
/// `review link --github-review` is the one way the command line says a review was posted,
/// and Review Desk posts nothing itself.
pub fn post_round(desk: &Desk, review: &str) {
    desk.command()
        .args([
            "review",
            "link",
            "--review",
            review,
            "--github-review",
            &format!("gh-review-{review}"),
        ])
        .assert()
        .success();
}

/// Make a review count as started: somebody has begun on it.
///
/// **One finding is the cheapest honest way to say a session got that far**, and it is what
/// `ReviewWork` asks for — a brief, a finding or a draft. Since DEV-962 this no longer
/// decides whether a new commit opens a round; [`post_round`] does. What it is still for is
/// a test about work a delete would lose, or about `review find`'s `started`.
pub fn start_review(desk: &Desk, review: &str) {
    desk.command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(
            serde_json::json!({
                "lens": "correctness",
                "severity": "nit",
                "claim": "a session reviewed this round",
            })
            .to_string(),
        )
        .assert()
        .success();
}
