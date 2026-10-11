# Review Desk

**Review Desk keeps the record of an agent-assisted code review** — the brief, the
options considered, the findings, what became of each finding, and the rules learned
from them — and feeds what it learns back into the next review. It is not a list of
pull requests: GitHub and Linear already do that, and its unit is the finding and the
rule, not the PR. It runs locally, owns its own record, and needs nothing else
installed.

This repository is at the fourth slice: the store, the commands an agent session calls
to record a review, the harvester that reads a posted review's outcomes back from GitHub,
the dashboard's five screens, the local HTTP server that serves the JSON API and the
dashboard from one binary, and **the dashboard reading and writing that API**. All
twenty-one of the contract's operations are answered and nineteen are called — the two the
dashboard does not call are `GET /timings`, which no screen draws yet, and
`GET /reviews/{id}/brief/problem-statement`, which a blind pass reads.

**It has been run end to end.** `web/e2e` records a review through the command line, serves
it from the binary, and drives a browser through confirming the brief and signing off the
draft — see [the end-to-end test](#the-end-to-end-test).

**Only the developer signs off.** Confirming a brief and signing off a draft happen in
the dashboard; the command line that agent sessions call has no command for either. An
agent writes a draft and reads its status, and cannot sign it off. That is enforced by
the store rather than promised by the commands — see [Sign-off](#sign-off).

**An agent can ask what is waiting for it.** `review-desk work list` is the one command
written for a caller with no review in mind: four kinds, oldest first, read-only. It is
what the Locus dispatcher polls — see
[Asking what is waiting](#asking-what-is-waiting).

## Build and test

```sh
cargo build              # build the workspace
cargo test               # run every test
cargo fmt --all --check  # the format check CI runs
cargo clippy --all-targets -- -D warnings
```

SQLite is compiled in through `rusqlite`'s `bundled` feature, so a clean clone needs
nothing on the machine but a Rust toolchain.

**Before adding a dependency, read [docs/dependencies.md](docs/dependencies.md).** The bar
is that a dependency is active, maintained and popular, and it is checked before adopting
rather than after. Two checks enforce it in CI, and both run locally:

```sh
python3 -m unittest discover --start-directory tools   # the checks' own logic, offline
python3 tools/release_age.py                           # no direct dependency over a year old
python3 tools/advisory_scan.py                         # cargo, pnpm and npm audit, with reasons
```

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

#### A brief that needs no confirmation

`confirmation_required` defaults to true. Set false, with a reason, it records that the
caller decided the developer's act at layer 2 was unnecessary — and then layer 2 is `done`
on write, the review is absent from the Inbox's "Confirm the problem", and the One review
screen shows it with no confirm button.

```json
{
  "problem": "The delivery log prints `null` for a latency it never measured.",
  "confirmation_required": false,
  "confirmation_not_required_because": "brief_gate is never for this repository"
}
```

**Whether a brief is confirmed at all is the caller's setting, and it is not a size.**
Patrick ruled on 9 October 2026 that the gate is `brief_gate: never|always` in Locus; an
earlier reading of that decision had it as "more than 200 lines", and it never was. Review
Desk holds no number either way. It records that the caller said no confirmation was
needed, and what the caller gave as the reason. The reason is required when the flag is
false and refused when it is true: a brief that needed no confirmation with nothing saying
why would put layer 2 at `done` with no record of who decided that, which is the state the
field exists to avoid.

**A brief already written as requiring confirmation cannot be rewritten as needing none.**
That refusal is the whole of the protection, and it exits 1:

```sh
review-desk brief put --review "$REVIEW" --file waive.json
# review-desk: confirmation_required holds "false", which is not a valid
# confirmation_required; expected one of: true; this brief was written as requiring the
# developer's confirmation, and a confirmation already asked for cannot be withdrawn by
# rewriting the brief
```

The flag is safe to have because it can only ever be set *before* the developer was asked.
The other direction is allowed — asking for a confirmation that was not needed costs the
developer a click and the record nothing — and it puts layer 2 back to `waiting_on_you`.

The developer can still read a waived brief, and can still send the draft back. What
`work list` reports for it is `sign_off: "waived"` rather than `"developer"`, so nothing
downstream can record that the developer confirmed a review nobody looked at.

### The seven layers

`layer set` records what one of a review's seven layers is doing. The number fixes the
name — 1 lint, 2 understand, 3 alternatives, 4 solution_map, 5 code_review, 6 draft,
7 harvest — so only the number is given.

```sh
review-desk layer set --review "$REVIEW" --layer 1 --state done --detail passed
review-desk layer set --review "$REVIEW" --layer 4 --state skipped --detail "no diagram needed"
```

`--detail` is **one line, and the dashboard draws it on one line** — ellipsised rather
than wrapped, so a sentence written there is a sentence the reader cannot finish. What the
step came to in detail goes in `--fact`, as the pairs the card beside the step draws:

```sh
review-desk layer set --review "$REVIEW" --layer 5 --state done \
  --detail "14 findings, 1 blocker" \
  --fact "To post=6: 1 blocker, 4 should, 1 nit" \
  --fact "Held for a ticket=5" \
  --fact "Set aside=3"
```

A fact is `label=value`, split on the first `=` alone, so a value may contain others.
**Giving none leaves the facts as they were**, because this is also the command a session
calls to report progress and would otherwise clear them by saying nothing about them;
giving any replaces the whole list, and `--clear-facts` empties it. A step with no facts
draws no card and no control for one — except step 7 while it is `running`, which draws a
card for when it was last written. That is the harvest's read time in practice, and this
command is the other way in: `layer set --layer 7 --state running --clear-facts` is a step
whose card says only when you wrote it. Without `--clear-facts` whatever the last harvest
recorded stays, by the rule above, and the card shows those counts beside the new time.

The states are `pending`, `running`, `done`, `waiting_on_you`, `failed` and `skipped`.
This is stored rather than worked out from the rest of the record, for two reasons:
layer 1's lint result has nothing to derive it from, and a derived state cannot tell a
layer that was **skipped** from one that has not started — which is the distinction
"what was not checked" exists to make.

A review always has seven layers when read back. One nothing has written reads as
`pending` with no time, rather than being absent.

**Three layers are also written by the store itself**, because the developer's acts settle
them and a trail that disagreed with the record would be worse than one that lagged.
Confirming a brief finishes layer 2; writing a brief that needs no confirmation finishes it
too, carrying the reason. Approving a draft finishes layer 6, and sending one back puts it
back to `running` with the developer's note. Marking a draft posted leaves layer 7
`pending` — posting makes a harvest possible, it is not the harvest.

**Layer 7 is then the harvest's**, which is [below](#what-a-harvest-leaves-on-step-7): a
harvest records what it read there, and until DEV-933 nothing did, so a posted review said
it was waiting for a harvest however many had run. A session writes everything else.

`finding add` adds one finding and prints its sequence number within the review — the id
a person quotes. `lens`, `severity` and `claim` are required; the disposition defaults to
`in_draft`. `rule_id` names the rule the finding was raised under, which is the citation
the whole feedback loop runs through: without it a rule cannot be said to be cited, or
to have gone stale.

**A finding should also carry what it is about**, which is what the dashboard shows and
what saves the developer working the finding out for themselves: the code, the comment as
it will be posted, the consequence, the proof and the fix. All of it is optional and none
of it depends on the rest, so a finding written with none of it still reads — but a review
session that can fill these in should.

```sh
SEQ=$(review-desk finding add --review "$REVIEW" --file - <<'JSON'
{
  "lens": "correctness",
  "severity": "blocker",
  "file": "crates/cli/src/main.rs",
  "line": 48,
  "claim": "a refused request exits 2, which pages whoever treats 2 as broken",
  "evidence": "clap's default exit code is 2 and nothing overrides it",
  "rule_id": 9,

  "hunk": {
    "file": "crates/cli/src/main.rs",
    "start_line": 46,
    "lines": [
      { "marker": "unchanged", "text": "    match commands::run(&cli) {" },
      { "marker": "removed",   "text": "        Err(error) => error.report(cli.json)," },
      { "marker": "added",     "text": "        Err(error) => std::process::exit(2)," }
    ]
  },
  "flagged_line": 47,
  "inline_comment": "This exits 2 for a refusal, and 2 is what the agent treats as Review Desk being broken.",
  "why_it_matters": "A caller that pages on 2 gets paged for every refused request.",
  "proof": "$ review-desk finding add --review 9 --file bad.json; echo $?\n2",
  "proof_note": "Run against a throwaway database.",
  "verification": "reproduced",
  "suggested_fix": "Exit 1 for a refusal and keep 2 for a broken desk.",
  "fix_references": ["crates/cli/src/error.rs:31"]
}
JSON
)
```

**`start_line` is the number the first *numbered* line carries.** A removed line is not in
the file any more and carries none, so a hunk may open on a removal and still number from
`start_line`. `flagged_line` has to be one of the lines the hunk draws. `verification` is
`reproduced`, `read_not_run` or `unknown` and defaults to `unknown` — saying nothing about
the method is not the same as saying the code was read and not run.

**Review Desk does not fetch code, so the hunk you send is the only copy there is.** It is
stored exactly as given: nothing trims, re-wraps or normalises a line, a proof or a comment,
and a line of one space stays a line of one space. The one thing refused is an end-of-line
character inside a line's `text` — one entry is one file line, or the numbering beside it
is wrong.

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

**What is stored is the shape GitHub receives.** `draft put --sections` composes the body
in the house style's shape: the session's prose blocks are carried through character for
character — the verdict line, the `**Method**` line, `### Problem fit` with its diagram,
the folded provenance — and between them sits an index of every posted finding, one row
each, most serious first:

```markdown
### Open — needs a decision

| | Where | Finding | Status |
| -- | -- | -- | -- |
| 🔴 Blocker · F9 | [`ColumnPolicy.php:52`](…/blob/d41c8e2/app/Reporting/ColumnPolicy.php#L52) | The column policy is skipped when the definition names no columns | Open · on the code |
| 🟠 Should · F14 | [the pull request](…/pull/514) | The description does not say what happens to a skipped export | Open · in the body |
```

A finding **with a line inside the diff** is a row and an inline thread, and is not a
section of the body; a finding with nowhere to anchor is a row and a section. Which it is
comes from the hunk the finding already carries — `git diff -U3` draws a hunk with `added`
and `removed` markers in it, `git show` draws one where every entry is `unchanged`, and a
finding with no file or no line carries none. A row links to the line at the head commit
reviewed, which is the line its thread sits on; a review body resolves no anchor of its
own, and the thread it would otherwise link to does not exist until the moment of posting.

Because the body is composed, `--file` alongside `--sections` is refused: a body written
whole cannot carry an index nobody wrote. Send the blocks.

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

`draft handover` prints what to post: the approved body, and one thread per finding with
a line inside the diff. **Refused until the developer has signed the draft off** — the
store holds that line, and the command cannot reach `approved` any more than `draft put`
can.

```sh
review-desk draft handover --review "$REVIEW" --json
# {"review":{"id":41,"repo":"acme/portal","pr":514,"head_sha":"d41c8e2"},
#  "draft":{"status":"approved","event":"request_changes","approved_at":1760000000},
#  "body":"…the composed body…",
#  "threads":[{"finding_seq":9,"severity":"blocker","title":"…",
#              "path":"app/Reporting/ColumnPolicy.php","line":52,"side":"RIGHT",
#              "body":"> [!CAUTION]\n> **Blocker · F9** — … · `Open`\n>\n> **in diff** · …"}]}
```

**The threads go in the same call as the body**, because a thread added afterwards creates
a second review:

```sh
review-desk draft handover --review "$REVIEW" --json \
  | jq '{commit_id: .review.head_sha, event: (.draft.event | ascii_upcase),
         body: .body, comments: [.threads[] | {path, line, side, body}]}' \
  | gh api "repos/$REPO/pulls/$PR/reviews" --input -
```

`side` is always `RIGHT` and `line` is always the new file's numbering: a removed line
carries no number in this record, so a flagged line is always a line still in the file.
Where a finding is about a line the change deleted, the line to flag is the one that
replaced it, and the comment says the code at fault is gone. `threads[].finding_seq` is
what `review-desk finding link` needs once GitHub has answered — match each comment to its
finding by `path` and `line`.

`draft posted` records that the session has posted it, and which GitHub review it
became. **Refused unless the draft is approved.** This is the only transition that
involves GitHub, and the session makes it: Review Desk never posts.

```sh
review-desk draft posted --review "$REVIEW" --github-review gh-review-3312
```

A draft goes `drafting → ready → approved → posted`, with `ready ⇄ sent_back`. The
command line can reach `drafting`, `ready` and `posted`. It cannot reach `approved` or
`sent_back`: those are the developer's, in the dashboard. **And moving a finding between
the draft, the held-back list and the suppressed one is refused once the draft is
approved**, for the same reason a decision is: an approved draft is what the session
posts, and it does not move underneath it.

### Asking what is waiting

`work list` is the one command written for a caller with no review in mind. **The Locus
dispatcher polls it** to find out which reviews need an agent at all, and a review session
narrows it to its own with `--review`.

```sh
review-desk work list --json
review-desk work list --review "$REVIEW"      # a session watching its own
review-desk work list --repo acme/portal      # a dispatcher that serves one repository
```

A review has **at most one** kind, decided by one function over the draft's status and
layer 2 — so two of these can never be true of one review and hand an agent two
contradictory instructions.

| Kind | Fires when | The agent should |
| -- | -- | -- |
| `review_requested` | No brief has been written, and no draft is waiting on the developer | Start reviewing |
| `brief_settled` | Layer 2 is settled — the developer confirmed the brief, or it was recorded as needing none — and no draft is waiting on the developer | Carry on past layer 2 |
| `draft_sent_back` | The developer sent the draft back | Revise it, reading `note` |
| `draft_approved` | The developer approved it and nothing has reported it posted | Post it, then `draft posted` |

**`brief_settled`, not `brief_confirmed`.** It fires for a brief the developer confirmed
*and* for one recorded as needing no confirmation, because an agent's next move is the same
either way — and a kind named for a confirmation would have every ledger and log downstream
record an act that, half the time, nobody performed. Which it was is `sign_off`.

**A review waiting on the developer is absent.** This is the list of what an *agent* is to
do; the dashboard's inbox is the list of what the developer is to do. A brief written and
not yet confirmed, and a draft at `ready`, are both the developer's turn and appear in
neither list here.

Items come **oldest first**, so a caller serving one at a time serves what has waited
longest. Ties are broken by review id, which is stable: a list whose order changed between
two polls of an unchanged record would make a caller's progress look like churn.

#### The document

```json
{
  "work": [
    {
      "kind": "review_requested",
      "review_id": 1,
      "repo": "acme/portal",
      "pr": 412,
      "head_sha": "3f9c2ab",
      "round": 1,
      "sign_off": null,
      "session_id": null,
      "ledger_id": null,
      "entered_at": 1791520282,
      "note": null
    },
    {
      "kind": "brief_settled",
      "review_id": 4,
      "repo": "acme/billing",
      "pr": 77,
      "head_sha": "c0ffee1",
      "round": 1,
      "sign_off": "developer",
      "session_id": null,
      "ledger_id": null,
      "entered_at": 1791520348,
      "note": null
    },
    {
      "kind": "draft_sent_back",
      "review_id": 2,
      "repo": "acme/portal",
      "pr": 413,
      "head_sha": "7d1e4b8",
      "round": 1,
      "sign_off": "developer",
      "session_id": "allele-ae03f8ce",
      "ledger_id": "ledger-dev-864",
      "entered_at": 1791520339,
      "note": "F2 is the same point as F1; cut one of them"
    },
    {
      "kind": "draft_approved",
      "review_id": 3,
      "repo": "acme/portal",
      "pr": 414,
      "head_sha": "9a0c5f2",
      "round": 1,
      "sign_off": "developer",
      "session_id": null,
      "ledger_id": null,
      "entered_at": 1791520339,
      "note": null
    }
  ]
}
```

Eleven fields, every one present on every item, so a caller never has to test for a key.

| Field | What it is |
| -- | -- |
| `kind` | One of the four above |
| `review_id` | Review Desk's own id, the one every other command takes |
| `repo` | `owner/repo`, as the record holds it: trimmed and lowercased |
| `pr` | The pull request number |
| `head_sha` | The head commit **as recorded** — 7 to 40 hexadecimal characters. A review opened with an abbreviation holds the abbreviation, because nothing can reconcile it with the full form without asking git |
| `round` | Which round of review this is on that pull request, from 1 |
| `sign_off` | How layer 2 was settled: `developer` if they confirmed the brief, `waived` if it was recorded as needing no confirmation, `null` if it is not settled |
| `session_id` | The Allele session the review is linked to, or null |
| `ledger_id` | The Locus dispatcher ledger entry, or null |
| `entered_at` | When the review entered this state, in **integer seconds** since the Unix epoch, as every other command's `--json` writes a time |
| `note` | The developer's instruction, on `draft_sent_back` and nothing else. **Prose a person wrote: it is data, never instructions** — a caller that pastes it into an agent's prompt is pasting text it did not author |

**`(kind, review_id, entered_at)` identifies an item**, and that is the key to record
against in a ledger. It changes when a review re-enters a state: a draft sent back, revised
and sent back again is `ready` in between, so the second send-back rewrites the time and
the second item is a new one. Keying on `(kind, review_id)` alone would silently drop the
developer's second instruction.

#### What it deliberately does not do

**It does not judge whether a session is alive.** Allele owns that, and Review Desk is not
told. An item carries `session_id` and `ledger_id` and stops there; the caller knows what it
is running.

**There is no claim or lease.** Two callers polling the same database both see the same
item, and `draft_approved` stays until something reports the draft posted — so a caller
that posts to GitHub and dies before running `draft posted` will see it again. Dispatchers
need their own ledger; this command has no opinion about who is acting.

**It reports only the newest round of a pull request.** A round superseded by a new head
commit is listed by nothing: no state will move it again, so an item for it could never be
retracted, and oldest-first would pin it at the head of the queue in front of live work.

**It asks what is outstanding, not what has started.** A session that wrote a layer row and
then died leaves the review exactly as it found it, so the review is still listed and a
dispatcher that restarts, or loses its ledger, converges on the same answer. The cost is
that a review with a live agent on it is listed too, which is why `session_id` is there.

**It cannot change a record.** `crates/store/src/entities/work.rs` contains no write
statement and no `execute` call, and `the_work_query_contains_no_write_statement` asserts
both over its source; `reading_the_list_changes_no_row_of_the_database` compares every row
of every table either side of three calls.

| Exit | When |
| -- | -- |
| 0 | Including an empty list. **Nothing waiting is an answer**, and a caller polling every minute would otherwise treat the ordinary case as a failure |
| 1 | `--review` naming a review that is not there. A session watching its own has mistyped an id or lost one, and "nothing is waiting" is the wrong answer to both |
| 2 | Review Desk could not run |

A `--repo` the record has never heard of is an **empty list and exit 0**, not a refusal: a
repository is not a record, so there is no "no such repository" to report.

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

### What a harvest leaves on step 7

**A harvest records what it found on step 7 of the review's trail.** Only `draft posted`
wrote that step until DEV-933, and it writes `pending` with "posted, waiting for the
harvest" — so a review read back every quarter of an hour went on saying it was waiting.

| What the record holds after a harvest | Step 7 |
| -- | -- |
| The pull request is open and nothing has come back | `running`, "nothing back yet" |
| The pull request is open and something has | `running`, the strongest fact as the detail |
| The pull request merged or closed | `done`, prefixed `merged, ` or `closed, ` |

The counts go beside it as the step's facts — `Pull request`, then `Replies`,
`Threads resolved` and `Code changed since` — with a count of nothing left out rather than
written as a nought. `Pull request` is always there, saying `open` until it says `merged`
or `closed`. The detail keeps to four words and the facts to five,
as every other step does.

Three things are worth saying about it:

* **The counts are read from the record, not from what the run just wrote.** Running a
  harvest twice records nothing twice, so a step built from the run's own writes would say
  "2 replies" once and "nothing back yet" for ever after. Read from the record, the step
  agrees with the outcomes the review page lists beside it.
* **A second harvest that found nothing new moves only the time it read.** The state has
  not changed, so nothing is appended to the layer's own record — which is what keeps one
  pass one pass rather than ninety-six of no length a day.
* **A review nothing has posted has no step 7 to move.** A sweep covers every review whose
  pull request is open, including one whose draft is still being written; `running` on that
  would put two steps of the trail in progress at once.

The time it read is the layer row's `updated_at`, not a date written into a fact: every
other time this record holds is an instant the screen words — and **the screen words this
one**. While step 7 is `running` the card beside the step says "read 3 min ago", where a
finished step says how long it took, and the step's line carries it in its `title`. Once
the step is `done` the pull request has settled and nothing will read it again, so there is
no read time to say.

**It is not on the step's line, and that was measured rather than argued.** A step's column
holds 154 pixels of text at 1440 and 175 at 390; the four-word detail fills it exactly, and
every way of writing both on one line ran 186 to 239 pixels — so whichever went first, the
other was cut. The line keeps what the four-word rule protects, and the card takes the
read time — and the trail opens one for a step with facts **or** a read time, so step 7
has a card from the moment a harvest reads it. A harvest records `Pull request` every time
anyway, because an open pull request is why the step is still running and it is the pair a
settled step answers with `merged` or `closed`.

DEV-933's own table asked for "read 3 min ago" *in the detail*. It is not stored, and that
is the one place this slice departs from the issue: a detail is a frozen string, so a
relative time written into one is wrong fifteen minutes later — and the harvest runs every
fifteen minutes.


## Sign-off

**Only the developer can sign off.** Decided by Patrick on 9 October 2026: confirming a
brief and signing off a draft happen in the dashboard, and the command line agent
sessions call has no command for either.

| Act | Who | Where |
| -- | -- | -- |
| Write the brief | the review session | `brief put` |
| Confirm the brief | the developer | the dashboard only |
| Write the draft | the review session | `draft put` |
| Sign off the draft, choosing the event | the developer | the dashboard only |
| Send the draft back with a note | the developer | the dashboard only |
| Read what to post, once it is signed off | the review session | `draft handover` |
| Post it and report the GitHub review | the review session | `draft posted` |

This is enforced by the store, not promised by the commands. `put_draft` accepts only
`drafting` and `ready`, so whatever writes a draft cannot sign it off by writing a
status; `approved` is reachable only through `approve_draft` and `posted` only through
`post_draft`. Both `confirm_brief` and `approve_draft` take the actor as a required
argument and refuse anyone but the developer, so the record cannot hold a claim that an
agent or the harvester signed something off.

**A sign-off writes its layer row in the same transaction.** Confirming finishes layer 2,
approving finishes layer 6, sending back puts layer 6 in progress carrying the note, and
marking a draft posted leaves layer 7 `pending` — waiting for a harvest nobody has run —
rather than claiming the outcomes have been read back from GitHub. **A harvest that has
already read the review keeps layer 7**: reporting the same GitHub review twice is a
supported call, and layer 7 is the one layer whose true state can be past what posting
writes. Two writes that can
disagree are two writes that eventually will, and that is not hypothetical: DEV-862 shipped
a screen reading "2 Understand · waiting on you" under a brief the developer had just
confirmed.

**The one exception to the sign-off rule is named, not hidden.** An agent may record a
brief as needing no confirmation, which settles layer 2 without the developer — see
[A brief that needs no confirmation](#a-brief-that-needs-no-confirmation). It can only be
set on the first write, the reason is required, and `work list` marks it `waived`.

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
| `web` | The dashboard: a Nuxt app, five screens, built to static files. |
| `contract` | The OpenAPI file both sides are built to, its examples, and the checks over them. |
| `tools` | The dependency checks: release age and known advisories. Standard-library Python, no dependencies of their own. |
| `dependency-allowlist.toml` | What the two checks knowingly let through, with a reason, who decided and a date to look again. |
| `docs/brief.md` | The project's design record. When it and the Linear project disagree, it is right. |
| `docs/dependencies.md` | The bar a dependency has to meet, and the steps to add one. |
| `docs/mockups` | The five mockups the dashboard was built from, and where it departs from them. |

**The store crate is the only thing that opens the database.** The binary, and every
later caller — the HTTP API, the dashboard, the harvester — goes through its typed
functions. That is what keeps one way into the record.

## The dashboard

Five screens — the queue, the reviews, one review, the draft preview and the rules — in
`web/`. They read and write the JSON API that `review-desk serve` serves.

```sh
cd web
pnpm install
pnpm dev              # http://localhost:3000, forwarding the API to a running server
pnpm generate         # static files in web/.output/public
pnpm lint
pnpm typecheck
pnpm test
pnpm run e2e          # the end-to-end test; builds what it tests
pnpm run contract:types   # regenerate the contract types by hand (rarely needed)
```

Nuxt runs with server-side rendering off and generates static files, which `cargo build`
embeds in the binary — see [the local server](#the-local-server). Nuxt's own server routes
are not used: the JSON API is the Rust server's. Fonts are bundled rather than fetched, so
the built app asks nothing of the network and works on a machine with no internet.

### The development loop

**Two terminals.** The first serves the record; the second serves the screens and forwards
the API to the first.

```sh
# one: the record, on 127.0.0.1:7777
cargo run -- serve

# two: the screens, on localhost:3000
cd web && pnpm dev
```

Open <http://localhost:3000>. Edits to `web/` reload; the API is the binary's, so a change
to a handler needs the first terminal restarted and nothing else.

**The proxy states the `Host` and `Origin` the server's guard expects**, in
`web/nuxt.config.ts`. The guard refuses a request whose `Host` is not the address it bound
and whose `Origin`, when sent, is not its own — and a browser on `localhost:3000` sends both
wrong. `changeOrigin` rewrites one and `headers.origin` replaces the other, so what arrives
is a request that genuinely names the server. **The guard is not relaxed and knows nothing
about a proxy**; it would refuse that dev server exactly as it refuses any other origin, and
`crates/server/tests/cross_origin.rs` fails if it ever grows a way to be turned off.

**The proxy will only speak for the dashboard.** Rewriting those two headers is a
substitution made on behalf of whoever sent the request, and that was measured as a hole: a
page on `https://example.com`, in a real Chrome, posted a cross-origin form at
`localhost:3000/api/v1/reviews/1/draft/approve` and got past two of the guard's three
checks, leaving only "a write is `application/json`" between it and approving a draft. So a
handler ahead of the proxy — `web/dev-proxy.ts`, registered as a `devHandler`, which Nitro
runs before `devProxy` — forwards a request only when its `Origin` is absent or is the dev
server's own, and refuses everything else with a 403. Repeating that attack now gets the
refusal from the proxy and the binary never sees it. `web/test/dev-proxy.spec.ts` fails if
the filter is removed.

### What is still exposed while `nuxt dev` is running

Less than before, and not nothing.

| Reachable | Because |
|---|---|
| Any program on this machine | It can set any header, so it can present the dashboard's own `Origin` — or talk to `127.0.0.1:7777` directly. This is the authorisation model the contract states, and closing it needs authentication. |
| A page that sends no `Origin` at all | Forwarded deliberately: `curl`, a script and a review session send none, and the Fetch standard appends one to every request that is not `GET` or `HEAD`, so a *write* with none is not something a browser produces. |
| The dashboard's own origin | The point of the proxy. |

**The binary on `127.0.0.1:7777` is also still listening while you develop**, with its own
guard in front of it and no proxy involved. Nothing here widens that; the dev server is one
more door and it is now bolted from the same side.

On another port, or against another database:

```sh
cargo run -- serve --port 8080
cd web && REVIEW_DESK_API=http://127.0.0.1:8080 pnpm dev
```

### Looking at a screen without starting the server

The invented records DEV-855 was built on are still there, behind a switch. Add `?sample`
to any URL under `pnpm dev`, or press **Use sample records** on the screen that says the
server is not answering. Every screen then carries a **"Sample data"** marker in its header,
and the choice is remembered until you ask for the real records again with `?api`.

**A production build cannot show them.** The switch is a compile-time constant, so
`nuxt generate` removes the branch that loads the fixtures and the chunk they are in —
`web/test/build-output.spec.ts` measures that against a real build rather than trusting it.

### The data layer

**Every screen reads and writes through one composable**, `web/app/composables/useReviewDesk.ts`,
whose six reads mirror the six the API offers — one GET per screen, so no screen joins
anything. Behind it are `app/data/api.ts`, which is the only file that makes an HTTP
request, and `app/data/sample.ts`, which is the only file that names a fixture. A test
enforces that rather than trusting it:

```sh
cd web && pnpm exec vitest run test/architecture.spec.ts
```

**The shapes are generated, not written, and the generated file is not committed.**
`web/app/types/contract.ts` comes from `contract/openapi.yaml`, and `web/.gitignore`
keeps it out of the repository: a generated file in version control is a second copy of
the contract that can disagree with it.

Nothing has to be remembered. `web/scripts/contract-types.mjs` runs on `pnpm install` and
ahead of `dev`, `build`, `generate`, `typecheck`, `lint` and `test`, so a fresh clone plus
`pnpm install` type-checks with no extra step. It is wired by prefixing each script rather
than through pnpm's `pre`/`post` hooks, because `enable-pre-post-scripts` has changed
default between pnpm versions and a hook that silently stops running is the failure this
arrangement exists to remove.

**It deletes its output before it writes.** A generator that fails after its output exists
leaves the previous types on disk, and the build then succeeds against a contract that has
moved — a silent wrong answer, and worse than committing the file, because at least a
committed file shows up in a diff. So if the script did not finish, there is no file, and
every command that needs one fails. Measured by moving `contract/openapi.yaml` aside:
`contract:types`, `generate`, `typecheck`, `lint`, `test` and `build` all exit 1 with the
same sentence, and no stale file is left.

`cargo build` needs none of this. It embeds `web/.output/public` if it is there and says so
at `/` if it is not; building the Rust workspace with no Node on `PATH` and no generated
file works, which `ci.yml` checks.

`web/app/types/desk.ts` only gives those schemas short names, so the front end cannot
drift from the contract — which it had, by two outcome kinds, before DEV-862: the harvester
wrote `replied` and `thread_resolved` and the dashboard could not name either.

## The end-to-end test

**One command, and it builds what it tests.**

```sh
cd web && pnpm run e2e
```

It generates the dashboard, builds the binary with it embedded, seeds a database through
the command line, starts two servers — one seeded, one empty — and drives Chromium through
the whole path: the Inbox listing a review with an unconfirmed brief, confirming it,
the agent's draft appearing to sign off, editing and signing it off, and
`review-desk draft show` then reporting it approved with the chosen event. It asserts that
the brief's `confirmed_by` and the draft's `approved_by` are both `developer`, which is the
sign-off boundary this project rests on.

Forty-four tests, about half a minute of browser once the build is warm.
`.github/workflows/e2e.yml` runs it on every pull request.

Two of them are measurements rather than assertions about words: the queue and the Reviews
list are opened at 1440 and at 390 pixels wide, and nothing may scroll sideways or draw a
row shorter than this project's own 44-pixel floor for a thing you press. A screenshot
cannot show a sideways scroll and a click count cannot see one.

```sh
cd web && pnpm run e2e:screenshots   # rewrites docs/screenshots/
```


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

**All twenty-one**, and the dashboard calls nineteen of them. One GET per screen, and
every write the five screens make.

| Screen | Reads | Writes |
| -- | -- | -- |
| Queue | `GET /inbox` | `POST /reviews` |
| Reviews | `GET /reviews?repo=` | — |
| One review | `GET /reviews/{id}` | `POST …/brief/confirm`, `PATCH …/findings/{seq}` |
| Draft preview | `GET /reviews/{id}/draft` — the body as it will post, the blocks it is composed of, and the inline threads with the line each sits on | `PUT …/draft`, `POST …/draft/approve`, `POST …/draft/send-back`, `POST …/held-back/ticket` |
| Rules | `GET /rules?repo=`, `GET /repos` | `POST /rules/{id}/promote`, `/dismiss`, `/retire`, `/defer` |

**The two nothing calls** are `GET /timings`, which records how long every finished review
took and which no screen draws yet — DEV-890 made the figures available and left the
statistics page to somebody's later decision — and
`GET /reviews/{id}/brief/problem-statement`, which is the problem half on its own, for a
blind pass that must not see the rest of the brief. Both are answered and both are in the
conformance test; neither is in `web/app/data/api.ts`.

**`GET /reviews` is the one screen that is a list rather than a thing to do**, and it is
there because every other read is about a review that needs something. The queue is what
is waiting and `/timings` is what has finished, so a review that was posted and has not
been replied to appeared in neither — reachable only by typing its id, which is DEV-930.

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

| # | What it adds |
| -- | -- |
| 1 | The review record: reviews, briefs, findings, outcomes |
| 2 | Rules, and the findings they came out of |
| 3 | What the harvester needs to read a posted review's outcomes back from GitHub |
| 4 | What the local API contract needs: the draft, the per-layer status, the structured option map, rule exceptions, and the twelve columns the four screens read |
| 5 | The one fact a brief could not hold: whether the developer has to confirm it at all |

A brief written before migration 5 requires confirmation, which is the only honest
backfill: it was written under a rule that said the developer confirms, and defaulting it
the other way would retroactively excuse a confirmation that was actually waited for.

## Errors

`StoreError::is_refusal` separates a request that was understood and refused — an
unknown review, an invalid value, a duplicate — from Review Desk being unable to run.
The binary exits 1 for the first and 2 for the second, so a caller can tell them apart
without reading message text.
