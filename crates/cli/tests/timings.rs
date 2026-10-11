//! **No command carries a time for a step.**
//!
//! DEV-890's second rule, at the surface agent sessions use. The store stamps every time
//! it writes, so a duration cannot be backdated or invented — and the way to keep that
//! true of the command line is for there to be no argument to pass. These tests try to
//! pass one anyway, on every command that moves a step, and each attempt is a usage
//! error rather than a review whose history says what the caller wanted it to say.
//!
//! The same rule at the HTTP boundary is `crates/server/tests/timings.rs`; the arithmetic
//! it protects is `crates/store/tests/timings.rs`.

mod common;

use common::Desk;

/// A review with a brief, as a session opens one.
fn opened(desk: &Desk) -> String {
    let id = desk.json(&[
        "review",
        "open",
        "--repo",
        "acme/portal",
        "--pr",
        "412",
        "--head",
        "3f9c2ab",
    ])["review"]["id"]
        .as_i64()
        .expect("a review id")
        .to_string();
    let brief = desk.write_file(
        "brief.json",
        r#"{"problem":"A failed delivery is never tried again."}"#,
    );
    desk.run(&[
        "brief",
        "put",
        "--review",
        &id,
        "--file",
        brief.to_str().unwrap(),
    ]);
    id
}

#[test]
fn layer_set_takes_no_time_whatever_it_is_called() {
    let desk = Desk::new();
    let review = opened(&desk);

    // The names a caller would reach for. Each is a usage error: clap refuses an argument
    // the command does not declare, which is the whole of the defence — there is nothing
    // to validate because there is nothing to pass.
    for argument in [
        "--at",
        "--started-at",
        "--finished-at",
        "--entered-at",
        "--seconds",
        "--duration",
    ] {
        desk.command()
            .args([
                "layer",
                "set",
                "--review",
                &review,
                "--layer",
                "5",
                "--state",
                "done",
                argument,
                "1759000000",
            ])
            .assert()
            .failure();
    }

    // And the command without one works, so the loop above is not failing for some other
    // reason — which is the way a test like this goes quietly wrong.
    let written = desk.json(&[
        "layer", "set", "--review", &review, "--layer", "5", "--state", "done",
    ]);
    assert_eq!(written["layer"]["state"], "done");
}

#[test]
fn nothing_that_moves_a_step_takes_a_time() {
    // Every command that moves one of the seven steps, with a time appended. The list is
    // the point: a command added later that takes a time has to be added here to pass,
    // and the review's own history is what it would otherwise be able to rewrite.
    let desk = Desk::new();
    let review = opened(&desk);
    let body = desk.write_file("draft.md", "The change is the right shape.\n");
    let body = body.to_str().unwrap().to_string();
    let finding = desk.write_file(
        "finding.json",
        r#"{"lens":"correctness","severity":"should","claim":"The retry has no ceiling."}"#,
    );
    // The brief `opened` wrote, by its real path: an attempt that failed because the file
    // was not there would prove nothing about the argument.
    let brief = desk.dir().join("brief.json");
    let brief = brief.to_str().unwrap().to_string();

    let attempts: Vec<Vec<&str>> = vec![
        vec!["brief", "put", "--review", &review, "--file", &brief],
        vec![
            "draft", "put", "--review", &review, "--file", &body, "--status", "ready",
        ],
        vec![
            "finding",
            "add",
            "--review",
            &review,
            "--file",
            finding.to_str().unwrap(),
        ],
        vec![
            "layer", "set", "--review", &review, "--layer", "6", "--state", "running",
        ],
    ];

    for attempt in attempts {
        for argument in ["--at", "--started-at", "--finished-at"] {
            let mut arguments = attempt.clone();
            arguments.push(argument);
            arguments.push("1759000000");
            desk.command()
                .args(&arguments)
                .assert()
                .failure()
                .stderr(predicates::str::contains("unexpected argument"));
        }
    }
}

#[test]
fn a_review_reads_back_the_passes_each_step_made() {
    // `review show --json` is how a session reads the record back, and it prints the
    // store's own document — so the passes and their lengths are there without a command
    // of their own. Whole seconds, as the store holds them: formatting belongs to
    // whatever displays a record, which the command line's own `show_lines` says too.
    let desk = Desk::new();
    let review = opened(&desk);

    desk.run(&[
        "layer", "set", "--review", &review, "--layer", "5", "--state", "running",
    ]);
    desk.run(&[
        "layer",
        "set",
        "--review",
        &review,
        "--layer",
        "5",
        "--state",
        "done",
        "--detail",
        "3 findings",
    ]);

    let shown = desk.json(&["review", "show", "--review", &review]);
    let step = shown["layers"]
        .as_array()
        .expect("seven steps")
        .iter()
        .find(|layer| layer["number"] == 5)
        .expect("step 5")
        .clone();

    let runs = step["runs"].as_array().expect("its passes");
    assert_eq!(runs.len(), 1, "one pass, started and finished: {step}");
    assert_eq!(runs[0]["state"], "running");
    assert!(
        runs[0]["started_at"].is_i64() && runs[0]["finished_at"].is_i64(),
        "both ends are the integers the store holds: {step}"
    );

    // A step nothing has written has no passes, which is not the same as a pass of no
    // length.
    let harvest = shown["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["number"] == 7)
        .expect("step 7")
        .clone();
    assert_eq!(harvest["runs"].as_array().map(Vec::len), Some(0));
}
