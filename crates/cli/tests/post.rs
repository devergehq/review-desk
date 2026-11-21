//! `review-desk post`, run as the binary — DEV-1347.
//!
//! **GitHub is faked, and nothing here touches the network.** Each test puts a `gh` of its
//! own first and alone on `PATH`: a shell script that writes what it was sent to a file and
//! answers with a canned review id, or fails, or answers something unreadable. Allele is
//! either a socket path nothing listens on or a listener in the test answering one line.

mod common;

use std::path::{Path, PathBuf};

use common::Desk;
use review_desk_store::{Actor, DraftStatus, NewDraft, NewReview, PostedTagStatus, ReviewEvent};
use serde_json::Value;
use tempfile::TempDir;

/// A fake `gh`. It records its arguments and standard input, and answers from `answer`
/// unless a file called `fail` exists beside it.
struct FakeGh {
    dir: TempDir,
}

impl FakeGh {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let script = dir.path().join("gh");
        let here = dir.path().display();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 echo \"$@\" >> '{here}/calls'\n\
                 /bin/cat > '{here}/payload.json'\n\
                 if [ -f '{here}/fail' ]; then echo 'HTTP 422: Unprocessable Entity' >&2; exit 1; fi\n\
                 /bin/cat '{here}/answer'\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let gh = FakeGh { dir };
        gh.answer(r#"{"id": 4140231301, "state": "CHANGES_REQUESTED"}"#);
        gh
    }

    fn answer(&self, text: &str) {
        std::fs::write(self.dir.path().join("answer"), text).unwrap();
    }

    fn fail(&self, failing: bool) {
        let flag = self.dir.path().join("fail");
        if failing {
            std::fs::write(flag, "").unwrap();
        } else {
            let _ = std::fs::remove_file(flag);
        }
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn payload(&self) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("payload.json")).unwrap(),
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

/// `review-desk post` with the fake `gh` alone on `PATH` and Allele at `allele`.
fn post(desk: &Desk, gh: &FakeGh, review: i64, allele: &Path) -> assert_cmd::Command {
    let line = desk.write_file("go-ahead.txt", "nothing, post it\n");
    let mut command = desk.command();
    command.env("PATH", gh.dir.path()).args([
        "post",
        &review.to_string(),
        "--session",
        "session-1",
        "--go-ahead-at",
        "1763700000",
        "--go-ahead-file",
        line.to_str().unwrap(),
        "--allele-socket",
        allele.to_str().unwrap(),
        "--json",
    ]);
    command
}

fn no_allele(desk: &Desk) -> PathBuf {
    desk.dir().join("no-allele.sock")
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
        "no Allele, so desk: {done}"
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
    assert!(
        !body.contains("nothing, post it"),
        "the line itself never posts"
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

    gh.fail(true);
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

    gh.fail(false);
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

#[cfg(unix)]
mod allele {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    use super::*;

    /// An Allele that answers one connection with `reply`, and hands back what it was asked.
    fn allele(dir: &Path, reply: &'static str) -> (PathBuf, thread::JoinHandle<String>) {
        let path = dir.join("allele.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut asked = String::new();
            BufReader::new(&stream).read_line(&mut asked).unwrap();
            writeln!(&stream, "{reply}").unwrap();
            asked
        });
        (path, handle)
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

        let done = document(&post(&desk, &gh, review, &socket).assert().success());
        assert_eq!(done["posted"]["basis"], "typed", "{done}");
        assert!(gh.payload()["body"]
            .as_str()
            .unwrap()
            .contains(" basis=typed "));

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

        let done = document(&post(&desk, &gh, review, &socket).assert().success());
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
        assert!(
            done["basis_because"]
                .as_str()
                .unwrap()
                .contains("no prompt check"),
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

        let done = document(&post(&desk, &gh, review, &socket).assert().success());
        assert_eq!(done["posted"]["basis"], "desk", "{done}");
    }
}
