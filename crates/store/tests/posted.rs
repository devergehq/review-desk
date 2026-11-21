//! Posting through Review Desk, and the salted tag — DEV-1347.
//!
//! Nothing here reaches GitHub: the store records an attempt and its outcome, and the one
//! GitHub call is the command line's, faked in `crates/cli/tests/post.rs`.

mod common;

use common::{new_review, temp_store};
use review_desk_store::tag::{self, ApprovalStatement, APPROVAL_SCHEMA, TAG_MAX_BYTES};
use review_desk_store::{
    sha256_hex, Actor, DraftStatus, NewDraft, NewReview, PostBasis, PostRequest, PostedTagStatus,
    ReviewEvent, Store, StoreError, TagReading,
};
use rusqlite::Connection;

const LINE: &str = "nothing, post it";

/// A review with a draft the developer approved, as `REQUEST_CHANGES`.
fn approved_review(store: &Store, pr: i64) -> i64 {
    let review = store
        .create_review(NewReview {
            session_id: Some("session-1".to_string()),
            ..new_review("devergehq/locus", pr, "9535132", 1)
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

fn request(basis: PostBasis) -> PostRequest {
    PostRequest {
        session_id: "session-1".to_string(),
        go_ahead_line: Some(LINE.to_string()),
        go_ahead_at: Some(1_763_700_000),
        basis,
    }
}

/// The salt of one attempt, read the only way it can be: from the table itself.
fn salt_of(db: &std::path::Path, tag_id: i64) -> Vec<u8> {
    Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT salt FROM posted_tags WHERE id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn the_same_line_on_two_reviews_gives_two_line_values() {
    let (_dir, store) = temp_store();
    let one = store
        .begin_post(approved_review(&store, 64), &request(PostBasis::Desk))
        .unwrap();
    let two = store
        .begin_post(approved_review(&store, 65), &request(PostBasis::Desk))
        .unwrap();

    let one = store.get_posted_tag(one.tag_id).unwrap();
    let two = store.get_posted_tag(two.tag_id).unwrap();
    assert_eq!(one.go_ahead_line.as_deref(), Some(LINE));
    assert_eq!(two.go_ahead_line.as_deref(), Some(LINE));
    assert_ne!(one.line_hmac, two.line_hmac, "the salt is per review");
    assert_ne!(one.statement_hmac, two.statement_hmac);
}

/// **Nothing in the tag is a plain hash of anything it was made from**, so no guess about
/// the body, the threads, the statement or the line can be confirmed by hashing it.
#[test]
fn no_tag_contains_the_plain_sha256_of_any_of_its_inputs() {
    let (dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    let row = store.get_posted_tag(pending.tag_id).unwrap();
    let draft = store.get_draft(review).unwrap();
    let shown = draft.approved_shown.clone().unwrap();
    let salt = salt_of(&common::db_path(&dir), pending.tag_id);

    let inputs = [
        sha256_hex(draft.body.as_bytes()),
        shown.body_sha256.clone(),
        shown.threads_sha256.clone(),
        sha256_hex(row.statement.as_bytes()),
        sha256_hex(LINE.as_bytes()),
        sha256_hex(tag::normalise_line(LINE).as_bytes()),
        sha256_hex(&salt),
        sha256_hex(format!("review-desk/statement\0{}", row.statement).as_bytes()),
        sha256_hex(format!("review-desk/line\0{LINE}").as_bytes()),
    ];
    for input in inputs {
        assert!(
            !pending.tag.contains(&input),
            "{input} is in {}",
            pending.tag
        );
    }
    assert!(!pending.tag.contains("sha256"), "{}", pending.tag);
    assert!(!pending.tag.contains(LINE), "the line itself never posts");
}

/// **Every public fact about the post gives the statement exactly, and without the salt it
/// still cannot be checked against the tag.** The statement is rebuilt here from the pull
/// request, the round, the event and the two hashes — everything a reader of the PR could
/// learn or guess — and it is byte-for-byte the one recorded. Its plain hash and its HMAC
/// under any key but the salt are not the tag's value; under the salt, it is.
#[test]
fn the_statement_cannot_be_rebuilt_from_public_facts_without_the_salt() {
    let (dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    let row = store.get_posted_tag(pending.tag_id).unwrap();
    let shown = store.get_draft(review).unwrap().approved_shown.unwrap();

    let guessed = ApprovalStatement {
        schema: APPROVAL_SCHEMA,
        review_id: review,
        repo: "devergehq/locus".to_string(),
        pr: 64,
        head_sha: "9535132".to_string(),
        round: 1,
        event: ReviewEvent::RequestChanges,
        body_sha256: shown.body_sha256,
        threads_sha256: shown.threads_sha256,
        draft_rev: shown.draft_rev,
    }
    .canonical();
    assert_eq!(guessed, row.statement, "the guess is perfect");

    let posted = tag::parse(&pending.body).unwrap().unwrap().statement_hmac;
    assert_ne!(sha256_hex(guessed.as_bytes()), posted);
    for key in [&[][..], &[0u8; 32][..], b"review-desk"] {
        assert_ne!(tag::statement_hmac(key, &guessed), posted);
    }
    let salt = salt_of(&common::db_path(&dir), pending.tag_id);
    assert_eq!(salt.len(), 32);
    assert_eq!(
        tag::statement_hmac(&salt, &guessed),
        posted,
        "only the salt opens it"
    );
}

#[test]
fn the_statement_is_jcs_of_approval_at_1() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    let row = store.get_posted_tag(pending.tag_id).unwrap();

    let value: serde_json::Value = serde_json::from_str(&row.statement).unwrap();
    assert_eq!(value["schema"], "review-desk/approval@1");
    assert_eq!(value["event"], "request_changes");
    assert_eq!(tag::jcs(&value).as_deref(), Some(row.statement.as_str()));
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "body_sha256",
            "draft_rev",
            "event",
            "head_sha",
            "pr",
            "repo",
            "review_id",
            "round",
            "schema",
            "threads_sha256"
        ]
    );
}

#[test]
fn the_tag_is_at_most_220_bytes_with_no_newline_and_ends_the_body() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store
        .begin_post(review, &request(PostBasis::Typed))
        .unwrap();

    assert!(pending.tag.len() <= TAG_MAX_BYTES, "{}", pending.tag.len());
    assert!(!pending.tag.contains('\n'));
    assert!(pending.body.ends_with(&pending.tag));
    assert!(pending.body.starts_with("The approach fits the ticket."));
    assert!(pending.tag.contains(" basis=typed "), "{}", pending.tag);
    assert!(pending.tag.contains(" line=hmac:"), "{}", pending.tag);
}

/// **A draft changed after approval is refused, and nothing is recorded or posted.**
/// Changed the way DEV-1346's revision counts a change: a block's text, under the body.
#[test]
fn a_draft_changed_after_approval_is_refused_and_nothing_is_recorded() {
    let (dir, store) = temp_store();
    let review = approved_review(&store, 64);
    Connection::open(common::db_path(&dir))
        .unwrap()
        .execute(
            "UPDATE draft_sections SET text = 'Something else.' WHERE review_id = ?1",
            [review],
        )
        .unwrap();

    match store.begin_post(review, &request(PostBasis::Desk)) {
        Err(error @ StoreError::ShownMoved { .. }) => {
            assert_eq!(error.code(), "changed_since_shown");
            assert!(error.is_refusal());
            assert!(error.to_string().contains("nothing was posted"), "{error}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(
        store.posted_tags(review).unwrap().is_empty(),
        "no attempt was recorded"
    );
}

#[test]
fn a_sign_off_that_recorded_nothing_it_was_of_is_refused() {
    let (dir, store) = temp_store();
    let review = approved_review(&store, 64);
    Connection::open(common::db_path(&dir))
        .unwrap()
        .execute(
            "UPDATE drafts SET approved_body_sha256 = NULL, approved_threads_sha256 = NULL, \
             approved_rev = NULL WHERE review_id = ?1",
            [review],
        )
        .unwrap();

    match store.begin_post(review, &request(PostBasis::Desk)) {
        Err(StoreError::DraftStatus { found, wanted, .. }) => {
            assert_eq!(found, DraftStatus::Approved);
            assert!(wanted.contains("Send it back"), "{wanted}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(store.posted_tags(review).unwrap().is_empty());
}

#[test]
fn a_draft_that_is_not_approved_is_refused() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("devergehq/locus", 64, "9535132", 1))
        .unwrap()
        .id;
    store
        .put_draft(review, NewDraft::ready("Ready, not approved."))
        .unwrap();

    match store.begin_post(review, &request(PostBasis::Desk)) {
        Err(StoreError::DraftStatus { found, .. }) => assert_eq!(found, DraftStatus::Ready),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(store.posted_tags(review).unwrap().is_empty());
}

/// **The row is committed before the call**: another connection sees it `pending` the
/// moment `begin_post` returns, which is before the command line makes any GitHub call.
#[test]
fn the_pending_row_is_committed_before_anything_is_sent() {
    let (dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();

    let other = Store::open(common::db_path(&dir)).unwrap();
    let row = other.get_posted_tag(pending.tag_id).unwrap();
    assert_eq!(row.status, PostedTagStatus::Pending);
    assert_eq!(row.github_review_id, None);
    assert_eq!(
        other.get_draft(review).unwrap().status,
        DraftStatus::Approved
    );
}

#[test]
fn a_failed_call_leaves_the_row_failed_with_no_github_review_id() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();

    let failed = store
        .fail_post(pending.tag_id, "HTTP 422: Unprocessable Entity")
        .unwrap();
    assert_eq!(failed.status, PostedTagStatus::Failed);
    assert_eq!(failed.github_review_id, None);
    assert_eq!(failed.posted_at, None);
    assert_eq!(
        failed.failure.as_deref(),
        Some("HTTP 422: Unprocessable Entity")
    );
    assert_eq!(
        store.get_draft(review).unwrap().status,
        DraftStatus::Approved
    );
    assert_eq!(store.get_review(review).unwrap().github_review_id, None);
}

/// **While an attempt is pending, a second post is refused**: it may have reached GitHub.
#[test]
fn a_second_post_is_refused_while_one_is_pending() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let first = store.begin_post(review, &request(PostBasis::Desk)).unwrap();

    match store.begin_post(review, &request(PostBasis::Desk)) {
        Err(error @ StoreError::PostInFlight { .. }) => {
            assert_eq!(error.code(), "post_in_flight");
            assert!(error.is_refusal());
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(store.posted_tags(review).unwrap().len(), 1);
    assert_eq!(
        store.get_posted_tag(first.tag_id).unwrap().status,
        PostedTagStatus::Pending
    );
}

/// **A failed attempt is known not to have gone up, so it may be retried**, and the retry
/// is a new attempt with a salt of its own.
#[test]
fn a_failed_post_may_be_retried_and_the_success_marks_the_draft_posted() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let first = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    store
        .fail_post(first.tag_id, "gh is not logged in")
        .unwrap();

    let second = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    assert_ne!(first.tag, second.tag, "a new salt");
    let posted = store.finish_post(second.tag_id, "4140231301").unwrap();

    assert_eq!(posted.status, PostedTagStatus::Posted);
    assert_eq!(posted.github_review_id.as_deref(), Some("4140231301"));
    assert!(posted.posted_at.is_some());
    let draft = store.get_draft(review).unwrap();
    assert_eq!(draft.status, DraftStatus::Posted);
    assert_eq!(
        store
            .get_review(review)
            .unwrap()
            .github_review_id
            .as_deref(),
        Some("4140231301")
    );

    match store.begin_post(review, &request(PostBasis::Desk)) {
        Err(StoreError::DraftStatus { found, .. }) => assert_eq!(found, DraftStatus::Posted),
        other => panic!("a posted review is not posted again, got {other:?}"),
    }
}

#[test]
fn an_outcome_is_reported_once() {
    let (_dir, store) = temp_store();
    let review = approved_review(&store, 64);
    let pending = store.begin_post(review, &request(PostBasis::Desk)).unwrap();
    store.fail_post(pending.tag_id, "refused").unwrap();

    assert!(store.finish_post(pending.tag_id, "1").is_err());
    assert!(store.fail_post(pending.tag_id, "again").is_err());
}

/// The four readings the developer is shown, from the body GitHub hands back.
#[test]
fn a_body_read_back_is_one_of_the_four_readings() {
    let (_dir, store) = temp_store();
    let typed = approved_review(&store, 64);
    let desk = approved_review(&store, 65);
    let typed = store.begin_post(typed, &request(PostBasis::Typed)).unwrap();
    let desk = store.begin_post(desk, &request(PostBasis::Desk)).unwrap();
    store.finish_post(typed.tag_id, "1").unwrap();
    store.finish_post(desk.tag_id, "2").unwrap();

    let held_typed = store.read_tag(&typed.body).unwrap();
    assert!(matches!(
        held_typed,
        TagReading::Held {
            basis: PostBasis::Typed,
            ..
        }
    ));
    assert!(held_typed.sentence().contains("Allele saw it typed"));
    assert!(matches!(
        store.read_tag(&desk.body).unwrap(),
        TagReading::Held {
            basis: PostBasis::Desk,
            ..
        }
    ));

    // Another Mac's tag: well-formed, and not in this record.
    let (_other_dir, other) = temp_store();
    assert_ne!(other.install_id().unwrap(), store.install_id().unwrap());
    assert_eq!(other.read_tag(&typed.body).unwrap(), TagReading::NotHeld);

    // A basis rewritten on GitHub no longer matches what this record holds.
    let forged = typed.body.replace("basis=typed", "basis=desk");
    assert_eq!(store.read_tag(&forged).unwrap(), TagReading::NotHeld);

    assert_eq!(store.read_tag("Looks good.").unwrap(), TagReading::NoTag);
    let twice = format!("{}\n\n{}", typed.body, typed.tag);
    assert!(matches!(
        store.read_tag(&twice).unwrap(),
        TagReading::Rejected { .. }
    ));
}

/// **One install id per database, written once.** Twelve hex characters, so that the tag
/// fits; the same on every open.
#[test]
fn the_install_id_is_random_per_database_and_does_not_change() {
    let (dir, store) = temp_store();
    let id = store.install_id().unwrap();
    assert_eq!(id.len(), 12);
    assert!(id
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    drop(store);
    assert_eq!(
        Store::open(common::db_path(&dir))
            .unwrap()
            .install_id()
            .unwrap(),
        id
    );
}
