# Review Desk

**Review Desk keeps the record of an agent-assisted code review** — the brief, the
options considered, the findings, what became of each finding, and the rules learned
from them — and feeds what it learns back into the next review. It is not a list of
pull requests: GitHub and Linear already do that, and its unit is the finding and the
rule, not the PR. It runs locally, owns its own record, and needs nothing else
installed.

This repository is at the second slice: the store and the commands an agent session
calls to record a review. There is no dashboard and no HTTP API yet.

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

`kind` is `refused` for exit 1 and `broken` for exit 2, so a caller reading only standard
output learns the same thing the exit code says.

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
| `crates/cli` | `review-desk`: the binary. One module per command group under `src/commands`. |

**The store crate is the only thing that opens the database.** The binary, and every
later caller — the HTTP API, the dashboard, the harvester — goes through its typed
functions. That is what keeps one way into the record.

## The record

Five entities, in two schema versions: the review record (version 1) and the learning
layer (version 2).

| Entity | Holds |
| -- | -- |
| Review | The repository as `owner/repo`, the pull request, the head commit, the round, and the ids of the Allele session, the dispatcher ledger entry and the posted GitHub review |
| Brief | One per review: the problem, its diagram, the option map, the approach verdict, and whether and when the developer confirmed it |
| Finding | Lens, severity, location, claim, evidence, and its disposition with a reason |
| Outcome | What became of a finding, who did it, when, and the detail |
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
