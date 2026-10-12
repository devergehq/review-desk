//! `harvest`: read the outcomes of posted reviews back from GitHub.
//!
//! Everything harvesting does lives in `review-desk-harvest`. This module turns
//! arguments into a [`Selection`] and the result into a [`Report`]. It chooses no exit
//! code and prints nothing: `main` does both for every command, which is what keeps the
//! `--json` contract in one place.
//!
//! # `--from`, and why it is not a test-only switch bolted on
//!
//! GitHub sits behind one trait with four read-only methods, and `review-desk-harvest`
//! has shipped two of them since the first slice: [`Gh`] runs the `gh` command, and
//! `recorded::Recorded` replays responses captured off a real pull request — which the
//! crate's own header calls "how the tests run with no network". `--from` is the second
//! one reachable from the command line.
//!
//! **DEV-957 is what needed it.** That issue asks for a record shaped like Patrick's —
//! four rounds of one merged pull request — measured in a browser, and the end-to-end seed
//! goes through this binary and writes no SQL, deliberately. Nothing on the command line
//! records a pull request's state: the harvest is the only writer, which the issue says is
//! right. So either the harvest becomes reachable without a network, or the seed grows a
//! second way to write `pr_state` and the harvest stops being the only writer. This is the
//! first.
//!
//! It is the same code path in every other respect: the same `harvest`, the same writes,
//! the same step 7. Only where the four reads come from is chosen.

use std::path::PathBuf;

use clap::Args;
use review_desk_harvest::{gh::Gh, harvest, recorded::Recorded, GitHub, Selection};
use review_desk_store::Store;
use serde_json::json;

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Args)]
pub struct HarvestArgs {
    /// Only this repository, as OWNER/REPO.
    #[arg(long, value_name = "OWNER/REPO")]
    repo: Option<String>,

    /// Only this review. It is read whatever state its pull request is in, while the
    /// unnarrowed run leaves a merged pull request alone.
    #[arg(long, value_name = "ID")]
    review: Option<i64>,

    /// Read GitHub's answers from a directory of recorded responses instead of calling
    /// `gh`. Nothing leaves the machine. For tests and for seeding a record.
    #[arg(long, value_name = "DIR")]
    from: Option<PathBuf>,
}

pub fn run(args: &HarvestArgs) -> Result<Report> {
    let store = Store::open_default()?;
    // One of the two readers, chosen here and nowhere else. Boxed because they are
    // different types behind one trait, which is the whole point of the trait.
    let github: Box<dyn GitHub> = match &args.from {
        Some(dir) => Box::new(Recorded::from_dir(dir)),
        None => Box::new(Gh::new()),
    };
    let report = harvest(
        &store,
        github.as_ref(),
        Selection {
            repo: args.repo.clone(),
            review: args.review,
        },
    )?;

    let mut plain = vec![report.summary()];
    plain.extend(report.lines());

    Ok(Report::new(json!({ "harvest": report }), plain.join("\n")))
}
