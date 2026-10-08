//! The five record entities and the closed sets of values they hold.
//!
//! Each closed set is a Rust enum stored as text. Every one can name its own valid
//! values, because a caller that rejects input has to say what it would have accepted.

use serde::{Deserialize, Serialize};

use crate::error::StoreError;

/// Seconds since the Unix epoch. Review Desk stores times as integers and leaves
/// formatting to whatever displays them.
pub type Timestamp = i64;

/// The current time, in seconds since the Unix epoch.
pub fn now() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Defines a closed set of values stored as text, with parsing, display, the full list
/// of valid values, and SQLite conversion.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $name:ident as $field:literal {
            $($variant:ident => $text:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stored text for this value.
            pub fn as_str(&self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            /// The valid values, comma separated, for an error message.
            pub fn valid_values() -> String {
                Self::ALL
                    .iter()
                    .map(|value| value.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = StoreError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($text => Ok($name::$variant),)+
                    other => Err(StoreError::InvalidValue {
                        field: $field,
                        value: other.to_string(),
                        expected: Self::valid_values(),
                    }),
                }
            }
        }

        impl rusqlite::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
            }
        }

        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let text = value.as_str()?;
                text.parse::<$name>()
                    .map_err(|error| rusqlite::types::FromSqlError::Other(Box::new(error)))
            }
        }
    };
}

string_enum! {
    /// How much a finding matters.
    Severity as "severity" {
        Blocker => "blocker",
        Should => "should",
        Nit => "nit",
        Question => "question",
    }
}

string_enum! {
    /// What is to become of a finding in this round's draft.
    Disposition as "disposition" {
        InDraft => "in_draft",
        HeldBack => "held_back",
        Suppressed => "suppressed",
    }
}

string_enum! {
    /// What happened to a finding.
    OutcomeKind as "kind" {
        Cut => "cut",
        Regraded => "regraded",
        Rewritten => "rewritten",
        Kept => "kept",
        Rejected => "rejected",
        RaisedAgain => "raised_again",
        CodeChangedAfter => "code_changed_after",
    }
}

string_enum! {
    /// Who brought an outcome about.
    Actor as "by" {
        Developer => "developer",
        Author => "author",
        Harvester => "harvester",
    }
}

string_enum! {
    /// How far a rule has graduated.
    RuleStage as "stage" {
        Proposed => "proposed",
        Active => "active",
        Lint => "lint",
        Ci => "ci",
        Retired => "retired",
    }
}

/// How widely a rule applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleScope {
    /// Applies to every repository.
    Global,
    /// Applies to one repository, as `owner/repo`.
    Repo(String),
}

impl RuleScope {
    /// The text stored in the `scope` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleScope::Global => "global",
            RuleScope::Repo(_) => "repo",
        }
    }

    /// The repository this is scoped to, if any.
    pub fn repo(&self) -> Option<&str> {
        match self {
            RuleScope::Global => None,
            RuleScope::Repo(repo) => Some(repo),
        }
    }

    /// Rebuild a scope from its two stored columns.
    pub fn from_columns(scope: &str, repo: Option<String>) -> Result<Self, StoreError> {
        match (scope, repo) {
            ("global", _) => Ok(RuleScope::Global),
            ("repo", Some(repo)) => Ok(RuleScope::Repo(repo)),
            // "repo" is a valid scope, so blaming the scope column here would be both
            // wrong and self-contradictory. The fault is the missing repository.
            ("repo", None) => Err(StoreError::InvalidValue {
                field: "scope_repo",
                value: String::new(),
                expected: "a repository as owner/repo, because the scope is repo".to_string(),
            }),
            (other, _) => Err(StoreError::InvalidValue {
                field: "scope",
                value: other.to_string(),
                expected: "global, repo".to_string(),
            }),
        }
    }
}

/// One code review of one pull request at one head commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub id: i64,
    /// The repository as `owner/repo`.
    pub repo: String,
    /// The pull request number.
    pub pr: i64,
    /// The head commit reviewed.
    pub head_sha: String,
    /// Which round of review this is on that pull request, from 1.
    pub round: i64,
    /// The Allele session that ran it.
    pub session_id: Option<String>,
    /// The Locus dispatcher ledger entry.
    pub ledger_id: Option<String>,
    /// The review posted on GitHub.
    pub github_review_id: Option<String>,
    pub created_at: Timestamp,
}

/// A review to create.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NewReview {
    pub repo: String,
    pub pr: i64,
    pub head_sha: String,
    pub round: i64,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ledger_id: Option<String>,
    #[serde(default)]
    pub github_review_id: Option<String>,
}

/// The ids of the other systems' records for a review. A field left `None` is left
/// as it was, so one call can set one id without clearing the others.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReviewLinks {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ledger_id: Option<String>,
    #[serde(default)]
    pub github_review_id: Option<String>,
}

/// What the review understood before it read the diff. One per review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brief {
    pub review_id: i64,
    /// What problem the change is solving.
    pub problem: String,
    /// The source of the diagram, in whatever notation was used.
    pub diagram: Option<String>,
    /// The alternatives considered, written before the diff was read.
    pub option_map: Option<String>,
    /// The verdict on the approach taken.
    pub approach_verdict: Option<String>,
    /// When the developer confirmed it; `None` means they have not.
    pub confirmed_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}

impl Brief {
    /// Whether the developer has confirmed this brief.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

/// A brief to store, replacing any brief already held for the review.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NewBrief {
    pub problem: String,
    #[serde(default)]
    pub diagram: Option<String>,
    #[serde(default)]
    pub option_map: Option<String>,
    #[serde(default)]
    pub approach_verdict: Option<String>,
}

/// One thing the review found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub review_id: i64,
    /// Stable within its review, from 1. This is the id a person quotes.
    pub seq: i64,
    /// Which lens found it.
    pub lens: String,
    pub severity: Severity,
    pub file: Option<String>,
    pub line: Option<i64>,
    /// What is claimed to be wrong.
    pub claim: String,
    /// What supports the claim.
    pub evidence: Option<String>,
    pub disposition: Disposition,
    /// Why it was held back or suppressed.
    pub disposition_reason: Option<String>,
    pub created_at: Timestamp,
}

/// A finding to add to a review. The sequence number is assigned by the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewFinding {
    pub lens: String,
    pub severity: Severity,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    pub claim: String,
    #[serde(default)]
    pub evidence: Option<String>,
    /// Defaults to in draft, which is where a fresh finding starts.
    #[serde(default = "default_disposition")]
    pub disposition: Disposition,
    #[serde(default)]
    pub disposition_reason: Option<String>,
}

fn default_disposition() -> Disposition {
    Disposition::InDraft
}

/// What became of a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub id: i64,
    pub review_id: i64,
    /// The finding's sequence number within the review.
    pub finding_seq: i64,
    pub kind: OutcomeKind,
    /// Who did it.
    pub actor: Actor,
    pub occurred_at: Timestamp,
    /// Anything worth saying beyond the kind.
    pub detail: Option<String>,
}

/// An outcome to record against a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewOutcome {
    pub kind: OutcomeKind,
    pub actor: Actor,
    #[serde(default)]
    pub detail: Option<String>,
    /// When it happened. Defaults to now.
    #[serde(default)]
    pub occurred_at: Option<Timestamp>,
}

/// A finding, named by its review and sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FindingRef {
    pub review_id: i64,
    pub seq: i64,
}

/// Something learned, that should stop a finding recurring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    /// What the rule says.
    pub statement: String,
    /// The failure it prevents. A rule without one is a preference.
    pub prevents: String,
    pub scope: RuleScope,
    pub stage: RuleStage,
    /// The review it came out of.
    pub origin_review_id: Option<i64>,
    /// The findings it came out of.
    pub origin_findings: Vec<FindingRef>,
    pub created_at: Timestamp,
}

/// A rule to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRule {
    pub statement: String,
    pub prevents: String,
    pub scope: RuleScope,
    pub stage: RuleStage,
    #[serde(default)]
    pub origin_review_id: Option<i64>,
    #[serde(default)]
    pub origin_findings: Vec<FindingRef>,
}

/// A whole review, as a reader wants it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullReview {
    pub review: Review,
    pub brief: Option<Brief>,
    pub findings: Vec<Finding>,
    pub outcomes: Vec<Outcome>,
}
