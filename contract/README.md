# The Review Desk local API contract

This directory is the agreement between the local Rust server and the Nuxt dashboard. It
holds no implementation of either.

| File | What it is |
|---|---|
| `openapi.yaml` | OpenAPI 3.1 for the local JSON API. 16 operations. |
| `examples/` | One example response per endpoint, one request body per write, and the error shape. |
| `validate-examples.mjs` | Checks every example against its schema, and fails when an endpoint has no example. |
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
| Nothing posts to GitHub | No operation calls GitHub. Approving a draft sets a status; the review session reads it and posts. |
| Review Desk's own ids are integers | Review id, finding `seq` and rule id, matching `crates/store/src/model.rs`. |
| Null is not zero | A count the store cannot yet answer is `null`. Zero is a measurement; null is the absence of one. |

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
| the "Not checked" paragraph | `not_checked` |
| that an "Open draft" link is shown at all | `draft_status` |

`finding_severities` is also served, for a caller that wants the Inbox's "1 blocker, 2
should" wording on this screen.

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
| Inbox | "Understand it first" | `POST /reviews` |
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
reviewer to a pull request, which is an outward action on another system's record. DEV-854
forbids the API posting to GitHub, and the brief gives the same reason: Review Desk owns
the record, and the review session performs the outward act. The button belongs to
whatever drives the session, not to this API. Until that exists, the dashboard should
either link out to the pull request or disable the button.

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
| "Your PRs with no reviewer", and those PRs' description lint | Inbox | GitHub. Review Desk does not read GitHub, by design. |
| "session alive" | Inbox, in progress | Allele. The store holds `session_id` and nothing about liveness. |
| "9 description problems caught by the CI check" | Inbox | A CI run. The brief puts `pr_lint.py` in CI explicitly outside this project. |
| "Caught by the check", "Raised in a review since" | Rules | The same CI source. |
| "last said by a human reviewer 19 days ago" | Rules | A GitHub harvest that does not exist yet (slice 3, not filed). |
| "Same answer on re-run 9 of 10" | Rules | The replay consistency check (slice 5, not filed). |

## What looks wrong

Recorded here rather than acted on, because DEV-854 says to specify and not to implement.

1. **The One review mockup has no control for either write DEV-854 assigns to that
   screen.** Confirming a brief and changing a finding's disposition both need a button
   that is not drawn. The Inbox's "Read brief" links to this screen, so the confirmation
   has to happen here. DEV-855's acceptance criteria require confirming a brief to work.
2. **DEV-854's screen table gives the Inbox no writes**, but the Inbox mockup has "Start
   a review" with a button, and DEV-855 requires it. The endpoint exists here.
3. **"Request review" cannot be built as drawn** without something that acts on GitHub,
   which both DEV-854 and the brief forbid this API.
4. **The merged `Rule` cannot support the Rules screen** as drawn: no counts, no
   staleness, no reason on a stage change, and no finding-to-rule citation. S11–S16 and
   S19 are the gap. The brief says "Rules carry counts and go stale when nothing cites
   them", so this is a gap against the brief, not only against the mockups.
5. **`Brief::option_map` as one `String`** cannot serve the option cards. S3.
6. **Six figures on the mockups have no data source at all** — the table above. The
   Inbox and Rules screens will have visible holes on first run, and the brief's "absent
   is silent, broken is loud" suggests they should read as absent rather than as zero.
