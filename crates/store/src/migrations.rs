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
//!
//! Version 9 is what the posted body is composed of: one row per block, so that cutting a
//! finding removes it from the review rather than the developer deleting a paragraph. It
//! backfills one prose block per draft already held, which is the honest reading of a body
//! nothing wrote in parts — and it normalises that body to end in exactly one newline,
//! because from here on the body is composed and the composition ends in one.
//!
//! Version 10 is what the three boards Patrick approved on 10 October 2026 draw and a brief
//! of three free-text fields could not hold: a headline, how the system works today, a
//! before flow and an after flow, numbered problems with their detail, one supporting table,
//! who proposed each option and how each option scores against each problem, the blind pass
//! a session reported, the change in parts, and the choices and disagreements the change
//! left behind. Thirteen tables, because every one of them is a list a page draws and the
//! store holds no opaque blobs.
//!
//! Version 11 is one column: a finding's short title. The code review page board A8 draws
//! is one line per finding, and the record had nothing a line long — a `claim` is what the
//! review says is wrong, in the review's voice, and on the review this was measured against
//! one ran to 56 words. The draft holds a title per block, but only once a draft exists, and
//! this page is read before that.
//!
//! Version 12 is when each of a review's seven steps started and finished. Each step held
//! one `updated_at`, overwritten on every write, so a step's start was lost the moment it
//! finished and Review Desk could not say how long a review took. It adds an append-only
//! row per state a step entered — a step that ran twice keeps both runs — and the one
//! column that answers "is the session still alive": when anything last wrote to the
//! review. It was written as 11 and renumbered when DEV-878's own 11 merged first, which
//! is the rule above being used rather than quoted.
//!
//! Version 16 is the one thing a review could not say about its ticket: where the ticket
//! is. The record has held `ticket_id` since version 1, and nothing can turn `DUC-2045`
//! into an address — the tracker and its workspace are the session's knowledge, not the
//! record's — so a header could print the id and not link it.

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
    Migration {
        version: 9,
        name: "draft-composition",
        sql: r#"
-- The blocks a draft body is composed of, in order.
--
-- **The body is no longer one block of markdown the developer edits.** It is this list,
-- composed: a `prose` block is carried through exactly as written, and a `finding` block
-- is a heading the store composes from the finding's live severity, its number and the
-- title here, followed by the paragraph. Cutting a finding stops its block posting;
-- regrading it changes the heading because the heading was never stored.
--
-- Keyed on the position rather than on the finding, for the reason `review_sources` gives:
-- two prose blocks have nothing to tell them apart but their order. The whole list is
-- replaced on every write, so the position is the store's to assign and no caller numbers
-- blocks.
--
-- `agent_title` and `agent_text` are what the agent wrote. They are not a history — the
-- newest rewrite replaces the one before it — they are the one comparison the feedback
-- loop asks for: what the developer changed, and from what.
--
-- The foreign key to `drafts` is what makes a draft and its composition one record: no
-- draft, no blocks, and deleting the review takes both.
CREATE TABLE draft_sections (
    review_id   INTEGER NOT NULL REFERENCES drafts (review_id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    finding_seq INTEGER,
    title       TEXT,
    text        TEXT    NOT NULL,
    agent_title TEXT,
    agent_text  TEXT    NOT NULL,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (kind IN ('prose', 'finding')),
    -- A prose block is text and nothing else. An empty one would compose a blank line
    -- into the body that nothing could see to remove.
    CHECK (kind <> 'prose'
           OR (finding_seq IS NULL AND title IS NULL AND agent_title IS NULL
               AND text <> '')),
    -- A finding block names its finding and carries a title. Its paragraph may be empty:
    -- a heading on its own is a finding the draft raises without elaborating.
    CHECK (kind <> 'finding'
           OR (finding_seq IS NOT NULL AND title IS NOT NULL AND title <> ''
               AND agent_title IS NOT NULL AND agent_title <> '')),
    -- The block is about a finding of this review, which is what stops a composition
    -- pointing at another review's F1. The same composite shape `outcomes` uses.
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);

-- One block per finding. Two blocks for F1 would both answer "what does cutting F1
-- remove", and the heading is composed from the finding, so the second would repeat it.
CREATE UNIQUE INDEX draft_sections_one_per_finding
    ON draft_sections (review_id, finding_seq)
    WHERE finding_seq IS NOT NULL;

-- Every draft already on the record becomes one prose block holding its whole body.
--
-- **That is the honest reading of it, and it is a decision with a visible consequence:**
-- the per-finding decisions refuse on such a draft, saying it was written as one block,
-- because nothing can say which characters are F1's without inferring it from the text —
-- which is the guessing this table exists to stop. A session that rewrites the draft in
-- parts gets the buttons.
INSERT INTO draft_sections
    (review_id, position, kind, finding_seq, title, text, agent_title, agent_text)
SELECT review_id, 1, 'prose', NULL, NULL,
       TRIM(body, char(10)), NULL, TRIM(body, char(10))
FROM drafts
WHERE TRIM(body, char(10)) <> '';

-- And the body is normalised to what composing that one block produces, so that the
-- column and the blocks agree from the first read rather than from the first write.
UPDATE drafts
SET body = TRIM(body, char(10)) || char(10)
WHERE TRIM(body, char(10)) <> '';

UPDATE drafts SET body = '' WHERE TRIM(body, char(10)) = '';
"#,
    },
    Migration {
        version: 10,
        name: "brief-structure",
        sql: r#"
-- The twelve things the three boards Patrick approved on 10 October 2026 draw, and a
-- brief of three free-text fields could hold none of.
--
-- **Every field here is plain text.** The pages interpolate; nothing renders markup, and
-- `briefs.diagram` stays exactly as it was for the briefs that already use it.
--
-- **No length is enforced anywhere below.** The recorded brief ran 310 words where the
-- approved board shows 85, and that is a writing fault: it belongs to the linter in Locus,
-- not to a CHECK constraint. What is refused here is structural nonsense only -- a flow row
-- of four boxes, a verdict against a problem that does not exist, a title over an empty
-- table.
--
-- Two vocabulary words run through all of it. `position` is the row, from 1; `slot` is the
-- place across that row, from 1. Both are the store's to assign, from the order written, so
-- no caller manages numbering -- the bargain `review_sources` struck in migration 8. The one
-- exception is `brief_problems.number`, which is an **identity** rather than an order: a
-- flow box, an option verdict and a solution part all name it, so the caller owns it.

-- Row 1: the one headline sentence, and row 7: how the system works today.
--
-- `how_it_works_today` is the context a newcomer needs, and on 10 October it was mixed into
-- `problem` -- which is why the problem half could not be handed to anything. It is also the
-- larger half of what a blind pass may be shown; see `problem_statement`.
ALTER TABLE briefs ADD COLUMN headline           TEXT;
ALTER TABLE briefs ADD COLUMN how_it_works_today TEXT;

-- The two sub-captions above the flows -- "The staff page handles OAuth sign-in", "OAuth
-- sign-in has a page of its own". They are the one part of rows 2 and 3 that is not a box.
ALTER TABLE briefs ADD COLUMN flow_before_caption TEXT;
ALTER TABLE briefs ADD COLUMN flow_after_caption  TEXT;

-- Row 6's heading -- "The idea: three questions, each with one owner". Non-null only when
-- the table has a column, which `put_brief` enforces: a heading over nothing is a section
-- the page draws empty.
ALTER TABLE briefs ADD COLUMN support_table_title TEXT;

-- Rows 4 and 5: the numbered problems.
--
-- **The number is the brief's one internal identity.** A flow box marks a problem, an option
-- is judged against it and a solution part fixes it, so it is the caller's to choose and the
-- store's to enforce the existence of.
--
-- `was_wrong` is required and `now_fixed` is not, and that asymmetry is the whole reason the
-- problem half can be written on its own: a numbered problem with nothing wrong is not a
-- problem, but a problem whose fix has not been described yet is exactly what step 2 holds
-- before step 4 has run. DEV-883's order -- understand, then the blind pass, then the
-- solution -- is only writable because of it.
CREATE TABLE brief_problems (
    review_id    INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    number       INTEGER NOT NULL,
    was_wrong    TEXT    NOT NULL,
    now_fixed    TEXT,
    detail_title TEXT,
    PRIMARY KEY (review_id, number),
    CHECK (number >= 1),
    CHECK (was_wrong <> ''),
    CHECK (now_fixed IS NULL OR now_fixed <> ''),
    CHECK (detail_title IS NULL OR detail_title <> '')
);

-- Row 5's named detail, split before and after. One row per line of it.
--
-- Hung off the problem rather than the brief, so detail for a problem that is not there
-- cannot exist -- and a brief rewritten with one problem fewer takes its detail with it.
CREATE TABLE brief_problem_detail (
    review_id INTEGER NOT NULL,
    number    INTEGER NOT NULL,
    side      TEXT    NOT NULL,
    position  INTEGER NOT NULL,
    text      TEXT    NOT NULL,
    PRIMARY KEY (review_id, number, side, position),
    CHECK (side IN ('before', 'after')),
    CHECK (position >= 1),
    CHECK (text <> ''),
    FOREIGN KEY (review_id, number)
        REFERENCES brief_problems (review_id, number) ON DELETE CASCADE
);

-- Rows 2 and 3: the before flow and the after flow, as rows of one to three boxes.
--
-- One table with a `side`, not two tables: the two flows are the same shape drawn twice,
-- and the approved board's own rule is that they are read side by side. They are served as
-- two lists, which is what makes the problem half separable -- `problem_statement` selects
-- `before` and there is nothing in it that could carry `after`.
--
-- **Three boxes is the cap and the schema holds it**, because "rows of one to three boxes"
-- is a property of the picture: a fourth box on a row is not a wider row, it is a row that
-- does not fit the column the page draws it in.
--
-- `problem` is nullable, and the composite foreign key is satisfied when it is null, which
-- is right: the plain boxes on the approved board -- the ones that are neither red nor green
-- -- are the steps of the flow that nothing is wrong with.
CREATE TABLE brief_flow_boxes (
    review_id INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    side      TEXT    NOT NULL,
    position  INTEGER NOT NULL,
    slot      INTEGER NOT NULL,
    title     TEXT    NOT NULL,
    note      TEXT,
    problem   INTEGER,
    PRIMARY KEY (review_id, side, position, slot),
    CHECK (side IN ('before', 'after')),
    CHECK (position >= 1),
    CHECK (slot BETWEEN 1 AND 3),
    CHECK (title <> ''),
    CHECK (note IS NULL OR note <> ''),
    FOREIGN KEY (review_id, problem)
        REFERENCES brief_problems (review_id, number) ON DELETE CASCADE
);

-- Row 6: the one small supporting table. On the approved board it is three questions and
-- who owns each, with a before and an after column.
--
-- Two tables rather than one, because a cell has to sit in a column that was declared. The
-- foreign key is what says so; without it a brief could hold a four-cell row under three
-- headings and nothing would be able to say which heading the fourth belongs to.
CREATE TABLE brief_support_columns (
    review_id INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    heading   TEXT    NOT NULL,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (heading <> '')
);

-- `slot` names the column, by its position. A cell may be empty -- a blank in a small table
-- is a real answer, and the approved board has none only because that review had none.
CREATE TABLE brief_support_cells (
    review_id INTEGER NOT NULL,
    position  INTEGER NOT NULL,
    slot      INTEGER NOT NULL,
    text      TEXT    NOT NULL,
    PRIMARY KEY (review_id, position, slot),
    CHECK (position >= 1),
    CHECK (slot >= 1),
    FOREIGN KEY (review_id, slot)
        REFERENCES brief_support_columns (review_id, position) ON DELETE CASCADE
);

-- Row 9: who put each option forward.
--
-- **`ticket` is the honest backfill, not a convenience.** Review Desk runs no blind pass and
-- nothing had run one when these rows were written -- DEV-883 is the issue that adds it --
-- so there is no option already on the record that could have come from anywhere else.
--
-- SQLite cannot add a CHECK to a table that already exists, so the closed set is enforced on
-- the way in and on the way out by `OptionProposer`, the way `NitTicketStatus` is for the
-- column migration 4 added to `reviews`.
ALTER TABLE brief_options ADD COLUMN proposed_by TEXT NOT NULL DEFAULT 'ticket';

-- Row 8: per option, a verdict against each numbered problem.
--
-- Four verdicts, and `not_assessed` is one of them rather than the absence of a row. The
-- approved board's option D is the case: the blind pass never came up with it, so it has no
-- score against the three problems, and "nobody judged this" is a different statement from
-- "the brief forgot to". The linter in DEV-884 can only ask for a verdict per option per
-- problem because the difference is on the record.
--
-- `why` is the one line beside the verdict, required for the three verdicts that are a
-- judgement and optional for the one that is the absence of one.
CREATE TABLE brief_option_verdicts (
    review_id INTEGER NOT NULL,
    key       TEXT    NOT NULL,
    problem   INTEGER NOT NULL,
    verdict   TEXT    NOT NULL,
    why       TEXT,
    PRIMARY KEY (review_id, key, problem),
    CHECK (verdict IN ('fixed', 'partly', 'stays', 'not_assessed')),
    CHECK (why IS NULL OR why <> ''),
    CHECK (verdict = 'not_assessed' OR why IS NOT NULL),
    FOREIGN KEY (review_id, key)
        REFERENCES brief_options (review_id, key) ON DELETE CASCADE,
    FOREIGN KEY (review_id, problem)
        REFERENCES brief_problems (review_id, number) ON DELETE CASCADE
);

-- Row 10: the blind pass itself.
--
-- **Review Desk runs no blind pass.** This records what a session reported running, and it
-- depends on nothing: `given` is the problem statement the session says it handed over, and
-- `looked_up` is the session's own answer to "what did you read apart from this" -- the word
-- "nothing", or a list. Review Desk cannot check either, and storing them is not a claim
-- that it did.
--
-- **The row's existence is the first of three states.** No row means the pass did not run.
-- A row with no pick means it ran and recommended nothing. A row with a pick means it ran
-- and reported. The second and third are told apart by `pick_key`, and `pick_why` travels
-- with it so a pick can never appear without its reason. See `Brief::blind_pass_state`.
--
-- The pick names one of the brief's own options, because an option list that the pass's
-- choice is not in would leave the page drawing a title nothing else on it refers to.
CREATE TABLE brief_blind_pass (
    review_id INTEGER PRIMARY KEY REFERENCES briefs (review_id) ON DELETE CASCADE,
    given     TEXT    NOT NULL,
    pick_key  TEXT,
    pick_why  TEXT,
    looked_up TEXT    NOT NULL,
    CHECK (given <> ''),
    CHECK (looked_up <> ''),
    CHECK (pick_key IS NULL OR pick_key <> ''),
    CHECK (pick_why IS NULL OR pick_why <> ''),
    CHECK ((pick_key IS NULL) = (pick_why IS NULL)),
    FOREIGN KEY (review_id, pick_key)
        REFERENCES brief_options (review_id, key) ON DELETE CASCADE
);

-- The questions the pass said it would need answered. Hung off the pass, so a brief whose
-- pass did not run cannot hold its questions.
CREATE TABLE brief_blind_questions (
    review_id INTEGER NOT NULL REFERENCES brief_blind_pass (review_id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    text      TEXT    NOT NULL,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (text <> '')
);

-- Row 11: the change in parts.
--
-- `coverage.files` from migration 8 is a flat list of every file the change touched, which
-- answers "was this read" and cannot answer "what was built". A part is the grouping: a
-- title, one line, its files, and which problems it fixes.
CREATE TABLE brief_parts (
    review_id INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    title     TEXT    NOT NULL,
    summary   TEXT    NOT NULL,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (title <> '' AND summary <> '')
);

-- The files of one part, with what the change did to each.
--
-- **`in_diff` is how a part claims a file the pull request did not change.** Two findings on
-- the review of 10 October sat in files outside the diff, and a part that could not name
-- them would have left those two findings belonging to no part at all. A file outside the
-- diff has no lines added or removed -- that is what being outside it means -- and the CHECK
-- is what stops a row claiming both.
--
-- One part per file, which is the point of the primary key: "which findings sit on which
-- part" is derived from a finding's path, and a path owned twice would answer it twice.
CREATE TABLE brief_part_files (
    review_id     INTEGER NOT NULL,
    part          INTEGER NOT NULL,
    position      INTEGER NOT NULL,
    path          TEXT    NOT NULL,
    lines_added   INTEGER NOT NULL DEFAULT 0,
    lines_removed INTEGER NOT NULL DEFAULT 0,
    in_diff       INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (review_id, path),
    -- The order is the order within its own part, so the uniqueness is too. One list per
    -- review -- the shape `review_files` uses -- would make the second part's first file
    -- collide with the first part's.
    UNIQUE (review_id, part, position),
    CHECK (position >= 1),
    CHECK (path <> ''),
    CHECK (lines_added >= 0 AND lines_removed >= 0),
    CHECK (in_diff IN (0, 1)),
    CHECK (in_diff = 1 OR (lines_added = 0 AND lines_removed = 0)),
    FOREIGN KEY (review_id, part)
        REFERENCES brief_parts (review_id, position) ON DELETE CASCADE
);

-- Which numbered problems a part fixes. A set, not a list: a part fixing problems 1 and 3
-- says nothing about which it fixes more.
CREATE TABLE brief_part_problems (
    review_id INTEGER NOT NULL,
    part      INTEGER NOT NULL,
    problem   INTEGER NOT NULL,
    PRIMARY KEY (review_id, part, problem),
    FOREIGN KEY (review_id, part)
        REFERENCES brief_parts (review_id, position) ON DELETE CASCADE,
    FOREIGN KEY (review_id, problem)
        REFERENCES brief_problems (review_id, number) ON DELETE CASCADE
);

-- Row 12, first half: the choices the ticket left open, and what the change chose.
--
-- `departs_from_ticket` is the one on the approved board that was not left open at all --
-- the ticket said where the pending record gets written and the change wrote it somewhere
-- else, with a reason. A departure and an open choice are the same row with that flag
-- flipped, because what the reader wants is the same column either way: what was chosen and
-- why.
--
-- The finding is a pointer and is optional here. An open choice is a choice, not a fault.
CREATE TABLE brief_open_choices (
    review_id           INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    position            INTEGER NOT NULL,
    left_open           TEXT    NOT NULL,
    chosen              TEXT    NOT NULL,
    why                 TEXT    NOT NULL,
    departs_from_ticket INTEGER NOT NULL DEFAULT 0,
    finding_seq         INTEGER,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (left_open <> '' AND chosen <> '' AND why <> ''),
    CHECK (departs_from_ticket IN (0, 1)),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);

-- Row 12, second half: where the description and the code disagree.
--
-- **The finding is required**, which is DEV-875's rule applied where it applies again: a
-- brief that says the description claims one thing and the code does another, and raises
-- nothing about it, posts a review that never mentions the disagreement. The author then
-- reads a description the record knows is wrong. Raise the finding first.
--
-- The composite foreign key is the same shape `outcomes` uses, so a disagreement cannot
-- point at another review's F3.
CREATE TABLE brief_disagreements (
    review_id        INTEGER NOT NULL REFERENCES briefs (review_id) ON DELETE CASCADE,
    position         INTEGER NOT NULL,
    description_says TEXT    NOT NULL,
    code_does        TEXT    NOT NULL,
    finding_seq      INTEGER NOT NULL,
    PRIMARY KEY (review_id, position),
    CHECK (position >= 1),
    CHECK (description_says <> '' AND code_does <> ''),
    FOREIGN KEY (review_id, finding_seq)
        REFERENCES findings (review_id, seq) ON DELETE CASCADE
);
"#,
    },
    Migration {
        version: 11,
        name: "finding-title",
        sql: r#"
-- A finding's short title: the one line board A8 puts on every row.
--
-- **The record had none, and the row needs one.** A finding's `claim` is what the review
-- says is wrong, in the review's voice, and on the review this was measured against it ran
-- to 56 words -- which is a paragraph, not a row. The draft's own `draft_sections.title`
-- holds a title, but only once a draft has been written, and the code review page is read
-- before that: a page that could only name a finding after the draft existed would be
-- unreadable in exactly the state it is opened in.
--
-- **Nullable, and that is the honest default.** Nothing writes one today -- having the
-- review session write titles is DEV-878's own out-of-scope note -- so every finding
-- already held has none, and the page clamps the claim to two lines instead. Defaulting to
-- the claim here would put a 56-word "title" on the record and make the two indistinguishable
-- afterwards.
--
-- **No length is enforced.** A title that runs long is a writing fault and belongs to the
-- linter in Locus, which is migration 10's reasoning for the brief and is no different here:
-- a CHECK on the length would refuse a finding rather than report a bad title. What is
-- refused is a title that is not one -- the empty string, which is a field nobody filled in
-- wearing the clothes of one somebody did.
-- **A blank title is refused, and not by a CHECK here.** SQLite cannot add one to an
-- existing table, and the refusal belongs beside the hunk's and the fix reference's anyway:
-- `add_finding` refuses `"title": ""` by name, the way `finding_evidence` refuses a blank
-- fix reference. Stated here so that the absence of a constraint reads as a decision rather
-- than an omission. A column added by ALTER is NULL on every row already held, which is
-- what "no finding has a title yet" means.
ALTER TABLE findings ADD COLUMN title TEXT;
"#,
    },
    Migration {
        version: 12,
        name: "step-timings",
        sql: r#"
-- One row per state a step entered, with when it entered it. **Append-only: nothing
-- updates or deletes a row here.**
--
-- `review_layers` keeps one `updated_at` per step and overwrites it, which is the defect
-- DEV-890 fixes: two more columns on that table would be overwritten by a second run just
-- as surely. A step that ran twice — a draft sent back and redrafted — has two rows that
-- say `running` here, and both runs are reported.
--
-- **`entered_at` is when the step entered this state**, and that one reading is what makes
-- the backfill below honest as well as the rows written from here on. A row whose state is
-- a finished one therefore carries that step's finish; the run it finished has a start only
-- if an earlier row opened one.
--
-- No row is written for a state that has not changed. `set_review_layer` is deliberately
-- idempotent — "writing the same state twice is the same as writing it once, except that
-- the time moves" — so a session reporting progress ten times would otherwise be ten runs
-- of a few seconds each instead of one run that is still going.
--
-- The detail is kept with the state it was written against, so a step that ran twice can
-- say what each run reported: "waiting on you", then the developer's own note back.
CREATE TABLE review_layer_states (
    id         INTEGER PRIMARY KEY,
    review_id  INTEGER NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    number     INTEGER NOT NULL,
    state      TEXT    NOT NULL,
    detail     TEXT,
    entered_at INTEGER NOT NULL,
    CHECK (number BETWEEN 1 AND 7),
    CHECK (state IN ('pending', 'running', 'done', 'waiting_on_you', 'failed', 'skipped'))
);

-- Read by review and step, in the order the rows were written. `id` is the order: the
-- table is append-only and two states entered in the same second are still ordered by it,
-- where `entered_at` alone would leave them a coin toss.
CREATE INDEX review_layer_states_by_step ON review_layer_states (review_id, number, id);

-- **What a review already held, read as what it is and nothing more.** A review recorded
-- before this migration has exactly one time per step, `updated_at`, and the honest reading
-- of it is that the step entered the state it is in then. So a step already finished gets
-- its finish from it and no start at all, and every figure that would need a start reads as
-- not recorded rather than as zero. Nothing here invents a start.
INSERT INTO review_layer_states (review_id, number, state, detail, entered_at)
SELECT review_id, number, state, detail, updated_at
FROM review_layers;

-- When anything last wrote to this review. **Null for every review recorded before this
-- migration**, which is why it is nullable and not defaulted: "nothing has written since
-- the record started keeping this" and "written a long time ago" are different answers, and
-- the queue says the first one rather than drawing a figure from the review's creation.
ALTER TABLE reviews ADD COLUMN last_activity_at INTEGER;
"#,
    },
    Migration {
        version: 13,
        name: "problem-label",
        sql: r#"
-- A numbered problem's short name: the two or three words a column heading can be.
--
-- **The heading was the whole sentence, and it wrapped ten lines deep.** The Alternatives
-- page heads one matrix column per numbered problem, and the only words it had for one were
-- `was_wrong` -- a line of prose, which is right in the "Was wrong" column of the Understand
-- page and is not a heading. The board this is measured against heads those columns with the
-- red mark and a label of two or three words, and nothing on the record held one.
--
-- **Nullable, and that is the honest default.** Every problem already on the record has
-- none, and the page then heads the column with the number alone and puts the sentence one
-- click away. Defaulting to `was_wrong` would write a ten-line "label" onto the record and
-- leave nothing able to tell a label from a sentence afterwards.
--
-- **No length is enforced**, which is migration 10's reasoning for the whole brief and is no
-- different here: a label that runs long is a writing fault and belongs to the linter in
-- Locus (DEV-893), not to a CHECK that refuses the problem. What is refused is a label that
-- is not one -- the empty string, a field nobody filled in wearing the clothes of one
-- somebody did -- and it is refused by `put_brief` beside the refusals of a blank
-- `detail_title` and a blank `now_fixed`, because SQLite cannot add a CHECK to a table that
-- already exists. Stated here so the absence of a constraint reads as a decision rather than
-- an omission.
ALTER TABLE brief_problems ADD COLUMN label TEXT;
"#,
    },
    Migration {
        version: 14,
        name: "layer-facts-and-source-reading",
        sql: r#"
-- The facts a step recorded, as the card beside it draws them: a label and a value.
--
-- **The note was a paragraph pretending to be a phrase.** A step draws its `detail` on one
-- line now, and the sentences that used to be crushed into a 150-pixel column have somewhere
-- to go: an ordered list of short pairs, which is what the note was trying to be. "Problems"
-- and "3" is a fact; "the brief was rewritten twice, because the first pass mixed how the
-- system works today into the problem" is a paragraph, and no column was ever going to hold
-- it.
--
-- **Ordered by the position the store assigns**, for the reason every other list on this
-- record is: two facts of one step have nothing else to tell them apart, and a caller that
-- numbered its own would be a caller that could number two the same.
--
-- **No length is enforced**, which is migration 10's reasoning and migration 13's: a label
-- or a value that runs long is a writing fault and belongs to the linter in Locus, not to a
-- CHECK that refuses the step. What is refused, in `put_review_layer_facts`, is a pair with
-- an empty label or an empty value -- a field nobody filled in wearing the clothes of one
-- somebody did.
--
-- The parent is the layer row, not the review: facts about a step nothing has written are
-- facts about nothing. SQLite needs the composite key `review_layers` already declares
-- UNIQUE for that, which it has.
CREATE TABLE review_layer_facts (
    review_id INTEGER NOT NULL,
    number    INTEGER NOT NULL,
    position  INTEGER NOT NULL,
    label     TEXT    NOT NULL,
    value     TEXT    NOT NULL,
    PRIMARY KEY (review_id, number, position),
    FOREIGN KEY (review_id, number) REFERENCES review_layers (review_id, number)
        ON DELETE CASCADE
) WITHOUT ROWID;

-- What a source's ticket said, and what it tells us. Two fields, both optional.
--
-- "Already decided" was a heading, a small paragraph and seven source lines that all looked
-- alike. It is three levels now -- what was decided, what is still open, then where it came
-- from -- and the third is a table, which needs two columns the record did not hold: the
-- state the session read off the ticket, and the one line saying what the source tells us.
--
-- **Both are what the session read, and neither is checked.** Review Desk fetches no ticket
-- and follows no link; `status` is a state as the session reported it, which is why it is
-- free text and not an enum -- every tracker names its own columns, and a fixed set here
-- would either refuse a real board or quietly rename it. A row with no `note` shows its
-- reference alone.
ALTER TABLE review_sources ADD COLUMN status TEXT;
ALTER TABLE review_sources ADD COLUMN note TEXT;
"#,
    },
    Migration {
        version: 15,
        name: "solution-part-one-line",
        sql: r#"
-- A part's one line: the sentence its row on the Solution page carries under the title.
--
-- **`summary` is not one line, and the page was drawing it as though it were.** On the
-- review this was measured against the eight parts' summaries ran 33 to 52 words each, and
-- board A10 reads them down a single column -- so the column either wrapped four deep or cut
-- the sentence mid-word. The summary is right where it is, inside the opened row; what the
-- record had nothing for was the line above it.
--
-- **Nullable, and that is the honest default**, which is migration 13's reasoning for a
-- problem's label and holds here for the same reason: every part already on the record has
-- none, and the page then cuts the summary to a line with an ellipsis and says so by drawing
-- it that way. Copying the first sentence of the summary onto the record at write time would
-- put a derived value in a column nothing could afterwards tell from a written one.
--
-- **No length is enforced here**, which is migration 10's rule for the whole brief and
-- migrations 13 and 14's for their own columns. A one-line summary that runs to three lines
-- is a writing fault and belongs to the linter in Locus -- DEV-920, filed alongside this --
-- not to a CHECK that refuses the part. What is refused is a value that is not one: the
-- empty string, a field nobody filled in wearing the clothes of one somebody did, and it is
-- refused by `put_brief` beside the refusals of a blank `label` and a blank `now_fixed`,
-- because SQLite cannot add a CHECK to a table that already exists. Stated here so the
-- absence of a constraint reads as a decision rather than an omission.
ALTER TABLE brief_parts ADD COLUMN one_line TEXT;
"#,
    },
    Migration {
        version: 16,
        name: "ticket-url",
        sql: r#"
-- Where the review's ticket is: the address the session was given, beside the id it
-- already records.
--
-- **Review Desk cannot work this out from the id.** `ticket_id` is a `ForeignId` -- opaque,
-- never parsed -- and turning `DUC-2045` into an address needs the tracker and the
-- workspace, which the session knows from its dispatch and the record does not. So the
-- address is given or there is none; nothing here derives it and nothing fetches it.
--
-- **Nullable, and that is the honest default**, for migration 13's reason and 15's: every
-- review already on the record has no address, and a header with the id alone draws it as
-- text. Backfilling a guess -- a Linear workspace inferred from the id's prefix -- would put
-- a derived address in a column nothing could afterwards tell from a given one, and it
-- would be a link to somewhere nobody said.
--
-- **`https` only, and refused on the way in rather than on the way out.** The one thing a
-- stored address is for is being rendered as a link, and a `javascript:` or `data:` value
-- reaching an `href` is the whole of that risk; refusing it at the write is the only place
-- it can be refused once. SQLite cannot add a CHECK to a table that already exists, so the
-- refusal lives in `validate` and `set_review_facts` beside the refusals of a blank
-- `label` and a blank `one_line`, which is migration 15's reasoning and its precedent.
-- Stated here so the absence of a constraint reads as a decision rather than an omission.
-- The dashboard refuses a non-`https` value a second time at render, because a database can
-- be written to by hand and a column is not a promise about every row in it.
ALTER TABLE reviews ADD COLUMN ticket_url TEXT;

-- **And the ids the old writer stored unnormalised are normalised, which is what they mean.**
-- `set_review_facts` wrote whatever it was handed, so two kinds of row exist on a released
-- database: `review open --ticket ""` from a shell whose variable was unset stored the empty
-- string, and `--ticket "$(…)"` stored `DEV-860` with the command substitution's newline on
-- it.
--
-- **Both halves matter, and the padded one is the dangerous half.** An empty id makes the API
-- serve `"ticket_id": ""`, which the contract's `ForeignId` says cannot happen
-- (`minLength: 1`), and makes `--ticket-url` alone refused for having no id — blocking the
-- two-call pattern the README documents. A *padded* id is worse: it is non-blank, so an
-- address records against it happily, and then `set_review_facts` compares the id a caller
-- passes (normalised) against the one held (`DEV-860 `), finds them unequal, and drops the
-- address as stale. Re-stating the **same** ticket destroyed a correct address. Measured
-- against the binary before this line existed.
--
-- `NULLIF(TRIM(…), '')` does both: trimmed to what the id means, and nothing left is no id.
-- So "an id is trimmed, and blank is absent" is true of the record this code already wrote
-- and not only of writes made from here on, which is what `ForeignId` claims of it.
--
-- **The characters are spelt out because SQLite's `TRIM` takes spaces and nothing else.**
-- One argument trims `' '` only — not a tab, not a newline — while Rust's `str::trim`, which
-- every write goes through, takes all whitespace. So a bare `TRIM(ticket_id)` left
-- `DEV-860\n` exactly as it found it, which is the one row that matters here: a trailing
-- newline is what `--ticket "$(git …)"` produces. Measured by the test for this migration,
-- which failed on it. ASCII whitespace is what a shell can put there; Rust's notion is
-- wider and SQLite cannot express it, and nothing needs it to.
--
-- **A database already at 16 does not run this.** Migrations are forward-only and keyed on
-- version, so a machine that ran an earlier commit of *this branch* is past it. Nothing
-- released is affected — master is at 15 — and the rows this repairs were written by
-- released code, so every database the merge reaches gets it. A developer's own database
-- that followed this branch is the one exception, and re-stating the ticket there fixes the
-- row by hand.
UPDATE reviews
   SET ticket_id = NULLIF(
         TRIM(ticket_id, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
       );
"#,
    },
    Migration {
        version: 17,
        name: "review-dismissal",
        sql: r#"
-- When a review was dismissed, and why. Both null for a review in play.
--
-- **Hidden and kept, rather than deleted.** Decided by Patrick on 12 October 2026 against
-- the alternative of deleting outright: a review carries the developer's sign-offs, the
-- dispositions they made one finding at a time, and the evidence the rules in force were
-- built from, so a row that goes takes all of that with it. A dismissal takes the review
-- out of the queue, out of the work list and out of the Reviews list's default view, and
-- changes nothing underneath it. `a_dismissal_keeps_every_row_under_the_review`
-- asserts the second half, because "it keeps the record" is the whole argument for
-- choosing this over a delete and it is the kind of claim a later convenience quietly
-- breaks.
--
-- **The reason is not nullable in practice and is nullable here**, which is migration 13's
-- shape and for a different argument. Every dismissal this code makes carries one —
-- `dismiss_review` refuses a blank, the way the rules screen refuses a rule dismissed
-- without a reason, because a review hidden silently is a review nobody can later say why
-- they hid. What cannot be done is a CHECK tying the two columns together: SQLite will not
-- add one to a table that already exists, and a CHECK on this column alone would have to
-- allow the null that every existing row holds. So the pairing lives in one write and is
-- stated here rather than enforced by the schema.
--
-- **Two columns and no `dismissed_by`.** Only the developer dismisses a review — there is
-- no command line path to it and the dashboard's API records no actor for it — so a column
-- holding `developer` in every row would be a field that cannot disagree with itself, which
-- is the argument `SIGN_OFF_ACTOR` makes in the server for not taking an actor as an
-- argument.
--
-- **And no index.** The Reviews list reads every review it is going to draw, so its scan is
-- the page's own read either way, and the dismissed-only view is the rarer of the two. An
-- index here would be a second structure to keep in step for a table that holds one row per
-- review of one developer's pull requests.
ALTER TABLE reviews ADD COLUMN dismissed_at     INTEGER;
ALTER TABLE reviews ADD COLUMN dismissed_reason TEXT;

-- **And the four ids an empty string would now change the meaning of.**
--
-- `set_review_links` reads every optional string through `normalise::optional_fact`,
-- which is this record's rule and the reason migration 16 repaired `ticket_id`: a
-- `--session "$SESSION_ID"` from a shell whose variable is unset is the empty string, and
-- `COALESCE` is only told to leave a column alone by a `NULL`. Released code had no such
-- check on these four, so the rows are out there.
--
-- Normalising the writes is not enough, because this migration changes what an empty
-- string *means*. A `session_id` of `''` is `is_some()`, so `review_is_claimed` reads the
-- review as one somebody is working on and its head never moves again — one request
-- becomes two unstarted reviews, which is the state DEV-945 exists to remove. A
-- `github_review_id` of `''` makes `ReviewWork::started` true, so the review can never be
-- deleted. `not_checked` claims the review the same way. Each is a row a developer cannot
-- get out of, and fixing the writes does not reach them.
--
-- Migration 16's own argument, applied to this migration's own columns: the rule has to be
-- true of the record this code already wrote and not only of writes made from here on. The
-- characters are spelt out for the reason it gives — SQLite's one-argument `TRIM` takes
-- spaces and nothing else, and a trailing newline is what a shell substitution produces.
--
-- `ledger_id` is repaired beside them although nothing reads it as a flag, because it is
-- written by the same call that writes `session_id`, and a rule that held for three
-- columns of four would be the next reader's surprise.
UPDATE reviews
   SET session_id = NULLIF(
         TRIM(session_id, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
       ),
       ledger_id = NULLIF(
         TRIM(ledger_id, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
       ),
       github_review_id = NULLIF(
         TRIM(github_review_id,
              ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
       ),
       not_checked = NULLIF(
         TRIM(not_checked, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
       ),
       -- **And the title, which is the fifth column written through that same loose
       -- path.** It changes no rule of this migration's, so it was left out of the four
       -- above — and a review of this branch pointed out that the argument does not
       -- depend on the rule: `POST /reviews` fills a missing title from GitHub only when
       -- the column `is_none()`, so a legacy row holding `''` can never be given one, and
       -- `review find` prints a dangling "— " for it for ever. One sweep, five columns.
       title = NULLIF(
         TRIM(title, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)),
         ''
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
