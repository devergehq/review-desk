//! A code change a later posted round has already reviewed, through the binary. DEV-983.
//!
//! **The three fixtures the issue asks for**, each seeded by the commands an agent session
//! would run and read back off `GET /api/v1/inbox` from a `review-desk serve` the test
//! started. Nothing here writes SQL or imports the store: the complaint was about what the
//! queue drew, and the queue reads that endpoint, so a shortcut past either end would be a
//! shortcut past the thing being tested.
//!
//! Every test gets its own database through [`Desk`] and its own server on port 0, so none
//! can read or write `~/.review-desk/review-desk.db` and two runs cannot collide.
//!
//! ## What Patrick saw
//!
//! On 14 October 2026 his queue drew a two-round pull request as "round 1 · The code under
//! F1 changed · the file changed in <B>, but not line 1122 · See what changed", linking
//! round 1 — though round 2 had been posted the day before, and the change it reported was
//! round 2's own head. `GET /api/v1/inbox` had the pull request in `author_replies` for
//! round 1 and in `posted_awaiting_author` for round 2, and the one-row-per-pull-request
//! merge chose the round 1 item.

mod common;

use common::{get_json, serve, wait_for, Desk, Served};
use serde_json::Value;

/// The repository and pull request these fixtures are on.
const REPO: &str = "acme/portal";
const PR: &str = "741";

/// Round 1's head, round 2's head, and a commit after both that no round records.
const FIRST: &str = "c1d2e31";
const SECOND: &str = "d2e3f42";
const THIRD: &str = "e3f4a53";

/// The file a finding points at, which is what a code change is recorded against.
const FILE: &str = "app/Exports/ScheduledExport.php";

/// A review's one finding, with the file and line a code change is reported about.
///
/// **Hands back the sequence number it printed, which is not 1 on a later round.** A
/// finding's `seq` runs across the pull request rather than within one round, so that a
/// finding carried into round 2 keeps the key the posted review already named it by — round
/// 2's own first finding here is `F2`. A fixture that assumed 1 is refused with "outcome
/// refers to finding 2/1, which does not exist", which is how this was found.
fn finding(desk: &Desk, review: &str) -> String {
    let output = desk
        .command()
        .args(["finding", "add", "--review", review, "--file", "-"])
        .write_stdin(
            serde_json::json!({
                "lens": "correctness",
                "severity": "should",
                "claim": "The export reads the tenant from the request that queued it",
                "file": FILE,
                "line": 1122,
                "disposition": "in_draft",
            })
            .to_string(),
        )
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .expect("output is text")
        .trim()
        .to_string()
}

/// Record that this round posted, which is the one thing the command line says it with.
///
/// `review link --github-review` is what [`Store::review_is_posted`] reads, and it is also
/// what makes the next commit a round of its own: since DEV-962 a new head on an *unposted*
/// round moves that round in place. So this is load-bearing twice over, and deleting it as
/// redundant would collapse the two heads below into one round.
fn posted(desk: &Desk, review: &str, which: u32) {
    desk.command()
        .args([
            "review",
            "link",
            "--review",
            review,
            "--github-review",
            &format!("PRR_acme741r{which}"),
        ])
        .assert()
        .success();
}

/// Record a code change on a finding, with the detail the harvester would have written.
///
/// `harvest_code_changes` builds this document: the commit the change is at, and the
/// `compare A...B` of the comparison it was read from. Both are what the already-reviewed
/// rule reads, so a fixture that invented a shape would be testing nothing.
fn code_changed(desk: &Desk, review: &str, seq: &str, commit: &str, from: &str, to: &str) {
    let detail = serde_json::json!({
        "file": FILE,
        "line": 1122,
        "lines_changed": false,
        "file_status": "modified",
        "commit": commit,
        "basis": format!("compare {from}...{to}"),
    })
    .to_string();
    desk.command()
        .args([
            "outcome",
            "add",
            "--review",
            review,
            "--finding",
            seq,
            "--kind",
            "code_changed_after",
            "--by",
            "harvester",
            "--detail",
            &detail,
        ])
        .assert()
        .success();
}

/// The two rounds of `acme/portal#741`, both posted: round 1 at `FIRST`, round 2 at
/// `SECOND`.
///
/// Each round carries one finding, which a code change is reported against, and each round's
/// own sequence number comes back with its id — see [`finding`] for why they differ.
struct Rounds {
    one: String,
    one_seq: String,
    two: String,
    two_seq: String,
}

impl Rounds {
    /// The two review ids as numbers, for asserting against the inbox.
    fn ids(&self) -> (i64, i64) {
        (
            self.one.parse().expect("round 1's id"),
            self.two.parse().expect("round 2's id"),
        )
    }
}

fn two_posted_rounds(desk: &Desk) -> Rounds {
    let one = open_round(desk, FIRST);
    let one_seq = finding(desk, &one);
    posted(desk, &one, 1);

    let two = open_round(desk, SECOND);
    let two_seq = finding(desk, &two);
    posted(desk, &two, 2);

    Rounds {
        one,
        one_seq,
        two,
        two_seq,
    }
}

/// Open the pull request at this commit and hand back the round's review id.
///
/// The ticket and its address go on this call rather than a later `review link`: they are
/// facts about the review and `review open` is what writes them — and a later round is its
/// own review and inherits neither, so each round names them for itself. The queue's row
/// links the ticket, which is what makes them worth seeding.
fn open_round(desk: &Desk, head: &str) -> String {
    desk.run(&[
        "review",
        "open",
        "--repo",
        REPO,
        "--pr",
        PR,
        "--head",
        head,
        "--title",
        "Hold a scheduled export's tenant for the whole run",
        "--ticket",
        "ABC-741",
        "--ticket-url",
        "https://linear.app/acme/issue/ABC-741",
    ])
}

/// Start a server on this desk and wait until it answers.
fn serving(desk: &Desk) -> Served {
    let served = serve(desk, true);
    assert!(
        wait_for(&served.address()),
        "nothing answered at {}, which serve had just printed",
        served.address()
    );
    served
}

/// The review ids one inbox array holds.
fn ids(inbox: &Value, group: &str) -> Vec<i64> {
    inbox[group]
        .as_array()
        .unwrap_or_else(|| panic!("the inbox has a {group} array: {inbox}"))
        .iter()
        .map(|item| item["review_id"].as_i64().expect("a review id"))
        .collect()
}

/// One item of an array, by the review it is about.
fn item<'a>(inbox: &'a Value, group: &str, review: i64) -> &'a Value {
    inbox[group]
        .as_array()
        .expect("the array")
        .iter()
        .find(|item| item["review_id"].as_i64() == Some(review))
        .unwrap_or_else(|| panic!("no {group} item for review {review}: {inbox}"))
}

/// The five arrays `totals.items` counts, so the heading can be checked against them.
fn waiting_items(inbox: &Value) -> usize {
    [
        "briefs_to_confirm",
        "drafts_to_approve",
        "author_replies",
        "own_prs_without_reviewer",
        "proposed_rules",
    ]
    .iter()
    .map(|group| inbox[group].as_array().map(Vec::len).unwrap_or(0))
    .sum()
}

#[test]
fn a_change_the_posted_round_read_leaves_the_queue_and_the_round_2_item_remains() {
    let desk = Desk::new();
    let rounds = two_posted_rounds(&desk);
    // The change Patrick's record held: round 1's comparison ends at round 2's own head.
    code_changed(&desk, &rounds.one, &rounds.one_seq, SECOND, FIRST, SECOND);

    let served = serving(&desk);
    let inbox = get_json(&served.address(), "/api/v1/inbox");

    let (one, two) = rounds.ids();
    assert!(
        !ids(&inbox, "author_replies").contains(&one),
        "round 1 has nothing left waiting on anybody: {inbox}"
    );
    assert!(
        !ids(&inbox, "author_replies").contains(&two),
        "and round 2 has had nothing back at all"
    );
    assert_eq!(
        ids(&inbox, "posted_awaiting_author"),
        vec![two],
        "one item, and it is the newest round"
    );
    let posted = item(&inbox, "posted_awaiting_author", two);
    assert_eq!(
        posted["round"], 2,
        "which is what the row takes its number from"
    );
    assert_eq!(
        posted["code_changed_after"],
        Value::Null,
        "nothing has harvested round 2, so the figure is an absence and not a nought — \
         which is the rule `inbox_posted` states and the one thing this row must not invent"
    );
    assert_eq!(
        inbox["totals"]["items"].as_u64().unwrap() as usize,
        waiting_items(&inbox),
        "the heading counts the items the groups hold"
    );

    // **Nothing is deleted.** The What happened page reads the review, and the event is
    // still on it exactly as the harvester recorded it.
    let round_one = get_json(&served.address(), &format!("/api/v1/reviews/{one}"));
    let kinds: Vec<&str> = round_one["outcomes"]
        .as_array()
        .expect("the outcomes")
        .iter()
        .map(|outcome| outcome["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(
        kinds,
        vec!["code_changed_after"],
        "the earlier round still holds every event: {round_one}"
    );
}

#[test]
fn a_further_change_counts_on_the_newest_round() {
    let desk = Desk::new();
    let rounds = two_posted_rounds(&desk);
    code_changed(&desk, &rounds.one, &rounds.one_seq, SECOND, FIRST, SECOND);
    // The author pushes again, past round 2's head. The harvester reads both rounds, so
    // each gets the change against its own comparison — and neither comparison ends at a
    // posted round's head, so neither is already reviewed.
    code_changed(&desk, &rounds.one, &rounds.one_seq, THIRD, FIRST, THIRD);
    code_changed(&desk, &rounds.two, &rounds.two_seq, THIRD, SECOND, THIRD);

    let served = serving(&desk);
    let inbox = get_json(&served.address(), "/api/v1/inbox");
    let (one, two) = rounds.ids();

    let newest = item(&inbox, "author_replies", two);
    assert_eq!(newest["round"], 2);
    let kinds: Vec<&str> = newest["events"]
        .as_array()
        .expect("the events")
        .iter()
        .map(|event| event["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(
        kinds,
        vec!["code_changed_after"],
        "the newest round carries the change nothing has reviewed"
    );

    // Round 1's own reading of the same push is still there — its comparison ends nowhere
    // any round posted, and ancestry is in no record here — and the covered one is gone.
    let earlier = item(&inbox, "author_replies", one);
    assert_eq!(
        earlier["events"].as_array().expect("the events").len(),
        1,
        "one of round 1's two changes was already reviewed: {earlier}"
    );
    assert!(
        earlier["events"][0]["detail"]
            .as_str()
            .expect("the detail")
            .contains(&format!("compare {FIRST}...{THIRD}")),
        "and it is the one the posted round did not read: {earlier}"
    );
    assert_eq!(
        inbox["totals"]["items"].as_u64().unwrap() as usize,
        waiting_items(&inbox),
    );
}

#[test]
fn a_reply_on_round_one_after_round_two_posts_is_still_waiting() {
    let desk = Desk::new();
    let rounds = two_posted_rounds(&desk);
    code_changed(&desk, &rounds.one, &rounds.one_seq, SECOND, FIRST, SECOND);
    // The author answers a round-1 thread the day after round 2 posted. A reply is a thing
    // a person said; only a code change can be already reviewed.
    desk.command()
        .args([
            "outcome",
            "add",
            "--review",
            &rounds.one,
            "--finding",
            &rounds.one_seq,
            "--kind",
            "replied",
            "--by",
            "author",
            "--detail",
            "The retry passes the tenant now.",
        ])
        .assert()
        .success();

    let served = serving(&desk);
    let inbox = get_json(&served.address(), "/api/v1/inbox");
    let (one, two) = rounds.ids();

    let reply = item(&inbox, "author_replies", one);
    assert_eq!(
        reply["round"], 1,
        "the item says which round was replied to, which the row then words"
    );
    let kinds: Vec<&str> = reply["events"]
        .as_array()
        .expect("the events")
        .iter()
        .map(|event| event["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(
        kinds,
        vec!["replied"],
        "the reply survives and the already-reviewed change does not: {reply}"
    );

    // And the newest round is in the answer, so the row can take its number and its link.
    let posted = item(&inbox, "posted_awaiting_author", two);
    assert_eq!(posted["round"], 2);
    assert_eq!(posted["rounds_on_pr"], 2);
    assert_eq!(
        inbox["totals"]["items"].as_u64().unwrap() as usize,
        waiting_items(&inbox),
    );
}

#[test]
fn a_change_at_a_commit_no_round_records_is_still_waiting() {
    let desk = Desk::new();
    let rounds = two_posted_rounds(&desk);
    // Neither commit is a round's head. Nothing here reads git, so nothing can say whether
    // round 2 contains `9a8b7c6` — and an unknown commit errs toward showing.
    code_changed(
        &desk,
        &rounds.one,
        &rounds.one_seq,
        "9a8b7c6",
        FIRST,
        "7f6e5d4",
    );

    let served = serving(&desk);
    let inbox = get_json(&served.address(), "/api/v1/inbox");
    let (one, _two) = rounds.ids();

    assert!(
        ids(&inbox, "author_replies").contains(&one),
        "an unrecognised commit is shown, not hidden: {inbox}"
    );
}

#[test]
fn a_detail_that_is_not_json_does_not_empty_the_queue() {
    let desk = Desk::new();
    let rounds = two_posted_rounds(&desk);
    // `outcome add --detail` takes free text, and a clause that read it as JSON unguarded
    // would raise rather than return null — failing the whole query over one row of prose.
    desk.command()
        .args([
            "outcome",
            "add",
            "--review",
            &rounds.one,
            "--finding",
            &rounds.one_seq,
            "--kind",
            "code_changed_after",
            "--by",
            "harvester",
            "--detail",
            "the author pushed a fix",
        ])
        .assert()
        .success();

    let served = serving(&desk);
    let inbox = get_json(&served.address(), "/api/v1/inbox");
    let (one, _two) = rounds.ids();

    assert!(
        ids(&inbox, "author_replies").contains(&one),
        "prose says nothing about a commit, so the change is shown: {inbox}"
    );
}
