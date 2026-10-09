# Review Desk

**Review Desk keeps the record of an agent-assisted code review** — the brief, the
options considered, the findings, what became of each finding, and the rules learned
from them — and feeds what it learns back into the next review. It is not a list of
pull requests: GitHub and Linear already do that, and its unit is the finding and the
rule, not the PR. It runs locally, owns its own record, and needs nothing else
installed.

This repository is at the third slice: the store, the commands an agent session calls
to record a review, the harvester that reads a posted review's outcomes back from
GitHub, the dashboard's four screens, and the local HTTP server that serves the JSON API
and the dashboard from one binary. **All sixteen of the contract's operations are
answered.** The dashboard still runs on sample data and calls no API yet; switching it
over is the next slice.

**Only the developer signs off.** Confirming a brief and approving a draft happen in
the dashboard; the command line that agent sessions call has no command for either. An
agent writes a draft and reads its status, and cannot approve it. That is enforced by
the store rather than promised by the commands — see [Sign-off](#sign-off).

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

It also takes what the caller knows about the pull request. A fact given is written and
a fact left out is left as it was, so running it again on a head commit already
reviewed records what the review did not check without starting a round:

```sh
review-desk review open --repo devergehq/review-desk --pr 851 --head c0ffee1 \
  --title "Bring the store up to the API contract" --ticket DEV-860 --changed-lines 214

review-desk review open --repo devergehq/review-desk --pr 851 --head c0ffee1 \
  --not-checked "the dashboard against the real API; a database at version 1"
```

`review find` lists a pull request's rounds, newest first, for an agent that has lost
an id:

```sh
review-desk review find --repo devergehq/review-desk --pr 851
# 2 round 2 head 4f1a9c2 — Bring the store up to the API contract
# 1 round 1 head c0ffee1 — Bring the store up to the API contract
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
  "approach_verdict": "the command line first: any agent on any platform can call it",
  "provenance": "from the ticket and the code · you corrected one sentence",
  "options": [
    {
      "key": "A",
      "title": "A local HTTP API only",
      "argument_for": "One way in, and the dashboard needs it anyway",
      "argument_against": "An agent on another machine cannot reach it",
      "chosen": false
    },
    {
      "key": "B",
      "title": "A command line first",
      "argument_for": "Any agent on any platform can make a shell call",
      "argument_against": "Two ways into the record once the API lands",
      "chosen": true
    }
  ]
}
```

The options are cards: a key, a title, the argument each way, and which one was built.
The older `option_map` field — one opaque string — still works and is still read, so
nothing written before this is lost, but it cannot serve the option cards on the
dashboard and nothing new should use it. Giving both is refused rather than merged.

**`brief confirm` has been removed.** Confirming a brief is the developer's sign-off
and is done in the dashboard; running the command exits 1 and says so. An agent
session reads the confirmation instead, through `review show`.

### The seven layers

`layer set` records what one of a review's seven layers is doing. The number fixes the
name — 1 lint, 2 understand, 3 alternatives, 4 solution_map, 5 code_review, 6 draft,
7 harvest — so only the number is given.

```sh
review-desk layer set --review "$REVIEW" --layer 1 --state done --detail passed
review-desk layer set --review "$REVIEW" --layer 4 --state skipped --detail "no diagram needed"
```

The states are `pending`, `running`, `done`, `waiting_on_you`, `failed` and `skipped`.
This is stored rather than worked out from the rest of the record, for two reasons:
layer 1's lint result has nothing to derive it from, and a derived state cannot tell a
layer that was **skipped** from one that has not started — which is the distinction
"what was not checked" exists to make.

A review always has seven layers when read back. One nothing has written reads as
`pending` with no time, rather than being absent.

`finding add` adds one finding and prints its sequence number within the review — the id
a person quotes. `lens`, `severity` and `claim` are required; the disposition defaults to
`in_draft`. `rule_id` names the rule the finding was raised under, which is the citation
the whole feedback loop runs through: without it a rule cannot be said to be cited, or
to have gone stale.

```sh
SEQ=$(review-desk finding add --review "$REVIEW" --file - <<'JSON'
{
  "lens": "correctness",
  "severity": "blocker",
  "file": "crates/cli/src/main.rs",
  "line": 48,
  "claim": "a refused request exits 2, which pages whoever treats 2 as broken",
  "evidence": "clap's default exit code is 2 and nothing overrides it",
  "rule_id": 9
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

`review show` prints the whole review: the layers, the brief with its options, the
findings, the outcomes and the draft.

```sh
review-desk review show --review "$REVIEW" --json
```

### The draft

`draft put` stores the review body and marks it ready for the developer. The file is
GitHub-flavoured markdown taken verbatim, or `-` for standard input — not JSON, because
a review body carries code fences and quotation marks and escaping them to get them
through a JSON string would be work for every caller and a chance to mangle what gets
posted.

```sh
review-desk draft put --review "$REVIEW" --file draft.md
review-desk draft put --review "$REVIEW" --file draft.md --lint lint.json
review-desk draft put --review "$REVIEW" --file draft.md --drafting   # not finished yet
```

`--lint` is what the review-craft lint said about this body. Every field is optional,
and a body saved without one has not been linted — so rewriting a draft clears a lint
that described the old text.

```json
{
  "passes": true,
  "visible_words": 118,
  "word_budget": 150,
  "findings_in_body": 3,
  "findings_expected": 3,
  "raised_before_on_pr": 0
}
```

`draft show` prints the draft, its status and, once it is approved, the event to post it
as. **This is how a review session learns it may post.**

```sh
review-desk draft show --review "$REVIEW"
# draft of review 3: approved
#   post as: request_changes (approved at 1760000000)
#   lint: passes, 118 of 150 words, 3 of 3 findings
#
# The queue-with-backoff approach fits the ticket…
```

`draft posted` records that the session has posted it, and which GitHub review it
became. **Refused unless the draft is approved.** This is the only transition that
involves GitHub, and the session makes it: Review Desk never posts.

```sh
review-desk draft posted --review "$REVIEW" --github-review gh-review-3312
```

A draft goes `drafting → ready → approved → posted`, with `ready ⇄ sent_back`. The
command line can reach `drafting`, `ready` and `posted`. It cannot reach `approved` or
`sent_back`: those are the developer's, in the dashboard.

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
| `draft_status` | The draft is not at a status this change can be made from: an approved draft cannot be rewritten, and an unapproved one cannot be marked posted |
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


## Sign-off

**Only the developer can sign off.** Decided by Patrick on 9 October 2026: confirming a
brief and approving a draft happen in the dashboard, and the command line agent sessions
call has no command for either.

| Act | Who | Where |
| -- | -- | -- |
| Write the brief | the review session | `brief put` |
| Confirm the brief | the developer | the dashboard only |
| Write the draft | the review session | `draft put` |
| Approve the draft, choosing the event | the developer | the dashboard only |
| Send the draft back with a note | the developer | the dashboard only |
| Post it and report the GitHub review | the review session | `draft posted` |

This is enforced by the store, not promised by the commands. `put_draft` accepts only
`drafting` and `ready`, so whatever writes a draft cannot sign it off by writing a
status; `approved` is reachable only through `approve_draft` and `posted` only through
`post_draft`. Both `confirm_brief` and `approve_draft` take the actor as a required
argument and refuse anyone but the developer, so the record cannot hold a claim that an
agent or the harvester signed something off.

`crates/cli/tests/signoff.rs` keeps it true: it fails if any source file in
`crates/cli` or `crates/harvest` so much as names those two functions, and it also
checks that they still exist, so renaming them cannot quietly retire the rule.

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
| `crates/server` | `review-desk-server`: the local HTTP server. The contract's JSON API and the embedded dashboard, on `127.0.0.1`. |
| `crates/cli` | `review-desk`: the binary. One module per command group under `src/commands`. |
| `web` | The dashboard: a Nuxt app, four screens, built to static files. |
| `contract` | The OpenAPI file both sides are built to, its examples, and the checks over them. |
| `docs/brief.md` | The project's design record. When it and the Linear project disagree, it is right. |
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

Nuxt runs with server-side rendering off and generates static files, which `cargo build`
embeds in the binary — see [the local server](#the-local-server). Nuxt's own server routes are not used: the JSON API is the Rust
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


## The local server

**`review-desk serve` serves the JSON API in `contract/openapi.yaml` and the dashboard,
from one binary, on `127.0.0.1`.**

```sh
review-desk serve                 # http://127.0.0.1:7777/
review-desk serve --port 8080
review-desk serve --port 0        # any free port; the one bound is printed
review-desk serve --json          # one document saying where it is
```

It prints where it is and then serves. The API is under `/api/v1`, matching the
contract's `servers` entry; everything else is the dashboard.

```sh
curl -s http://127.0.0.1:7777/api/v1/health
# {"front_end_embedded":true,"schema_version":4,"status":"ok"}
```

### Building a binary with the dashboard in it

Two commands, in this order. The second embeds whatever the first produced.

```sh
cd web && pnpm install --frozen-lockfile && pnpm run generate && cd ..
cargo build --release
```

**`cargo build` on a machine with no Node also works**, and is the normal case for
anyone working on the Rust side. The binary then serves the API exactly as it otherwise
would, and `/` is a page saying the front end was not built and naming the two commands
above. `GET /api/v1/health` reports which kind of build it is, which is also how CI
checks both.

The embedding is a build script: it walks `web/.output/public` if it is there and emits
an empty table if it is not. Build the front end after a `cargo build` and the next
`cargo build` picks it up.

**If `/` shows the "not built" page when you expected the dashboard, check
`front_end_embedded` and then the modification times.** Cargo decides whether to re-run
a build script by comparing them, and copying or moving a previously built
`web/.output/public` into place preserves them — so nothing looks changed. A fresh
`pnpm run generate` always writes new files and is never affected; `touch -c` over the
directory fixes a restored one.

### What it binds, and who may talk to it

**It binds `127.0.0.1` and there is no flag to change that.** There is no authentication
in this version — being able to reach the port is the authorisation, which
`contract/README.md` states and justifies. That is sound only while the bind holds: on
`0.0.0.0`, or through a tunnel, the whole record becomes readable and writable by anyone
who can reach it. The version that adds authentication is the version that may add the
flag.

Two tests hold the line: one asserts the address bound, and one asserts that every
non-loopback address this machine has refuses a connection on the same port. The second
exists because a listener on `0.0.0.0` would pass the first.

**Binding loopback is not enough on its own, because a web page is also on this machine.**
Confirming a brief and approving a draft happen through this API and nowhere else, and an
approved draft is what a review session posts to GitHub under the developer's name. A page
on another origin that could `POST` to `127.0.0.1:7777` could approve a draft the
developer has never read — and "only the developer signs off" would be enforced by
nothing. So every request to `/api/v1` passes `crates/server/src/guard.rs` first:

| Check | Closes |
| -- | -- |
| `Host` is the loopback address and the port actually bound | DNS rebinding: a name the attacker controls, resolved to 127.0.0.1 |
| `Origin`, when present, is this server's own | A page on another origin driving `fetch`, or submitting a form |
| `Content-Type` is `application/json` on a write | A plain HTML form, which cannot send one |

**Every response also carries `X-Frame-Options: DENY` and
`Content-Security-Policy: frame-ancestors 'none'`.** The guard stops another origin
*talking* to the API; it cannot stop one *embedding* the dashboard, and a click stolen
through a transparent iframe produces a request that genuinely comes from the dashboard's
own origin — every check passes, because nothing is wrong with the request. One stolen
click on a draft at `ready` publishes a review the developer never read, under their name.

**No CORS headers are sent**, by omission and on purpose; a test asserts none ever appear.
`localhost:<port>` is accepted beside the literal address, which is safe rather than a
convenience hole: rebinding points a name *the attacker controls* at 127.0.0.1, so the
browser sends that name and never `localhost`.

**An absent `Origin` is allowed**, deliberately. Nothing that is not a browser sends one —
`curl`, a script, a review session — and a browser always sends one on a cross-origin
request, including a form submission. Refusing the absent case would break every
non-browser caller and stop no attacker; the content-type check is what covers the form.

**What this does not stop: another process on the same machine.** A local program can set
any header it likes, so none of this defends against code already running as the
developer. That is the authorisation model the contract states, and closing it needs
authentication. What these checks buy is the narrower and more valuable thing — a web page
cannot reach the API, and a web page is the one attacker that arrives without the
developer installing anything.

### Which operations are answered

**All sixteen.** One GET per screen, and every write the four screens make.

| Screen | Reads | Writes |
| -- | -- | -- |
| Inbox | `GET /inbox` | `POST /reviews` |
| One review | `GET /reviews/{id}` | `POST …/brief/confirm`, `PATCH …/findings/{seq}` |
| Draft preview | `GET /reviews/{id}/draft` | `PUT …/draft`, `POST …/draft/approve`, `POST …/draft/send-back`, `POST …/held-back/ticket` |
| Rules | `GET /rules?repo=`, `GET /repos` | `POST /rules/{id}/promote`, `/dismiss`, `/retire`, `/defer` |

**Two of them are sign-offs, and only the developer can make one.** Confirming a brief
and approving a draft happen here and nowhere else: `review-desk brief confirm` does not
exist, and the store refuses any actor but the developer. The server passes a constant,
not an argument — a sign-off endpoint that could be told whose signature to record would
be one that could be told the wrong one.

**Approving does not post.** It sets the draft's status, stores the chosen event and the
time, and stops. The review session finds the approved draft and posts it under the
developer's name, then records the GitHub review id through `review link
--github-review`, which is what moves the status to `posted`.

A field the record has no source for is served as `null`, never as zero: session
liveness, the developer's own pull requests, the CI figures and the replay figures are
all absent rather than measured, and the dashboard has to render the absence.

### It reads GitHub once and never writes to it

`POST /reviews` reads a pull request's head commit when the caller omits one, which is
what the Inbox screen needs: it has only a pasted link. The read goes through
`crates/harvest`'s `GitHub` trait — four read-only methods, run through `gh`, so the
developer's existing login is used and Review Desk handles no tokens.

**The request is checked in full before anything reaches for GitHub.** A malformed link
is refused with `invalid_value`, not with a report that `gh` is missing, so a machine
with no `gh` is told the real problem. A supplied `head_sha` means no read is made at
all. If the read cannot be made, the answer is `github_unavailable` and **no review is
created** — a review with no head commit could not be deduplicated or given a round.

### Errors on the wire

The body is the contract's `Error` object itself, not a document wrapping one — which is
different from the command line's `--json`, because the two have different contracts.

```json
{ "code": "conflict",
  "message": "this draft is approved and only a draft that is ready can be sent back" }
```

| `code` | Status | Means |
| -- | -- | -- |
| `bad_request` | 400 | The body was not the shape the operation takes |
| `invalid_value` | 400 | One value was not acceptable; `field` and `expected` say which and what |
| `not_found` | 404 | No such review, finding, draft or rule |
| `conflict` | 409 | The record is not in a state the request can be made against |
| `store_unavailable` | 500 | Review Desk is broken. The HTTP equivalent of exit 2 |
| `github_unavailable` | 502 | A read Review Desk needed could not be made: `gh` missing or logged out |

**Refused and broken are tellable apart without reading the message**, which is the
command line's exit-code split carried onto the wire: `store_unavailable` is the only
code that means Review Desk itself is broken.

### Conformance

`crates/server/tests/conformance.rs` reads the schemas out of `contract/openapi.yaml` and
validates every response the server actually serves against them — rather than
transcribing the contract into Rust types, which would only check the server against one
reading of it.

```sh
cargo test -p review-desk-server
```

It includes a test that a deliberately wrong body **fails** validation, because a
conformance test that silently validates nothing passes every time.

### The transport, and why it is not `tiny_http` any more

**The server used to lose requests, silently and for ever, and that is why it runs on
`axum` now.**

It was built on `tiny_http` for a reason that still reads well: the store is one blocking
SQLite connection, so an async runtime buys nothing and costs a thread hop; the library is
tiny; and a `Server` that four threads call `recv()` on is exactly a small worker pool.
What that argument missed is what happens when the pool runs out. Each of its threads
parks on a keep-alive connection for as long as the browser holds it open, and when they
were all parked its queue did not merely wait — a connection could be left in it with
nothing able to pick the connection up. The request on it was then never answered and
never refused.

A browser opens several connections to one origin, six in Chromium, and asks for the
dashboard's assets down all of them at once. So this landed on the dashboard's **first
page load**: measured at a silent request in roughly one cold start in five, with `load`
never firing and the screen staying blank with no error on it. Nothing recovered it. The
server even went on answering `/health` perfectly, because a fresh connection always got
a thread, which is what made the fault so hard to see.

It could not be fixed from here: the pool is internal, the crate sets no socket read
timeout and offers no way to ask for one, and its last release was October 2022. The
choice was to maintain a fork of an abandoned HTTP server or to move to a maintained one.

| | Before | After |
| -- | -- | -- |
| Library | `tiny_http` 0.12.0, last released October 2022 | `axum` 0.8.9 and `tokio`, both `tokio-rs` |
| Serving | 4 threads, one parked per open connection | connections are futures on one thread |
| Blocking store work | on the serving thread | `tokio::task::spawn_blocking`, at one call site |
| Production crates | 30 | 53 |

**Twenty-three more crates is the real cost.** What it buys is a transport that does not
lose requests and no fixed worker count to get wrong. `crates/server/tests/concurrency.rs`
is the fault written down — it opens 5, 6 and 10 connections in one burst against twenty
freshly started servers and insists every request is answered. It fails on the old
transport within the first few trials and passes on this one.

Nothing above `crates/server/src/lib.rs` knows any of this. `Server::bind`, `serve`,
`spawn` and `Running` kept their signatures, `review-desk serve` is unchanged and still
synchronous, and no test was rewritten to suit the new library.

## The record

Nine entities, in four schema versions: the review record (version 1), the learning
layer (version 2), what harvesting needs (version 3), and what the local API contract
needs (version 4).

| Entity | Holds |
| -- | -- |
| Review | The repository as `owner/repo`, the pull request, the head commit, the round, the ids of the Allele session, the dispatcher ledger entry and the posted GitHub review, and what the pull request was last seen to be with when it was last harvested |
| Brief | One per review: the problem, its diagram, the option map, the approach verdict, and whether and when the developer confirmed it |
| Finding | Lens, severity, location, claim, evidence, its disposition with a reason, and the GitHub comment it was posted as |
| Outcome | What became of a finding, who did it, when, the detail, and the GitHub fact it was harvested from |
| Rule | A statement, the failure it prevents, its scope, its stage, the findings and review it came from, why it was proposed, the free line under it, when a finding last cited it, when to ask about it again, and when it was last changed |
| Review layer | One of a review's seven layers: its state, the phrase shown after its name, and when it was written |
| Brief option | One alternative considered: a key, a title, the argument each way, and whether it is what was built |
| Draft | The review body, its status, the event chosen at approval, who approved it and when, the note it was sent back with, when it was posted, and what the lint said |
| Rule exception | An accepted exception to a rule, with the reason. The rule stays in force |

A review is unique on repository, pull request number and round. A review has one
brief, one draft and at most seven layers.

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
