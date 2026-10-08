//! `harvest`: read the outcomes of posted reviews back from GitHub.
//!
//! Everything harvesting does lives in `review-desk-harvest`. This module turns
//! arguments into a [`Selection`] and the result into a [`Report`]. It chooses no exit
//! code and prints nothing: `main` does both for every command, which is what keeps the
//! `--json` contract in one place.

use clap::Args;
use review_desk_harvest::{gh::Gh, harvest, Selection};
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
    /// unnarrowed run leaves a merged or closed pull request alone.
    #[arg(long, value_name = "ID")]
    review: Option<i64>,
}

pub fn run(args: &HarvestArgs) -> Result<Report> {
    let store = Store::open_default()?;
    let report = harvest(
        &store,
        &Gh::new(),
        Selection {
            repo: args.repo.clone(),
            review: args.review,
        },
    )?;

    let mut plain = vec![report.summary()];
    plain.extend(report.lines());

    Ok(Report::new(json!({ "harvest": report }), plain.join("\n")))
}
