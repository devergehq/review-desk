# The uplift boards

**The designs DEV-876 and the issues after it are built from.** Patrick approved the
control set and these two screens on 10 October 2026; of the controls he said
"Everything's perfect". They are kept here so the reference outlives the link:
[claude.ai/artifact/HRuDVtNJvdCJnYCwcHj3qu](https://claude.ai/artifact/HRuDVtNJvdCJnYCwcHj3qu).

| File | Board | Built as |
| -- | -- | -- |
| `Controls.dc.html` | Controls · softer and tactile | `web/app/components/*` — the control set. Its `<script type="text/x-dc">` block is working state logic and is where the decision track's and the split button's behaviour comes from. |
| `Queue.dc.html` | Queue · one row per review | `web/app/pages/index.vue` |
| `Rules.dc.html` | Rules · what the reviewer has learned | `web/app/pages/rules.vue` |
| `BriefRefined.dc.html` | A2 · The brief, refined | Nothing yet. The review page, a later issue, built with this issue's components. |
| `RoundTwo.dc.html` | A3 · Round 2 of the same pull request | Nothing yet. Same. |
| `Main.dc.html`, `Workbench.dc.html`, `TwoColumns.dc.html` | A, B, C · the three first directions | Nothing. Superseded by A2 and A3, kept because the canvas holds them. |
| `canvas.json` | Where each board sits on the canvas, and its size | — |

**These are copies, not a build input.** Nothing in `web/` reads them and nothing builds
them. Each references a `support.js` that belongs to the tool they were drawn in and is not
in this repository, so opening one in a browser shows unstyled markup: read the source, or
open the artifact above.

The earlier set, which DEV-855 built the first dashboard from, is in the directory above
this one. Where the two disagree, these are the approved ones.
