/*
 * What the end-to-end test needs: a seeded database, a binary serving it, and a fresh one
 * beside it.
 *
 * **Everything here goes through the command line and the binary.** Nothing writes SQL,
 * nothing imports the store crate, and nothing stubs a request — the point of this test is
 * that the whole path works, so a shortcut anywhere in the setup would be a shortcut
 * through the thing being tested. The seed is the commands an agent session would run, in
 * the order it would run them.
 */

import { execFileSync, spawn, spawnSync, type ChildProcess } from 'node:child_process'
import type { Page } from '@playwright/test'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

/** The contract's invented brief at real length, which the three step pages are seeded from. */
const REAL_LENGTH_BRIEF = resolve('../contract/examples/confirmBrief.response.real-length.json')

/**
 * The same review's findings and draft, at the length board A8 was drawn at.
 *
 * Fourteen findings across all three lists, ten of them with the code the finding is about
 * and the comment as it will post. The code review page is seeded from this, and so are its
 * unit tests and its screenshots.
 */
const CODE_REVIEW_EXAMPLE = resolve('../contract/examples/getReview.response.code-review.json')

/**
 * The same review's draft, as the blocks the house style's shape is composed of.
 *
 * The draft page is seeded from this, and so are its screenshots: twelve posted findings,
 * nine of them with a line inside the diff, a mermaid fence in the `### Problem fit` block,
 * and the folded provenance at the end.
 */
const REAL_LENGTH_DRAFT = resolve('../contract/examples/getDraft.response.real-length.json')

/**
 * A diagram field holding something that is not a diagram.
 *
 * **This is not invented.** It is what a review session wrote into a brief on 10 October
 * 2026 — a flow drawn by hand in text, in the field the dashboard hands to mermaid. mermaid
 * rejected it and left its own error graphic at the bottom of every page the developer
 * opened afterwards, which is DEV-880. A session will write one of these again, so the
 * seed carries one and `review-desk.spec.ts` asserts what the page does with it.
 *
 * It is on the *waived* review rather than the main one, so that the main review's valid
 * `flowchart` keeps proving that a diagram still draws.
 */
export const TEXT_SKETCH = `BEFORE (shared surface, implicit flow record)

consumer app --> GET /oauth/authorize --(guest)--> redirect()->guest() writes url.intended
    |
    v
GET /login  (SHARED staff page)
    |
    FortifyServiceProvider reads url.intended`

/*
 * **Step 2's note is the long one, and deliberately.** It is the same note
 * `getReview.response.code-review.json` carries — **invented, like everything else in
 * this example**, and written to 45 words because that is the length DEV-892 measured a
 * real one at: 27 words under Understand when the step had finished, 52 while the brief
 * was half written. A trail drawn against seven short notes cannot show whether a long
 * one is drawn on one line and ellipsised, so the one example these screenshots and the
 * unit tests share carries one.
 *
 * Exported because `review-desk.spec.ts` rewrites this step with a short detail to measure
 * what moves, and has to put it back exactly.
 */
export const STRUCTURED_LONG_NOTE
  = 'The brief was rewritten twice: the first pass mixed how the system works today '
    + 'into the problem, so the problem half could not be handed over, and the second '
    + 'split them and numbered three problems before the options pass was asked for '
    + 'anything at all.'

/**
 * What the seed leaves on step 7: posted, and no harvest has read it yet.
 *
 * Exported for the same reason the note above is — `review-desk.spec.ts` writes a
 * harvested step 7 to measure the trail with one and has to put this back exactly. A copy
 * typed into the spec would drift from the seed the first time either changed, and the
 * drift is invisible: a blank step-7 line is only wrong next to what the seed meant.
 */
export const STRUCTURED_STEP_SEVEN_DETAIL = 'after posting'

/** The binary, as `cargo build` leaves it. */
export const BINARY = resolve(
  process.env.REVIEW_DESK_BIN ?? '../target/debug/review-desk',
)

/** Where the two servers' addresses are left for the specs to read. */
export const ADDRESSES = resolve('e2e/.addresses.json')

/**
 * What `global-setup.ts` leaves for the specs.
 *
 * Declared once and used by the writer and both readers, which is what keeps the three in
 * step: each spec runs in its own process, so the file is the only thing they share, and
 * each spec used to describe its shape again with a different subset of the fields.
 */
export interface Addresses {
  seeded: string
  seededDatabase: string
  empty: string
  reviewId: number
  /**
   * A third review carrying the **structured** brief: the twelve rows DEV-882 added, at the
   * length the approved boards draw. It is what the Understand, Alternatives and Solution
   * pages are seeded from, and what their screenshots are taken of.
   *
   * **It has a draft, and until DEV-878 it did not.** The comment here used to say it was
   * deliberately in nobody's queue — brief confirmed, no draft, none of the Inbox's counts
   * touched. That stopped being possible: the code review page's decisions change a
   * finding's *block* of the draft, so a review with no draft has nothing to decide
   * against and the page it is seeded for could not be exercised. Its own layer trail
   * already claimed a draft by saying step 6 was waiting on the developer, so this is the
   * seed being made to agree with itself.
   *
   * So "Needs you" holds two rows, not one, and the counts in this suite say two.
   */
  structuredReviewId: number
  /**
   * A second review on the same repository whose brief was recorded as needing no
   * confirmation. It is deliberately not in anybody's queue: no brief to confirm and no
   * draft to approve, so it changes none of the Inbox's counts and is only reachable by
   * going to it.
   */
  waivedReviewId: number
  /**
   * A fourth review whose draft holds **eight findings back**, which is the state DEV-928
   * was raised from: six nits and two questions, claims of 41 to 63 words, one of them past
   * 60, one with no title at all and one whose title carries a long code name.
   *
   * **It is here because one or two rows prove nothing.** What went wrong on the real review
   * — a code name running out of the card, 63 words printed where a title belongs, eight
   * paragraphs where a reader wanted a list — are properties of the panel at that volume,
   * and the only place they can be measured is a browser at a real width. The other four
   * reviews between them hold one held-back finding and three; DEV-930's posted review,
   * added after this comment was written, holds none.
   *
   * It has a ready draft, so it is a fourth thing waiting on the developer: "Needs you"
   * holds three rows and the counts in this suite say three.
   */
  heldBackReviewId: number
  /**
   * A fifth review, **posted and with no draft**: its GitHub review id was recorded by
   * `review link`, which is the one way the command line can post a review at all.
   *
   * It is in no waiting group — no brief to confirm, no draft to approve, no outcome — so
   * it changes none of the counts the other specs assert, and it is the whole of what
   * "Waiting on the author" has to draw from. Before DEV-930 it was in no group on any
   * screen, which is the complaint the issue quotes.
   */
  postedReviewId: number
  /**
   * A sixth review: a review request, recorded and never started.
   *
   * No title, no brief, no finding, no draft — `acme/portal #777`, which is the shape of
   * the stray review DEV-945 was raised from. It is in no group of the queue, so it moves
   * none of this suite's counts, and it is the only review the dashboard offers a delete
   * for.
   */
  requestedReviewId: number
}

/** The addresses, as the setup wrote them. */
export function readAddresses(): Addresses {
  return JSON.parse(readFileSync(ADDRESSES, 'utf8')) as Addresses
}

/** One running server, and the database it is serving. */
export interface Serving {
  url: string
  database: string
  process: ChildProcess
  /** Everything it has printed, so a death has its last words rather than none. */
  said: () => string
}

/** Run one `review-desk` command against a database, and hand back what it printed. */
export function cli(database: string, args: string[]): string {
  return execFileSync(BINARY, args, {
    env: { ...process.env, REVIEW_DESK_DB: database },
    encoding: 'utf8',
  })
}

/** Run one command and read its `--json` answer. */
export function cliJson(database: string, args: string[]): Record<string, never> {
  return JSON.parse(cli(database, [...args, '--json']))
}

/** A directory that dies with the test run. */
export function scratch(): string {
  return mkdtempSync(join(tmpdir(), 'review-desk-e2e-'))
}

/**
 * The record an agent session would have written by the time the developer is needed.
 *
 * Layer 1 passed, layer 2 wrote a brief and is waiting on the developer, layers 3 to 5 ran,
 * and layer 6 wrote a draft. That is the state DEV-862's end-to-end criterion starts from:
 * the inbox listing a review with an unconfirmed brief, and a draft behind it waiting to be
 * approved once the brief is confirmed.
 *
 * **Every fact here is written by the command line, including the draft.** The developer's
 * two acts — confirming the brief and approving the draft — are deliberately absent,
 * because there is no command for either. That is the boundary this test exists to exercise.
 */
/**
 * The invented brief, with the nine fields the server derives taken back off it.
 *
 * `contract/examples/confirmBrief.response.real-length.json` is a **response**, and
 * `NewBrief` and every one of its sub-structures is `serde(deny_unknown_fields)` — so
 * feeding the example to `brief put` as it stands is refused, naming the first extra field
 * and nothing about the other eight. What is removed is exactly what the server works out
 * for itself: the review it belongs to, whether it is confirmed and when, the blind pass's
 * derived state, when it was last written, a part's line totals (summed from its files so
 * the card cannot disagree with the list) and the `F`-prefixed keys (built from a sequence
 * number, because the dashboard builds no ids).
 *
 * **Stripping rather than hand-writing a second copy** is the point: the pages, their unit
 * tests and these screenshots are then all drawn from one invented example, and an example
 * that changes changes all three.
 */
function structuredBrief(): Record<string, unknown> {
  const served = JSON.parse(readFileSync(REAL_LENGTH_BRIEF, 'utf8')) as Record<string, unknown>

  const {
    review_id: _reviewId,
    confirmed: _confirmed,
    confirmed_at: _confirmedAt,
    blind_pass_state: _blindPassState,
    updated_at: _updatedAt,
    ...written
  } = served

  written.parts = (written.parts as Array<Record<string, unknown>>).map(
    ({ lines_added: _added, lines_removed: _removed, ...part }) => part,
  )
  written.open_choices = (written.open_choices as Array<Record<string, unknown>>).map(
    ({ finding_key: _key, ...choice }) => choice,
  )
  written.disagreements = (written.disagreements as Array<Record<string, unknown>>).map(
    ({ finding_key: _key, ...clash }) => clash,
  )

  /*
   * **Recorded as needing no confirmation, and the reason is in the record.**
   *
   * There is no `review-desk brief confirm`: the binary's own help says "Confirming one is
   * in the dashboard", because confirming is the developer's act and the command line
   * cannot perform it — which is the boundary this whole suite exists to exercise. So an
   * unconfirmed brief seeded here would put a third row in the Inbox's "Needs you" and
   * rewrite every count the other specs assert.
   *
   * This is the third of layer 2's three states rather than a way round the second: a brief
   * an agent recorded as needing no confirmation is settled, unsigned and in nobody's
   * queue. None of the three step pages draws the confirmation state at all — DEV-878's
   * pages do — so what is seeded here is exactly what they read either way.
   */
  written.confirmation_required = false
  written.confirmation_not_required_because
    = 'the brief gate is off for this repository (brief_gate: never)'

  return written
}

/**
 * The findings the structured review carries, read from the contract's own example.
 *
 * **Read and stripped, not written out again.** `getReview.response.code-review.json` is
 * the review DEV-878's page is drawn from, and `web/app/data/reviews.json` holds the same
 * document for the unit tests — so a third hand-written copy here would be the one that
 * drifted. `NewFinding` is `deny_unknown_fields` and a hunk line's `line` is derived from
 * the hunk's `start_line`, so what is taken off is exactly what the store works out for
 * itself: the review it belongs to, its number and `F`-prefixed key, the GitHub comment it
 * was posted as, everything about the round, and each hunk line's number.
 *
 * **Every path is one the invented brief's own `parts[].files[].path` names**, because the
 * Solution page joins the review's findings to a part by file — `contract/README.md`: "the
 * review's findings whose `file` is one of `brief.parts[].files[].path`". One finding names
 * no file at all: it is about the pull request's description, and the code review page has
 * its own sentence for that.
 *
 * Four of the sequence numbers are load-bearing. `brief_open_choices` and
 * `brief_disagreements` each carry a foreign key to `findings (review_id, seq)`, and the
 * invented example points at 3, 7, 9 and 13 — so those findings have to exist **before**
 * `brief put` runs, or the write is refused by the database rather than by a validator.
 * They are seeded in sequence order for the same reason: a number is assigned by the store
 * from the order they are added, and the foreign keys name numbers.
 */
function structuredFindings(): Array<Record<string, unknown>> {
  const served = JSON.parse(readFileSync(CODE_REVIEW_EXAMPLE, 'utf8')) as {
    findings: Array<Record<string, unknown>>
  }

  return [...served.findings]
    .sort((one, two) => (one.seq as number) - (two.seq as number))
    .map(({
      review_id: _reviewId,
      seq: _seq,
      key: _key,
      rule_key: _ruleKey,
      github_comment_id: _commentId,
      round_status: _roundStatus,
      round_status_by: _roundStatusBy,
      round_status_checked: _roundStatusChecked,
      round_status_at: _roundStatusAt,
      round_status_commit: _roundStatusCommit,
      continues_review_id: _continues,
      continues_round: _continuesRound,
      rounds_open: _roundsOpen,
      introduced_by_key: _introducedByKey,
      late: _late,
      created_at: _createdAt,
      ...written
    }) => ({
      ...written,
      hunk: written.hunk == null
        ? null
        : {
            ...(written.hunk as Record<string, unknown>),
            lines: ((written.hunk as { lines: Array<Record<string, unknown>> }).lines).map(
              ({ line: _line, ...rest }) => rest,
            ),
          },
    }))
}

/**
 * The draft the review session wrote, as the blocks it is composed of.
 *
 * **Read from the contract's own example, not built here.** `getDraft.response.real-length.json`
 * is the draft that goes with `getReview.response.code-review.json` — the same review, the
 * same twelve posted findings — and since DEV-929 its blocks are the house style's shape:
 * the verdict line with its `**Method**` line, `### Problem fit` with the before-and-after
 * diagram, a block per finding, and the folded provenance. Building a second set of blocks
 * here would be a second opinion about the same review, and the screenshots would stop being
 * of the thing the contract publishes.
 *
 * The body is not given. `draft put --sections` composes it, so a body stated here would be
 * a second copy of what the store already builds, and the store refuses the two when they
 * differ — which, since the composition adds an index, they now always would.
 */
function structuredSections(): Array<Record<string, unknown>> {
  const example = JSON.parse(readFileSync(REAL_LENGTH_DRAFT, 'utf8')) as {
    sections: Array<{
      kind: string
      finding_seq: number | null
      title: string | null
      text: string
    }>
  }

  return example.sections.map(section => ({
    kind: section.kind,
    ...(section.finding_seq == null ? {} : { finding_seq: section.finding_seq }),
    /*
     * **The title goes with it**, because the example's blocks carry one and a block written
     * with none takes the finding's instead. For most of these the two are the same words;
     * sending what the example holds is what keeps the seeded body equal to the example's.
     */
    ...(section.title == null ? {} : { title: section.title }),
    text: section.text,
  }))
}

/**
 * A third review, with the structured brief the three step pages draw.
 *
 * **In the order the record requires**: the review, then its findings — because the brief's
 * open choices and disagreements point at four of them by sequence number through a foreign
 * key — then the brief, then the layers so the trail has an outcome under every step.
 */
function seedStructured(directory: string, database: string): number {
  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '514',
    '--head',
    'd41c8e2',
    '--title',
    'Give a scheduled export its own context',
    '--ticket',
    'ABC-402',
    '--changed-lines',
    '1442',
    '--not-checked',
    'The reporting front end, which reads the same definitions but was not changed.',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  const findings = structuredFindings()
  findings.forEach((finding, index) => {
    const path = join(directory, `structured-finding-${index}.json`)
    writeFileSync(path, JSON.stringify(finding))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  })

  const brief = join(directory, 'structured-brief.json')
  writeFileSync(brief, JSON.stringify(structuredBrief()))
  cli(database, ['brief', 'put', '--review', String(reviewId), '--file', brief])

  /*
   * The draft, written in parts. It is what makes step 6 a page with something on it and
   * what makes the code review page's decisions real — a decision changes a block, and a
   * draft of one block of markdown has none.
   *
   * The lint is written with it and left failing, because the body it measured runs over
   * its budget: a review of fourteen findings does. A failing lint and an absent one are
   * different things, and the draft screen says which.
   */
  const sections = join(directory, 'structured-sections.json')
  writeFileSync(sections, JSON.stringify(structuredSections()))
  const put = (lint?: string) =>
    cli(database, [
      'draft',
      'put',
      '--review',
      String(reviewId),
      '--sections',
      sections,
      ...(lint == null ? [] : ['--lint', lint]),
    ])

  /*
   * **Written twice, and the second write is what makes the lint true rather than plausible.**
   * The body is composed by the store from the blocks, so nothing here knows how long it is
   * until it has been written — and a hard-coded `visible_words` is a number that goes stale
   * the first time a block's wording changes. So: write the blocks, read the body back,
   * count it, write the same blocks again with the lint that measured it. Two identical puts
   * compose the same body, which is the whole reason this is safe.
   */
  put()
  const body = String(
    (cliJson(database, ['draft', 'show', '--review', String(reviewId)]) as unknown as {
      draft: { body: string }
    }).draft.body,
  )
  const words = body.split(/\s+/).filter(word => /[a-z0-9]/i.test(word)).length
  const expected = findings.filter(one => one.disposition === 'in_draft').length

  const lint = join(directory, 'structured-lint.json')
  writeFileSync(lint, JSON.stringify({
    /*
     * Failing, and failing on the one thing a review of fourteen findings fails on. A
     * failing lint and an absent one are different things and the draft screen says which,
     * so the suite needs one of each: this draft fails, and `seed()`'s has none at all.
     */
    passes: false,
    visible_words: words,
    word_budget: 420,
    findings_in_body: expected,
    findings_expected: expected,
    raised_before_on_pr: 0,
    errors: `The body runs ${words} words against a budget of 420.`,
  }))
  put(lint)

  const longNote = STRUCTURED_LONG_NOTE

  /*
   * **The facts behind each step's one line**, which is what the card beside it draws.
   * Invented, like everything else in this example. Steps 4 and 7 are given none on
   * purpose: a step with nothing behind it draws no card and no control, and a seed where
   * every step had one could not show that.
   */
  const facts: Record<number, string[]> = {
    1: ['Tests=214 passed', 'Code style=passed', 'Description=passes, 2 warnings'],
    2: ['Problems=3', 'Your sign-off=required', 'Size=486 lines, 12 files'],
    3: [
      'On the ticket=3',
      'From the blind pass=6',
      'On both=1',
      'Built=C',
      'Blind pass picked=C, without the second queue',
    ],
    5: [
      'To post=6: 1 blocker, 4 should, 1 nit',
      'Held for a ticket=5',
      'Set aside=3',
      'Blockers=1: the retry writes the wrong bucket',
    ],
    6: ['In the body=6 findings', 'Length=437 of 420 words'],
  }

  const layers: Array<[number, string, string]> = [
    [1, 'done', 'passes, 2 warnings'],
    [2, 'done', longNote],
    [3, 'done', '8 options, C built'],
    [4, 'done', 'right shape'],
    [5, 'done', `${findings.length} findings, 1 blocker`],
    [6, 'waiting_on_you', 'waiting on you'],
    [7, 'pending', STRUCTURED_STEP_SEVEN_DETAIL],
  ]
  for (const [number, state, detail] of layers) {
    cli(database, [
      'layer',
      'set',
      '--review',
      String(reviewId),
      '--layer',
      String(number),
      '--state',
      state,
      '--detail',
      detail,
      ...(facts[number] ?? []).flatMap(fact => ['--fact', fact]),
    ])
  }

  /*
   * **What the review read the problem from**, which is the third level of "Already
   * decided": kind, reference, the state the session read off the ticket, and one line
   * saying what it tells us. The last row carries a commit id, which is the thing the
   * seven-character rule is for, and the third carries neither a state nor a note — the
   * row that draws its reference alone.
   */
  const sources = join(directory, 'structured-sources.json')
  writeFileSync(sources, JSON.stringify({
    sources: [
      {
        kind: 'ticket',
        label: 'ABC-402 · Scheduled exports carry their own context',
        link: 'https://example.invalid/browse/ABC-402',
        status: 'In Review',
        note: 'The problem, four options, and three open questions',
      },
      {
        kind: 'description',
        label: 'The pull request description, revision 3',
        note: 'Seven sections; two doubts the author raised',
      },
      {
        kind: 'comment',
        label: 'Agreed with you on the ticket on 6 October, before work started',
      },
      {
        kind: 'ticket',
        label: 'ABC-388 · Retire the request-scoped tenant slot',
        link: 'https://example.invalid/browse/ABC-388',
        status: 'Backlog',
        note: 'Names the helper this change deletes, at 3f2b1c4d5e6f708192a3b4c5d6e7f80912345678',
      },
    ],
  }))
  cli(database, ['provenance', 'sources', '--review', String(reviewId), '--file', sources])

  return reviewId
}

/**
 * A review that is **posted and has no draft**, recorded the way an agent records one.
 *
 * `review link --github-review` writes the review's GitHub id with no draft transition —
 * and `/repos` has always counted that as the review no longer being open, so before
 * DEV-930 such a review was in no group on any screen and reachable only by typing its id.
 * That is the complaint the issue was opened about, and this is the record it is about.
 *
 * **It is the only posted review the command line can make.** Approving a draft is the
 * developer's act and happens in the dashboard, and `draft posted` refuses a draft that is
 * not approved — so a seed cannot carry a review posted the ordinary way at all. Nothing in
 * this suite runs `draft posted`: the acceptance path signs a draft off in the browser and
 * stops there, which is where the boundary it exists to test ends.
 *
 * Nothing has harvested it, which is every review until `review-desk harvest` runs: its
 * pull-request state is unknown and its three figures are unmeasured, which is the half of
 * the group's rule that says a nought is not an absence.
 */
/**
 * A sixth review: a review **request**, recorded and never started.
 *
 * `review open` and nothing else — no title, no brief, no finding, no draft — which is
 * what the Locus dispatcher leaves behind when it records a review request on sight, and
 * the shape of the stray review DEV-945 was raised from: `acme/portal #777`, written into
 * the developer's own database by a build session that nobody asked.
 *
 * **It is in nobody's queue**, so it changes none of the counts this suite asserts: there
 * is no brief to confirm, no draft to approve and no outcome, and the groups the queue
 * draws are the developer's rather than an agent's. It is on the Reviews list, where every
 * review is, and it is the only review a delete is offered for.
 */
function seedRequested(database: string): number {
  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '777',
    '--head',
    '4f1a9c2',
    '--ledger',
    'ledger-2026-10-12-777',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }
  return reviewId
}

function seedPosted(directory: string, database: string): number {
  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '405',
    '--head',
    'a9f3b21',
    '--title',
    'Record the delivery attempt count',
    '--ticket',
    'ABC-244',
    '--changed-lines',
    '132',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  const finding = join(directory, 'posted-finding.json')
  writeFileSync(
    finding,
    JSON.stringify({
      lens: 'correctness',
      severity: 'should',
      claim: 'The attempt count is written outside the transaction',
      file: 'jobs/DeliverWebhook.php',
      line: 73,
      disposition: 'in_draft',
    }),
  )
  cli(database, ['finding', 'add', '--review', String(reviewId), '--file', finding])

  /* Posted, with the GitHub review it became. No draft, and that is the point. */
  cli(database, [
    'review',
    'link',
    '--review',
    String(reviewId),
    '--github-review',
    'PRR_kwDOseeded',
  ])

  return reviewId
}

export function seed(
  directory: string,
): {
  database: string
  reviewId: number
  waivedReviewId: number
  structuredReviewId: number
  heldBackReviewId: number
  postedReviewId: number
  requestedReviewId: number
} {
  const database = join(directory, 'seeded.db')

  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '412',
    '--head',
    '3f9c2ab',
    '--title',
    'Retry failed webhook deliveries',
    '--ticket',
    'ABC-231',
    // The ticket's address beside its id, as `contract/examples/getReview.response.json`
    // holds it — so the header on the seeded review draws the id as a link, which is the
    // state DEV-936 is about and the one the screenshots then show.
    '--ticket-url',
    'https://linear.app/acme/issue/ABC-231',
    '--changed-lines',
    '214',
    '--not-checked',
    'The admin screens, which this change does not touch.',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  /*
   * **With its options, which `brief put` does take.** `NewBrief` carries
   * `options: Vec<BriefOption>` and `option_map` is marked superseded; the command's own
   * `--help` still lists the four older fields and says anything else is refused, which is
   * what this seed believed until it was checked. Layer 3 is where a review writes the
   * alternatives it considered before it read the diff, and the One review screen's "Ways
   * to solve it" is the only place they are shown — so a seed without them leaves a panel
   * untested and a screenshot that does not show the screen.
   */
  const brief = join(directory, 'brief.json')
  writeFileSync(
    brief,
    JSON.stringify({
      problem:
        'A webhook delivery that fails once is never tried again, so a customer whose '
        + 'endpoint was briefly down loses the event entirely.',
      diagram: 'flowchart LR\n  A[Event] --> B[Deliver]\n  B -->|fails| C[Dropped]',
      options: [
        {
          key: 'A',
          title: 'Retry inside the request',
          argument_for: 'The smallest change, and no new table.',
          argument_against: 'Holds the request open for as long as the endpoint is down.',
          chosen: false,
        },
        {
          key: 'B',
          title: 'Queue with backoff',
          argument_for: 'Meets the 24-hour window the ticket asks for.',
          argument_against: 'Needs an idempotency key, which is finding F1.',
          chosen: true,
        },
        {
          key: 'C',
          title: 'Dead-letter and replay by hand',
          argument_for: 'Nothing is ever delivered twice.',
          argument_against: 'Somebody has to notice, which is the failure being fixed.',
          chosen: false,
        },
      ],
      approach_verdict: 'B is the right shape for this ticket.',
    }),
  )
  cli(database, ['brief', 'put', '--review', String(reviewId), '--file', brief])

  const findings = [
    {
      lens: 'sibling diff',
      severity: 'blocker',
      claim: 'A retried delivery is sent without an idempotency key.',
      file: 'jobs/DeliverWebhook.php',
      line: 58,
      evidence: 'throwaway test, 2 deliveries for 1 event',
      disposition: 'in_draft',
    },
    {
      lens: 'boundary',
      severity: 'should',
      claim: 'The backoff schedule has no ceiling, so a dead endpoint is retried forever.',
      file: 'jobs/DeliverWebhook.php',
      line: 91,
      disposition: 'in_draft',
    },
    {
      lens: 'repetition',
      severity: 'nit',
      claim: 'Backoff schedule is repeated in two files.',
      disposition: 'held_back',
      disposition_reason: 'a nit; it goes in the ticket',
    },
  ]

  findings.forEach((finding, index) => {
    const path = join(directory, `finding-${index}.json`)
    writeFileSync(path, JSON.stringify(finding))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  })

  const layers: Array<[number, string, string]> = [
    [1, 'done', 'passed'],
    [2, 'waiting_on_you', 'waiting on you'],
    [3, 'done', '3 options'],
    [4, 'done', 'queue with backoff'],
    [5, 'done', '3 findings'],
    [6, 'waiting_on_you', 'waiting on you'],
    [7, 'pending', 'once the session posts it'],
  ]
  for (const [number, state, detail] of layers) {
    cli(database, [
      'layer',
      'set',
      '--review',
      String(reviewId),
      '--layer',
      String(number),
      '--state',
      state,
      '--detail',
      detail,
    ])
  }

  /*
   * The draft, as the session writes it. The body raises the two findings that are in the
   * draft and holds the third back, which is what the lint below reports — and what the
   * Draft preview screen shows as "2 of 2".
   */
  const body = join(directory, 'draft.md')
  writeFileSync(
    body,
    `The queue-with-backoff approach fits the ticket, and the two findings below are
about the retry path rather than the choice.

### Blocker · F1 · A retried delivery is sent without an idempotency key

A second delivery of the same event is indistinguishable from a new one, so a customer
who was briefly down receives it twice.

### Should · F2 · The backoff schedule has no ceiling

A permanently dead endpoint is retried forever, which keeps the queue busy with
deliveries nobody will receive.

<details>
<summary>How I checked</summary>

Ran the job twice against a local endpoint that 500s once, and counted the deliveries.

</details>
`,
  )

  const lint = join(directory, 'lint.json')
  writeFileSync(
    lint,
    JSON.stringify({
      passes: true,
      visible_words: 96,
      word_budget: 150,
      findings_in_body: 2,
      findings_expected: 2,
      raised_before_on_pr: 0,
      checked_at: Math.floor(Date.now() / 1000),
    }),
  )
  cli(database, [
    'draft',
    'put',
    '--review',
    String(reviewId),
    '--file',
    body,
    '--lint',
    lint,
  ])

  /*
   * **A second review whose brief needs no confirmation**, written by the same command
   * line an agent uses. DEV-864 asks for this state end to end, and it is the one place
   * an agent decides a developer step was unnecessary — so the test that it is absent
   * from the Inbox needs a real record to be absent from, not a stub.
   *
   * 61 changed lines, which is why the reason reads the way it does: the 200-line
   * threshold belongs to the caller and Review Desk does not hold it.
   */
  const waivedOpened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '418',
    '--head',
    'bb41e07',
    '--title',
    'Say "not measured" rather than null in the delivery log',
    '--changed-lines',
    '61',
  ])
  const waivedReviewId = Number(waivedOpened.match(/\d+/)?.[0])
  if (!Number.isInteger(waivedReviewId)) {
    throw new Error(`review open printed no id: ${waivedOpened}`)
  }

  const waivedBrief = join(directory, 'waived-brief.json')
  writeFileSync(
    waivedBrief,
    JSON.stringify({
      problem:
        'The delivery log prints `null` for a latency it never measured, which reads as '
        + 'a measurement of zero.',
      diagram: TEXT_SKETCH,
      confirmation_required: false,
      confirmation_not_required_because:
        '61 changed lines, under this caller\'s 200-line threshold',
    }),
  )
  cli(database, [
    'brief',
    'put',
    '--review',
    String(waivedReviewId),
    '--file',
    waivedBrief,
  ])

  return {
    database,
    reviewId,
    waivedReviewId,
    structuredReviewId: seedStructured(directory, database),
    heldBackReviewId: seedHeldBack(directory, database),
    postedReviewId: seedPosted(directory, database),
    requestedReviewId: seedRequested(database),
  }
}

/**
 * The body of that fourth review's draft. One block of prose; it raises the three findings
 * that are in the draft and says nothing about the eight that are not.
 *
 * **It carries a ```` ```mermaid ```` fence, and that is the only drawn diagram in a draft
 * body anywhere in this seed.** DEV-928's sixth criterion asks for the larger view to work
 * "for a diagram in the draft preview and for one on the Understand page", and the two are
 * different paths into the same component: a fence goes through `MarkdownView`, which passes
 * `declared`, and the Understand page's comes from an old brief's `diagram` field, which
 * does not. The seeded review's own body had no fence at all — the drawn diagram in this
 * seed was its brief's — so the browser could not reach the first of the two.
 */
const HELD_BACK_BODY
  = 'Redacting in the writer rather than in the query is the right place for it, and the '
    + 'column list on the definition is the right record. One thing blocks the merge and two '
    + 'should be fixed with it; the rest is a ticket.\n\n'
    + '### Blocker · F2 · The redaction list is read once at boot, not per export\n\n'
    + '`app/Reporting/Redaction.php:24` reads the list at boot, so a column added to it '
    + 'while a worker is up is exported in the clear until that worker restarts.\n\n'
    + '### Should · F1 · The JSON writer does not redact the columns the CSV writer does\n\n'
    + '`app/Reporting/Redaction.php:58` redacts in the CSV writer only. The same export '
    + 'downloaded as JSON carries the personal data the ticket asks to remove.\n\n'
    + '### Should · F3 · The redaction test passes with the redaction reverted\n\n'
    + '`tests/Reporting/RedactionTest.php:31` asserts the value is `null`, which is what an '
    + 'absent column reads as too.\n\n'
    + '```mermaid\n'
    + 'flowchart LR\n'
    + '  A["Schedule runs"] --> B["ColumnPolicy allows"]\n'
    + '  B --> C["ReportBuilder writes"]\n'
    + '  C --> D["CSV writer redacts"]\n'
    + '  C --> E["JSON writer does not"]\n'
    + '  E --> F["Personal data exported"]\n'
    + '```\n\n'
    + '**Not checked:** the warehouse loader, which reads the same definitions and was not '
    + 'changed.'

/**
 * A claim of **exactly** `n` words, opening with something readable about the finding.
 *
 * Padded to the length and asserted on the way in. The panel's defects are properties of the
 * length — 41 to 63 words is what the real review held — so a claim that drifted shorter
 * would leave them untested with every test still passing.
 */
function claimOf(n: number, about: string): string {
  const opening = `The ${about}`.split(/\s+/)
  const padding = Array.from({ length: n - opening.length }, (_, at) => `word${at + 1}`)
  const claim = [...opening, ...padding].join(' ')
  const words = claim.split(/\s+/).length
  if (words !== n) throw new Error(`a claim of ${n} words came out at ${words}`)
  return claim
}

/**
 * The eleven findings of that review: three in the draft and **eight held back**.
 *
 * `seq` is carried only so the order is readable here and the padding can name it; it is
 * stripped before the record is written, because the store assigns the number from the order
 * findings are added. They are added in that order for the same reason.
 *
 * Three of these exist for one acceptance criterion each, and every one of the three is a
 * state no other seeded review can show:
 *
 * | Finding | What it is for |
 * |---|---|
 * | F6 | a claim past 60 words |
 * | F7 | **no title at all**, so the row stands in with twelve words of the claim |
 * | F4 | a title carrying `report_definitions.allowed_columns` |
 */
function heldBackFindings(): Array<Record<string, unknown>> {
  /*
   * One factory. The two severities differed in three fields and repeated eleven, which
   * review pointed out — and the eleven are the shape of a held-back record, so they are
   * the ones that would have to be edited in both places.
   */
  const heldBack = (
    lens: string,
    severity: string,
    reason: string,
  ) => (seq: number, title: string | null, file: string, line: number, words: number, about: string) => ({
    seq,
    lens,
    severity,
    file,
    line,
    claim: claimOf(words, about),
    title,
    disposition: 'held_back',
    disposition_reason: reason,
    verification: 'read_not_run',
  })

  const nit = heldBack('repetition', 'nit', 'a nit; it goes in the ticket')
  const question = heldBack('correctness', 'question', 'a question for the author; it goes in the ticket')

  return [
    {
      seq: 1,
      lens: 'correctness',
      severity: 'should',
      file: 'app/Reporting/Redaction.php',
      line: 58,
      claim:
        'A column named in `redacted_columns` is redacted in the CSV writer and not in the '
        + 'JSON writer, so the same export downloaded in the other format carries the '
        + 'personal data the ticket asks to remove.',
      title: 'The JSON writer does not redact the columns the CSV writer does',
      disposition: 'in_draft',
      verification: 'read_not_run',
    },
    {
      seq: 2,
      lens: 'correctness',
      severity: 'blocker',
      file: 'app/Reporting/Redaction.php',
      line: 24,
      claim:
        'The redaction list is read once at boot, so a column added to it while a '
        + 'long-running worker is up is not redacted until that worker is restarted.',
      title: 'The redaction list is read once at boot, not per export',
      disposition: 'in_draft',
      verification: 'read_not_run',
    },
    {
      seq: 3,
      lens: 'tests',
      severity: 'should',
      file: 'tests/Reporting/RedactionTest.php',
      line: 31,
      claim:
        'The test asserts the redacted value is `null`, which is what an absent column also '
        + 'reads as, so it passes with the redaction reverted.',
      title: 'The redaction test passes with the redaction reverted',
      disposition: 'in_draft',
      verification: 'read_not_run',
    },
    nit(
      4,
      '`report_definitions.allowed_columns` is read twice, in two orders',
      'app/Reporting/ColumnPolicy.php',
      44,
      57,
      'list in `report_definitions.allowed_columns` is read twice in this class, once to '
      + 'decide whether a column may be exported and once to decide whether it has to be '
      + 'redacted, and the two reads sort it differently, so a reader comparing them has to '
      + 'work out that the order carries no meaning in either place and that',
    ),
    nit(
      5,
      'The class is named for the smaller half of what it does',
      'app/Reporting/Redaction.php',
      12,
      44,
      'class is called `Redaction` and three of its five public methods are about reading '
      + 'the policy rather than about redacting anything, so the name describes the smaller '
      + 'half of what is in the file and',
    ),
    nit(
      6,
      'The address pattern is written twice, and the two have drifted',
      'app/Reporting/ExportPipeline.php',
      96,
      61,
      'pattern list that decides whether a value looks like an email address is written out '
      + 'here and again in `app/Audit/RecordsAnExport.php`, and the two have already drifted: '
      + 'this one accepts an address with no dot in the domain and the other does not, so the '
      + 'same value is redacted in the export and printed in the audit entry about it, which',
    ),
    nit(
      7,
      null,
      'app/Reporting/ReportBuilder.php',
      140,
      50,
      'comment above this block says the redaction happens after the column policy is '
      + 'applied, which was true before the policy moved into its own class in this change '
      + 'and is now the opposite way round, so the one line of prose in the file describes',
    ),
    nit(
      8,
      'The builder method this change replaced has no caller left',
      'app/Reporting/ExportContext.php',
      71,
      41,
      '`withRedaction()` builder method has no caller left after this change, because the '
      + 'pipeline reads the policy for itself now, and it is still public with nothing saying',
    ),
    nit(
      9,
      'The redaction default is a string where every reader wants a boolean',
      'config/exports.php',
      18,
      63,
      'default for `exports.redaction.enabled` is written as the string `"true"` rather than '
      + 'as a boolean, and every reader of it compares with `===` against a boolean, so the '
      + 'default is falsy everywhere it is read and the behaviour comes from the environment '
      + 'file instead of from the configuration the ticket asked to add, which means',
    ),
    question(
      10,
      'What an existing schedule with no policy does is not established',
      'app/Jobs/RunScheduledExport.php',
      118,
      54,
      'schedule written before this change names no redaction policy at all, and nothing in '
      + 'the diff says what one of those should do when it next runs: whether it redacts '
      + 'nothing, as it did yesterday, or everything the new default names, which would',
    ),
    question(
      11,
      'Whether the per-row policy read is paid once or per row is unsaid',
      'app/Reporting/ColumnPolicy.php',
      88,
      59,
      'policy is resolved per row rather than per export, which is a database read for every '
      + 'row of a file that runs to tens of thousands of them on the largest tenant, and '
      + 'whether that is paid once from a warm cache or a hundred thousand times is not',
    ),
  ]
}

/**
 * A fourth review, whose aside is the one DEV-928 is about: eight findings held back.
 *
 * **Every number here is measured rather than described.** The claims are padded to a word
 * count and the count is asserted on the way in, because "about sixty words" is what the
 * example had before this and it measured nothing: the panel's defects are all properties
 * of the length, so a seed that drifted shorter would leave them untested while every test
 * still passed.
 *
 * Three states the other four reviews cannot show, and each is one of the issue's criteria:
 * a claim past 60 words, a title carrying a long code name, and a finding with **no title at
 * all**, whose row has to stand in with the claim's first twelve words and say that it did.
 */
function seedHeldBack(directory: string, database: string): number {
  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '519',
    '--head',
    '7b31fa0',
    '--title',
    'Redact personal columns from scheduled exports',
    '--ticket',
    'ABC-410',
    '--changed-lines',
    '318',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  for (const finding of heldBackFindings()) {
    const path = join(directory, `held-back-finding-${finding.seq}.json`)
    const { seq: _seq, ...written } = finding
    writeFileSync(path, JSON.stringify(written))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  }

  /*
   * The draft, as one block of prose. It raises the three findings that are in it and holds
   * the other eight back, which is what this review is seeded to show — the body is not the
   * point here, and `draft put --sections` composes it so nothing states it twice.
   */
  const sections = join(directory, 'held-back-sections.json')
  writeFileSync(sections, JSON.stringify([{ kind: 'prose', text: HELD_BACK_BODY }]))
  cli(database, ['draft', 'put', '--review', String(reviewId), '--sections', sections])

  /*
   * **Five edits on one finding, which is the third defect.** The real review's own shape:
   * kept, kept, kept, cut, kept, inside twelve seconds. Four of them are recorded with no
   * detail, and `wire::draft_edit` writes the kind into `summary` when there is none — so
   * the panel printed "Kept F1 · kept" four times over. Seeded through `outcome add`,
   * because the developer's edits are outcomes with actor `developer` and that is the only
   * way to put one in the record from outside the dashboard.
   */
  const at = Math.floor(Date.parse('2026-10-11T08:03:00Z') / 1000)
  const edits: Array<[string, string | null]> = [
    ['kept', null],
    ['kept', null],
    ['kept', null],
    ['cut', 'the second sentence'],
    ['kept', null],
  ]
  edits.forEach(([kind, detail], index) => {
    cli(database, [
      'outcome',
      'add',
      '--review',
      String(reviewId),
      '--finding',
      '1',
      '--kind',
      kind,
      '--by',
      'developer',
      '--at',
      String(at + index * 3),
      ...(detail == null ? [] : ['--detail', detail]),
    ])
  })

  /* Step 6 waiting on the developer, so the queue's age cell has a time to read. */
  for (const [number, state, detail] of [
    [1, 'done', 'passed'],
    [2, 'done', 'confirmation not required: the ticket names the columns'],
    [3, 'done', '2 options, one built'],
    [4, 'done', 'done'],
    [5, 'done', '11 findings, 8 of them held for a ticket'],
    [6, 'waiting_on_you', 'waiting on you'],
  ] as Array<[number, string, string]>) {
    cli(database, [
      'layer',
      'set',
      '--review',
      String(reviewId),
      '--layer',
      String(number),
      '--state',
      state,
      '--detail',
      detail,
    ])
  }

  return reviewId
}

/** A database with nothing in it: what the first `review-desk serve` serves. */
export function fresh(directory: string): string {
  const database = join(directory, 'fresh.db')
  /* Created and migrated without writing a record, which is what `db version` is for. */
  cli(database, ['db', 'version'])
  return database
}

/**
 * Start the binary on a free port and wait until it answers.
 *
 * Port 0, so two servers and two test runs cannot collide, and the URL comes from the
 * binary's own `--json` rather than from a guess.
 */
export async function serve(database: string): Promise<Serving> {
  const child = spawn(BINARY, ['serve', '--port', '0', '--json'], {
    env: { ...process.env, REVIEW_DESK_DB: database },
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  /*
   * **Both pipes are drained and kept, which they were not.**
   *
   * A CI run of this suite failed with three asset requests dying at the transport level
   * and no API request made at all: the server had stopped answering partway through a
   * page load, and there was nothing to say why, because its stderr went into a pipe
   * nothing read. A test harness that discards the one artefact that would explain a death
   * turns a diagnosable failure into a ten-second timeout on a blank page.
   *
   * Draining matters on its own, too. An unread pipe fills, and a child that blocks writing
   * to it stops serving — which is the shape of the bug `fdb0b40` fixed in the serve test
   * for DEV-861, where the test killed the server it had started.
   */
  let printed = ''
  child.stdout?.on('data', (chunk: Buffer) => {
    printed += chunk.toString()
  })
  child.stderr?.on('data', (chunk: Buffer) => {
    printed += chunk.toString()
  })

  const said = () => printed.trim()

  /*
   * **An exit is noticed and remembered**, so that a server which dies mid-run fails the
   * next thing that touches it with its own words rather than with a timeout.
   */
  let died: string | null = null
  child.on('exit', (code, signal) => {
    died = `the server exited (code ${code}, signal ${signal}). It said:\n${said()}`
  })

  const url = await new Promise<string>((done, failed) => {
    /*
     * Polled rather than matched on every chunk. The regular expression runs over
     * everything printed so far, and the server prints for the life of the run — matching
     * per chunk would re-scan an ever-growing string for an address found in its first line.
     */
    const looking = setInterval(() => {
      const match = printed.match(/"url":"([^"]+)"/)
      if (match) {
        clearInterval(looking)
        clearTimeout(giveUp)
        done(match[1]!)
      }
      else if (died) {
        clearInterval(looking)
        clearTimeout(giveUp)
        failed(new Error(`${died}`))
      }
    }, 50)

    const giveUp = setTimeout(() => {
      clearInterval(looking)
      failed(new Error(`the server printed no address in 20s. It said:\n${said()}`))
    }, 20_000)

    child.on('error', failed)
  })

  /*
   * Answering, not merely started. `/health` is the server saying its database is open and
   * migrated, which is what the first screen read needs to be true.
   */
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (died) throw new Error(died)
    try {
      const answer = await fetch(`${url}api/v1/health`)
      if (answer.ok) return { url, database, process: child, said }
    }
    catch {
      /* Not up yet. */
    }
    await new Promise(wait => setTimeout(wait, 100))
  }

  child.kill()
  throw new Error(`${url} never answered its health endpoint. It said:\n${said()}`)
}

/**
 * The servers this process started, for the checks below.
 *
 * Each spec file runs in its own process and the servers were started in `globalSetup`'s,
 * so a spec cannot hold their handles. What it can do is ask whether the addresses still
 * answer, which is what `stillServing` does when it has no handle to inspect.
 */
export function serversUnderTest(): Array<{ url: string }> {
  const addresses = readAddresses()
  return [{ url: addresses.seeded }, { url: addresses.empty }]
}

/**
 * Fail now, with the server's own words, if either server has died.
 *
 * Called at the top of each spec. A dead server otherwise shows up as a browser timing out
 * on a blank page ten seconds later, which says nothing about what happened and points at
 * the assertion rather than at the cause.
 */
export function stillServing(servers: Array<{ url: string }>): void {
  for (const server of servers) {
    const answered = spawnSync(
      'curl',
      ['-s', '-o', '/dev/null', '-m', '5', '-w', '%{http_code}', `${server.url}api/v1/health`],
      { encoding: 'utf8' },
    )
    if (answered.stdout.trim() !== '200') {
      throw new Error(
        `${server.url} is not answering its health endpoint (curl said `
        + `${answered.stdout.trim() || 'nothing'}). A server under test has died; the run's `
        + 'log carries what it printed.',
      )
    }
  }
}

/** Stop a server, and wait for it to be gone. */
export async function stop(serving: Serving): Promise<void> {
  if (serving.process.exitCode != null) return
  const ended = new Promise<void>(done => serving.process.once('exit', () => done()))
  serving.process.kill('SIGTERM')
  await Promise.race([ended, new Promise(wait => setTimeout(wait, 5_000))])
  if (serving.process.exitCode == null) serving.process.kill('SIGKILL')
}

/**
 * A review with one finding and a draft ready for the developer, opened on the fly.
 *
 * **Not part of `seed()`, and that is the point.** Everything the acceptance suite asserts
 * on the seeded database includes how many reviews the queue is waiting on, so a review
 * added to the seed to test one screen changes the assertions and the pictures of four
 * others. The two callers — the draft page's sign-off block and its screenshots — open
 * their own and name their own pull request numbers.
 *
 * **`--sections` rather than `--file`.** The store composes the body from the blocks, which
 * is what a session writes since DEV-929 and is what keeps `body_is_stale` false; a draft
 * written as one block of markdown would do as well for the test and worse as a picture.
 *
 * It stops at `ready`. Signing off is the developer's act and there is no command for it,
 * which is the boundary the whole of this file is arranged around.
 *
 * **It finds its own pull request number, by asking the record.** Picking one by hand was
 * wrong twice over. A number the seed already uses opens a *round two* of that review —
 * `review open` on a pull request reviewed at another head starts the next round, which
 * carries the earlier round's findings forward and moves `rounds_on_pr` on a review some
 * other test asserts about; the only symptom was "draft section refers to finding 6/1,
 * which does not exist", naming neither the pull request nor the round. The *same* number
 * twice hands back the same review and a refused `draft put`, which is what
 * `--repeat-each` hit.
 *
 * A counter in this module does not fix the second one, because the runner reloads the
 * module between repeats and the counter starts again. So it asks the record instead,
 * from 900 upward — well clear of the 405–519 the seeds use — for the first pull request
 * nothing has reviewed. `review find` is the read that answers it.
 *
 * **What that buys is repeating the block, not repeating the run.** Every review this
 * makes is left on the shared database, and one of them is marked posted — which is the
 * predicate of `posted_reviews_awaiting_author`, so it becomes a "Waiting on the author"
 * row that the queue counts. Two consequences, and the second is the one that bites:
 *
 * - `--repeat-each` over the whole of `review-desk.spec.ts` replays the queue assertions
 *   above this block against a database that has grown, and they would be right to fail.
 *   Within one run it costs nothing, because the block is last.
 * - **The two Playwright projects share the database**, so a bare `playwright test` with
 *   no `--project` runs the acceptance suite and then the screenshots over a desk with
 *   four reviews on it that were not there, and regenerates committed queue pictures with
 *   rows nobody added. `pnpm run e2e` and `pnpm run e2e:screenshots` each name a project
 *   and reseed, which is why they are the documented commands; this is the reason they
 *   are not interchangeable with running the lot.
 */
const FIRST_PULL_REQUEST = 900
const LAST_PULL_REQUEST = FIRST_PULL_REQUEST + 50

/**
 * Open a review on the first pull request in that range that nothing has reviewed.
 *
 * **The search is a read, and getting there took three goes.** Probing with `review open`
 * and the title rewrote the title of whatever review was already on each number it
 * rejected — a finished test's record changed underneath it. Probing with `review open`
 * and the three identifying fields alone wrote nothing, but still *opened* a review on
 * every number it tried and cost two process spawns apiece. `review find` answers the same
 * question in one spawn and writes nothing at all: a list of that pull request's reviews,
 * newest round first, and an empty list for one nobody has reviewed.
 *
 * Spawns are what this costs — 15.7 ms each, measured over ten runs against a scratch
 * database — so the difference is small and the write hazard is retired rather than
 * stepped around.
 */
function openAFreshReview(
  database: string,
  title: string,
): { reviewId: number, pr: number } {
  for (let pr = FIRST_PULL_REQUEST; pr <= LAST_PULL_REQUEST; pr += 1) {
    const { reviews } = cliJson(database, [
      'review',
      'find',
      '--repo',
      'acme/portal',
      '--pr',
      String(pr),
    ]) as unknown as { reviews: unknown[] }
    if (reviews.length > 0) continue

    const opened = cli(database, [
      'review',
      'open',
      '--repo',
      'acme/portal',
      '--pr',
      String(pr),
      '--head',
      'b4d9e01',
      '--title',
      title,
      '--ticket',
      'ABC-251',
      '--changed-lines',
      '64',
    ])
    const reviewId = Number(opened.match(/\d+/)?.[0])
    if (!Number.isInteger(reviewId)) {
      throw new Error(`review open printed no id: ${opened}`)
    }
    return { reviewId, pr }
  }

  throw new Error(
    `every pull request from ${FIRST_PULL_REQUEST} to ${LAST_PULL_REQUEST} on this `
    + 'database is already reviewed. Something is seeding in this range, or a test is '
    + 'asking for far more reviews than this helper was meant to make.',
  )
}

export function seedReadyDraft(
  database: string,
  title: string,
): { reviewId: number, pr: number } {
  const directory = scratch()
  const { reviewId, pr } = openAFreshReview(database, title)

  /*
   * **The session that ran it, which a real review has and `review open` does not set.**
   * `Review.session_id` is nullable and the draft page says different things with and
   * without it — it will not tell a developer to go and look at a session the record
   * never recorded. A seed that left it null would have the browser checks asserting the
   * wording for the rarer case.
   */
  cli(database, ['review', 'link', '--review', String(reviewId), '--session', 'ses-dev-932'])

  const finding = join(directory, 'posting-finding.json')
  writeFileSync(
    finding,
    JSON.stringify({
      lens: 'correctness',
      severity: 'should',
      claim: 'The attempt count is written outside the transaction',
      title: 'The attempt count is written outside the transaction',
      file: 'app/Jobs/DeliverWebhook.php',
      line: 73,
      disposition: 'in_draft',
    }),
  )
  /*
   * The number the store gave it, not a 1. A block about a finding that is not there is
   * refused, and the refusal names a sequence number rather than the reason — so reading
   * it back is a line here instead of a puzzle wherever this is called from.
   */
  const seq = (
    cliJson(database, [
      'finding',
      'add',
      '--review',
      String(reviewId),
      '--file',
      finding,
    ]) as unknown as { finding: { seq: number } }
  ).finding.seq

  const sections = join(directory, 'posting-sections.json')
  writeFileSync(
    sections,
    JSON.stringify([
      {
        kind: 'prose',
        text:
          '**1 Should — 1 open.** The retry carries a key now, which is the right shape. '
          + 'One finding, about where the attempt count is written.',
      },
      {
        kind: 'finding',
        finding_seq: seq,
        text:
          'A delivery that fails after the increment leaves the count ahead of what was '
          + 'actually attempted, and the audit row is written from the count.',
      },
    ]),
  )
  cli(database, ['draft', 'put', '--review', String(reviewId), '--sections', sections])

  return { reviewId, pr }
}

/** The draft's two stamps, as the record holds them: whole seconds, or null. */
export function draftStamps(
  database: string,
  reviewId: number,
): { approvedAt: number | null, postedAt: number | null } {
  const { draft } = cliJson(database, [
    'draft',
    'show',
    '--review',
    String(reviewId),
  ]) as unknown as { draft: { approved_at: number | null, posted_at: number | null } }
  return { approvedAt: draft.approved_at, postedAt: draft.posted_at }
}

/**
 * Stop the browser's clock a stated number of seconds after a stamp the record holds.
 *
 * **Pinned to the record, never to a wall clock.** The draft page words its figures from
 * `approved_at` and `posted_at`, so a test that moves the browser's clock by a *duration*
 * is asserting on 45 seconds plus however long the sign-off itself took — which is 45
 * until the wording crosses a minute and becomes "1 min", and then it is a capture group
 * that does not match. Pinning to the stamp makes the figure the one the test names, on
 * any machine, however slow.
 *
 * `pauseAt` rather than `fastForward`: the jump fires each due timer once and then nothing
 * moves again, which is what a picture wants and what an assertion about one figure needs.
 * The clock must already be installed; `page.clock.resume()` starts it going again.
 */
export async function pinClockAt(
  page: Page,
  stamp: number | null,
  secondsAfter: number,
): Promise<void> {
  if (stamp == null) {
    throw new Error('the record holds no stamp to pin the clock to')
  }
  await page.clock.pauseAt(new Date((stamp + secondsAfter) * 1000))
}
