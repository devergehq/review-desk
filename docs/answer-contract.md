# The answer contract

**What a review's session sends back when it answers in the conversation, and what Review Desk
does with it.** Drawn as screen S7 (`docs/mockups/s-structured/S07-answer-contract.webp`), agreed
by Patrick on 21 November 2026. Patrick, 15 November, on why it exists: so the panel *"just
doesn't have this just walls of text… make sure the formatting is correct. It doesn't run away
from us."*

**The session proposes; the developer decides.** Nothing in an answer changes the record until the
developer accepts it, and an answer that does not parse is never kept.

## Where it travels

The conversation already exists (DEV-1141, README *Talking to the review's session*): the
developer asks from the page, a session reads, marks read and replies with
`review-desk message reply`. **The contract is the shape of that reply's body.** Today the body is
free prose; under the contract it is one JSON object, and the server parses it before it is kept.

The rules that already hold keep holding:

- **No command writes the developer's words.** The optional `asked` field below is the session's
  own account of what it was asked, drawn as the session's (*"Your question, as your session
  recorded it"*), never stored as a developer message.
- **No answer signs anything off.** Accepting a suggestion records a decision the developer could
  already make on the page; approving, confirming and posting are not suggestions.
- **A reply is data.** It is interpolated, never rendered as markup.

## The fields

| Field | Required | Limit | What it is |
| -- | -- | -- | -- |
| `kind` | always | `answer` · `working` · `cant` | `cant` says why, and what would let it answer |
| `headline` | always | 1 sentence, ≤140 characters | The answer in one line. Always drawn |
| `points` | no | 0–5 items, each ≤200 characters | Each point carries a `basis` |
| `points[].basis` | per point | `ran` · `read` · `told` | How the point is known |
| `cites` | ≥1 when the answer claims anything | strings | `F9`, `path/to/file.php:41`, `criterion 3`, `option C`, a commit SHA |
| `not_sure` | no | 0–1, ≤200 characters | What it couldn't check, said plainly |
| `suggests` | no | 0–3 items, from the list below | Actions the developer can accept with one click |
| `activity` | while `kind` is `working` | ≤6 steps, each ≤60 characters | What it is doing now |
| `about` | no | a step, a finding, or a reply | What the answer is about, so the page can link it |
| `asked` | no | ≤200 characters | One-way mode only: the question as the session recorded it |

Severity words are **blocker, should, question, nit**. Basis words are **ran, read, told**.

## What it may suggest, and only these

| `type` | Carries | Accepting it | Key on the page |
| -- | -- | -- | -- |
| `new_finding` | severity, title, where, basis | adds a finding to *To post* | — |
| `regrade` | finding, from, to, why | regrades the finding | `G` |
| `add_to_draft` | finding, or one sentence | edits the finding's wording | `E` |
| `set_aside` | finding, why | sets the finding aside | `S` |
| `answer_question` | which of the five questions, the answer | ticks it on Overview | — |
| `draft_reply` | finding, `R`/`A`/`W`, the words | fills the reply on Replies | `R` `A` `W` |
| `follow_up` | one question | fills the ask box | — |

A suggestion of any other type makes the whole answer not parse.

## When it doesn't fit

**An answer that doesn't parse is drawn folded and inert:** the first ~200 characters, a
*Show the rest (n words)* fold, the line *"No cites and no basis, so no actions are offered and it
isn't kept with the review"*, and one button, **Ask it again, in the format**. It is kept only as
the raw message, flagged as not parsed; it never reaches a finding, a draft or a count.

## Example

```json
{
  "kind": "answer",
  "about": { "finding": "F9" },
  "headline": "Yes, the web path has the same hole: it calls the same policy with the same empty list.",
  "points": [
    { "basis": "ran",  "text": "A web export of a definition with no columns returned all 14 columns." },
    { "basis": "read", "text": "ExportController.php:41 passes the definition straight through." }
  ],
  "cites": ["F9", "app/Http/Controllers/ExportController.php:41", "criterion 3"],
  "not_sure": "Didn't check the CSV download button, which may use a third path.",
  "suggests": [
    { "type": "regrade", "finding": "F9", "from": "should", "to": "blocker",
      "why": "Both paths leak private columns." }
  ]
}
```

## Who checks it

**The server**, on `message reply`, before it is kept; the CLI prints the first failing rule and
exits non-zero so the session can resend. The page never re-validates and never repairs. The
session is told the contract with every question it is asked (the watcher includes this file's
*fields* and *suggests* tables in the delivery).
