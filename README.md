# Review Desk

**Review Desk keeps the record of an agent-assisted code review** — the brief, the
options considered, the findings, what became of each finding, and the rules learned
from them — and feeds what it learns back into the next review. It is not a list of
pull requests: GitHub and Linear already do that, and its unit is the finding and the
rule, not the PR. It runs locally, owns its own record, and needs nothing else
installed.

This repository is at the second slice: the store, the commands an agent session
calls to record a review, and the harvester that reads a posted review's outcomes back
from GitHub. There is no dashboard and no HTTP API yet.

## Build and test

```sh
cargo build              # build the workspace
cargo test               # run every test
cargo fmt --all --check  # the format check CI runs
cargo clippy --all-targets -- -D warnings
```

SQLite is compiled in through `rusqlite`'s `bundled` feature, so a clean clone needs
nothing on the machine but a Rust toolchain.

## The commands

**The command line is the single way into the record.** An agent on any platform can make
a shell call, which is what keeps Locus platform-agnostic. The local HTTP API and the
dashboard come later and sit on the same store crate.

```sh
review-desk --version    # the crate version
review-desk db path      # where the database is, without opening it
review-desk db version   # the schema version, creating the database if absent
```

### Recording a review

`review open` prints the review's id and nothing else, so a shell caller can keep it in a
variable. A head commit already reviewed is handed back; a new head commit on a pull
request reviewed before starts the next round.

```sh
REVIEW=$(review-desk review open --repo devergehq/review-desk --pr 851 --head c0ffee1)
```

The repository and the head commit are identity, so both are stored canonical — trimmed
and lowercased. `DevergeHQ/Review-Desk` and `devergehq/review-desk` are one repository,
as they are on GitHub, and `C0FFEE1` is one commit with `c0ffee1`, because `git rev-parse`
only ever emits lowercase.

**An abbreviated commit is still a different commit from its full form.** Nothing can
reconcile `c0ffee1` with `c0ffee1234…` without asking git, so pass the same spelling every
time — `git rev-parse HEAD` in one caller and `git rev-parse --short HEAD` in another would
open two reviews and spend two round numbers.

`review link` stores the other systems' ids. An id left out is left as it was, so one
call can set the session without clearing the posted GitHub review.

```sh
review-desk review link --review "$REVIEW" \
  --session allele-ae03f8ce --ledger ledger-851 --github-review gh-review-3312
```

`brief put` stores what the review understood before it read the diff, replacing any
brief already held. Replacing clears the confirmation: the developer confirmed the text
that was there, not whatever replaces it.

```sh
review-desk brief put --review "$REVIEW" --file brief.json
```

```json
{
  "problem": "nothing outside Rust can write a review",
  "diagram": "agent -> review-desk -> store -> sqlite",
  "option_map": "a command line, a local HTTP API, or a Rust library only",
  "approach_verdict": "the command line first: any agent on any platform can call it"
}
```

`brief confirm` marks that brief confirmed by the developer. Confirming twice keeps the
first confirmation time.

```sh
review-desk brief confirm --review "$REVIEW"
```

`finding add` adds one finding and prints its sequence number within the review — the id
a person quotes. `lens`, `severity` and `claim` are required; the disposition defaults to
`in_draft`.

```sh
SEQ=$(review-desk finding add --review "$REVIEW" --file - <<'JSON'
{
  "lens": "correctness",
  "severity": "blocker",
  "file": "crates/cli/src/main.rs",
  "line": 48,
  "claim": "a refused request exits 2, which pages whoever treats 2 as broken",
  "evidence": "clap's default exit code is 2 and nothing overrides it"
}
JSON
)
```

`finding set` changes what is to become of a finding in this round's draft. The reason
belongs to the disposition, so omitting `--reason` clears the one that was there: the
reason a finding was held back is not a reason it is back in the draft.

```sh
review-desk finding set --review "$REVIEW" --finding "$SEQ" \
  --disposition held_back --reason "a nit in a file this change does not touch"
```

`outcome add` records what became of a finding, and who brought it about.

```sh
review-desk outcome add --review "$REVIEW" --finding "$SEQ" \
  --kind kept --by developer --detail "fixed before the pull request opened"
```

`review show` prints the whole review: the brief, the findings and the outcomes.

```sh
review-desk review show --review "$REVIEW" --json
```

### Reading the rules

`rules list` prints the rules that apply to a repository — the global ones and its own —
and only those in force, which is to say at stage `active`, `lint` or `ci`. A proposed
rule is not yet a rule, and a retired one no longer is.

```sh
review-desk rules list --repo devergehq/review-desk
```

Nothing writes a rule yet. Promoting one is the developer's act, and harvesting one is
the harvester's; both are later slices, so `rules list` reads a record this command line
cannot fill.

### Reading the outcomes back

Once a review has been posted, `finding link` records which GitHub review comment each
finding became, and `harvest` reads what happened to them back off GitHub.

```sh
review-desk finding link --review "$REVIEW" --finding "$SEQ" --github-comment 4140231301

review-desk harvest --review "$REVIEW"
# harvested 1 review: 3 replies, 3 threads resolved, 3 code changed after; 1 finding
# has no linked comment, skipped for replies and resolution
```

`harvest` takes no argument and reads every review whose pull request was open at its
last harvest. `--repo OWNER/REPO` narrows it to one repository; `--review ID` names one
review and reads it whatever state its pull request is in. What it records, and why
running it twice records nothing twice, is [below](#harvesting).

### Input and output

| | |
| -- | -- |
| Structured input | JSON, from `--file PATH`, or from standard input when the path is `-` |
| `--json` | One JSON document on standard output and nothing else, on a refusal as much as on success |
| Without `--json` | A short line for a person on standard output; failures on standard error |

A field the record does not have is refused rather than dropped. A misspelled
`option_map` would otherwise become a brief with no option map, and the record would
quietly lose what the review wrote.

`--help` and `--version` are the one exception to the `--json` rule: they stay text,
because they describe the command line rather than answering a question about a record.

## Exit codes

| Code | Meaning |
| -- | -- |
| 0 | Done |
| 1 | The request was understood and refused: an unknown review, an invalid value, a duplicate, input that could not be used |
| 2 | Review Desk could not run: the database is unreadable, a migration failed |

**A caller can tell "refused" from "broken" without parsing text.** Locus's review worker
treats 2 as a loud warning that never blocks the review, so a mistyped flag must not land
there — clap's own default for a usage error is 2, and the binary remaps it to 1.

With `--json`, the document carries the reason as well:

```json
{ "error": { "code": "not_found", "message": "no review with id 99", "kind": "refused" } }
```

| `code` | Raised when |
| -- | -- |
| `usage` | The arguments could not be parsed |
| `bad_input` | A `--file` could not be read, or its JSON could not be used |
| `invalid_value` | A severity, disposition, outcome kind, actor, repository or number is not one the record accepts |
| `not_found` | The review, finding or outcome asked for is not there |
| `unknown_parent` | The review or finding a record would hang off is not there |
| `duplicate_review` | A review already exists for that repository, pull request and round |
| `cannot_open_database` | The database file could not be opened |
| `database_error` | SQLite refused for any other reason |
| `migration_failed` | A migration could not be applied |
| `database_from_the_future` | The database is newer than this build understands |
| `no_home` | No home directory, so the default path is unknowable |
| `cannot_serialise` | A record on the database could not be turned into JSON |
| `duplicate_github_comment` | That GitHub review comment is already linked to another finding |
| `gh_missing` | `harvest` needs the `gh` command and it is not installed |
| `gh_not_authenticated` | `gh` is installed but nobody is logged in |
| `gh_failed` | `gh` ran and failed for some other reason |
| `github_response_unreadable` | A response from GitHub was not the shape it had to be |

`kind` is `refused` for exit 1 and `broken` for exit 2, so a caller reading only standard
output learns the same thing the exit code says.
## Harvesting

**`review-desk harvest` reads four facts back from GitHub and records them without
judging them.**

| Fact | Where it comes from | Recorded as |
| -- | -- | -- |
| The pull request was merged or closed | The pull request | `pr_state` and `pr_state_at` on the review |
| Someone replied to a finding | The finding's comment thread | An outcome of kind `replied`, with the author and the verbatim text |
| A finding's thread was resolved | The thread's resolved flag | An outcome of kind `thread_resolved` |
| The file a finding points at changed later | A comparison of the reviewed commit with the current head | An outcome of kind `code_changed_after`, with the commit and whether the finding's own lines changed |

It does **not** decide whether a reply accepted or rejected a finding. That needs
judgement; this command is deterministic. The reply is stored as written, and a person
or an agent reads it later.

A finding's own lines are "changed" when they were removed or replaced. Inserting lines
above a finding moves it without changing it, so that is recorded as the file changing
and not the finding's lines. A finding with no line number, or pointing at a file no
later commit touched, is treated the same way: the record says what changed, not what it
meant.

### What it needs, and what it skips

* GitHub is reached by running `gh`, as the Locus linters do, so the developer's
  existing login is used and Review Desk handles no tokens. **Every call is a read; the
  harvester never posts to GitHub.**
* A finding with no linked GitHub comment is skipped for replies and resolution — there
  is no thread to read — and is still checked for a later code change. The run says how
  many were skipped.
* Without `--review`, only reviews whose pull request was open at their last harvest are
  fetched. A merged or closed pull request has nothing further to say. `--review` names
  one review and reads it whatever state it is in.
* It is one shot. Scheduling it is somebody else's job.

### Running it twice records nothing twice

Every row the harvester writes carries `harvest_key`: the id of the GitHub fact it came
from — a reply comment's id, a thread's root comment id, a commit. A unique index on
`(review_id, finding_seq, kind, harvest_key)` refuses the second write, so a repeat run
finds every fact already there and reports it as such. The harvester does not remember
what it has done; the database does.

The index is partial — `WHERE harvest_key IS NOT NULL` — so an outcome recorded by hand
is never caught by it. Two developers may record "kept" against one finding twice; two
harvests of one reply may not.

A missing `gh` and a logged-out `gh` are both exit 2, with a message and a `code` saying
which it was — they are Review Desk unable to run, not a request refused.


## Where the database lives

`~/.review-desk/review-desk.db` by default, overridden by the `REVIEW_DESK_DB`
environment variable:

```sh
REVIEW_DESK_DB=/tmp/scratch.db review-desk db path
```

Tests always use a temporary file and never touch the default path.

## Layout

| Path | What it is |
| -- | -- |
| `crates/store` | `review-desk-store`: the record store. The only code that opens the database file. |
| `crates/harvest` | `review-desk-harvest`: reads a posted review's outcomes back from GitHub. GitHub sits behind a trait, so its tests replay responses captured from a real pull request. |
| `crates/cli` | `review-desk`: the binary. One module per command group under `src/commands`. |

**The store crate is the only thing that opens the database.** The binary, and every
later caller — the HTTP API, the dashboard, the harvester — goes through its typed
functions. That is what keeps one way into the record.

## The record

Five entities, in three schema versions: the review record (version 1), the learning
layer (version 2), and what harvesting needs (version 3).

| Entity | Holds |
| -- | -- |
| Review | The repository as `owner/repo`, the pull request, the head commit, the round, the ids of the Allele session, the dispatcher ledger entry and the posted GitHub review, and what the pull request was last seen to be with when it was last harvested |
| Brief | One per review: the problem, its diagram, the option map, the approach verdict, and whether and when the developer confirmed it |
| Finding | Lens, severity, location, claim, evidence, its disposition with a reason, and the GitHub comment it was posted as |
| Outcome | What became of a finding, who did it, when, the detail, and the GitHub fact it was harvested from |
| Rule | A statement, the failure it prevents, its scope, its stage, and the findings and review it came from |

A review is unique on repository, pull request number and round.

### Migrations

Migrations are forward-only, declared in order in `crates/store/src/migrations.rs`, and
applied when the database is opened. The applied version is `PRAGMA user_version` and is
readable through `Store::schema_version`. A migration that has shipped is never edited;
the next change is a new entry.

## Errors

`StoreError::is_refusal` separates a request that was understood and refused — an
unknown review, an invalid value, a duplicate — from Review Desk being unable to run.
The binary exits 1 for the first and 2 for the second, so a caller can tell them apart
without reading message text.
