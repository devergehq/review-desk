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

/// A commit a record merely points at — the one that fixed a finding, the one that
/// introduced it, one of the commits between two rounds.
///
/// The same canonical form as [`head_sha`], because they name the same kind of thing and a
/// round's commit list is matched against a review's head. A blank one is no commit rather
/// than a commit named by the empty string, which is what a caller passing `--commit ""`
/// would otherwise write.
pub(crate) fn optional_commit(sha: Option<&str>) -> Option<String> {
    sha.map(head_sha).filter(|sha| !sha.is_empty())
}

/// A free-text fact a caller may or may not have filled in: trimmed, and blank is absent.
///
/// **Absent, not empty**, which is [`optional_commit`]'s rule for the same reason and is
/// what the record means by optional: a field left out is left as it was, so a `--ticket
/// "$TICKET"` from a shell whose variable was unset has to read as "say nothing about the
/// ticket" rather than as "the ticket is the empty string". `COALESCE(?n, column)` is only
/// told to leave a column alone by a `NULL`, so an empty string reaching it *replaces* what
/// was there — which is how `--ticket ""` came to wipe a recorded `ABC-231`.
///
/// **Here rather than in each caller, because two copies of this rule is the bug it
/// prevents.** A review's ticket id and its address are checked against each other, and
/// for one commit the address was normalised and the id was not: the pair `--ticket ""
/// --ticket-url https://…` then passed a check written to refuse exactly it. One rule,
/// read by everything that writes one of these columns and by
/// [`crate::ReviewFacts::is_empty`], which has to agree with them about what counts as a
/// fact.
pub(crate) fn optional_fact(given: Option<&str>) -> Option<String> {
    given
        .map(str::trim)
        .filter(|held| !held.is_empty())
        .map(str::to_string)
}

/// Whether this is a repository name: the contract's `RepoName`, and not a relative path.
///
/// `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`, which is what GitHub allows and what the contract
/// declares — **and not `.` or `..` as either half**, because the pattern alone permits
/// them: `.` is a legal character in a repository name and the pattern says nothing about
/// what a segment means.
///
/// **That exclusion is the load-bearing half, and it is load-bearing in two places.** Every
/// store query binds its parameters, so a dot segment is inert in SQL — but this value is
/// read back out and put into two paths by other crates. `harvest` formats
/// `repos/{repo}/pulls/{pr}` into a `gh api` call, where `repos/../../pulls/1` is
/// `api.github.com/pulls/1`; and the dashboard builds `https://github.com/{repo}/pull/{pr}`,
/// which a browser resolves `https://github.com/../../pull/1` to `github.com/pull/1`. In
/// both the value goes somewhere other than the repository anybody named, and in the second
/// the link's text and its destination disagree — which is the defect DEV-936 exists to
/// remove.
///
/// **One copy, here, because it was two.** `wire::is_repo_name` had this rule and its
/// reason; DEV-936 then wrote a second, looser one in the store's `validate` — missing
/// exactly the dot guard — and the store is the gate `review open` passes through, since the
/// command line reaches `Store` without going near the server. `wire` delegates to this.
pub fn is_repo_name(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    if matches!(owner, "." | "..") || matches!(name, "." | "..") {
        return false;
    }
    let usable = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    };
    usable(owner) && usable(name)
}

/// Whether this is a git object name and not a word.
///
/// **Checked, and not merely trimmed, because these columns are typed rather than
/// resolved.** A review's `head_sha` comes from git by way of `review open`; the commit
/// that fixed a finding comes from `--commit`, where `HEAD` or a four-character
/// abbreviation is a plausible thing for a person or an agent to type. The API serves these
/// as `^[0-9a-f]{7,40}$`, so an unchecked value is a response that fails its own contract —
/// found by reviewing DEV-873.
///
/// Seven is git's own shortest abbreviation and forty is a full object name.
pub(crate) fn is_commit_sha(sha: &str) -> bool {
    (7..=40).contains(&sha.len()) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}
