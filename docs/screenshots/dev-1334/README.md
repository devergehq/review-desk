# DEV-1334 · The session's suggestions as Accept and Dismiss

Taken at 1,600 on the e2e build: the real binary, a fresh database seeded through the command
line, and the session's answers sent through `review-desk message reply --answer`, which the
server checks against the contract. Every click is the developer's, in the browser, and writes
through the API. **One thing is stood in**, as for DEV-1333: Allele's reading of the session
(`GET …/session`), so the header reads *idle* rather than *No agent connected*; accepting does
not depend on it. The spec that took them was a throwaway and is not committed;
`web/e2e/suggestions.spec.ts` asserts the same flows with nothing stubbed.

| File | What it shows | Target |
| -- | -- | -- |
| `1-findings-suggestions*.png` | Three open suggestions: a new finding, a regrade (*Keep as should* is its Dismiss), a follow-up | S08 |
| `2-findings-accepted-new*.png` | The new finding accepted, *Added as F16 · should · ran* with *Open F16*; the follow-up dismissed, *Say why* after the click | S08 |
| `2-findings-row-marked-new.png` | F16 on Findings, in *To post*, marked *new* | S08 |
| `3-dismissed-with-reason-panel.png` | The dismissal with the reason given after it | S08 |
| `4-turn-into-a-finding-*.png` | An answer with no suggestions, before and after *Turn into a finding* | S06 (panel 4) |
| `5-replies-drafted-reply.png` | A `draft_reply` accepted: Replies opens on F9 with the session's words in the field, the call it drafted on the button, nothing sent | S09 |
