# The uplift boards

**The designs DEV-876 and the issues after it are built from.** Patrick approved the
control set and these two screens on 10 October 2026; of the controls he said
"Everything's perfect".

| File | Board | Built as |
| -- | -- | -- |
| `Controls.dc.html` | Controls · softer and tactile | `web/app/components/*` — the control set. Its `<script type="text/x-dc">` block is working state logic and is where the decision track's and the split button's behaviour comes from. |
| `Queue.dc.html` | Queue · one row per review | `web/app/pages/index.vue` |
| `Rules.dc.html` | Rules · what the reviewer has learned | `web/app/pages/rules.vue` |
| `BriefRefined.dc.html` | A2 · The brief, refined | Nothing. Superseded: Patrick ruled on 10 October 2026 that a review is a page per step, not one page folded. |
| `RoundTwo.dc.html` | A3 · Round 2 of the same pull request | Nothing. Same, and what it drew about a second round is on the step pages. |
| `Main.dc.html`, `Workbench.dc.html`, `TwoColumns.dc.html` | A, B, C · the three first directions | Nothing. Superseded by A2 and A3, kept because the canvas holds them. |
| `canvas.json` | Where each board sits on the canvas, and its size | — |

**The boards the review's own pages were built from are not here.** A5 to A8 — Understand,
Alternatives, Solution and Code review — were drawn on a real review of a client's private
repository, so neither they nor anything copied from them is in this repository. The first
three became one page on 16 October 2026 — `web/app/components/ReviewCase.vue` and the three
bands beside it, drawn by `web/app/pages/reviews/[id]/{understand,alternatives,solution}.vue`
— and the fourth is `code-review.vue`. Their invented example is
`contract/examples/getReview.response.code-review.json`, written at the lengths those boards
were measured at, which is why it was invented rather than copied.

**These are copies, not a build input.** Nothing in `web/` reads them and nothing builds
them. Each references a `support.js` that belongs to the tool they were drawn in and is not
in this repository, so opening one in a browser shows unstyled markup: read the source, or
open the artifact above.

The earlier set, which DEV-855 built the first dashboard from, is in the directory above
this one. Where the two disagree, these are the approved ones.
