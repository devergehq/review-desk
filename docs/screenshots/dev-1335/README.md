# DEV-1335 · The panel says who is answering

Taken at 1,600 by `web/e2e/answering.spec.ts`, with a screenshot line added for the run and
not committed: the real binary and one review, served twice from one database. **Nothing is
stubbed**: two way is the server reading `web/e2e/fakeAllele.ts`, a socket speaking Allele's
wire, and one way is a reply written with `review-desk message reply`.

| File | Mode | Target |
| -- | -- | -- |
| `1-two-way-panel.png` | two way: a question asked here, in the thread | S06, S11 |
| `2-none-panel.png` | none: no Allele, nothing a session wrote; read only | S11 |
| `3-one-way-panel.png`, `3-one-way.png` | one way: what the session wrote, Copy a reference, no send | S13 |
