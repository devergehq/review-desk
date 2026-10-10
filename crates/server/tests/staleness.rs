//! The contract's staleness rule and this server's behaviour, tied together.
//!
//! **`Rule.stale` is the one derived field in the contract with a threshold inside it.**
//! Everything else the server derives — `key`, `stage_label`, `finding_severities` — is a
//! restatement of what the store holds. Staleness is a judgement, and until DEV-862 the
//! number behind it existed only in `crates/server/src/wire.rs`, chosen because a mockup
//! said "No finding has cited it in 60 days".
//!
//! That is the failure this file prevents: the contract now states the rule, so the
//! server's constant and the contract's prose can disagree, and nothing else in the
//! repository would notice. A test that only exercised the behaviour would pass with the
//! constant at 90 days and the contract saying 60.
//!
//! So two things are checked here, and they are different checks:
//!
//! | Check | Fails when |
//! |---|---|
//! | The contract's prose names the same number as `STALE_AFTER_DAYS` | Either side is changed alone |
//! | A served rule goes stale on exactly that boundary, and a deferral takes it out | The constant and the code that reads it drift |

use std::fs;
use std::path::PathBuf;

use review_desk_server::wire::{self, STALE_AFTER_DAYS};
use review_desk_store::{NewFinding, NewRule, RuleScope, RuleStage, Severity, Timestamp};

mod common;

/// Seconds in a day, as the server counts them.
const DAY: Timestamp = 86_400;

/// Move one of a rule's times `days` into the past.
///
/// In SQL, for the reason `crates/store/tests/rules.rs` gives: the store has no function
/// for moving a time backwards and should not grow one so that a test can pass. The second
/// handle on the same file is the one the harness already keeps for seeding.
fn age(harness: &common::Harness, statement: &str, rule: i64, days: Timestamp) {
    let connection = rusqlite::Connection::open(harness.store.path())
        .expect("opening the served database to age a rule");
    connection
        .execute(
            statement,
            rusqlite::params![review_desk_store::now() - days * DAY, rule],
        )
        .expect("aging the rule");
}

/// `contract/openapi.yaml`, read from the repository rather than copied into this test.
fn contract() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.yaml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

#[test]
fn the_contract_states_the_number_the_server_uses() {
    let contract = contract();

    // The sentence the contract states the rule in. Matched as a whole rather than by
    // searching for "60": the number on its own appears in the deferral control's wording
    // too, and a test that found that one would pass with the rule unstated.
    let rule = format!(
        "A rule is stale when no finding has cited it for {STALE_AFTER_DAYS} days and it has not been"
    );
    assert!(
        contract.contains(&rule),
        "contract/openapi.yaml does not state the staleness rule with the server's own \
         number. The server's STALE_AFTER_DAYS is {STALE_AFTER_DAYS}, so Rule.stale has to \
         contain: {rule:?}"
    );

    // And the deferral the rule's second clause depends on. "Keep 60 more days" is the
    // control that answers staleness, so the two numbers are the same number.
    assert!(
        contract.contains(&format!("Keep {STALE_AFTER_DAYS} more days")),
        "the contract's deferral control and the staleness threshold are the same number, \
         and the contract no longer says {STALE_AFTER_DAYS}"
    );
}

#[test]
fn the_contract_names_this_file_as_what_ties_them() {
    // A test nobody can find is a test nobody maintains. The contract points at this
    // file, so moving or renaming it has to update the contract.
    assert!(
        contract().contains("crates/server/tests/staleness.rs"),
        "contract/openapi.yaml claims a test ties its staleness rule to the server; it has \
         to name this file so a reader can check the claim"
    );
}

/// A rule last cited `days` ago, served, and whether the server called it stale.
fn stale_after(days: Timestamp) -> bool {
    let harness = common::start_with_no_github();

    // A review and a finding, because a citation is a finding carrying a rule id — the
    // staleness measurement has no other source.
    let review = harness
        .store
        .open_review("acme/portal", 412, "3f9c2ab")
        .expect("opening a review")
        .review
        .id;

    let rule = harness
        .store
        .create_rule(NewRule {
            statement: "A retry must carry an idempotency key.".to_string(),
            prevents: "One event delivered twice.".to_string(),
            scope: RuleScope::Repo("acme/portal".to_string()),
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: Vec::new(),
            why_proposed: None,
            note: None,
        })
        .expect("writing a rule")
        .id;

    harness
        .store
        .add_finding(
            review,
            NewFinding {
                lens: "sibling diff".to_string(),
                severity: Severity::Blocker,
                file: None,
                line: None,
                claim: "A retried delivery carries no idempotency key.".to_string(),
                evidence: None,
                disposition: review_desk_store::Disposition::InDraft,
                disposition_reason: None,
                rule_id: Some(rule),
                ..Default::default()
            },
        )
        .expect("writing the citing finding");

    // The citation's age is what staleness measures, and `add_finding` stamps the rule
    // with now. Aged in the database's own SQL, as `crates/store/tests/rules.rs` does for
    // the same reason: the store has no function for moving a time backwards and should
    // not grow one so that a test can pass.
    age(
        &harness,
        "UPDATE rules SET last_cited_at = ?1 WHERE id = ?2",
        rule,
        days,
    );

    let answer = harness.api("GET", "/rules?repo=acme/portal", None);
    assert_eq!(answer.status, 200, "{}", answer.text());

    let page = answer.json();
    let in_stale = page["stale"]
        .as_array()
        .expect("a stale group")
        .iter()
        .any(|rule| rule["id"].as_i64() == Some(1));
    let in_active = page["active"]
        .as_array()
        .expect("an active group")
        .iter()
        .any(|rule| rule["id"].as_i64() == Some(1));

    // Every rule in force is in exactly one of the two groups. A rule in neither, or in
    // both, would make the answer below meaningless rather than wrong.
    assert_ne!(
        in_stale,
        in_active,
        "a rule in force belongs to exactly one of active and stale: {}",
        answer.text()
    );

    // And the flag the contract makes a field in its own right agrees with the grouping.
    let served = page[if in_stale { "stale" } else { "active" }][0].clone();
    assert_eq!(
        served["stale"].as_bool(),
        Some(in_stale),
        "Rule.stale and the group the rule was served in disagree: {}",
        answer.text()
    );

    in_stale
}

#[test]
fn a_rule_cited_inside_the_window_is_not_stale() {
    assert!(
        !stale_after(STALE_AFTER_DAYS - 1),
        "a rule cited {} days ago is inside the {STALE_AFTER_DAYS}-day window",
        STALE_AFTER_DAYS - 1
    );
}

#[test]
fn a_rule_cited_on_the_boundary_is_not_yet_stale() {
    // Exactly 60 days is not "more than 60 days". The contract says "more than 60 days
    // have passed", so the boundary day itself is the last day the rule is current — and
    // an off-by-one here is the difference between the Rules screen asking about a rule
    // today and asking tomorrow.
    assert!(
        !stale_after(STALE_AFTER_DAYS),
        "a rule cited exactly {STALE_AFTER_DAYS} days ago has not passed the window"
    );
}

#[test]
fn a_rule_cited_past_the_window_is_stale() {
    assert!(
        stale_after(STALE_AFTER_DAYS + 1),
        "a rule cited {} days ago is past the {STALE_AFTER_DAYS}-day window",
        STALE_AFTER_DAYS + 1
    );
}

#[test]
fn keeping_a_stale_rule_for_longer_takes_it_out_of_the_stale_group() {
    let harness = common::start_with_no_github();

    let rule = harness
        .store
        .create_rule(NewRule {
            statement: "A retry must carry an idempotency key.".to_string(),
            prevents: "One event delivered twice.".to_string(),
            scope: RuleScope::Global,
            stage: RuleStage::Active,
            origin_review_id: None,
            origin_findings: Vec::new(),
            why_proposed: None,
            note: None,
        })
        .expect("writing a rule")
        .id;

    // Nothing has ever cited it, so staleness is measured from when it was made — which
    // is what keeps a rule created yesterday out of the stale group on its first day.
    age(
        &harness,
        "UPDATE rules SET created_at = ?1 WHERE id = ?2",
        rule,
        STALE_AFTER_DAYS + 5,
    );

    let before = harness.api("GET", "/rules", None).json();
    assert_eq!(
        before["stale"].as_array().map(Vec::len),
        Some(1),
        "a rule nothing has cited since it was made is stale: {before}"
    );

    // The deferral the contract's second clause is about, with the number the contract
    // words the control as.
    let deferred = harness.api(
        "POST",
        &format!("/rules/{rule}/defer"),
        Some(&format!("{{\"days\":{STALE_AFTER_DAYS}}}")),
    );
    assert_eq!(deferred.status, 200, "{}", deferred.text());
    assert_eq!(
        deferred.json()["stale"].as_bool(),
        Some(false),
        "deferring a rule answers the staleness question: {}",
        deferred.text()
    );

    let after = harness.api("GET", "/rules", None).json();
    assert_eq!(
        after["stale"].as_array().map(Vec::len),
        Some(0),
        "a deferred rule is no longer in the stale group: {after}"
    );
    assert_eq!(
        after["active"].as_array().map(Vec::len),
        Some(1),
        "and it is still in force, because a deferral is not a retirement: {after}"
    );
}

#[test]
fn a_proposal_and_a_retired_rule_are_never_stale() {
    // The rule's second clause: staleness is a question about rules in force. A proposal
    // is not in force yet and a retired rule no longer is, so neither can go stale —
    // which matters because both can sit untouched for far longer than sixty days.

    for stage in [RuleStage::Proposed, RuleStage::Retired] {
        let harness = common::start_with_no_github();
        let rule = harness
            .store
            .create_rule(NewRule {
                statement: "Do not raise rename-for-clarity suggestions.".to_string(),
                prevents: "Nits that generate more nits.".to_string(),
                scope: RuleScope::Global,
                stage,
                origin_review_id: None,
                origin_findings: Vec::new(),
                why_proposed: None,
                note: None,
            })
            .expect("writing a rule")
            .id;
        age(
            &harness,
            "UPDATE rules SET created_at = ?1 WHERE id = ?2",
            rule,
            STALE_AFTER_DAYS + 500,
        );

        let held = harness
            .store
            .find_rule(rule)
            .expect("reading it back")
            .expect("a rule");
        assert!(
            !wire::is_stale(&held, review_desk_store::now()),
            "a rule at {stage} is not in force, so it cannot have gone stale"
        );
    }
}
