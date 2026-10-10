//! What opening a database does: create, migrate, refuse.

mod common;

use common::{db_path, temp_store};
use review_desk_store::{
    newest_version, Actor, DraftStatus, FindingHunk, HunkLine, HunkLineMarker, NewDraft,
    NewFinding, NewRoundCommit, NitTicketStatus, ReviewLayerName, ReviewLayerState, RoundStatus,
    Severity, Store, StoreError, Verification, WorkFilter,
};
use tempfile::TempDir;

#[test]
fn opening_an_absent_path_creates_the_database_at_the_newest_version() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    assert!(!path.exists(), "the database should not exist yet");

    let store = Store::open(&path).expect("open creates the database");

    assert!(path.exists(), "the database file was created");
    assert_eq!(
        store.schema_version().unwrap(),
        newest_version(),
        "a fresh database lands on the newest migration"
    );
    assert!(newest_version() > 0, "there is at least one migration");
}

#[test]
fn opening_creates_missing_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("deeper").join("rd.db");

    let store = Store::open(&path).expect("open creates the directories");

    assert!(path.exists());
    assert_eq!(store.schema_version().unwrap(), newest_version());
}

#[test]
fn an_older_database_is_migrated_forward_and_keeps_its_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    // **Version 2 is where the rows below belong**, and the database is migrated from
    // there all the way to the newest version rather than from one step back.
    //
    // It used to come from `newest_version() - 1`, which read as the stricter choice and
    // quietly stopped being one. The claims further down are about what each migration
    // does to rows that predate it — that migration 4's backfill gives a rule with no
    // `updated_at` the only honest answer there is, that migration 5 leaves an existing
    // brief requiring confirmation — and a backfill only runs on the step that
    // introduces it. Coming from one version back, migration 5's step was the only one
    // exercised and migration 4's backfill had already run before the row existed, so
    // the rule assertion was checking a default rather than a backfill.
    //
    // Coming from 2 exercises every migration from 3 onward, which is more than the
    // previous arrangement ever did, and every assertion below is then about a row that
    // really did predate the migration it is making a claim about.
    const FROM: i64 = 2;
    assert!(
        newest_version() > FROM,
        "this test needs a migration to come from"
    );

    // A database held deliberately at that version, with rows written the way the code
    // wrote them then — in its own SQL, not through today's typed functions. Those
    // select the columns the newest schema has, so a migration that adds one makes them
    // unable to speak an older schema at all. That is the store's rule, not a defect:
    // one way into the record, at one version.
    {
        let old = Store::open_at_version(&path, FROM).expect("open at the older version");
        assert_eq!(old.schema_version().unwrap(), FROM);
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO reviews (id, repo, pr, head_sha, round, created_at)
                 VALUES (1, 'devergehq/review-desk', 7, 'abc123', 1, 1000);
             INSERT INTO findings
                 (review_id, seq, lens, severity, claim, disposition, created_at)
                 VALUES (1, 1, 'correctness', 'should', 'the old claim', 'in_draft', 1001);
             INSERT INTO outcomes (review_id, finding_seq, kind, actor, occurred_at)
                 VALUES (1, 1, 'kept', 'developer', 1002);
             INSERT INTO briefs (review_id, problem, option_map, updated_at)
                 VALUES (1, 'the old problem', 'A or B, written as one string', 1003);
             INSERT INTO rules
                 (id, statement, prevents, scope, scope_repo, stage, created_at)
                 VALUES (4, 'the old rule', 'the old failure', 'repo',
                         'devergehq/review-desk', 'active', 1004);",
        )
        .expect("write rows under the old schema");
    }

    // Reopened without a cap, it migrates.
    let migrated = Store::open(&path).expect("reopen migrates");

    assert_eq!(
        migrated.schema_version().unwrap(),
        newest_version(),
        "the database was migrated to the newest version"
    );

    let review = migrated
        .get_review(1)
        .expect("the row written before the migration is still there");
    assert_eq!(review.repo, "devergehq/review-desk");
    assert_eq!(review.pr, 7);
    assert_eq!(review.head_sha, "abc123");
    assert_eq!(review.round, 1);
    assert_eq!(review.pr_state, None, "the new columns start empty");
    assert_eq!(review.pr_state_at, None);
    assert_eq!(review.harvested_at, None);

    let finding = migrated.get_finding(1, 1).expect("the finding survived");
    assert_eq!(finding.claim, "the old claim");
    assert_eq!(finding.github_comment_id, None);

    // Migration 7's columns on a finding that predates it. **A review from before the
    // migration reads as a first round**, which is what DEV-873 asks for: its rounds were
    // numbered independently, so a link inferred from a matching number would join two
    // findings with nothing to do with each other.
    assert_eq!(
        finding.round_status,
        RoundStatus::New,
        "a finding raised before anything could carry one was new in its round, which is          the only honest reading of it"
    );
    assert_eq!(
        finding.continues_review_id, None,
        "and it continues nothing"
    );
    assert_eq!(finding.round_status_by, None);
    assert_eq!(finding.round_status_checked, None);
    assert_eq!(finding.round_status_at, None);
    assert_eq!(finding.round_status_commit, None);
    assert_eq!(finding.introduced_by_commit, None);
    assert_eq!(finding.introduced_by_seq, None);
    assert_eq!(
        migrated
            .rounds_open(1, 1)
            .expect("the rounds it has been open"),
        1,
        "it is open, and this is its first round"
    );
    assert!(
        migrated
            .carryable(1)
            .expect("the carryable findings")
            .is_none(),
        "a first round has no earlier round to carry from"
    );
    assert!(
        migrated
            .list_round_commits(1)
            .expect("the round's commits")
            .is_empty(),
        "and nothing has been recorded about what changed before it"
    );

    let outcomes = migrated.list_outcomes(1).expect("the outcome survived");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].kind.as_str(), "kept");
    assert_eq!(outcomes[0].harvest_key, None);

    // Migration 4's columns on rows that existed before it.
    assert_eq!(review.title, None, "the new columns start empty");
    assert_eq!(review.ticket_id, None);
    assert_eq!(review.changed_lines, None);
    assert_eq!(review.not_checked, None);
    assert_eq!(
        review.nit_ticket_status,
        NitTicketStatus::NotRequested,
        "a review nobody has asked a ticket for has not asked for one, rather than \
         holding no answer at all"
    );
    assert_eq!(review.nit_ticket_id, None);
    assert_eq!(finding.rule_id, None, "an old finding cites no rule");

    let brief = migrated.get_brief(1).expect("the brief survived");
    assert_eq!(brief.problem, "the old problem");
    assert_eq!(
        brief.option_map.as_deref(),
        Some("A or B, written as one string"),
        "the superseded option map is kept: a migration must not destroy what a review \
         already wrote"
    );
    assert!(
        brief.options.is_empty(),
        "and it is not reinterpreted as options"
    );
    assert_eq!(brief.provenance, None);
    assert_eq!(brief.confirmed_by, None);
    assert!(
        brief.confirmation_required,
        "a brief written before migration 5 was written under a rule that said the \
         developer confirms it, and backfilling it to need no confirmation would \
         retroactively excuse a confirmation that was actually waited for"
    );
    assert_eq!(brief.confirmation_not_required_because, None);
    assert!(!brief.is_settled(), "so layer 2 is still the developer's");

    // Migration 6's columns on a finding that predates it. There is no backfill and
    // there could not be an honest one: a finding written before this existed said
    // nothing about the code it was about, which is not the same as having said there
    // was none.
    assert_eq!(finding.hunk, None, "an old finding carries no hunk");
    assert_eq!(finding.flagged_line, None);
    assert_eq!(finding.inline_comment, None);
    assert_eq!(finding.why_it_matters, None);
    assert_eq!(finding.proof, None);
    assert_eq!(finding.proof_note, None);
    assert_eq!(
        finding.verification,
        Verification::Unknown,
        "a finding written before the column existed said nothing about its method, \
         which is `unknown` and not `read_not_run`: defaulting to the latter would put \
         a claim about how the review was done on every row already in the database"
    );
    assert_eq!(finding.suggested_fix, None);
    assert!(finding.fix_references.is_empty());

    // And the fields are writable against the migrated database, not merely present in
    // it: the three tables migration 6 adds exist and take rows.
    let with_evidence = migrated
        .add_finding(
            1,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "written after the migration".to_string(),
                hunk: Some(FindingHunk {
                    file: "src/lib.rs".to_string(),
                    start_line: 9,
                    lines: vec![HunkLine {
                        marker: HunkLineMarker::Added,
                        text: "    let x = 1;".to_string(),
                    }],
                }),
                flagged_line: Some(9),
                proof: Some("cargo test: 1 failed".to_string()),
                proof_note: Some("Run against a throwaway database.".to_string()),
                verification: Verification::Reproduced,
                fix_references: vec!["src/other.rs:4".to_string()],
                ..Default::default()
            },
        )
        .expect("a finding with a hunk can be written after the migration");
    assert_eq!(
        with_evidence.hunk.as_ref().map(|hunk| hunk.lines.len()),
        Some(1)
    );
    assert_eq!(with_evidence.verification, Verification::Reproduced);
    assert_eq!(
        with_evidence.proof_note.as_deref(),
        Some("Run against a throwaway database.")
    );
    assert_eq!(with_evidence.fix_references, ["src/other.rs:4"]);

    let rule = migrated.get_rule(4).expect("the rule survived");
    assert_eq!(rule.statement, "the old rule");
    assert_eq!(
        rule.updated_at, 1004,
        "a rule that existed before the column was added was last changed when it was \
         created, which is the only honest answer"
    );
    assert_eq!(rule.created_at, 1004);
    assert_eq!(rule.last_cited_at, None);
    assert_eq!(rule.review_again_at, None);
    assert_eq!(rule.why_proposed, None);
    assert_eq!(rule.note, None);

    // And the four tables migration 4 adds are usable against the migrated database,
    // not merely present in it.
    migrated
        .set_review_layer(1, 1, ReviewLayerState::Done, Some("passed"))
        .expect("a layer can be written after the migration");
    let layers = migrated.list_review_layers(1).expect("seven layers");
    assert_eq!(layers.len(), ReviewLayerName::COUNT as usize);
    assert_eq!(layers[0].state, ReviewLayerState::Done);
    assert_eq!(layers[6].state, ReviewLayerState::Pending);

    let draft = migrated
        .put_draft(1, NewDraft::ready("the body"))
        .expect("a draft can be written after the migration");
    assert_eq!(draft.status, DraftStatus::Ready);

    migrated
        .add_rule_exception(4, "accepted for the reporting tables")
        .expect("an exception can be written after the migration");

    migrated
        .confirm_brief(1, Actor::Developer)
        .expect("the brief can be confirmed after the migration");

    // And migration 7's two tables are usable against the migrated database rather than
    // merely present in it.
    let commits = migrated
        .put_round_commits(
            1,
            &[NewRoundCommit {
                sha: "a41c9e2".to_string(),
                summary: "Send an idempotency key with each delivery".to_string(),
                committed_at: Some(2000),
                files: vec!["jobs/DeliverWebhook.php".to_string()],
            }],
        )
        .expect("a round's commits can be recorded after the migration");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].files, ["jobs/DeliverWebhook.php"]);

    // And the one query migration 5 made answerable runs against the migrated database.
    // The brief is confirmed now and the draft above is `ready`, so the review is waiting
    // on the developer and nothing is waiting on an agent.
    let waiting = migrated
        .work_list(&WorkFilter::default())
        .expect("the work list can be read after the migration");
    assert!(
        waiting.is_empty(),
        "a review waiting on the developer is not an agent's work: {waiting:?}"
    );
}

#[test]
fn migrating_is_forward_only_and_reopening_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    let first = Store::open(&path).unwrap();
    let version = first.schema_version().unwrap();
    drop(first);

    let second = Store::open(&path).unwrap();
    assert_eq!(second.schema_version().unwrap(), version);

    // Asking for an older target does not roll anything back.
    let third = Store::open_at_version(&path, 1).unwrap();
    assert_eq!(
        third.schema_version().unwrap(),
        version,
        "a lower target leaves a newer database alone"
    );
}

#[test]
fn a_database_from_the_future_is_refused() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    Store::open(&path).unwrap();

    // Pretend a later build has been here.
    let future = newest_version() + 5;
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(&format!("PRAGMA user_version = {future}"))
            .unwrap();
    }

    match Store::open(&path) {
        Err(StoreError::FromTheFuture { found, known }) => {
            assert_eq!(found, future);
            assert_eq!(known, newest_version());
        }
        other => panic!("expected FromTheFuture, got {other:?}"),
    }
}

#[test]
fn foreign_keys_are_enforced_on_the_connection() {
    // SQLite leaves foreign keys off by default, which would make the store's parent
    // checks claims that nothing enforces.
    let (_dir, store) = temp_store();
    assert!(store.foreign_keys_enforced().unwrap());
}

#[test]
fn default_path_is_never_used_by_these_tests() {
    let (dir, store) = temp_store();
    let default = review_desk_store::default_db_path().expect("a home directory");

    assert!(store.path().starts_with(dir.path()));
    assert_ne!(store.path(), default.as_path());
}

#[test]
fn the_default_path_sits_under_the_home_directory() {
    let path = review_desk_store::default_db_path().expect("a home directory");
    assert!(path.ends_with("review-desk.db"));
    assert!(path.to_string_lossy().contains(".review-desk"));
}
