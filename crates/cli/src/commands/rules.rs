//! `rules list`.

use clap::Subcommand;
use review_desk_store::{Rule, Store};
use serde_json::json;

use crate::error::Result;
use crate::report::Report;

#[derive(Debug, Subcommand)]
pub enum RulesCommand {
    /// Print the rules that apply to a repository: the global ones and its own.
    ///
    /// Only rules in force are printed — active, lint or CI. A proposed rule is not yet
    /// a rule, and a retired one no longer is.
    List {
        /// The repository, as owner/repo.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
    },
}

pub fn run(command: &RulesCommand) -> Result<Report> {
    match command {
        RulesCommand::List { repo } => {
            let store = Store::open_default()?;
            let rules = store.rules_for_repo(repo)?;

            let plain = if rules.is_empty() {
                format!("no rules in force for {repo}")
            } else {
                rules.iter().map(rule_line).collect::<Vec<_>>().join("\n")
            };

            Ok(Report::new(json!({ "repo": repo, "rules": rules }), plain))
        }
    }
}

/// `12 active global — a rule cites a set of findings (prevents: a list implies order)`
fn rule_line(rule: &Rule) -> String {
    format!(
        "{} {} {} — {} (prevents: {})",
        rule.id,
        rule.stage,
        rule.scope.repo().unwrap_or("global"),
        rule.statement,
        rule.prevents,
    )
}
