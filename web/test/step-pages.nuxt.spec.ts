// @vitest-environment nuxt
import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import Understand from '../app/pages/reviews/[id]/understand.vue'
import Alternatives from '../app/pages/reviews/[id]/alternatives.vue'
import Solution from '../app/pages/reviews/[id]/solution.vue'

import type { ReviewDetail } from '../app/types/desk'

/*
 * The three step pages DEV-885 builds — Understand, Alternatives and Solution — mounted
 * against the records.
 *
 * **Review 41 is the contract's invented example at real length**, the same brief
 * `contract/examples/confirmBrief.response.real-length.json` holds, carried into the
 * fixtures with thirteen findings on the file paths its parts name. It is the review the
 * screenshots are taken of, so a test here and a picture in the pull request are about the
 * same page.
 *
 * **Review 17's brief was written the old way** — `problem`, `diagram` and `provenance` and
 * nothing else — which is what every brief recorded before 10 October 2026 looks like. Its
 * acceptance criterion is that the Understand page still shows it, so that state is tested
 * here rather than being a comment.
 *
 * These check that each page renders from the data layer, that every state the brief has is
 * drawn from the record rather than assumed, and that the rules the issue states hold. They
 * do not check that it looks right; the screenshots in the pull request are for that.
 */

/** The review carrying the structured brief at real length. */
const STRUCTURED = 41
/** A review whose brief was written before the structured one existed. */
const OLD_WAY = 17

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

async function shown(
  component: Parameters<typeof mountSuspended>[0],
  route: string,
) {
  /*
   * **Attached to the document, which the focus assertions need.** `element.focus()` on a
   * detached tree does nothing and leaves `document.activeElement` at the body, so a test
   * of "Escape returns the focus" would fail against a component that does it correctly.
   */
  const page = await mountSuspended(component, { route, attachTo: document.body })
  await settle(page)
  return page
}

const understand = (id: number) => shown(Understand, `/reviews/${id}/understand`)
const alternatives = (id: number) => shown(Alternatives, `/reviews/${id}/alternatives`)
const solution = (id: number) => shown(Solution, `/reviews/${id}/solution`)

/** Click the button whose text contains this, and let what it started finish. */
async function open(page: VueWrapper, within: string) {
  const found = page.findAll('button').find(one => one.text().includes(within))
  if (!found) throw new Error(`no button whose text contains ${within}`)
  await found.trigger('click')
  await settle(page)
  return found
}

/**
 * Open the card on one step of the trail, and hand back the control that opened it.
 *
 * By number, because that is how the trail is read: "step 2" is the thing on screen.
 */
async function openCard(page: VueWrapper, number: number) {
  const step = page.findAll('.steps li')[number - 1]
  if (!step) throw new Error(`the trail has no step ${number}`)
  const control = step.find('.outcome')
  if (control.element.tagName !== 'BUTTON') {
    throw new Error(`step ${number} recorded no facts, so it has no card`)
  }
  await control.trigger('click')
  await settle(page)
  return control
}

/** The review the fixtures hold, as an object a test can change one field of. */
function fixture(id: number): ReviewDetail {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return found
}

/** The same records, with one review replaced. */
async function withReview(detail: ReviewDetail) {
  const reviews = loadSource().reviews.map(review =>
    review.review.id === detail.review.id ? detail : review)
  await resetDesk({ reviews })
}

/* ================================== The shell ================================== */

describe('the shell every step page sits in', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('says which review you are in, above everything else', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('acme/portal')
    expect(page.text()).toContain('#514')
    expect(page.text()).toContain('Give a scheduled export its own context')
  })

  it('draws seven steps, in the two groups a review actually has', async () => {
    const page = await understand(STRUCTURED)
    const steps = page.findAll('.steps li').map(one => one.text())
    expect(steps).toHaveLength(7)
    expect(page.text()).toContain('The pull request · settled once')
    expect(page.text()).toContain('Round 1 · repeats each round')
    /* Four settled once, three repeating: the split is a fact about the review. */
    const groups = page.findAll('.trail .group')
    expect(groups).toHaveLength(2)
    expect(groups[0]!.findAll('.steps li')).toHaveLength(4)
    expect(groups[1]!.findAll('.steps li')).toHaveLength(3)
  })

  it('shows each step’s own outcome in a few words', async () => {
    const page = await understand(STRUCTURED)
    const trail = page.find('.trail').text()
    /* The store's own words, not a restatement of the state. */
    expect(trail).toContain('passes, 2 warnings')
    expect(trail).toContain('8 options, C built')
    expect(trail).toContain('14 findings, 1 blocker')
    expect(trail).toContain('waiting on you')
  })

  it('falls back to the state for a step whose outcome was not recorded', async () => {
    const detail = fixture(STRUCTURED)
    detail.layers = detail.layers.map(layer =>
      layer.name === 'harvest' ? { ...layer, detail: null } : layer)
    await withReview(detail)

    const page = await understand(STRUCTURED)
    /* "not yet", rather than an empty cell where an outcome should be. */
    expect(page.find('.trail').text()).toContain('not yet')
  })

  it('shows how long a finished step took, on the step’s own card', async () => {
    const page = await understand(STRUCTURED)

    /*
     * **The figure moved off the trail in DEV-917.** It was a third line under the name,
     * and a third line is what the one-line rule is against: the trail is a map now, and
     * how long a step took is one of the facts behind it.
     */
    expect(page.find('.trail').text()).not.toMatch(/\d+ (sec|min|hr) (?!ago)/)

    await openCard(page, 2)
    const card = page.find('.floating')
    expect(card.exists(), 'the card opened').toBe(true)
    expect(card.text()).toContain('Understand')
    expect(card.text(), 'and says how long the step took').toMatch(/\d+ (sec|min|hr|day)/)
  })

  it('says a finished step’s duration is not recorded rather than drawing a nought', async () => {
    // **A review from before DEV-890**: the record holds one time per step, the finish,
    // and no start at all. Every figure that would need the start is null — and a nought
    // in its place would say the step took no time.
    const detail = fixture(STRUCTURED)
    detail.layers = detail.layers.map(layer =>
      layer.state === 'done'
        ? { ...layer, seconds: null, runs: [{
            state: null,
            started_at: null,
            ended_as: 'done' as const,
            finished_at: layer.updated_at ?? null,
            seconds: null,
            detail: null,
          }] }
        : layer)
    detail.timings = {
      ...detail.timings,
      with_the_agent_seconds: null,
      waiting_on_you_seconds: null,
    }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    await openCard(page, 2)
    const card = page.find('.floating').text()
    expect(card).toContain('time not recorded')
    expect(card).not.toMatch(/\b0 sec\b/)
  })

  /**
   * **A step draws one line and the rest floats.** DEV-917: a session wrote 34 words into a
   * step's `detail` on the review this was read against, and the column is about 150px
   * wide. DEV-892 clamped that to two lines and offered "Read the rest", which opened it in
   * place and moved everything below it; this draws one ellipsised line and opens the facts
   * on a card that overlays the page.
   *
   * The ellipsis and the overlaying are CSS and jsdom computes no layout, so what is
   * asserted here is everything either side of them: that the whole string is still in the
   * document, that the line is the control, that the card carries the facts, and that the
   * keyboard can open and close it. The two measurements — one line at both widths, and
   * nothing moving when the card opens — are made in a real browser, in
   * `e2e/review-desk.spec.ts`.
   */
  it('draws a long step detail on one line and opens its facts on a card', async () => {
    const page = await understand(STRUCTURED)

    const understandStep = page.findAll('.steps li')[1]!
    expect(understandStep.text()).toContain('2 Understand')

    const line = understandStep.find('.outcome')
    expect(line.exists()).toBe(true)
    expect(
      line.text(),
      'the whole string is in the document; only the drawing is cut',
    ).toContain('the options pass was asked for anything at all')
    expect(line.element.tagName, 'the line is the control').toBe('BUTTON')
    expect(line.attributes('aria-expanded')).toBe('false')
    expect(page.find('.floating').exists()).toBe(false)

    await line.trigger('click')
    await settle(page)

    const card = page.find('.floating')
    expect(card.exists()).toBe(true)
    expect(card.attributes('role')).toBe('group')
    expect(card.attributes('aria-label')).toContain('step 2 Understand')
    /* The facts the example's step 2 recorded, as label and value. */
    expect(card.text()).toContain('Problems')
    expect(card.text()).toContain('486 lines, 12 files')
    expect(
      page.findAll('.steps li')[1]!.find('.outcome').attributes('aria-expanded'),
    ).toBe('true')
  })

  it('opens one card at a time', async () => {
    const page = await understand(STRUCTURED)

    await openCard(page, 2)
    expect(page.findAll('.floating')).toHaveLength(1)
    await openCard(page, 3)
    const cards = page.findAll('.floating')
    expect(cards).toHaveLength(1)
    expect(cards[0]!.text()).toContain('Alternatives')
  })

  it('closes the card on a second press, and on Escape', async () => {
    const page = await understand(STRUCTURED)

    const control = await openCard(page, 2)
    expect(page.find('.floating').exists()).toBe(true)

    await control.trigger('click')
    await settle(page)
    expect(page.find('.floating').exists(), 'a second press closes it').toBe(false)

    /*
     * **By keyboard alone, and the focus comes back.** A reader who pressed Escape and
     * found the focus at the top of the document would be the trail's whole length away
     * from the step they were reading.
     */
    const again = page.findAll('.steps li')[1]!.find('.outcome')
    ;(again.element as HTMLButtonElement).focus()
    await again.trigger('keydown', { key: 'Enter' })
    await again.trigger('click')
    await settle(page)
    expect(page.find('.floating').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await settle(page)
    expect(page.find('.floating').exists(), 'Escape closes it').toBe(false)
    expect(
      document.activeElement,
      'and the focus is back on the step that opened it',
    ).toBe(page.findAll('.steps li')[1]!.find('.outcome').element)
  })

  it('closes the card when something outside it is pressed', async () => {
    const page = await understand(STRUCTURED)
    await openCard(page, 2)
    expect(page.find('.floating').exists()).toBe(true)

    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    await settle(page)
    expect(page.find('.floating').exists()).toBe(false)
  })

  it('draws no card and no control on a step that recorded no facts', async () => {
    const page = await understand(STRUCTURED)
    const steps = page.findAll('.steps li')

    /* Step 4 recorded none, and step 7 has not run. */
    for (const index of [3, 6]) {
      const step = steps[index]!
      expect(step.find('.outcome').element.tagName, step.text()).toBe('SPAN')
      expect(step.find('button').exists()).toBe(false)
    }

    /*
     * Five of the seven, which is the measurement worth making: a control under every step
     * would be a row of buttons, two of which open nothing.
     */
    expect(page.findAll('.trail button')).toHaveLength(5)
  })

  it('shows the total with the agent and the total waiting on the developer', async () => {
    const page = await understand(STRUCTURED)
    const totals = page.find('.totals')
    expect(totals.exists()).toBe(true)
    expect(totals.text()).toContain('with the agent')
    expect(totals.text()).toContain('waiting on you')
    expect(totals.text()).toMatch(/\d+ (min|hr|day).{0,4} with the agent/)
    expect(totals.text()).toContain('requested')
  })

  it('says which total is not recorded rather than showing a nought', async () => {
    const detail = fixture(STRUCTURED)
    detail.timings = {
      ...detail.timings,
      draft_ready_at: null,
      with_the_agent_seconds: null,
      waiting_on_you_seconds: null,
    }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('with the agent: not recorded yet')
    expect(page.text()).toContain('waiting on you: not recorded')
  })

  it('links the five steps that have somewhere to go, and no others', async () => {
    const page = await understand(STRUCTURED)
    const links = page.findAll('.trail a').map(one => one.attributes('href'))
    /*
     * **The trail is the only way to four of these.** Every row in the queue opens
     * Understand, so if the trail did not reach the findings and the draft the queue would
     * lead somewhere a reader could not leave. Steps 1 and 7 have no page, and a link to a
     * page that does not exist is worse than a label.
     */
    expect(links).toEqual([
      `/reviews/${STRUCTURED}/understand`,
      `/reviews/${STRUCTURED}/alternatives`,
      `/reviews/${STRUCTURED}/solution`,
      `/reviews/${STRUCTURED}/code-review`,
      `/reviews/${STRUCTURED}/draft`,
    ])
    /* Lint and Harvest are labels. */
    const labels = page.findAll('.trail .face:not(a)').map(one => one.text())
    for (const step of ['1 Lint', '7 Harvest']) {
      expect(labels.some(one => one.includes(step)), `${step} should be a label`).toBe(true)
    }
  })

  /*
   * **Step 6 is a link whatever the draft status says, and that is DEV-878's doing.**
   *
   * It used to be guarded: `GET …/draft` refuses when there is no draft row, and the draft
   * screen then drew a blocking not-found panel carrying no trail and no way back — the
   * dead end steps 1 and 7 are labels to avoid. The draft screen is inside the shell now,
   * so it carries the trail even when there is nothing to read, and a review whose session
   * has not written one says so on step 6's own page. The guard went with the dead end.
   *
   * Three statuses, because the guard distinguished them and nothing does any more: a
   * session part-way through writing one, a review no session has drafted at all, and one
   * waiting on the developer.
   */
  it('links step 6 whatever the draft status says', async () => {
    for (const status of ['ready', 'drafting', null] as const) {
      const detail = fixture(STRUCTURED)
      detail.draft_status = status
      await withReview(detail)

      const page = await understand(STRUCTURED)
      expect(
        page.findAll('.trail a').map(one => one.attributes('href')),
        `step 6 should be a link with draft_status ${String(status)}`,
      ).toContain(`/reviews/${STRUCTURED}/draft`)
    }
  })

  it('still says which step is waiting, whichever page you are on', async () => {
    const page = await understand(STRUCTURED)
    /*
     * Step 6 is a link now, and it is also the step waiting on the developer. The two
     * facts are drawn by different things — `face--current` is the review's state,
     * `face--here` is the browser's — and linking it changed neither.
     */
    const waiting = page.findAll('.trail .face--current')
    expect(waiting).toHaveLength(1)
    expect(waiting[0]!.text()).toContain('6 Draft')
    /*
     * What the step came to is the line under the face rather than inside it, because the
     * face is a link and the line is the card's control.
     */
    const step = page.findAll('.steps li').find(one => one.text().includes('6 Draft'))!
    expect(step.text()).toContain('waiting on you')
    expect(page.find('.trail .face--here').text()).toContain('2 Understand')
  })

  it('marks the step you are on, on each of the three pages', async () => {
    for (const [page, name] of [
      [await understand(STRUCTURED), '2 Understand'],
      [await alternatives(STRUCTURED), '3 Alternatives'],
      [await solution(STRUCTURED), '4 Solution map'],
    ] as const) {
      const current = page.find('.trail [aria-current="page"]')
      expect(current.exists()).toBe(true)
      expect(current.text()).toContain(name)
    }
  })

  it('says a review is not on the desk rather than drawing an empty page', async () => {
    for (const make of [understand, alternatives, solution]) {
      const page = await make(999)
      expect(page.text()).toContain('Not on the desk')
    }
  })
})

/* ============================ Layer 2, the one act ============================= */

/*
 * **Confirming the brief is on Understand because the page it was on is gone.**
 *
 * `/reviews/{id}` drew it and DEV-878 retires that screen. Without this the dashboard would
 * have no way to confirm a brief at all — and `confirmBrief` is one of only two operations
 * that record the developer's sign-off, so `crates/cli/tests/signoff.rs` would be guarding a
 * boundary with nothing on the far side of it. The tests that were on the old screen are
 * these.
 */
describe('confirming the brief', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('asks for it while the brief is unconfirmed', async () => {
    const page = await understand(OLD_WAY)
    expect(page.text()).toContain('Confirm the problem before anything is posted')
    expect(page.findAll('button').some(one => one.text() === 'Confirm the problem')).toBe(true)
  })

  it('records it, and says the developer did', async () => {
    const page = await understand(OLD_WAY)
    await open(page, 'Confirm the problem')

    expect(page.text()).toContain('Problem confirmed.')
    expect(page.text()).toContain('The draft is yours to sign off.')
    expect(page.findAll('button').some(one => one.text() === 'Confirm the problem')).toBe(false)
  })

  it('reads as done on a brief already confirmed, with no button', async () => {
    const detail = fixture(OLD_WAY)
    detail.brief = { ...detail.brief!, confirmed: true, confirmed_at: '2026-10-09T10:00:00Z' }
    await withReview(detail)

    const page = await understand(OLD_WAY)
    expect(page.text()).toContain('Problem confirmed.')
    expect(page.findAll('button').some(one => one.text() === 'Confirm the problem')).toBe(false)
  })

  /*
   * **The third state, and the defect that went with the page.**
   *
   * The old screen wrote "The session recorded this brief as needing none: {{ field }}." —
   * and since DEV-883 a session writes a brief twice, with that field reading "problem half
   * only" between the writes. The sentence then gave "problem half only" as the reason no
   * confirmation was needed, which it is not. DEV-878's own comment says the problem goes
   * with the page; this is the assertion that it did not come back.
   */
  it('says confirmation was not required, without giving the field as the reason', async () => {
    const detail = fixture(OLD_WAY)
    detail.brief = {
      ...detail.brief!,
      confirmation_required: false,
      confirmation_not_required_because: 'problem half only',
    }
    await withReview(detail)

    const page = await understand(OLD_WAY)
    expect(page.text()).toContain('Confirmation not required.')
    expect(page.text()).toContain('nothing here is waiting on you')
    expect(page.findAll('button').some(one => one.text() === 'Confirm the problem')).toBe(false)

    /* The field is shown as what the session wrote, under a label that says so. */
    expect(page.find('.why').text()).toContain('What the session wrote')
    expect(page.find('.why').text()).toContain('problem half only')
    expect(page.text()).not.toContain('needing none: problem half only')
  })

  it('is drawn on a structured brief too, not only on an old one', async () => {
    const page = await understand(STRUCTURED)
    /* The real-length example's brief is confirmed, which is the second of the three. */
    expect(page.text()).toContain('Problem confirmed.')
  })
})

/* ================================== Understand ================================== */

describe('Understand', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('opens with the brief’s one headline sentence', async () => {
    const page = await understand(STRUCTURED)
    expect(page.find('h1').text()).toBe(
      'Scheduled exports get their own context, so the report builder no longer reads '
      + 'whichever tenant a web request left behind.',
    )
  })

  it('draws both flows as boxes, in the rows the record holds', async () => {
    const page = await understand(STRUCTURED)
    const flows = page.findAll('.flow')
    expect(flows).toHaveLength(2)
    /* Five rows before, six after, which is what the example records. */
    expect(flows[0]!.findAll('.row')).toHaveLength(5)
    expect(flows[1]!.findAll('.row')).toHaveLength(6)
    /* Nothing was handed to mermaid: a flow is HTML, and this page draws no figure. */
    expect(page.find('figure').exists()).toBe(false)
  })

  it('captions each flow from the record, never from the page', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('The tenant arrives through a slot the web also writes')
    expect(page.text()).toContain(
      'The export carries its own context, and both paths share one chain',
    )
  })

  it('draws a split row as a row of two, side by side', async () => {
    const page = await understand(STRUCTURED)
    const before = page.findAll('.flow')[0]!
    const split = before.findAll('.row').find(row => row.findAll('.box').length === 2)
    expect(split).toBeTruthy()
    expect(split!.text()).toContain('Web export')
    expect(split!.text()).toContain('Scheduled export')
    /* Two columns, from the row's own length. The page assigns no position. */
    expect(split!.attributes('style')).toContain('--columns: 2')
  })

  it('names a problem on the flow, red before and green after', async () => {
    const page = await understand(STRUCTURED)
    const [before, after] = page.findAll('.flow')
    const wrong = before!.findAll('.tag--wrong')
    const fixed = after!.findAll('.tag--fixed')
    expect(wrong.length).toBeGreaterThan(0)
    expect(fixed.length).toBeGreaterThan(0)

    /*
     * **The name, which is DEV-917's defect 2.** The marks read 1, 1, 2, 3, 1 down the
     * flow on the review this was read against: every number right, and a bare number
     * down a flow reads as a step count.
     */
    expect(wrong[0]!.text()).toContain('The shared slot')

    /*
     * The same number is on both sides, which is the whole mechanism of the picture. Read
     * off the circle: a tag's full text is the numeral, the name, and the words behind
     * it, which is the point of the words but makes `.text()` the wrong thing to compare.
     */
    const numerals = (tags: typeof wrong) =>
      tags.map(one => one.find('.circle').text())
    expect(numerals(wrong)).toContain('1')
    expect(numerals(fixed)).toContain('1')
    /* And neither flow carries the other's colour. */
    expect(before!.findAll('.tag--fixed')).toHaveLength(0)
    expect(after!.findAll('.tag--wrong')).toHaveLength(0)
  })

  it('carries the same name on every box one problem touches', async () => {
    /*
     * **One problem on four boxes, which is the shape the defect was found in.** Problem 1
     * touched four boxes of the flow and the column read 1, 1, 2, 3, 1; four tags carrying
     * one name is what makes that repetition legible instead of confusing.
     */
    const detail = fixture(STRUCTURED)
    detail.brief = {
      ...detail.brief!,
      flow_before: [
        [{ title: 'One', note: null, problem: 1 }],
        [{ title: 'Two', note: null, problem: 1 }],
        [
          { title: 'Three', note: null, problem: 1 },
          { title: 'Four', note: null, problem: 1 },
        ],
      ],
    }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    const tags = page.findAll('.flow--wrong .tag')
    expect(tags).toHaveLength(4)
    for (const tag of tags) {
      expect(tag.text()).toContain('The shared slot')
      expect(tag.find('.circle').text()).toBe('1')
    }

    /* And the row above the flows lists it once, however many boxes it is on. */
    const key = page.find('.key')
    expect(key.findAll('.tag')).toHaveLength(3)
    expect(key.text()).toContain('3 problems')
  })

  it('calls a problem with no name "Problem 1", never the bare number', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = {
      ...detail.brief!,
      problems: detail.brief!.problems!.map(problem => ({ ...problem, label: null })),
    }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    const tags = page.findAll('.flow--wrong .tag')
    expect(tags.length).toBeGreaterThan(0)
    expect(tags[0]!.text()).toContain('Problem 1')
    expect(page.find('.key').text()).toContain('Problem 2')
  })

  it('never leaves colour carrying a mark on its own', async () => {
    const page = await understand(STRUCTURED)
    const [before, after] = page.findAll('.flow')
    /*
     * The words are in the accessible name, so a reader who cannot tell the two hues apart
     * still reads which side of the picture a tag is on.
     */
    expect(before!.find('.tag--wrong').text()).toContain('what is wrong')
    expect(after!.find('.tag--fixed').text()).toContain('what fixes it')
    /* And the number itself is visible in both. */
    expect(before!.find('.tag--wrong .circle').text()).toMatch(/^\d+$/)
  })

  it('lists what was wrong and what it is now, one line each', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('3 things were wrong')
    const rows = page.findAll('.problem')
    expect(rows).toHaveLength(3)
    expect(rows[0]!.text()).toContain(
      'A scheduled export read its tenant from a slot the web middleware also writes.',
    )
    expect(rows[0]!.text()).toContain(
      'The schedule carries its own context, and nothing else writes to it.',
    )
  })

  it('says so when the change half of a problem is not written yet', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.problems = detail.brief!.problems.map((problem, at) =>
      at === 0 ? { ...problem, now_fixed: null } : problem)
    await withReview(detail)

    const page = await understand(STRUCTURED)
    /*
     * `now_fixed` is nullable by design — the problem half can be written and handed to a
     * blind pass before the change is described — so the absence is a state with words.
     */
    expect(page.findAll('.problem')[0]!.text()).toContain('not written yet')
  })

  it('opens a problem’s named detail in place, and navigates nowhere', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).not.toContain('The request-scoped singleton')

    const row = await open(page, 'A scheduled export read its tenant')

    expect(row.attributes('aria-expanded')).toBe('true')
    expect(row.attributes('aria-controls')).toBe('problem-1')
    /* The detail is a sibling on the same page, with the id the row names. */
    const detail = page.find('#problem-1')
    expect(detail.exists()).toBe(true)
    expect(detail.text()).toContain('The request-scoped singleton')
    expect(detail.text()).toContain('is written by the web middleware on every request')
    expect(detail.text()).toContain('is constructed by the schedule')
    /* Nothing on this page is a link to GitHub, let alone the only way to read this. */
    expect(page.findAll('a').map(one => one.attributes('href') ?? ''))
      .not.toContain('https://github.com')
  })

  it('picks the open problem’s boxes out of both flows', async () => {
    const page = await understand(STRUCTURED)
    expect(page.findAll('.box--focus')).toHaveLength(0)

    await open(page, 'A scheduled export read its tenant')

    /*
     * Problem 1 is on two boxes before and two after. One row's state changing three
     * places at once is why these rows are a button and not a `<details>`.
     */
    const focused = page.findAll('.box--focus')
    expect(focused.length).toBe(4)
    for (const box of focused) expect(box.text()).toContain('1')
  })

  it('closes a problem that is open, so one is open at a time', async () => {
    const page = await understand(STRUCTURED)
    await open(page, 'A scheduled export read its tenant')
    expect(page.find('#problem-1').exists()).toBe(true)

    await open(page, 'The report builder had to know')
    expect(page.find('#problem-1').exists()).toBe(false)
    expect(page.find('#problem-2').exists()).toBe(true)
  })

  it('draws the supporting table from the record, heading and all', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('The idea: three questions, each with one owner')
    const table = page.findAll('table').find(one =>
      one.text().includes('Which tenant is this export for?'))
    expect(table).toBeTruthy()
    expect(table!.findAll('th').map(one => one.text())).toEqual([
      'The question',
      'Who should answer it',
      'Before',
      'After',
    ])
    expect(table!.findAll('tbody tr')).toHaveLength(3)
    /* Each cell says which column it is, for the layout at a phone's width. */
    expect(table!.find('tbody td').attributes('data-label')).toBe('The question')
  })

  it('draws how the system works today, in the session’s own words', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('How it works today')
    expect(page.text()).toContain('Every report is built by one class.')
  })

  /**
   * **Three levels, in order: the decision, what is still open, then the sources.**
   *
   * DEV-917's defect 4. This was a heading, a small paragraph and seven source lines that
   * all looked alike, one of them carrying a 40-character commit id twice — "lacking
   * structure and it's difficult to read".
   */
  it('puts the decision first, as a pill and a title with a way to the rest', async () => {
    const page = await understand(STRUCTURED)
    expect(page.text()).toContain('Already decided')

    const decision = page.find('.decision')
    expect(decision.text()).toContain('Option C of 8')
    expect(decision.text()).toContain(
      'An export context passed explicitly, and one filter chain for both',
    )
    expect(
      decision.findAll('a').map(one => one.attributes('href')),
      'and a way to the options it was chosen from',
    ).toContain(`/reviews/${STRUCTURED}/alternatives`)
  })

  it('draws what is still open as figures, not as a clause', async () => {
    const page = await understand(STRUCTURED)
    const counts = page.find('.open-counts')
    expect(counts.exists()).toBe(true)
    const figures = counts.findAll('.open-count').map(one => one.text())
    expect(figures).toEqual([
      '3 choices the ticket left open',
      '2 places the description and the code disagree',
    ])
    /* Each figure is the number, drawn as the thing to look at. */
    expect(counts.find('.open-count b').text()).toBe('3')
  })

  it('draws no figure for a count of nought', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = { ...detail.brief!, disagreements: [] }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    const figures = page.findAll('.open-count').map(one => one.text())
    expect(figures).toEqual(['3 choices the ticket left open'])
  })

  it('draws the sources as a table of kind, reference, state and what it tells us', async () => {
    const page = await understand(STRUCTURED)
    const table = page.findAll('table')
      .find(one => one.text().includes('ABC-402'))
    expect(table, 'the sources are a table').toBeDefined()

    expect(table!.findAll('thead th, thead .th').map(one => one.text()).slice(0, 4))
      .toEqual(['Kind', 'Reference', 'State', 'What it tells us'])

    const rows = table!.findAll('tbody tr')
    expect(rows).toHaveLength(4)
    expect(rows[0]!.text()).toContain('ticket')
    expect(rows[0]!.text()).toContain('ABC-402 · Scheduled exports carry their own context')
    expect(rows[0]!.text()).toContain('In Review')
    expect(rows[0]!.text()).toContain('The problem, four options, and three open questions')
    /* A link is an extra way to read one, never the only way. */
    expect(rows[0]!.find('a').attributes('href')).toContain('ABC-402')

    /* And a source that reported no state and no note shows its reference alone. */
    const bare = rows[2]!
    expect(bare.text()).toContain('Agreed with you on the ticket on 6 October')
    expect(bare.findAll('td').at(2)!.text()).toBe('')
    expect(bare.findAll('td').at(3)!.text()).toBe('')
  })

  it('leaves the open-choices table to Solution, and links to it', async () => {
    const page = await understand(STRUCTURED)
    /*
     * DEV-885's comment: "One list, drawn once." The Understand board's fold and the
     * Solution board's table are the same `open_choices`, so this page carries the line
     * and the link, and Solution carries the table.
     */
    expect(page.text()).toContain('3 choices the ticket left open')
    expect(page.text()).toContain('2 places the description and the code disagree')
    expect(page.findAll('a').map(one => one.attributes('href')))
      .toContain(`/reviews/${STRUCTURED}/solution`)
    /* And none of the table's own columns are on this page. */
    expect(page.text()).not.toContain('Against the ticket')
    expect(page.text()).not.toContain('Where the ticket left a choice')
  })

  /**
   * **No figure at the foot, which is DEV-892's defect 4.**
   *
   * The card carried "105 words as the session recorded it", counted off the brief in hand.
   * It came off the board, where the figure is half of a pair that argues this page is
   * shorter than the record it is drawn from — an argument about the design, not a fact a
   * reader of one review can act on, and on this example it ran the wrong way round.
   */
  it('draws no word count over the recorded text', async () => {
    const page = await understand(STRUCTURED)
    const foot = page.find('.measured').text()

    expect(foot).not.toMatch(/\d+ words/)
    expect(foot).not.toContain('text sketch')
  })

  it('keeps the recorded problem a click away, and interpolated', async () => {
    const page = await understand(STRUCTURED)
    expect(page.find('#as-recorded').exists()).toBe(false)

    await open(page, 'Read the problem as recorded')

    const recorded = page.find('#as-recorded')
    expect(recorded.exists()).toBe(true)
    expect(recorded.text()).toContain('Scheduled exports run with whatever tenant')
    /* `pre-wrap` at a readable measure, which is what keeps a record a record. */
    expect(recorded.classes()).toContain('rd-recorded')
  })

  /*
   * And the record is not drawn twice. The problem is at the foot and where the decision
   * came from is under "Already decided"; before DEV-892 the foot held both, and folding
   * the provenance in its own section without taking it out of the foot would have put the
   * same paragraph on the page in two places.
   */
  it('draws each piece of recorded text in exactly one place', async () => {
    const page = await understand(STRUCTURED)
    await open(page, 'Read the problem as recorded')
    await open(page, 'Where this came from, as recorded')

    const brief = fixture(STRUCTURED).brief!
    const text = page.text()
    expect(text.split(brief.problem).length - 1, 'the problem, once').toBe(1)
    expect(
      text.split(brief.provenance!).length - 1,
      'where it came from, once',
    ).toBe(1)
  })

  describe('a brief caught between the session\u2019s two writes', () => {
    /**
     * The problem half written and the change half not: a before flow and numbered
     * problems, no headline, no after flow, no `now_fixed`, no options, no parts. A
     * session writes the problem first and hands it to an independent options pass, so
     * this state lasts for minutes on every review.
     */
    async function halfWritten() {
      const detail = fixture(STRUCTURED)
      const brief = detail.brief!
      brief.headline = null
      brief.flow_after = []
      brief.flow_after_caption = null
      brief.problems = brief.problems.map(problem => ({ ...problem, now_fixed: null }))
      brief.options = []
      brief.parts = []
      await withReview(detail)
      return understand(STRUCTURED)
    }

    it('draws the after side as not written yet, not as missing', async () => {
      const page = await halfWritten()
      /* The picture is a before and an after; an absent right-hand side reads as a
         page that failed to load rather than a record that is twenty minutes old. */
      expect(page.findAll('.flow')).toHaveLength(1)
      expect(page.text()).toContain('After')
      expect(page.text()).toContain('Not written yet.')
      expect(page.text()).toContain('this side arrives second')
    })

    it('is not mistaken for a brief written the old way', async () => {
      const page = await halfWritten()
      expect(page.text()).not.toContain('Written before the structured brief.')
      expect(page.text()).toContain('The problem is written. What was done about it is not, yet.')
    })

    it('does not explain green marks that are not there', async () => {
      const page = await halfWritten()
      expect(page.text()).not.toContain('green on the right, with the same number')
      expect(page.text()).toContain('arrive with the after flow')
    })

    it('draws no legend at all when there is no picture', async () => {
      const detail = fixture(STRUCTURED)
      detail.brief!.flow_before = []
      detail.brief!.flow_after = []
      await withReview(detail)

      const page = await understand(STRUCTURED)
      expect(page.find('.flows').exists()).toBe(false)
      expect(page.text()).not.toContain('Each number is one of the problems below')
    })
  })

  describe('a brief written before any of this', () => {
    it('still shows, with one line saying what it is', async () => {
      const page = await understand(OLD_WAY)
      expect(page.text()).toContain('Written before the structured brief.')
      expect(page.text()).toContain('which is what a brief held before 10 October 2026')
      /* The recorded text, as the session wrote it. Never a blank page. */
      /*
       * A substring with no apostrophe in it. The fixture writes a straight one and this
       * file is formatted with curly quotes, which is a difference nothing should turn on.
       */
      expect(page.text()).toContain(
        'a webhook delivery fails once and is never tried again',
      )
      expect(page.text()).toContain('The problem, as recorded')
      expect(page.find('.rd-recorded').exists()).toBe(true)
    })

    it('hands its diagram to mermaid, which is the one place that happens', async () => {
      const page = await understand(OLD_WAY)
      expect(page.find('figure').exists()).toBe(true)
    })

    it('draws none of the structured page, because there is nothing to draw', async () => {
      const page = await understand(OLD_WAY)
      expect(page.find('.flow').exists()).toBe(false)
      expect(page.text()).not.toContain('were wrong')
      expect(page.text()).not.toContain('How it works today')
    })

    it('shows where it came from, when the brief recorded it', async () => {
      const page = await understand(OLD_WAY)
      expect(page.text()).toContain('Where this came from')
    })
  })

  it('says step 2 has not run when there is no brief at all', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = null
    await withReview(detail)

    const page = await understand(STRUCTURED)
    /* A different state from a brief written the old way, and a different sentence. */
    expect(page.text()).toContain('Nothing has been written about this problem yet.')
    expect(page.text()).not.toContain('Written before the structured brief.')
  })
})

/* ================================= Alternatives ================================= */

describe('Alternatives', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('draws a row per option and a column per numbered problem', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.findAll('.option')).toHaveLength(8)
    /* Option, Seen by, three problems, and the column the opener sits in. */
    expect(page.findAll('thead th')).toHaveLength(6)
  })

  /**
   * **A problem column is headed by its number and its label, never by its sentence.**
   *
   * DEV-892's defect 2. This heading used to be `was_wrong` — a line of prose over a
   * column of three verdicts, which on the real review wrapped ten lines deep. The test
   * above asserted the sentence was there, which is how the defect survived an invented
   * example: it was a measurement of the wrong thing.
   */
  it('heads a problem column with its number and its label', async () => {
    const page = await alternatives(STRUCTURED)
    const head = page.find('thead').text()

    expect(head).toContain('The shared slot')
    expect(head).toContain('The builder\'s branch')
    expect(head).toContain('Filters skipped')

    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(head, `problem ${problem.number}'s sentence is not a heading`)
        .not.toContain(problem.was_wrong)
    }
  })

  it('heads a problem column with the number alone when the brief wrote no label', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.problems = detail.brief!.problems.map(problem => ({
      ...problem,
      label: null,
    }))
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    const head = page.find('thead')

    /* Still three problem columns, each with its mark and nothing else. */
    expect(head.findAll('.th--marked')).toHaveLength(3)
    for (const problem of detail.brief!.problems) {
      expect(head.text(), 'the sentence is never promoted to a heading')
        .not.toContain(problem.was_wrong)
    }
    /* And the number is still there, which is all a reader has to go on. */
    expect(head.findAll('.th--marked')[0]!.text()).toContain('1')
  })

  /**
   * And the sentence is one click away **on this page**.
   *
   * A reader working down column 2 should not have to leave the matrix to find out what
   * column 2 is, and a brief with no labels needs this most — the headings are then bare
   * numbers.
   */
  it('says what every number is, in full, one click away', async () => {
    const page = await alternatives(STRUCTURED)

    const fold = page.find('details.numbers')
    expect(fold.exists()).toBe(true)
    expect(fold.find('summary').text()).toContain('What each number is')

    const said = fold.findAll('.says')
    expect(said).toHaveLength(3)
    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(fold.text(), `problem ${problem.number} is said in full`)
        .toContain(problem.was_wrong)
    }
  })

  /*
   * The narrow layout names each verdict's column above it, and that label is the heading
   * too — so the sentence is gone from there as well. It was the same defect in a card
   * rather than in a column, and a page tested only at one width would have kept it.
   */
  it('names a verdict’s column by number and label at a phone’s width', async () => {
    const page = await alternatives(STRUCTURED)
    const labels = page.findAll('.option .cell[data-label]')
      .map(cell => cell.attributes('data-label'))

    expect(labels).toContain('1 · The shared slot')
    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(labels.join(' | ')).not.toContain(problem.was_wrong)
    }
  })

  it('tells the three real verdicts apart, by word as well as by colour', async () => {
    const page = await alternatives(STRUCTURED)
    const sheet = page.find('table').text()
    expect(sheet).toContain('Fixed')
    expect(sheet).toContain('Partly')
    expect(sheet).toContain('Stays')
    /* Each has a shape too, which is what `SeverityTag` does for a severity. */
    expect(page.find('.word--fixed .shape').exists()).toBe(true)
    expect(page.find('.word--partly .shape').exists()).toBe(true)
    expect(page.find('.word--stays .shape').exists()).toBe(true)
  })

  it('draws not_assessed as the verdict it is, not as a blank', async () => {
    const page = await alternatives(STRUCTURED)
    /*
     * Option D is on the ticket and nothing scored it against the three problems.
     * DEV-885's comment: "`not_assessed` is a stored verdict, not a missing one."
     */
    const row = page.findAll('.option').find(one =>
      one.text().includes('A tenant-scoped database connection per export'))!
    expect(row.findAll('.word--not_assessed')).toHaveLength(3)
    expect(row.text()).toContain('Not assessed')
  })

  it('draws a verdict the brief never recorded differently again', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.options = detail.brief!.options.map((option, at) =>
      at === 0 ? { ...option, verdicts: [] } : option)
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    const row = page.findAll('.option').find(one =>
      one.text().includes('Clear the slot between jobs'))!
    /* "Nobody judged this" and "the brief recorded nothing" are different sentences. */
    expect(row.text()).toContain('Not recorded')
    expect(row.text()).not.toContain('Not assessed')
    expect(row.findAll('.word--absent')).toHaveLength(3)
  })

  it('says who put each option forward', async () => {
    const page = await alternatives(STRUCTURED)
    const both = page.findAll('.option').find(one =>
      one.text().includes('Clear the slot between jobs'))!
    /* `both` draws both pills: two sides arrived at it independently. */
    expect(both.find('.seen').text()).toContain('Ticket A')
    expect(both.find('.seen').text()).toContain('Blind')

    const ticketOnly = page.findAll('.option').find(one =>
      one.text().includes('A tenant-scoped database connection per export'))!
    expect(ticketOnly.find('.seen').text()).toContain('Ticket D')
    expect(ticketOnly.find('.seen').text()).not.toContain('Blind')

    const blindOnly = page.findAll('.option').find(one =>
      one.text().includes('Make the scheduled path call the web path'))!
    expect(blindOnly.find('.seen').text()).toContain('Blind')
    expect(blindOnly.find('.seen').text()).not.toContain('Ticket')
  })

  it('marks the option that was built, and the ones the ticket never had', async () => {
    const page = await alternatives(STRUCTURED)
    const built = page.find('.option--built')
    expect(built.text()).toContain('Built')
    expect(built.text()).toContain('An export context passed explicitly')

    const blind = page.findAll('.option--blind')
    /* Four of the eight came only from the blind pass, which is the point of running one. */
    expect(blind).toHaveLength(4)
    expect(blind[0]!.text()).toContain('Not on the ticket')
  })

  it('opens an option’s reasoning in place', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.find('#option-C').exists()).toBe(false)

    const row = await open(page, 'An export context passed explicitly')

    expect(row.attributes('aria-expanded')).toBe('true')
    const detail = page.find('#option-C')
    expect(detail.exists()).toBe(true)
    expect(detail.text()).toContain('For')
    expect(detail.text()).toContain('Fixes all three')
    expect(detail.text()).toContain('Against')
    expect(detail.text()).toContain('Two entry points to keep in step')
  })

  it('puts what was built beside what a fresh pair of eyes would pick', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.text()).toContain('Built against what a fresh pair of eyes would pick')
    expect(page.text()).toContain('The blind pass would pick')
    expect(page.text()).toContain('C · An export context passed explicitly')
    expect(page.text()).toContain('It puts the split where the paths genuinely differ')
    /* They agree here, and the page says so rather than leaving it to be worked out. */
    expect(page.find('.card--blind').text()).toContain('Same as what was built')
  })

  it('says when the blind pass would have picked something else', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.blind_pass = { ...detail.brief!.blind_pass!, pick_key: 'F' }
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    expect(page.find('.card--blind').text()).toContain('Not what was built')
    expect(page.find('.card--blind').text()).toContain('Extract reporting into a service')
  })

  it('counts the questions rather than asserting there are four', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.blind_pass = {
      ...detail.brief!.blind_pass!,
      questions: ['Only one thing was unclear.'],
    }
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    /* The heading said "Four things" over however many rows the record held. */
    expect(page.text()).toContain('1 thing the problem statement did not say')
    expect(page.findAll('.question')).toHaveLength(1)
  })

  it('lists what the blind pass needed to know', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.text()).toContain('What the blind pass needed to know')
    expect(page.findAll('.question')).toHaveLength(4)
    expect(page.text()).toContain(
      'Does any report definition deliberately expose a column the web column policy hides',
    )
  })

  it('shows where the options came from, and what the pass looked up', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.text()).toContain('Where these came from')
    /* Eight options: four on the ticket, seven from the pass, three on both. */
    expect(page.text()).toContain('4 options')
    expect(page.text()).toContain('7 options')
    /*
     * **What it looked up is behind the same control as what it was given.** DEV-892: this
     * printed open, as a line beginning "It looked up:", and on a real review that line was
     * a 91-word paragraph in a card beside another card.
     */
    const folds = page.findAll('details.given')
    expect(folds).toHaveLength(2)
    expect(page.text()).not.toContain('It looked up:')
    expect(folds[1]!.find('summary').text()).toContain('What it looked up')
    expect(folds[1]!.find('.given-text').text()).toContain('nothing')

    /* And what it was given, verbatim, on the page rather than linked to. */
    expect(folds[0]!.find('.given-text').text()).toContain(
      'A multi-tenant PHP web application serves reports.',
    )
  })

  describe('the three states of a blind pass', () => {
    async function withState(
      state: 'not_run' | 'ran_and_found_nothing' | 'ran',
      pass: boolean,
    ) {
      const detail = fixture(STRUCTURED)
      detail.brief!.blind_pass_state = state
      if (!pass) detail.brief!.blind_pass = null
      detail.brief!.options = detail.brief!.options.map(option => ({
        ...option,
        proposed_by: state === 'ran' ? option.proposed_by : 'ticket',
      }))
      await withReview(detail)
      return alternatives(STRUCTURED)
    }

    it('says a pass never ran, in its own words', async () => {
      const page = await withState('not_run', false)
      expect(page.text()).toContain('No blind pass was run')
      expect(page.text()).toContain(
        'Nothing has looked at this problem without also seeing the answer',
      )
    })

    it('says a pass ran and found nothing new, in different words', async () => {
      const page = await withState('ran_and_found_nothing', true)
      expect(page.text()).toContain(
        'A blind pass ran and found nothing the ticket had missed',
      )
      expect(page.text()).toContain(
        'which is a result about the option list, not an absence of one',
      )
      /* And it does not read like the one above. */
      expect(page.text()).not.toContain('No blind pass was run')
    })

    it('neither of those two reads like the other', async () => {
      const never = (await withState('not_run', false)).text()
      const nothing = (await withState('ran_and_found_nothing', true)).text()
      expect(never).toContain('No blind pass was run')
      expect(nothing).not.toContain('No blind pass was run')
      expect(nothing).toContain('found nothing the ticket had missed')
      expect(never).not.toContain('found nothing the ticket had missed')
    })

    it('says a pass ran when one did', async () => {
      const page = await withState('ran', true)
      expect(page.text()).toContain('A blind pass ran')
      expect(page.text()).toContain('What it put forward is marked below.')
    })
  })

  /*
   * **What the self-review found, pinned.** Three of these were states the page claimed
   * something false about, and all three came from the same habit: a brief recorded before
   * a field existed carries `undefined`, and `undefined` is not the field's default.
   */
  describe('a brief recorded before these fields existed', () => {
    /** The shape of every brief written before 10 October 2026: options, nothing else. */
    async function oldWayWithOptions() {
      const detail = fixture(STRUCTURED)
      const brief = detail.brief!
      brief.options = brief.options.map(option => ({
        ...option,
        proposed_by: undefined as unknown as 'ticket',
        verdicts: undefined as unknown as [],
      }))
      brief.blind_pass_state = undefined as unknown as 'not_run'
      brief.blind_pass = null
      brief.problems = []
      await withReview(detail)
      return alternatives(STRUCTURED)
    }

    it('does not invent a blind pass that never ran', async () => {
      const page = await oldWayWithOptions()
      /*
       * The switch fell through to "a blind pass ran" for an absent state, which is the
       * strongest of the three things it could say and the one nothing had reported.
       */
      expect(page.text()).toContain('No blind pass was run')
      expect(page.text()).not.toContain('A session reported handing this problem')
    })

    it('does not badge every option as raised by both sides', async () => {
      const page = await oldWayWithOptions()
      const seen = page.findAll('.seen').map(one => one.text())
      /* `!== 'blind_pass'` and `!== 'ticket'` are both true of `undefined`. */
      for (const cell of seen) {
        expect(cell).not.toContain('Blind')
        expect(cell).toContain('not recorded')
      }
      /* And the counts do not claim options for a side that raised none. */
      expect(page.text()).not.toContain('0 options')
    })

    it('leaves the matrix a valid grid when no problem is numbered', async () => {
      const page = await oldWayWithOptions()
      const template = page.find('table').attributes('style')
      /*
       * `repeat(0, …)` is invalid, and a browser drops the whole declaration — taking the
       * board's column layout off the header and every row with it.
       */
      expect(template).not.toContain('repeat(0')
      expect(template).toContain('2.4fr 150px 110px')
    })
  })

  /**
   * **No options, and the page says which nothing it is — from step 3's own state.**
   *
   * DEV-892's defect 6. This said "No alternatives are recorded for this change… Nothing has
   * written any" whatever the trail said, and on the review it was found on the trail said
   * the blind pass was running. A page contradicting the record it is drawn from is worse
   * than a page with nothing on it.
   */
  it.each([
    ['running', 'Being worked on now.', 'A session is writing them now.'],
    ['pending', 'Not written yet.', 'Nothing has written any.'],
    ['done', 'None was recorded.', 'The step finished without recording any.'],
  ] as const)('says step 3 is %s in its own words', async (state, headline, why) => {
    const detail = fixture(STRUCTURED)
    detail.brief!.options = []
    detail.layers = detail.layers.map(layer =>
      layer.name === 'alternatives' ? { ...layer, state } : layer)
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    expect(page.find('h1').text()).toBe(headline)
    expect(page.text()).toContain(why)
    /* Never the sentence the defect was: it asserted an absence the record contradicted. */
    expect(page.text()).not.toContain('No alternatives are recorded')
  })
})

/* =================================== Solution =================================== */

describe('Solution', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('opens with the verdict on the approach', async () => {
    const page = await solution(STRUCTURED)
    expect(page.find('h1').text()).toContain('C is the right shape.')
    /* And the size of the change, counted off the parts rather than asserted. */
    expect(page.text()).toContain('26 files')
    expect(page.text()).toContain('959 lines added and 423 removed')
    expect(page.text()).toContain('in 6 parts')
  })

  it('draws a card per part, with its files and its lines', async () => {
    const page = await solution(STRUCTURED)
    const parts = page.findAll('.part')
    expect(parts).toHaveLength(6)
    expect(parts[0]!.text()).toContain('The export context')
    expect(parts[0]!.text()).toContain('Replaces the request-scoped slot')
    expect(parts[0]!.text()).toContain('3 files')
    expect(parts[0]!.text()).toContain('+134')
    expect(parts[0]!.text()).toContain('−70')
  })

  it('marks which problems a part fixes, in green', async () => {
    const page = await solution(STRUCTURED)
    const parts = page.findAll('.part')
    const marks = parts[1]!.findAll('.mark--fixed').map(one => one.text())
    /* The second part fixes problems 1 and 2. */
    expect(marks).toHaveLength(2)
    expect(marks[0]).toContain('1')
    expect(marks[1]).toContain('2')
    /* The words say what the mark means here, which is not "what fixes it". */
    expect(parts[1]!.find('.mark--fixed').text()).toContain('this part fixes problem 1')
    /* A part that fixes none carries no mark rather than a nought. */
    const tests = parts.find(one => one.text().includes('Tests and generated types'))!
    expect(tests.findAll('.mark--fixed')).toHaveLength(0)
  })

  it('says on the card what the review found in that part', async () => {
    const page = await solution(STRUCTURED)
    const parts = page.findAll('.part')
    const policy = parts.find(one => one.text().includes('Allowed columns'))!
    expect(policy.text()).toContain('1 Blocker')
    /* A part the review found nothing in says so, rather than drawing an empty space. */
    const tests = parts.find(one => one.text().includes('Tests and generated types'))!
    expect(tests.text()).toContain('Nothing found here')
  })

  it('counts a finding that is not going to the author apart from the rest', async () => {
    const page = await solution(STRUCTURED)
    /* F2 is held back and F8 suppressed; neither is something the author will read. */
    const parts = page.findAll('.part')
    const context = parts.find(one => one.text().includes('The export context'))!
    expect(context.text()).toContain('1 not posting')
  })

  /**
   * **A part's tally counts what will post, not what carries the disposition.**
   *
   * DEV-892's comment of 11 October: this filtered `disposition === 'in_draft'` into a list
   * it called `posted`, under a docstring about what the author will read. A finding
   * carried into this round as `fixed` keeps whatever disposition it was carried with, so
   * it sits at `in_draft` and is not composed into the draft — which is the mistake PR #32
   * fixed on the code review page's headline and left standing here.
   */
  it('leaves a finding this round closed out of the part’s severities', async () => {
    const detail = fixture(STRUCTURED)
    const part = detail.brief!.parts[0]!
    const paths = new Set(part.files.map(file => file.path))
    const on = detail.findings.filter(
      finding => finding.file != null && paths.has(finding.file),
    )
    const posting = on.filter(
      finding => finding.disposition === 'in_draft'
        && (finding.round_status == null || finding.round_status === 'new'
          || finding.round_status === 'still_open'),
    )
    expect(posting.length, 'the part has something to post before we close one').toBeGreaterThan(0)

    /* What the card says before anything is closed, read off the card itself. */
    const was = await solution(STRUCTURED)
    const firstCard = was.findAll('.part')[0]!
    const severitiesBefore = firstCard.findAll('.part-tally .pill').map(pill => pill.text())
    const wasNotPosting = Number(/(\d+) not posting/.exec(firstCard.text())?.[1] ?? 0)
    expect(severitiesBefore.length, 'the card announces a severity to lose').toBeGreaterThan(0)

    /*
     * One of them carried from an earlier round and fixed there. Its disposition is
     * untouched — that is the whole point: the record still says `in_draft`.
     */
    const closed = posting[0]!
    detail.findings = detail.findings.map(finding =>
      finding.seq === closed.seq
        ? { ...finding, round_status: 'fixed' as const, continues_review_id: 40 }
        : finding)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    const card = page.findAll('.part')[0]!

    /*
     * One more finding is not posting than before, and the severity pills are the
     * difference: the card counts what the author receives, and the disposition on the
     * record did not move.
     */
    expect(card.text()).toContain(`${wasNotPosting + 1} not posting`)
    expect(
      card.findAll('.part-tally .pill').map(pill => pill.text()),
      'the closed finding is out of the severities the card announces',
    ).not.toEqual(severitiesBefore)

    /* And the row in the fold says why, rather than leaving the arithmetic unexplained. */
    await open(page, part.title)
    const row = page.find('#part-0').findAll('.finding')
      .find(one => one.text().includes(closed.key))!
    expect(row.classes()).toContain('finding--aside')
    expect(row.text()).toContain('Fixed')
  })

  /**
   * **One layout for every part card's head**, which is the first half of DEV-892's defect
   * 5: a title that did not fit beside its marks went under them, so some cards read across
   * and others read down.
   *
   * The arrangement is what is asserted here — a lane of marks, then the title, in that
   * order, in every card that has marks, and the title alone in one that has none. Whether
   * it *stays* beside them is layout, and `e2e/review-desk.spec.ts` measures that in a real
   * browser along with the heights of a row.
   */
  it('arranges every part card’s head the same way', async () => {
    const page = await solution(STRUCTURED)
    const cards = page.findAll('.part')
    expect(cards.length).toBeGreaterThan(1)

    const parts = fixture(STRUCTURED).brief!.parts
    let withMarks = 0
    let withNone = 0

    for (const [at, card] of cards.entries()) {
      const head = card.find('.part-head')
      const title = head.find('.part-title')
      expect(title.exists(), `part ${at + 1} has its title in the head`).toBe(true)
      expect(title.text()).toBe(parts[at]!.title)

      const marks = head.find('.part-marks')
      if (parts[at]!.fixes.length === 0) {
        /* No mark rather than a nought, and no empty lane either. */
        expect(marks.exists(), `part ${at + 1} fixes nothing and draws no lane`).toBe(false)
        withNone += 1
        continue
      }
      withMarks += 1
      expect(marks.findAll('.mark')).toHaveLength(parts[at]!.fixes.length)
      /* The lane comes first, every time: the title is never above or inside it. */
      expect(head.element.children[0]).toBe(marks.element)
      expect(head.element.children[1]).toBe(title.element)
    }

    /* Both cases are in the example, so this is a test of two shapes rather than one. */
    expect(withMarks, 'a part that fixes something').toBeGreaterThan(0)
    expect(withNone, 'a part that fixes nothing').toBeGreaterThan(0)
  })

  it('opens a part to its files, with what the change did to each', async () => {
    const page = await solution(STRUCTURED)
    expect(page.find('#part-0').exists()).toBe(false)

    await open(page, 'The export context')

    const detail = page.find('#part-0')
    expect(detail.exists()).toBe(true)
    expect(detail.text()).toContain('app/Reporting/ExportContext.php')
    expect(detail.text()).toContain('+112')
    expect(detail.text()).toContain('−61')
    expect(detail.findAll('.file')).toHaveLength(3)
  })

  it('says of a file outside the diff that the pull request did not change it', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'One filter chain for both paths')

    const detail = page.find('#part-3')
    const outside = detail.findAll('.file').find(one =>
      one.text().includes('app/Audit/RecordsAnExport.php'))!
    /*
     * `in_diff: false` is how a part claims a file the pull request never touched, and such
     * a file has no lines either way — so the fact replaces the figures rather than being
     * drawn as a pair of noughts.
     */
    expect(outside.text()).toContain('not changed by this pull request')
    expect(outside.text()).not.toContain('+0')
  })

  it('lists the findings on a part, joined to it by file', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'Allowed columns on the report definition')

    const detail = page.find('#part-2')
    expect(detail.text()).toContain('What the code review found here')
    /* F6 and F9 are both in `ColumnPolicy.php`, which this part names. */
    expect(detail.text()).toContain('F6')
    expect(detail.text()).toContain('F9')
    expect(detail.text()).toContain('Blocker')
    /* And nothing from another part's files. */
    expect(detail.text()).not.toContain('F1')
  })

  it('picks a finding up on a part the brief wrote before it existed', async () => {
    const detail = fixture(STRUCTURED)
    detail.findings = [
      ...detail.findings,
      {
        review_id: STRUCTURED,
        seq: 14,
        key: 'F14',
        lens: 'boundary',
        severity: 'should',
        file: 'app/Reporting/ExportContext.php',
        line: 91,
        claim: 'Written after the brief, and on a file the first part names.',
        disposition: 'in_draft',
        created_at: '2026-10-10T08:00:00Z',
      },
    ]
    await withReview(detail)

    const page = await solution(STRUCTURED)
    await open(page, 'The export context')
    /*
     * The join is a join, not a stored list — so a finding written after the brief appears
     * on its part without the brief being rewritten.
     */
    expect(page.find('#part-0').text()).toContain('F14')
  })

  it('shows a finding no part accounts for, rather than losing it', async () => {
    const detail = fixture(STRUCTURED)
    detail.findings = [
      ...detail.findings,
      {
        review_id: STRUCTURED,
        seq: 15,
        key: 'F15',
        lens: 'boundary',
        severity: 'nit',
        file: 'app/Reporting/Formatters/CsvWriter.php',
        line: 12,
        claim: 'In a file none of the parts names.',
        disposition: 'in_draft',
        created_at: '2026-10-10T08:05:00Z',
      },
    ]
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.text()).toContain('2 findings in no part of this description')
    expect(page.text()).toContain('F15')

    /*
     * And with the whole set accounted for, the section is not there at all.
     *
     * **Built by taking the two away rather than by resetting**, because the example's own
     * F14 is in no part for good: it names no file, being about the pull request's
     * description. `resetDesk()` would put it back and this would assert the opposite of
     * what it means to.
     */
    const accounted = fixture(STRUCTURED)
    accounted.findings = accounted.findings.filter(finding => finding.file != null)
    await withReview(accounted)
    const whole = await solution(STRUCTURED)
    expect(whole.text()).not.toContain('in no part of this description')
  })

  /**
   * **The list of findings in no part speaks the same vocabulary as a part's own.**
   *
   * Found reviewing DEV-892's own diff: the per-part rows and the card's count moved to
   * `willPost`, and this list was left dimming by the disposition. A finding carried from an
   * earlier round as `fixed` and still at `in_draft` was then drawn here full-strength — as
   * something the author will read — three inches under a card counting it as not posting.
   */
  it('dims a finding in no part by the same rule a part’s own rows use', async () => {
    const detail = fixture(STRUCTURED)
    const loose = detail.findings.find(
      finding => finding.file == null && finding.disposition === 'in_draft',
    )!
    expect(loose, 'the example has a finding in no part that would post').toBeTruthy()

    detail.findings = detail.findings.map(finding =>
      finding.seq === loose.seq
        ? { ...finding, round_status: 'fixed' as const, continues_review_id: 40 }
        : finding)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    const row = page.findAll('.findings--loose .finding')
      .find(one => one.text().includes(loose.key))!
    expect(row, 'the finding is still listed').toBeTruthy()

    /* The disposition did not move; what the author receives did. */
    expect(row.classes()).toContain('finding--aside')
    expect(row.text()).toContain('Fixed')
  })

  it('draws the open-choices table, which Understand leaves to it', async () => {
    const page = await solution(STRUCTURED)
    expect(page.text()).toContain('Where the ticket left a choice')
    const table = page.findAll('table').find(one =>
      one.text().includes('Where the export context is constructed'))!
    expect(table.findAll('tbody tr')).toHaveLength(3)
    expect(table.text()).toContain('In the scheduling command, before the job is dispatched')
    expect(table.text()).toContain('A context built inside the job cannot be logged')
  })

  it('tells a departure from the ticket from a gap the ticket left', async () => {
    const page = await solution(STRUCTURED)
    const table = page.findAll('table').find(one =>
      one.text().includes('Where the export context is constructed'))!
    const rows = table.findAll('tbody tr')
    /* The first departs; the second fills a gap. Two different things. */
    expect(rows[0]!.text()).toContain('Departs from the ticket')
    expect(rows[1]!.text()).toContain('Left open by the ticket')
    /* A choice that names a finding says which, and one that names none says nothing. */
    expect(rows[0]!.text()).toContain('F3 raises it')
    expect(rows[1]!.text()).not.toContain('raises it')
  })

  it('draws where the description and the code disagree, each naming its finding', async () => {
    const page = await solution(STRUCTURED)
    expect(page.text()).toContain('Where the description and the code disagree')
    const table = page.findAll('table').find(one =>
      one.text().includes('The column policy applies to scheduled exports'))!
    expect(table.findAll('tbody tr')).toHaveLength(2)
    expect(table.text()).toContain('F9')
    expect(table.text()).toContain('F13')
    expect(table.text()).toContain('the backfill leaves older definitions naming none')
  })

  /**
   * **No parts, said once, and from step 4's own state.** The other half of defect 6: the
   * headline and the notice under it carried the same sentence three inches apart, and both
   * of them asserted an absence whatever the trail said about step 4.
   *
   * The example's brief carries an `approach_verdict`, so the heading here is the verdict
   * and the state leads the notice — which is the one arrangement in which it is said in
   * the notice at all.
   */
  it.each([
    ['running', 'Being worked on now.', 'A session is writing them now.'],
    ['pending', 'Not written yet.', 'A session writes the problem first'],
    ['done', 'None was recorded.', 'The step finished without describing the change'],
  ] as const)('says step 4 is %s in its own words, once', async (state, said, why) => {
    const detail = fixture(STRUCTURED)
    detail.brief!.parts = []
    detail.layers = detail.layers.map(layer =>
      layer.name === 'solution_map' ? { ...layer, state } : layer)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    /* The heading is the verdict; the notice says the state, and says it once. */
    expect(page.find('h1').text()).toBe(detail.brief!.approach_verdict)
    expect(page.text().split(said).length - 1, `${said} is on the page once`).toBe(1)
    expect(page.text()).toContain(why)
    expect(page.text()).not.toContain('The change has not been described in parts yet.')
  })

  it('says the state as its heading when there is no verdict to head the page', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.parts = []
    detail.brief!.approach_verdict = null
    detail.layers = detail.layers.map(layer =>
      layer.name === 'solution_map' ? { ...layer, state: 'running' as const } : layer)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.find('h1').text()).toBe('Being worked on now.')
    /*
     * And not again in the notice underneath. "One sentence, once" is the issue's own
     * wording about exactly this pair.
     */
    expect(page.text().split('Being worked on now.').length - 1).toBe(1)
  })

  it('still draws the choices and the clashes when there are no parts', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.parts = []
    await withReview(detail)

    const page = await solution(STRUCTURED)
    /*
     * **Understand gives this page its open-choices table and says so in a line and a
     * link.** An empty-state page whenever `parts` was empty therefore left a brief with
     * choices recorded showing them nowhere at all, under a promise on the page before it.
     * The three sections are three independent lists.
     */
    expect(page.text()).toContain('Where the ticket left a choice')
    expect(page.text()).toContain('In the scheduling command, before the job is dispatched')
    expect(page.text()).toContain('Where the description and the code disagree')
    expect(page.text()).toContain('F9')
  })

  it('says there is no brief at all differently again', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = null
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.text()).toContain('Nothing has been written about this change yet.')
    expect(page.text()).not.toContain('Where the ticket left a choice')
  })
})

/* ============================ Every field interpolated ========================== */

/*
 * **The rule, as a test rather than as a convention.** DEV-885: "Every field is
 * interpolated. The only `v-html` in these components is `MermaidFigure`'s, and a test says
 * so."
 *
 * Read off the files rather than off a rendered page, because the failure this catches is
 * one somebody writes six months from now in a component nothing here mounts.
 */
describe('nothing on these pages is handed to the browser as markup', () => {
  const COMPONENTS = resolve('app/components')
  const PAGES = resolve('app/pages/reviews/[id]')

  /** Every component and every page of a review, with its text. */
  const sources = [
    ...readdirSync(COMPONENTS)
      .filter(name => name.endsWith('.vue'))
      .map(name => ({ name: `components/${name}`, text: readFileSync(resolve(COMPONENTS, name), 'utf8') })),
    ...readdirSync(PAGES)
      .filter(name => name.endsWith('.vue'))
      .map(name => ({ name: `pages/reviews/[id]/${name}`, text: readFileSync(resolve(PAGES, name), 'utf8') })),
  ]

  it('finds the files it is about to check', () => {
    expect(sources.length).toBeGreaterThan(30)
    expect(sources.map(file => file.name)).toContain('pages/reviews/[id]/understand.vue')
    expect(sources.map(file => file.name)).toContain('pages/reviews/[id]/alternatives.vue')
    expect(sources.map(file => file.name)).toContain('pages/reviews/[id]/solution.vue')
  })

  it('uses v-html in two files, and in both for something already sanitised', () => {
    /* The directive, not the word: several of these discuss `v-html` in a comment. */
    const uses = sources
      .filter(file => file.text.split('\n').some(line => line.includes('v-html=')))
      .map(file => file.name)
    /*
     * `MermaidFigure` for mermaid's own SVG, and `MarkdownView` for the draft body it
     * renders through markdown-it and DOMPurify. Neither is on these three pages except
     * `MermaidFigure`, and only for a brief old enough to carry a `diagram`.
     */
    expect(uses.sort()).toEqual(['components/MarkdownView.vue', 'components/MermaidFigure.vue'])
  })

  it('uses none of it on the three step pages', () => {
    const pages = sources.filter(file => file.name.startsWith('pages/'))
    for (const page of pages) {
      expect(page.text, `${page.name} uses v-html`).not.toContain('v-html=')
    }
  })

  it('draws the flows as boxes rather than as a diagram', () => {
    const flow = sources.find(file => file.name === 'components/FlowDiagram.vue')!
    /*
     * The import and the use, not the word: this component's own doc argues at length about
     * why a flow is not mermaid, and a test that forbade the word would forbid the argument.
     */
    expect(flow.text).not.toContain('v-html')
    expect(flow.text).not.toMatch(/import\(['"]mermaid['"]\)/)
    expect(flow.text).not.toContain('<MermaidFigure')
    /*
     * Nothing loads mermaid but the component that is mermaid. It is a dynamic `import()`
     * rather than a static one, deliberately — mermaid is not in the app shell's graph.
     */
    const loads = sources
      .filter(file => /import\(['"]mermaid['"]\)/.test(file.text))
      .map(file => file.name)
    expect(loads).toEqual(['components/MermaidFigure.vue'])
  })

  it('says out loud that its tables are tables', () => {
    /*
     * `display: grid` on a table row strips the table, row and cell roles in every browser,
     * and with them the `<th>`-to-cell association. The component claimed a screen reader
     * got the column name either way; it did not, until these roles were written.
     */
    const table = sources.find(file => file.name === 'components/DeskTable.vue')!
    for (const role of ['role="table"', 'role="rowgroup"', 'role="row"', 'role="columnheader"']) {
      expect(table.text, `DeskTable is missing ${role}`).toContain(role)
    }
    const pages = sources.filter(file => file.name.startsWith('pages/reviews'))
    for (const page of pages.filter(one => one.text.includes('<DeskTable'))) {
      expect(page.text, `${page.name} has table rows with no role`).toContain('role="row"')
      expect(page.text, `${page.name} has cells with no role`).toContain('role="cell"')
    }
  })

  it('never lets a pill break across two lines', () => {
    /*
     * The one defect Patrick flagged on the board, pinned where it is fixed. Three
     * declarations hold it, and a pill that lost any of them would wrap again.
     */
    const pill = sources.find(file => file.name === 'components/DeskPill.vue')!
    expect(pill.text).toContain('white-space: nowrap')
    expect(pill.text).toContain('flex: none')
    expect(pill.text).toContain('min-width: max-content')
  })
})
