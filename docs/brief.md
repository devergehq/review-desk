# Review Desk — project brief

This is the design record for Review Desk. It was first written on 8 October 2026 as the Linear project brief; it lives here because agent sessions could not read it there. **When this file and the Linear project disagree, this file is right.** Change it by pull request.

## What this is

**Review Desk is a standalone local app that owns the record of a code review** — the problem brief, the options considered, the findings, what happened to each finding, and the rules learned from them — and shows the developer what is waiting on them.

It is not a list of pull requests. GitHub and Linear already do that. **Its unit is the finding and the rule, not the PR.**

## Why it exists

- Review volume is high and reviews are getting repetitive and going in circles.
- The same findings recur, and nothing stops a human reviewer saying them again.
- Nits get in the way of progress and generate more nits.
- Nothing structured records what a review found or what became of it, so the review method never learns.

## What Patrick wants from it

| Wanted | Not wanted |
|---|---|
| Consistency, correctness, logical decisions | Preference-based review |
| A preview of the review before anything is posted | Anything posting unread in his name |
| Rules justified by the failure they prevent | A banner or boilerplate on every review |
| A review driven locally by the developer | A remote bot that reviews first |

## Decisions

| Decision | Date | By |
|---|---|---|
| Standalone app, not an Allele feature. Prove it first; decide later whether it moves into Allele or stays its own product. | 8 Oct 2026 | Patrick |
| Named Review Desk | 8 Oct 2026 | Patrick |
| Rust for the store, the command line and the local server; one binary; SQLite | 8 Oct 2026 | Proposed by Claude, confirmed by Patrick |
| The interface is a browser app in Vue, built with Nuxt in static mode and embedded in the binary. Nuxt's server features are not used. | 8 Oct 2026 | Patrick |
| **Only the developer can sign off.** Confirming a brief and signing off a draft happen in the dashboard only. The command line that agents use has no command for either. | 9 Oct 2026 | Patrick |
| **One exception, and it is narrow.** A brief may be written as needing no confirmation, with the reason. A brief is confirmed on pull requests changing more than 200 lines; the threshold is the caller's setting, not Review Desk's. A brief already written as requiring confirmation cannot be rewritten as needing none, and the record says `waived` rather than `developer` wherever it is read. | 9 Oct 2026 | Patrick (DEV-864) |
| Review Desk reads GitHub (through `gh`, under the developer's login) and never writes to it | 8 Oct 2026 | Settled in the API contract |
| **Dependencies must be active, maintained and popular, and that is checked before adopting one.** An update bot, an advisory scan and a release-age check enforce it; see [dependencies.md](dependencies.md). | 9 Oct 2026 | Patrick |
| **On screen, "approve" is the GitHub review event and nothing else.** What the developer does is **sign off**: the draft page's button reads "Sign off and post", the Inbox group reads "Sign off a draft", and the three event options keep GitHub's names. The API, the contract, the store and the command line are unchanged — `approveDraft`, the `approved` status and `draft_approved` keep their names. | 9 Oct 2026 | Patrick (DEV-870) |

## The shape of the app

**Review Desk is a web app served from a local Rust binary.** The developer runs `review-desk serve`, it serves a page on `127.0.0.1`, and they use it in a browser.

| Part | Where | Does |
|---|---|---|
| Store crate | `crates/store` | The only code that opens the SQLite file |
| Command line | `crates/cli` | What agent sessions call to write and read the record |
| Harvester | `crates/harvest` | Reads outcomes back from GitHub |
| Local HTTP server | `crates/server` | Serves the JSON API in `contract/` and the embedded front end |
| Front end | `web/` | The dashboard: inbox, one review, draft preview, rules |
| API contract | `contract/` | The OpenAPI file and examples both sides are built to |

## How the pieces fit

**One owner per fact.** Review Desk copies nothing from the other systems; it stores their ids.

| Fact | Owner |
|---|---|
| Briefs, findings, outcomes, rules, drafts | Review Desk |
| Sessions and workspaces | Allele |
| Who is working on which item | Locus dispatcher ledger |
| The posted review | GitHub |
| Tickets | Linear |

**Three rules of interaction:**

1. **One way in.** Everything writes through the command line or the local HTTP API. Nothing else opens the database file.
2. **One direction of dependency.** Review Desk needs nothing else installed. Locus calls it when it is configured. Allele may link to it.
3. **Absent is silent, broken is loud.** Not configured: callers behave as they do today. Configured but unreachable: a visible warning that never blocks a review.

**Two surfaces, two audiences:**

| Surface | Used by | May do |
|---|---|---|
| Command line | Agent sessions | Record a review: open it, write the brief, findings, layer status and the draft; read rules. **Never sign off.** |
| Dashboard and its API | The developer | Everything the developer decides: confirm a brief, edit and approve a draft, send it back, change a finding's disposition, promote or retire a rule |

**Posting.** The developer approves a draft in the dashboard. That changes the draft's status. The review session reads the approved draft and posts it to GitHub. Review Desk itself does not post.

## The seven layers of a review

Each layer writes one thing to the record. The developer acts at layers 2 and 6.

| # | Layer | Run by | Writes |
|---|---|---|---|
| 1 | Lint | Automatic | Lint result |
| 2 | Understand | Agent, then the developer confirms | Brief |
| 3 | Alternatives, written before the diff is read | Agent | Option map |
| 4 | Solution map | Agent | Approach verdict |
| 5 | Code review | Agent | Findings with evidence |
| 6 | Triage and draft | Agent, then the developer approves | Draft |
| 7 | Harvest | Automatic | Outcomes and proposed rules |

## The feedback loop

Signals, strongest first:

1. The developer's own edits between draft and post — cut, regraded, rewritten.
2. An author's rejection with a reason.
3. The same finding raised again in a later round.
4. Code changed after the finding.
5. Acceptance and reactions — weak, because an authoring agent complies with almost anything.

**Rules are proposed by the app and promoted by the developer. The app never promotes one itself.** A rule graduates: observation → repository rule → lint check → CI check. Rules carry counts and go stale when nothing cites them.

## Build order

State on 9 October 2026.

| Slice | What | State |
|---|---|---|
| 1 | Store, command line | Merged (DEV-850, DEV-851) |
| — | API contract | Merged (DEV-854) |
| — | Harvester | Merged (DEV-859) |
| — | Dashboard screens on sample data | Merged (DEV-855) |
| 2 | Store changes the contract needs; draft and layer commands; sign-off removed from the command line | In progress |
| 3 | Local HTTP server, serving the API and the embedded front end | In progress |
| 4 | Front end switched from sample data to the API | Not started |
| 5 | Locus's review worker writes to Review Desk when it is configured | Not started — lives in the Locus repository |
| 6 | Approval reaches the review session that posts | Not started |
| 7 | Rule proposals and the replay consistency check | Not started |

## Open questions

- **Approval delivery.** How an approved draft reaches the review session that posts it. The Locus dispatcher's watcher is the candidate.
- **Sharing.** How much is shared with a team. Proposed: the baseline lives in CI, and personal rules export as a plain file in the repository.
- **Review Center.** Allele has a planned project of that name that overlaps with the dashboard. If Review Desk earns its place, Review Center becomes a view onto this store.
