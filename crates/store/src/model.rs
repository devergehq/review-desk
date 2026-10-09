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
///
/// A variant may carry its own documentation. That is not decoration: `WorkKind`'s four
/// values are a published interface, and what an agent is supposed to *do* about each one
/// belongs against the value rather than in a paragraph above the set.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $name:ident as $field:literal {
            $($(#[$variant_meta:meta])* $variant:ident => $text:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $($(#[$variant_meta])* #[serde(rename = $text)] $variant),+
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
    /// What a line of a stored hunk is: added by the pull request, removed by it, or
    /// carried through unchanged.
    ///
    /// Words rather than `+`, `-` and a space. The marker is a fact about the line; which
    /// character draws it is the page's business, and board A2 draws the removal as `−`,
    /// which is not the character a diff uses.
    HunkLineMarker as "marker" {
        Added => "added",
        Removed => "removed",
        Unchanged => "unchanged",
    }
}

string_enum! {
    /// Whether a finding's proof was reproduced or only read.
    ///
    /// **`Unknown` is the default and it is the honest one.** An agent that says nothing
    /// has not said the finding was read rather than run; it has said nothing. Boards A2
    /// and A3 badge `Reproduced` and show no badge otherwise, so a default of `read_not_run`
    /// would put a claim about the method on every finding written before this field
    /// existed.
    Verification as "verification" {
        Reproduced => "reproduced",
        ReadNotRun => "read_not_run",
        Unknown => "unknown",
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
    /// What a review needs an agent to do next.
    ///
    /// **This is a published interface.** The Locus dispatcher polls `work list` and is
    /// written against these four strings, so they are named here once and nothing
    /// formats one by hand.
    ///
    /// The four are exhaustive and mutually exclusive by construction: [`WorkKind`] is
    /// decided by one total function over the draft's status and layer 2, so a review
    /// has at most one. A review waiting on the developer has none, which is the whole
    /// point — this is the list of what an *agent* is to do.
    WorkKind as "kind" {
        /// No brief has been written, and no draft is waiting on the developer — so
        /// either there is no draft at all, or one a session left at `drafting`. An agent
        /// should start reviewing. This is the state `POST /reviews` leaves a review in
        /// when the developer pastes a pull request link into the dashboard, and the one
        /// a session that died before writing a brief leaves behind.
        ReviewRequested => "review_requested",
        /// Layer 2 is settled — the developer confirmed the brief, or it was recorded as
        /// needing no confirmation — and no draft is waiting on the developer. An agent
        /// should carry on past layer 2.
        BriefSettled => "brief_settled",
        /// The developer sent the draft back. An agent should revise it, reading the
        /// note.
        DraftSentBack => "draft_sent_back",
        /// The developer approved the draft and nothing has reported it posted. An agent
        /// should post it and then run `draft posted`.
        DraftApproved => "draft_approved",
    }
}

string_enum! {
    /// How layer 2 came to be settled.
    ///
    /// **`Waived` exists so that nothing downstream can mistake one for the other.** A
    /// brief recorded as needing no confirmation produces the same `brief_settled` item
    /// as one the developer confirmed, and without this field a dispatcher's ledger, and
    /// every log after it, would say the developer confirmed a review nobody looked at.
    /// The reason the caller gave is on the brief.
    SignOff as "sign_off" {
        /// The developer confirmed the brief.
        Developer => "developer",
        /// An agent recorded the brief as needing no confirmation, and said why.
        Waived => "waived",
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
    /// Whether the developer has to confirm this brief at all.
    ///
    /// True for every brief unless the caller said otherwise when it was written. It is
    /// the one place an agent decides a developer step is unnecessary, which is why
    /// [`crate::Store::put_brief`] refuses to move it from true to false.
    pub confirmation_required: bool,
    /// Why the caller said no confirmation was needed, in the caller's own words.
    ///
    /// `Some` exactly when `confirmation_required` is false. The threshold that decided
    /// it is the caller's — Review Desk does not hold one — so the reason is the only
    /// record of why a developer step was skipped.
    pub confirmation_not_required_because: Option<String>,
    pub updated_at: Timestamp,
}

impl Brief {
    /// Whether the developer has confirmed this brief.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }

    /// Whether layer 2 is finished with: confirmed, or recorded as needing no
    /// confirmation.
    ///
    /// **Not the same question as [`Brief::is_confirmed`], and the difference is the
    /// point.** A screen asks whether the developer said yes. A review session, and the
    /// dispatcher reading `work list`, ask whether anything is still waiting at layer 2 —
    /// and a brief that needed no confirmation never was.
    pub fn is_settled(&self) -> bool {
        self.is_confirmed() || !self.confirmation_required
    }
}

/// A brief to store, replacing any brief already held for the review.
///
/// Unknown fields are refused. Every field but `problem` is optional, so a misspelled
/// `option_map` would otherwise deserialise to `None` and the record would quietly lose
/// what the review actually wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Whether the developer has to confirm this brief. **Defaults to true**, and a
    /// brief already written as requiring confirmation cannot be rewritten as not
    /// requiring it.
    #[serde(default = "confirmation_is_required")]
    pub confirmation_required: bool,
    /// Why the caller says no confirmation is needed. Required when
    /// `confirmation_required` is false, and refused when it is true.
    #[serde(default)]
    pub confirmation_not_required_because: Option<String>,
}

/// What `confirmation_required` is when a caller does not mention it.
///
/// A function rather than serde's `bool` default, which is `false`. A brief that silently
/// needed no confirmation because the field was misspelled — or because a caller on an
/// older version never sent it — would skip the developer's one act at layer 2, which is
/// the opposite of the safe answer.
fn confirmation_is_required() -> bool {
    true
}

/// **Hand-written, because `derive(Default)` would give `confirmation_required: false`.**
///
/// Nineteen call sites in this crate's tests build a `NewBrief` with
/// `..Default::default()`, and a derived default would quietly make every one of them a
/// brief needing no confirmation. The field whose whole purpose is to be hard to set must
/// not be settable by leaving something out.
impl Default for NewBrief {
    fn default() -> Self {
        NewBrief {
            problem: String::new(),
            diagram: None,
            option_map: None,
            options: Vec::new(),
            approach_verdict: None,
            provenance: None,
            confirmation_required: true,
            confirmation_not_required_because: None,
        }
    }
}

/// One line of a stored hunk: what the pull request did to it, and its text.
///
/// **The text is stored exactly as the agent gave it.** Review Desk does not fetch code —
/// Patrick's rule — so this string is the only copy of that line there is, and a trim, a
/// re-wrap or a tab turned into spaces would be Review Desk inventing source it never
/// read. A line that is nothing but a space is a line that is nothing but a space.
///
/// The line *number* is not here. It is derivable from the hunk's `start_line` and the
/// markers above it, and storing both would let a row say line 58 is the fourth line of a
/// hunk that starts at 56. See [`FindingHunk::numbered_lines`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HunkLine {
    pub marker: HunkLineMarker,
    /// The line, verbatim, with no end-of-line character.
    pub text: String,
}

/// The lines around a finding, as the pull request changed them.
///
/// The same shape going in and coming out, for [`BriefOption`]'s reason: the store assigns
/// nothing here, so a second type would be the first type with the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingHunk {
    /// The file the hunk is from, as the agent spelled it.
    ///
    /// Its own field rather than [`Finding::file`]'s: a finding can point at a call site
    /// and show the hunk it calls into, and nothing here should decide that the two are
    /// the same path.
    pub file: String,
    /// The number the hunk's first **numbered** line carries.
    ///
    /// A removed line carries no number, because it is not in the file any more — board
    /// A3's F6 opens on a removed line and still numbers from 58. So this is the number of
    /// the first added or unchanged line, whether or not that line is the first one.
    pub start_line: i64,
    /// The lines, in the order they are to be drawn.
    pub lines: Vec<HunkLine>,
}

impl FindingHunk {
    /// Each line with the number the page draws beside it, and `None` for a removed line.
    ///
    /// **This is the whole of the derivation, in one place.** An added or unchanged line
    /// takes the next number; a removed line takes none. Board A2's F1 starts at 56 and
    /// draws 56, 57, (blank), 58, 59, 60, 61, 62, 63 — this function, run over that hunk.
    pub fn numbered_lines(&self) -> Vec<(Option<i64>, &HunkLine)> {
        let mut next = self.start_line;
        self.lines
            .iter()
            .map(|line| match line.marker {
                HunkLineMarker::Removed => (None, line),
                _ => {
                    let number = next;
                    // Saturating, so that nothing here can panic or wrap. A hunk written
                    // through this crate cannot reach the end of `i64` —
                    // `finding_evidence::validate` refuses a start line that high — but
                    // this function also runs over rows read back from a database file,
                    // and it is read by a server. It does not get to panic over a number
                    // somebody put in the file by hand.
                    next = next.saturating_add(1);
                    (Some(number), line)
                }
            })
            .collect()
    }

    /// Whether this hunk draws a line with this number.
    ///
    /// What makes [`Finding::flagged_line`] checkable: a comment anchored to a line the
    /// hunk does not draw is a comment the page has nowhere to put.
    pub fn draws_line(&self, line: i64) -> bool {
        self.numbered_lines()
            .iter()
            .any(|(number, _)| *number == Some(line))
    }
}

/// One thing the review found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub review_id: i64,
    /// **Stable across every round of the pull request, from 1. This is the id a person
    /// quotes, and it is the finding's identity.**
    ///
    /// A carried finding keeps the number it had — F2 in round 1 is F2 in round 3 — and a
    /// new finding takes the next number unused on the pull request, so no two findings of
    /// one pull request share a number. That is what makes [`Finding::introduced_by_seq`]
    /// able to name a finding with a number alone.
    ///
    /// Findings written before migration 6 were numbered within their review, so two
    /// rounds of one pull request may hold the same number. Such a round has no links and
    /// reads as a first round.
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
    /// The lines around the finding, as the pull request changed them. `None` for a
    /// finding that was written with only a file and a line, which still displays.
    pub hunk: Option<FindingHunk>,
    /// The line within the hunk the inline comment attaches to.
    pub flagged_line: Option<i64>,
    /// The comment as it will be posted on that line.
    ///
    /// **Not the claim.** The claim is what the record says is wrong; this is what the
    /// author reads on GitHub, in the second person, and board A2 captions it "as it will
    /// post" to say exactly that.
    pub inline_comment: Option<String>,
    /// The consequence, in a sentence or two.
    pub why_it_matters: Option<String>,
    /// The proof as text — test output, two logged keys — stored verbatim.
    pub proof: Option<String>,
    /// The plain caption drawn beneath the proof — "A throwaway test, not committed."
    ///
    /// Its own field rather than more of [`Finding::proof`], because the two are drawn
    /// differently: the proof is a monospace block and this is a sentence about it. Folded
    /// into `proof` it would render as though it were test output.
    pub proof_note: Option<String>,
    /// Whether that proof was reproduced or only read. `Unknown` when nothing said.
    pub verification: Verification,
    /// What would fix it.
    pub suggested_fix: Option<String>,
    /// Sibling code the fix should look like, in the order given.
    pub fix_references: Vec<String>,
    /// The earlier round this finding was carried from. `None` for a finding raised fresh
    /// in this round, and for every finding written before migration 6.
    ///
    /// The seq is not repeated here, because it cannot differ: a carried finding keeps its
    /// number, so this plus [`Finding::seq`] names the finding it continues.
    pub continues_review_id: Option<i64>,
    /// What became of this finding **in this round**.
    ///
    /// Not the same question as [`Finding::disposition`], which is what is to become of it
    /// in this round's draft. A finding can be `fixed` and so in no draft at all.
    pub round_status: RoundStatus,
    /// Who decided that status. `None` while nothing has been decided.
    pub round_status_by: Option<DecidedBy>,
    /// How it was checked, in the checker's own words — "their new test fails with the fix
    /// reverted".
    pub round_status_checked: Option<String>,
    pub round_status_at: Option<Timestamp>,
    /// The commit the status rests on: for a fixed finding, the commit that fixed it.
    pub round_status_commit: Option<String>,
    /// The commit a new finding is attributed to.
    pub introduced_by_commit: Option<String>,
    /// The earlier finding whose fix caused this one, by its number. A number alone is
    /// enough: after migration 6 a number is an identity across the whole pull request.
    pub introduced_by_seq: Option<i64>,
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
    /// The lines around the finding, as the pull request changed them.
    #[serde(default)]
    pub hunk: Option<FindingHunk>,
    /// The line within the hunk the inline comment attaches to. Refused when a hunk is
    /// given and does not draw that line.
    #[serde(default)]
    pub flagged_line: Option<i64>,
    /// The comment as it will be posted on that line. Stored verbatim.
    #[serde(default)]
    pub inline_comment: Option<String>,
    /// The consequence. Stored verbatim.
    #[serde(default)]
    pub why_it_matters: Option<String>,
    /// The proof as text. Stored verbatim.
    #[serde(default)]
    pub proof: Option<String>,
    /// The plain caption drawn beneath the proof. Stored verbatim.
    #[serde(default)]
    pub proof_note: Option<String>,
    /// Whether the proof was reproduced or only read. Defaults to unknown.
    #[serde(default = "default_verification")]
    pub verification: Verification,
    /// What would fix it. Stored verbatim.
    #[serde(default)]
    pub suggested_fix: Option<String>,
    /// Sibling code the fix should look like. A blank reference is refused.
    #[serde(default)]
    pub fix_references: Vec<String>,
    /// The commit this finding is attributed to — "introduced by a41c9e2".
    #[serde(default)]
    pub introduced_by_commit: Option<String>,
    /// The earlier finding whose fix caused this one, by its number.
    ///
    /// Checked against the pull request's findings on the way in, so a typo is a refusal
    /// naming the number rather than a finding attributed to one that was never raised.
    #[serde(default)]
    pub introduced_by_seq: Option<i64>,
}

fn default_disposition() -> Disposition {
    Disposition::InDraft
}

/// What `verification` is when a caller does not mention it.
///
/// Hand-written for [`default_disposition`]'s reason, and for a second one: serde's default
/// for an enum is its first variant, which here would be `reproduced`. A caller that said
/// nothing about its method would have the record claim it ran the code.
fn default_verification() -> Verification {
    Verification::Unknown
}

/// **Hand-written, because `derive(Default)` would get two fields wrong.**
///
/// It would give `disposition: Disposition::InDraft` only by accident of declaration order
/// and `verification: Verification::Reproduced` outright — the first variant of each. The
/// same argument as [`NewBrief`]'s own hand-written default, and the same payoff: the call
/// sites in this crate's tests build a finding with `..Default::default()`, so a field
/// added here does not quietly change what any of them mean.
impl Default for NewFinding {
    fn default() -> Self {
        NewFinding {
            lens: String::new(),
            severity: Severity::Should,
            file: None,
            line: None,
            claim: String::new(),
            evidence: None,
            disposition: default_disposition(),
            disposition_reason: None,
            rule_id: None,
            hunk: None,
            flagged_line: None,
            inline_comment: None,
            why_it_matters: None,
            proof: None,
            proof_note: None,
            verification: default_verification(),
            suggested_fix: None,
            fix_references: Vec::new(),
            introduced_by_commit: None,
            introduced_by_seq: None,
        }
    }
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

/// One thing a review needs an agent to do.
///
/// The review is carried whole rather than flattened, so the command line decides which
/// of its fields reach the wire and the store does not hold a second opinion about the
/// shape of its own records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub kind: WorkKind,
    pub review: Review,
    /// How layer 2 was settled, or `None` while it is not.
    pub sign_off: Option<SignOff>,
    /// When the review entered this state: its creation for `review_requested`, the
    /// confirmation or the brief's write for `brief_settled`, and the draft's
    /// `sent_back_at` or `approved_at` for the other two.
    ///
    /// **`(kind, review.id, entered_at)` is the item's identity**, which is what a caller
    /// keyed on to avoid acting twice. It moves when a review re-enters a state: a draft
    /// sent back, revised and sent back again is `ready` in between, so the second
    /// send-back rewrites `sent_back_at` and the item is a new one.
    pub entered_at: Timestamp,
    /// The developer's instruction, on `draft_sent_back` and nothing else.
    pub note: Option<String>,
}

/// Which reviews a caller is asking about.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkFilter {
    /// One review, for a session watching its own.
    pub review_id: Option<i64>,
    /// One repository, for a dispatcher that serves one.
    pub repo: Option<String>,
}

/// How many rules one repository may hold in force before the set stops being one
/// anybody can remember.
pub const ACTIVE_RULES_CAP: i64 = 20;

// ---------------------------------------------------------------------------------------
// A finding's identity across rounds, and what changed between them. DEV-873.
//
// Appended rather than slotted in beside the sets above, deliberately: three sessions are
// editing this file at once, and an append conflicts with nothing.
// ---------------------------------------------------------------------------------------

string_enum! {
    /// What became of a finding **in one round**.
    ///
    /// **Not the same set as [`Disposition`], and the two answer different questions.** A
    /// disposition says what is to become of the finding in this round's draft; this says
    /// what happened to the finding itself. A fixed finding has nothing to post, so it is
    /// `fixed` here and absent from the draft — which a disposition could not express
    /// without `suppressed` coming to mean two different things.
    ///
    /// Nothing derives one of these. DEV-873 puts deciding the status out of scope on
    /// purpose: an agent or the developer sets it, and says how they checked.
    RoundStatus as "round_status" {
        /// Raised in this round. Every finding starts here, including every finding
        /// written before migration 6.
        New => "new",
        /// The author fixed it, and somebody checked that they had.
        Fixed => "fixed",
        /// Still there. This is the status that makes a round go in a circle, and
        /// [`RoundStatus::is_open`] is what counts the rounds it has stayed in.
        StillOpen => "still_open",
        /// The author gave a reason and it was accepted. The reason is the checked note.
        Accepted => "accepted",
        /// Withdrawn by the reviewer: it should not have been raised, or no longer holds.
        Withdrawn => "withdrawn",
        /// Moved to a ticket, and so not part of this round.
        Ticketed => "ticketed",
    }
}

impl RoundStatus {
    /// Whether the finding is still outstanding at this status.
    ///
    /// `new` and `still_open` only. The other four are settled one way or another, which
    /// is what "2 fixed · 2 still open · 1 new" counts and what stops the consecutive-round
    /// count carrying on past the round a finding was answered in.
    pub fn is_open(&self) -> bool {
        matches!(self, RoundStatus::New | RoundStatus::StillOpen)
    }
}

string_enum! {
    /// Who decided a finding's status in a round.
    ///
    /// **Deliberately not [`Actor`].** `Actor` is who brought an *outcome* about —
    /// developer, author or harvester — and it is on the wire in `Outcome`, so widening it
    /// would change a published shape. More to the point it has no value for an agent, and
    /// an agent decides most of these: recording that as `developer` would put the
    /// developer's name against a check nobody showed them, which is exactly what
    /// [`SignOff::Waived`] was added to stop.
    ///
    /// **This is not a sign-off.** Only the developer signs off, in the dashboard. Saying
    /// a finding is fixed is not approving a review, and `agent` is here so that the record
    /// can tell the two apart.
    DecidedBy as "round_status_by" {
        /// The developer, in the dashboard.
        Developer => "developer",
        /// A review session.
        Agent => "agent",
        /// The pull request's author, through a reply the harvester read.
        Author => "author",
    }
}

/// A finding's status in a round, with who decided it and how.
///
/// One shape for carrying a finding forward and for changing the status of one already
/// carried, because they record the same four facts. A second type would be the first with
/// the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingCarry {
    pub status: RoundStatus,
    /// Who decided. Required: "each change records who decided" is the whole point, and a
    /// default would make it the record's guess rather than the caller's statement.
    pub by: DecidedBy,
    /// How it was checked, in the checker's own words.
    #[serde(default)]
    pub checked: Option<String>,
    /// The commit the status rests on — for a fixed finding, the commit that fixed it.
    #[serde(default)]
    pub commit: Option<String>,
}

/// The previous round's findings, ready to be carried into this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carryable {
    /// The review of the previous round.
    pub from_review_id: i64,
    pub from_round: i64,
    /// Every finding that round held, in number order — settled ones included.
    ///
    /// Settled ones are not filtered out. A finding ticketed in round 1 is not part of
    /// round 2, and the page says so in those words; deciding that here would hide from
    /// the caller the one fact it needs to say it.
    pub findings: Vec<Finding>,
}

/// One commit between two rounds' head commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundCommit {
    pub review_id: i64,
    /// The commit, as GitHub gave it. Never parsed.
    pub sha: String,
    /// Where GitHub listed it, from 0, oldest first.
    pub position: i64,
    /// The commit message's first line.
    pub summary: String,
    pub committed_at: Option<Timestamp>,
    /// The files it touched, in path order.
    pub files: Vec<String>,
}

/// A commit to record against a round.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRoundCommit {
    pub sha: String,
    pub summary: String,
    #[serde(default)]
    pub committed_at: Option<Timestamp>,
    #[serde(default)]
    pub files: Vec<String>,
}

/// How a round's findings stand, by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoundStatusCounts {
    pub new: i64,
    pub fixed: i64,
    pub still_open: i64,
    pub accepted: i64,
    pub withdrawn: i64,
    pub ticketed: i64,
}

impl RoundStatusCounts {
    /// Count one finding's status.
    pub fn add(&mut self, status: RoundStatus) {
        let field = match status {
            RoundStatus::New => &mut self.new,
            RoundStatus::Fixed => &mut self.fixed,
            RoundStatus::StillOpen => &mut self.still_open,
            RoundStatus::Accepted => &mut self.accepted,
            RoundStatus::Withdrawn => &mut self.withdrawn,
            RoundStatus::Ticketed => &mut self.ticketed,
        };
        *field += 1;
    }
}

/// One round of a pull request, as the timeline across the top of the page reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundSummary {
    pub review_id: i64,
    pub round: i64,
    pub head_sha: String,
    pub created_at: Timestamp,
    /// When this round's draft was posted. `None` for a round that has not been.
    pub posted_at: Option<Timestamp>,
    /// The GitHub review event it was posted as, or is to be posted as.
    pub event: Option<ReviewEvent>,
    pub findings: i64,
    /// Commits recorded between the previous round's head and this one's.
    pub commits: i64,
    /// Replies the harvester read on this round's findings.
    pub replies: i64,
    pub threads_resolved: i64,
    pub statuses: RoundStatusCounts,
}

/// A finding with the three things about it that are derived from the carry chain rather
/// than stored.
///
/// Derived, so none of them can drift. The chain and the round's commits are the source,
/// and both are written by something else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingInRound {
    pub finding: Finding,
    /// Which round it was carried from, by number. `None` when it was raised in this one.
    pub continues_round: Option<i64>,
    /// How many consecutive rounds it has been open, counting this one. 0 when it is not
    /// open now — "open for 2 rounds" is a sentence about a finding that still is.
    pub rounds_open: i64,
    /// Whether this is a new finding on code that has not changed since the previous
    /// round — a thing the earlier round could have caught and did not.
    ///
    /// `None` when the question cannot be answered: a first round, a finding that is not
    /// new, a finding that names no file, or a round whose commits have not been recorded.
    /// Three-valued on purpose — reporting `false` for "nobody looked" would read as "this
    /// was not missed".
    pub late: Option<bool>,
}
