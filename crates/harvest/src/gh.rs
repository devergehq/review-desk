//! GitHub by way of the `gh` command.
//!
//! Review Desk handles no tokens. It runs `gh`, as the Locus linters do, so whatever
//! login the developer already has is the login it uses — and when there is none, the
//! message says so rather than reporting an empty pull request.
//!
//! Every call here is a read except one: [`Gh::post_review`], which `review-desk post`
//! uses to put a review the developer approved on the pull request (DEV-1347). **It is the
//! one GitHub write in Review Desk**, and it goes through the same `gh` login as the reads,
//! so Review Desk still stores no GitHub token of its own.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{HarvestError, Result};
use crate::github::{BlobAt, CommitFiles, Comparison, GitHub, PullRequest, ReviewThread};
use crate::wire;

/// A thread's resolved flag exists only in GraphQL; the REST comments endpoint has no
/// such field. 100 is GraphQL's ceiling for one page.
const THREADS_QUERY: &str = "\
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        nodes {
          isResolved
          resolvedBy { login }
          comments(first: 100) {
            nodes { databaseId author { login } body createdAt originalCommit { oid } }
          }
        }
      }
    }
  }
}";

/// One file at one commit, and whether the commit is there at all. DEV-1344.
///
/// `object(expression:)` answers `null` both for a commit GitHub no longer has and for a
/// path the commit does not hold, so the commit is asked for on its own beside it. A blob's
/// `oid` is git's own blob SHA — equal contents, equal id — which is what "did this file
/// change" is answered from. Read per repository rather than from a search, which came
/// back incomplete on 150 of 269 pull requests in the exploration behind this.
const BLOB_QUERY: &str = "\
query($owner: String!, $name: String!, $commit: String!, $file: String!) {
  repository(owner: $owner, name: $name) {
    commit: object(expression: $commit) { oid }
    file: object(expression: $file) { ... on Blob { oid text isBinary isTruncated } }
  }
}";

/// GitHub, read by running `gh`.
#[derive(Debug, Clone)]
pub struct Gh {
    program: OsString,
}

impl Gh {
    /// `gh`, found on the path.
    pub fn new() -> Self {
        Self {
            program: OsString::from("gh"),
        }
    }

    /// `gh` under another name or at another path.
    pub fn with_program(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Post a review on a pull request: `POST repos/<repo>/pulls/<pr>/reviews`, with
    /// `payload` as the request body. Returns the id of the review GitHub created.
    ///
    /// **The one write.** The payload is built by the caller from the record — the body
    /// with its tag already at the end, the threads, the event and `commit_id` — and is sent
    /// as it is, on standard input, so nothing in it passes through an argument list.
    pub fn post_review(&self, repo: &str, pr: i64, payload: &str) -> Result<String> {
        let answer = self.run_with_input(
            [
                "api",
                "--method",
                "POST",
                &format!("repos/{repo}/pulls/{pr}/reviews"),
                "--input",
                "-",
            ],
            Some(payload.as_bytes()),
        )?;
        wire::created_review_id(&answer)
    }

    /// Every review on a pull request, as its id and its body — DEV-1347.
    ///
    /// Read by `review-desk post` before it retries an attempt whose outcome was unknown, to
    /// find out whether that attempt's review is already up.
    pub fn review_bodies(&self, repo: &str, pr: i64) -> Result<Vec<(String, String)>> {
        let answer = self.run([
            "api",
            "--paginate",
            &format!("repos/{repo}/pulls/{pr}/reviews"),
            "--jq",
            ".[] | {id: .id, body: .body}",
        ])?;
        answer
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(wire::review_body)
            .collect()
    }

    /// Run `gh` with these arguments and return its standard output.
    fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run_with_input(args, None)
    }

    /// Run `gh`, writing `input` to its standard input when there is any.
    fn run_with_input<I, S>(&self, args: I, input: Option<&[u8]>) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect();

        let spawn_error = |error: std::io::Error| match error.kind() {
            // Told apart deliberately: "gh is not installed" and "gh is not logged
            // in" are both exit 2, but a developer fixes them differently.
            std::io::ErrorKind::NotFound => HarvestError::GhMissing,
            _ => HarvestError::GhFailed {
                args: describe(&args),
                status: error.kind().to_string(),
                stderr: error.to_string(),
            },
        };

        let mut child = Command::new(&self.program)
            .args(&args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            // Dropped at the end of this block, which closes it and lets `gh` read to EOF.
            stdin.write_all(input).map_err(spawn_error)?;
        }
        let output = child.wait_with_output().map_err(spawn_error)?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if is_logged_out(&stderr) {
            return Err(HarvestError::GhNotAuthenticated);
        }

        Err(HarvestError::GhFailed {
            args: describe(&args),
            status: output
                .status
                .code()
                .map_or_else(|| "killed".to_string(), |code| format!("exit {code}")),
            stderr: stderr.trim().to_string(),
        })
    }
}

impl Default for Gh {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHub for Gh {
    fn pull_request(&self, repo: &str, pr: i64) -> Result<PullRequest> {
        wire::pull_request(&self.run(["api", &format!("repos/{repo}/pulls/{pr}")])?)
    }

    fn review_threads(&self, repo: &str, pr: i64) -> Result<Vec<ReviewThread>> {
        let (owner, name) = split_repo(repo);
        wire::review_threads(&self.run([
            "api",
            "graphql",
            "-f",
            &format!("query={THREADS_QUERY}"),
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
            "-F",
            &format!("number={pr}"),
        ])?)
    }

    fn compare(&self, repo: &str, base: &str, head: &str) -> Result<Comparison> {
        wire::comparison(&self.run(["api", &format!("repos/{repo}/compare/{base}...{head}")])?)
    }

    fn commit(&self, repo: &str, sha: &str) -> Result<CommitFiles> {
        wire::commit(&self.run(["api", &format!("repos/{repo}/commits/{sha}")])?)
    }

    fn blob(&self, repo: &str, commit: &str, path: &str) -> Result<BlobAt> {
        let (owner, name) = split_repo(repo);
        wire::blob(&self.run([
            "api",
            "graphql",
            "-f",
            &format!("query={BLOB_QUERY}"),
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
            // `-f`, not `-F`: these are strings whatever they look like, and `-F` would
            // read an all-digit abbreviated sha as a number.
            "-f",
            &format!("commit={commit}"),
            "-f",
            &format!("file={commit}:{path}"),
        ])?)
    }
}

/// `owner/repo` split for GraphQL, which takes the two separately. The store already
/// refuses a repository that is not `owner/repo`, so this needs no second opinion.
fn split_repo(repo: &str) -> (&str, &str) {
    repo.split_once('/').unwrap_or((repo, ""))
}

/// Whether this is `gh` saying nobody is logged in.
fn is_logged_out(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("gh auth login")
        || stderr.contains("not logged in")
        || stderr.contains("authentication required")
        || stderr.contains("requires authentication")
}

/// The command, for a message. The arguments carry no secrets — the token lives in
/// `gh`'s own keyring and is never passed through here.
fn describe(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_gh_is_missing_not_broken_in_some_other_way() {
        let gh = Gh::with_program("review-desk-no-such-command");
        let error = gh.pull_request("devergehq/locus", 64).expect_err("no gh");

        assert!(matches!(error, HarvestError::GhMissing));
        assert!(!error.is_refusal(), "an absent gh is exit 2, not exit 1");
        assert_eq!(error.code(), "gh_missing");
    }

    #[test]
    fn ghs_own_words_for_being_logged_out_are_recognised() {
        assert!(is_logged_out(
            "To get started with GitHub CLI, please run: gh auth login"
        ));
        assert!(is_logged_out("error: not logged in to any hosts"));
        assert!(is_logged_out("HTTP 401: Requires authentication"));
        assert!(!is_logged_out("HTTP 404: Not Found"));
    }

    #[test]
    fn a_4xx_is_certainly_not_sent_and_a_5xx_or_a_timeout_may_have_been() {
        let failed = |stderr: &str| HarvestError::GhFailed {
            args: String::new(),
            status: "exit 1".to_string(),
            stderr: stderr.to_string(),
        };
        assert_eq!(
            failed("gh: Validation Failed (HTTP 422)").http_status(),
            Some(422)
        );
        assert_eq!(failed("HTTP 404: Not Found").http_status(), Some(404));
        assert!(failed("gh: Validation Failed (HTTP 422)").certainly_not_sent());
        assert!(!failed("gh: Bad Gateway (HTTP 502)").certainly_not_sent());
        assert!(!failed("HTTP 408: Request Timeout").certainly_not_sent());
        assert!(!failed("Post \"https://api.github.com/...\": i/o timeout").certainly_not_sent());
        assert!(HarvestError::GhNotAuthenticated.certainly_not_sent());
    }

    #[test]
    fn a_repository_splits_for_graphql() {
        assert_eq!(split_repo("devergehq/locus"), ("devergehq", "locus"));
    }
}
