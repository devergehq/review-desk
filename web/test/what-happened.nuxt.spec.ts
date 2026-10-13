// @vitest-environment nuxt
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import WhatHappened from '../app/pages/reviews/[id]/what-happened.vue'

import type {
  Finding,
  NotForSigning,
  Outcome,
  PrState,
  ReviewDetail,
  RoundSummary,
  Severity,
} from '../app/types/desk'

/*
 * **Step 7's page — "What happened". DEV-958.**
 *
 * ## The density review is built here, not added to `app/data/reviews.json`
 *
 * The issue asks for fixtures at real density: 14 findings, 2 replies, 7 code changes, one
 * reply over 80 words. A review of that size added to the sample records would move things
 * this issue does not touch — `Store::reviews_with_author_activity` has no `pr_state`
 * filter, so any review with a harvested outcome is in the queue's "Author replied" group,
 * and that is a row, a count in `totals`, and the Inbox lengths four other specs assert.
 * The same mistake has been made in this repository before and six assertions said so.
 *
 * `resetDesk({ reviews })` takes any list, so the density lives in this file and is read by
 * the one page it is about. The second measurement the issue asks for — a browser at 1440
 * and 390 — is in `e2e/what-happened.spec.ts`, which seeds a database and a server of its
 * own for exactly the same reason `e2e/header-links.spec.ts` does.
 *
 * ## What is checked here
 *
 * That the page renders from the record, that each state the record can be in has its own
 * sentence, and that the rules DEV-958 states hold — a row per posted finding, outcomes in
 * time order, a finding nothing came back on said rather than left blank, the two links,
 * and no colour that grades. It does not check that it looks right; the browser does that.
 */

const REVIEW = 50

const STILL_READING = /Reading [^…]*…/

/** Let every pending read finish. The shape the other page specs argue for. */
async function settle(page: VueWrapper) {
  let quiet = 0
  for (let round = 0; round < 200; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
    await page.vm.$nextTick()
    if (STILL_READING.test(page.text())) {
      quiet = 0
      continue
    }
    quiet += 1
    if (quiet >= 2) return
  }
}

async function shown(id: number) {
  const page = await mountSuspended(WhatHappened, {
    route: `/reviews/${id}/what-happened`,
    attachTo: document.body,
  })
  await settle(page)
  return page
}

/* ============================ The density review ============================= */

/**
 * The reply DEV-958 asks for: **108 words**, which is over its 80.
 *
 * Invented, and written at the length an author really answers a blocker at. A short reply
 * is a length every one of these screens survives, so it measures nothing: what this is for
 * is the quotation block at 390 pixels, where the page is measured.
 */
const LONG_REPLY
  = 'I pushed a fix for this one but I want to flag that it is not the fix you suggested. '
    + 'Taking the column policy out of the query builder means the scheduled export and the '
    + 'interactive export stop sharing it, and the interactive one is the path with the test '
    + 'coverage, so I would rather keep them together and gate the scheduled path on the '
    + 'same policy object. That is what the new commit does. If you still want them split I '
    + 'can do it, but it should be its own change with its own tests rather than something I '
    + 'slip into a fix for a reporting bug.'

const SHORT_REPLY = 'Moved to a temporary name and renamed on success.'

/** Which severity each of the fourteen carries. The shape a real review of this size has. */
const SEVERITIES: Severity[] = [
  'blocker',
  'should', 'should', 'should', 'should', 'should', 'should',
  'question', 'question', 'question',
  'nit', 'nit', 'nit', 'nit',
]

/** The paths the findings sit on, so a row's `path:line` is a plausible one. */
const FILES = [
  'app/Reporting/ColumnPolicy.php',
  'app/Reporting/ScheduledExport.php',
  'app/Reporting/ExportContext.php',
  'app/Http/Controllers/ExportController.php',
  'jobs/ExportTenant.php',
  'app/Reporting/QueryBuilder.php',
  'app/Reporting/Columns.php',
]

function finding(seq: number): Finding {
  return {
    review_id: REVIEW,
    seq,
    key: `F${seq}`,
    lens: 'correctness',
    severity: SEVERITIES[seq - 1]!,
    file: FILES[(seq - 1) % FILES.length]!,
    line: 40 + seq * 7,
    claim: `What F${seq} says is wrong, written at the length a session really writes a `
      + 'claim at, which is a sentence or two and well past the twelve words a row has '
      + 'room for.',
    title: seq === 4 ? null : `What F${seq} says is wrong`,
    disposition: 'in_draft',
    round_status: 'new',
    verification: 'unknown',
    fix_references: [],
    /*
     * GitHub's own numeric comment id, which is what `commentUrl` needs and what the
     * harvester requires: it matches a finding to a thread by comparing this to a thread's
     * `root_comment_id`, read from GraphQL's `databaseId`.
     *
     * **F13 and F14 carry none**, which is the ordinary state: a finding the review raised
     * in its body alone has no comment, so no reply could be read on it and the page's
     * footnote has something to be about.
     */
    github_comment_id: seq >= 13 ? null : String(2_314_990_000 + seq),
    created_at: '2026-10-09T07:00:00Z',
  } as unknown as Finding
}

let nextOutcome = 900

function outcome(over: Partial<Outcome> & Pick<Outcome, 'finding_seq' | 'kind'>): Outcome {
  nextOutcome += 1
  return {
    id: nextOutcome,
    review_id: REVIEW,
    finding_key: `F${over.finding_seq}`,
    actor: 'author',
    occurred_at: '2026-10-11T09:00:00Z',
    detail: null,
    ...over,
  } as Outcome
}

/** A `replied` detail in the shape `crates/harvest/src/run.rs` writes it. */
function repliedDetail(seq: number, body: string): string {
  return JSON.stringify({
    comment_id: 2_315_000_000 + seq,
    in_reply_to: 2_314_990_000 + seq,
    login: 'dana-okoye',
    body,
    time_basis: 'github',
  })
}

/** A `code_changed_after` detail in the same shape. */
function changedDetail(seq: number, linesChanged: boolean): string {
  return JSON.stringify({
    file: FILES[(seq - 1) % FILES.length],
    line: 40 + seq * 7,
    lines_changed: linesChanged,
    file_status: 'modified',
    commit: `65eff63${seq}a04f7b9c0d8e5a1b2c3d4e5f607182`,
    basis: 'compare 633e04a...65eff63',
  })
}

/**
 * The density the issue names: **14 findings, 2 replies, 7 code changes.**
 *
 * The two replies are on F1 and F2 — F1's is the 108-word one. F1 also has its thread
 * resolved and its code changed, which is the "a finding with several outcomes" case, and
 * its three are deliberately **written out of time order** so that the sort is doing work.
 */
const OUTCOMES: Outcome[] = [
  /* F1: three outcomes, newest written first. */
  outcome({
    finding_seq: 1,
    kind: 'code_changed_after',
    actor: 'harvester',
    occurred_at: '2026-10-12T16:40:00Z',
    detail: changedDetail(1, true),
  }),
  outcome({
    finding_seq: 1,
    kind: 'thread_resolved',
    occurred_at: '2026-10-12T09:15:00Z',
    detail: JSON.stringify({
      thread: 2_314_990_001,
      resolved_by: 'dana-okoye',
      time_basis: 'observed',
    }),
  }),
  outcome({
    finding_seq: 1,
    kind: 'replied',
    occurred_at: '2026-10-11T11:02:00Z',
    detail: repliedDetail(1, LONG_REPLY),
  }),

  /* F2: the second reply. */
  outcome({
    finding_seq: 2,
    kind: 'replied',
    occurred_at: '2026-10-11T11:30:00Z',
    detail: repliedDetail(2, SHORT_REPLY),
  }),

  /* Six more findings whose code changed: seven in all with F1's. */
  ...[2, 3, 5, 6, 8, 11].map((seq, at) =>
    outcome({
      finding_seq: seq,
      kind: 'code_changed_after',
      actor: 'harvester',
      occurred_at: `2026-10-12T1${at}:00:00Z`,
      /* F6's file changed and its own lines did not, which is its own sentence. */
      detail: changedDetail(seq, seq !== 6),
    }),
  ),

  /* A decision of the developer's own, which this page does not draw. */
  outcome({
    finding_seq: 12,
    kind: 'cut',
    actor: 'developer',
    occurred_at: '2026-10-10T08:00:00Z',
    detail: 'formatting nit',
  }),
]

function layers() {
  return Array.from({ length: 7 }, (_, at) => ({
    review_id: REVIEW,
    number: at + 1,
    name: (['lint', 'understand', 'alternatives', 'solution_map', 'code_review', 'draft',
      'harvest'] as const)[at],
    state: at === 6 ? 'running' : 'done',
    detail: at === 6 ? '2 replies back' : 'done',
    updated_at: '2026-10-12T17:00:00Z',
    runs: [],
    seconds: null,
    facts: [],
  }))
}

const ROUNDS: RoundSummary[] = [
  {
    review_id: 12,
    round: 1,
    head_sha: '633e04a',
    created_at: '2026-10-06T11:00:00Z',
    posted_at: '2026-10-07T09:00:00Z',
    event: 'request_changes',
    findings: 11,
    commits: 4,
    replies: 3,
    threads_resolved: 2,
    statuses: { new: 11, fixed: 0, still_open: 0, accepted: 0, withdrawn: 0, ticketed: 0 },
  },
  {
    review_id: REVIEW,
    round: 2,
    head_sha: '65eff63',
    created_at: '2026-10-09T07:00:00Z',
    posted_at: '2026-10-09T18:00:00Z',
    event: 'request_changes',
    findings: 14,
    commits: 7,
    replies: 2,
    threads_resolved: 1,
    statuses: { new: 10, fixed: 2, still_open: 1, accepted: 1, withdrawn: 0, ticketed: 0 },
  },
]

/**
 * When the pull request this fixture describes merged or closed.
 *
 * **Local noon, not an instant written out as UTC.** `dayLabel` words the developer's own
 * day, so `2026-10-11T22:41:00Z` is the 11th in London and the 12th in Sydney and a test
 * naming either is a test that fails on somebody else's machine. Noon local is the same
 * calendar day in every zone, which lets the headline be asserted whole rather than as a
 * pattern.
 */
const SETTLED_AT = new Date(2026, 9, 11, 12, 0, 0).toISOString()
const SETTLED_DAY = '11 October 2026'

/**
 * The density review. `over` replaces any part of it, which is how the states below differ.
 *
 * **`pr_state_at` and `not_for_signing` follow `pr_state` unless a caller says otherwise,
 * because a record where they disagree is not a record the server can produce.** A merged
 * pull request has a day it merged and is refused for signing with `reason: finished`; an
 * open one has neither. A fixture that set `pr_state: 'merged'` and left the other two at
 * their open values would have the page's headline drawing a merge with no day — the state
 * this function is *least* able to tell apart from the real "GitHub gave no time for it",
 * which is a case with its own test below.
 *
 * **And it is not cast any more** — it returned `as unknown as ReviewDetail`, which is how
 * `not_for_signing` came to be a required field of the contract that this fixture did not
 * carry, with every check green.
 *
 * **Dropping the cast is not what catches the next one, and this was measured rather than
 * assumed.** Deleting `not_for_signing` from the object below and running `pnpm typecheck`
 * exits 0: Nuxt's app tsconfig includes `test/nuxt/**`, and every spec in this repository
 * is `test/*.spec.ts`, so none of them is typechecked at all. The cast is gone because an
 * unchecked cast is a claim nobody can read, not because removing it checks anything. What
 * checks it is the test directly below, which reads the contract's own `required` list.
 */
function dense(over: {
  prState?: PrState | null
  prStateAt?: string | null
  notForSigning?: NotForSigning | null
  harvestedAt?: string | null
  outcomes?: Outcome[]
  round?: number
  rounds?: RoundSummary[] | undefined
  findings?: Finding[]
} = {}): ReviewDetail {
  const findings = over.findings ?? Array.from({ length: 14 }, (_, at) => finding(at + 1))
  const state = over.prState === undefined ? 'open' : over.prState
  const settled = state === 'merged' || state === 'closed'
  const stateAt = over.prStateAt === undefined ? (settled ? SETTLED_AT : null) : over.prStateAt
  return {
    review: {
      id: REVIEW,
      repo: 'acme/portal',
      pr: 601,
      head_sha: '65eff63',
      round: over.round ?? 1,
      title: 'Give a scheduled export its own context',
      ticket_id: 'ABC-402',
      ticket_url: null,
      changed_lines: 412,
      session_id: 'ses_dense',
      ledger_id: null,
      github_review_id: 'PRR_kwDODense',
      created_at: '2026-10-09T07:00:00Z',
      pr_state: state,
      pr_state_at: stateAt,
      harvested_at: over.harvestedAt === undefined ? '2026-10-12T17:00:00Z' : over.harvestedAt,
      dismissed_at: null,
      dismissed_reason: null,
    },
    not_for_signing: over.notForSigning === undefined
      ? (settled ? { reason: 'finished', how: state, at: stateAt } : null)
      : over.notForSigning,
    layers: layers(),
    brief: null,
    findings,
    outcomes: over.outcomes ?? OUTCOMES,
    finding_severities: { blocker: 1, should: 6, nit: 4, question: 3 },
    finding_dispositions: { in_draft: 14, held_back: 0, suppressed: 0 },
    not_checked: null,
    nit_ticket: { review_id: REVIEW, status: 'none', finding_seqs: [] },
    draft_status: 'posted',
    sources: [],
    criteria: [],
    description_lint: null,
    coverage: { source: 'none', files: [] },
    timings: {},
    rounds: over.rounds,
    commits_since_previous_round: [],
  }
}

/** The density review, plus whatever the sample records already hold. */
async function withDense(detail: ReviewDetail = dense()) {
  await resetDesk({ reviews: [...loadSource().reviews, detail] })
}

/* ====================== The fixture against the contract ====================== */

/**
 * **Every field the contract requires, checked against the contract itself.**
 *
 * `not_for_signing` was added to `ReviewDetail` as a required field by DEV-957 and this
 * fixture did not carry it, with lint, typecheck, 1,097 unit tests and 91 browser tests
 * green — because `dense()` was cast, and because no spec in this repository is
 * typechecked (Nuxt's app tsconfig includes `test/nuxt/**`, and these specs are
 * `test/*.spec.ts`). A fixture short of a field the server always sends is a page measured
 * against a record that cannot arrive, and the field it is short of is invisible.
 *
 * So the check reads the `required` list out of `contract/openapi.yaml` — the document the
 * served type is generated from — rather than naming the fields here, which would be the
 * same list going stale in a second place. The next required field added to either schema
 * fails this spec until the fixture carries it.
 */
describe('the density fixture against the contract', () => {
  const YAML = readFileSync(resolve('../contract/openapi.yaml'), 'utf8').split('\n')

  /**
   * One schema's `required` names, in either of the two shapes this document writes them:
   * a block list of `- name` lines, or a bracketed list that may run over several lines.
   */
  function requiredOf(schema: string): string[] {
    const at = YAML.findIndex(line => line === `    ${schema}:`)
    expect(at, `${schema} is a schema in the contract`).toBeGreaterThan(-1)

    const body: string[] = []
    for (const line of YAML.slice(at + 1)) {
      if (/^ {4}\S/.test(line)) break
      body.push(line)
    }

    const key = body.findIndex(line => /^ {6}required:/.test(line))
    expect(key, `${schema} declares required fields`).toBeGreaterThan(-1)

    /*
     * Everything indented past the key, whichever shape it is in: the `- name` lines of a
     * block list sit at eight spaces and a bracketed list's continuation at nine, while
     * the next key of the schema — `description`, `properties` — sits at six and ends it.
     * The first draft of this branched on the bracket and read `Review` as an empty list,
     * which the test above caught before either assertion below could pass on nothing.
     */
    const tail: string[] = []
    for (const line of body.slice(key + 1)) {
      if (/^ {6}\S/.test(line)) break
      tail.push(line)
    }

    const text = [body[key]!.replace(/^ {6}required:/, ''), ...tail].join(' ')
    return text.match(/[a-z_]+/g) ?? []
  }

  /*
   * **The parser is checked before it is trusted.** `arrayContaining([])` passes over any
   * object at all, so a `requiredOf` that quietly returned nothing would make both tests
   * below green against a fixture missing every field — which is the shape of the three
   * checks on this branch that proved nothing until the defect was put back.
   */
  it('reads a real list out of the contract, in both the shapes it is written in', () => {
    expect(requiredOf('ReviewDetail').length).toBeGreaterThan(10)
    expect(requiredOf('Review').length).toBeGreaterThan(10)
    expect(requiredOf('ReviewDetail')).not.toContain('brief')
  })

  it('carries every field ReviewDetail requires, not_for_signing among them', () => {
    const names = requiredOf('ReviewDetail')
    expect(names).toContain('not_for_signing')
    expect(Object.keys(dense())).toEqual(expect.arrayContaining(names))
  })

  it('carries every field Review requires, pr_state_at among them', () => {
    const names = requiredOf('Review')
    expect(names).toContain('pr_state_at')
    expect(Object.keys(dense().review)).toEqual(expect.arrayContaining(names))
  })

  /*
   * **And the pair the headline is drawn from agrees with itself.** A merged pull request
   * has a day it merged and is refused for signing as `finished`; the fixture derives both
   * from `pr_state` so no state below can be measured against a record the server could
   * not produce.
   */
  it('keeps pr_state, pr_state_at and not_for_signing in one state', () => {
    expect(dense().review.pr_state_at).toBeNull()
    expect(dense().not_for_signing).toBeNull()

    const merged = dense({ prState: 'merged' })
    expect(merged.review.pr_state_at).toBe(SETTLED_AT)
    expect(merged.not_for_signing).toEqual({ reason: 'finished', how: 'merged', at: SETTLED_AT })

    const none = dense({ prState: 'merged', prStateAt: null })
    expect(none.review.pr_state_at).toBeNull()
  })
})

/* ================================== The shell ================================= */

describe('the page step 7 never had', () => {
  beforeEach(async () => {
    await withDense()
  })

  it('sits in the shell every step page sits in, and marks step 7 as where you are', async () => {
    const page = await shown(REVIEW)
    expect(page.text()).toContain('acme/portal')
    expect(page.text()).toContain('#601')
    expect(page.find('.trail').exists()).toBe(true)
    expect(page.find('.face--here .name').text()).toBe('7 What happened')
  })

  it('says how the pull request stands, and when it was last read', async () => {
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request is still open')
    expect(page.find('.sub').text()).toContain('Last read')
    expect(page.find('.sub').text()).toContain('14 findings were posted')
    expect(page.find('.sub').text()).toContain('7 of them had something come back')
    expect(page.find('.sub').text()).toContain('2 replies')
    expect(page.find('.sub').text()).toContain('7 whose code changed')
  })

  /**
   * **Replies are counted as replies and code changes as findings**, and the two counts
   * are not the same shape on purpose.
   *
   * The harvester keys a code change on the commit it saw it in, so a finding whose file
   * three pushes touch holds three rows — counting rows would give a figure about findings
   * that exceeds the findings it is about, which is why
   * `Store::posted_reviews_awaiting_author` counts that one `DISTINCT`. A reply is a thing
   * the author wrote, and two replies on one finding are two things.
   */
  it('counts two replies on one finding as two, and three pushes on one as one', async () => {
    const twice = [
      ...OUTCOMES,
      outcome({
        finding_seq: 1,
        kind: 'replied',
        occurred_at: '2026-10-12T08:00:00Z',
        detail: repliedDetail(1, 'And one more thing.'),
      }),
      outcome({
        finding_seq: 3,
        kind: 'code_changed_after',
        actor: 'harvester',
        occurred_at: '2026-10-12T18:00:00Z',
        detail: changedDetail(3, true),
      }),
    ]
    await withDense(dense({ outcomes: twice }))
    const page = await shown(REVIEW)
    expect(page.find('.sub').text()).toContain('3 replies')
    expect(page.find('.sub').text()).toContain('7 whose code changed')
  })
})

/* ============================== What came back ================================ */

describe('what came back', () => {
  beforeEach(async () => {
    await withDense()
  })

  /**
   * **Most serious first** — the review's own grade, which the code review page already
   * ranks its rows by, so a reader arriving from there meets them in the same order. What
   * is not done is ordering by what *came back*: a finding whose code changed is not
   * sorted above one that was replied to.
   *
   * **The findings are raised out of severity order on purpose.** The shared fixture
   * happens to number them most serious first, so a test over it is green whether the
   * rows are sorted or left alone — measured, by taking the sort out and watching it stay
   * green. Here F1 is a nit and F4 is the blocker.
   */
  it('draws the rows most serious first, whatever order they were raised in', async () => {
    const scrambled: Severity[] = ['nit', 'question', 'should', 'blocker']
    const findings = scrambled.map((severity, at) => ({
      ...finding(at + 1),
      severity,
    }))
    const outcomes = findings.map(one => outcome({
      finding_seq: one.seq,
      kind: 'code_changed_after',
      actor: 'harvester',
      occurred_at: `2026-10-12T1${one.seq}:00:00Z`,
      detail: changedDetail(one.seq, true),
    }))
    await withDense(dense({ findings, outcomes }))
    const page = await shown(REVIEW)

    const rows = page.findAll('.answered-row')
    expect(rows.map(one => one.find('.key').text())).toEqual(['F4', 'F3', 'F2', 'F1'])
    expect(rows.map(one => one.find('.severity').text().replace('Severity:', '').trim()))
      .toEqual(['blocker', 'should', 'question', 'nit'])
  })

  it('draws a row for every posted finding, across both groups', async () => {
    const page = await shown(REVIEW)
    const answered = page.findAll('.answered-row').length
    /* The fold is closed, so the quiet ones are counted on its heading rather than drawn. */
    expect(page.text()).toContain(`${14 - answered} of 14 findings`)
    expect(page.text()).toContain(`${answered} of 14 findings`)
  })

  it('puts a finding’s several outcomes in time order, oldest first', async () => {
    const page = await shown(REVIEW)
    const first = page.findAll('.answered-row')[0]!
    expect(first.text()).toContain('F1')
    const said = first.findAll('.came-said').map(one => one.text())
    expect(said).toHaveLength(3)
    expect(said[0]).toContain('A reply from dana-okoye')
    expect(said[1]).toContain('The thread was resolved by dana-okoye')
    expect(said[2]).toContain('The code at line 47 changed in 65eff63')
  })

  it('quotes the author’s words, whole, however long they run', async () => {
    /*
     * **The density the issue asks for, asserted where it is used.** "One reply over 80
     * words" is a property of this fixture, and a fixture that quietly shrank would leave
     * the page measured at a length no author writes at. Each suite holds its own copy of
     * the reply and its own assertion of the length, rather than one shared across
     * `test/`, `e2e/` and nothing to tell them apart.
     */
    expect(LONG_REPLY.trim().split(/\s+/).length).toBeGreaterThan(80)

    const page = await shown(REVIEW)
    const quote = page.findAll('.quote').map(one => one.text())
    expect(quote).toHaveLength(2)
    expect(quote[0]).toBe(LONG_REPLY)
    expect(quote[1]).toBe(SHORT_REPLY)
  })

  it('prints no outcome’s stored detail as JSON', async () => {
    const page = await shown(REVIEW)
    const text = page.text()
    for (const token of ['{', 'comment_id', 'in_reply_to', 'time_basis', 'lines_changed', 'basis']) {
      expect(text, `the page printed ${token}`).not.toContain(token)
    }
  })

  /*
   * The case the issue names in the same breath as the sentence: `lines_changed` false is
   * a finding whose *file* changed while the lines it is about did not.
   */
  it('says the file changed and the lines did not, where that is what happened', async () => {
    const page = await shown(REVIEW)
    expect(page.text()).toContain('but not line 82')
  })

  it('does not replay the developer’s own decisions at them', async () => {
    const page = await shown(REVIEW)
    /* F12's `cut` is in the record and is the code review page's to draw, not this one's. */
    expect(page.text()).not.toContain('cut from this round')
  })

  it('links a row to its finding and to its comment on GitHub', async () => {
    const page = await shown(REVIEW)
    const first = page.findAll('.answered-row')[0]!
    const to = first.findAll('a').map(one => one.attributes('href'))
    expect(to).toContain(`/reviews/${REVIEW}/code-review#F1`)
    expect(to).toContain('https://github.com/acme/portal/pull/601#discussion_r2314990001')
  })

  it('shows the claim cut to a title’s length for a finding with no title, and says so', async () => {
    const page = await shown(REVIEW)
    /* F4 has no title. It has no outcome either, so the row is in the folded group. */
    await page.find('.fold').trigger('click')
    await settle(page)
    const row = page.findAll('.silent-row').find(one => one.text().includes('F4'))!
    expect(row.text()).toContain('…')
  })

  /**
   * **No sentence about what came back names a round.**
   *
   * The ticket asks for outcomes described in words — the author's words for a reply, the
   * thread for a resolution, the commit for a code change. A round is not one of those
   * facts: it is where the finding was raised, which the row already says in its own note
   * where it matters, and the pull request's timeline says once at the foot of the page.
   * An earlier draft headed a reply with the round it came back in, which read as though
   * the reply were about the round rather than about the finding.
   *
   * Over every sentence the page draws, not over one row: the three kinds the harvest
   * writes are each worded by a different branch of `outcomeSaid`.
   */
  it('words what came back without naming a round', async () => {
    const page = await shown(REVIEW)
    const said = page.findAll('.came-said').map(one => one.text())
    expect(said.length).toBeGreaterThan(6)
    for (const one of said) expect(one.toLowerCase()).not.toContain('round')
  })
})

/* ========================== Nothing came back on ============================== */

describe('the findings nothing came back on', () => {
  beforeEach(async () => {
    await withDense()
  })

  it('are counted and folded, not drawn open', async () => {
    const page = await shown(REVIEW)
    expect(page.findAll('.silent-row')).toHaveLength(0)
    expect(page.find('.fold').text()).toContain('findings nothing came back on')
  })

  it('open, and each says so rather than showing a blank', async () => {
    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    const rows = page.findAll('.silent-row')
    expect(rows.length).toBeGreaterThan(0)

    await rows[0]!.trigger('click')
    await settle(page)
    const opened = page.find('.silent-opened')
    expect(opened.text()).toContain('Nothing has come back on')
  })

  /*
   * **Why nothing could have come back, where that is the real answer.** F13 and F14 carry
   * no comment id, so there is no thread for a reply to attach to and zero replies is the
   * absence of a measurement rather than one — the rule `authorResponseLabel` keeps for the
   * queue, said here about the one finding it is true of.
   */
  it('says when no reply could have been read at all', async () => {
    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    const row = page.findAll('.silent-row').find(one => one.text().includes('F13'))!
    await row.trigger('click')
    await settle(page)
    expect(page.find('.silent-opened').text()).toContain('no thread for a')
  })

  it('opens one row at a time', async () => {
    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    const rows = page.findAll('.silent-row')
    await rows[0]!.trigger('click')
    await settle(page)
    await rows[1]!.trigger('click')
    await settle(page)
    expect(page.findAll('.silent-opened')).toHaveLength(1)
  })
})

/* ============================ Every state it can be in ======================== */

describe('the states a posted review can be in', () => {
  it('a merged pull request, on the day it merged', async () => {
    await withDense(dense({ prState: 'merged' }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe(`The pull request was merged on ${SETTLED_DAY}`)
  })

  it('a pull request closed without being merged, on the day it closed', async () => {
    await withDense(dense({ prState: 'closed' }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text())
      .toBe(`The pull request was closed on ${SETTLED_DAY} without being merged`)
  })

  /**
   * **The day comes from `pr_state_at` and from no other instant on the record.**
   *
   * Two others are within reach and each is a different fact. `harvested_at` is when this
   * desk last read GitHub — three days after the merge on this fixture — and the round's
   * `created_at` is when the review started. Either would put a day in the headline that
   * nobody merged on, and the reader has no way to tell which day they were given.
   */
  it('does not date the merge from the harvest, or from when the review started', async () => {
    await withDense(dense({
      prState: 'merged',
      prStateAt: null,
      harvestedAt: '2026-10-14T06:00:00Z',
    }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request was merged')
    expect(page.find('h1').text()).not.toContain('14 October')
    expect(page.find('h1').text()).not.toContain('9 October')
    expect(page.find('h1').text()).not.toContain(' on ')
  })

  /**
   * **A settled pull request GitHub gave no time for says it settled and names no day**,
   * which is the contract's own instruction about this column: null is "no time given",
   * and a reader must not be handed an invented one.
   */
  it('a merged pull request the record holds no day for', async () => {
    await withDense(dense({ prState: 'merged', prStateAt: null }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request was merged')
  })

  /** And an open one is not dated at all: the column is null while it is open. */
  it('does not date a pull request that is still open', async () => {
    await withDense(dense({ prState: 'open', prStateAt: SETTLED_AT }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request is still open')
  })

  /**
   * **A state with no read time is a harvest that stopped half way, not one that never
   * ran.** `harvest_one` writes `pr_state` first and `harvested_at` last with no
   * transaction across them, so a failed thread read leaves one and not the other — and
   * the page drew "The pull request was merged" over a notice saying no harvest has read
   * this review, which cannot both be true of one record.
   */
  it('does not say nothing has looked over a state only a harvest could know', async () => {
    await withDense(dense({ prState: 'merged', harvestedAt: null }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe(`The pull request was merged on ${SETTLED_DAY}`)
    expect(page.find('.notice').exists()).toBe(false)
    /* And it does not claim the harvest found nothing, because it did not finish. */
    expect(page.text()).not.toContain('found no reply, no resolved thread')
    /*
     * **Nor anywhere else on the page**, which is where the first fix left it: the "Nothing
     * came back" section carried the same claim keyed on `!harvested`, so the page said a
     * harvest had found the merge, said seven findings had something come back, and said
     * nothing had looked. Asserted over the whole page rather than over the two sentences
     * known about, because this is the second place it was found in.
     */
    expect(page.text()).not.toContain('Nothing has looked')
  })

  /*
   * **A review nothing has read is not an open pull request.** `pr_state` and
   * `harvested_at` are written by the same harvest, so both are null until one runs — and
   * drawing that as "open" would be reporting a measurement nobody took.
   */
  it('a review no harvest has read', async () => {
    await withDense(dense({ prState: null, harvestedAt: null, outcomes: [] }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('Nothing has read this pull request back from GitHub')
    expect(page.text()).toContain('No harvest has read this review back from GitHub')
    expect(page.find('.sub').text()).not.toContain('Last read')
  })

  it('a harvest that ran and found nothing', async () => {
    await withDense(dense({ outcomes: [] }))
    const page = await shown(REVIEW)
    expect(page.text()).toContain('Nothing has come back on any of the findings')
    expect(page.text()).toContain('found no reply, no resolved thread')
    expect(page.findAll('.answered-row')).toHaveLength(0)
  })

  /*
   * `review-desk outcome add` records an outcome without reading GitHub at all, so a page
   * can hold rows with no read time behind them.
   *
   * **Said once, in the notice.** The headline already says nothing has read this pull
   * request, so a sub-line repeating it put one fact on the screen three times in four
   * inches — seen in the browser, and the fault `steps.ts` names in as many words.
   */
  it('outcomes recorded with no harvest behind them', async () => {
    await withDense(dense({ prState: null, harvestedAt: null }))
    const page = await shown(REVIEW)
    expect(page.find('.notice').text()).toContain('recorded by hand')
    expect(page.find('.sub').text()).not.toContain('harvest')
    expect(page.find('.sub').text()).toContain('7 of them had something come back')
  })

  /*
   * **The path is on the row, and not again under it.** The harvester records the file it
   * compared, which is the finding's own in every ordinary case; drawing it on the outcome
   * too printed a 53-character path twice in four lines, which at 390 is four wrapped
   * lines of the same thing.
   */
  it('does not print a finding’s path twice', async () => {
    await withDense()
    const page = await shown(REVIEW)
    const row = page.findAll('.answered-row')[0]!
    expect(row.findAll('.came-file')).toHaveLength(0)
    expect(row.text()).toContain('app/Reporting/ColumnPolicy.php:47')
  })

  /* And it is drawn where the two really differ, which a rename makes them. */
  it('prints the outcome’s own path where it is not the finding’s', async () => {
    const renamed = [outcome({
      finding_seq: 1,
      kind: 'code_changed_after',
      actor: 'harvester',
      occurred_at: '2026-10-12T16:40:00Z',
      detail: JSON.stringify({
        file: 'app/Reporting/ColumnPolicyOld.php',
        line: 47,
        lines_changed: true,
        file_status: 'modified',
        commit: '65eff631a04',
      }),
    })]
    await withDense(dense({ outcomes: renamed }))
    const page = await shown(REVIEW)
    expect(page.find('.came-file').text()).toBe('app/Reporting/ColumnPolicyOld.php')
  })

  it('a review with no findings at all', async () => {
    await withDense(dense({ findings: [], outcomes: [] }))
    const page = await shown(REVIEW)
    expect(page.find('.sub').text()).toContain('0 findings were posted')
    expect(page.findAll('.silent-row')).toHaveLength(0)
    expect(page.find('.fold').exists()).toBe(false)
  })
})

/* ================= What the page counts, and what it does not ================= */

describe('the figures and the rows count the same findings', () => {
  /**
   * **The harvester writes against any finding of the review.** `harvest_code_changes`
   * iterates all of them and asks only for a file; `harvest_thread` asks only for a
   * comment id. Neither looks at the disposition. So a nit held back for a ticket, a
   * finding set aside, or one carried in as `fixed` can carry an outcome — and this page
   * draws a row for the *posted* ones.
   *
   * Counting the review's outcomes therefore reported figures about findings that were not
   * on the screen: "1 of them had something come back: 3 whose code changed", with one row
   * under it. That is the defect this whole issue is about, one level up.
   */
  function withHeldBack() {
    const findings = Array.from({ length: 14 }, (_, at) => finding(at + 1))
    /* Three held back for a ticket, whose files changed in the same push. */
    for (const seq of [12, 13, 14]) {
      findings[seq - 1] = { ...findings[seq - 1]!, disposition: 'held_back' }
    }
    const outcomes = [
      ...OUTCOMES.filter(one => one.finding_seq === 1 && one.kind === 'code_changed_after'),
      ...[12, 13, 14].map(seq => outcome({
        finding_seq: seq,
        kind: 'code_changed_after',
        actor: 'harvester',
        occurred_at: '2026-10-12T16:00:00Z',
        detail: changedDetail(seq, true),
      })),
    ]
    return dense({ findings, outcomes })
  }

  it('counts only the findings it draws a row for', async () => {
    await withDense(withHeldBack())
    const page = await shown(REVIEW)
    expect(page.find('.sub').text()).toContain('11 findings were posted')
    expect(page.find('.sub').text()).toContain('1 of them had something come back')
    expect(page.find('.sub').text()).toContain('1 whose code changed')
    expect(page.findAll('.answered-row')).toHaveLength(1)
  })

  /*
   * And what the record holds about the other three is not lost. Counting them into the
   * figures put numbers on screen about rows that were not there; saying nothing would
   * lose a measurement, which is this desk's other rule.
   */
  it('says what the record holds about findings it draws no row for', async () => {
    await withDense(withHeldBack())
    const page = await shown(REVIEW)
    expect(page.text()).toContain('Outcomes on findings this page does not list')
    expect(page.text()).toContain('3 outcomes')
    expect(page.text()).toContain('Against findings this review did not post')
  })

  it('says nothing about them where there are none', async () => {
    await withDense()
    const page = await shown(REVIEW)
    expect(page.text()).not.toContain('Outcomes on findings this page does not list')
  })
})

describe('a review whose repository name is not a repository name', () => {
  /**
   * **A released Review Desk could store one**, which `links.ts` says in as many words —
   * and every comment address on this page is built on top of the pull request's. So no
   * row has a link, and the reason is neither the finding nor its comment id. The page
   * blamed the id: "the id recorded for that comment is not one this page can build an
   * address from", over a record holding a perfectly good numeric one.
   */
  async function withBadRepo() {
    const bad = dense()
    await withDense({ ...bad, review: { ...bad.review, repo: 'acme/portal#x' } })
  }

  it('says the repository is why, and does not blame the comment id', async () => {
    await withBadRepo()
    const page = await shown(REVIEW)
    expect(page.find('.notes').text()).toContain('No link to GitHub.')
    expect(page.find('.notes').text()).not.toContain('A comment with no address')
  })

  /*
   * **It is a fact about the review, so it is read off the review.** Derived from the rows
   * — `rows.some(row => !row.addressable)` — it was false for a review with an unusable
   * repository name and no posted finding, which is the one page where nothing else
   * explains why there is no link.
   */
  it('says it on a review with no posted finding at all', async () => {
    const bad = dense({ findings: [], outcomes: [] })
    await withDense({ ...bad, review: { ...bad.review, repo: 'acme/portal#x' } })
    const page = await shown(REVIEW)
    expect(page.find('.notes').text()).toContain('No link to GitHub')
    /*
     * And it does not promise links on rows that are not on the screen. That is the same
     * shape as the sentence above it, which was guarded for the same reason.
     */
    expect(page.find('.notes').text()).not.toContain('code review page are unaffected')
  })

  /* And the links to each finding's own row are unaffected, which the note says. */
  it('does not claim the links to the code review page are gone', async () => {
    await withBadRepo()
    const page = await shown(REVIEW)
    expect(page.find('.notes').text()).toContain('code review page are unaffected')
    expect(page.findAll('.answered-row')[0]!.find('a').attributes('href'))
      .toBe(`/reviews/${REVIEW}/code-review#F1`)
  })

  it('says it on the row too, where the row has to explain itself', async () => {
    await withBadRepo()
    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    const row = page.findAll('.silent-row').find(one => one.text().includes('F4'))!
    await row.trigger('click')
    await settle(page)
    const opened = page.find('.silent-opened').text()
    expect(opened).toContain('repository name is not one a GitHub address can be built from')
    expect(opened).not.toContain('no thread for a reply to attach to')
  })
})

/* ================================ Later rounds ================================ */

describe('the rounds of the pull request', () => {
  /**
   * **The sketch at the top of the page draws the same cells the page does.**
   *
   * The page's header carries an ASCII sketch of the screen, under a line saying the
   * figures are the fixture's "because a sketch that disagrees with the thing it is a
   * sketch of is worse than none". Both round cells were rewritten when the column stopped
   * claiming a carry, and neither was: the sketch said "nothing decided yet" and "2 fixed,
   * 1 still open" over a page drawing "nothing decided about any of them" and "2 fixed, 1
   * still open, 1 their reason accepted".
   *
   * **Checked rather than reread, because this is the third claim on this branch to go
   * stale in a comment** — the whitespace rule and the module header's "the contract does
   * not serve `pr_state_at`" were the others, and the rule this repository already settled
   * on is that the second instance of a defect gets a check and not a third careful edit.
   * It reads the cells off the rendered page, so a wording change fails here until the
   * sketch follows it.
   */
  it('is drawn in the sketch at the top of the page, cell for cell', async () => {
    await withDense(dense({ round: 2, rounds: ROUNDS }))
    const page = await shown(REVIEW)
    const drawn = page.findAll('[data-label="How they stood"]')
      .map(cell => cell.text())
      .filter(cell => cell !== '')

    expect(drawn.length).toBeGreaterThan(1)

    const source = readFileSync(resolve('app/pages/reviews/[id]/what-happened.vue'), 'utf8')
    const sketch = source.slice(
      source.indexOf('THE ROUNDS OF THIS PULL REQUEST'),
      source.indexOf('```', source.indexOf('THE ROUNDS OF THIS PULL REQUEST')),
    )
    expect(sketch, 'the sketch is where this says it is').toContain('Round 1')

    for (const cell of drawn) expect(sketch).toContain(cell)
  })

  it('are not drawn for a pull request with one round', async () => {
    await withDense()
    const page = await shown(REVIEW)
    expect(page.text()).not.toContain('The rounds of this pull request')
  })

  it('say what each round held and how its findings stood', async () => {
    await withDense(dense({ round: 2, rounds: ROUNDS }))
    const page = await shown(REVIEW)
    expect(page.text()).toContain('The rounds of this pull request')
    const rows = page.findAll('.row').map(one => one.text())
    expect(rows.some(one => one.includes('Round 1') && one.includes('11 findings'))).toBe(true)
    expect(rows.some(one => one.includes('Round 2') && one.includes('· this one'))).toBe(true)
    /* Round 2's own counts, as statuses rather than as a total called something else. */
    expect(rows.some(one => one.includes('2 fixed, 1 still open, 1 their reason accepted')))
      .toBe(true)
    expect(rows.some(one => one.includes('nothing decided about any of them'))).toBe(true)
  })

  /**
   * **A status is not a carry**, which is what the column said before.
   *
   * `to_ticket`, `accept_reason` and `withdraw` write `ticketed`, `accepted` and
   * `withdrawn` on a finding *raised in that round* — `FindingDecision`'s table says so and
   * `entities/decision.rs` applies it with no carried guard. Adding every status but `new`
   * together and calling it "carried in" therefore reported a carry for a first round,
   * which carries nothing by definition.
   */
  it('does not call a first round’s ticketed nit a finding carried in', async () => {
    const first = {
      ...ROUNDS[0]!,
      statuses: { new: 10, fixed: 0, still_open: 0, accepted: 0, withdrawn: 0, ticketed: 1 },
    }
    await withDense(dense({ round: 2, rounds: [first, ROUNDS[1]!] }))
    const page = await shown(REVIEW)
    const rows = page.findAll('.row').map(one => one.text())
    const round1 = rows.find(one => one.includes('Round 1'))!
    expect(round1).toContain('1 moved to a ticket')
    expect(round1).not.toContain('carried in')
  })

  it('name the round a carried finding came from, on its own row', async () => {
    const carried = Array.from({ length: 14 }, (_, at) => finding(at + 1))
    carried[0] = {
      ...carried[0]!,
      continues_review_id: 12,
      continues_round: 1,
      round_status: 'still_open',
    }
    await withDense(dense({ round: 2, rounds: ROUNDS, findings: carried }))
    const page = await shown(REVIEW)
    const first = page.findAll('.answered-row')[0]!
    expect(first.text()).toContain('Carried from round 1')
    expect(first.text()).toContain('Still open')
  })
})

/* ============================ It describes; it does not grade ================== */

describe('it describes and does not grade', () => {
  beforeEach(async () => {
    await withDense()
  })

  /*
   * **No severity tag and no severity colour.** `HeldBackRows` made the same call for the
   * same reason and states it: a colour tells a reader which of these to be afraid of,
   * which is a judgement about what came back rather than a report of it. The severity is
   * still on every row, as the record's own word.
   */
  it('draws a severity as a word, with no tag', async () => {
    const page = await shown(REVIEW)
    /*
     * `Severity:` is the visually-hidden label, which is in the text and not on the
     * screen — and the space after it is an interpolated one, because Vue's
     * `whitespace: 'condense'` deletes a whitespace-only text node between two elements and
     * merges a trailing `&#32;` into it. Written as a newline, as a second `<span>` around
     * the value, or as an entity at the end of a line, this read `Severity:blocker` — which
     * is what a reader of the text then gets. `textContent` rather than `text()`, which
     * trims.
     */
    expect(page.find('.severity').element.textContent).toBe('Severity: blocker')
    expect(page.findAll('.tag')).toHaveLength(0)
  })

  it('says of no finding that it was fixed', async () => {
    const page = await shown(REVIEW)
    expect(page.text()).not.toContain('fixed in')
    expect(page.text()).not.toContain('was fixed')
  })

  /*
   * GitHub does not record when a thread was resolved, so the harvester records when it
   * looked. The row says "seen", and the footnote says once what that means.
   */
  it('says a resolution’s time is when it was seen, and footnotes why', async () => {
    const page = await shown(REVIEW)
    expect(page.find('.answered-row').text()).toContain('seen')
    expect(page.find('.notes').text()).toContain('GitHub does not record when a review thread')
  })

  it('explains once why some findings have no link to GitHub', async () => {
    const page = await shown(REVIEW)
    /* F13 and F14 were raised in the body alone: they have no thread at all. */
    expect(page.find('.notes').text()).toContain('raised in the body alone')
    expect(page.find('.notes').text()).not.toContain('A comment with no address')
  })

  /**
   * **A comment with no address is not a finding with no comment**, and the row said the
   * second about the first — which is false of it and tells the reader to stop looking.
   *
   * `github_comment_id` is required to be GitHub's numeric `databaseId`, because that is
   * what the harvester matches a thread by; a node id (`IC_kwDOA1…`) is what every record
   * written before anyone checked holds, and `app/data/reviews.json` held two.
   */
  it('tells a comment with no address apart from a finding with no comment', async () => {
    const nodeIds = Array.from({ length: 14 }, (_, at) => ({
      ...finding(at + 1),
      github_comment_id: `IC_kwDOA${at + 1}`,
    }))
    await withDense(dense({ findings: nodeIds }))
    const page = await shown(REVIEW)

    /* One link, and it is the one to the code review page. The text carries its own
       visually-hidden tail, which is why this is `startsWith` and not an equality. */
    const links = page.findAll('.answered-row')[0]!.findAll('a')
    expect(links).toHaveLength(1)
    expect(links[0]!.text()).toContain('The finding')
    expect(links[0]!.attributes('href')).toBe(`/reviews/${REVIEW}/code-review#F1`)
    expect(page.find('.notes').text()).toContain('14 comments with no address')
    expect(page.find('.notes').text()).not.toContain('raised in the body alone')

    await page.find('.fold').trigger('click')
    await settle(page)
    await page.findAll('.silent-row')[0]!.trigger('click')
    await settle(page)
    const opened = page.find('.silent-opened').text()
    expect(opened).toContain('so a reply would be read')
    expect(opened).not.toContain('no thread for a reply to attach to')
  })

  /*
   * **Another review starts folded, with nothing open.** The component is reused across a
   * parameter change — the router's own behaviour — and both pieces of state here are
   * about *this* review's rows, with `open` keyed on a sequence number: a row left open on
   * one review came back open on the next over whichever finding shared the number.
   */
  it('starts folded again when another review is opened', async () => {
    const second = dense()
    second.review = { ...second.review, id: REVIEW + 1, pr: 602 }
    second.findings = second.findings.map(one => ({ ...one, review_id: REVIEW + 1 }))
    second.outcomes = second.outcomes.map(one => ({ ...one, review_id: REVIEW + 1 }))
    await resetDesk({ reviews: [...loadSource().reviews, dense(), second] })

    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    await page.findAll('.silent-row')[0]!.trigger('click')
    await settle(page)
    expect(page.findAll('.silent-opened')).toHaveLength(1)

    await page.vm.$router.push(`/reviews/${REVIEW + 1}/what-happened`)
    await settle(page)
    expect(page.findAll('.silent-row')).toHaveLength(0)
    expect(page.findAll('.silent-opened')).toHaveLength(0)
  })

  /**
   * Folding the quiet half lets go of whichever row was open inside it. Without that,
   * unfolding brought one back already open, which reads as the control having done
   * nothing.
   */
  it('lets go of an open row when the group is folded', async () => {
    const page = await shown(REVIEW)
    await page.find('.fold').trigger('click')
    await settle(page)
    await page.findAll('.silent-row')[0]!.trigger('click')
    await settle(page)
    expect(page.findAll('.silent-opened')).toHaveLength(1)

    await page.find('.fold').trigger('click')
    await settle(page)
    await page.find('.fold').trigger('click')
    await settle(page)
    expect(page.findAll('.silent-opened')).toHaveLength(0)
  })

  it('footnotes nothing it has nothing to footnote', async () => {
    const onlyChanged = OUTCOMES.filter(one => one.kind === 'code_changed_after')
    const linked = Array.from({ length: 12 }, (_, at) => finding(at + 1))
    await withDense(dense({ outcomes: onlyChanged, findings: linked }))
    const page = await shown(REVIEW)
    expect(page.find('.notes').exists()).toBe(false)
  })
})
