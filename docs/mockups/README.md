# The Review Desk mockups

Five screens with sample data, drawn on 8 October 2026 and the reference DEV-855 built
`web/` from. They are kept here so the reference outlives the link:
[claude.ai/artifact/MbzJCUfZeTeSQhZ4rBeYM6](https://claude.ai/artifact/MbzJCUfZeTeSQhZ4rBeYM6).

| File | Screen | Built as |
| -- | -- | -- |
| `Main.dc.html` | Inbox | `web/app/pages/index.vue` |
| `Review.dc.html` | One review | `web/app/pages/reviews/[id]/index.vue` |
| `Draft.dc.html` | Draft preview | `web/app/pages/reviews/[id]/draft.vue` |
| `Rules.dc.html` | Rules | `web/app/pages/rules.vue` |
| `Flow.dc.html` | How a review flows | Nothing. Reference only, by DEV-855's out-of-scope list. |
| `canvas.json` | Where each board sits on the canvas, and its size | — |

**These are copies, not a build input.** Nothing in `web/` reads them, and nothing builds
them. Each one references a `support.js` that is part of the tool they were drawn in and
is not in this repository, so opening one in a browser shows unstyled markup; read the
source, or open the artifact above.

## Where the app departs from them

Each of these is a decision, not a drift. The visual language — the neutral ground, the
dark ink, the single blue accent, IBM Plex Sans and Mono, no emoji and no bot banner — is
the mockups' throughout.

| Departure | Why |
| -- | -- |
| The "How it flows" navigation item is gone | It pointed at `Flow.dc.html`, which DEV-855 puts out of scope as a screen. The other three items remain. |
| "Reviews" goes to whichever review is waiting | There is no review index in this slice, and a link to nothing is worse than a link to the thing you were going to open. |
| The three-box problem diagram is drawn by mermaid | The record stores `brief.diagram` as diagram source, and a browser was chosen as the renderer so that source is drawn. The content and position are the mockup's. |
| An interactive border is darker than the mockups' `#AEB3BD` | That grey is 2.1:1 against a white surface. The edge of a text field has to clear 3:1, so `--rd-control-line` is `#868C99` at 3.4:1. The original is kept for the dashed edges it draws. |
| The progress meter's track is a light blue, not grey | So the whole bar reads as one measure. The number is written out beside it either way. |
| "Request review" is a link out to GitHub | Review Desk never writes to GitHub. A button that looked like it did would be a lie about where the write goes. |
| One review gained a brief-confirmation control and a per-finding disposition control | The mockups have neither, and DEV-855 requires confirming a brief to work. Both are in the mockups' visual language. |
| "Same answer on re-run" reads "Not measured yet" | The replay consistency check is a later slice, and the contract says a figure with no source is null rather than a number. |
| One pull request shows "description not checked" | Same reason: that whole group is GitHub state Review Desk has no source for, so one row is left null to show what the absence looks like. |
| Empty states exist at all | A mockup has no empty list, and an app always does. |
