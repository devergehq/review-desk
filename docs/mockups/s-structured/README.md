# S · the structured app

**The design target from 21 November 2026.** Patrick, on seeing the first sixteen screens: *"I honestly
think it's perfect"* — the layout, the structure and the workflow. Typography and styling may
still move; the layout, structure and workflow are settled. [`docs/design.md`](../../design.md)
carries the rules these screens are drawn to; this folder is the pictures.

## Where the design lives, and which copy wins

| Copy | What it is | Who may change it |
| -- | -- | -- |
| `review-desk.pen` in pen.dev | **The source.** Every frame, component and token; exact values by node | The design session only. One session edits a `.pen` file at a time |
| The `.webp` files here | Exports of the source at 2× (sample data only: `acme/portal #514`) | Re-exported whenever a screen changes, in the same PR as the change |
| [The screens page](https://claude.ai/artifact/Lfja1y6Tty3Y5e74Q2u9Ac) | The same exports, one per screen, tap to zoom | The design session republishes it |

**A build session works from the pictures and from `design.md`, never from the `.pen` file.** The
file is encrypted, it is open in someone's pen.dev window, and a second editor corrupts the first
one's work. If a value can't be read off a picture (a gap, a weight), take it from `tokens.css`;
if `tokens.css` has nothing close, ask on the ticket rather than inventing a token.

## The screens

| Screen | File | Pen node | What it is | Route it becomes |
| -- | -- | -- | -- | -- |
| S0 | `S00-what-changes.webp` | `KsYwb` | What changes from the earlier B direction, and the type scale | — |
| S1 | `S01-pull-requests.webp` | `LFzD2` | One list of pull requests with a preview pane | `/` (replaces Queue and Reviews) |
| S2 | `S02-overview.webp` | `A560I` | Review · Overview: bottom line, where it stands, the five questions | `/reviews/[id]` |
| S3 | `S03-understanding.webp` | `tebGX` | Review · Understanding: the case in one tab | `/reviews/[id]/understanding` (was understand, alternatives, solution) |
| S4 | `S04-findings.webp` | `Ofrjv` | Review · Findings: the list and the selected finding | `/reviews/[id]/findings` (was code-review) |
| S5 | `S05-draft.webp` | `cs9gm` | Review · Draft | `/reviews/[id]/draft` |
| S6 | `S06-conversation-states.webp` | `JW0mm` | The conversation panel's four states | The panel on every review tab |
| S7 | `S07-answer-contract.webp` | `BiqHh` | The answer contract | [`docs/answer-contract.md`](../../answer-contract.md) |
| S8 | `S08-suggestions.webp` | `I6rNii` | Findings with the session's suggestions | `/reviews/[id]/findings` |
| S9 | `S09-replies.webp` | `fDWo9` | Review · Replies: Raise it again, Accept their reason, Withdraw | `/reviews/[id]/replies` (was what-happened) |
| S10 | `S10-rules.webp` | `K2kj6x` | Rules in the shell. A design target: rules are not wired yet | `/rules` |
| S11 | `S11-any-agent.webp` | `Qt2Go` | Who answers: Allele (two way), your own session (one way), none (read only) | The panel's header and Settings |
| S12 | `S12-being-written.webp` | `LxmEG` | Overview while the brief is still being written | `/reviews/[id]` |
| S13 | `S13-one-way.webp` | `UnGrD` | The panel with no Allele: only what your session wrote | The panel |
| S14 | `S14-phone-sheet.webp` | `C7bmP` | Phone: the conversation as a bottom sheet | Every review route under 700px |
| S15 | `S15-replies-in.webp` | `xdmn6` | Overview when the author's replies and commits arrive | `/reviews/[id]` |
| S16 | `S16-round-2-sign-off.webp` | `zDcGz` | Draft for round 2: what changed since round 1, lines moved with the code, signing against the head read | `/reviews/[id]/draft` |
| S17 | `S17-nothing-needs-you.webp` | `ZQeYR` | The list when nothing waits on you: where everything else is, and what happened since yesterday | `/` |

Old routes keep working as redirects to their new tab, so links in posted reviews and in Linear
don't break.

## How a build session uses this

1. Read the ticket. It names its screens.
2. Read `design.md`, then open the named `.webp` files. They are the target.
3. Build. Take the copy from the picture word for word unless the record can't support it; a
   fact the record can't derive is left out, never guessed (`design.md`, *the case*).
4. Before opening the PR, screenshot the running page at 1,600 and at 390 wide and put the
   screenshot beside the target in the PR description.
5. Where the build has to differ from the picture, say so in the PR under **Departs from the
   design**, with the reason. The design session folds accepted departures back into the `.pen`
   file and re-exports.
