//! The store's error type.
//!
//! Callers must be able to tell a refused request from a broken Review Desk without
//! reading message text, so constraint failures get their own variants rather than
//! arriving as an opaque SQLite error.

use std::path::PathBuf;

/// Everything the store can fail with.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A review already exists for this repository, pull request and round.
    #[error("review already exists for {repo}#{pr} round {round}")]
    DuplicateReview { repo: String, pr: i64, round: i64 },

    /// A GitHub review comment is already linked to a different finding.
    #[error("GitHub comment {comment_id} is already linked to finding {review_id}/{seq}")]
    DuplicateGithubComment {
        comment_id: String,
        review_id: i64,
        seq: i64,
    },

    /// A draft is not at a status the change asked for can be made from.
    ///
    /// Its own variant rather than an `InvalidValue`, because nothing the caller typed
    /// was invalid: the request was understood and the record is simply not in a state
    /// that allows it. A caller branching on `code` can tell the two apart.
    #[error("the draft of review {review_id} is {found}, and {wanted}")]
    DraftStatus {
        review_id: i64,
        found: crate::model::DraftStatus,
        wanted: &'static str,
    },

    /// A decision was made about a finding the draft is not composed of.
    ///
    /// Its own variant, because it is the one refusal a screen has something to say about:
    /// the draft was written as one block of markdown, so there is no paragraph of it that
    /// is F6's and nothing can cut one without guessing which characters to remove. A
    /// session that writes the draft in parts gets the buttons. Every draft written before
    /// migration 9 is in this state.
    #[error(
        "the draft of review {review_id} has no block for F{seq}, so there is nothing to \
         decide about: it was written as one block of markdown rather than in parts"
    )]
    NoDraftSection { review_id: i64, seq: i64 },

    /// A review is already dismissed, so a second dismissal would overwrite the time and
    /// the words of the one the developer actually made.
    ///
    /// **No time in the message.** The store holds times as integers and the server
    /// converts them at its own boundary, so a timestamp here would reach the developer
    /// as `1791780818`. When it was dismissed is on the review, which is what the page
    /// re-reads.
    #[error(
        "review {review_id} is already dismissed; restore it first to dismiss it again \
         with different words"
    )]
    AlreadyDismissed { review_id: i64 },

    /// An act that needs an agent afterwards was asked for on a dismissed review.
    #[error(
        "review {review_id} is dismissed, so no session will be started for it and \
         nothing would come of {act}; bring it back first"
    )]
    DismissedReview { review_id: i64, act: &'static str },

    /// An act that needs an agent afterwards was asked for on a review whose pull request
    /// has merged or closed.
    ///
    /// **Its own variant rather than a reworded dismissal**, because the two are different
    /// facts and DEV-957 asks for them to be told apart everywhere: nobody dismissed this
    /// review, and there is nothing to bring back. `how` is the word GitHub used, so the
    /// message reads "merged" or "closed" rather than a term of Review Desk's own.
    #[error(
        "the pull request of review {review_id} {how}, so nothing would come of {act}: no \
         round of a {how} pull request can be posted"
    )]
    FinishedReview {
        review_id: i64,
        how: &'static str,
        act: &'static str,
    },

    /// An act that needs an agent afterwards was asked for on an unposted round a newer
    /// round has superseded.
    #[error(
        "review {review_id} is round {round}, and round {by_round} of the same pull \
         request is newer, so nothing would come of {act}: this round was never posted \
         and the newest round is the one that can be"
    )]
    SupersededRound {
        review_id: i64,
        round: i64,
        by_round: i64,
        act: &'static str,
    },

    /// A review that is not dismissed cannot be brought back.
    #[error("review {review_id} is not dismissed, so there is nothing to bring back")]
    NotDismissed { review_id: i64 },

    /// A review holds work a delete would lose.
    ///
    /// **`lost` is "that" or "them", and it is a field rather than a word in the
    /// sentence.** "review 7 holds 2 findings, and deleting it would lose that" is what
    /// the one-word version said, and this message is not only read by the page: the page
    /// hides the button, so the refusal is what a developer sees on the race the hidden
    /// button cannot cover — a finding added between the page's read and the `DELETE`.
    /// `contract/README.md` says the API refuses in the same words the card uses, and
    /// [`ReviewWork::in_words`] is beside the thing that decides this one so the two
    /// cannot drift.
    #[error(
        "review {review_id} holds {holds}, and deleting it would lose {lost}: dismiss it \
         instead, which takes it out of the queue and keeps the record"
    )]
    ReviewStarted {
        review_id: i64,
        holds: String,
        lost: &'static str,
    },

    /// A row was requested, or referenced, that is not there.
    /// The developer acted on something that has changed since it was shown to them.
    ///
    /// DEV-1346. Nothing was written: the act was about what the page showed, and that is
    /// no longer what the record holds. The page reads the draft again and shows the change.
    #[error(
        "the {what} of review {review_id} changed while you were reading it: {moved}. \
         Nothing was written; read it again and review the change"
    )]
    ShownMoved {
        what: &'static str,
        review_id: i64,
        moved: String,
    },

    /// A post of this review is already in flight — DEV-1347.
    ///
    /// `review-desk post` records the attempt before it calls GitHub, and a `pending` row is
    /// the one state that cannot say whether the review went up: the call may have been made
    /// and its answer lost. Posting again could put the same review up twice under the
    /// developer's name, so it is refused until the attempt is known to have failed.
    #[error(
        "review {review_id} already has a post in flight (attempt {tag_id}, recorded at \
         {since}), and it may have reached GitHub. Nothing was posted. Look for the review on \
         the pull request before trying again"
    )]
    PostInFlight {
        review_id: i64,
        tag_id: i64,
        since: i64,
    },

    /// A thread's line was measured on a commit that is not the one being posted — DEV-1347.
    ///
    /// `review-desk post` pins every comment to the reviewed head. A line counted on an
    /// earlier commit would land on whatever that number is now, silently, so the post is
    /// refused until the session re-reads the diff and records the line again.
    #[error(
        "F{finding_seq}'s line in review {review_id} was measured on {line_sha}, and the \
         review is posted at {head_sha}, so its comment could land on the wrong line. Nothing \
         was posted. Re-read the diff and record the finding's line on the head"
    )]
    ThreadOffHead {
        review_id: i64,
        finding_seq: i64,
        line_sha: String,
        head_sha: String,
    },

    /// The operating system's randomness could not be read, so no salt could be made.
    #[error("cannot read randomness from the operating system: {0}")]
    NoRandomness(#[source] std::io::Error),

    #[error("no {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    /// A row referenced a parent row that does not exist.
    #[error("{entity} refers to {parent} {id}, which does not exist")]
    UnknownParent {
        entity: &'static str,
        parent: &'static str,
        id: String,
    },

    /// A value read from the database is not one this version of the code knows.
    #[error("{field} holds {value:?}, which is not a valid {field}; expected one of: {expected}")]
    InvalidValue {
        field: &'static str,
        value: String,
        expected: String,
    },

    /// The database could not be opened, or its directory could not be created.
    #[error("cannot open database at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A migration failed to apply.
    #[error("migration {version} ({name}) failed: {source}")]
    Migration {
        version: i64,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },

    /// The database is newer than this binary understands.
    #[error("database is at version {found}, but this build only knows up to {known}")]
    FromTheFuture { found: i64, known: i64 },

    /// No home directory could be determined, so the default path is unknowable.
    #[error("cannot determine the home directory; set REVIEW_DESK_DB to choose a database")]
    NoHome,

    /// Anything else SQLite reports.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// The store's result alias.
pub type Result<T> = std::result::Result<T, StoreError>;

impl StoreError {
    /// A short, stable name for this failure, for a caller that must branch on the
    /// reason rather than read the message.
    ///
    /// The exit code already says refused or broken; this says which refusal. It is
    /// part of the command line's JSON contract, so a new variant must be given a name
    /// here rather than falling through to a catch-all.
    pub fn code(&self) -> &'static str {
        match self {
            StoreError::DuplicateReview { .. } => "duplicate_review",
            StoreError::DuplicateGithubComment { .. } => "duplicate_github_comment",
            StoreError::DraftStatus { .. } => "draft_status",
            StoreError::NoDraftSection { .. } => "no_draft_section",
            StoreError::AlreadyDismissed { .. } => "already_dismissed",
            StoreError::DismissedReview { .. } => "dismissed_review",
            StoreError::FinishedReview { .. } => "finished_review",
            StoreError::SupersededRound { .. } => "superseded_round",
            StoreError::NotDismissed { .. } => "not_dismissed",
            StoreError::ReviewStarted { .. } => "review_started",
            StoreError::ShownMoved { .. } => "changed_since_shown",
            StoreError::PostInFlight { .. } => "post_in_flight",
            StoreError::NoRandomness(_) => "no_randomness",
            StoreError::ThreadOffHead { .. } => "thread_off_head",
            StoreError::NotFound { .. } => "not_found",
            StoreError::UnknownParent { .. } => "unknown_parent",
            StoreError::InvalidValue { .. } => "invalid_value",
            StoreError::Open { .. } => "cannot_open_database",
            StoreError::Migration { .. } => "migration_failed",
            StoreError::FromTheFuture { .. } => "database_from_the_future",
            StoreError::NoHome => "no_home",
            StoreError::Sqlite(_) => "database_error",
        }
    }

    /// True when the request was understood and refused, rather than Review Desk
    /// itself being broken. DEV-851's exit codes rest on this split: refused is 1,
    /// broken is 2.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            StoreError::DuplicateReview { .. }
                | StoreError::DuplicateGithubComment { .. }
                | StoreError::DraftStatus { .. }
                | StoreError::NoDraftSection { .. }
                | StoreError::AlreadyDismissed { .. }
                | StoreError::DismissedReview { .. }
                | StoreError::FinishedReview { .. }
                | StoreError::SupersededRound { .. }
                | StoreError::NotDismissed { .. }
                | StoreError::ReviewStarted { .. }
                | StoreError::ShownMoved { .. }
                | StoreError::PostInFlight { .. }
                | StoreError::ThreadOffHead { .. }
                | StoreError::NotFound { .. }
                | StoreError::UnknownParent { .. }
                | StoreError::InvalidValue { .. }
        )
    }
}
