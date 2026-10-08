# Review Desk

**Review Desk keeps the record of an agent-assisted code review** — the brief, the
options considered, the findings, what became of each finding, and the rules learned
from them — and feeds what it learns back into the next review. It is not a list of
pull requests: GitHub and Linear already do that, and its unit is the finding and the
rule, not the PR. It runs locally, owns its own record, and needs nothing else
installed.

This repository is at the first slice: the store, the beginning of the command line, and
the dashboard's four screens running on sample data. There is no HTTP API yet, and the
dashboard is not embedded in the binary yet.

## Build and test

```sh
cargo build              # build the workspace
cargo test               # run every test
cargo fmt --all --check  # the format check CI runs
cargo clippy --all-targets -- -D warnings
```

SQLite is compiled in through `rusqlite`'s `bundled` feature, so a clean clone needs
nothing on the machine but a Rust toolchain.

## The binary

```sh
review-desk --version    # the crate version
review-desk db path      # where the database is, without opening it
review-desk db version   # the schema version, creating the database if absent
```

The commands that write and read review records are the next issue.

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
| `crates/cli` | `review-desk`: the binary. |
| `web` | The dashboard: a Nuxt app, four screens, built to static files. |
| `docs/mockups` | The five mockups the dashboard was built from, and where it departs from them. |

**The store crate is the only thing that opens the database.** The binary, and every
later caller — the HTTP API, the dashboard, the harvester — goes through its typed
functions. That is what keeps one way into the record.

## The dashboard

Four screens — the inbox, one review, the draft preview and the rules — in `web/`. It runs
on sample data and calls no API yet.

```sh
cd web
pnpm install
pnpm dev        # http://localhost:3000
pnpm generate   # static files in web/.output/public
pnpm lint
pnpm typecheck
pnpm test
```

To check the built output the way the binary will serve it, serve the directory with
anything that serves files:

```sh
cd web/.output/public && python3 -m http.server 8080
```

Nuxt runs with server-side rendering off and generates static files, which a later issue
embeds in the binary. Nuxt's own server routes are not used: the JSON API is the Rust
server's. Fonts are bundled rather than fetched, so the built app asks nothing of the
network and works on a machine with no internet.

**Every screen reads and writes through one composable**, `web/app/composables/useReviewDesk.ts`,
whose four reads mirror the four the API offers — one GET per screen, so no screen joins
anything. It gets its records from `web/app/data/snapshot.ts`, the only file that names a
fixture. Pointing the dashboard at the real server is a change to that one file. A test
enforces it rather than trusting it:

```sh
cd web && pnpm exec vitest run test/architecture.spec.ts
```

While the fixtures are serving, every screen carries a "Sample data" marker in the header.

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
