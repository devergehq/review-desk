//! `review-desk post`, run as the binary — DEV-1347.
//!
//! **GitHub is faked, and nothing here touches the network.** Each test puts a `gh` of its
//! own first and alone on `PATH`: a shell script that writes what it was sent to a file and
//! answers with a canned review id, or fails, or answers something unreadable. `HOME` is the
//! test's own directory, so Allele is either absent or a listener the test put at
//! `$HOME/.allele/control.sock` — never the developer's real one.

mod common;

use std::path::PathBuf;

use common::Desk;
use review_desk_fake_bin::FakeBin;
use review_desk_store::{Actor, DraftStatus, NewDraft, NewReview, PostedTagStatus, ReviewEvent};
use serde_json::Value;
use tempfile::TempDir;

/// A fake `gh`. It records its arguments. A listing of reviews answers from `reviews`; a
/// post records its standard input and answers from `answer`, unless a file called `fail`
/// exists beside it, whose text it prints as `gh`'s error.
struct FakeGh {
    fake: FakeBin,
}

impl FakeGh {
    fn new() -> Self {
        // The script reads and writes beside itself, in the fake's own directory. Its
        // body is written before that directory is known, so it finds it from `$0`.
        let fake = FakeBin::new(
            "gh",
            "here=\"${0%/*}\"\n\
             echo \"$@\" >> \"$here/calls\"\n\
             case \"$*\" in *--paginate*) /bin/cat \"$here/reviews\" 2>/dev/null; exit 0;; esac\n\
             /bin/cat > \"$here/payload.json\"\n\
             if [ -f \"$here/fail\" ]; then /bin/cat \"$here/fail\" >&2; exit 1; fi\n\
             /bin/cat \"$here/answer\"\n",
        );
        let gh = FakeGh { fake };
        gh.answer(r#"{"id": 4140231301, "state": "CHANGES_REQUESTED"}"#);
        gh
    }

    fn answer(&self, text: &str) {
        std::fs::write(self.fake.dir().join("answer"), text).unwrap();
    }

    /// Make the next post fail with `stderr`, or succeed again with `None`.
    fn fail(&self, stderr: Option<&str>) {
        let flag = self.fake.dir().join("fail");
        match stderr {
            Some(stderr) => std::fs::write(flag, stderr).unwrap(),
            None => {
                let _ = std::fs::remove_file(flag);
            }
        }
    }

    /// The reviews already on the pull request, as `gh api --paginate --jq` prints them.
    fn reviews(&self, reviews: &[(u64, &str)]) {
        let lines: Vec<String> = reviews
            .iter()
            .map(|(id, body)| serde_json::json!({ "id": id, "body": body }).to_string())
            .collect();
        std::fs::write(self.fake.dir().join("reviews"), lines.join("\n")).unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.fake.dir().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn payload(&self) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.fake.dir().join("payload.json")).unwrap(),
        )
        .unwrap()
    }
}

/// A review linked to `session-1`, with an approved draft, and its id.
fn approved(desk: &Desk) -> i64 {
    let store = desk.store();
    let review = store
        .create_review(NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: "9535132".to_string(),
            round: 1,
            session_id: Some("session-1".to_string()),
            ..Default::default()
        })
        .unwrap()
        .id;
    store
        .put_draft(review, NewDraft::ready("The approach fits the ticket."))
        .unwrap();
    let shown = store.draft_shown(review).unwrap();
    store
        .approve_draft(
            review,
            ReviewEvent::RequestChanges,
            Actor::Developer,
            &shown,
        )
        .unwrap();
    review
}

/// `review-desk post` with the fake `gh` alone on `PATH`.
///
/// **With no Allele, no go-ahead line is given**, so the binary never asks any Allele —
/// including the developer's real one, which `post` finds through the passwd entry whatever
/// `HOME` says. With one, the line is given and the `fake-allele` build is pointed at the
/// test's own home.
fn post(desk: &Desk, gh: &FakeGh, review: i64, allele: &Option<PathBuf>) -> assert_cmd::Command {
    let mut command = desk.command();
    command
        .env("PATH", gh.fake.dir())
        .env("HOME", desk.dir())
        .args(["post", &review.to_string(), "--session", "session-1"])
        .args(["--go-ahead-at", "1763700000", "--json"]);
    if let Some(home) = allele {
        let line = desk.write_file("go-ahead.txt", "nothing, post it\n");
        command
            .env("REVIEW_DESK_FAKE_ALLELE_HOME", home)
            .arg("--go-ahead-file")
            .arg(line);
    }
    command
}

/// No Allele, and so no go-ahead line.
fn no_allele(_desk: &Desk) -> Option<PathBuf> {
    None
}

/// Make the review's attempt in flight old enough that the next post goes looking for it.
fn age_the_pending_attempt(desk: &Desk, review: i64) {
    rusqlite::Connection::open(desk.db())
        .unwrap()
        .execute(
            "UPDATE posted_tags SET created_at = created_at - 600 \
             WHERE review_id = ?1 AND status = 'pending'",
            [review],
        )
        .unwrap();
}

fn document(output: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

#[test]
fn an_approved_review_is_posted_with_its_tag_at_the_reviewed_head() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);

    let done = document(
        &post(&desk, &gh, review, &no_allele(&desk))
            .assert()
            .success(),
    );
    assert_eq!(done["posted"]["status"], "posted");
    assert_eq!(done["posted"]["github_review_id"], "4140231301");
    assert_eq!(
        done["posted"]["basis"], "desk",
        "no go-ahead line, so desk: {done}"
    );

    assert_eq!(
        gh.calls(),
        ["api --method POST repos/devergehq/locus/pulls/64/reviews --input -"]
    );
    let sent = gh.payload();
    assert_eq!(sent["commit_id"], "9535132", "pinned to the reviewed head");
    assert_eq!(sent["event"], "REQUEST_CHANGES");
    let body = sent["body"].as_str().unwrap();
    let tag = done["tag"].as_str().unwrap();
    assert!(
        body.ends_with(tag),
        "the tag is in the body that was sent: {body}"
    );
    assert!(
        tag.starts_with("<!-- review-desk:posted v=1 install="),
        "{tag}"
    );

    let store = desk.store();
    assert_eq!(store.get_draft(review).unwrap().status, DraftStatus::Posted);
    assert_eq!(
        store
            .get_review(review)
            .unwrap()
            .github_review_id
            .as_deref(),
        Some("4140231301")
    );
}

#[test]
fn a_draft_changed_after_approval_is_refused_and_nothing_is_sent() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);
    rusqlite::Connection::open(desk.db())
        .unwrap()
        .execute(
            "UPDATE draft_sections SET text = 'Something else.' WHERE review_id = ?1",
            [review],
        )
        .unwrap();

    let refused = document(&post(&desk, &gh, review, &no_allele(&desk)).assert().code(1));
    assert_eq!(refused["error"]["code"], "changed_since_shown", "{refused}");
    assert!(gh.calls().is_empty(), "gh was never run");
    assert!(desk.store().posted_tags(review).unwrap().is_empty());
}

#[test]
fn a_failed_call_leaves_the_row_failed_and_may_be_retried() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);

    gh.fail(Some("gh: Validation Failed (HTTP 422)"));
    let broken = document(&post(&desk, &gh, review, &no_allele(&desk)).assert().code(2));
    assert_eq!(broken["error"]["code"], "gh_failed", "{broken}");
    let rows = desk.store().posted_tags(review).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, PostedTagStatus::Failed);
    assert_eq!(rows[0].github_review_id, None);
    assert!(
        rows[0].failure.as_deref().unwrap().contains("422"),
        "{rows:?}"
    );
    assert_eq!(
        desk.store().get_draft(review).unwrap().status,
        DraftStatus::Approved
    );

    gh.fail(None);
    post(&desk, &gh, review, &no_allele(&desk))
        .assert()
        .success();
    let rows = desk.store().posted_tags(review).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].status, PostedTagStatus::Posted);
    assert_eq!(gh.calls().len(), 2);
}

/// **An answer that cannot be read may still be a review that went up**, so the attempt
/// stays pending and the next post is refused without running `gh` at all.
#[test]
fn a_second_post_is_refused_while_the_first_is_pending() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);

    gh.answer("<html>a proxy said something</html>");
    post(&desk, &gh, review, &no_allele(&desk)).assert().code(2);
    assert_eq!(
        desk.store().posted_tags(review).unwrap()[0].status,
        PostedTagStatus::Pending
    );

    gh.answer(r#"{"id": 1}"#);
    let refused = document(&post(&desk, &gh, review, &no_allele(&desk)).assert().code(1));
    assert_eq!(refused["error"]["code"], "post_in_flight", "{refused}");
    assert_eq!(gh.calls().len(), 1, "the second post never reached gh");
}

/// **GitHub created the review and the answer never came back.** A 504 is not a refusal, so
/// the attempt stays pending; the next post finds its tag on the pull request, records it
/// as posted, and sends nothing.
#[test]
fn a_timeout_after_github_created_the_review_is_found_and_not_sent_again() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);

    gh.fail(Some("gh: Gateway Timeout (HTTP 504)"));
    post(&desk, &gh, review, &no_allele(&desk)).assert().code(2);
    let rows = desk.store().posted_tags(review).unwrap();
    assert_eq!(
        rows[0].status,
        PostedTagStatus::Pending,
        "unknown, not failed"
    );

    // It went up anyway: the review on the PR is the body that was sent.
    let sent = gh.payload()["body"].as_str().unwrap().to_string();
    gh.reviews(&[(11, "An earlier review."), (777, &sent)]);
    gh.fail(None);
    age_the_pending_attempt(&desk, review);

    let done = document(
        &post(&desk, &gh, review, &no_allele(&desk))
            .assert()
            .success(),
    );
    assert_eq!(done["reconciled"], true, "{done}");
    assert_eq!(done["posted"]["github_review_id"], "777");
    let calls = gh.calls();
    assert_eq!(
        calls.len(),
        2,
        "one post, one look, no second post: {calls:?}"
    );
    assert!(calls[1].contains("--paginate"), "{calls:?}");
    assert_eq!(
        desk.store().get_draft(review).unwrap().status,
        DraftStatus::Posted
    );
    assert_eq!(desk.store().posted_tags(review).unwrap().len(), 1);
}

/// **And when it did not go up**, the look finds nothing, the attempt is marked failed, and
/// this post goes ahead with a new one.
#[test]
fn a_timeout_that_created_nothing_is_marked_failed_and_posted_afresh() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);

    gh.fail(Some("gh: Bad Gateway (HTTP 502)"));
    post(&desk, &gh, review, &no_allele(&desk)).assert().code(2);
    gh.reviews(&[(11, "An earlier review, by somebody else.")]);
    gh.fail(None);
    age_the_pending_attempt(&desk, review);

    post(&desk, &gh, review, &no_allele(&desk))
        .assert()
        .success();
    let rows = desk.store().posted_tags(review).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, PostedTagStatus::Failed);
    assert_eq!(rows[0].github_review_id, None);
    assert_eq!(rows[1].status, PostedTagStatus::Posted);
    assert_eq!(gh.calls().len(), 3, "post, look, post: {:?}", gh.calls());
}

/// **A thread measured on an earlier commit is refused**: pinned to the head, its line
/// could land on the wrong code.
#[test]
fn a_thread_measured_on_an_earlier_commit_is_refused() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = common::open_review(&desk, "acme/portal", "412", "a1b2c3d");
    desk.command()
        .args(["finding", "add", "--review", &review, "--file", "-"])
        .write_stdin(
            r#"{"lens":"error handling","severity":"should","claim":"finding 1",
                "title":"Finding 1","file":"app/Export.php","line":40,"line_sha":"a1b2c3d",
                "flagged_line":40,
                "hunk":{"file":"app/Export.php","start_line":39,
                        "lines":[{"marker":"unchanged","text":"    {"},
                                 {"marker":"added","text":"        return $r;"}]}}"#,
        )
        .assert()
        .success();
    // The author pushes, and the unposted round moves onto the new commit.
    common::open_review(&desk, "acme/portal", "412", "e4f5a6b");
    let sections = desk.write_file(
        "sections.json",
        r#"[{"kind":"prose","text":"**1 Should.**"},
            {"kind":"finding","finding_seq":1,"text":"**in diff** · `Open`"}]"#,
    );
    desk.command()
        .args(["draft", "put", "--review", &review, "--sections"])
        .arg(&sections)
        .assert()
        .success();
    let id: i64 = review.parse().unwrap();
    let store = desk.store();
    store
        .approve_draft(
            id,
            ReviewEvent::RequestChanges,
            Actor::Developer,
            &store.draft_shown(id).unwrap(),
        )
        .unwrap();

    let refused = document(&post(&desk, &gh, id, &no_allele(&desk)).assert().code(1));
    assert_eq!(refused["error"]["code"], "thread_off_head", "{refused}");
    assert!(gh.calls().is_empty(), "nothing was sent");
}

/// **A sign-off given before migration 24 recorded nothing it was of**, so there is nothing
/// to hold the draft to: refused, nothing sent, and no attempt recorded.
#[test]
fn a_sign_off_that_recorded_nothing_it_was_of_is_refused() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let review = approved(&desk);
    rusqlite::Connection::open(desk.db())
        .unwrap()
        .execute(
            "UPDATE drafts SET approved_body_sha256 = NULL, approved_threads_sha256 = NULL, \
             approved_rev = NULL WHERE review_id = ?1",
            [review],
        )
        .unwrap();
    assert_eq!(desk.store().get_draft(review).unwrap().approved_shown, None);

    let refused = document(&post(&desk, &gh, review, &no_allele(&desk)).assert().code(1));
    assert_eq!(refused["error"]["code"], "draft_status", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Send it back and approve it again"),
        "{refused}"
    );
    assert!(gh.calls().is_empty(), "nothing was sent");
    assert!(
        desk.store().posted_tags(review).unwrap().is_empty(),
        "no attempt recorded"
    );
}

#[test]
fn a_draft_nobody_approved_is_refused() {
    let desk = Desk::new();
    let gh = FakeGh::new();
    let store = desk.store();
    let review = store
        .create_review(NewReview {
            repo: "devergehq/locus".to_string(),
            pr: 64,
            head_sha: "9535132".to_string(),
            round: 1,
            ..Default::default()
        })
        .unwrap()
        .id;
    store
        .put_draft(review, NewDraft::ready("Not signed off."))
        .unwrap();

    let refused = document(&post(&desk, &gh, review, &no_allele(&desk)).assert().code(1));
    assert_eq!(refused["error"]["code"], "draft_status", "{refused}");
    assert!(gh.calls().is_empty());
}

/// **Run with `--features fake-allele`**, which CI's `--all-features` does: these stand up an
/// Allele of their own, and only a test build can be pointed at one.
#[cfg(all(unix, feature = "fake-allele"))]
mod allele {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::thread;

    use super::*;

    /// An Allele at `$HOME/.allele/control.sock`, mode 600 as Allele makes it, that answers
    /// one connection with `reply` and hands back what it was asked.
    fn allele(home: &Path, reply: &'static str) -> (PathBuf, thread::JoinHandle<String>) {
        allele_with_mode(home, reply, 0o600)
    }

    fn allele_with_mode(
        home: &Path,
        reply: &'static str,
        mode: u32,
    ) -> (PathBuf, thread::JoinHandle<String>) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(home.join(".allele")).unwrap();
        let path = home.join(".allele").join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut asked = String::new();
            BufReader::new(&stream).read_line(&mut asked).unwrap();
            writeln!(&stream, "{reply}").unwrap();
            asked
        });
        (home.to_path_buf(), handle)
    }

    #[test]
    fn basis_is_typed_only_when_allele_confirms_the_line() {
        let desk = Desk::new();
        let gh = FakeGh::new();
        let review = approved(&desk);
        let sockets = TempDir::new().unwrap();
        let (socket, asked) = allele(
            sockets.path(),
            r#"{"result": "prompt_verified", "typed": true}"#,
        );

        let done = document(&post(&desk, &gh, review, &Some(socket)).assert().success());
        assert_eq!(done["posted"]["basis"], "typed", "{done}");
        assert!(gh.payload()["body"]
            .as_str()
            .unwrap()
            .contains(" basis=typed "));

        assert!(
            !gh.payload()["body"]
                .as_str()
                .unwrap()
                .contains("nothing, post it"),
            "the line itself never posts"
        );
        let asked: Value = serde_json::from_str(&asked.join().unwrap()).unwrap();
        assert_eq!(asked["op"], "prompts_verify");
        assert_eq!(asked["session_id"], "session-1");
        assert_eq!(asked["text"], "nothing, post it");
    }

    /// **Today's Allele has no prompt check**, and answers the op with an error. That is
    /// `desk`, never a guess at `typed`.
    #[test]
    fn an_allele_without_the_check_gives_desk() {
        let desk = Desk::new();
        let gh = FakeGh::new();
        let review = approved(&desk);
        let sockets = TempDir::new().unwrap();
        let (socket, _asked) = allele(
            sockets.path(),
            r#"{"result": "error", "code": "bad_request", "message": "unknown op prompts_verify"}"#,
        );

        let done = document(&post(&desk, &gh, review, &Some(socket)).assert().success());
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
        assert!(
            done["basis_because"]
                .as_str()
                .unwrap()
                .contains("no prompt check"),
            "{done}"
        );
    }

    /// **`HOME` does not choose the Allele.** A socket that would vouch for the line sits
    /// under `HOME`, and the Allele this build is pointed at has none: the answer is `desk`.
    #[test]
    fn an_allele_under_home_is_not_asked() {
        let desk = Desk::new();
        let gh = FakeGh::new();
        let review = approved(&desk);
        let planted = TempDir::new().unwrap();
        let (planted, _asked) = allele(
            planted.path(),
            r#"{"result": "prompt_verified", "typed": true}"#,
        );
        let elsewhere = TempDir::new().unwrap();

        let done = document(
            &post(&desk, &gh, review, &Some(elsewhere.path().to_path_buf()))
                .env("HOME", &planted)
                .assert()
                .success(),
        );
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
    }

    /// **A socket anyone may open is not Allele's**, whatever it answers.
    #[test]
    fn a_socket_that_is_not_the_owners_alone_gives_desk() {
        let desk = Desk::new();
        let gh = FakeGh::new();
        let review = approved(&desk);
        let home = TempDir::new().unwrap();
        let (home, _asked) = allele_with_mode(
            home.path(),
            r#"{"result": "prompt_verified", "typed": true}"#,
            0o666,
        );

        let done = document(&post(&desk, &gh, review, &Some(home)).assert().success());
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
        assert!(
            done["basis_because"].as_str().unwrap().contains("600"),
            "{done}"
        );
    }

    /// An answer that is a yes in any other shape is still not a yes.
    #[test]
    fn an_answer_that_is_not_an_unqualified_yes_gives_desk() {
        let desk = Desk::new();
        let gh = FakeGh::new();
        let review = approved(&desk);
        let sockets = TempDir::new().unwrap();
        let (socket, _asked) = allele(
            sockets.path(),
            r#"{"result": "prompt_verified", "typed": "yes"}"#,
        );

        let done = document(&post(&desk, &gh, review, &Some(socket)).assert().success());
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
    }
}
