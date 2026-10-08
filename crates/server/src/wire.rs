//! The shapes on the wire, and the two conversions the contract asks the server to make.
//!
//! **Times.** The store holds `Timestamp = i64`, seconds since the Unix epoch; the
//! contract is RFC 3339 in UTC with a `Z` offset. `contract/README.md` names this as the
//! one mismatch that will look like a bug the first time somebody serialises a store
//! record straight out, so the conversion lives here and no handler formats a time
//! itself.
//!
//! **Display keys.** `F1` and `R-09` are derived, not stored — `F` plus the finding's
//! `seq`, and `R-` plus the rule's id zero-padded to two digits. The contract has the
//! server derive them so the dashboard never computes an identifier.
//!
//! Bodies are built as `serde_json::Value` with every field written out in one place.
//! The contract sets `additionalProperties: false` on every object, so a mistyped field
//! name is a validation failure in the conformance test rather than a field the
//! dashboard silently never sees.

use review_desk_store::{Brief, Finding, Review, Timestamp};
use serde_json::{json, Value};

use crate::error::ApiError;

/// A time on the wire: RFC 3339, UTC, `Z` offset.
pub fn rfc3339(seconds: Timestamp) -> String {
    // Floor division, so a time before 1970 does not round towards zero and land on the
    // wrong day. Review Desk will not hold one, but a clock set wrong can produce one
    // and a date that is silently off by a day is a bad way to find out.
    let days = seconds.div_euclid(86_400);
    let within = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (within / 3600, (within % 3600) / 60, within % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// An optional time on the wire: the string, or JSON null.
pub fn rfc3339_or_null(seconds: Option<Timestamp>) -> Value {
    seconds.map_or(Value::Null, |seconds| json!(rfc3339(seconds)))
}

/// The civil date for a count of days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`, which is exact for the whole range of `i64` days
/// and needs no table. Written out rather than taken from `chrono` or `time`: one date
/// conversion does not justify a dependency in a crate whose whole argument for its HTTP
/// stack is dependency weight.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// A finding's display key: `F` plus its sequence number.
pub fn finding_key(seq: i64) -> String {
    format!("F{seq}")
}

/// The contract's `Review`.
///
/// `title`, `ticket_id` and `changed_lines` are null because the store on `master` has
/// no column for any of them — S5, S6 and S7. They are declared nullable, and null is
/// the contract's own word for a fact that is absent rather than measured, so nothing
/// here is invented. A caller that *supplies* one of the three is refused instead of
/// having it quietly dropped; see `handlers::review`.
pub fn review(review: &Review) -> Value {
    json!({
        "id": review.id,
        "repo": review.repo,
        "pr": review.pr,
        "head_sha": review.head_sha,
        "round": review.round,
        "title": Value::Null,
        "ticket_id": Value::Null,
        "changed_lines": Value::Null,
        "session_id": review.session_id,
        "ledger_id": review.ledger_id,
        "github_review_id": review.github_review_id,
        "created_at": rfc3339(review.created_at),
    })
}

/// The contract's `Brief`.
///
/// `options` is always empty, and that is a decision rather than an omission. The store
/// holds the option map as one opaque `String`; `BriefOption` is four structured fields,
/// and `Brief` has no field for the blob. The contract says as much at
/// `BriefOption.description` and replaces it with the `brief_option` table in S3. Empty
/// reads as "no structured options are on record", which is true: there are no rows,
/// because there is no table.
///
/// `provenance` is null for the same reason — S10 has no column.
pub fn brief(brief: &Brief) -> Value {
    json!({
        "review_id": brief.review_id,
        "problem": brief.problem,
        "diagram": brief.diagram,
        "options": Value::Array(Vec::new()),
        "approach_verdict": brief.approach_verdict,
        "provenance": Value::Null,
        "confirmed": brief.is_confirmed(),
        "confirmed_at": rfc3339_or_null(brief.confirmed_at),
        "updated_at": rfc3339(brief.updated_at),
    })
}

/// The contract's `Finding`.
///
/// `rule_id` and `rule_key` are null: the store records rule-to-finding and not
/// finding-to-rule, which is S11 — the citation the whole feedback loop runs through.
pub fn finding(finding: &Finding) -> Value {
    json!({
        "review_id": finding.review_id,
        "seq": finding.seq,
        "key": finding_key(finding.seq),
        "lens": finding.lens,
        "severity": finding.severity.as_str(),
        "file": finding.file,
        "line": finding.line,
        "claim": finding.claim,
        "evidence": finding.evidence,
        "disposition": finding.disposition.as_str(),
        "disposition_reason": finding.disposition_reason,
        "rule_id": Value::Null,
        "rule_key": Value::Null,
        "created_at": rfc3339(finding.created_at),
    })
}

/// A repository and pull request number read out of a pasted link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrLink {
    pub repo: String,
    pub pr: i64,
}

/// What a pull request link has to look like, for an error message that says so.
const PR_LINK_SHAPE: &str = "a pull request link such as https://github.com/acme/portal/pull/412";

/// Read `owner/repo` and the number out of a pull request link.
///
/// Checked here, in full, **before anything reaches for GitHub**: DEV-861 requires that a
/// malformed link is refused with no GitHub read, so that a machine with no `gh` reports
/// the real problem rather than reporting `gh` missing.
///
/// The shape is matched from the host forwards — `HOST/owner/repo/pull/NUMBER` — rather
/// than by searching for a `pull` segment. Searching looks more forgiving and is wrong:
/// `https://github.com/portal/pull/412` has a `pull` segment with two segments in front
/// of it, and reading those two as the repository makes the host part of the repository
/// name. The repository is then `github.com/portal`, which matches the contract's
/// `RepoName` pattern, so nothing downstream would have objected.
pub fn parse_pr_link(url: &str) -> Result<PrLink, ApiError> {
    let refused = |detail: &str| {
        ApiError::invalid_value("pr_url", format!("{detail}: {url:?}"), PR_LINK_SHAPE)
    };

    // The fragment and query say nothing about which pull request this is, and GitHub
    // adds both — `#discussion_r1`, `?diff=split`.
    let trimmed = url
        .split(['#', '?'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');

    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| refused("the link has no http:// or https:// scheme"))?;
    if !matches!(scheme, "http" | "https") {
        return Err(refused("the link's scheme is not http or https"));
    }

    let segments: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();

    // A path with a `.` or `..` segment means something different once it is normalised:
    // `.../acme/portal/pull/412/../../evil/pull/9` is `acme/evil/pull/9` to a browser and
    // `acme/portal#412` to anything reading the front of the path. Rather than pick one,
    // the link is refused — nobody pastes one of these by accident, and guessing at what
    // a link meant is how the owner and repository end up wrong.
    if segments.iter().any(|part| *part == ".." || *part == ".") {
        return Err(refused(
            "the link's path has a relative segment and does not say which pull request it means",
        ));
    }

    // Host, owner, repository, the literal `pull`, the number. Anything after that —
    // `/files`, `/commits` — is still the same pull request and is ignored.
    let [_host, owner, name, "pull", number, ..] = segments.as_slice() else {
        return Err(refused(
            "the link's path is not HOST/owner/repo/pull/NUMBER",
        ));
    };

    let repo = format!("{owner}/{name}");
    if !is_repo_name(&repo) {
        return Err(refused(
            "the owner and repository in the link are not usable",
        ));
    }

    let pr: i64 = number
        .parse()
        .map_err(|_| refused("the pull request number is not a number"))?;
    if pr < 1 {
        return Err(refused("the pull request number is below 1"));
    }

    Ok(PrLink { repo, pr })
}

/// The contract's `RepoName` pattern: `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`.
pub fn is_repo_name(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    let usable = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    };
    usable(owner) && usable(name)
}

/// The contract's `head_sha` pattern: 7 to 40 lower-case hexadecimal characters.
pub fn is_head_sha(sha: &str) -> bool {
    (7..=40).contains(&sha.len())
        && sha
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}
