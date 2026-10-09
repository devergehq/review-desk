# The Review Desk local API contract

This directory is the agreement between the local Rust server and the Nuxt dashboard. It
holds no implementation of either.

| File | What it is |
|---|---|
| `openapi.yaml` | OpenAPI 3.1 for the local JSON API. 16 operations. |
| `examples/` | One example response per endpoint, one request body per write, and the error shape. A `.<variant>.` form adds a second shape where one example cannot show an optional field both present and absent. `getReview.response.round-two.json` is the second round of the same pull request as the canonical one. |
| `validate-examples.mjs` | Checks every example against its schema, and fails when an endpoint has no example. |

**Every example that carries `findings[]` carries the whole of a finding**, the canonical
file and each `.<variant>.` form alike, because `Finding` is `additionalProperties: false`
and `validate-examples.mjs` checks a variant against the same schema. A variant added for
one optional field still has to show every other — a `getReview` variant written to
demonstrate an absent `brief.provenance` needs the finding fields too, or the check goes
red on a file whose own subject never changed.
| `redocly.yaml` | Lint configuration, including the one rule turned off and why. |
| `package.json` | The pinned tooling the checks run on. |

Written for DEV-854. The design it serves is in the Review Desk project brief; the store
it reads from is DEV-850, merged to `master` as pull requests #1 and #2 on 8 October 2026.

## Running the checks

```bash
cd contract
npm ci
npm run lint       # redocly lint openapi.yaml
npm run validate   # node validate-examples.mjs
npm test           # both
```

`.github/workflows/contract.yml` runs the same two commands on every pull request that
touches `contract/`.

**`npm run lint` reports two warnings and exits 0.** Both are `operation-4xx-response`
against `getInbox` and `listRepos`, which take no input and so have no 4xx to declare.
The rule is left on because it is doing real work on the other fourteen operations. CI
gates on errors, which is what redocly exits non-zero for.

## There is no authentication

**The server binds `127.0.0.1` only, and this version has no authentication of any
kind.** Being able to reach the port is the authorisation. That is why `redocly.yaml`
turns off the `security-defined` rule: the honest statement is "no auth", and declaring an
empty security scheme to quiet a linter would imply a scheme exists.

This is sound only while the bind holds. Binding `0.0.0.0`, putting the port through a
tunnel, or running it on a shared machine makes the whole record readable and writable by
anyone who can reach it. Authentication is out of scope for DEV-854, and the rule above
goes back on in the same change that adds it.

## Conventions

| Rule | How it shows up |
|---|---|
| Times are RFC 3339 in UTC | Every time field is `type: string, format: date-time`, with a `Z` offset. The store holds integer epoch seconds; the server converts here. |
| Foreign identifiers are opaque | `ForeignId` — the Allele session, ledger entry, GitHub review and Linear ticket are strings Review Desk stores and never parses. |
| One error shape | `Error`, with `code` from a closed set, a `message` for a person, and `field`/`expected` when one value caused the refusal. |
| Refused and broken are distinguishable | `store_unavailable` is the only code meaning Review Desk is broken — the HTTP equivalent of DEV-851's exit code 2. Every other code means the request was understood and refused. A caller branches on `code`, never on the message. |
| Nothing **posts** to GitHub | No operation writes to GitHub. Approving a draft sets a status; the review session reads it and posts. One operation *reads* GitHub — `POST /reviews` resolves a head commit when the caller omits one — and that is the only one. |
| Review Desk's own ids are integers | Review id, finding `seq` and rule id, matching `crates/store/src/model.rs`. |
| Null is not zero | A count the store cannot yet answer is `null`. Zero is a measurement; null is the absence of one. |
| A derived field states how it is derived | `Rule.stale` is the one with a threshold in it — no finding has cited the rule for 60 days and it has not been deferred past today — so the number is written in `openapi.yaml` and the server's constant is tied to it by `crates/server/tests/staleness.rs`. It was undefined until DEV-862, and the server used 60 days because a mockup said so. |

`F1` and `R-09` are display keys, derived rather than stored: `F` plus the finding's `seq`,
and `R-` plus the rule's `id` zero-padded to two digits. The server derives them so the
dashboard never computes an identifier.

## Screen to endpoint

Each screen is one GET. The dashboard does no joining.

| Screen | Reads | Writes |
|---|---|---|
| Inbox | `GET /inbox` | `POST /reviews` |
| One review | `GET /reviews/{reviewId}` | `POST /reviews/{reviewId}/brief/confirm`, `PATCH /reviews/{reviewId}/findings/{seq}` |
| Draft preview | `GET /reviews/{reviewId}/draft` | `PUT …/draft`, `POST …/draft/approve`, `POST …/draft/send-back`, `POST …/held-back/ticket` |
| Rules | `GET /rules?repo=`, `GET /repos` | `POST /rules/{ruleId}/promote`, `/dismiss`, `/retire`, `/defer` |

The Inbox's `POST /reviews` is a write that DEV-854's own table records as "—". The
mockup has a "Start a review" field and a button, and DEV-855 lists it too, so the
endpoint exists. Flagged rather than quietly dropped.

## Every visible datum, by screen

The tables map **field kinds**, not the mockups' sample values: "#407" and "#412" are both
`pull_request.pr`, and "F1 · A retried delivery…" is `findings[].key` and `.claim`.

Page chrome is front-end copy and intentionally absent from the API. So that a deliberate
omission is distinguishable from an oversight, it is all of: the product name, the "Sample
data" badge, the navigation, the group and section headings, the summary tiles' captions
("findings matched on the last replay", "reviews that went in a circle", "cap of 20 per
repository" apart from its number), and these fixed sentences —

- "Nothing has been posted. This is what GitHub will show."
- "Starts with the problem brief. No findings are written until you confirm it."
- "Saved as feedback when you approve. Three edits of the same kind become a proposed rule."
- "The review session posts it under your name, with no bot banner."
- "written before the diff was read"
- "Every rule names the failure it prevents. A preference with no consequence does not qualify."

Everything else on the four screens is below.

### Inbox — `GET /inbox`, example `examples/getInbox.response.json`

| On the mockup | Field |
|---|---|
| "7 items across 2 repositories" | `totals.items`, `totals.repos` |
| "Confirm the problem · 1" | length of `briefs_to_confirm` |
| "#412" | `briefs_to_confirm[].pull_request.pr` |
| "Retry failed webhook deliveries" | `briefs_to_confirm[].pull_request.title` |
| "acme/portal" | `briefs_to_confirm[].pull_request.repo` |
| "ABC-231" | `briefs_to_confirm[].ticket_id` |
| "brief ready 6 min ago" | `briefs_to_confirm[].brief_ready_at` |
| "nothing else runs until you confirm" | `briefs_to_confirm[].blocks_later_layers` |
| "Approve a draft · 2" | length of `drafts_to_approve` |
| "round 1" | `drafts_to_approve[].round` |
| "1 blocker, 2 should" | `drafts_to_approve[].severities` |
| "4 nits held back for a ticket" | `drafts_to_approve[].held_back` |
| "delta since your last review" | `drafts_to_approve[].is_delta` |
| "Author replied · 1" | length of `author_replies` |
| "finding F2 rejected with a reason" | `author_replies[].events[]` — `finding_key`, `kind: rejected`, `detail` |
| "F1 fixed in a later commit" | `author_replies[].events[]` — `kind: code_changed_after` |
| "Your PRs with no reviewer · 2" | length of `own_prs_without_reviewer` |
| "open 3 days" | `own_prs_without_reviewer[].opened_at` |
| "description lint passes" | `own_prs_without_reviewer[].description_lint.passes` |
| "description is 2.3× its budget" | `own_prs_without_reviewer[].description_lint.budget_ratio` |
| "Proposed rule · 1" | length of `proposed_rules` |
| the rule's sentence | `proposed_rules[].statement` |
| "You cut this kind of finding from 5 of the last 5 drafts…" | `proposed_rules[].why_proposed` |
| "In progress · 2" | length of `in_progress` |
| "Layer 5 of 7 · code review" | `in_progress[].layer_number`, `.layers_total`, `.layer_name` |
| "session alive" | `in_progress[].session_alive` |
| the progress bar's fill | derived from `layer_number / layers_total` |
| "Handled without you this week" | `handled_without_you.window_days` |
| "9 description problems caught by the CI check" | `handled_without_you.description_problems_caught` |
| "11 nits filed as 3 tickets for an agent" | `handled_without_you.nits_filed`, `.nit_tickets` |
| the "Paste a pull request link" field | `StartReviewRequest.pr_url` |

### One review — `GET /reviews/{reviewId}`, example `examples/getReview.response.json`

| On the mockup | Field |
|---|---|
| "#412" | `review.pr` |
| "Retry failed webhook deliveries" | `review.title` |
| "acme/portal" | `review.repo` |
| "ticket ABC-231" | `review.ticket_id` |
| "round 1" | `review.round` |
| "head 3f9c2ab" | `review.head_sha` |
| "214 changed lines" | `review.changed_lines` |
| the seven layer chips | `layers[].number`, `.name`, `.state`, `.detail` |
| the chip's dark / dashed / plain styling | `layers[].state` — `waiting_on_you` and `running` are current, `pending` is not started |
| the problem paragraph | `brief.problem` |
| "from the ticket and the code · you corrected one sentence" | `brief.provenance` |
| the three-box diagram | `brief.diagram`, as mermaid source |
| "A · Retry inside the request" | `brief.options[].key`, `.title` |
| "For:" / "Against:" | `brief.options[].argument_for`, `.argument_against` |
| the "What they built" badge | `brief.options[].chosen` |
| "Verdict: …" | `brief.approach_verdict` |
| "3 in the draft · 2 held back · 1 suppressed" | `finding_dispositions` |
| the Blocker / Should / Nit pill | `findings[].severity` |
| "F1" | `findings[].key` |
| "A retried delivery is sent without an idempotency key" | `findings[].claim` |
| "jobs/DeliverWebhook.php:58" | `findings[].file`, `.line` |
| "lens: sibling diff" | `findings[].lens` |
| "evidence: throwaway test, 2 deliveries for 1 event" | `findings[].evidence` |
| "same ruling as R-09" | `findings[].rule_key`, `.rule_id` |
| "In draft" / "Suppressed" | `findings[].disposition` |
| "Held back · ticket" | `findings[].disposition` plus `nit_ticket.status` |
| "No consequence named · matches a proposed rule…" | `findings[].disposition_reason` |
| "jobs/DeliverWebhook.php · lines 56 to 63, as changed" | `findings[].hunk.file`, and the range derived from `.hunk.start_line` and the markers |
| the code rows, each with its `+`, `−` or blank gutter | `findings[].hunk.lines[].marker`, `.text` |
| the number beside each code row, blank on a removal | `findings[].hunk.lines[].line` |
| "Comment on line 58, as it will post" | `findings[].flagged_line` |
| the comment under that line | `findings[].inline_comment` |
| the "Why it matters" paragraph | `findings[].why_it_matters` |
| the "How we know" block | `findings[].proof` |
| the caption under that block | `findings[].proof_note` |
| the "Reproduced" badge, and its absence | `findings[].verification` |
| the "What would fix it" sentence | `findings[].suggested_fix` |
| the monospace lines under it | `findings[].fix_references` |
| the "Not checked" paragraph | `not_checked` |
| that an "Open draft" link is shown at all | `draft_status` |
| which of the three layer-2 cards is shown | `brief.confirmed` and `brief.confirmation_required` |
| "Confirmation not required. The session recorded this brief as needing none: …" | `brief.confirmation_not_required_because` |
| "Round 1 · posted 6 Oct as Request changes · 5 findings" | `rounds[].round`, `.posted_at`, `.event`, `.findings` |
| "The author came back · 3 commits · 3 replies · 2 threads resolved" | `rounds[].replies`, `.threads_resolved`, and the **next** round's `.commits` — see below |
| "Round 2 · this draft · 2 fixed · 2 still open · 1 new" | `rounds[].statuses` |
| "What changed since round 1" | `commits_since_previous_round[].sha`, `.summary`, `.files` |
| "Fixed · 2 · nothing to post" | `findings[].round_status` |
| "Checked. Their new test fails with the fix reverted." | `findings[].round_status_checked` |
| the `a41c9e2` chip on a fixed finding | `findings[].round_status_commit` |
| who decided a status | `findings[].round_status_by` |
| "raised in round 1" | `findings[].continues_round`, linked by `.continues_review_id` |
| "open for 2 rounds" | `findings[].rounds_open` |
| "Introduced by a41c9e2, the commit that fixed F1" | `findings[].introduced_by_commit`, `.introduced_by_seq`, `.introduced_by_key` |
| "Late" | `findings[].late` |
| "Went to ticket ABC-246 in round 1. Not part of this round." | `findings[].round_status` of `ticketed`, with `.round_status_checked` |

`finding_severities` is also served, for a caller that wants the Inbox's "1 blocker, 2
should" wording on this screen.

**Layer 2 has three states, not two.** `confirmed` true is a brief the developer signed.
`confirmed` false with `confirmation_required` true is one they are being asked to sign, and
is the only one of the three that appears in the Inbox's `briefs_to_confirm`. `confirmed`
false with `confirmation_required` false is a brief an agent recorded as needing no
confirmation — settled, unsigned, in nobody's queue, and the screen must say so rather than
offering the button. The reason is required whenever the flag is false; the threshold behind
it is the caller's and Review Desk does not hold it.

#### What a finding is about — the hunk, the comment, the proof and the fix

Added by DEV-874, which is where boards A2 and A3 of the approved design draw a finding in
full. **All eight fields are optional and independent; a finding with none of them displays
with its file and its line, as it always did.**

| Field | What it is |
|---|---|
| `hunk` | The lines around the finding as the pull request changed them: a file, a start line, and each line marked `added`, `removed` or `unchanged`. `null` when the finding has none. |
| `hunk.lines[].line` | **Derived, not stored.** Counting from `hunk.start_line`, an `added` or `unchanged` line takes the next number and a `removed` line takes `null` — it is not in the file any more, and the design draws no number for it. Board A2's F1 starts at 56 and draws 56, 57, (blank), 58 … 63; board A3's F6 opens *on* a removal and still numbers from 58. |
| `flagged_line` | The line the inline comment attaches to. When a `hunk` is present this is one of the numbers that hunk draws, and the server refuses a write that anchors a comment anywhere else. |
| `inline_comment` | The comment as it will be posted on that line. **Not the `claim`**: the claim is what the record says is wrong, this is what the author reads on GitHub. Nothing here posts it. |
| `why_it_matters` | The consequence, in a sentence or two. |
| `proof` | How the review knows — test output, two logged keys, a line read. Newlines are part of it. |
| `proof_note` | The plain caption beneath the proof — "A throwaway test, not committed." Its own field because the proof is drawn as a monospace block and this is a sentence about it. |
| `verification` | `reproduced`, `read_not_run` or `unknown`, defaulting to `unknown`. |
| `suggested_fix` | What would fix it. |
| `fix_references` | Sibling code the fix should look like, in order. `[]` when there are none. |

**Every one of the text fields is stored and served verbatim.** Review Desk does not fetch
code — the project brief's rule — so the hunk the API serves is the only copy of those lines
there is. Nothing trims, re-wraps or normalises them at any point: a line of a single space
is a line of a single space, a tab stays a tab, and a trailing space survives. `text` may be
the empty string. The one piece of text the server looks at before storing is a
`fix_references` entry, which is a pointer the dashboard draws as one line and is refused
when it is blank; refusing is not normalising, and nothing stored is altered.

**A hunk line is one file line.** `text` carries no end-of-line character, and the server
refuses a write whose line text contains one: `hunk.lines[].line` gives exactly one number
per non-removed entry, so an entry holding three file lines would draw three rows under one
number and shift every number after it — and `flagged_line` with them.

**A removed line cannot be commented on.** It carries no number, so no value of
`flagged_line` names it, and a write that tries is refused rather than stored undrawable.
GitHub can comment on the left-hand side of a diff; this model cannot, and that is a stated
limit rather than an oversight.
### A finding across rounds

**A finding's number is its identity.** F2 in round 1 is F2 in round 3: `seq` is allocated
once for the whole pull request, a carried finding keeps the number it had, and a new one
takes the next number unused on the pull request. Before this, each round numbered its
findings from 1, so round 1's F2 and round 2's F2 were unrelated rows and "two fixed, two
still open, one new" was unanswerable.

| Fact | Where |
|---|---|
| Which earlier round a finding continues | `Finding.continues_review_id`, and `.continues_round` for the number |
| What became of it in this round | `Finding.round_status`, with `.round_status_by`, `.round_status_checked`, `.round_status_at`, `.round_status_commit` |
| What a new finding is attributed to | `Finding.introduced_by_commit`, `.introduced_by_seq` |
| The rounds so far | `ReviewDetail.rounds` |
| What changed between two rounds | `ReviewDetail.commits_since_previous_round` |

**Three fields are derived, not stored**, so none of them can drift from the chain they
come from: `continues_round`, `rounds_open` and `late`. They are served by `getReview` only,
because each needs a read the other operations have no reason to make — the contract
declares all three nullable, and `setFindingDisposition` returns a `Finding` without them.

**`round_status` is not `disposition`, and both are served.** A disposition says what is to
become of a finding in this round's draft; a round status says what happened to the finding.
A fixed finding has nothing to post, so it is `fixed` with whatever disposition the agent
gave it. **Nothing derives a round status**: an agent or the developer sets it and says how
they checked, which is what `round_status_by` and `round_status_checked` hold.

**`DecidedBy` is a separate set from `Actor`, deliberately.** `Actor` is developer, author or
harvester, and has no value for an agent — which decides most round statuses. Recording an
agent's decision as `developer` would put the developer's name against a check nobody showed
them, which is the mistake `SignOff.waived` was added to stop. Saying a finding is fixed is
not a sign-off: only the developer signs off, and only through `confirmBrief` and
`approveDraft`.

**`late` is three-valued.** True is a finding raised in this round, on code the round's
commits did not touch — something the earlier round could have caught. Null means the
question cannot be answered: a first round, a finding carried from an earlier round, a
finding that names no file, or a round where nothing is known about what its commits
touched. `false` would read as "this was not missed", which is a claim nothing is in a
position to make.

Two of those four were wrong first time and both were found by reviewing the change rather
than by a test. It keyed on `round_status` being `new`, so accepting the author's reason on
a finding raised in this round erased its lateness; it now keys on `continues_review_id`,
which is the fact that says where a finding came from. And it gated on "commits recorded"
rather than "anything known about what they touched", so a round whose commit list arrived
with no file lists reported every new finding as late. `review-desk round commits` now fails
rather than recording a commit whose files it could not read, and refuses a range longer
than GitHub will list in one comparison — its cap is 250 — because a truncated list that
nothing says is truncated feeds the same flag.

**`rounds[].commits` counts the commits recorded *for* that round — the ones since the round
before it.** The mockup shows the figure under the *earlier* round's card, as "the author
came back · 3 commits", so the page reads round *n*'s comeback from `rounds[n + 1].commits`.
It is one index rather than a second copy of the number, and the number has one meaning.

**The commits are read from GitHub and written nowhere.** `review-desk round commits` reads
`repos/OWNER/REPO/compare/BASE...HEAD` and one `repos/OWNER/REPO/commits/SHA` per commit,
through the harvest crate's four read-only methods. No operation in this contract fetches
them; the API serves what was recorded.

**A review from before this change reads as a first round.** Its findings have no links and
every `round_status` is `new`, which is true: nothing before migration 6 could carry one, and
inferring a link from a matching number would join two findings that have nothing to do with
each other.

### Draft preview — `GET /reviews/{reviewId}/draft`, example `examples/getDraft.response.json`

| On the mockup | Field |
|---|---|
| "#412" | `review.pr` |
| the rendered review body | `draft.body`, GitHub-flavoured markdown |
| the Markdown view | the same `draft.body`, unrendered |
| "Visible words 118 of 150" | `lint.visible_words`, `.word_budget` |
| "Findings in the body 3 of 3" | `lint.findings_in_body`, `.findings_expected` |
| "Review lint Passes" | `lint.passes`, with `lint.errors` when it does not |
| "Raised before on this PR None" | `lint.raised_before_on_pr` |
| "Your edits to this draft" | `edits[]` |
| "Cut F6 · rename suggestion" | `edits[].kind`, `.finding_key`, `.summary` |
| "Held back · 2 nits" | length of `held_back` |
| "F4 · Backoff schedule repeated in two files" | `held_back[].finding_key`, `.claim` |
| the "Post as" options | the `ReviewEvent` enum — `comment`, `request_changes`, `approve` |
| whether the approve controls are live | `draft.status` |
| that a ticket has already been asked for | `nit_ticket.status`, `.ticket_id` |

The example body carries a mermaid fence and a `<details>` fold, which is what DEV-855's
"renders a mermaid diagram and a folded section from sample markdown" needs to work
against.

### Rules — `GET /rules?repo=`, example `examples/listRules.response.json`

| On the mockup | Field |
|---|---|
| the repository picker's options | `GET /repos` → `repos[].repo`; "All repositories" is `GET /rules` with no `repo` |
| "Active rules 14" | `summary.active_rules` |
| "cap of 20 per repository" | `summary.active_rules_cap` |
| "Same answer on re-run 9 of 10" | `summary.replay_matched`, `.replay_total` |
| "Findings you kept 31 of 40" | `summary.findings_kept`, `.findings_total` |
| "last 30 days, draft to posted" | `summary.findings_window_days` |
| "Raised twice on one PR 2" | `summary.raised_twice_on_one_pr` |
| "Proposed · 1" | length of `proposed` |
| "Why proposed: …" | `proposed[].why_proposed` |
| "Would suppress, not delete…" | `proposed[].note` |
| "R-09" | `active[].key` |
| the rule's sentence | `active[].statement` |
| "Prevents: …" | `active[].prevents` |
| "Decided on #377" | `active[].origin_pr`, linked by `.origin_review_id` |
| "cited by 6 later findings" | `active[].counts.cited_by_findings` |
| "Raised 7" | `active[].counts.raised` |
| "Kept by you 7" | `active[].counts.kept` |
| "Rejected with a reason 0" | `active[].counts.rejected_with_reason` |
| "Caught by the check 9" | `active[].counts.caught_by_check` |
| "Raised in a review since 0" | `active[].counts.raised_in_a_review_since` |
| "Stage: Repository rule" / "CI check" | `active[].stage_label`, from `.stage` |
| "· 1 exception" | `active[].counts.exceptions` |
| "Checked without a model · last said by a human reviewer 19 days ago" | `active[].note`, `.counts.last_human_mention_at` |
| "author disagreed once, with a reason you accepted…" | `active[].note` |
| "Going stale · 1" | length of `stale` |
| "No finding has cited it in 60 days…" | `stale[].note`, `.last_cited_at`, `.stale` |

A rule's count set differs by stage — a repository rule shows raised, kept and rejected; a
CI check shows what the check caught. Every field of `RuleCounts` is therefore optional,
and the screen renders the ones that are present.

## Every button

| Screen | Control | Endpoint |
|---|---|---|
| Inbox | "Understand it first" | `POST /reviews`, sending only `pr_url` — the screen has no head commit, so the server resolves one |
| Inbox | "Read brief", "Open draft", "See reply", "Decide" | none — links to another screen |
| Inbox | "Request review" | **none, deliberately** — see below |
| One review | "Open draft" | none — a link |
| Draft preview | "Rendered", "Markdown", "Edit" | none — view toggles over `draft.body` |
| Draft preview | "Back to the review" | none — a link |
| Draft preview | "File as one ticket for an agent" | `POST /reviews/{reviewId}/held-back/ticket` |
| Draft preview | "Approve and post" | `POST /reviews/{reviewId}/draft/approve` |
| Draft preview | "Send back with a note" | `POST /reviews/{reviewId}/draft/send-back` |
| Rules | repository picker | `GET /rules?repo=` |
| Rules | "Promote" | `POST /rules/{ruleId}/promote` |
| Rules | "Dismiss" | `POST /rules/{ruleId}/dismiss` |
| Rules | "Retire" | `POST /rules/{ruleId}/retire` |
| Rules | "Keep 60 more days" | `POST /rules/{ruleId}/defer` |

**"Request review" has no endpoint and should not get one.** It asks GitHub to add a
reviewer to a pull request — a *write* to another system's record. `POST /reviews` reads
GitHub, so the objection is not that GitHub is out of reach; it is that DEV-854 forbids
this API writing there, and the brief gives the reason: Review Desk owns the record, and
the review session performs the outward act. The button belongs to whatever drives the
session, not to this API. Until that exists, the dashboard should either link out to the
pull request or disable the button.

**"Approve and post" does not post.** It sets `draft.status` to `approved`, stores the
chosen event and `approved_at`, and stops. The review session finds the approved draft and
posts it; it then records the GitHub review id through DEV-851's `review link
--github-review`, which is what moves the status to `posted`. How the session learns of
the approval is an open question in the project brief, and nothing in this contract depends
on the answer.

**Two writes this contract defines have no control on the mockups**: confirming a brief
and changing a finding's disposition. DEV-854 requires both, and DEV-855's acceptance
criteria require confirming a brief to remove it from the Inbox, so the endpoints are
here. The One review mockup has no button for either — see "What looks wrong" below.

## The draft record

**The merged store has no table for the draft or its approval.** This is the
specification; the migration is a later issue. One draft per review, replaced in place.

| Field | Type | Holds |
|---|---|---|
| `review_id` | integer, primary key | The review it belongs to. One draft per review. |
| `body` | text | The review body as GitHub-flavoured markdown, posted verbatim. May be empty while `drafting`. |
| `status` | text | `drafting`, `ready`, `approved`, `sent_back`, `posted`. |
| `event` | text, nullable | The GitHub review event chosen at approval: `comment`, `request_changes`, `approve`. Null until approved. |
| `approved_at` | integer, nullable | When the developer approved it. |
| `sent_back_note` | text, nullable | The developer's instruction when the draft was returned. |
| `sent_back_at` | integer, nullable | When it was returned. |
| `posted_at` | integer, nullable | When the session reported it posted. Set with the review's `github_review_id`, never by this API acting on GitHub. |
| `lint_passes` | integer, nullable | Whether the review-craft lint reported no errors. |
| `lint_visible_words` | integer, nullable | For "118 of 150". |
| `lint_word_budget` | integer, nullable | The budget for this change's size. |
| `lint_findings_in_body` | integer, nullable | Findings the body actually raises. |
| `lint_findings_expected` | integer, nullable | Findings at disposition `in_draft`. A mismatch means one was dropped silently. |
| `lint_raised_before_on_pr` | integer, nullable | Findings already raised in an earlier round on the same pull request. Non-zero is a review going in a circle. |
| `lint_errors` | text, nullable | The lint's error lines, verbatim. |
| `lint_checked_at` | integer, nullable | When the lint last ran. |
| `updated_at` | integer | Last change. |

The status transitions, and nothing else:

```
drafting ──► ready ──► approved ──► posted
                │  ▲
                ▼  │
             sent_back
```

- `drafting → ready` when the session finishes writing. The review then appears in the
  Inbox's "Approve a draft".
- `ready → approved` is `POST …/draft/approve`, which also sets `event` and `approved_at`.
- `ready → sent_back` is `POST …/draft/send-back`, which stores the note.
- `sent_back → ready` when the session has reworked it.
- `approved → posted` when the session reports the GitHub review id. **Only this
  transition involves GitHub, and the session makes it, not the API.**
- `PUT …/draft` is refused once the status is `approved` or `posted`: an approved draft is
  what the session reads, and editing it underneath would change what gets posted.

**The developer's edits are not stored on the draft.** They are `Outcome` rows with actor
`developer` and kind `cut`, `regraded` or `rewritten`, which is where PR #1 already puts
them, and they are the strongest signal in the feedback loop. `PUT …/draft` takes them in
its body rather than inferring them from a text diff, because "regraded F3 from Blocker to
Should" is not recoverable from the characters that changed.

### Where a review's per-layer status lives

**A new `review_layer` table: `(review_id, number)` as the key, plus `state`, `detail` and
`updated_at`.** Seven rows per review, written by the layer that ran.

Stored rather than derived, deliberately. Most of it looks derivable — layer 2 from
`brief.confirmed_at`, layer 3 from the option count, layer 5 from the finding count, layer
6 from the draft status — but two things break that. Layer 1's lint result has nowhere to
be derived from at all, and a derived status cannot tell a layer that was *skipped* from
one that has not run, which is exactly the distinction "what was not checked" exists to
make. The brief also says each layer writes one thing to the record, and this is layer 1's
and layer 4's one thing.

### Where the held-back nits' ticket id lives

**On the review: `nit_ticket_status`, `nit_ticket_id`, `nit_ticket_requested_at`,
`nit_ticket_filed_at`.** One ticket per review covering all of that review's held-back
nits, which is what the mockup's single "File as one ticket for an agent" button implies.

`nit_ticket_id` is a `ForeignId` — a Linear ticket, opaque. The findings the ticket covers
are not stored against it; they are every finding on the review at disposition
`held_back`, which the API returns as `nit_ticket.finding_seqs`.

### What migration 6 added, for a finding's identity across rounds

Eight columns on `findings` and two tables. DEV-873.

| Column on `findings` | Holds |
|---|---|
| `continues_review_id` | The earlier round this finding was carried from. The number is not repeated beside it, because it cannot differ: a carried finding keeps its number, so this plus `seq` names the finding it continues. SQLite cannot add a composite foreign key by `ALTER TABLE` either way. |
| `round_status` | `new`, `fixed`, `still_open`, `accepted`, `withdrawn`, `ticketed`. `NOT NULL DEFAULT 'new'`, which is the honest value for every finding already on the record. |
| `round_status_by` | `developer`, `agent` or `author` — a `DecidedBy`, not an `Actor`. |
| `round_status_checked` | How it was checked, in the checker's own words. |
| `round_status_at` | When. |
| `round_status_commit` | The commit the status rests on: for a fixed finding, the one that fixed it. |
| `introduced_by_commit` | The commit a new finding is attributed to. |
| `introduced_by_seq` | The earlier finding whose fix caused it, by number. A number alone, because after this migration a number is an identity across the pull request. |

| Table | Holds |
|---|---|
| `round_commits` | `(review_id, sha)` plus `position`, `summary`, `committed_at`. Stored against the round that reviewed them. `position` is GitHub's order rather than a sort on `committed_at`, because a rebased commit can carry a date out of order. |
| `round_commit_files` | `(review_id, sha, path)`. A table rather than a joined string, because "was this finding's file changed since round 1" is a lookup rather than a substring search — it is what `late` is answered from. |

The closed sets `RoundStatus` and `DecidedBy` are `string_enum!` sets in
`crates/store/src/model.rs`, so they name their own valid values in a refusal, as the note
under "Not a store change" asks of the others.

**Writing all of it is the command line's**, and none of it signs anything off:
`review-desk finding carry` records what became of a carried finding,
`review-desk finding became` changes it, `review-desk finding add` takes a new finding's
attribution, `review-desk round commits` reads GitHub and records the commits, and
`review-desk round list` reads the timeline back. `review-desk review open` hands back the
previous round's findings, ready to be carried.

## Store changes this contract needs beyond the merged store

Read against `crates/store/src/model.rs` at `e61137c` on `dev-850-scaffold-store`, then
re-checked against `master` at `5c7b670` after #1 and #2 merged: `model.rs` gained one
error branch and no public function in `entities/` changed, so every item below still
stands. Each item is one migration or one query; a later issue can be written from this
list without re-deriving it.

### New tables

- [ ] **S1 — `draft`.** The table specified above. Needed by: `getDraft`, `saveDraft`,
      `approveDraft`, `sendBackDraft`, and the Inbox's `drafts_to_approve` group.
- [ ] **S2 — `review_layer`.** Seven rows per review: `state`, `detail`, `updated_at`.
      Needed by: `getReview.layers`, and the Inbox's `in_progress` group.
- [ ] **S3 — `brief_option`.** `(review_id, key)` plus `title`, `argument_for`,
      `argument_against`, `chosen`. Replaces `Brief::option_map`, which is one opaque
      `String` today and cannot serve the option cards on the One review screen. Needed
      by: `getReview.brief.options`.
- [ ] **S4 — `rule_exception`.** `rule_id`, `reason`, `created_at`. Needed by:
      `Rule.counts.exceptions` and the "· 1 exception" in `stage_label`.

### New columns on existing tables

- [ ] **S5 — `review.title`.** The pull request's title. On every screen; the store has no
      column for it.
- [ ] **S6 — `review.ticket_id`.** The Linear ticket, opaque. DEV-854 lists the Linear
      ticket among the foreign ids; `Review` in PR #1 carries `session_id`, `ledger_id` and
      `github_review_id` but not this one.
- [ ] **S7 — `review.changed_lines`.** Additions plus deletions. Shown on One review, and
      needed to compute the draft's word budget.
- [ ] **S8 — `review.not_checked`.** What the review did not look at. Shown on One review
      and in the draft body.
- [ ] **S9 — `review.nit_ticket_status`, `nit_ticket_id`, `nit_ticket_requested_at`,
      `nit_ticket_filed_at`.** Specified above.
- [ ] **S10 — `brief.provenance`.** Where the problem statement came from and what the
      developer changed — "from the ticket and the code · you corrected one sentence".
- [ ] **S11 — `finding.rule_id`.** The rule a finding was raised under. The store records
      `Rule::origin_findings` — rule to finding — but not finding to rule, so "cited by 6
      later findings" and a rule's staleness cannot be answered. This is the single most
      load-bearing missing column: the whole feedback loop runs through it.
- [ ] **S12 — `rule.last_cited_at`.** Derivable from S11 once it exists; a stored column
      avoids a scan on every Rules load. Either is acceptable, but the choice must be made.
- [ ] **S13 — `rule.review_again_at`.** Set by "Keep 60 more days". Without it, a deferred
      rule reappears as stale on the next load.
- [ ] **S14 — `rule.why_proposed` and `rule.note`.** The proposal's justification and the
      free line under the statement. Both are shown on the Rules screen and in the Inbox.
- [ ] **S15 — `rule.updated_at`.** `Rule` has `created_at` only, so a promotion, dismissal
      or deferral leaves no trace of when it happened.
- [ ] **S16 — a reason on a rule's stage change.** `set_rule_stage` takes a stage and no
      reason, so "Dismiss" and "Retire" cannot record why. A dismissed proposal with no
      reason gets proposed again next week.

- [x] **S22 — a GitHub read path for the server, for `POST /reviews`.** It needs a pull
      request's head commit, title and changed-line count from a link. `crates/harvest`
      (DEV-859, merged as pull request #6) is that path: read-only, by running `gh`, with
      GitHub behind a four-method trait that has no write in it.

      **This item claimed the harvester already provided all three, and it did not.** Its
      `GitHub` trait returned a `PullRequest` carrying `state`, `state_at`, `head_sha` and
      `author` — the four facts the *harvester* needs — and no title or line count. So when
      DEV-861 built `POST /reviews` against it, the head commit resolved and the other two
      could not: the server wrote `title: null` for every review started from the Inbox,
      and `crates/server/src/api.rs` carried a comment saying this item was wrong rather
      than the item being corrected. A review with no title has nothing for the Inbox and
      the One review screen to list it by, which is every screen it appears on.

      **DEV-862 closed it by widening the read.** `PullRequest` now also carries `title`
      and `changed_lines` — additions plus deletions, as GitHub reports them on the same
      response the head commit comes from — and `POST /reviews` stores both for a caller
      that supplied neither. One `gh api repos/OWNER/REPO/pulls/N`, three facts, still a
      read. The recorded response in `crates/harvest/tests/fixtures/pull_request.json` was
      re-captured from `devergehq/locus#64` with the new fields rather than having them
      written in by hand, so the parsing is still tested against what GitHub actually
      sent.

      This also answers who supplies S5 (`review.title`) and S7 (`review.changed_lines`).

### New queries on the store crate

- [ ] **S17 — cross-repository review queries.** The Inbox needs every review waiting on
      the developer across all repositories. The store offers `find_review`,
      `find_review_by_head`, `latest_round` and `list_reviews(repo, pr)` — all scoped to
      one pull request. Needed by: every group in `getInbox`.
- [ ] **S18 — the repository list.** Distinct repositories across reviews and
      repository-scoped rules, with counts. Needed by: `listRepos`.
- [ ] **S19 — rule counts.** `raised`, `kept`, `rejected_with_reason` and
      `cited_by_findings` aggregated from findings and outcomes. Needs S11.
- [ ] **S20 — the Rules summary figures.** `findings_kept` / `findings_total` over a
      window, and `raised_twice_on_one_pr` from `raised_again` outcomes.
- [ ] **S21 — a repository-scoped rule query that includes proposals.** `rules_for_repo`
      filters to `active`, `lint` and `ci`, which is right for `rules list` in DEV-851 and
      wrong for this screen, whose purpose is deciding on proposals. `list_rules` returns
      every stage but is not scoped to a repository, so neither function serves
      `listRules`. Add a stage filter argument rather than changing either default.

### Not a store change, but do not miss it

- **Times.** The store holds `Timestamp = i64`, epoch seconds. The contract is RFC 3339.
  The conversion belongs in the server at this boundary; no migration is needed. It is
  listed because it is the one mismatch that will look like a bug the first time somebody
  serialises a `Review` straight out of the store crate.
- **`Severity`, `Disposition`, `OutcomeKind`, `Actor`, `RuleStage`** are the same five
  closed sets as `model.rs`, with the same wire strings. `DraftStatus`, `ReviewEvent`,
  `NitTicketStatus`, `ReviewLayerName` and `ReviewLayerState` are new and should be added
  as `string_enum!` sets so they keep naming their own valid values in errors.

### Facts on the mockups that no store change can supply

These are not migrations. Each one needs a source Review Desk has not got, and no filed
issue gives it one. Every field is declared nullable, and the dashboard must render the
null.

| Fact | Where | Who owns it |
|---|---|---|
| "Your PRs with no reviewer", and those PRs' description lint | Inbox | GitHub. Reachable in principle — see "Reading GitHub" below — but no endpoint or issue covers listing the developer's own pull requests. |
| "session alive" | Inbox, in progress | Allele. The store holds `session_id` and nothing about liveness. |
| "9 description problems caught by the CI check" | Inbox | A CI run. The brief puts `pr_lint.py` in CI explicitly outside this project. |
| "Caught by the check", "Raised in a review since" | Rules | The same CI source. |
| "last said by a human reviewer 19 days ago" | Rules | A GitHub harvest. DEV-859 harvests four kinds of outcome; this is not one of them. |
| "Same answer on re-run 9 of 10" | Rules | The replay consistency check (slice 5, not filed). |

## Reading GitHub

**One operation reads GitHub: `POST /reviews`, and only to resolve a head commit the
caller did not supply.** Nothing in this contract writes to GitHub.

The Inbox's "Start a review" has a pasted pull request link and nothing else, so
`head_sha` is optional on `StartReviewRequest`. When it is omitted the server reads the
pull request and resolves the head commit, the title and the changed-line count, then
creates the review.

**The head is resolved before the review exists, not afterwards.** The alternative — create
the review now, let a review session fill the head in later — was considered and rejected:

- The store finds an existing review by repository, number and head commit
  (`find_review_by_head`), and DEV-851 requires `review open` called twice with the same
  head to return the same review. A review with no head cannot be matched, so a second
  click of "Understand it first" would create a second review.
- `round` is "which round of review this is on that pull request". Which round a commit is
  depends on the commit, so a round assigned before the head is known can turn out to be a
  second round on the first round's commit.
- It would need a nullable column, a new state meaning "head not yet known", and a write to
  resolve it. Resolving at creation needs no store change at all.

**How the server reads it.** By running `gh`, as `crates/harvest/src/gh.rs` does: Review
Desk handles no tokens, uses whatever login the developer already has, and reads only. That
crate's `GitHub` trait returns a `PullRequest`, which since DEV-862 carries the title and
the changed-line count beside the head commit — one response, three facts. **This contract
names no crate**; how the server reads is the server's business. See S22, which was wrong
about this until DEV-862 and says how.

**When the read fails**, `POST /reviews` returns `github_unavailable` and creates nothing.
That code is separate from `store_unavailable` because it is the developer's environment
rather than their request or a broken Review Desk: `gh` missing, or not logged in. The
screen can then say which, and offer the link again.

## What looks wrong

Recorded here rather than acted on, because DEV-854 says to specify and not to implement.

1. **The One review mockup has no control for either write DEV-854 assigns to that
   screen.** Confirming a brief and changing a finding's disposition both need a button
   that is not drawn. The Inbox's "Read brief" links to this screen, so the confirmation
   has to happen here. DEV-855's acceptance criteria require confirming a brief to work.
2. **DEV-854's screen table gives the Inbox no writes**, but the Inbox mockup has "Start
   a review" with a button, and DEV-855 requires it. The endpoint exists here.
3. **"Request review" cannot be built as drawn** without the API *writing* to GitHub,
   which DEV-854 forbids. Reading is now allowed and does not help here: adding a
   reviewer is a write.
4. **The merged `Rule` cannot support the Rules screen** as drawn: no counts, no
   staleness, no reason on a stage change, and no finding-to-rule citation. S11–S16 and
   S19 are the gap. The brief says "Rules carry counts and go stale when nothing cites
   them", so this is a gap against the brief, not only against the mockups.
5. **`Brief::option_map` as one `String`** cannot serve the option cards. S3.
6. **I had this wrong, and it is recorded rather than quietly fixed.** Earlier versions
   of this README said "Review Desk does not read GitHub, by design" and required
   `head_sha` on `POST /reviews`, which made the Inbox's "Start a review" unbuildable —
   the screen has only a link. That claim was an inference from the brief's "one owner per
   fact", not a rule anyone wrote: DEV-854 forbids the API **posting** to GitHub, and
   DEV-859 reads it. Found by the DEV-855 session, 8 October 2026.
7. **Six figures on the mockups have no data source at all** — the table above. The
   Inbox and Rules screens will have visible holes on first run, and the brief's "absent
   is silent, broken is loud" suggests they should read as absent rather than as zero.
8. **Board A3's "Touches F1, F6" column against each commit cannot be derived from what
   DEV-873 asks for.** Two of the three commits on the board are matched to a finding by
   attribution — `round_status_commit` and `introduced_by_commit` — and the third, which
   only adds a test, is shown against F3 by nothing the record holds. The page can match
   `commits_since_previous_round[].files` against `findings[].file`, which is what the
   `late` flag does; that does not produce the board's third row. Recorded rather than
   invented: DEV-873's own table asks for "id, a one-line summary, files touched" and
   nothing more.
9. **Board A3's "Across both rounds" panel — raised, fixed, accepted or ticketed, open,
   introduced by a fix — is five pull-request-wide totals that DEV-873 does not ask for.**
   Four of the five are summable from `rounds[].statuses`; "introduced by a fix" is not,
   because `rounds[].statuses` counts statuses and not attributions. Left out rather than
   guessed at, and it overlaps DEV-875.
10. **A fixed finding keeps whatever disposition it was carried with.** `lint.findings_expected`
   counts findings at `in_draft`, so a fixed finding carried as `in_draft` is still expected
   in the draft body — and the board says "Fixed · 2 · nothing to post". Nothing in DEV-873
   decides a disposition from a status, deliberately: that is the draft's composition, and
   an agent sets it with `finding set`. Worth a line in whichever issue owns the draft.
   `RoundStatus`'s own description in `openapi.yaml` claimed a fixed finding was "absent
   from the draft", which was this gap written up as though it were closed; corrected.
