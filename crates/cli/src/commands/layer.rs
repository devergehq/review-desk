//! `layer set`: what one of a review's seven layers is doing.

use clap::Subcommand;
use review_desk_store::{NewLayerFact, ReviewLayerName, ReviewLayerState, Store};
use serde_json::json;

use crate::error::{CliError, Result};
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
        ///
        /// **One line, and the screen draws it on one line.** It is ellipsised rather
        /// than wrapped, so a sentence written here is a sentence the reader cannot
        /// finish; what the step came to in detail goes in `--fact`.
        #[arg(long, value_name = "TEXT")]
        detail: Option<String>,
        /// One fact the step recorded, as `label=value`. Repeat it for each.
        ///
        /// These are what the card beside the step draws, as two columns:
        /// `--fact "Problems=3" --fact "Your sign-off=not required"`. Split on the
        /// first `=` alone, so a value may contain others.
        ///
        /// **Giving none leaves the facts as they were**, because this command is also
        /// the one a session calls to report progress and would otherwise clear them by
        /// saying nothing about them. Giving any replaces the whole list;
        /// `--clear-facts` empties it.
        #[arg(long = "fact", value_name = "LABEL=VALUE")]
        facts: Vec<String>,
        /// Take every fact off this step.
        #[arg(long, conflicts_with = "facts")]
        clear_facts: bool,
    },
}

/// One `label=value`, split on the first `=` alone.
///
/// A value may contain an `=` — "Blind pass picked=C, without the second page" is one
/// fact and so is `Flag=--no-verify=false`. A label may not, which is the half that is
/// two or three words.
fn fact(given: &str) -> Result<NewLayerFact> {
    // **A bad-input refusal rather than the store's `InvalidValue`.** The store never saw
    // this string: it is a flag this command could not read, which is what `BadInput` is
    // for — and `InvalidValue` words itself as "expected one of", which is right for a
    // closed set of values and wrong for a shape.
    let (label, value) = given
        .split_once('=')
        .ok_or_else(|| CliError::bad_input("--fact", format!("{given:?} is not label=value")))?;
    Ok(NewLayerFact {
        label: label.to_string(),
        value: value.to_string(),
    })
}

pub fn run(command: &LayerCommand) -> Result<Report> {
    match command {
        LayerCommand::Set {
            review,
            layer,
            state,
            detail,
            facts,
            clear_facts,
        } => {
            // Parsed here rather than by clap, so an unknown state is refused as an
            // invalid value — with the valid ones named — instead of as a usage error.
            let state: ReviewLayerState = state.parse()?;

            // Parsed before the database is opened, so a malformed fact does not create
            // a database file as a side effect — the rule `provenance sources` follows
            // for its document.
            let given = facts
                .iter()
                .map(|one| fact(one))
                .collect::<Result<Vec<_>>>()?;

            let store = Store::open_default()?;
            let written = store.set_review_layer(*review, *layer, state, detail.as_deref())?;

            // **Written after the state, and only when this call says something about
            // them.** The step has to exist before its facts can hang off it, which the
            // line above guarantees.
            let facts = if *clear_facts || !given.is_empty() {
                store.put_review_layer_facts(*review, *layer, given)?
            } else {
                written.facts.clone()
            };

            let mut plain = format!(
                "review {} layer {} ({}) is {}",
                written.review_id, written.number, written.name, written.state
            );
            if let Some(detail) = &written.detail {
                plain.push_str(&format!(": {detail}"));
            }
            if !facts.is_empty() {
                plain.push_str(&format!(" ({})", counted(facts.len(), "fact", "facts")));
            }

            // The layer as it now stands, facts included: the write above left the state
            // and this write left the facts, and a caller reading one answer should not
            // have to know that it took two statements.
            let answer = review_desk_store::ReviewLayer {
                facts: facts.clone(),
                ..written
            };

            Ok(Report::new(
                json!({ "layer": answer, "layers_total": ReviewLayerName::COUNT }),
                plain,
            ))
        }
    }
}

/// "1 fact", "3 facts". The same shape `provenance` prints its lists in.
fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
