# Screenshots

**Produced, not taken.** `pnpm run e2e:screenshots` builds the dashboard into the binary,
seeds a database through the command line, starts two servers, and drives a real browser
over them — the same servers and the same browser the end-to-end test asserts against. A
screen that changes changes its picture here, and the pull request shows both.

```sh
cd web && pnpm run e2e:screenshots
```

| File | What it is |
| -- | -- |
| `seeded-1-queue.png` | The queue, on a database an agent session has recorded one review into. **One row**, not two: that session wrote the draft before the brief was confirmed, so the review is waiting on the developer for two things and the row shows the earlier one. |
| `seeded-2-review.png` | One review, with the brief unconfirmed — the control only the dashboard has. |
| `seeded-3-draft.png` | The draft preview, before anything is posted: the checks, the nit held back, and the event it would be posted as. |
| `seeded-4-rules.png` | Rules, on a database with reviews and no rules. The repository came from the review; the command line cannot write a rule, which is a gap rather than an empty screen. The populated page is in `uplift/`. |
| `seeded-5-review-no-confirmation.png` | One review whose brief the session recorded as needing no confirmation, with the reason it gave. Layer 2 is finished with and nobody signed it, so the card is dashed and offers no button. |
| `fresh-1-queue.png` | The queue on a database with nothing in it. It reads as new rather than as broken, and says the one thing that fills it. |
| `fresh-2-review.png` | A review that is not there. `not_found` and an empty review are different answers. |
| `fresh-3-draft.png` | A draft that is not there. |
| `fresh-4-rules.png` | Rules with nothing learned yet. |
| `seeded-6-queue-390.png` | The queue at a phone's width, which DEV-876 asks for. Every cell wraps onto its own line in reading order; `e2e/review-desk.spec.ts` asserts that nothing scrolls sideways. |
| `server-stopped.png` | `review-desk serve` stopped while a page was open: what is wrong, the command that fixes it, and a retry. |

The records in the seeded shots are invented, but they are **real rows in a real SQLite
database**, written by `review-desk review open`, `brief put`, `finding add`, `layer set`
and `draft put`. Nothing on these screens came from a fixture — that is what the absence of
the "Sample data" marker means.

## `uplift/` — the approved boards beside the pages built from them

DEV-876 rebuilt the queue and the rules page from the boards in
[`docs/mockups/uplift/`](../mockups/uplift/). These six images are the comparison that
pull request carried, and they are **not** produced by `pnpm run e2e:screenshots`:

| File | What it is |
| -- | -- |
| `board-queue.png`, `board-rules.png` | The approved boards, rendered at 1440 |
| `queue-wide.png`, `rules-wide.png` | The pages built from them, at 1440 |
| `queue-390.png`, `rules-390.png` | The same two at a phone's width |

**Why these are not from the binary.** A board has records on every row, and the only
database with that many records is sample mode — which exists in development only, and is
dropped from a production build on purpose. A seeded database has one review and no rules
at all, so the binary's own shots above show the empty states and these show the full ones.
The "Sample data" marker is visible in all four, which is what it is for.

How they were made, so anyone can make them again:

```sh
cd web && pnpm run dev            # sample mode at /?sample and /rules?sample
```

then Playwright at `{width: 1440, height: 1000}` and `{width: 390, height: 844}`,
`deviceScaleFactor: 2`, `fullPage`. The boards are the `.dc.html` files from
`docs/mockups/uplift/` with the drawing tool's `support.js` line replaced by
`<style>x-dc{display:block}helmet{display:contents}</style>` and the trailing
`<script type="text/x-dc">` block removed — both belong to the tool rather than to the
drawing. Neither board uses the tool's templating, so what a browser draws is what the
board draws.
