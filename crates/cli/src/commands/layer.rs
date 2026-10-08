//! `layer set`: what one of a review's seven layers is doing.

use clap::Subcommand;
use review_desk_store::{ReviewLayerName, ReviewLayerState, Store};
use serde_json::json;

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum LayerCommand {
    /// Record what one of the review's seven layers is doing.
    ///
    /// The layers are, in order: 1 lint, 2 understand, 3 alternatives, 4 solution_map,
    /// 5 code_review, 6 draft, 7 harvest. A layer's number fixes its name, so only the
    /// number is given.
    ///
    /// Writing the same state twice is the same as writing it once, so a layer can
    /// report progress without checking first.
    Set {
        /// The review's id.
        #[arg(long, value_name = "ID")]
        review: i64,
        /// The layer's number, from 1 to 7.
        #[arg(long, value_name = "N")]
        layer: i64,
        /// One of: pending, running, done, waiting_on_you, failed, skipped.
        #[arg(long, value_name = "VALUE")]
        state: String,
        /// The short phrase shown after the layer's name: "passed", "3 options",
        /// "6 findings".
        #[arg(long, value_name = "TEXT")]
        detail: Option<String>,
    },
}

pub fn run(command: &LayerCommand) -> Result<Report> {
    match command {
        LayerCommand::Set {
            review,
            layer,
            state,
            detail,
        } => {
            // Parsed here rather than by clap, so an unknown state is refused as an
            // invalid value — with the valid ones named — instead of as a usage error.
            let state: ReviewLayerState = state.parse()?;

            let store = Store::open_default()?;
            let written = store.set_review_layer(*review, *layer, state, detail.as_deref())?;

            let mut plain = format!(
                "review {} layer {} ({}) is {}",
                written.review_id, written.number, written.name, written.state
            );
            if let Some(detail) = &written.detail {
                plain.push_str(&format!(": {detail}"));
            }

            Ok(Report::new(
                json!({ "layer": written, "layers_total": ReviewLayerName::COUNT }),
                plain,
            ))
        }
    }
}
