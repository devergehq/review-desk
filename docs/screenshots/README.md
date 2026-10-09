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
| `seeded-1-inbox.png` | The Inbox, on a database an agent session has recorded one review into. Two items: a brief to confirm, and the draft that session wrote without waiting. |
| `seeded-2-review.png` | One review, with the brief unconfirmed — the control only the dashboard has. |
| `seeded-3-draft.png` | The draft preview, before anything is posted: the checks, the nit held back, and the event it would be posted as. |
| `seeded-4-rules.png` | Rules, on a database with reviews and no rules. The repository came from the review; the command line cannot write a rule, which is a gap rather than an empty screen. |
| `fresh-1-inbox.png` | The Inbox on a database with nothing in it. It reads as new rather than as broken, and says the one thing that fills it. |
| `fresh-2-review.png` | A review that is not there. `not_found` and an empty review are different answers. |
| `fresh-3-draft.png` | A draft that is not there. |
| `fresh-4-rules.png` | Rules with nothing learned yet. |
| `server-stopped.png` | `review-desk serve` stopped while a page was open: what is wrong, the command that fixes it, and a retry. |

The records in the seeded shots are invented, but they are **real rows in a real SQLite
database**, written by `review-desk review open`, `brief put`, `finding add`, `layer set`
and `draft put`. Nothing on these screens came from a fixture — that is what the absence of
the "Sample data" marker means.
