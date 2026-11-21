//! Whether the code a finding points at changed after the review was posted. DEV-1344.
//!
//! **Answered from file contents, not from a comparison of the two commits.** On a pull
//! request that was rebased or force-pushed, `compare(posted...head)` is `diverged` and
//! carries upstream work. With base-branch merges in range it can be `ahead` and still
//! carry it, and past 300 files GitHub cuts the list short. Reading every file in it as the
//! author's change credited authors with upstream work on 79 of 268 reviewed pull requests.
//!
//! | Question | Answered by | A comparison is trusted only when |
//! | -- | -- | -- |
//! | Which files are the pull request's own? | `compare(base...head)`, three dots | under 300 files, and its line total equals GitHub's own count for the pull request |
//! | Did a finding's **file** change? | git blob ids at the posted commit and the head, for the pull request's own files only | never: contents only. The posted→head comparison, when it is `identical` or `ahead` with no merges, under 250 commits and under 300 files, only names *which* commit did it |
//! | Did a finding's **lines** change? | the 7 lines around the finding at the posted commit, searched for in the later file, whitespace-trimmed | never: contents only |
//!
//! | Situation | What is answered |
//! | -- | -- |
//! | Both readable | changed, or unchanged |
//! | The posted commit is gone | unknown, `history_rewritten` |
//! | The file is gone at the head | changed: `removed`, or `renamed` where the pull request renamed it |
//! | The file is not among the pull request's own | unchanged *by the pull request*, which is not "the author acted" |
//! | A GitHub read failed | unknown, `rate_limited`, `auth_failed`, `read_failed`, … |
//!
//! Every answer says how it was reached — `method`, `checks_run`, `checks_passed` and
//! [`METHOD_VERSION`] — so a figure can be read with the method that produced it, and a
//! later method can be told from this one.
//!
//! **"After" is decided by ancestry, never by committer date.** A head the posted commit is
//! *ahead of* is not after it, whatever its dates say: that answer is unknown, `not_after`.

use std::collections::{BTreeMap, HashMap};

use crate::error::Result;
use crate::github::{BlobAt, Comparison, GitHub, PullRequest};

/// Which version of this method produced an answer. Raise it when an answer to the same
/// inputs could come out differently.
pub const METHOD_VERSION: i64 = 1;

/// GitHub lists at most this many files in a comparison; a list this long may be cut short.
const FILE_CAP: usize = 300;

/// GitHub lists at most this many commits in a comparison.
const COMMIT_CAP: i64 = 250;

/// Lines either side of a finding's own line: seven in all.
const WINDOW: i64 = 3;

/// Why an answer is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// The commit the review was posted on is no longer on GitHub: a force-push took it.
    HistoryRewritten,
    /// Which files are the pull request's own could not be confirmed, so no file of it can
    /// be credited to the author.
    OwnFilesUnverified,
    /// The pull request's head is behind the posted commit, so nothing on it came after.
    NotAfter,
    /// The posted commit holds no file at the path the finding names.
    NotInPostedCommit,
    /// One GitHub read failed — rate-limited, refused, or unreadable. It says nothing
    /// lasting about the code, so it is reported and **not stored**: the next harvest asks
    /// again, and a stored "unknown" would outlive the answer it gets.
    Read(&'static str),
}

impl Unknown {
    /// The word the record stores in a detail's `reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            Unknown::HistoryRewritten => "history_rewritten",
            Unknown::OwnFilesUnverified => "own_files_unverified",
            Unknown::NotAfter => "not_after",
            Unknown::NotInPostedCommit => "not_in_posted_commit",
            Unknown::Read(reason) => reason,
        }
    }

    /// Whether this is a fact about the history, worth recording, rather than one read
    /// that may succeed next time.
    pub fn is_lasting(self) -> bool {
        !matches!(self, Unknown::Read(_))
    }
}

/// What happened to a finding's file, and how that was decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub change: Change,
    /// How the answer was reached: `blob_sha`, or `blob_sha+content_window` where the
    /// finding's own lines were checked as well.
    pub method: &'static str,
    /// How many of the method's preconditions were checked, and how many held.
    pub checks_run: u32,
    pub checks_passed: u32,
}

/// The answer itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The pull request does not change this file, so whatever changed it was not the
    /// author's work on this pull request. Not "the author acted".
    NotThePullRequests,
    /// The file is what it was when the review was posted.
    Unchanged,
    /// The file changed.
    Changed {
        /// `modified`, `removed`, or `renamed`.
        file_status: &'static str,
        /// Whether the finding's own lines changed: `None` where that could not be asked —
        /// no line, or a file GitHub sent no text for.
        lines_changed: Option<bool>,
    },
    Unknown(Unknown),
}

/// The pull request's own files, as read from its base.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnFiles {
    /// Each path the pull request changes, with the path it was renamed from where it was.
    /// A finding on the old name of a renamed file is on one of the pull request's files.
    paths: std::result::Result<BTreeMap<String, Option<String>>, Unknown>,
    checks_run: u32,
    checks_passed: u32,
}

impl OwnFiles {
    /// Whether this path is one the pull request changes, and if it renamed it away.
    fn holds(&self, path: &str) -> Option<bool> {
        let paths = self.paths.as_ref().ok()?;
        if paths.contains_key(path) {
            return Some(true);
        }
        Some(paths.values().any(|from| from.as_deref() == Some(path)))
    }

    fn renamed_away(&self, path: &str) -> bool {
        self.paths
            .as_ref()
            .is_ok_and(|paths| paths.values().any(|from| from.as_deref() == Some(path)))
    }
}

/// The posted→head comparison, where GitHub could give one.
#[derive(Debug, Clone)]
struct Range {
    comparison: Option<Comparison>,
    /// Whether the comparison passed every guard, and so may name the commit that changed
    /// a file. Never whether a file changed: that is always answered from contents.
    trusted: bool,
}

/// Answers for one pull request, reading each thing from GitHub at most once.
pub struct Checker<'a> {
    github: &'a dyn GitHub,
    repo: &'a str,
    head: &'a str,
    own: Option<OwnFiles>,
    expected_lines: Option<i64>,
    base: Option<&'a str>,
    ranges: HashMap<String, Range>,
    blobs: HashMap<(String, String), BlobAt>,
}

impl std::fmt::Debug for Checker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checker")
            .field("repo", &self.repo)
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

impl<'a> Checker<'a> {
    /// A checker for this pull request as GitHub has it now.
    ///
    /// **GitHub's own count of the pull request's changed lines is the cross-check**, not
    /// the figure the record took when the review started. That one is the count at the
    /// reviewed head; the own-files comparison is read at the current head, and any push
    /// since would make the two differ for a reason that says nothing about truncation.
    pub fn new(github: &'a dyn GitHub, repo: &'a str, pull_request: &'a PullRequest) -> Self {
        Self {
            github,
            repo,
            head: pull_request.head_sha.as_str(),
            own: None,
            expected_lines: pull_request.changed_lines,
            base: pull_request.base_sha.as_deref(),
            ranges: HashMap::new(),
            blobs: HashMap::new(),
        }
    }

    /// Did the file at `path` change between `posted` and the head, and its lines with it?
    pub fn answer(&mut self, posted: &str, path: &str, line: Option<i64>) -> Result<Answer> {
        let own = self.own_files()?.clone();
        let mut run = own.checks_run;
        let mut passed = own.checks_passed;
        let unknown = |why: Unknown, run: u32, passed: u32| Answer {
            change: Change::Unknown(why),
            method: "blob_sha",
            checks_run: run,
            checks_passed: passed,
        };

        if let Err(why) = &own.paths {
            return Ok(unknown(*why, run, passed));
        }
        if own.holds(path) == Some(false) {
            return Ok(Answer {
                change: Change::NotThePullRequests,
                method: "own_files",
                checks_run: run,
                checks_passed: passed,
            });
        }

        // Ancestry, from the comparison's status rather than from any commit's date.
        // A comparison GitHub would not give — the posted commit gone, or a read that failed
        // — leaves ancestry unestablished: the check ran and did not pass. Only `behind` is
        // refused outright; contents answer the rest.
        run += 1;
        let status = self
            .range(posted)?
            .comparison
            .as_ref()
            .map(|c| c.status.clone());
        match status.as_deref() {
            Some("behind") => return Ok(unknown(Unknown::NotAfter, run, passed)),
            Some(_) => passed += 1,
            None => {}
        }

        run += 1;
        let (was, was_text) = match self.blob(posted, path)? {
            Ok(BlobAt::Blob { oid, text }) => (oid, text),
            Ok(BlobAt::CommitMissing) => {
                return Ok(unknown(Unknown::HistoryRewritten, run, passed))
            }
            Ok(BlobAt::PathMissing) => return Ok(unknown(Unknown::NotInPostedCommit, run, passed)),
            Err(why) => return Ok(unknown(why, run, passed)),
        };
        passed += 1;

        run += 1;
        let head = self.head.to_string();
        let after = match self.blob(&head, path)? {
            // The head is the pull request's own head as GitHub reported it a moment ago;
            // not finding it is a failed read, not a rewritten history.
            Ok(BlobAt::CommitMissing) => {
                return Ok(unknown(Unknown::Read("read_failed"), run, passed))
            }
            Ok(found) => found,
            Err(why) => return Ok(unknown(why, run, passed)),
        };
        passed += 1;

        let BlobAt::Blob {
            oid: is,
            text: is_text,
        } = after
        else {
            // Gone at the head. Its lines went with it.
            return Ok(Answer {
                change: Change::Changed {
                    file_status: if own.renamed_away(path) {
                        "renamed"
                    } else {
                        "removed"
                    },
                    lines_changed: Some(true),
                },
                method: "blob_sha",
                checks_run: run,
                checks_passed: passed,
            });
        };

        if was == is {
            return Ok(Answer {
                change: Change::Unchanged,
                method: "blob_sha",
                checks_run: run,
                checks_passed: passed,
            });
        }

        // The file changed. Whether the finding's own lines did is asked of its contents.
        let lines = match (line, was_text.as_deref(), is_text.as_deref()) {
            (Some(line), Some(was_text), Some(is_text)) => {
                run += 1;
                window(was_text, line).map(|window| {
                    passed += 1;
                    !still_there(&window, is_text)
                })
            }
            _ => None,
        };

        Ok(Answer {
            change: Change::Changed {
                file_status: "modified",
                lines_changed: lines,
            },
            method: if lines.is_some() {
                "blob_sha+content_window"
            } else {
                "blob_sha"
            },
            checks_run: run,
            checks_passed: passed,
        })
    }

    /// The posted→head comparison, when it passed every guard — the only comparison that
    /// may say which commit changed a file. `None` otherwise.
    pub fn trusted_range(&mut self, posted: &str) -> Result<Option<&Comparison>> {
        let range = self.range(posted)?;
        Ok(if range.trusted {
            range.comparison.as_ref()
        } else {
            None
        })
    }

    /// The pull request's own files, read once.
    fn own_files(&mut self) -> Result<&OwnFiles> {
        if self.own.is_none() {
            let own = self.read_own_files()?;
            self.own = Some(own);
        }
        Ok(self.own.as_ref().expect("just read"))
    }

    fn read_own_files(&self) -> Result<OwnFiles> {
        let refused = |why: Unknown, run: u32, passed: u32| OwnFiles {
            paths: Err(why),
            checks_run: run,
            checks_passed: passed,
        };

        // 1: a base to read from.
        let Some(base) = self.base else {
            return Ok(refused(Unknown::OwnFilesUnverified, 1, 0));
        };

        // 2: the comparison read.
        let comparison = match self.github.compare(self.repo, base, self.head) {
            Ok(comparison) => comparison,
            Err(error) if error.is_not_found() => {
                return Ok(refused(Unknown::OwnFilesUnverified, 2, 1))
            }
            Err(error) => match error.unknown_reason() {
                Some(reason) => return Ok(refused(Unknown::Read(reason), 2, 1)),
                None => return Err(error),
            },
        };

        // 3: short enough that GitHub listed every file.
        if comparison.files.len() >= FILE_CAP {
            return Ok(refused(Unknown::OwnFilesUnverified, 3, 2));
        }

        // 4: its lines add up to GitHub's own count for the pull request. A list that lost
        // a file, or a range that is not the pull request's, does not.
        let total: Option<i64> = comparison.files.iter().map(|file| file.changes).sum();
        if total.is_none() || total != self.expected_lines {
            return Ok(refused(Unknown::OwnFilesUnverified, 4, 3));
        }

        Ok(OwnFiles {
            paths: Ok(comparison
                .files
                .into_iter()
                .map(|file| (file.path, file.previous_path))
                .collect()),
            checks_run: 4,
            checks_passed: 4,
        })
    }

    /// The posted→head comparison, read once per posted commit, and whether it may be
    /// trusted to name commits.
    ///
    /// A comparison GitHub will not give — the posted commit gone, or a read that failed —
    /// is no comparison rather than a failed run: nothing here depends on it but which
    /// commit gets the credit, and the head is credited instead.
    fn range(&mut self, posted: &str) -> Result<&Range> {
        if !self.ranges.contains_key(posted) {
            let comparison = match self.github.compare(self.repo, posted, self.head) {
                Ok(comparison) => Some(comparison),
                Err(error) if error.is_not_found() || error.unknown_reason().is_some() => None,
                Err(error) => return Err(error),
            };
            let trusted = comparison.as_ref().is_some_and(trustworthy);
            self.ranges.insert(
                posted.to_string(),
                Range {
                    comparison,
                    trusted,
                },
            );
        }
        Ok(self.ranges.get(posted).expect("just read"))
    }

    /// One file at one commit, read once. A read that failed is the reason it is unknown.
    fn blob(&mut self, commit: &str, path: &str) -> Result<std::result::Result<BlobAt, Unknown>> {
        let key = (commit.to_string(), path.to_string());
        if let Some(held) = self.blobs.get(&key) {
            return Ok(Ok(held.clone()));
        }
        match self.github.blob(self.repo, commit, path) {
            Ok(found) => {
                self.blobs.insert(key, found.clone());
                Ok(Ok(found))
            }
            Err(error) => match error.unknown_reason() {
                Some(reason) => Ok(Err(Unknown::Read(reason))),
                None => Err(error),
            },
        }
    }
}

/// Whether a posted→head comparison is safe to read commits from.
///
/// `identical` or `ahead` — the head descends from the posted commit — with no merge in
/// range, fewer than 250 commits and fewer than 300 files, so nothing upstream rode in and
/// GitHub left nothing out.
fn trustworthy(comparison: &Comparison) -> bool {
    let listed = comparison.commits.len() as i64;
    matches!(comparison.status.as_str(), "identical" | "ahead")
        && !comparison.commits.iter().any(|commit| commit.is_merge)
        && listed < COMMIT_CAP
        && comparison.total_commits.is_none_or(|total| total == listed)
        && comparison.files.len() < FILE_CAP
}

/// The seven lines around `line`, trimmed, or `None` where the file has no such line.
fn window(text: &str, line: i64) -> Option<Vec<&str>> {
    let lines: Vec<&str> = text.lines().collect();
    let count = lines.len() as i64;
    if line < 1 || line > count {
        return None;
    }
    let first = (line - WINDOW).max(1);
    let last = (line + WINDOW).min(count);
    Some(
        lines[(first - 1) as usize..last as usize]
            .iter()
            .map(|line| line.trim())
            .collect(),
    )
}

/// Whether those lines still stand together, in order, somewhere in the later file.
///
/// Anywhere, not at the same line number: lines inserted above a finding move it without
/// changing it.
fn still_there(window: &[&str], later: &str) -> bool {
    let later: Vec<&str> = later.lines().map(str::trim).collect();
    later.windows(window.len()).any(|run| run == window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_seven_lines_and_shorter_at_the_edges() {
        let text = (1..=20)
            .map(|n| format!("  line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(window(&text, 10).unwrap().len(), 7);
        assert_eq!(window(&text, 10).unwrap()[0], "line 7");
        assert_eq!(window(&text, 1).unwrap().len(), 4);
        assert_eq!(window(&text, 20).unwrap().len(), 4);
        assert!(window(&text, 21).is_none());
        assert!(window(&text, 0).is_none());
    }

    #[test]
    fn lines_that_moved_are_still_there_and_lines_that_changed_are_not() {
        let was = "a\nb\nc\nd\ne\nf\ng\nh";
        let moved = "new\nnew\na\nb\n    c\nd\ne\nf\ng\nh";
        let changed = "a\nb\nc\nD\ne\nf\ng\nh";

        let around_d = window(was, 4).unwrap();
        assert!(
            still_there(&around_d, moved),
            "inserted above and reindented"
        );
        assert!(!still_there(&around_d, changed));
    }

    #[test]
    fn only_a_fact_about_the_history_is_lasting() {
        assert!(Unknown::HistoryRewritten.is_lasting());
        assert!(Unknown::OwnFilesUnverified.is_lasting());
        assert!(!Unknown::Read("rate_limited").is_lasting());
        assert_eq!(Unknown::Read("rate_limited").as_str(), "rate_limited");
    }
}
