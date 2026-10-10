//! The command groups.
//!
//! One module per group, one variant in [`Command`] and one arm in [`run`]. A new group
//! is a three-line change in this file and a file of its own, which is what keeps two
//! sessions adding commands at the same time out of each other's way.

mod brief;
mod db;
mod draft;
mod finding;
mod harvest;
mod layer;
mod outcome;
mod provenance;
mod review;
mod round;
mod rules;
mod serve;
mod work;

use clap::{Parser, Subcommand};

use crate::error::Result;
use crate::report::Report;

/// The whole command line.
#[derive(Debug, Parser)]
#[command(
    name = "review-desk",
    version,
    about = "The record of an agent-assisted code review."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Print one JSON document on standard output and nothing else.
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect the database.
    Db {
        #[command(subcommand)]
        command: db::DbCommand,
    },
    /// Open, link and read a review.
    Review {
        #[command(subcommand)]
        command: review::ReviewCommand,
    },
    /// Store a review's brief. Confirming one is done in the dashboard.
    Brief {
        #[command(subcommand)]
        command: brief::BriefCommand,
    },
    /// Record what one of a review's seven layers is doing.
    Layer {
        #[command(subcommand)]
        command: layer::LayerCommand,
    },
    /// Write the review draft, read its status, and report it posted.
    Draft {
        #[command(subcommand)]
        command: draft::DraftCommand,
    },
    /// Add findings and change what is to become of them.
    Finding {
        #[command(subcommand)]
        command: finding::FindingCommand,
    },
    /// Record what became of a finding.
    Outcome {
        #[command(subcommand)]
        command: outcome::OutcomeCommand,
    },
    /// Record where the review's understanding came from: its sources, the ticket's
    /// criteria, the description lint's results and the per-file change map.
    Provenance {
        #[command(subcommand)]
        command: provenance::ProvenanceCommand,
    },
    /// Record what changed between two rounds, and read the rounds so far.
    Round {
        #[command(subcommand)]
        command: round::RoundCommand,
    },
    /// Read the rules in force.
    Rules {
        #[command(subcommand)]
        command: rules::RulesCommand,
    },
    /// Read the outcomes of posted reviews back from GitHub and record them.
    Harvest(harvest::HarvestArgs),
    /// Serve the JSON API and the dashboard on 127.0.0.1.
    Serve(serve::ServeArgs),
    /// Ask what is waiting for an agent.
    Work {
        #[command(subcommand)]
        command: work::WorkCommand,
    },
}

/// Run whatever was asked for, and hand back what it has to say.
pub fn run(cli: &Cli) -> Result<Report> {
    match &cli.command {
        Command::Db { command } => db::run(command),
        Command::Review { command } => review::run(command),
        Command::Brief { command } => brief::run(command),
        Command::Layer { command } => layer::run(command),
        Command::Draft { command } => draft::run(command),
        Command::Finding { command } => finding::run(command),
        Command::Outcome { command } => outcome::run(command),
        Command::Provenance { command } => provenance::run(command),
        Command::Round { command } => round::run(command),
        Command::Rules { command } => rules::run(command),
        Command::Harvest(args) => harvest::run(args),
        Command::Serve(args) => serve::run(args, cli.json),
        Command::Work { command } => work::run(command),
    }
}
