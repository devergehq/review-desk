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
    ///
    /// The last three are what the harvester records. `Replied` is deliberately not
    /// `Accepted` or `Rejected`: deciding which a reply was needs judgement, and the
    /// harvester is deterministic. It stores the reply and leaves the reading of it to
    /// a person or an agent.
    OutcomeKind as "kind" {
        Cut => "cut",
        Regraded => "regraded",
        Rewritten => "rewritten",
        Kept => "kept",
        Rejected => "rejected",
        RaisedAgain => "raised_again",
        Replied => "replied",
        ThreadResolved => "thread_resolved",
        CodeChangedAfter => "code_changed_after",
    }
}

string_enum! {
    /// What a pull request was last seen to be.
    PrState as "pr_state" {
        Open => "open",
        Merged => "merged",
        Closed => "closed",
    }
}

impl PrState {
    /// Whether this pull request is finished with, and so has nothing left to harvest.
    pub fn is_settled(&self) -> bool {
        matches!(self, PrState::Merged | PrState::Closed)
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

string_enum! {
    /// Where a draft has got to.
    ///
    /// The transitions, and nothing else: `drafting → ready → approved → posted`, with
    /// `ready ⇄ sent_back`. Only the last one involves GitHub, and the review session
    /// makes it, not Review Desk.
    DraftStatus as "status" {
        Drafting => "drafting",
        Ready => "ready",
        Approved => "approved",
        SentBack => "sent_back",
        Posted => "posted",
    }
}

impl DraftStatus {
    /// Whether a draft at this status may still be rewritten.
    ///
    /// An approved draft is what the review session reads and posts, so editing it
    /// underneath would change what gets posted.
    pub fn is_writable(&self) -> bool {
        !matches!(self, DraftStatus::Approved | DraftStatus::Posted)
    }
}

string_enum! {
    /// The GitHub review event an approved draft is to be posted as.
    ReviewEvent as "event" {
        Comment => "comment",
        RequestChanges => "request_changes",
        Approve => "approve",
    }
}

string_enum! {
    /// How far the one ticket covering a review's held-back nits has got.
    NitTicketStatus as "nit_ticket_status" {
        NotRequested => "none",
        Requested => "requested",
        Filed => "filed",
    }
}

string_enum! {
    /// The seven layers of a review, in order. A layer's number fixes its name, which
    /// is why only the number is stored.
    ReviewLayerName as "layer" {
        Lint => "lint",
        Understand => "understand",
        Alternatives => "alternatives",
        SolutionMap => "solution_map",
        CodeReview => "code_review",
        Draft => "draft",
        Harvest => "harvest",
    }
}

impl ReviewLayerName {
    /// How many layers a review has. Stated once, here, so no caller carries a 7 of
    /// its own.
    pub const COUNT: i64 = 7;

    /// The layer with this number, from 1.
    pub fn for_number(number: i64) -> Result<Self, StoreError> {
        usize::try_from(number)
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| Self::ALL.get(index))
            .copied()
            .ok_or_else(|| StoreError::InvalidValue {
                field: "layer",
                value: number.to_string(),
                expected: format!("a layer number from 1 to {}", Self::COUNT),
            })
    }

    /// This layer's number, from 1.
    pub fn number(&self) -> i64 {
        Self::ALL
            .iter()
            .position(|name| name == self)
            // ALL is this enum's own variants, so the position is always there.
            .map_or(0, |index| index as i64 + 1)
    }
}

string_enum! {
    /// What a layer is doing.
    ///
    /// `waiting_on_you` is the developer's turn, and is what the Inbox groups on.
    /// `skipped` is a layer deliberately not run, which a derived status could never
    /// tell apart from one that has not started.
    ReviewLayerState as "state" {
        Pending => "pending",
        Running => "running",
        Done => "done",
        WaitingOnYou => "waiting_on_you",
        Failed => "failed",
        Skipped => "skipped",
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
    /// The pull request's title.
    pub title: Option<String>,
    /// The Linear ticket, opaque.
    pub ticket_id: Option<String>,
    /// Additions plus deletions, as the caller supplied them.
    pub changed_lines: Option<i64>,
    /// What the review did not look at, in the reviewer's own words.
    pub not_checked: Option<String>,
    /// The Allele session that ran it.
    pub session_id: Option<String>,
    /// The Locus dispatcher ledger entry.
    pub ledger_id: Option<String>,
    /// The review posted on GitHub.
    pub github_review_id: Option<String>,
    /// What the pull request was last seen to be. `None` means the harvester has never
    /// looked.
    pub pr_state: Option<PrState>,
    /// When the pull request reached that state: merged or closed at. `None` while it
    /// is open.
    pub pr_state_at: Option<Timestamp>,
    /// When the harvester last read this review's outcomes back from GitHub.
    pub harvested_at: Option<Timestamp>,
    /// How far the one ticket covering this review's held-back nits has got.
    pub nit_ticket_status: NitTicketStatus,
    /// The ticket those nits were filed as, opaque.
    pub nit_ticket_id: Option<String>,
    pub nit_ticket_requested_at: Option<Timestamp>,
    pub nit_ticket_filed_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

impl Review {
    /// Whether this review's pull request was finished with at the last harvest, and
    /// so should not be fetched again.
    pub fn is_settled(&self) -> bool {
        self.pr_state.is_some_and(|state| state.is_settled())
    }
}

/// A review to create.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NewReview {
    pub repo: String,
    pub pr: i64,
    pub head_sha: String,
    pub round: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub ticket_id: Option<String>,
    #[serde(default)]
    pub changed_lines: Option<i64>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ledger_id: Option<String>,
    #[serde(default)]
    pub github_review_id: Option<String>,
}

/// A review opened for a head commit: the one that was already there, or a new round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedReview {
    pub review: Review,
    /// True when this call created the review, false when it was already on the record.
    /// A caller that needs to know which happened should not have to read the round and
    /// guess.
    pub created: bool,
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
    /// The alternatives considered, as one opaque string.
    ///
    /// **Superseded by [`Brief::options`]**, which is what a screen can render. It is
    /// kept, and still written and read, because migration 4 must not destroy what a
    /// review already wrote here; nothing new should use it.
    pub option_map: Option<String>,
    /// The alternatives considered, written before the diff was read, in key order.
    pub options: Vec<BriefOption>,
    /// The verdict on the approach taken.
    pub approach_verdict: Option<String>,
    /// Where the problem statement came from and what the developer changed about it.
    pub provenance: Option<String>,
    /// When the developer confirmed it; `None` means they have not.
    pub confirmed_at: Option<Timestamp>,
    /// Who confirmed it. Only the developer signs off.
    pub confirmed_by: Option<Actor>,
    pub updated_at: Timestamp,
}

impl Brief {
    /// Whether the developer has confirmed this brief.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

/// A brief to store, replacing any brief already held for the review.
///
/// Unknown fields are refused. Every field but `problem` is optional, so a misspelled
/// `option_map` would otherwise deserialise to `None` and the record would quietly lose
/// what the review actually wrote.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewBrief {
    pub problem: String,
    #[serde(default)]
    pub diagram: Option<String>,
    /// Superseded by `options`. Giving both is refused rather than merged: they are two
    /// spellings of one fact, and nothing could say which the review meant.
    #[serde(default)]
    pub option_map: Option<String>,
    #[serde(default)]
    pub options: Vec<BriefOption>,
    #[serde(default)]
    pub approach_verdict: Option<String>,
    #[serde(default)]
    pub provenance: Option<String>,
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
    /// The rule this finding was raised under, if any. The citation the whole feedback
    /// loop runs through: without it a rule cannot be said to be cited or stale.
    pub rule_id: Option<i64>,
    /// The GitHub review comment this finding was posted as, set by
    /// `review-desk finding link`. Without it the harvester cannot find the finding's
    /// thread, so it is skipped for replies and resolution.
    pub github_comment_id: Option<String>,
    pub created_at: Timestamp,
}

/// A finding to add to a review. The sequence number is assigned by the store.
///
/// Unknown fields are refused, for the same reason as [`NewBrief`]: a misspelled
/// `evidence` is a finding with no evidence, which is worse than a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The rule this finding was raised under, if any.
    #[serde(default)]
    pub rule_id: Option<i64>,
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
    /// The GitHub fact this was harvested from — a reply comment's id, a thread's root
    /// comment id, a commit — or `None` for an outcome recorded by hand. It is what
    /// makes a second harvest write nothing.
    pub harvest_key: Option<String>,
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
    /// Why the app proposed it, with the count behind it. Set on proposals.
    pub why_proposed: Option<String>,
    /// The free line shown under the statement: a staleness note, an accepted
    /// exception, or the reason the rule was last moved between stages.
    pub note: Option<String>,
    /// When a finding last cited it. `None` means never.
    pub last_cited_at: Option<Timestamp>,
    /// When to ask about it again, set by keeping it for longer. Until then it is not
    /// shown as stale.
    pub review_again_at: Option<Timestamp>,
    pub created_at: Timestamp,
    /// When it was last changed: created, promoted, dismissed, retired or deferred.
    pub updated_at: Timestamp,
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
    #[serde(default)]
    pub why_proposed: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// A whole review, as a reader wants it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullReview {
    pub review: Review,
    /// All seven layers, in order, whether or not each has been written.
    pub layers: Vec<ReviewLayer>,
    pub brief: Option<Brief>,
    pub findings: Vec<Finding>,
    pub outcomes: Vec<Outcome>,
    /// The draft, once one has been written.
    pub draft: Option<Draft>,
}

/// The facts about a review that are not its identity.
///
/// A field left `None` is left as it was, for the reason [`ReviewLinks`] gives: one
/// call should be able to set the title without clearing what was not checked.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReviewFacts {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub ticket_id: Option<String>,
    #[serde(default)]
    pub changed_lines: Option<i64>,
    #[serde(default)]
    pub not_checked: Option<String>,
}

impl ReviewFacts {
    /// Whether this would change anything. A call that sets no fact is a mistake worth
    /// naming rather than reporting success for.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.ticket_id.is_none()
            && self.changed_lines.is_none()
            && self.not_checked.is_none()
    }
}

/// One alternative the review considered, written before the diff was read.
///
/// The same shape going in and coming out: there is nothing the store assigns, so a
/// second type would only be the first type with the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefOption {
    /// The letter the option is known by.
    pub key: String,
    pub title: String,
    pub argument_for: String,
    pub argument_against: String,
    /// Whether this is what the author built.
    pub chosen: bool,
}

/// What one of a review's seven layers is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewLayer {
    pub review_id: i64,
    pub number: i64,
    /// Fixed by the number, and derived from it rather than stored.
    pub name: ReviewLayerName,
    pub state: ReviewLayerState,
    /// The short phrase shown after the layer's name — "passed", "3 options",
    /// "6 findings". Written by the layer that ran.
    pub detail: Option<String>,
    /// `None` for a layer nothing has written yet.
    pub updated_at: Option<Timestamp>,
}

/// A review and the layer it is currently on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAtLayer {
    pub review: Review,
    pub layer: ReviewLayer,
}

/// What running the review-craft lint over a draft body reported.
///
/// Every field is optional, and `None` means the lint has not run rather than that it
/// found nothing: a count Review Desk cannot answer is absent, and zero is a
/// measurement.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftLint {
    /// Whether the lint reported no errors. Warnings do not fail it.
    #[serde(default)]
    pub passes: Option<bool>,
    #[serde(default)]
    pub visible_words: Option<i64>,
    /// The budget for this change's size, so a screen can say "118 of 150".
    #[serde(default)]
    pub word_budget: Option<i64>,
    #[serde(default)]
    pub findings_in_body: Option<i64>,
    /// Findings at disposition `in_draft`. A mismatch with `findings_in_body` means one
    /// was dropped silently.
    #[serde(default)]
    pub findings_expected: Option<i64>,
    /// Findings already raised in an earlier round on the same pull request. Non-zero
    /// is a review going in a circle.
    #[serde(default)]
    pub raised_before_on_pr: Option<i64>,
    /// The lint's error lines, verbatim, as the lint printed them. One string rather
    /// than a list, because the store holds what was reported and splitting it is a
    /// display decision.
    #[serde(default)]
    pub errors: Option<String>,
    #[serde(default)]
    pub checked_at: Option<Timestamp>,
}

impl DraftLint {
    /// Whether anything was reported at all.
    pub fn is_empty(&self) -> bool {
        *self == DraftLint::default()
    }
}

/// The draft review: one per review, replaced in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub review_id: i64,
    /// The review body as GitHub-flavoured markdown, posted verbatim.
    pub body: String,
    pub status: DraftStatus,
    /// The GitHub review event chosen at approval. `None` until then.
    pub event: Option<ReviewEvent>,
    pub approved_at: Option<Timestamp>,
    /// Who approved it. Only the developer signs off.
    pub approved_by: Option<Actor>,
    /// The developer's instruction when the draft was returned.
    pub sent_back_note: Option<String>,
    pub sent_back_at: Option<Timestamp>,
    /// When the review session reported it posted. Set with the review's
    /// `github_review_id`, never by Review Desk acting on GitHub.
    pub posted_at: Option<Timestamp>,
    pub lint: DraftLint,
    pub updated_at: Timestamp,
}

/// A draft to store, replacing any draft already held for the review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDraft {
    pub body: String,
    /// Either `drafting` or `ready`, and nothing else.
    ///
    /// **This is the boundary the sign-off decision rests on.** `approved` is reachable
    /// only through [`crate::Store::approve_draft`] and `posted` only through
    /// [`crate::Store::post_draft`], so whatever writes a draft cannot sign it off by
    /// writing a status.
    pub status: DraftStatus,
    /// What the lint said about this body, if it was run.
    pub lint: Option<DraftLint>,
}

impl NewDraft {
    /// A finished draft: this body, waiting on the developer.
    pub fn ready(body: impl Into<String>) -> Self {
        NewDraft {
            body: body.into(),
            status: DraftStatus::Ready,
            lint: None,
        }
    }
}

/// The one ticket covering every nit held back from a review's draft.
///
/// The findings it covers are not stored against it. They are every finding on the
/// review at disposition `held_back`, read at the time of asking, so the set cannot
/// drift from the dispositions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NitTicket {
    pub review_id: i64,
    pub status: NitTicketStatus,
    /// The Linear ticket, opaque.
    pub ticket_id: Option<String>,
    /// The held-back findings the ticket covers, in sequence order.
    pub finding_seqs: Vec<i64>,
    pub requested_at: Option<Timestamp>,
    pub filed_at: Option<Timestamp>,
}

/// An accepted exception to a rule. The rule stays in force.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleException {
    pub id: i64,
    pub rule_id: i64,
    pub reason: String,
    pub created_at: Timestamp,
}

/// One repository, with what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSummary {
    /// The repository as `owner/repo`.
    pub repo: String,
    /// Reviews on this repository with no posted GitHub review yet.
    pub open_reviews: i64,
    /// Rules in force scoped to this repository.
    pub active_rules: i64,
}

/// A rule's counts.
///
/// Every field is optional because the set shown differs by stage, and because a count
/// Review Desk has no source for is absent rather than zero. The three fields that are
/// always `None` say so in their own documentation rather than being quietly dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuleCounts {
    /// Findings raised citing this rule.
    pub raised: Option<i64>,
    /// Of those, the ones with a `kept` outcome.
    pub kept: Option<i64>,
    /// Of those, the ones with a `rejected` outcome that gave a reason.
    pub rejected_with_reason: Option<i64>,
    /// Findings citing this rule other than the ones it came out of — the "cited by 6
    /// later findings" on the Rules screen.
    pub cited_by_findings: Option<i64>,
    /// Accepted exceptions.
    pub exceptions: Option<i64>,
    /// **Always `None`.** Catches by a lint or CI check. Review Desk holds no CI data
    /// and no filed issue gives it any.
    pub caught_by_check: Option<i64>,
    /// **Always `None`.** Findings raised by a review since the rule became a check.
    /// The same absent CI source.
    pub raised_in_a_review_since: Option<i64>,
    /// **Always `None`.** When a human reviewer last said this out loud. A GitHub
    /// harvest would own it; the harvester reads four kinds of outcome and this is not
    /// one of them.
    pub last_human_mention_at: Option<Timestamp>,
}

/// The figures across the top of the Rules screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesSummary {
    pub active_rules: i64,
    /// The per-repository cap. A rule set nobody can hold in their head is not a rule
    /// set.
    pub active_rules_cap: i64,
    /// **Always `None`.** The replay consistency check does not exist yet.
    pub replay_matched: Option<i64>,
    /// **Always `None`.** As above.
    pub replay_total: Option<i64>,
    /// Findings in the window with a `kept` outcome.
    pub findings_kept: Option<i64>,
    /// Findings created in the window.
    pub findings_total: Option<i64>,
    /// How many days those two figures cover.
    pub findings_window_days: Option<i64>,
    /// Pull requests where a finding was raised a second time — a review that went in
    /// a circle.
    pub raised_twice_on_one_pr: Option<i64>,
}

/// How many rules one repository may hold in force before the set stops being one
/// anybody can remember.
pub const ACTIVE_RULES_CAP: i64 = 20;
