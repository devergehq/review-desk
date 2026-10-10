//! Forward-only schema migrations.
//!
//! Each migration is a `(version, name, sql)` triple in `MIGRATIONS`, ordered by
//! version with no gaps. Opening a database applies every migration above its current
//! `PRAGMA user_version`, each in its own transaction, and records the new version. A
//! migration that has shipped is never edited; the next change is a new entry.
//!
//! The split is not arbitrary: version 1 is the review record, version 2 is the
//! learning layer the project brief treats as a later concern, version 3 is what the
//! harvester needs to read a posted review's outcomes back from GitHub, version 4
//! is what the local API contract needs — the draft, the per-layer status, the
//! structured option map, rule exceptions, and the twelve columns the four screens
//! read — version 5 is the one fact a brief could not hold: whether the developer
//! has to confirm it at all, and version 6 is what a finding needs in order to mean
//! something on its own: the code it is about, the comment as it will post, the
//! consequence, the proof and the fix.
//!
//! **A migration takes the next free version at the time it merges, and nothing here
//! predicts which number that will be.** Three branches were adding a migration at once
//! when version 6 was written, so an earlier draft of this comment named the branch it
//! expected to merge first and the number it expected to end up with — and the merge
//! order changed under it, which is the whole argument for stating the rule instead of
//! the outcome. `versions_are_ordered_from_one_with_no_gaps` below is the rule: it refuses
//! a gap and refuses a repeat, so a rebase that leaves this number wrong in either
//! direction fails loudly rather than reaching `master` quietly. Renumber to whatever is
//! free and let the test confirm it.
//!
//! Version 7 is what gives a finding an identity that outlives the round it was raised
//! in: the round it continues, what became of it in this one, what a new one is
//! attributed to, and the commits between two rounds. It was written as 6 while version
//! 6 was being written too, and renumbered on the rebase — which is the rule above doing
//! exactly what it is for.
//!
//! Version 8 is where a review's understanding came from: its sources, the ticket's
//! criteria, the description lint's individual results and the per-file change map. It
//! was written as 6 and rebased twice, which is the rule above being used rather than
//! quoted.

use rusqlite::Connection;

use crate::error::{Result, StoreError};

/// One migration.
#[derive(Debug)]
pub struct Migration {
    /// The `user_version` the database holds once this has been applied.
    pub version: i64,
    /// A short name, used in error messages.
    pub name: &'static str,
    /// The statements to run. Executed as a batch inside one transaction.
    pub sql: &'static str,
}

/// Every migration, in order. Append only.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "review-record",
        sql: r#"
CREATE TABLE reviews (
    id               INTEGER PRIMARY KEY,
    repo             TEXT    NOT NULL,
    pr               INTEGER NOT NULL,
    head_sha         TEXT    NOT NULL,
    round            INTEGER NOT NULL,
    session_id       TEXT,
    ledger_id        TEXT,
    github_review_id TEXT,
    created_at       INTEGER NOT NULL,
    UNIQUE (repo, pr, round)
);

CREATE INDEX reviews_by_head ON reviews (repo, pr, head_sha);

CREATE TABLE briefs (
    review_id        INTEGER PRIMARY KEY REFERENCES reviews (id) ON DELETE CASCADE,
    problem          TEXT    NOT NULL,
    diagram          TEXT,
    option_map       TEXT,
    approach_verdict TEXT,
    confirmed_at     INTEGER,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE findings (
    review_id          INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    seq                INTEGER NOT NULL,
    lens               TEXT    NOT NULL,
    severity           TEXT    NOT NULL,
    file               TEXT,
    line               INTEGER,
    claim              TEXT    NOT NULL,
    evidence           TEXT,
    disposition        TEXT    NOT NULL,
    disposition_reason TEXT,
    created_at         INTEGER NOT NULL,
    PRIMARY KEY (review_id, seq)
);

CREATE TABLE outcomes (
    id          INTEGER PRIMARY KEY,
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    actor       TEXT    NOT NULL,
    occurred_at INTEGER NOT NULL,
    detail      TEXT,
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);

CREATE INDEX outcomes_by_finding ON outcomes (review_id, finding_seq);
"#,
    },
    Migration {
        version: 2,
        name: "rules",
        sql: r#"
CREATE TABLE rules (
    id               INTEGER PRIMARY KEY,
    statement        TEXT    NOT NULL,
    prevents         TEXT    NOT NULL,
    scope            TEXT    NOT NULL,
    scope_repo       TEXT,
    stage            TEXT    NOT NULL,
    origin_review_id INTEGER REFERENCES reviews (id) ON DELETE SET NULL,
    created_at       INTEGER NOT NULL,
    CHECK (
        (scope = 'global' AND scope_repo IS NULL)
        OR (scope = 'repo' AND scope_repo IS NOT NULL AND scope_repo <> '')
    )
);

CREATE INDEX rules_by_scope ON rules (scope, scope_repo, stage);

CREATE TABLE rule_findings (
    rule_id     INTEGER NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    PRIMARY KEY (rule_id, review_id, finding_seq),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);
"#,
    },
    Migration {
        version: 3,
        name: "harvest",
        sql: r#"
-- The comment a finding was posted as. Set by `review-desk finding link`, and the
-- only handle the harvester has on a finding's thread.
ALTER TABLE findings ADD COLUMN github_comment_id TEXT;

-- One GitHub comment is one finding. The index is partial because most findings have
-- no linked comment, and SQLite would otherwise have to be trusted to treat every one
-- of those NULLs as distinct.
CREATE UNIQUE INDEX findings_by_github_comment
    ON findings (github_comment_id)
    WHERE github_comment_id IS NOT NULL;

-- What the pull request was last seen to be, when it reached that state, and when the
-- harvester last looked. A review whose pull request is settled is not fetched again.
ALTER TABLE reviews ADD COLUMN pr_state     TEXT;
ALTER TABLE reviews ADD COLUMN pr_state_at  INTEGER;
ALTER TABLE reviews ADD COLUMN harvested_at INTEGER;

CREATE INDEX reviews_by_pr_state ON reviews (pr_state);

-- The identity of the GitHub fact an outcome was harvested from: a reply comment's id,
-- a thread's root comment id, a commit. It is what makes a second harvest write
-- nothing: the index below refuses the row rather than the code remembering not to
-- write it.
--
-- The index is partial so that an outcome recorded by hand, which has no key, is never
-- caught by it. Two developers may record "kept" against one finding twice; two
-- harvests of one reply may not.
ALTER TABLE outcomes ADD COLUMN harvest_key TEXT;

CREATE UNIQUE INDEX outcomes_by_harvest_key
    ON outcomes (review_id, finding_seq, kind, harvest_key)
    WHERE harvest_key IS NOT NULL;
"#,
    },
    Migration {
        version: 4,
        name: "contract",
        sql: r#"
-- The draft review: one per review, replaced in place. An approved draft is what the
-- review session reads and posts, which is why editing one is refused rather than
-- merged: the characters that get posted must be the characters that were approved.
CREATE TABLE drafts (
    review_id                INTEGER PRIMARY KEY REFERENCES reviews (id) ON DELETE CASCADE,
    body                     TEXT    NOT NULL,
    status                   TEXT    NOT NULL,
    event                    TEXT,
    approved_at              INTEGER,
    -- Who approved it. Only the developer signs off, so this is the record of whose
    -- assent was given; the store refuses any other actor.
    approved_by              TEXT,
    sent_back_note           TEXT,
    sent_back_at             INTEGER,
    posted_at                INTEGER,
    lint_passes              INTEGER,
    lint_visible_words       INTEGER,
    lint_word_budget         INTEGER,
    lint_findings_in_body    INTEGER,
    lint_findings_expected   INTEGER,
    lint_raised_before_on_pr INTEGER,
    lint_errors              TEXT,
    lint_checked_at          INTEGER,
    updated_at               INTEGER NOT NULL,
    CHECK (status IN ('drafting', 'ready', 'approved', 'sent_back', 'posted')),
    CHECK (event IS NULL OR event IN ('comment', 'request_changes', 'approve')),
    -- The state machine, enforced by the database rather than remembered by the code.
    -- An approved or posted draft names the event it is to be posted as, who approved
    -- it and when; anything earlier names none of them.
    CHECK (
        (status IN ('approved', 'posted'))
        = (event IS NOT NULL AND approved_at IS NOT NULL AND approved_by IS NOT NULL)
    ),
    -- One direction only. A draft that was sent back and then reworked is `ready`
    -- again and keeps the note, because what the developer asked for is part of the
    -- record even once it has been done.
    CHECK (status <> 'sent_back' OR (sent_back_at IS NOT NULL AND sent_back_note IS NOT NULL)),
    CHECK ((status = 'posted') = (posted_at IS NOT NULL))
);

-- The Inbox groups on this, across every repository.
CREATE INDEX drafts_by_status ON drafts (status);

-- One row per layer that ran. Stored rather than derived: layer 1's lint result has
-- nowhere to be derived from, and a derived status cannot tell a layer that was
-- skipped from one that has not run — which is the distinction "what was not checked"
-- exists to make.
--
-- The layer's *name* is not a column. It is fixed by its number, so storing both
-- would let a row say layer 5 is called `lint`.
CREATE TABLE review_layers (
    review_id  INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    number     INTEGER NOT NULL,
    state      TEXT    NOT NULL,
    detail     TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (review_id, number),
    CHECK (number BETWEEN 1 AND 7),
    CHECK (state IN ('pending', 'running', 'done', 'waiting_on_you', 'failed', 'skipped'))
);

CREATE INDEX review_layers_by_state ON review_layers (state, number);

-- The alternatives considered, written before the diff was read. One row per option,
-- replacing `briefs.option_map` as the thing a screen reads: an opaque string cannot
-- serve a card with a title, an argument each way and a badge.
--
-- Hung off the brief, not the review: an option map with no brief is a set of
-- arguments about nothing.
CREATE TABLE brief_options (
    review_id        INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    key              TEXT    NOT NULL,
    title            TEXT    NOT NULL,
    argument_for     TEXT    NOT NULL,
    argument_against TEXT    NOT NULL,
    chosen           INTEGER NOT NULL,
    PRIMARY KEY (review_id, key),
    CHECK (key <> ''),
    CHECK (title <> '' AND argument_for <> '' AND argument_against <> ''),
    CHECK (chosen IN (0, 1))
);

-- An accepted exception to a rule, with the reason it was accepted. A rule with an
-- exception is still in force; the screen says so next to its stage.
CREATE TABLE rule_exceptions (
    id         INTEGER PRIMARY KEY,
    rule_id    INTEGER NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    reason     TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK (reason <> '')
);

CREATE INDEX rule_exceptions_by_rule ON rule_exceptions (rule_id);

-- The pull request's own facts. The store had nowhere to put any of them, so every
-- screen that shows a review showed its number and not its name.
ALTER TABLE reviews ADD COLUMN title         TEXT;
ALTER TABLE reviews ADD COLUMN ticket_id     TEXT;
ALTER TABLE reviews ADD COLUMN changed_lines INTEGER;

-- What the review did not look at, in the reviewer's own words. The one section of a
-- review with no adversary, so it is stored rather than derived.
ALTER TABLE reviews ADD COLUMN not_checked   TEXT;

-- The one ticket covering every nit held back from this review's draft. The findings
-- it covers are not stored against it: they are every finding on the review at
-- disposition `held_back`, so the set cannot drift from the dispositions.
--
-- SQLite cannot add a CHECK to an existing table, so the closed set is enforced on the
-- way in and on the way out by `NitTicketStatus` rather than by the schema. The
-- default is what gives every row that already exists an honest value.
ALTER TABLE reviews ADD COLUMN nit_ticket_status       TEXT NOT NULL DEFAULT 'none';
ALTER TABLE reviews ADD COLUMN nit_ticket_id           TEXT;
ALTER TABLE reviews ADD COLUMN nit_ticket_requested_at INTEGER;
ALTER TABLE reviews ADD COLUMN nit_ticket_filed_at     INTEGER;

-- Where the problem statement came from and what the developer changed about it.
ALTER TABLE briefs ADD COLUMN provenance   TEXT;

-- Who confirmed the brief. Only the developer signs off; this is the record of it.
ALTER TABLE briefs ADD COLUMN confirmed_by TEXT;

-- The rule a finding was raised under. The store recorded rule-to-finding and never
-- finding-to-rule, so "cited by 6 later findings" and a rule's staleness could not be
-- answered at all. The whole feedback loop runs through this column.
--
-- SET NULL rather than CASCADE: deleting a rule must not delete the findings that
-- cited it. The finding was still raised; it just no longer cites anything.
ALTER TABLE findings ADD COLUMN rule_id INTEGER REFERENCES rules (id) ON DELETE SET NULL;

-- Partial, because most findings cite no rule.
CREATE INDEX findings_by_rule ON findings (rule_id) WHERE rule_id IS NOT NULL;

-- The proposal's justification, and the free line the screen shows under the
-- statement. `note` is also where the reason for a stage change lands, so a dismissed
-- proposal is not proposed again next week with no memory of having been told no.
ALTER TABLE rules ADD COLUMN why_proposed    TEXT;
ALTER TABLE rules ADD COLUMN note            TEXT;

-- When a finding last cited the rule. Stored rather than scanned on every Rules load;
-- it cannot drift, because a finding is never deleted, only dispositioned.
ALTER TABLE rules ADD COLUMN last_cited_at   INTEGER;

-- Set by "Keep 60 more days". Without it a deferred rule comes back as stale on the
-- next load, and the developer's decision to keep it is lost.
ALTER TABLE rules ADD COLUMN review_again_at INTEGER;

-- A rule had `created_at` only, so a promotion, dismissal or deferral left no trace of
-- when it happened. The default makes the column NOT NULL on a table that already has
-- rows; the backfill then gives those rows the only honest answer there is, which is
-- when they were created. 0 is not a time any rule was created at.
ALTER TABLE rules ADD COLUMN updated_at      INTEGER NOT NULL DEFAULT 0;
UPDATE rules SET updated_at = created_at WHERE updated_at = 0;
"#,
    },
    Migration {
        version: 5,
        name: "optional-confirmation",
        sql: r#"
-- Whether the developer has to confirm this brief at all.
--
-- **This is the one place an agent decides a developer step is unnecessary**, so the
-- default is the strict answer and it is what every brief already written gets: a brief
-- stored before this migration was stored under a rule that said the developer confirms,
-- and backfilling it to "not required" would retroactively excuse confirmations that
-- were actually waited for.
--
-- The caller owns the threshold. Patrick's decision of 9 October 2026 is that a brief is
-- confirmed only on pull requests changing more than 200 lines, and Review Desk does not
-- hold that number: it records that the caller said no confirmation was needed, and the
-- reason the caller gave.
ALTER TABLE briefs ADD COLUMN confirmation_required INTEGER NOT NULL DEFAULT 1;

-- Why the caller said so, in the caller's own words. Required when confirmation is not
-- required and refused when it is, which SQLite cannot express as a CHECK on a table
-- that already exists — so `put_brief` enforces it on the way in, the way
-- `NitTicketStatus` does for the column migration 4 added to `reviews`.
--
-- A brief that needs no confirmation with no reason attached would put layer 2 at `done`
-- with nothing on the record saying who decided that or why, which is the state this
-- whole field exists to avoid.
ALTER TABLE briefs ADD COLUMN confirmation_not_required_because TEXT;
"#,
    },
    Migration {
        version: 6,
        name: "finding-evidence",
        sql: r#"
-- What a finding is about, as the agent wrote it.
--
-- A finding held a file, a line, a claim and one line of evidence. Boards A2 and A3 of
-- the approved design draw the code, the comment as it will post, the consequence, the
-- proof and the fix — so the developer had to go and work out what a finding meant
-- before they could decide anything about it. These columns are what it means.
--
-- **Every one is optional and they are independent.** A finding with none of them
-- displays as it does today, with its file and its line. That is a rule of DEV-874, and
-- it is also what makes this migration safe on a database that already has findings in
-- it: no backfill, because there is no honest value to backfill.
ALTER TABLE findings ADD COLUMN flagged_line    INTEGER;
ALTER TABLE findings ADD COLUMN inline_comment  TEXT;
ALTER TABLE findings ADD COLUMN why_it_matters  TEXT;
ALTER TABLE findings ADD COLUMN proof           TEXT;
ALTER TABLE findings ADD COLUMN suggested_fix   TEXT;

-- The plain caption beneath the proof — "A throwaway test, not committed."
--
-- Its own column rather than more of `proof`, because the two are drawn differently: the
-- proof is a monospace block and this is a sentence about it. Both boards draw both, and
-- a caption folded into `proof` would render as though it were test output.
ALTER TABLE findings ADD COLUMN proof_note      TEXT;

-- Whether the proof was reproduced or only read.
--
-- `unknown` is the default, and it is the only honest value for a finding written before
-- this column existed: such a finding said nothing about its method, which is not the
-- same as having said it was read and not run. The closed set is enforced on the way in
-- and on the way out by `Verification` rather than by a CHECK, because SQLite cannot add
-- one to a table that already exists — the same arrangement migration 4 made for
-- `reviews.nit_ticket_status`.
ALTER TABLE findings ADD COLUMN verification TEXT NOT NULL DEFAULT 'unknown';

-- The hunk's own two facts. One row per finding that has a hunk, so that "this finding
-- has no hunk" is one fact rather than two nullable columns that can disagree: a file
-- with no start line is not a hunk anybody can draw, and SQLite cannot add a CHECK to
-- `findings` to forbid it.
--
-- `start_line` is the number the hunk's first *numbered* line carries. A removed line
-- carries none, so board A3's F6 opens on a removed line and still numbers from 58.
-- The number beside each line is derived from this and the markers below; it is not
-- stored, because storing both would let a row say line 58 is the fourth line of a hunk
-- that starts at 56.
CREATE TABLE finding_hunks (
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    file        TEXT    NOT NULL,
    start_line  INTEGER NOT NULL,
    PRIMARY KEY (review_id, finding_seq),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE,
    CHECK (file <> ''),
    CHECK (start_line >= 1)
);

-- The lines, in the order they are to be drawn.
--
-- A table rather than one TEXT column holding a unified diff. **The marker has to survive
-- a line whose own text begins with a `+`, and the text has to survive being nothing but
-- a space.** In a diff those two facts share one character position and one trailing
-- newline, and Review Desk does not fetch code — the hunk it serves is the only copy
-- there is, so a round trip that is merely usually right is not good enough.
--
-- `position` is the order. Not the rowid, and not insertion order: a read that depended
-- on either would be a read whose correctness nothing states.
CREATE TABLE finding_hunk_lines (
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    position    INTEGER NOT NULL,
    marker      TEXT    NOT NULL,
    text        TEXT    NOT NULL,
    PRIMARY KEY (review_id, finding_seq, position),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE,
    CHECK (position >= 1),
    CHECK (marker IN ('added', 'removed', 'unchanged'))
);

-- Sibling code the fix should look like — "SendInvoiceEmail.php:44" — in the order given.
--
-- A reference is a pointer the page draws as one line, so a blank one draws as a blank
-- line and means nothing. That is the one piece of text in this migration the store looks
-- at before storing: it is refused by name, and whatever is stored is stored verbatim.
CREATE TABLE finding_fix_references (
    review_id   INTEGER NOT NULL,
    finding_seq INTEGER NOT NULL,
    position    INTEGER NOT NULL,
    reference   TEXT    NOT NULL,
    PRIMARY KEY (review_id, finding_seq, position),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE,
    CHECK (position >= 1),
    CHECK (reference <> '')
);
"#,
    },
    Migration {
        version: 7,
        name: "rounds",
        sql: r#"
-- The earlier round whose finding this one continues. **The seq is not stored beside it,
-- because it cannot differ**: a carried finding keeps the number it had, so the link is
-- "the finding with this same seq, on that review". `carry_finding` is the only thing
-- that writes the column and it reuses the number by construction, which is a stronger
-- guarantee than two columns that could disagree — and SQLite cannot add the composite
-- foreign key that would police them anyway.
--
-- SET NULL rather than CASCADE, for the reason `findings.rule_id` gives: deleting an
-- earlier round must not delete the finding that continued from it. The finding was
-- still raised; it just no longer says where it came from.
--
-- NULL is the honest value for every finding already on the record. A review from before
-- this migration has no links and reads as a first round, which is what DEV-873 asks for:
-- its rounds were numbered independently, so inferring a link from a matching seq would
-- connect two findings that have nothing to do with each other.
ALTER TABLE findings ADD COLUMN continues_review_id INTEGER
    REFERENCES reviews (id) ON DELETE SET NULL;

CREATE INDEX findings_by_continues
    ON findings (continues_review_id)
    WHERE continues_review_id IS NOT NULL;

-- What became of the finding *in this round*: new, fixed, still_open, accepted,
-- withdrawn, ticketed. Distinct from `disposition`, which is what is to become of it in
-- this round's draft — a finding can be `fixed` and so in no draft at all.
--
-- The default is what gives every finding already on the record an honest value: each one
-- was new in the round it was raised in, because nothing before this migration could
-- carry one. SQLite cannot add a CHECK to a table that already exists, so the closed set
-- is enforced on the way in and out by `RoundStatus`, as `NitTicketStatus` is for the
-- column migration 4 added to `reviews`.
ALTER TABLE findings ADD COLUMN round_status TEXT NOT NULL DEFAULT 'new';

CREATE INDEX findings_by_round_status ON findings (review_id, round_status);

-- Who decided that status, how they checked it, when, and the commit it is attributed to.
--
-- `round_status_by` is a `DecidedBy` — developer, agent or author — and **deliberately not
-- an `Actor`**. An agent decides most of these, `Actor` has no value for one, and
-- recording an agent's decision as `developer` is the mistake `SignOff::Waived` was added
-- to stop: a record that says the developer checked something nobody showed them.
--
-- `round_status_commit` is the commit the status rests on: for a fixed finding, the commit
-- that fixed it. It is a plain string, never parsed — Review Desk reads GitHub and stores
-- what it read.
ALTER TABLE findings ADD COLUMN round_status_by      TEXT;
ALTER TABLE findings ADD COLUMN round_status_checked TEXT;
ALTER TABLE findings ADD COLUMN round_status_at      INTEGER;
ALTER TABLE findings ADD COLUMN round_status_commit  TEXT;

-- What a new finding is attributed to: the commit that introduced it, and the earlier
-- finding whose fix caused it — "introduced by a41c9e2, the commit that fixed F1".
--
-- `introduced_by_seq` is a seq and no review id, and that is the point of this migration:
-- after it, a finding's number is its identity across every round of the pull request, so
-- the number alone names the finding. Which round it was raised in is a separate question,
-- answered by walking `continues_review_id`.
ALTER TABLE findings ADD COLUMN introduced_by_commit TEXT;
ALTER TABLE findings ADD COLUMN introduced_by_seq    INTEGER;

-- The commits between the previous round's head and this round's — "what changed since
-- round 1". Stored against the round that reviewed them, so a round's commits are the ones
-- it was answerable for.
--
-- Read from GitHub through the harvest crate and written here. Nothing is written to
-- GitHub. `summary` is the commit message's first line, as GitHub sent it.
--
-- `position` is the order GitHub listed them in, oldest first. Stored rather than sorted
-- by `committed_at`: a rebased or amended commit can carry a committer date out of order,
-- and the order the page shows is the order the pull request has.
CREATE TABLE round_commits (
    review_id    INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    sha          TEXT    NOT NULL,
    position     INTEGER NOT NULL,
    summary      TEXT    NOT NULL,
    committed_at INTEGER,
    PRIMARY KEY (review_id, sha),
    CHECK (sha <> ''),
    CHECK (position >= 0)
);

CREATE INDEX round_commits_by_position ON round_commits (review_id, position);

-- The files one of those commits touched. A table rather than a joined string, because
-- "was this finding's file changed since round 1" is the question the `late` flag is
-- answered from, and that is a lookup rather than a substring search.
CREATE TABLE round_commit_files (
    review_id INTEGER NOT NULL,
    sha       TEXT    NOT NULL,
    path      TEXT    NOT NULL,
    PRIMARY KEY (review_id, sha, path),
    CHECK (path <> ''),
    FOREIGN KEY (review_id, sha)
        REFERENCES round_commits (review_id, sha) ON DELETE CASCADE
);

CREATE INDEX round_commit_files_by_path ON round_commit_files (review_id, path);
"#,
    },
    Migration {
        version: 8,
        name: "provenance",
        sql: r#"
-- Where the review read the problem from: the ticket, the pull request description, a
-- comment. **Written by the agent through the command line; Review Desk reads no ticket.**
--
-- Keyed on the position rather than on the label, because two comments on one pull
-- request are two sources with the same kind and nothing to tell them apart but their
-- order. The whole list is replaced on every write, so the position is the store's to
-- assign and no caller manages numbering.
CREATE TABLE review_sources (
    review_id INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    kind      TEXT    NOT NULL,
    label     TEXT    NOT NULL,
    link      TEXT,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (kind IN ('ticket', 'description', 'comment')),
    CHECK (label <> ''),
    CHECK (link IS NULL OR link <> '')
);

-- What the ticket asks for, one row per criterion, with a status and one pointer.
--
-- The pointer is one of three things and never two: a finding, a place in the code, or a
-- note. Two pointers on one criterion would leave nothing able to say which the screen
-- follows, so the schema refuses it rather than the display choosing.
--
-- **A criterion that is not met names a finding.** DEV-875's rule, enforced here: a review
-- that says the change fails a criterion and raises nothing about it posts a review that
-- never mentions the failure. The finding has to exist on this review, which is why the
-- foreign key is composite — the same shape `outcomes` uses, so a criterion cannot point
-- at another review's F1.
--
-- Since migration 7 a finding's number is its identity across rounds, so `finding_seq`
-- names one finding on the pull request rather than one per round, and a criterion in a
-- later round pointing at F2 means the same F2 the first round raised. The foreign key
-- still resolves because a carried finding is written as a row under the later review
-- keeping its number — and a criterion pointing at a finding that was *not* carried is
-- still refused, which is right: a criterion whose finding is closed is a criterion whose
-- status should have changed.
--
-- **A criterion is not carried between rounds, and that is a decision rather than an
-- omission.** It belongs to the review, not to the finding: it is this round's assessment
-- of what the ticket asks for, and copying it forward would assert that a later round
-- reached the same verdict with no agent having looked. DEV-873's
-- `every_column_of_a_finding_is_decided_about_when_it_is_carried` is what forces that
-- choice to be made for anything hanging off a finding, and it finds those tables by the
-- name prefix `finding%` — which this table does not carry, so the guard cannot ask. The
-- answer is recorded here instead, and `a_carried_finding_brings_no_criteria_with_it` in
-- `crates/store/tests/provenance.rs` is what enforces it.
CREATE TABLE review_criteria (
    review_id   INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    text        TEXT    NOT NULL,
    status      TEXT    NOT NULL,
    finding_seq INTEGER,
    file        TEXT,
    line        INTEGER,
    note        TEXT,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (status IN ('met', 'not_met', 'not_in_this_pr')),
    CHECK (text <> ''),
    CHECK (file IS NULL OR file <> ''),
    CHECK (note IS NULL OR note <> ''),
    CHECK (line IS NULL OR (file IS NOT NULL AND line >= 1)),
    -- At most one pointer. SQLite has no boolean type; a comparison yields 0 or 1, so
    -- adding the three is the count of pointers given.
    CHECK ((finding_seq IS NOT NULL) + (file IS NOT NULL) + (note IS NOT NULL) <= 1),
    CHECK (status <> 'not_met' OR finding_seq IS NOT NULL),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);

-- The change map: one row per file the pull request touched, plus a row per area that was
-- deliberately not reviewed.
--
-- `checked` is how the file was looked at and `how_checked` is the sentence the screen
-- shows — "Read in full. Compared with 2 sibling senders." A row that was **not reviewed
-- must say why**, which is the one thing an area with no coverage owes a reader.
--
-- **The findings on a file are not a column here.** They are every finding on the review
-- whose `file` is this path, read at the time of asking, so the table and the findings
-- cannot drift apart — the same argument the held-back nits' ticket makes for reading its
-- findings from the dispositions rather than storing them.
CREATE TABLE review_files (
    review_id     INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    path          TEXT    NOT NULL,
    summary       TEXT    NOT NULL,
    lines_added   INTEGER NOT NULL DEFAULT 0,
    lines_removed INTEGER NOT NULL DEFAULT 0,
    checked       TEXT    NOT NULL,
    how_checked   TEXT,
    PRIMARY KEY (review_id, path),
    UNIQUE (review_id, position),
    CHECK (position >= 1),
    CHECK (path <> '' AND summary <> ''),
    CHECK (lines_added >= 0 AND lines_removed >= 0),
    CHECK (checked IN ('read_in_full', 'read_in_part', 'run', 'not_reviewed')),
    CHECK (how_checked IS NULL OR how_checked <> ''),
    CHECK (checked <> 'not_reviewed' OR how_checked IS NOT NULL)
);

-- Layer 1's result, as the individual checks rather than one verdict. The review page
-- shows three named lines — the description's length against its budget, where the working
-- notes are, which template headings are present — and "passes" alone can serve none of
-- them.
--
-- **The overall verdict is not stored.** It is "no result failed", derived on the way out,
-- so a stored `passes: true` can never sit above a list containing a failure.
CREATE TABLE description_lint_results (
    review_id INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    label     TEXT    NOT NULL,
    status    TEXT    NOT NULL,
    detail    TEXT,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (status IN ('pass', 'warn', 'fail')),
    CHECK (label <> ''),
    CHECK (detail IS NULL OR detail <> '')
);
"#,
    },
];

/// The version a freshly created database lands on.
pub fn newest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// The version this database currently holds.
pub fn applied_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// Apply every migration above the current version, up to and including `target`.
///
/// Forward-only: a database already at or above `target` is left alone. A database
/// beyond the newest migration this build knows is refused rather than guessed at.
pub fn migrate_to(conn: &Connection, target: i64) -> Result<i64> {
    let from = applied_version(conn)?;
    if from > newest_version() {
        return Err(StoreError::FromTheFuture {
            found: from,
            known: newest_version(),
        });
    }

    for migration in MIGRATIONS {
        if migration.version <= from || migration.version > target {
            continue;
        }
        apply(conn, migration)?;
    }

    applied_version(conn)
}

/// Apply one migration and record its version, atomically.
///
/// The version is read again inside the transaction. Two processes opening a fresh
/// database at the same moment both see version 0 outside it; without the second read
/// the loser would run a migration that has just been applied and fail on a table that
/// already exists.
fn apply(conn: &Connection, migration: &Migration) -> Result<()> {
    let run = || -> rusqlite::Result<bool> {
        // IMMEDIATE takes the write lock before the read, so the version this reads
        // cannot change under it and a competing process waits rather than racing.
        conn.execute_batch("BEGIN IMMEDIATE")?;

        let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current >= migration.version {
            // Someone else applied it while we were waiting for the lock.
            conn.execute_batch("COMMIT")?;
            return Ok(false);
        }

        conn.execute_batch(migration.sql)?;
        // PRAGMA will not take a bound parameter, so the version is formatted in. It
        // comes from the const table above, never from a caller.
        conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))?;
        conn.execute_batch("COMMIT")?;
        Ok(true)
    };

    run().map(|_| ()).map_err(|source| {
        let _ = conn.execute_batch("ROLLBACK");
        StoreError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_ordered_from_one_with_no_gaps() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                migration.version,
                index as i64 + 1,
                "migration {:?} is out of order",
                migration.name
            );
        }
    }

    #[test]
    fn newest_version_is_the_last_migration() {
        assert_eq!(newest_version(), MIGRATIONS.len() as i64);
    }
}
