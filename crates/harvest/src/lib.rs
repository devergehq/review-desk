//! Reads the outcomes of a posted review back from GitHub and records them.
//!
//! Four facts, and nothing else:
//!
//! | Fact | Recorded as |
//! | -- | -- |
//! | The pull request was merged or closed | A state and a time on the review |
//! | Someone replied to a finding | An outcome of kind `replied`, with who and the verbatim text |
//! | A finding's thread was resolved | An outcome of kind `thread_resolved` |
//! | The file a finding points at changed later | An outcome of kind `code_changed_after` |
//!
//! **What it found goes on step 7 of the review's trail**, which is DEV-933: `running`
//! while the pull request is open, `done` once it has merged or closed, and the counts as
//! the step's facts. Until then only "posted" wrote that step, so a review harvested every
//! quarter of an hour went on saying it was waiting for a harvest.
//!
//! **The harvester does not decide what a reply meant.** Reading a reply as an
//! acceptance or a rejection needs judgement; this crate is deterministic, so it stores
//! the reply as written and leaves the reading to a person or an agent.
//!
//! GitHub sits behind the [`GitHub`] trait. [`gh::Gh`] runs the `gh` command, so the
//! developer's existing login is used and no token is handled here; [`recorded::Recorded`]
//! replays responses captured from a real pull request, which is how the tests run with
//! no network.
//!
//! ```no_run
//! use review_desk_harvest::{gh::Gh, harvest, Selection};
//! use review_desk_store::Store;
//!
//! let store = Store::open("/tmp/review-desk.db")?;
//! let report = harvest(&store, &Gh::new(), Selection::everything())?;
//! println!("{}", report.summary());
//! # Ok::<(), review_desk_harvest::HarvestError>(())
//! ```

#![deny(missing_debug_implementations)]

pub mod gh;
pub mod github;
pub mod patch;
pub mod recorded;
pub mod rounds;
pub mod run;
pub mod wire;

mod error;

pub use crate::error::{HarvestError, Result};
pub use crate::github::{
    ChangedFile, Commit, CommitFiles, Comparison, GitHub, PullRequest, Reply, ReviewThread,
};
pub use crate::rounds::commits_between;
pub use crate::run::{harvest, Report, ReviewReport, Selection, StepSeven};
