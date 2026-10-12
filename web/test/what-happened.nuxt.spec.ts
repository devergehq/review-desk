// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import WhatHappened from '../app/pages/reviews/[id]/what-happened.vue'

import type {
  Finding,
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

/** The density review. `over` replaces any part of it, which is how the states below differ. */
function dense(over: {
  prState?: PrState | null
  harvestedAt?: string | null
  outcomes?: Outcome[]
  round?: number
  rounds?: RoundSummary[] | undefined
  findings?: Finding[]
} = {}): ReviewDetail {
  const findings = over.findings ?? Array.from({ length: 14 }, (_, at) => finding(at + 1))
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
      pr_state: over.prState === undefined ? 'open' : over.prState,
      harvested_at: over.harvestedAt === undefined ? '2026-10-12T17:00:00Z' : over.harvestedAt,
      dismissed_at: null,
      dismissed_reason: null,
    },
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
  } as unknown as ReviewDetail
}

/** The density review, plus whatever the sample records already hold. */
async function withDense(detail: ReviewDetail = dense()) {
  await resetDesk({ reviews: [...loadSource().reviews, detail] })
}

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
    expect(page.find('.sub').text()).toContain('2 with a reply')
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
    expect(page.find('.sub').text()).toContain('3 with a reply')
    expect(page.find('.sub').text()).toContain('7 whose code changed')
  })
})

/* ============================== What came back ================================ */

describe('what came back', () => {
  beforeEach(async () => {
    await withDense()
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
  it('a merged pull request', async () => {
    await withDense(dense({ prState: 'merged' }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request was merged')
  })

  it('a pull request closed without being merged', async () => {
    await withDense(dense({ prState: 'closed' }))
    const page = await shown(REVIEW)
    expect(page.find('h1').text()).toBe('The pull request was closed without being merged')
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

/* ================================ Later rounds ================================ */

describe('the rounds of the pull request', () => {
  it('are not drawn for a pull request with one round', async () => {
    await withDense()
    const page = await shown(REVIEW)
    expect(page.text()).not.toContain('The rounds of this pull request')
  })

  it('say what each round held and what it carried in', async () => {
    await withDense(dense({ round: 2, rounds: ROUNDS }))
    const page = await shown(REVIEW)
    expect(page.text()).toContain('The rounds of this pull request')
    const rows = page.findAll('.row').map(one => one.text())
    expect(rows.some(one => one.includes('Round 1') && one.includes('11 findings'))).toBe(true)
    expect(rows.some(one => one.includes('Round 2') && one.includes('· this one'))).toBe(true)
    /*
     * **What a round carried, from its own status counts.** Every status but `new` is a
     * finding the round carried in — a finding raised in a round is `new` there by
     * definition — so round 2's 2 fixed, 1 still open and 1 accepted is 4 carried.
     */
    expect(rows.some(one => one.includes('4 findings carried in, 1 still open'))).toBe(true)
    expect(rows.some(one => one.includes('nothing — every finding was raised in it'))).toBe(true)
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
    /* `Severity:` is the visually-hidden label, which is in the text and not on the screen. */
    expect(page.find('.severity').text()).toBe('Severity:blocker')
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
    expect(page.find('.notes').text()).toContain('A comment with no address')
    expect(page.find('.notes').text()).not.toContain('raised in the body alone')

    await page.find('.fold').trigger('click')
    await settle(page)
    await page.findAll('.silent-row')[0]!.trigger('click')
    await settle(page)
    const opened = page.find('.silent-opened').text()
    expect(opened).toContain('so a reply would be read')
    expect(opened).not.toContain('no thread for a reply to attach to')
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
