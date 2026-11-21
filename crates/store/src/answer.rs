//! The answer contract: what a review's session may send back, checked before it is kept.
//! DEV-1332, under DEV-1331. The spec is `docs/answer-contract.md`, field for field.
//!
//! **The session proposes; the developer decides.** An [`Answer`] changes nothing in the
//! record. It is kept beside the raw body of the reply it came in, so the page can draw it
//! and a later accept can act on one of its [`Suggestion`]s, which is always the developer's
//! act and never this module's.
//!
//! **One parser, and it is the only one.** [`Answer::parse`] is what `review-desk message
//! reply --answer` runs, and the page never re-validates or repairs what it was served. A body
//! that breaks a rule is refused whole, naming the first rule it broke, so the session can
//! resend; nothing of it is kept.
//!
//! **Cites are checked for being there, never resolved.** A cite is a string the page links
//! when it can — `F9`, a path and line, `criterion 3`, a commit SHA — and a cite that names
//! nothing the record holds still passes, because resolving it would make the contract
//! depend on which round the record is at.
//!
//! **The closed list of suggestions is [`SuggestionType::ALL`], here and nowhere else.** A
//! test that wants to say "no suggestion signs off, approves or posts" imports it rather
//! than copying six words that can drift.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Result, StoreError};
use crate::model::{string_enum, ReviewLayerName, Severity};

/// The longest a headline may be, in characters.
pub const HEADLINE_MAX: usize = 140;
/// The most points an answer may make.
pub const POINTS_MAX: usize = 5;
/// The longest a point, `not_sure`, `asked` or a one-line suggestion field may be.
pub const LINE_MAX: usize = 200;
/// The most suggestions an answer may carry.
pub const SUGGESTS_MAX: usize = 3;
/// The most activity steps a `working` reply may list.
pub const ACTIVITY_MAX: usize = 6;
/// The longest one activity step may be.
pub const ACTIVITY_LINE_MAX: usize = 60;
/// The longest a suggested finding's title may be: one line of the findings list.
pub const TITLE_MAX: usize = 100;
/// The longest the words of a suggested reply may be.
pub const WORDS_MAX: usize = 600;

string_enum! {
    /// What a reply is: an answer, word that the session is still working, or why it can't.
    AnswerKind as "kind" {
        /// It answers.
        Answer => "answer",
        /// It is still working on the question. **Kept, never counted**, and superseded by
        /// the next parsed reply to the same question.
        Working => "working",
        /// It can't answer: the headline says why, and what would let it.
        Cant => "cant",
    }
}

string_enum! {
    /// How a point is known.
    Basis as "basis" {
        /// The session ran something and saw it.
        Ran => "ran",
        /// The session read it in the code or the record.
        Read => "read",
        /// The session was told it, and did not check.
        Told => "told",
    }
}

string_enum! {
    /// The five tabs of a review, as the shell draws them (DEV-1324). What a message is
    /// about, and what "asked so far" is counted per.
    ReviewTab as "tab" {
        Overview => "overview",
        Understanding => "understanding",
        Findings => "findings",
        Draft => "draft",
        Replies => "replies",
    }
}

impl ReviewTab {
    /// **The tab each of the seven steps now lives on.** The step trail was replaced by the
    /// five tabs (DEV-1324), and a message written before migration 26 recorded only its
    /// step. This is the one copy of the mapping; the migration's backfill spells the same
    /// table in SQL, and a test holds the two together.
    ///
    /// | Step | Tab |
    /// | -- | -- |
    /// | `lint`, `understand`, `alternatives`, `solution_map` | `understanding` |
    /// | `code_review` | `findings` |
    /// | `draft` | `draft` |
    /// | `harvest` | `replies` |
    pub fn for_step(step: ReviewLayerName) -> Self {
        match step {
            ReviewLayerName::Lint
            | ReviewLayerName::Understand
            | ReviewLayerName::Alternatives
            | ReviewLayerName::SolutionMap => ReviewTab::Understanding,
            ReviewLayerName::CodeReview => ReviewTab::Findings,
            ReviewLayerName::Draft => ReviewTab::Draft,
            ReviewLayerName::Harvest => ReviewTab::Replies,
        }
    }
}

string_enum! {
    /// **The closed list of what an answer may suggest, and only these.** Any other `type`
    /// makes the whole answer not parse. None of them signs off, approves or posts:
    /// accepting one records a decision the developer could already make on the page.
    ///
    /// **`answer_question` is not on it.** Patrick dropped it on 21 November 2026 (DEV-1331),
    /// since nothing stores a tick on Overview's questions to accept one into; a reply that
    /// suggests it now fails as an unknown type.
    SuggestionType as "type" {
        NewFinding => "new_finding",
        Regrade => "regrade",
        AddToDraft => "add_to_draft",
        SetAside => "set_aside",
        DraftReply => "draft_reply",
        FollowUp => "follow_up",
    }
}

string_enum! {
    /// The three calls a suggested reply can make on a finding the author answered: the
    /// Replies page's `R`, `A` and `W`, named as the developer's decisions are.
    ReplyCall as "call" {
        RaiseAgain => "raise_again",
        AcceptReason => "accept_reason",
        Withdraw => "withdraw",
    }
}

/// A finding, as an answer names it: `F` and its number on the review.
///
/// Serialised as `"F9"`, the way the session wrote it. **Never resolved here**: the number
/// is checked for being one, not for naming a finding the record holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FindingNumber(pub i64);

impl Serialize for FindingNumber {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("F{}", self.0))
    }
}

impl<'de> Deserialize<'de> for FindingNumber {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        finding_number(&text).map_err(serde::de::Error::custom)
    }
}

/// `F9` as 9, or what was wrong with it.
fn finding_number(text: &str) -> std::result::Result<FindingNumber, String> {
    text.strip_prefix('F')
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse::<i64>().ok())
        .filter(|n| *n >= 1)
        .map(FindingNumber)
        .ok_or_else(|| format!("{text:?} is not a finding: write F and its number, as F9"))
}

/// One point an answer makes, and how it is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub basis: Basis,
    pub text: String,
}

/// What an answer is about, so the page can link it: a step, a finding, or a reply.
///
/// `step` takes a tab's name or one of the seven step names, and keeps the tab — the step
/// names are accepted because a session that knows the old trail still writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerAbout {
    Step(ReviewTab),
    Finding(FindingNumber),
    /// The reply on a finding the author answered, on the Replies tab.
    Reply(FindingNumber),
}

impl AnswerAbout {
    /// Where a message about this sits on the page.
    pub fn place(&self) -> About {
        match self {
            AnswerAbout::Step(tab) => About {
                tab: *tab,
                finding: None,
            },
            AnswerAbout::Finding(n) => About {
                tab: ReviewTab::Findings,
                finding: Some(n.0),
            },
            AnswerAbout::Reply(n) => About {
                tab: ReviewTab::Replies,
                finding: Some(n.0),
            },
        }
    }
}

/// Where a message sits on the page: one tab, and a finding on it when it is about one.
///
/// **Every message has one.** A developer's ask carries it from the page; an answer's own
/// `about` gives it; anything else — every message written before migration 26, and a
/// reply that names nothing — takes the tab its step now lives on ([`ReviewTab::for_step`]).
/// A finding is only ever on `findings` (a finding) or `replies` (the reply on one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    pub tab: ReviewTab,
    pub finding: Option<i64>,
}

impl About {
    /// The tab a step lives on, about no finding.
    pub fn for_step(step: ReviewLayerName) -> Self {
        About {
            tab: ReviewTab::for_step(step),
            finding: None,
        }
    }

    /// Refuse a finding on a tab that has none.
    pub fn checked(self) -> Result<Self> {
        if let Some(n) = self.finding {
            if n < 1 {
                return Err(StoreError::InvalidValue {
                    field: "about",
                    value: n.to_string(),
                    expected: "a finding's number, from 1".to_string(),
                });
            }
            if !matches!(self.tab, ReviewTab::Findings | ReviewTab::Replies) {
                return Err(StoreError::InvalidValue {
                    field: "about",
                    value: format!("{} with F{n}", self.tab),
                    expected: "a finding on findings, or the reply on one on replies; \
                               any other tab is about no finding"
                        .to_string(),
                });
            }
        }
        Ok(self)
    }
}

/// One action an answer suggests, which the developer can accept with one click.
///
/// Each arm carries exactly the fields its row of the doc's table names, and nothing else
/// parses. **None of them signs off, approves or posts.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Suggestion {
    /// Add a finding to *To post*.
    NewFinding {
        severity: Severity,
        title: String,
        /// Where it is: a path and line, as the session would cite it.
        #[serde(rename = "where")]
        at: String,
        basis: Basis,
    },
    /// Regrade a finding. `from` is what the session read; `to` differs from it.
    Regrade {
        finding: FindingNumber,
        from: Severity,
        to: Severity,
        why: String,
    },
    /// Edit a finding's wording in the draft, or add one sentence to it. At least one.
    AddToDraft {
        #[serde(skip_serializing_if = "Option::is_none")]
        finding: Option<FindingNumber>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Set a finding aside.
    SetAside { finding: FindingNumber, why: String },
    /// Fill the reply to an author's answer on Replies.
    DraftReply {
        finding: FindingNumber,
        call: ReplyCall,
        words: String,
    },
    /// Fill the ask box with a question.
    FollowUp { question: String },
}

impl Suggestion {
    /// Which of the closed list this is.
    pub fn suggestion_type(&self) -> SuggestionType {
        match self {
            Suggestion::NewFinding { .. } => SuggestionType::NewFinding,
            Suggestion::Regrade { .. } => SuggestionType::Regrade,
            Suggestion::AddToDraft { .. } => SuggestionType::AddToDraft,
            Suggestion::SetAside { .. } => SuggestionType::SetAside,
            Suggestion::DraftReply { .. } => SuggestionType::DraftReply,
            Suggestion::FollowUp { .. } => SuggestionType::FollowUp,
        }
    }
}

/// A session's reply in the shape of the contract, as it was checked.
///
/// Stored as JSON beside the reply's raw body and read back as this. Optional fields left
/// out are omitted when it is written again, so what the page is served is what was sent,
/// trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub kind: AnswerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<AnswerAbout>,
    pub headline: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cites: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_sure: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggests: Vec<Suggestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<String>,
    /// **The session's own account of what it was asked**, for one-way mode, where no
    /// developer message exists to answer. Never stored as a developer message, and drawn
    /// as the session's: *"Your question, as your session recorded it"*.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<String>,
}

/// The fields an answer may have, in the order they are checked.
const ANSWER_FIELDS: &[&str] = &[
    "kind", "about", "headline", "points", "cites", "not_sure", "suggests", "activity", "asked",
];

/// A refusal naming the field and the rule it broke.
fn broken(field: impl Into<String>, rule: impl Into<String>) -> StoreError {
    StoreError::NotTheContract {
        field: field.into(),
        rule: rule.into(),
    }
}

impl Answer {
    /// Parse a reply's body as the contract, or refuse it naming the first rule it broke.
    ///
    /// The rules are checked field by field in the doc's order, then the two that span
    /// fields: cites when the answer claims anything, and activity only while working.
    pub fn parse(body: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(body).map_err(|error| {
            broken(
                "body",
                format!("the body is one JSON object, and this is not JSON: {error}"),
            )
        })?;
        let Value::Object(object) = value else {
            return Err(broken("body", "the body is one JSON object"));
        };
        Self::from_object(&object)
    }

    fn from_object(object: &Map<String, Value>) -> Result<Self> {
        no_other_fields(object, ANSWER_FIELDS, "")?;

        let kind = closed::<AnswerKind>(object.get("kind"), "kind", true)?
            .expect("a required field is present");
        let about = object.get("about").map(parse_about).transpose()?;
        let headline = line(object.get("headline"), "headline", HEADLINE_MAX)?
            .ok_or_else(|| broken("headline", "an answer always has a headline"))?;

        let points = list(object.get("points"), "points", POINTS_MAX)?
            .iter()
            .enumerate()
            .map(|(i, point)| parse_point(point, &format!("points[{i}]")))
            .collect::<Result<Vec<_>>>()?;

        let cites = match object.get("cites") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(i, cite)| {
                    let field = format!("cites[{i}]");
                    match cite {
                        Value::String(text) if !text.trim().is_empty() => {
                            Ok(text.trim().to_string())
                        }
                        Value::String(_) => Err(broken(field, "a cite names something")),
                        _ => Err(broken(field, "a cite is a string")),
                    }
                })
                .collect::<Result<Vec<_>>>()?,
            Some(_) => return Err(broken("cites", "cites is a list of strings")),
        };

        let not_sure = line(object.get("not_sure"), "not_sure", LINE_MAX)?;

        let suggests = list(object.get("suggests"), "suggests", SUGGESTS_MAX)?
            .iter()
            .enumerate()
            .map(|(i, one)| parse_suggestion(one, &format!("suggests[{i}]")))
            .collect::<Result<Vec<_>>>()?;

        let activity = list(object.get("activity"), "activity", ACTIVITY_MAX)?
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let field = format!("activity[{i}]");
                line(Some(step), &field, ACTIVITY_LINE_MAX)?
                    .ok_or_else(|| broken(field, "an activity step says something"))
            })
            .collect::<Result<Vec<_>>>()?;

        let asked = line(object.get("asked"), "asked", LINE_MAX)?;

        let answer = Answer {
            kind,
            about,
            headline,
            points,
            cites,
            not_sure,
            suggests,
            activity,
            asked,
        };
        if answer.claims() && answer.cites.is_empty() {
            return Err(broken(
                "cites",
                "an answer that claims anything cites at least one thing: a finding, a \
                 path and line, a criterion, an option or a commit",
            ));
        }
        match (answer.kind, answer.activity.is_empty()) {
            (AnswerKind::Working, true) => {
                return Err(broken(
                    "activity",
                    "a working reply says what it is doing now, in 1 to 6 steps",
                ))
            }
            (AnswerKind::Answer | AnswerKind::Cant, false) => {
                return Err(broken(
                    "activity",
                    format!(
                        "activity is only for a working reply, and this is {}",
                        answer.kind
                    ),
                ))
            }
            _ => {}
        }
        Ok(answer)
    }

    /// **Whether this answer claims anything**, and so must cite.
    ///
    /// An `answer` always does: its headline is the answer. A `working` or `cant` reply
    /// claims something only once it makes a point or suggests an action.
    pub fn claims(&self) -> bool {
        self.kind == AnswerKind::Answer || !self.points.is_empty() || !self.suggests.is_empty()
    }

    /// This answer as it is stored: JSON, with the fields it left out omitted.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an answer serialises")
    }
}

/// Refuse a field the contract does not name.
fn no_other_fields(object: &Map<String, Value>, known: &[&str], within: &str) -> Result<()> {
    if let Some(other) = object.keys().find(|key| !known.contains(&key.as_str())) {
        let field = if within.is_empty() {
            other.clone()
        } else {
            format!("{within}.{other}")
        };
        return Err(broken(
            field,
            format!(
                "not a field of the contract; the fields here are {}",
                known.join(", ")
            ),
        ));
    }
    Ok(())
}

/// A value from a closed set, required or not.
fn closed<T: std::str::FromStr<Err = StoreError>>(
    value: Option<&Value>,
    field: &str,
    required: bool,
) -> Result<Option<T>> {
    match value {
        None if required => Err(broken(field, "required")),
        None => Ok(None),
        Some(Value::String(text)) => text.parse::<T>().map(Some).map_err(|error| match error {
            StoreError::InvalidValue { expected, .. } => {
                broken(field, format!("{text:?} is not one of: {expected}"))
            }
            other => other,
        }),
        Some(_) => Err(broken(field, "a string")),
    }
}

/// One line of at most `max` characters, trimmed, or `None` when the field is absent.
///
/// **"One sentence" is checked as one line.** Whether a line is one sentence is not
/// something a parser can say without guessing at full stops in `ExportController.php`.
fn line(value: Option<&Value>, field: &str, max: usize) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(broken(field, "says something: leave the field out instead"));
            }
            if text.contains(['\n', '\r']) {
                return Err(broken(field, "one line"));
            }
            let length = text.chars().count();
            if length > max {
                return Err(broken(
                    field,
                    format!("at most {max} characters, and this is {length}"),
                ));
            }
            Ok(Some(text.to_string()))
        }
        Some(_) => Err(broken(field, "a string")),
    }
}

/// A list of at most `max` items, or an empty one when the field is absent.
fn list<'a>(value: Option<&'a Value>, field: &str, max: usize) -> Result<&'a [Value]> {
    match value {
        None => Ok(&[]),
        Some(Value::Array(items)) if items.len() > max => Err(broken(
            field,
            format!("at most {max} items, and this has {}", items.len()),
        )),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(broken(field, "a list")),
    }
}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| broken(field, "an object"))
}

fn parse_point(value: &Value, field: &str) -> Result<Point> {
    let point = object(value, field)?;
    no_other_fields(point, &["basis", "text"], field)?;
    let basis =
        closed::<Basis>(point.get("basis"), &format!("{field}.basis"), true)?.expect("required");
    let text = line(point.get("text"), &format!("{field}.text"), LINE_MAX)?
        .ok_or_else(|| broken(format!("{field}.text"), "required"))?;
    Ok(Point { basis, text })
}

fn parse_about(value: &Value) -> Result<AnswerAbout> {
    let about = object(value, "about")?;
    no_other_fields(about, &["step", "finding", "reply"], "about")?;
    if about.len() != 1 {
        return Err(broken(
            "about",
            "exactly one of step, finding or reply: an answer is about one thing",
        ));
    }
    let (key, value) = about.iter().next().expect("one entry");
    let Value::String(text) = value else {
        return Err(broken(format!("about.{key}"), "a string"));
    };
    let field = format!("about.{key}");
    match key.as_str() {
        "step" => text
            .parse::<ReviewTab>()
            .or_else(|_| text.parse::<ReviewLayerName>().map(ReviewTab::for_step))
            .map(AnswerAbout::Step)
            .map_err(|_| {
                broken(
                    field,
                    format!(
                        "{text:?} is not a tab ({}) or a step ({})",
                        ReviewTab::valid_values(),
                        ReviewLayerName::valid_values()
                    ),
                )
            }),
        "finding" => finding_number(text)
            .map(AnswerAbout::Finding)
            .map_err(|rule| broken(field, rule)),
        _ => finding_number(text)
            .map(AnswerAbout::Reply)
            .map_err(|rule| broken(field, rule)),
    }
}

fn finding_field(object: &Map<String, Value>, field: &str) -> Result<FindingNumber> {
    match object.get("finding") {
        None => Err(broken(format!("{field}.finding"), "required")),
        Some(Value::String(text)) => {
            finding_number(text).map_err(|rule| broken(format!("{field}.finding"), rule))
        }
        Some(_) => Err(broken(format!("{field}.finding"), "a string, as F9")),
    }
}

fn required_line(
    object: &Map<String, Value>,
    name: &str,
    field: &str,
    max: usize,
) -> Result<String> {
    let path = format!("{field}.{name}");
    line(object.get(name), &path, max)?.ok_or_else(|| broken(path, "required"))
}

fn required_closed<T: std::str::FromStr<Err = StoreError>>(
    object: &Map<String, Value>,
    name: &str,
    field: &str,
) -> Result<T> {
    Ok(closed::<T>(object.get(name), &format!("{field}.{name}"), true)?.expect("required"))
}

/// One suggestion, checked against its own row of the closed list.
fn parse_suggestion(value: &Value, field: &str) -> Result<Suggestion> {
    let one = object(value, field)?;
    let kind: SuggestionType = required_closed(one, "type", field)?;
    let fields: &[&str] = match kind {
        SuggestionType::NewFinding => &["type", "severity", "title", "where", "basis"],
        SuggestionType::Regrade => &["type", "finding", "from", "to", "why"],
        SuggestionType::AddToDraft => &["type", "finding", "text"],
        SuggestionType::SetAside => &["type", "finding", "why"],
        SuggestionType::DraftReply => &["type", "finding", "call", "words"],
        SuggestionType::FollowUp => &["type", "question"],
    };
    no_other_fields(one, fields, field)?;
    Ok(match kind {
        SuggestionType::NewFinding => Suggestion::NewFinding {
            severity: required_closed(one, "severity", field)?,
            title: required_line(one, "title", field, TITLE_MAX)?,
            at: required_line(one, "where", field, LINE_MAX)?,
            basis: required_closed(one, "basis", field)?,
        },
        SuggestionType::Regrade => {
            let finding = finding_field(one, field)?;
            let from: Severity = required_closed(one, "from", field)?;
            let to: Severity = required_closed(one, "to", field)?;
            if from == to {
                return Err(broken(
                    format!("{field}.to"),
                    format!("a regrade changes the severity, and this is {from} to {to}"),
                ));
            }
            Suggestion::Regrade {
                finding,
                from,
                to,
                why: required_line(one, "why", field, LINE_MAX)?,
            }
        }
        SuggestionType::AddToDraft => {
            let finding = one
                .contains_key("finding")
                .then(|| finding_field(one, field))
                .transpose()?;
            let text = line(one.get("text"), &format!("{field}.text"), LINE_MAX)?;
            if finding.is_none() && text.is_none() {
                return Err(broken(
                    field,
                    "add_to_draft names a finding, or carries one sentence, or both",
                ));
            }
            Suggestion::AddToDraft { finding, text }
        }
        SuggestionType::SetAside => Suggestion::SetAside {
            finding: finding_field(one, field)?,
            why: required_line(one, "why", field, LINE_MAX)?,
        },
        SuggestionType::DraftReply => {
            let finding = finding_field(one, field)?;
            let call: ReplyCall = required_closed(one, "call", field)?;
            let path = format!("{field}.words");
            let words = match one.get("words") {
                None => return Err(broken(path, "required")),
                Some(Value::String(text)) if text.trim().is_empty() => {
                    return Err(broken(path, "says something"))
                }
                Some(Value::String(text)) if text.trim().chars().count() > WORDS_MAX => {
                    return Err(broken(path, format!("at most {WORDS_MAX} characters")))
                }
                Some(Value::String(text)) => text.trim().to_string(),
                Some(_) => return Err(broken(path, "a string")),
            };
            Suggestion::DraftReply {
                finding,
                call,
                words,
            }
        }
        SuggestionType::FollowUp => Suggestion::FollowUp {
            question: required_line(one, "question", field, LINE_MAX)?,
        },
    })
}
