//! Turning the strings the record is keyed on into their canonical form.
//!
//! The repository and the head commit are **identity**, not description. `reviews` is
//! unique on repository, pull request and round; `rules_for_repo` matches `scope_repo`
//! exactly; `find_review_by_head` compares the head commit — and all of it under
//! SQLite's binary collation, where `DevergeHQ/x` and `devergehq/x` are two
//! repositories.
//!
//! That is not a cosmetic difference. Two spellings of one repository give one pull
//! request two parallel round sequences, split its findings across two reviews that
//! cannot see each other, and hide a repository's own rules from the other spelling.
//! Two spellings of one head commit start a round for a commit already reviewed, and
//! because [`crate::Store::latest_round`] counts rounds per pull request, that round
//! number is spent for good.
//!
//! So normalisation lives in one place, and everything that reads or writes those
//! columns goes through it — a lookup normalised differently from the insert is the same
//! bug wearing a different hat.

/// A repository as `owner/repo`, in the form the record is keyed on.
///
/// Lowercased, because GitHub repository names are not case-sensitive identity:
/// `DevergeHQ/review-desk` and `devergehq/review-desk` are one repository, and the
/// record has to agree.
pub(crate) fn repo(repo: &str) -> String {
    repo.trim().to_ascii_lowercase()
}

/// A head commit, in the form the record is keyed on.
///
/// Lowercased, because a git object name is lowercase hexadecimal — `git rev-parse`
/// never emits anything else, so an uppercase one is a caller that normalised badly
/// rather than a different commit.
///
/// **An abbreviated commit is still a different commit from its full form.** Nothing
/// here can reconcile `c0ffee1` with `c0ffee123…` without asking git, so a caller has to
/// pass the same spelling each time. This normalises what it can and leaves that to the
/// caller, which the README says.
pub(crate) fn head_sha(head_sha: &str) -> String {
    head_sha.trim().to_ascii_lowercase()
}
