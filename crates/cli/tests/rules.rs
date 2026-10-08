//! `rules list`: what a review is told to look for before it reads a diff.
//!
//! DEV-851 leaves writing rules out of the command line, so these seed through the store
//! crate — the same crate the binary goes through, and the only code that opens the
//! database file.

mod common;

use common::Desk;
use review_desk_store::{NewRule, RuleScope, RuleStage, Store};

/// A rule in one scope at one stage.
fn rule(statement: &str, scope: RuleScope, stage: RuleStage) -> NewRule {
    NewRule {
        statement: statement.to_string(),
        prevents: format!("the failure {statement} prevents"),
        scope,
        stage,
        origin_review_id: None,
        origin_findings: Vec::new(),
    }
}

/// Seed one rule of every interesting scope and stage.
fn seed(store: &Store) {
    for new in [
        rule(
            "a rule cites a set of findings",
            RuleScope::Global,
            RuleStage::Active,
        ),
        rule(
            "every refusal names what it would have accepted",
            RuleScope::Global,
            RuleStage::Ci,
        ),
        rule(
            "the store is the only code that opens the database",
            RuleScope::Repo("devergehq/review-desk".to_string()),
            RuleStage::Active,
        ),
        rule(
            "a subcommand group lives in its own module",
            RuleScope::Repo("devergehq/review-desk".to_string()),
            RuleStage::Lint,
        ),
        rule(
            "a composable is never called from a store",
            RuleScope::Repo("devergehq/allele".to_string()),
            RuleStage::Active,
        ),
        rule(
            "a proposed rule is not yet a rule",
            RuleScope::Repo("devergehq/review-desk".to_string()),
            RuleStage::Proposed,
        ),
        rule(
            "a retired rule no longer is",
            RuleScope::Repo("devergehq/review-desk".to_string()),
            RuleStage::Retired,
        ),
        rule(
            "a retired global rule is also out",
            RuleScope::Global,
            RuleStage::Retired,
        ),
    ] {
        store.create_rule(new).expect("seed rule");
    }
}

/// The statements `rules list --repo` printed, as a JSON document.
fn statements(document: &serde_json::Value) -> Vec<String> {
    document["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|rule| rule["statement"].as_str().expect("statement").to_string())
        .collect()
}

#[test]
fn the_rules_for_a_repository_are_the_global_ones_and_its_own() {
    let desk = Desk::new();
    seed(&desk.store());

    let document = desk.json(&["rules", "list", "--repo", "devergehq/review-desk"]);

    assert_eq!(
        statements(&document),
        [
            // Global first, then the repository's own.
            "a rule cites a set of findings",
            "every refusal names what it would have accepted",
            "the store is the only code that opens the database",
            "a subcommand group lives in its own module",
        ]
    );
    assert_eq!(document["repo"], "devergehq/review-desk");
}

#[test]
fn another_repositorys_rules_are_left_out() {
    let desk = Desk::new();
    seed(&desk.store());

    let mine = statements(&desk.json(&["rules", "list", "--repo", "devergehq/review-desk"]));
    let theirs = statements(&desk.json(&["rules", "list", "--repo", "devergehq/allele"]));

    assert!(
        !mine.contains(&"a composable is never called from a store".to_string()),
        "another repository's rule must not apply here"
    );
    assert!(theirs.contains(&"a composable is never called from a store".to_string()));
    // And both still see the global ones.
    for statements in [&mine, &theirs] {
        assert!(statements.contains(&"a rule cites a set of findings".to_string()));
    }
}

#[test]
fn only_rules_in_force_are_listed() {
    let desk = Desk::new();
    seed(&desk.store());

    let document = desk.json(&["rules", "list", "--repo", "devergehq/review-desk"]);
    let listed = statements(&document);

    for out_of_force in [
        "a proposed rule is not yet a rule",
        "a retired rule no longer is",
        "a retired global rule is also out",
    ] {
        assert!(
            !listed.contains(&out_of_force.to_string()),
            "{out_of_force} is not in force and must not be listed"
        );
    }

    for stage in document["rules"].as_array().expect("rules") {
        let stage = stage["stage"].as_str().expect("stage");
        assert!(
            ["active", "lint", "ci"].contains(&stage),
            "{stage} is not a stage at which a rule is in force"
        );
    }
}

#[test]
fn a_repository_with_no_rules_is_still_answered() {
    let desk = Desk::new();
    // The database has to exist for the command to read it, and nothing else is seeded.
    drop(desk.store());

    let document = desk.json(&["rules", "list", "--repo", "devergehq/nothing-learned-yet"]);
    assert_eq!(document["rules"].as_array().expect("rules").len(), 0);

    let plain = desk.run(&["rules", "list", "--repo", "devergehq/nothing-learned-yet"]);
    assert_eq!(plain, "no rules in force for devergehq/nothing-learned-yet");
}
