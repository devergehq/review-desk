# DEV-1333 · Ask this review

Taken at 1,600 (and one at 390) on the e2e build: the real binary, the seeded database, asks
typed into the panel and replies sent through `review-desk message reply`, which the server
parses against the answer contract. **One thing is stood in**: Allele's reading of the session
(`GET …/session`), because e2e runs with no Allele and DEV-1335 owns its fake socket. The spec
that took them was a throwaway and is not committed; `screenshots.spec.ts` stubs nothing, and
these could not be taken there without a stub.

| File | S6 state | Target |
| -- | -- | -- |
| `1-empty-overview*.png` | 1 · nothing asked yet | S02, S06 |
| `2-working-findings-panel.png` | 2 · working | S04, S06 |
| `3-read-only-empty-panel.png`, `6-read-only-thread-panel.png` | 3 · no agent, read only | S06 |
| `4-answered-findings*.png` | 4 · answered, in the contract's shape | S04, S07 |
| `5-unparsed-and-cant-panel.png` | not parsed, and `cant` | S07 |
| `7-phone-390.png` | under 700px: no panel; the sheet is DEV-1336's | S14 |
