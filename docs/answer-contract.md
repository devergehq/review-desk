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
| `about` | no | exactly one of `{"step": …}`, `{"finding": "F9"}`, `{"reply": "F3"}` | What the answer is about, so the page can link it. `step` takes a tab (`overview`, `understanding`, `findings`, `draft`, `replies`) or one of the seven step names |
| `asked` | no | ≤200 characters | One-way mode only: the question as the session recorded it |

Severity words are **blocker, should, question, nit**. Basis words are **ran, read, told**.

**How the limits are read.** A length is in characters, not bytes. "1 sentence" is checked as
one line: whether a line is one sentence is not something a parser can say without guessing at
the full stop in `ExportController.php`. A field the table does not name is refused. **An
answer claims something** when its `kind` is `answer`, or when any reply makes a point or
suggests an action; that is when it must cite. `activity` is required on a `working` reply and
refused on the other two. Cites are checked for being strings that say something, and **never
resolved** against the record: a SHA, `criterion 3` or a finding from another round all pass.

## What it may suggest, and only these

| `type` | Carries | Accepting it | Key on the page |
| -- | -- | -- | -- |
| `new_finding` | `severity`, `title` (≤100), `where` (≤200), `basis` | adds a finding to *To post* | — |
| `regrade` | `finding`, `from`, `to` (not `from`), `why` (≤200) | regrades the finding | `G` |
| `add_to_draft` | `finding`, or `text` (one sentence, ≤200), or both | edits the finding's wording | `E` |
| `set_aside` | `finding`, `why` (≤200) | sets the finding aside | `S` |
| `answer_question` | `question` (1–5), `answer` (≤200) | ticks it on Overview | — |
| `draft_reply` | `finding`, `call` (`raise_again`, `accept_reason` or `withdraw`: `R`/`A`/`W`), `words` (≤600) | fills the reply on Replies | `R` `A` `W` |
| `follow_up` | `question` (≤200) | fills the ask box | — |

A `finding` is written `F` and its number, as `F9`. Every field a row names is required unless
the row says "or". The list is defined once, as `SuggestionType` in
`crates/store/src/answer.rs`; anything that needs it imports it. `answer_question` is
provisional (DEV-1331).

A suggestion of any other type makes the whole answer not parse.

## When it doesn't fit

**An answer that doesn't parse is drawn folded and inert:** the first ~200 characters, a
*Show the rest (n words)* fold, the line *"No cites and no basis, so no actions are offered and it
isn't kept with the review"*, and one button, **Ask it again, in the format**. It is kept only as
the raw message, flagged as not parsed; it never reaches a finding, a draft or a count.

## How it is kept

| Reply | Kept as | Counted | Superseded |
| -- | -- | -- | -- |
| `answer` | the raw body, and the answer beside it | yes | never |
| `working` | the same; it must name the question it works on (`--to`) | never | by the next parsed reply to that question |
| `cant` | the same | no | by a later parsed `answer` to that question |
| prose (`--body`) | the raw body alone, not parsed | never | never; it supersedes nothing either |

Nothing is deleted: a superseded reply stays in the conversation, and `superseded_by` on the
wire names the reply that replaced it.

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

**The server**, on `message reply --answer`, before it is kept; the CLI prints the first failing
rule and exits 1 so the session can resend, and nothing is kept. The page never re-validates and never repairs. The
session is told the contract with every question it is asked (the watcher includes this file's
*fields* and *suggests* tables in the delivery).
