// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import CodeReview from '../app/pages/reviews/[id]/code-review.vue'
import Draft from '../app/pages/reviews/[id]/draft.vue'

import type { DraftDetail, ReviewDetail } from '../app/types/desk'

/*
 * **Step 5, the code review page**, mounted against the records.
 *
 * DEV-878 rebuilds this page as board A8, and Patrick's warning when he approved the board
 * was "I think you're going to have trouble trying to contain that". So most of what is
 * here is about containment: a row that stays one line, one finding open at a time,
 * reasoning that is never unfolded by default, and a page that does not scroll sideways.
 *
 * Three of those can be checked by mounting and four cannot. **A jsdom test cannot measure
 * a layout**: it has no box model, so "the page does not scroll sideways" and "only the code
 * block does" are assertions about computed geometry that would pass here against any
 * stylesheet at all. They are asserted in `web/e2e/review-desk.spec.ts`, in a real browser,
 * at 1440 and at 390 — and the declarations they turn on are asserted here instead, which is
 * the honest division: this file checks that the rule is *stated*, the browser checks that
 * it *holds*.
 *
 * Review 41 is the invented example at real length — fifteen findings across all three
 * lists, ten with code, one with none — and is the same document
 * `contract/examples/getReview.response.code-review.json` holds, which
 * `test/example-fixtures.spec.ts` keeps true.
 */

/** #514, the real-length review: fifteen findings, seven parts, a draft of blocks. */
const REAL = 41
/** #412, whose draft was written as one block of markdown rather than in parts. */
const ONE_BLOCK = 17
/** #398, posted. Nothing can be decided about it, and the author has replied. */
const POSTED = 12
/** A review whose step 5 is running and has written nothing yet. */
const RUNNING = 18
/** A review whose step 5 has not run at all. */
const NOT_RUN = 19

const STILL_READING = /Reading [^…]*…/

/** Let every pending read finish. The shape `screens.nuxt.spec.ts` argues for. */
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

async function shown(component: Parameters<typeof mountSuspended>[0], route: string) {
  const page = await mountSuspended(component, { route })
  await settle(page)
  return page
}

const codeReview = (id: number) => shown(CodeReview, `/reviews/${id}/code-review`)
const draftPage = (id: number) => shown(Draft, `/reviews/${id}/draft`)

/** Press the button whose text contains this, and let what it started finish. */
async function press(page: VueWrapper, within: string) {
  const found = page.findAll('button').find(one => one.text().includes(within))
  if (!found) throw new Error(`no button whose text contains ${within}`)
  await found.trigger('click')
  await settle(page)
  return found
}

/** Open the row for one finding, by its key. */
async function openFinding(page: VueWrapper, key: string) {
  const row = page.findAll('.row').find(one => one.find('.key').text() === key)
  if (!row) throw new Error(`no row for ${key}`)
  await row.find('button.row').trigger('click')
  await settle(page)
  return row
}

function fixture(id: number): ReviewDetail {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return found
}

function draftFixture(id: number): DraftDetail {
  const found = loadSource().drafts.find(draft => draft.draft.review_id === id)
  if (!found) throw new Error(`the fixtures hold no draft on review ${id}`)
  return found
}

async function withReview(detail: ReviewDetail) {
  const reviews = loadSource().reviews.map(review =>
    review.review.id === detail.review.id ? detail : review)
  await resetDesk({ reviews })
}

/* ============================== The page as a whole ============================= */

describe('the code review page', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('leads with the counts, as a sentence', async () => {
    const page = await codeReview(REAL)
    expect(page.find('h1').text()).toBe(
      '1 blocker. 6 should change, 3 questions, 2 small points.',
    )
  })

  /*
   * **It counts the list that posts, not every finding.** The review holds five nits; three
   * of them are held back or set aside, so the headline says two. A review with six nits it
   * has decided not to mention has not found six small points to tell the author about.
   */
  it('counts only what is going to be posted', async () => {
    const detail = fixture(REAL)
    expect(detail.finding_severities.nit).toBe(5)
    const page = await codeReview(REAL)
    expect(page.find('h1').text()).toContain('2 small points')
  })

  it('says how many were reproduced by running something', async () => {
    const page = await codeReview(REAL)
    expect(page.text()).toContain('7 of 15 findings were reproduced by running something')
    expect(page.text()).toContain('The rest were read, not run.')
  })

  it('says the list can still grow while step 5 is running', async () => {
    const detail = fixture(REAL)
    detail.layers = detail.layers.map(layer =>
      layer.number === 5 ? { ...layer, state: 'running' as const } : layer)
    await withReview(detail)

    const page = await codeReview(REAL)
    expect(page.text()).toContain('Step 5 is still running, so this list can still grow.')
  })

  it('does not say so once it has finished', async () => {
    const page = await codeReview(REAL)
    expect(page.text()).not.toContain('can still grow')
  })

  it('says what the review did not look at', async () => {
    const page = await codeReview(REAL)
    expect(page.text()).toContain('What the review did not look at')
    expect(page.text()).toContain('The reporting front end')
  })

  /*
   * **And says it on a review that found nothing, which is where it matters most.**
   *
   * It sat inside the findings branch and vanished exactly when it was the whole of the
   * answer: a step 5 that finished and found nothing is saying "we looked at X and not at
   * Y", and the page was dropping the second half. The retired screen was the only thing
   * drawing this field; losing it here would have been the same regression one page along.
   */
  it('says it on a review that finished and found nothing', async () => {
    const detail = fixture(RUNNING)
    detail.layers = detail.layers.map(layer =>
      layer.number === 5 ? { ...layer, state: 'done' as const } : layer)
    detail.not_checked = 'The admin screens, which this change does not touch.'
    await withReview(detail)

    const page = await codeReview(RUNNING)
    expect(page.find('h1').text()).toBe('The code review found nothing to say.')
    expect(page.text()).toContain('What the review did not look at')
    expect(page.text()).toContain('The admin screens')
    /* And no list, no strip and no tally, which would each be a count of nothing. */
    expect(page.find('.rows').exists()).toBe(false)
    expect(page.find('.foot').exists()).toBe(false)
  })

  /*
   * **And says what an empty field means, rather than drawing nothing.**
   *
   * `not_checked` is null on most reviews, and a section that disappears there reads as
   * "nothing was missed" — the one thing this field can never be read to say. The screen
   * this page replaced drew the sentence; the move carried the half a record happens to
   * hold and left this half behind.
   */
  it('says what an empty coverage field means', async () => {
    const detail = fixture(REAL)
    detail.not_checked = null
    await withReview(detail)

    const page = await codeReview(REAL)
    expect(page.text()).toContain('What the review did not look at')
    expect(page.text()).toContain('Nothing is recorded as unchecked')
    expect(page.text()).toContain('not the same as everything having been looked at')
  })

  /*
   * **The foot closes the page, so the coverage comes before it.**
   *
   * Moving the coverage out of the findings branch put it after the foot, which is the
   * grey panel with the tally and the way on to step 6 — a section under it reads as
   * stranded past the end of the page. Asserted on the order of the rendered markup,
   * because the fix has to be in the DOM: `order` in the stylesheet would leave a screen
   * reader hearing one order and the eye seeing another.
   */
  it('draws the coverage above the foot, not below it', async () => {
    const page = await codeReview(REAL)
    const html = page.html()
    expect(html.indexOf('What the review did not look at'))
      .toBeLessThan(html.indexOf('The draft is assembled from what you keep'))
  })

  /*
   * **The headline counts what will be posted, not what is in the "To post" list.**
   *
   * A finding carried into this round as `fixed` keeps the disposition it was carried
   * with, so it sits at `in_draft` and the composition will not post it —
   * `DraftSection.posted` needs the disposition *and* an open round status. The list is
   * what the developer decided; the headline is the page's one claim about what the author
   * receives, and it was counting a finding that was fixed last round.
   */
  it('leaves a settled carried finding out of the headline, and in its list', async () => {
    const before = await codeReview(REAL)
    expect(before.find('h1').text()).toContain('6 should change')

    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 1
        ? {
            ...finding,
            continues_review_id: 40,
            continues_round: 1,
            round_status: 'fixed' as const,
          }
        : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    expect(page.find('h1').text()).toContain('5 should change')
    /* Still on the page, in the list its disposition puts it in, saying what became of it. */
    const row = page.findAll('.row').find(one => one.find('.key').text() === 'F1')!
    expect(row.text()).toContain('Fixed')
  })

  it('tallies the three lists at the foot, and leads on to the draft', async () => {
    const page = await codeReview(REAL)
    expect(page.find('.tally').text()).toBe('12 to post, 1 for a ticket, 2 set aside')
    const on = page.findAll('a').find(one => one.text().includes('the draft'))!
    expect(on.attributes('href')).toBe(`/reviews/${REAL}/draft`)
  })
})

/* ==================== Not written yet is not the same as none =================== */

describe('a review with no findings', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * DEV-878: "With no findings yet it says they are not written yet, which is different
   * from none found." Three states, three sentences, and the difference is load-bearing —
   * one of them says the code is fine and the other two say nothing about the code at all.
   */
  it('says the review has not run, when it has not', async () => {
    const page = await codeReview(NOT_RUN)
    expect(page.find('h1').text()).toBe('The code review has not run yet.')
    expect(page.text()).toContain('not the same as nothing being found')
    expect(page.find('.rows').exists()).toBe(false)
  })

  it('says it is still writing, when it is running', async () => {
    const page = await codeReview(RUNNING)
    expect(page.find('h1').text()).toBe('The code review has not written anything yet.')
    expect(page.text()).toContain('Findings appear here as it writes them.')
  })

  it('says it found nothing, when it finished and found nothing', async () => {
    const detail = fixture(RUNNING)
    detail.layers = detail.layers.map(layer =>
      layer.number === 5 ? { ...layer, state: 'done' as const } : layer)
    await withReview(detail)

    const page = await codeReview(RUNNING)
    expect(page.find('h1').text()).toBe('The code review found nothing to say.')
    expect(page.text()).toContain('a result rather than an absence')
  })

  it('says the step failed, when it did', async () => {
    const detail = fixture(RUNNING)
    detail.layers = detail.layers.map(layer =>
      layer.number === 5 ? { ...layer, state: 'failed' as const } : layer)
    await withReview(detail)

    const page = await codeReview(RUNNING)
    expect(page.find('h1').text()).toBe('The code review did not finish.')
    expect(page.text()).toContain('says nothing at all about the code')
  })
})

/* ================================= Containment ================================== */

describe('a closed row is one line', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('shows the finding’s title, number, place, method and part — and nothing else', async () => {
    const page = await codeReview(REAL)
    const row = page.findAll('.row').find(one => one.find('.key').text() === 'F9')!

    expect(row.text()).toContain('Blocker')
    expect(row.text()).toContain('F9')
    expect(row.text()).toContain('The column policy is skipped entirely')
    expect(row.text()).toContain('app/Reporting/ColumnPolicy.php:52')
    expect(row.text()).toContain('Reproduced')
    expect(row.text()).toContain('Allowed columns on the report definition')

    /* And none of what the open row shows. */
    expect(row.text()).not.toContain('Why it matters')
    expect(row.text()).not.toContain('Suggested fix')
    expect(row.text()).not.toContain('The full reasoning')
    expect(row.text()).not.toContain('Your call')
  })

  /*
   * **A finding with no title shows its claim clamped to two lines**, which is the
   * criterion's own wording: "With no title the row shows the claim clamped to two lines,
   * never the whole of it."
   *
   * jsdom cannot measure two lines, so what is asserted is the declaration that produces
   * them — the class the stylesheet clamps, present only when there is no title. The
   * stylesheet itself is read by `controls.nuxt.spec.ts`, and the browser is what shows the
   * result.
   */
  it('clamps the claim when a finding has no title, and only then', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9 ? { ...finding, title: null } : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    const untitled = page.findAll('.row').find(one => one.find('.key').text() === 'F9')!
    const titled = page.findAll('.row').find(one => one.find('.key').text() === 'F1')!

    expect(untitled.find('.title').classes()).toContain('title--clamped')
    expect(untitled.text()).toContain('The column policy applies only where')
    expect(titled.find('.title').classes()).not.toContain('title--clamped')
  })

  it('opens one finding at a time', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.findAll('.detail')).toHaveLength(1)

    await openFinding(page, 'F1')
    expect(page.findAll('.detail')).toHaveLength(1)
    expect(page.find('.detail').text()).toContain('An empty allowed-columns list')
  })

  it('closes the one that is open when it is pressed again', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    await openFinding(page, 'F9')
    expect(page.findAll('.detail')).toHaveLength(0)
  })

  it('closes whatever is open when the list or the part changes', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.findAll('.detail')).toHaveLength(1)

    await press(page, 'Set aside')
    expect(page.findAll('.detail')).toHaveLength(0)
  })
})

describe('the reasoning', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * DEV-878: "The reasoning is never shown unfolded." On this review it runs to 221 words
   * and on the review the board was drawn from, 326. The word count is on the button
   * because it is the measurement a reader needs *before* deciding to read it.
   */
  it('is folded, behind its own word count', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')

    const fold = page.findAll('button').find(one => one.text().includes('The full reasoning'))!
    expect(fold.text()).toMatch(/The full reasoning · \d+ words/)
    expect(fold.attributes('aria-expanded')).toBe('false')
    expect(page.find('.blocks').exists()).toBe(false)
  })

  it('opens as the labelled blocks the record’s own paragraphs name', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    await press(page, 'The full reasoning')

    const labels = page.findAll('.block-label').map(one => one.text())
    expect(labels).toContain('Mechanism')
    expect(labels).toContain('How we know')
    expect(labels).toContain('Limits')
    expect(page.find('.blocks').text()).toContain('Eloquent\'s array cast')
  })

  /*
   * **The count and the blocks come from one function over one string.** A count taken from
   * a different string than the blocks were built from is a label that lies about what
   * opens, and it is the one number on this page a reader will check against what they see.
   */
  it('counts the words of the text it actually shows', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')

    const fold = page.findAll('button').find(one => one.text().includes('The full reasoning'))!
    const said = Number(/· (\d+) words/.exec(fold.text())![1])
    await fold.trigger('click')
    await settle(page)

    const shownWords = page.findAll('.block-text')
      .map(one => one.text().split(/\s+/).filter(word => /[\p{L}\p{N}]/u.test(word)).length)
      .reduce((total, words) => total + words, 0)
    /*
     * Not equal: stripping "Mechanism:" from a paragraph removes a word from what is drawn
     * and not from what was recorded. Within the number of blocks is the bound that means
     * "the same text", and it is checked rather than hand-waved.
     */
    expect(Math.abs(said - shownWords)).toBeLessThanOrEqual(page.findAll('.block').length * 4)
  })

  it('folds again, and draws nothing for a finding with no evidence', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9 ? { ...finding, evidence: null } : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.text()).not.toContain('The full reasoning')
  })
})

/* ============================ The code and the comment ========================== */

describe('the code, with the comment on the line', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('draws the hunk and puts the comment under the flagged line', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')

    const lines = page.findAll('.line')
    expect(lines.length).toBeGreaterThan(5)

    const flagged = page.findAll('.line').filter(one => one.classes().includes('line--flagged'))
    expect(flagged, 'exactly one line is flagged').toHaveLength(1)
    expect(flagged[0]!.text()).toContain('if ($allowed === [])')

    /* The comment is the element immediately after it, which is what "on the line" means. */
    const after = flagged[0]!.element.nextElementSibling
    expect(after?.className).toContain('note')
    expect(after?.textContent).toContain('Your comment on this line')
    expect(after?.textContent).toContain('This early return makes an empty allowed list')
  })

  /*
   * **More than colour.** The criterion asks for it, and one of the three channels has to
   * work for a reader who cannot see the wash: a bar down the left edge (a border, not a
   * background), `aria-current`, and the comment's own hidden sentence naming the line.
   */
  it('marks the flagged line by more than colour', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')

    const flagged = page.find('.line--flagged')
    expect(flagged.attributes('aria-current')).toBe('true')
    expect(page.find('.note .rd-visually-hidden').text()).toContain('which is line 52')
  })

  it('numbers an added or unchanged line and gives a removed one none', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F3')

    const removed = page.findAll('.line').filter(one => one.classes().includes('line--removed'))
    expect(removed.length).toBeGreaterThan(0)
    for (const line of removed) expect(line.find('.number').text()).toBe('')
    /* And a removal is drawn with `−`, not the `-` a diff uses. */
    expect(removed[0]!.find('.mark').text()).toBe('−')
  })

  /*
   * A hunk whose every line is unchanged: the session read the lines around a finding out
   * of the head checkout because the pull request did not touch them. Drawn as code and
   * said in words, which is better than the sentence saying there is no code.
   */
  it('says in words when the pull request changed none of the lines', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F12')

    expect(page.find('.code').exists()).toBe(true)
    expect(page.find('.unchanged').text()).toContain(
      'The pull request changed none of these lines',
    )
  })

  it('does not say it of a hunk that draws a change', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.find('.unchanged').exists()).toBe(false)
  })

  /*
   * DEV-878: "A finding with no code says which of three things is true: its file is not in
   * the change, it is about the description, or the session recorded none." Since a session
   * records an unchanged hunk for the first of those, two of the three are no-code cases —
   * and the third has its own test above.
   */
  it('says a finding is about the pull request when it names no file', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F14')

    expect(page.find('.no-code').text()).toContain('names no file')
    expect(page.find('.no-code').text()).toContain('“description” lens')
    expect(page.find('.where').exists()).toBe(true)
  })

  /*
   * **DEV-936's first criterion, on step 5.** The header is `ReviewStepPage`'s and is
   * tested there, but the criterion names each page — so each page a spec already mounts
   * says it for itself rather than inheriting the claim from the component.
   */
  it('links the pull request in its header', async () => {
    const page = await codeReview(REAL)
    const pull = page.find('.where a.header-out')
    expect(pull.exists()).toBe(true)
    expect(pull.attributes('href')).toBe('https://github.com/acme/portal/pull/514')
    expect(pull.attributes('rel')).toBe('noopener noreferrer')
  })

  it('says the session recorded no code when it recorded none', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F4')

    expect(page.find('.no-code').text()).toContain('The session recorded no code')
    expect(page.find('.no-code').text()).toContain('app/Reporting/ScheduleToContext.php')
  })

  it('says a file outside the change is outside it, for a record with no hunk', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 12 ? { ...finding, hunk: null } : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    await openFinding(page, 'F12')
    expect(page.find('.no-code').text()).toContain('is not part of this change')
  })

  /*
   * A comment the hunk draws no line for. The server refuses a write that anchors one
   * anywhere else, so this is a record written before that rule — and dropping the comment
   * would lose the one thing the author actually reads.
   */
  it('still shows a comment the hunk cannot place', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9 ? { ...finding, flagged_line: null } : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.find('.adrift').text()).toContain('which this hunk draws no line for')
    expect(page.find('.adrift').text()).toContain('This early return makes an empty')
  })

  it('shows the proof and its caption, told apart', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')

    expect(page.find('.proof').text()).toContain('php artisan export:run 41')
    expect(page.find('.proof-note').text()).toContain('One local run against a seeded')
    /* The caption is not inside the monospace block, which is what its own field is for. */
    expect(page.find('.proof').text()).not.toContain('One local run')
  })
})

/* ================================ The parts strip =============================== */

describe('the parts strip', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('draws a tile per part, plus everything and what no part claims', async () => {
    const page = await codeReview(REAL)
    const tiles = page.findAll('.tile-name').map(one => one.text())

    expect(tiles[0]).toBe('Everything')
    expect(tiles).toContain('The export context')
    expect(tiles).toContain('Tests and generated types')
    expect(tiles.at(-1)).toBe('Outside the parts')
  })

  it('tallies each tile, and says so when a part holds nothing', async () => {
    const page = await codeReview(REAL)
    const tiles = page.findAll('.tile').map(one => one.text())

    expect(tiles.find(text => text.includes('Tests and generated types')))
      .toContain('Nothing found')
    expect(tiles.find(text => text.includes('The export context')))
      .toContain('1 to post · 1 not')
  })

  /*
   * **The tiles' counts sum to the review's own total.** A finding is assigned to the first
   * part that claims its file, so a file two parts both name does not put its findings in
   * both — which would be a strip of counts that added up to more than there were.
   */
  it('accounts for every finding exactly once', async () => {
    const page = await codeReview(REAL)
    const counts = page.findAll('.tile').slice(1).map((tile) => {
      const text = tile.find('.tile-tally').text()
      if (text === 'Nothing found') return 0
      const [, post, rest] = /(\d+) to post(?: · (\d+) not)?/.exec(text)!
      return Number(post) + Number(rest ?? 0)
    })
    expect(counts.reduce((total, one) => total + one, 0)).toBe(fixture(REAL).findings.length)
  })

  it('filters the rows to one part', async () => {
    const page = await codeReview(REAL)
    await press(page, 'Allowed columns on the report definition')

    expect(page.text()).toContain('Only: Allowed columns on the report definition.')
    const keys = page.findAll('.key').map(one => one.text())
    expect(keys).toEqual(['F9', 'F6'])
  })

  it('says so when a part has nothing in the list showing', async () => {
    const page = await codeReview(REAL)
    await press(page, 'Tests and generated types')
    expect(page.find('.none').text()).toBe('Nothing here for this part.')
  })

  /*
   * **And blames the list, not the part, when the list is empty everywhere.**
   *
   * A list holding nothing in the whole review holds nothing under a part either, so
   * "Nothing here for this part" there names a filter that is not the reason — the same
   * misattribution as the default tile's, the other way round, and reachable by choosing
   * "For a ticket" on a review with nothing held back and then choosing a part.
   */
  it('blames the list, not the part, when the list is empty everywhere', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(
      finding => ({ ...finding, disposition: 'in_draft' as const }),
    )
    await withReview(detail)

    const page = await codeReview(REAL)
    await press(page, 'For a ticket')
    expect(page.find('.none').text()).toContain('Nothing in this list')
    await press(page, 'Tests and generated types')
    expect(page.find('.none').text()).toContain('Nothing in this list')
  })

  it('draws no strip at all for a brief with no parts', async () => {
    const page = await codeReview(ONE_BLOCK)
    expect(page.find('.strip').exists()).toBe(false)
    expect(page.find('.rows').exists()).toBe(true)
  })
})

/* ================================= The three lists ============================== */

describe('the three lists', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('counts each over the whole review, not over the part showing', async () => {
    const page = await codeReview(REAL)
    const segments = page.findAll('.controls .item').map(one => one.text())
    expect(segments[0]).toContain('To post')
    expect(segments[0]).toContain('12')
    expect(segments[1]).toContain('For a ticket')
    expect(segments[2]).toContain('Set aside')

    await press(page, 'The export context')
    const narrowed = page.findAll('.controls .item').map(one => one.text())
    expect(narrowed, 'the segments are about the review, not the filter').toEqual(segments)
  })

  it('orders most serious first, with a question above a nit', async () => {
    const page = await codeReview(REAL)
    const severities = page.findAll('.row .tag').map(one => one.text())
    expect(severities[0]).toBe('Blocker')
    expect(severities.indexOf('Question')).toBeLessThan(severities.lastIndexOf('Nit'))
    expect(severities.lastIndexOf('Should')).toBeLessThan(severities.indexOf('Question'))
  })

  it('shows why the session set a finding aside, on the list it is in', async () => {
    const page = await codeReview(REAL)
    await press(page, 'Set aside')
    await openFinding(page, 'F8')
    expect(page.text()).toContain('Why the session set it aside')
    expect(page.text()).toContain('being moved to configuration in ABC-388')
  })
})

/* ================================== The decisions =============================== */

describe('the developer’s decisions', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('offers keep, cut, regrade, rewrite and to a ticket on an open finding', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.findAll('.call .item').map(one => one.text().replace(/[YXGE]$/, '')))
      .toEqual(['Keep', 'Cut', 'Regrade', 'Rewrite', 'To a ticket'])
  })

  it('keeps the session’s own choice visible beside the developer’s', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    expect(page.find('.call .said').text()).toBe('The session put this in the review.')

    await press(page, 'Set aside')
    await openFinding(page, 'F8')
    expect(page.find('.call .said').text()).toBe('The session set this aside.')
  })

  it('cuts a finding and redraws from the record', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F10')
    await press(page, 'Cut')

    expect(page.find('.notice').text()).toContain('will not be posted')
    /* The row has left the list it was in, because the record says so. */
    expect(page.findAll('.key').map(one => one.text())).not.toContain('F10')
    await press(page, 'Set aside')
    expect(page.findAll('.key').map(one => one.text())).toContain('F10')
  })

  it('asks which severity before regrading, and will not regrade to the same one', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    await press(page, 'Regrade')

    expect(page.find('.asking').text()).toContain('Regrade to')
    expect(page.findAll('.severity').map(one => one.text()))
      .toEqual(['Blocker', 'Should', 'Nit', 'Question'])

    await press(page, 'Regrade F9')
    expect(page.find('.notice').text()).toContain('already a blocker')

    const should = page.findAll('.severity').find(one => one.text() === 'Should')!
    await should.trigger('click')
    await press(page, 'Regrade F9')

    expect(page.find('h1').text()).toContain('No blockers')
    expect(page.findAll('.row').find(one => one.find('.key').text() === 'F9')!.text())
      .toContain('Should')
  })

  it('opens a rewrite on the words that are there and keeps the session’s beside them', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F6')
    await press(page, 'Rewrite')

    const title = page.find('#rewrite-title')
    expect((title.element as HTMLInputElement).value)
      .toBe('The policy is applied after the query is built, so a hidden column is still selected')

    await title.setValue('The policy runs after the query, not in it')
    await press(page, 'Post it in my words')

    const draft = await draftPage(REAL)
    const block = draftFixture(REAL)
    expect(draft.text()).toContain('The policy runs after the query, not in it')
    expect(block, 'the fixture is unchanged; the record is what moved').toBeTruthy()
  })

  /*
   * **An emptied box is an emptied paragraph, and the record has to agree.**
   *
   * `rewrite_finding_block` is `COALESCE(?4, text)`, so `null` means *leave the paragraph
   * alone*: sending it for a box the developer had cleared kept the session's words while
   * the notice said the finding would post in the developer's. The block composes to its
   * heading alone, which `composed()` in `composition.rs` writes out as its own case and
   * the contract allows. This was changed with no test over it.
   */
  it('posts a heading alone when the developer clears the paragraph', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F6')
    await press(page, 'Rewrite')

    const text = page.find('#rewrite-text')
    const said = (text.element as HTMLTextAreaElement).value
    expect(said.length, 'the box opens on the words that are there').toBeGreaterThan(0)

    await text.setValue('')
    await press(page, 'Post it in my words')

    /*
     * Read off the draft screen rather than off `draftFixture`, which loads the fixtures
     * again and so answers about the document before any of this — the record is what
     * moved, and the page is where the record is visible.
     */
    const draft = await draftPage(REAL)
    expect(draft.text(), 'the session’s paragraph is gone').not.toContain(said)
    expect(draft.text(), 'and the heading it belonged to is still there')
      .toContain('The policy is applied after the query is built')
  })

  it('will not rewrite with a blank heading', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F6')
    await press(page, 'Rewrite')
    await page.find('#rewrite-title').setValue('   ')
    await press(page, 'Post it in my words')
    expect(page.find('.call .notice').text()).toContain('The heading cannot be blank')
  })

  it('moves a finding to a ticket, and the record says the developer did', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F3')
    await press(page, 'To a ticket')

    expect(page.find('.notice').text()).toContain('has left the review for the ticket')
    await press(page, 'For a ticket')
    expect(page.findAll('.key').map(one => one.text())).toContain('F3')
  })

  /*
   * A carried finding that this round has settled is offered the three replies rather than
   * Keep. The store refuses Keep on one — "Keep" means post this as it stands, and a
   * finding carried as fixed is not posted as an open finding — so a button that is
   * refused is worse than a button that is absent.
   */
  it('offers the three replies on a carried finding, instead of keep', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9
        ? {
            ...finding,
            continues_review_id: 40,
            continues_round: 1,
            rounds_open: 2,
            round_status: 'still_open' as const,
            round_status_checked: 'their new test fails with the fix reverted',
          }
        : finding)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 1
        ? { ...finding, continues_review_id: 40, continues_round: 1, round_status: 'fixed' as const }
        : finding)
    await withReview(detail)

    const page = await codeReview(REAL)

    /* Carried and still open: the four decisions, as for any open finding. */
    await openFinding(page, 'F9')
    expect(page.findAll('.call .item').map(one => one.text().replace(/[YXGE]$/, '')))
      .toContain('Keep')
    expect(page.find('.detail').text()).toContain('Raised in round 1 and carried into this one')
    expect(page.find('.detail').text()).toContain('Open for 2 rounds')
    expect(page.find('.detail').text()).toContain('their new test fails with the fix reverted')
  })

  it('shows a carried finding’s round status on its row', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 1
        ? {
            ...finding,
            continues_review_id: 40,
            continues_round: 1,
            round_status: 'still_open' as const,
          }
        : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    const row = page.findAll('.row').find(one => one.find('.key').text() === 'F1')!
    expect(row.text()).toContain('Still open')

    /* And a finding raised this round carries no such label: every row would say it. */
    const fresh = page.findAll('.row').find(one => one.find('.key').text() === 'F9')!
    expect(fresh.text()).not.toContain('Raised this round')
  })

  it('requires the author’s reason before accepting it', async () => {
    const detail = fixture(REAL)
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9
        ? {
            ...finding,
            continues_review_id: 40,
            continues_round: 1,
            round_status: 'accepted' as const,
          }
        : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    const offered = page.findAll('.call .item').map(one => one.text())
    expect(offered).toContain('Raise it again')
    expect(offered).toContain('Accept their reason')
    expect(offered).toContain('Withdraw')
    expect(offered, 'the store refuses Keep on a settled finding').not.toContain('Keep')

    await press(page, 'Accept their reason')
    await press(page, 'Accept it')
    expect(page.find('.call .notice').text()).toContain('Say what the author’s reason was')
  })

  /*
   * DEV-878: "When the draft is one block and cannot be decided in parts, the page says
   * that, in words, once."
   */
  it('says once when the draft is one block of markdown', async () => {
    const page = await codeReview(ONE_BLOCK)
    await openFinding(page, 'F1')

    const said = page.findAll('.notice').filter(one =>
      one.text().includes('one block of markdown'))
    expect(said, 'said once, not once per button').toHaveLength(1)
    /*
     * **"Post it", not "Keep".** The contract's table is `Keep · Post it`: one decision,
     * two labels, because the act reads differently depending on where the finding stands.
     * A draft holding no block for it means pressing this *writes* it one and posts it,
     * which is what "Keep" says the opposite of.
     */
    expect(page.findAll('.call .item').map(one => one.text().replace(/[YXGE]$/, '')))
      .toEqual(['Post it', 'To a ticket'])
  })

  /*
   * The same rule on a held-back nit, which is where the contract's wording comes from: the
   * panel says "The session held this back for a ticket" and a button beside it reading
   * "Keep" is read as *leave it held back*, while pressing it posts the finding.
   */
  it('says "Post it" on a finding the draft holds no block for, and "Keep" when it does', async () => {
    const page = await codeReview(REAL)
    await press(page, 'For a ticket')
    await openFinding(page, 'F2')

    const held = page.findAll('.call .item').map(one => one.text().replace(/[YXGE]$/, ''))
    expect(held).toContain('Post it')
    expect(held).not.toContain('Keep')

    await press(page, 'To post')
    await openFinding(page, 'F9')
    expect(page.findAll('.call .item').map(one => one.text().replace(/[YXGE]$/, '')))
      .toContain('Keep')
  })

  it('disables the controls and says why once the draft is posted', async () => {
    const page = await codeReview(POSTED)
    await openFinding(page, 'F1')

    expect(page.find('.call .notice').text()).toContain('This review is posted')
    expect(page.findAll('.call .item')).toHaveLength(0)
  })

  it('disables the controls and says why once the draft is approved', async () => {
    const page = await codeReview(REAL)
    const draft = await draftPage(REAL)
    await press(draft, 'Sign off and post')

    const again = await codeReview(REAL)
    await openFinding(again, 'F9')
    expect(again.find('.call .notice').text()).toContain('You have signed this draft off')
    expect(page.exists()).toBe(true)
  })

  it('says there is nothing to decide when no draft has been written', async () => {
    const page = await codeReview(ONE_BLOCK)
    await resetDesk({ drafts: [] })
    const fresh = await codeReview(ONE_BLOCK)
    await openFinding(fresh, 'F1')
    expect(fresh.find('.call .notice').text()).toContain('There is no draft yet')
    expect(page.exists()).toBe(true)
  })
})

/* ============================== What came back ================================== */

describe('what the author said', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * **The one thing the retired page was the only place for.** Author replies belong to
   * step 7, which has no page, so they are drawn on the finding they are about — which is
   * better than the screen they came off, where they were grouped at the top away from the
   * finding each was answering.
   */
  it('shows a harvested reply on the finding it is about', async () => {
    const page = await codeReview(POSTED)
    const replied = fixture(POSTED).outcomes.find(one => one.kind === 'replied')!
    await openFinding(page, replied.finding_key)

    /*
     * **The author's words, and not the record they were stored in.** The harvester writes
     * an outcome's detail as JSON, and this block drew it raw until DEV-958 — so a reply
     * arrived as `{"body":"…","comment_id":…}` inside the quotation meant to carry it.
     * What is asserted is the body, read out of the fixture's own detail, and that no
     * brace reaches the page.
     */
    const body = JSON.parse(replied.detail!) as { body: string, login: string }
    expect(page.find('.reply').text()).toContain(body.body)
    expect(page.find('.reply').text()).toContain(`A reply from ${body.login}`)
    expect(page.find('.reply').text()).not.toContain('{')
    expect(page.find('.reply').text()).not.toContain('comment_id')
  })

  /**
   * **Who acted, where the sentence does not already say it.**
   *
   * A reply and a resolution name their login inside the sentence, composed from the
   * detail. Every other kind does not — so replacing `by the {{ outcome.actor }}` with the
   * time alone took the actor off the page for all of them, and an agent's `raised_again`
   * read "Raised again. 3 days ago" with nobody in it.
   */
  it('names who acted for a kind whose sentence does not', async () => {
    const detail = fixture(POSTED)
    const changed = detail.outcomes.find(one => one.kind === 'code_changed_after')!
    await withReview({
      ...detail,
      outcomes: [{ ...changed, kind: 'raised_again', actor: 'harvester', detail: null }],
    })
    const page = await codeReview(POSTED)
    await openFinding(page, changed.finding_key)
    expect(page.find('.reply').text()).toContain('by the harvester')
  })

  it('does not name the actor twice where the sentence already does', async () => {
    const page = await codeReview(POSTED)
    const replied = fixture(POSTED).outcomes.find(one => one.kind === 'replied')!
    await openFinding(page, replied.finding_key)
    expect(page.find('.reply').text()).toContain('A reply from dana-okoye')
    expect(page.find('.reply').text()).not.toContain('by the author')
  })

  it('does not replay the developer’s own decisions at them', async () => {
    const page = await codeReview(REAL)
    await openFinding(page, 'F10')
    await press(page, 'Cut')
    await press(page, 'Set aside')
    await openFinding(page, 'F10')
    expect(page.find('.reply').exists()).toBe(false)
  })
})

/* ============================ Nothing is markup ================================= */

describe('every field is interpolated', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * DEV-878: "Every field is interpolated; no `v-html` on these pages." A session's text is
   * a record, and a record that could carry markup is a record that could carry a script.
   * Asserted by feeding every text field a tag and finding it rendered as text.
   */
  it('renders a recorded angle bracket as text, not as an element', async () => {
    const detail = fixture(REAL)
    const poison = '<img src=x onerror="alert(1)"> **not bold**'
    detail.findings = detail.findings.map(finding =>
      finding.seq === 9
        ? {
            ...finding,
            title: poison,
            claim: poison,
            why_it_matters: poison,
            suggested_fix: poison,
            inline_comment: poison,
            proof: poison,
            proof_note: poison,
            evidence: `Mechanism: ${poison}`,
          }
        : finding)
    await withReview(detail)

    const page = await codeReview(REAL)
    await openFinding(page, 'F9')
    await press(page, 'The full reasoning')

    expect(page.element.querySelector('img'), 'no element came out of the record').toBeNull()
    expect(page.text()).toContain('<img src=x onerror="alert(1)">')
    expect(page.text()).toContain('**not bold**')
  })
})

/*
 * **Arriving at one finding, named in the hash.** DEV-919's third criterion, from the other
 * end: "Each finding line reaches that finding on the code review page, not just the page."
 *
 * The Solution page writes `…/code-review#F9`. What has to happen here is that the right
 * list is chosen, the part filter is cleared and the row is opened — in that order, because
 * the page closes whatever is open whenever the list or the part changes.
 */
describe('arriving at a finding named in the hash', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  const at = (id: number, hash: string) =>
    shown(CodeReview, `/reviews/${id}/code-review${hash}`)

  it('opens the finding the hash names', async () => {
    const page = await at(REAL, '#F9')
    expect(page.find('#finding-9').exists(), 'F9 is open').toBe(true)
    /* Open, not merely present: one row at a time, and it is this one. */
    expect(page.findAll('li.row--open')).toHaveLength(1)
    expect(page.find('li.row--open').find('.key').text()).toBe('F9')
  })

  /**
   * **The list has to be chosen, or the row is not drawn at all.** F2 is held back, and the
   * page opens on "To post" — so a link that only set `open` would land on a page where
   * nothing had happened, which is the failure this criterion exists to prevent.
   */
  it('chooses the list the finding is in, when it is not the one the page opens on', async () => {
    const page = await at(REAL, '#F2')
    expect(page.find('#finding-2').exists(), 'F2 is open in the list it belongs to').toBe(true)
    expect(page.find('li.row--open').find('.key').text()).toBe('F2')
  })

  it('reaches a finding that was set aside', async () => {
    const page = await at(REAL, '#F8')
    expect(page.find('#finding-8').exists()).toBe(true)
  })

  it('opens nothing when there is no hash, as before', async () => {
    const page = await codeReview(REAL)
    expect(page.findAll('li.row--open')).toHaveLength(0)
  })

  /**
   * A hash naming a finding this review does not have is a stale link — a number from
   * another review, or a finding withdrawn. The page behind it is correct and complete, so
   * it draws as it would have drawn: an error over it would be the page blaming the reader
   * for the link they followed.
   */
  it('draws the page as usual for a hash naming no finding of this review', async () => {
    const page = await at(REAL, '#F999')
    expect(page.findAll('li.row--open')).toHaveLength(0)
    expect(page.findAll('li.row').length, 'and the page is drawn').toBeGreaterThan(0)
  })

  it('ignores a hash that does not name a finding at all', async () => {
    const page = await at(REAL, '#parts-strip')
    expect(page.findAll('li.row--open')).toHaveLength(0)
  })

  /**
   * **The reader's own clicks own what is open once they are here.** The arrival is an
   * arrival, not a mode: a row folded after following a link stays folded, and a later
   * re-read of the review must not drag the page back to the finding in the hash.
   */
  it('lets the row be folded again, and does not reopen it', async () => {
    const page = await at(REAL, '#F9')
    expect(page.find('#finding-9').exists()).toBe(true)

    await openFinding(page, 'F9')
    expect(page.find('#finding-9').exists(), 'folded by its own control').toBe(false)

    /* Another read of the same review — the arrival does not happen twice. */
    await withReview(fixture(REAL))
    await settle(page)
    expect(page.find('#finding-9').exists()).toBe(false)
  })

  /** Each row carries its key as the anchor, which is what the hash lands on. */
  it('gives every row the anchor a link names it by', async () => {
    const page = await codeReview(REAL)
    const rows = page.findAll('li.row')
    expect(rows.length).toBeGreaterThan(0)
    for (const row of rows) {
      expect(row.attributes('id')).toBe(row.find('.key').text())
    }
  })
})
