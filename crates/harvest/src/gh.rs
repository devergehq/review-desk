//! GitHub by way of the `gh` command.
//!
//! Review Desk handles no tokens. It runs `gh`, as the Locus linters do, so whatever
//! login the developer already has is the login it uses — and when there is none, the
//! message says so rather than reporting an empty pull request.
//!
//! Every call here is a read. **Nothing in this module writes to GitHub.**

use std::ffi::{OsStr, OsString};
use std::process::Command;

use crate::error::{HarvestError, Result};
use crate::github::{CommitFiles, Comparison, GitHub, PullRequest, ReviewThread};
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
            nodes { databaseId author { login } body createdAt }
          }
        }
      }
    }
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

    /// Run `gh` with these arguments and return its standard output.
    fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect();

        let output =
            Command::new(&self.program)
                .args(&args)
                .output()
                .map_err(|error| match error.kind() {
                    // Told apart deliberately: "gh is not installed" and "gh is not logged
                    // in" are both exit 2, but a developer fixes them differently.
                    std::io::ErrorKind::NotFound => HarvestError::GhMissing,
                    _ => HarvestError::GhFailed {
                        args: describe(&args),
                        status: error.kind().to_string(),
                        stderr: error.to_string(),
                    },
                })?;

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
    fn a_repository_splits_for_graphql() {
        assert_eq!(split_repo("devergehq/locus"), ("devergehq", "locus"));
    }
}
