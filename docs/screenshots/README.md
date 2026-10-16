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
| `seeded-2-review.png` | One review, with the brief unconfirmed — the control only the dashboard has. Each finding's evidence is its own block below the one-line facts, keeping the paragraphs the session wrote. |
| `seeded-3-draft.png` | The draft preview, before anything is posted: the checks, the nit held back, and the event it would be posted as. |
| `seeded-4-rules.png` | Rules, on a database with reviews and no rules. The repository came from the review; the command line cannot write a rule, which is a gap rather than an empty screen. The populated page is in `uplift/`. |
| `seeded-5-review-no-confirmation.png` | One review whose brief the session recorded as needing no confirmation, with the reason it gave. Layer 2 is finished with and nobody signed it, so the card is dashed and offers no button. Its `diagram` is the text sketch from DEV-880, shown as written — **this one is from the binary**, so it is the sketch state drawn from a real database rather than a fixture. |
| `fresh-1-queue.png` | The queue on a database with nothing in it. It reads as new rather than as broken, and says the one thing that fills it. |
| `fresh-2-review.png` | A review that is not there. `not_found` and an empty review are different answers. |
| `fresh-3-draft.png` | A draft that is not there. |
| `fresh-4-rules.png` | Rules with nothing learned yet. |
| `seeded-6-queue-390.png` | The queue at a phone's width, which DEV-876 asks for. Every cell wraps onto its own line in reading order; `e2e/review-desk.spec.ts` asserts that nothing scrolls sideways. |
| `server-stopped.png` | `review-desk serve` stopped while a page was open: what is wrong, the command that fixes it, and a retry. |
| `dev-929-1-index.png`, `-390` | **The body as it will post, after DEV-929.** The session's verdict line and `### Problem fit` with its diagram, then an index of all twelve findings — the glyph and the word, the place in the code as a link, and whether the finding is on the code or in the body — then the three sections for the findings the diff cannot anchor. |
| `dev-929-2-threads.png`, `-390` | The other half of what posts: nine inline threads, each with the line it sits on and the comment as the author will read it. None of them is in the body. |
| `dev-929-3-nothing-to-anchor.png` | A draft written as one block of markdown: no blocks to index, and nothing to hang a comment on. The card says which it is rather than being empty. |
| `dev-932-1-waiting.png`, `-390` | **The gap a sign-off opens, after DEV-932.** Signed off and waiting: how long ago, when anything last wrote to the review, that the review's own session is what posts it, and that whether that session is running is not something Review Desk is told. |
| `dev-932-2-nothing-posted.png`, `-390` | Forty seconds on, with nothing reported. It says the session may not be running and what to do about it — a plain sentence in the body ink, not a boxed warning, because nothing has gone wrong with the record and what is in doubt is a session this app cannot see. |
| `dev-932-3-posted.png`, `-390` | The session posted it, and the page said so without a reload: when, as which event, how long it took, and a link to the review on GitHub. |
| `dev-932-4-sent-back.png`, `-390` | The developer took the sign-off back while the page was watching. The waiting words are gone and the draft is with the session again. |

**The four `dev-932` states are reached the way the real thing reaches them.** The sign-off
is a click, because that is the only way a draft can be signed off; the posting is
`review-desk draft posted`, which is the command a review session runs; the take-back is
the control on the page. The waits are moved with Playwright's clock rather than waited
out, and **pinned to the record's own stamps rather than moved by a duration** — 25 seconds
after `approved_at` for the first picture, 45 for the second, 40 after `posted_at` for the
third — so the 1440 and the 390 shot of one state agree with each other and two runs put
the same figures in the panel.

**Two figures in them are still the server's, and no browser clock can pin those.**
`waiting_on_you_seconds`, on the line under the trail, is measured from the draft becoming
ready to the developer signing off; "Taken to post", in the third picture, is `posted_at`
less `approved_at`. Both are real seconds between two commands, so each can land a second
either way and make a run differ. The alternative is moving the database's clock, which is
what the `dev-890` shots below had to do and is worth it only where the figures are the
subject of the picture.

The records in the seeded shots are invented, but they are **real rows in a real SQLite
database**, written by `review-desk review open`, `brief put`, `finding add`, `layer set`
and `draft put`. Nothing on these screens came from a fixture — that is what the absence of
the "Sample data" marker means.

## The case page — three steps on one page, 16 October 2026

| File | What it is |
| -- | -- |
| `case-1-the-case.png`, `-390` | **The case for one change, as it opens.** The brief's own headline sentence, then one line of facts read off the record — "3 problems · option C built · fixes all 3 · blind pass agrees" — then the verdict on the approach, then three bands: what was wrong, what was chosen, what was built. Each band is the shape of its own answer rather than a summary of it, so the counts are read rather than asserted. |
| `case-2-the-case-open.png`, `-390` | The same page with every fold open and the first row of each band open: the two flows, the reference material, the reasoning for and against each option, what the blind pass was given, and the three lists the ticket and the description left behind. **This is the cost of the page as well as its depth** — the picture is what one reader opening everything at once gets, and nothing on it is a page away. |

**What this replaced, and what became of its pictures.** Understand, Alternatives and
Solution were a page each: `dev-885-1` to `-3` below are of those three, and they are left
as the record of them. On review #9639 they came to 3,653 + 3,132 + 2,768 pixels and three
navigations for one review, and Patrick — who reads about seventy-two reviews a month —
said of all three that the content is good and that they are "quite broad, quite large,
you've got to dig into them". `dev-892-2-numbers-in-full.png` goes the same way: the fold it
pictures was the Alternatives page's answer to a matrix headed by bare numbers on a page
away from the sentences, and the sentences are now the rows of band 1 three inches above
the matrix.

**The three addresses still work and still draw this page**, so a run of this set that
asked for `/alternatives` would take the same picture twice. Steps 3 and 4 carry a fragment
to the band they are, which is what the trail links to.

## The two lists — one row per pull request, one line per fact, 16 October 2026

| File | What it is |
| -- | -- |
| `seeded-1-queue.png`, `seeded-6-queue-390.png` | **The queue, after step 1.5 of the design loop.** Every row's title in one face — it was Newsreader under "Needs you" and the sans everywhere else, on one column of titles — and the five cells on a grid, so a column is the same width on every row of every group. The status cell is a state and one line of facts under it. |
| `seeded-7-reviews.png`, `seeded-8-reviews-390.png` | **The Reviews list, one row per pull request.** The step carries its number on the state line — "What happened 7/7" — and what the step says is the line under it, where it was a third line reading "step 7 of 7". |
| `dev-957-1-queue.png`, `-390` | The queue's row for a pull request whose head has moved. The pull request's state — `open`, `merged`, `closed` — is on the row's own pull-request line beside the repository now, not in the status cell. |
| `dev-957-2-finished.png`, `-390` | **Four rounds of a merged pull request as one row, with the fold open.** It was four rows. The fold's own line says how many rounds are behind it, which they are and what became of them — "3 earlier rounds · rounds 1 to 3 · 3 finished" — so a reader never has to open it to find out whether it matters. Each round inside it keeps its own step, its own sentence and its own link, on the row's own columns. |

**What the design pass said, and what each picture answers.**

> **05 · Status cells are paragraphs.** On the Queue, "0 replies · 0 threads resolved ·
> 0 findings with code changed since · as at 24 min ago · pull request open at the last
> harvest" fills three lines to say *nothing new*. Titles switch between serif (Needs you)
> and sans (everything else) on the same list.
>
> **07 · One pull request, many rows.** #9541 appears five times on Reviews, once per round,
> with near-identical titles and status. The Queue already folds rounds together; Reviews
> doesn't.

**No figure was dropped to make a line shorter.** Three *measured* noughts became "Nothing
back", which says the same thing; a null is still never a nought, and a row with two noughts
and one unmeasured figure still draws the two. What left the status cell is the pull
request's state, which moved up one line to where the row's other facts about the pull
request are — and the sentence around it, "pull request … at the last harvest", which named
the row's own subject and repeated the basis the clause beside it had just given.

## DEV-880 — a diagram that is not a diagram

| File | What it is |
| -- | -- |
| `dev-880-before.png` | A brief whose `diagram` field holds a hand-drawn text sketch, **before** the fix. mermaid rejected it and left its own error graphic — a bomb, "Syntax error in text", "mermaid version 12.1.0" — at the bottom of the document, outside the card that asked for the diagram and outside the app's root element. It then stayed there on every page the developer opened. |
| `dev-880-after.png` | The same page after. The sketch is where the diagram would be, shown as written, and nothing is below the last card. |

**The sketch in both is verbatim**, down to the line breaks: it is what a review session
actually wrote into a brief's `diagram` field on 10 October 2026, not something drawn to
illustrate the bug. That is why it describes an OAuth authorize-and-redirect flow while the
sample review it sits on is about caching a permissions lookup — the two do not match, and
the real one did not either. A sketch written to fit the brief would have made a tidier
picture of a failure nobody had seen. `e2e/harness.ts` seeds the same text.

Both are sample mode, because sample mode is the only place with a brief that carries one
of these. Made the same way as the `uplift/` shots below:

```sh
cd web && pnpm run dev
```

then Playwright at `{width: 1440, height: 1000}`, `deviceScaleFactor: 2`, `fullPage`, on
`/reviews/16?sample`. For the "before", `app/components/MermaidFigure.vue` at its state
before this pull request and nothing else changed.

## DEV-892 — six defects a real review showed, and a seventh

| File | What it is |
| -- | -- |
| `dev-892-2-numbers-in-full.png` | The Alternatives matrix headed by its labels — "1 · the shared slot", not a sentence wrapped ten lines deep — with the fold under it open: every number, its label and the line that used to be the heading. |

**The retaken `dev-885` pictures above carry the rest of it.** The matrix headings and the
part cards are what those three are pictures of, and a defect in how a page reads is fixed
in the picture of the page rather than in a picture of its own. What changed in them:

- The trail on all three draws step 2's 34-word note on one ellipsised line. **DEV-917
  replaced the two-line clamp these pictures were first taken of**, so the control that
  used to open the note in place is gone and the picture of it with it; what opens now is
  below.
- The Alternatives matrix is headed by numbers and two- or three-word labels, at both
  widths: the phone picture shows the same heading above each verdict, where the sentence
  used to be.
- The Solution part cards have one head apiece — marks, then title, never title under
  marks — and a row of them is one height, with the figures, the tally and the control
  level across it.
- The Understand page's foot has no word count over its fold. **"Already decided" is three
  levels since DEV-917** — the decision, what is still open, then the sources as a table —
  where DEV-892 left it as a line, its sources and a control for the rest.

## DEV-917 — four places where a sentence was put in the room for a phrase

| File | What it is |
| -- | -- |
| `dev-917-1-step-card-open.png` | **All four fixes in one page.** The trail draws each step on two one-line rows and step 2's card is open over the page — the step's name, "time not recorded", and the three facts it recorded as label and value. **Nothing under the card has moved**: compare the headline's position with any of the `dev-885` pictures above, which is the acceptance criterion a test measures in `review-desk.spec.ts`. Below it: each flow box's mark is a tag carrying the problem's name, with the three listed once above both flows; `TenantContext::current()` is a code chip where the session's backticks used to be printed as characters; and "Already decided" is three levels, the third a table whose last row shortens a 40-character commit id to seven. |

The step's duration moved onto the card, which is why the trail in these pictures has no
figure under each step. The review's own totals are still under the trail.

## DEV-890 — how long each step took

| File | What it is |
| -- | -- |
| `dev-890-queue.png` | The queue. The row with the agent says which step it is on **and how long it has been on it** — "Alternatives · 26 min" — and reads "Last activity 9 min ago" where it read "Session state unknown". **Every row carries a time**, so the caption reads "Oldest first" with nothing after it: the clause about rows with no recorded time is drawn only when there is one. |
| `dev-890-trail.png` | A step page. Each finished step carries its duration under its outcome, and step 4 — reported straight to `done` with no start — says "time not recorded". Under the trail: the total with the agent, and the total waiting on the developer, which this review has not finished one of. |
| `dev-890-old-review.png` | **A review as it was recorded before this change.** Every step says "time not recorded", both totals say "not recorded", and nothing anywhere says 0. That is the acceptance criterion: an old review is never given a made-up start. |

**Three real reviews in a real database**, written through `review-desk review open`,
`brief put`, `finding add`, `layer set` and `draft put`, served by the binary with the
dashboard in it. Two things in them are not what a day's work would leave, and both are
named here rather than left to be assumed:

- **The elapsed times were moved back with `sqlite3`** after the commands ran, because the
  commands ran in seconds and the figures are the point of the picture. Nothing invented a
  duration: every pass still has the two ends the store stamped, and only the instants
  moved. There is no other way to photograph an hour.
- **The third review's state record was replaced** with what migration 12's backfill
  leaves — one row per step, from `updated_at`, and no start anywhere — so that the "before
  this change" state could be seen on a database created after it.

```sh
cd web && pnpm run generate && cargo build --manifest-path ../Cargo.toml
# seed a database through the command line, serve it, then Playwright at
# {width: 1440, height: 1000}, deviceScaleFactor: 2, fullPage
```

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
