// @vitest-environment nuxt
import { readFileSync, readdirSync } from 'node:fs'
import { relative, resolve, sep } from 'node:path'
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
 * fixtures with fifteen findings on the file paths its parts name. It is the review the
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
    throw new Error(`step ${number} has nothing behind it — no facts, no read time — so \
it draws no card`)
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

  /* ---------------- The two links out of a review (DEV-936) ---------------- */

  /**
   * The header's links, which are the only `.header-out` anchors on the page.
   *
   * The class was `.out` until a review pointed out that `QueueRow` and `SignOffCard` each
   * had one too — so this comment's claim held only because `SignOffCard` is drawn by no
   * page yet, and the draft page is a `ReviewStepPage`.
   */
  const wayOut = (page: VueWrapper) => page.findAll('.where a.header-out')

  it('links the repository and the number to the pull request', async () => {
    const page = await understand(STRUCTURED)
    const [pull] = wayOut(page)

    expect(pull, 'the header carries a link out').toBeDefined()
    expect(pull!.text()).toContain('acme/portal')
    expect(pull!.text()).toContain('#514')
    /*
     * **Built from `repo` and `pr`**, which is the assertion the acceptance criterion is
     * about: there is no stored field this could have come from, and nothing a session
     * wrote reaches it.
     */
    expect(pull!.attributes('href')).toBe('https://github.com/acme/portal/pull/514')
  })

  /**
   * **All five pages that have one, and the index that redirects to the first.**
   *
   * DEV-936's first criterion names six pages, and `ReviewStepPage` is the shell all of
   * them sit in — so one assertion on one page would be an assertion about the component
   * and a claim about the other five. Alternatives and Solution are mounted here; the code
   * review page and the draft page have their own specs, and `/reviews/{id}` is a redirect
   * to Understand rather than a page, which `screens.nuxt.spec.ts` holds.
   */
  it('links the pull request on every step page, not only the first', async () => {
    for (const [name, open] of [
      ['Understand', understand],
      ['Alternatives', alternatives],
      ['Solution', solution],
    ] as const) {
      const page = await open(STRUCTURED)
      const [pull] = wayOut(page)
      expect(pull, `${name} carries the link`).toBeDefined()
      expect(pull!.attributes('href'), name).toBe('https://github.com/acme/portal/pull/514')
    }
  })

  it('draws the repository as text when it is not a repository name', async () => {
    /*
     * **`prUrl` refuses rather than building a link to somewhere else.** Two populations of
     * rows reach it that the store's check does not: everything released Review Desk wrote,
     * which never checked the characters, and — until this branch's own review — a `..`
     * segment. `https://github.com/acme/portal#x/pull/412` is served as the repository page
     * and `https://github.com/../../pull/1` resolves to `github.com/pull/1`, so in both the
     * link's text and its destination name two different things.
     *
     * The same second refusal `ticketUrl` makes, for the reason it states: a column is not
     * a promise about every row in it.
     */
    for (const repo of ['acme/portal#x', '../..', 'acme/portal/extra', 'acme/..']) {
      const detail = fixture(STRUCTURED)
      detail.review = { ...detail.review, repo: repo as typeof detail.review.repo }
      await withReview(detail)

      const page = await understand(STRUCTURED)
      const where = page.find('.where')
      expect(where.text(), `${repo} is still named`).toContain(repo)
      expect(
        where.findAll('a').map(one => one.attributes('href') ?? ''),
        `${repo} is not linked`,
      ).not.toContain(`https://github.com/${repo}/pull/514`)
      expect(
        where.findAll('a.header-out').some(one =>
          (one.attributes('href') ?? '').startsWith('https://github.com/')),
        `${repo} draws no GitHub link at all`,
      ).toBe(false)
    }
  })

  it('opens both links in a new tab, and says so where only a reader would not see it', async () => {
    const page = await understand(OLD_WAY)
    const links = wayOut(page)
    expect(links.length, 'the pull request and the ticket').toBe(2)

    for (const link of links) {
      expect(link.attributes('target')).toBe('_blank')
      /*
       * `noopener` and `noreferrer` both, and `noopener` is the one that matters: a page
       * opened without it can reach back through `window.opener`.
       */
      expect(link.attributes('rel')).toBe('noopener noreferrer')
      expect(link.find('.rd-visually-hidden').text()).toContain('opens in a new tab')
    }

    /* And each says where it goes, which "opens in a new tab" alone does not. */
    expect(links[0]!.text()).toContain('on GitHub')
    expect(links[1]!.text()).toContain('on the tracker')
  })

  it('draws the ticket’s id as a link when the record holds its address', async () => {
    const page = await understand(OLD_WAY)
    const ticket = wayOut(page)[1]

    expect(ticket, 'review 17 holds an address').toBeDefined()
    expect(ticket!.text()).toContain('ABC-231')
    expect(ticket!.attributes('href')).toBe('https://linear.app/acme/issue/ABC-231')

    /*
     * **The id is the link's text and the address is not drawn at all.** A link whose text
     * is a URL is a link a reader has to parse, and the id is the thing they came for.
     */
    expect(page.find('.where').text()).not.toContain('linear.app')
  })

  it('draws the ticket’s id as text when the record holds no address', async () => {
    /* Review 41's fixture carries `ABC-402` and no address, as the contract example does. */
    const page = await understand(STRUCTURED)

    expect(page.find('.where').text()).toContain('ABC-402')
    expect(wayOut(page).length, 'the pull request alone is a link').toBe(1)
    expect(
      wayOut(page)[0]!.text(),
      'and the ticket is not inside it',
    ).not.toContain('ABC-402')
  })

  it('says nothing about a ticket for a review that has none', async () => {
    const detail = fixture(STRUCTURED)
    detail.review = { ...detail.review, ticket_id: null, ticket_url: null }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    const where = page.find('.where').text()
    expect(where).toContain('acme/portal')
    expect(where).not.toContain('ABC-402')
    /* No stranded separator where the ticket would have been. */
    expect(where).not.toMatch(/·\s*·/)
  })

  it('refuses an address that is not https, at render as well as at the write', async () => {
    /*
     * **The store refuses this on the way in, and this is the second refusal.** A column is
     * not a promise about every row in it: a database can be written to by hand, and the one
     * thing this value is for is reaching an `href`, where a `javascript:` address runs.
     */
    for (const stored of [
      'javascript:alert(1)',
      'http://linear.app/acme/issue/ABC-402',
      'linear.app/acme/issue/ABC-402',
    ]) {
      const detail = fixture(STRUCTURED)
      detail.review = { ...detail.review, ticket_url: stored }
      await withReview(detail)

      const page = await understand(STRUCTURED)
      const hrefs = page.findAll('a').map(one => one.attributes('href') ?? '')
      expect(hrefs, `${stored} never reaches an href`).not.toContain(stored)
      /* The id is still drawn, as text — the same thing a review with no address gets. */
      expect(page.find('.where').text()).toContain('ABC-402')
      expect(wayOut(page).length).toBe(1)
    }
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
    expect(trail).toContain('15 findings, 1 blocker')
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

  it('draws no card and no control on a step with nothing behind it', async () => {
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

  it('links the six steps that have somewhere to go, and no others', async () => {
    const page = await understand(STRUCTURED)
    const links = page.findAll('.trail a').map(one => one.attributes('href'))
    /*
     * **The trail is the only way to five of these.** Every row in the queue opens
     * Understand, so if the trail did not reach the findings and the draft the queue would
     * lead somewhere a reader could not leave. Step 1 has no page, and a link to a page
     * that does not exist is worse than a label.
     *
     * **Step 7 is a link since DEV-958**, which is the issue that gave it a page: "What
     * happened" is the whole of what came back on a posted review, and before it the only
     * trace of a harvest anywhere was a count in the trail's own tooltip.
     */
    /*
     * **Steps 3 and 4 carry a fragment**, because the three of them are one page since the
     * case page: the address is still each step's own, and the fragment is the band it lands
     * at. `BAND_OF_STEP` in `utils/theCase.ts` owns them.
     */
    expect(links).toEqual([
      `/reviews/${STRUCTURED}/understand`,
      `/reviews/${STRUCTURED}/alternatives#what-was-chosen`,
      `/reviews/${STRUCTURED}/solution#what-was-built`,
      `/reviews/${STRUCTURED}/code-review`,
      `/reviews/${STRUCTURED}/draft`,
      `/reviews/${STRUCTURED}/what-happened`,
    ])
    /* Lint alone is a label. */
    const labels = page.findAll('.trail .face:not(a)').map(one => one.text())
    expect(labels.some(one => one.includes('1 Lint')), '1 Lint should be a label').toBe(true)
    expect(
      labels.some(one => one.includes('What happened')),
      '7 What happened should be a link',
    ).toBe(false)
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
    expect(page.find('.trail .face[aria-current="page"]').text()).toContain('2 Understand')
  })

  /**
   * **Three steps are marked, and one of them is the address.**
   *
   * Steps 2, 3 and 4 are one page, so the trail washes all three: a reader who pressed step
   * 3 and arrived on a page the trail drew as step 2 would read the trail as broken.
   * `aria-current="page"` stays on exactly one, because it names the page a link would take
   * you to and three links claiming to be it tells a screen reader nothing.
   */
  it('marks all three of the steps that are one page, and names the one you asked for', async () => {
    for (const [page, name] of [
      [await understand(STRUCTURED), '2 Understand'],
      [await alternatives(STRUCTURED), '3 Alternatives'],
      [await solution(STRUCTURED), '4 Solution map'],
    ] as const) {
      const here = page.findAll('.trail .face--here')
      expect(here.map(one => one.text().trim())).toEqual([
        '2 Understand',
        '3 Alternatives',
        '4 Solution map',
      ])

      const current = page.findAll('.trail [aria-current="page"]')
      expect(current).toHaveLength(1)
      expect(current[0]!.text()).toContain(name)
    }
  })

  it('marks one step on a page that is only itself', async () => {
    const page = await shown(
      (await import('../app/pages/reviews/[id]/code-review.vue')).default,
      `/reviews/${STRUCTURED}/code-review`,
    )
    expect(page.findAll('.trail .face--here')).toHaveLength(1)
    expect(page.find('.trail .face--here').text()).toContain('5 Code review')
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
    expect(page.text()).toContain('What was wrong · 3')
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
  /*
   * **"Already decided" is gone, and what it said is in two better places.** It was a
   * heading over three levels: which option was built, what was still open, and where the
   * decision came from. The option is on the opening answer line and marked on its own row
   * in band 2; the two figures are the line on band 3's fold, over the lists themselves; and
   * where it came from is in band 1's reference fold. Nothing was dropped, and the section
   * that held copies of all three is.
   */
  it('names the option that was built on the opening line, and marks its row', async () => {
    const page = await understand(STRUCTURED)

    expect(page.find('.answer').text()).toContain('option C built')

    const built = page.findAll('.option').find(one => one.text().includes('Built'))
    expect(built, 'the option that was built is marked on its row').toBeDefined()
    expect(built!.text()).toContain(
      'An export context passed explicitly, and one filter chain for both',
    )
  })

  it('counts what is still open on the fold that holds it', async () => {
    const page = await understand(STRUCTURED)
    const summary = page.findAll('summary')
      .find(one => one.text().includes('choices the ticket left open'))
    expect(summary, 'the fold names what it holds').toBeDefined()
    expect(summary!.text()).toContain('3 choices the ticket left open')
    expect(summary!.text()).toContain('2 places the description and the code disagree')
  })

  it('counts nothing on the fold for a list that is empty', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = { ...detail.brief!, disagreements: [] }
    await withReview(detail)

    const page = await understand(STRUCTURED)
    const summary = page.findAll('summary')
      .find(one => one.text().includes('choices the ticket left open'))!
    expect(summary.text()).toContain('3 choices the ticket left open')
    expect(summary.text(), 'a nought is not drawn').not.toContain('disagree')
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

  it('draws each of the three lists exactly once', async () => {
    const page = await understand(STRUCTURED)
    /*
     * DEV-885's comment: "One list, drawn once." It used to mean one page each — the figures
     * on Understand and the table on Solution — and on one page it means what it says. The
     * fold's line is the figures and what is inside it is the tables, so a reader meets each
     * list once and the count beside its name.
     */
    const text = page.text()
    expect(text.split('Where the ticket left a choice').length - 1).toBe(2)
    expect(text.split('Where the description and the code disagree').length - 1).toBe(2)
    /*
     * Twice each, not once: `DeskTable` draws its own `label` for a screen reader as well as
     * the heading above it. What is being counted here is that the *table* is not drawn
     * twice, which is what two pages drawing one list came to.
     */
    expect(page.findAll('table').filter(one => one.text().includes('Against the ticket')))
      .toHaveLength(1)
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
    const fold = page.findAll('details')
      .find(one => one.text().includes('the problem as recorded'))!

    expect(fold.text()).not.toMatch(/\d+ words/)
    expect(fold.text()).not.toContain('text sketch')
  })

  it('keeps the recorded problem a click away, and interpolated', async () => {
    const page = await understand(STRUCTURED)
    const fold = page.findAll('details')
      .find(one => one.text().includes('the problem as recorded'))
    expect(fold, 'the reference fold names it').toBeDefined()
    /*
     * **Closed, asserted on the element rather than on what is in the document.** A
     * `<details>` holds its content in the DOM whether it is open or not, so "a click away"
     * is the absence of the `open` attribute — which is also the only thing a browser reads.
     */
    expect(fold!.attributes('open')).toBeUndefined()

    const recorded = fold!.findAll('.rd-recorded')
      .find(one => one.text().includes('Scheduled exports run with whatever tenant'))
    expect(recorded, 'and the record is inside it, interpolated').toBeDefined()
  })

  /*
   * And the record is not drawn twice. The problem is at the foot and where the decision
   * came from is under "Already decided"; before DEV-892 the foot held both, and folding
   * the provenance in its own section without taking it out of the foot would have put the
   * same paragraph on the page in two places.
   */
  it('draws each piece of recorded text in exactly one place', async () => {
    const page = await understand(STRUCTURED)

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

  it('draws a row per option and a dot per numbered problem', async () => {
    const page = await alternatives(STRUCTURED)
    expect(page.findAll('.option')).toHaveLength(8)
    /*
     * **The verdicts are one column of dots, not a column each.** A column each made every
     * problem as wide as the widest word in it — "Not assessed" — and eight rows of that
     * took the width of the page. Three dots per row, and the numbers over them.
     */
    expect(page.find('.caption-row .verdicts').findAll('.mark')).toHaveLength(3)
    expect(page.findAll('.option')[0]!.findAll('.verdict')).toHaveLength(3)
  })

  /**
   * **A problem column is headed by its number and its label, never by its sentence.**
   *
   * DEV-892's defect 2. This heading used to be `was_wrong` — a line of prose over a
   * column of three verdicts, which on the real review wrapped ten lines deep. The test
   * above asserted the sentence was there, which is how the defect survived an invented
   * example: it was a measurement of the wrong thing.
   */
  it('heads a verdict column with its number, never with its sentence', async () => {
    const page = await alternatives(STRUCTURED)
    const head = page.find('.caption-row .verdicts')

    expect(head.findAll('.mark')).toHaveLength(3)
    expect(head.text()).toContain('1')
    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(head.text(), `problem ${problem.number}'s sentence is not a heading`)
        .not.toContain(problem.was_wrong)
    }
    /* And each dot names its problem for a reader who cannot see the column it is in. */
    expect(page.findAll('.option')[0]!.find('.verdict').text())
      .toContain('1 · The shared slot')
  })

  it('heads a verdict column with the number alone when the brief wrote no label', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.problems = detail.brief!.problems.map(problem => ({
      ...problem,
      label: null,
    }))
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    const head = page.find('.caption-row .verdicts')

    expect(head.findAll('.mark')).toHaveLength(3)
    for (const problem of detail.brief!.problems) {
      expect(head.text(), 'the sentence is never promoted to a heading')
        .not.toContain(problem.was_wrong)
    }
    /* "Problem 1" rather than a bare number, which is what a dot alone would announce. */
    expect(page.findAll('.option')[0]!.find('.verdict').text()).toContain('Problem 1')
  })

  /**
   * And the sentence is said in full **on the same page**, in the band above.
   *
   * It used to be a fold under the matrix called "What each number is, in full", because the
   * sentences were a page away. On one page they are the rows of band 1, three inches up and
   * open — which is the fold's own purpose met without a control.
   */
  it('says what every number is, in full, in the band above', async () => {
    const page = await alternatives(STRUCTURED)

    expect(page.find('details.numbers').exists(), 'no fold for it any more').toBe(false)
    const rows = page.findAll('.problem')
    expect(rows).toHaveLength(3)
    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(rows.map(one => one.text()).join(' | '), `problem ${problem.number} in full`)
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
    const labels = page.findAll('.option [data-label]')
      .map(cell => cell.attributes('data-label'))

    expect(labels).toContain('1 · The shared slot')
    for (const problem of fixture(STRUCTURED).brief!.problems) {
      expect(labels.join(' | ')).not.toContain(problem.was_wrong)
    }
  })

  it('tells the three real verdicts apart, by word as well as by colour', async () => {
    const page = await alternatives(STRUCTURED)
    /*
     * **The word is in the markup though the cell draws a shape.** `VerdictCell`'s compact
     * mode hides it visually and no further, so a screen reader reads "Fixed" and nothing
     * on the page depends on telling four shapes apart.
     */
    const sheet = page.find('.matrix').text()
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
    /* "Ticket", not "Ticket A": the letter is the row's first column. */
    expect(both.find('.seen').text()).toContain('Ticket')
    expect(both.find('.seen').text()).toContain('Blind')

    const ticketOnly = page.findAll('.option').find(one =>
      one.text().includes('A tenant-scoped database connection per export'))!
    expect(ticketOnly.find('.seen').text()).toContain('Ticket')
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

  /*
   * **The two cards are gone and what they said is on the rows and the opening line.**
   * "Built" and "The blind pass would pick" were a pair of cards under the matrix drawing
   * the same two facts the matrix already marks — the option's letter, its title and
   * whether the two agree. The one thing a row cannot carry is `pick_why`, which is a
   * sentence, and it is drawn as one.
   */
  it('says so on the line and on the row when the pass agrees with the build', async () => {
    const page = await alternatives(STRUCTURED)

    expect(page.find('.answer').text()).toContain('blind pass agrees')
    expect(page.find('.pick-why').text()).toContain('The blind pass picked the same option')
    expect(page.find('.pick-why').text())
      .toContain('It puts the split where the paths genuinely differ')
    /* No second copy of it as a card. */
    expect(page.text()).not.toContain('Built against what a fresh pair of eyes would pick')
  })

  it('says when the blind pass would have picked something else', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.blind_pass = { ...detail.brief!.blind_pass!, pick_key: 'F' }
    await withReview(detail)

    const page = await alternatives(STRUCTURED)
    expect(page.find('.answer').text()).toContain('blind pass would pick F')

    const picked = page.findAll('.option')
      .find(one => one.text().includes('Extract reporting into a service'))!
    expect(picked.text()).toContain('Blind pass would pick')
    expect(page.find('.pick-why').text()).toContain('The blind pass would pick F')
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
    /* Eight options: four on the ticket, seven from the pass, three on both. */
    expect(page.text()).toContain('4 from the ticket')
    expect(page.text()).toContain('7 from the blind pass')
    /*
     * **What it was given and what it looked up are behind one control**, with the band's
     * own fold, and its line says both are there. DEV-892's measurement stands: printed
     * open, "It looked up:" was a 91-word paragraph in a card beside another card.
     */
    const fold = page.findAll('details.rd-fold')
      .find(one => one.find('summary').text().includes('what it read'))
    expect(fold, 'the fold names both').toBeDefined()
    expect(page.text()).not.toContain('It looked up:')
    expect(fold!.text()).toContain('What it looked up, in its own words')
    expect(fold!.text()).toContain('nothing')

    /* And what it was given, verbatim, on the page rather than linked to. */
    expect(fold!.text()).toContain(
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
      /* "above", because the line sits under the matrix it is about rather than over it. */
      expect(page.text()).toContain('What it put forward is marked above.')
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
      /*
       * `repeat(0, …)` was invalid and a browser dropped the whole declaration, taking the
       * column layout off the header and every row with it. The dots are one track now, so
       * the failure would be quieter and no better: a track of `calc(0 * 22px)` is an
       * 18-pixel gap where the verdicts are not. The class is what takes the track out.
       */
      expect(page.find('.matrix').classes()).toContain('matrix--unnumbered')
      expect(page.findAll('.verdict')).toHaveLength(0)
      expect(page.findAll('.option')).toHaveLength(8)
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
    /*
     * **In the band, not as the page's heading.** The page is three bands now and its
     * heading is the brief's own sentence, so an empty step 3 is a notice inside band 2 —
     * which is what lets the other two bands go on being read while a session writes this
     * one.
     */
    const band = page.find('#what-was-chosen')
    expect(band.text()).toContain(headline)
    expect(band.text()).toContain(why)
    /* Never the sentence the defect was: it asserted an absence the record contradicted. */
    expect(page.text()).not.toContain('No alternatives are recorded')
  })
})

/* =================================== Solution =================================== */

describe('Solution', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /** The rows of the table, in the order they are drawn — which is by group, not by number. */
  function rows(page: VueWrapper) {
    return page.findAll('.part')
  }

  /** The row whose title contains this. */
  function row(page: VueWrapper, title: string) {
    const found = rows(page).find(one => one.find('.part-title').text().includes(title))
    if (!found) throw new Error(`no row titled ${title}`)
    return found
  }

  it('carries the verdict on the approach under the headline', async () => {
    const page = await solution(STRUCTURED)
    /*
     * **The line under the headline, not the headline.** The page's heading is the brief's
     * own sentence now, because one page cannot have three headings — and the verdict is a
     * judgement on the approach, which is what the line under a headline is for.
     */
    expect(page.find('h1').text()).toContain(
      'Scheduled exports get their own context',
    )
    expect(page.find('.sub').text()).toContain('C is the right shape.')
    /*
     * And the size of the change, counted off the parts rather than asserted. **Every
     * file the parts name, and how many of them the pull request did not touch.** The
     * rows below count claimed files too, so a line counting only changed ones would not
     * sum to the column under it — and a reader holding this against GitHub's own "24
     * files changed" is told the difference rather than left to find it. `sub`'s
     * docstring has the argument and the two readings this replaced.
     */
    expect(page.find('#what-was-built').text())
      .toContain('7 parts · 27 files (3 not changed) · +959 −423')
  })

  /**
   * **A part is a row, not a card**, which is the whole of DEV-919.
   *
   * The card carried the part's entire summary — 33 to 52 words on the review this was
   * measured against — and reprinted its files and its findings. The row carries one line,
   * the problems it fixes, its size, and where the review looked.
   */
  it('draws a row per part, with its one line, its size and its counts', async () => {
    const page = await solution(STRUCTURED)
    expect(rows(page)).toHaveLength(7)

    const context = row(page, 'The export context')
    /* The one line the session wrote, not the summary. */
    expect(context.find('.one-line').text())
      .toBe('Every export is handed the tenant it is for.')
    expect(context.find('.cell--size').text()).toContain('3 files')
    expect(context.find('.cell--size').text()).toContain('+134 −70')
    /* Two findings on this part, one of them held back: two counts, in plain words. */
    expect(context.find('.cell--found').text()).toContain('1 to post')
    expect(context.find('.cell--found').text()).toContain('1 held for a ticket')

    /* And nothing that was on the card: no summary, no file list, no finding reprinted. */
    expect(context.text()).not.toContain('The slot that lived on the request is gone')
    expect(context.text()).not.toContain('app/Reporting/ExportContext.php')
  })

  it('says "Nothing" on a row the review found nothing in', async () => {
    const page = await solution(STRUCTURED)
    expect(row(page, 'Tests and generated types').find('.cell--found').text()).toContain('Nothing')
  })

  it('says "unchanged" rather than two noughts for a part outside the change', async () => {
    const page = await solution(STRUCTURED)
    const size = row(page, 'The long-running queue it relies on').find('.cell--size')
    expect(size.text()).toContain('1 file')
    expect(size.text()).toContain('unchanged')
    expect(size.text()).not.toContain('+0')
  })

  /**
   * **The three groups, from the issue's own rule**, and a heading only where there is a
   * row under it.
   *
   * The order the rule is applied in is what is being checked as much as the grouping: a
   * part the pull request did not touch belongs under "Not changed" whatever it claims to
   * fix, because it is not something that was built.
   */
  it('puts each row under the group the rule gives it', async () => {
    const page = await solution(STRUCTURED)
    const headings = page.findAll('.part-group').map(one => one.text())
    expect(headings).toEqual([
      'Fixes a problem',
      'Supporting',
      'Not changed, but the change relies on it',
    ])

    /* Read in drawn order: five that fix something, then the tests, then the queue. */
    const titles = rows(page).map(one => one.find('.part-title').text())
    expect(titles[4]).toBe('The web path, with the branch taken out')
    expect(titles[5]).toBe('Tests and generated types')
    expect(titles[6]).toBe('The long-running queue it relies on')
  })

  it('draws no heading for a group with no part in it', async () => {
    const detail = fixture(STRUCTURED)
    /* Every part fixing a problem and in the change: one group, not three. */
    detail.brief!.parts = detail.brief!.parts
      .filter(part => part.fixes.length > 0 && part.files.some(file => file.in_diff))
    expect(detail.brief!.parts.length, 'some parts fix a problem').toBeGreaterThan(0)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.findAll('.part-group').map(one => one.text())).toEqual(['Fixes a problem'])
    expect(page.text()).not.toContain('Not changed, but the change relies on it')
    expect(page.text()).not.toContain('Supporting')
  })

  it('names the problems a part fixes, as the tags the flows already use', async () => {
    const page = await solution(STRUCTURED)
    const tags = row(page, 'The scheduled export\'s own entry point').findAll('.tag')
    /* The second part fixes problems 1 and 2, and the tag carries each one's name. */
    expect(tags).toHaveLength(2)
    expect(tags[0]!.text()).toContain('The shared slot')
    expect(tags[1]!.text()).toContain('The builder\'s branch')

    /* A part that fixes none says so in a word rather than leaving the cell empty. */
    expect(row(page, 'Tests and generated types').findAll('.tag')).toHaveLength(0)
    expect(row(page, 'Tests and generated types').find('.cell--fixes').text()).toContain('None')
  })

  /**
   * **A row says where the review looked; it does not grade the part.**
   *
   * Patrick's rule, and the one thing on this table that would be easiest to put back by
   * accident: a severity tag in a cell, or a count painted by how bad it is. Asserted on
   * the rendered row rather than by reading the source, because the way it comes back is
   * somebody adding a component that carries its own colour.
   */
  it('carries no severity mark and no colour that differs by severity on a row', async () => {
    const page = await solution(STRUCTURED)
    for (const one of rows(page)) {
      const where = one.find('.part-title').text()
      expect(one.findAll('.severity-tag'), `${where} draws no severity tag`).toHaveLength(0)
      expect(one.find('.cell--found').html(), `${where}'s counts carry no severity class`)
        .not.toMatch(/blocker|should|nit|question/)
    }
    /* The whole section draws no severity tag at all, inside an opened row or on one. */
    await open(page, 'Allowed columns on the report definition')
    expect(page.findAll('.severity-tag')).toHaveLength(0)
  })

  it('counts the same findings on a row that the fold inside it lists', async () => {
    const page = await solution(STRUCTURED)
    const policy = row(page, 'Allowed columns on the report definition')
    /* F6 and F9 are both in `ColumnPolicy.php`, which this part names. */
    expect(policy.find('.cell--found').text()).toContain('2 to post')

    await open(page, 'Allowed columns on the report definition')
    const opened = page.find('#part-2')
    expect(opened.findAll('.finding')).toHaveLength(2)
    expect(opened.text()).toContain('F6')
    expect(opened.text()).toContain('F9')
  })

  /* ------------------------------- A row, opened ------------------------------- */

  it('opens a row in place, to the part’s full summary', async () => {
    const page = await solution(STRUCTURED)
    expect(page.find('#part-0').exists()).toBe(false)

    await open(page, 'The export context')

    const opened = page.find('#part-0')
    expect(opened.exists()).toBe(true)
    /*
     * The summary is here and nowhere else: the row above carries the one line, and the
     * whole point of the change is that the summary is not read down a column.
     */
    expect(opened.find('.summary').text())
      .toContain('The slot that lived on the request is gone')
    /*
     * And it says more than the row's own line, which is what the fold is for —
     * **normalised the way the store test normalises it**, so the one line with a full
     * stop added does not satisfy this while showing the reader the same sentence twice.
     */
    const plain = (text: string) => text.trim().replace(/\.+$/, '')
    expect(plain(opened.find('.summary').text()))
      .not.toBe(plain(row(page, 'The export context').find('.one-line').text()))
  })

  it('lists one line per finding in the part, linking each to that finding', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'Allowed columns on the report definition')

    const lines = page.find('#part-2').findAll('.finding')
    const blocker = lines.find(one => one.text().includes('F9'))!
    /* Severity, the reference, the title, and where it stands — and nothing else. */
    expect(blocker.find('.severity').text()).toBe('blocker')
    expect(blocker.find('.what').text())
      .toBe('The column policy is skipped entirely when the definition names no columns')
    expect(blocker.find('.stands').text()).toContain('To post')
    /* No claim, no evidence, no code: the code review page owns those. */
    expect(blocker.text()).not.toContain('ColumnPolicy::for')

    /* **The whole line reaches that finding**, not the page it is on. */
    expect(blocker.attributes('href')).toBe(`/reviews/${STRUCTURED}/code-review#F9`)
  })

  it('says where a held-back finding stands on its own line', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'The export context')
    const held = page.find('#part-0').findAll('.finding')
      .find(one => one.text().includes('F2'))!
    expect(held.find('.stands').text()).toContain('Held for a ticket')
  })

  it('says in words when the review found nothing in the part', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'Tests and generated types')
    expect(page.find('#part-5').text()).toContain('The code review found nothing in this part')
  })

  /**
   * **The files are reference, so they are behind a fold and the fold is closed.** Up to
   * seven paths per part was half of what the old card printed on every one of them.
   */
  it('puts the files behind a fold that is closed, and reachable', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'The export context')

    const fold = page.find('#part-0').find('details')
    expect(fold.exists()).toBe(true)
    expect((fold.element as HTMLDetailsElement).open, 'closed by default').toBe(false)
    /* A `<summary>` is focusable and opens on Enter and Space without any script. */
    expect(fold.find('summary').text()).toBe('3 files in this part')
    expect(fold.text()).toContain('app/Reporting/ExportContext.php')
    expect(fold.text()).toContain('+112 −0')
    expect(fold.text()).toContain('+8 −61')
  })

  it('says of a file outside the change that it is not in it', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'One filter chain for both paths')

    const outside = page.find('#part-3').findAll('.file')
      .find(one => one.text().includes('app/Audit/RecordsAnExport.php'))!
    /*
     * `in_diff: false` is how a part claims a file the pull request never touched, and such
     * a file has no lines either way — so the fact replaces the figures rather than being
     * drawn as a pair of noughts.
     */
    expect(outside.text()).toContain('not in the change')
    expect(outside.text()).not.toContain('+0')
  })

  it('opens one row at a time', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'The export context')
    expect(page.find('#part-0').exists()).toBe(true)

    await open(page, 'Allowed columns on the report definition')
    expect(page.find('#part-2').exists()).toBe(true)
    expect(page.find('#part-0').exists(), 'the first row folded itself').toBe(false)
  })

  it('folds a row again when its own control is pressed', async () => {
    const page = await solution(STRUCTURED)
    await open(page, 'The export context')
    await open(page, 'The export context')
    expect(page.find('#part-0').exists()).toBe(false)
  })

  /**
   * **The row is a button, which is what makes it work from the keyboard.**
   *
   * The real keypress is in `e2e/review-desk.spec.ts`, where Enter on a focused button
   * actually fires a click. What is checked here is the mechanism that makes it possible —
   * a native button, and the two attributes that tell a screen reader what it controls.
   */
  it('draws each row as a button that says what it controls', async () => {
    const page = await solution(STRUCTURED)
    for (const [at, one] of rows(page).entries()) {
      expect(one.element.tagName, `row ${at + 1} is a button`).toBe('BUTTON')
      expect(one.attributes('aria-expanded')).toBe('false')
      expect(one.attributes('aria-controls')).toBe(`part-${at}`)
    }

    await open(page, 'The export context')
    expect(row(page, 'The export context').attributes('aria-expanded')).toBe('true')
  })

  /* ------------------------------ The one line -------------------------------- */

  /**
   * **A part with no `one_line` draws one line, not four**, beside parts that have one.
   *
   * DEV-919's fourth criterion, with the summary at the length the review this was measured
   * against actually ran to.
   *
   * **Both halves of this are a fixture rather than the contract's example**, and for one
   * reason: that example is the document review-craft's `brief_lint.py` is run against, and
   * as of 11 October 2026 that linter caps a part's summary at 30 words and fails a brief
   * with any part missing a `one_line`. So the example carries a short one line on every
   * part, and the record this test describes — one part with none, and a summary at 52
   * words — is a brief written *before* both rules. Which is the point: those briefs exist,
   * nothing will rewrite them, and the page has to draw them as one line all the same.
   *
   * The clipping itself is CSS and is measured in a real browser at 1440 and at 390 by
   * `e2e/review-desk.spec.ts`. What is checked here is the branch: the summary stands in
   * for the one line, it is marked as a cut, and the whole of it is still reachable — and
   * that a page holding both kinds of part draws each the way it should.
   */
  const FIFTY_TWO_WORDS
    = 'Replaces the request-scoped slot with a context object constructed by whatever '
      + 'starts an export, so the scheduled path and the web path each carry the tenant '
      + 'they are acting for explicitly rather than reading it out of a shared surface '
      + 'that only one of the two of them ever writes to today.'

  it('draws the summary as the row’s one line where the session wrote none', async () => {
    expect(FIFTY_TWO_WORDS.split(/\s+/).length, 'the summary is at real length').toBe(52)

    const detail = fixture(STRUCTURED)
    detail.brief!.parts = detail.brief!.parts.map((part, at) =>
      at === 0 ? { ...part, one_line: null, summary: FIFTY_TWO_WORDS } : part)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    const line = row(page, 'The export context').find('.one-line')
    expect(line.text()).toBe(FIFTY_TWO_WORDS)
    /* Marked as a cut, and the whole of it one hover away. */
    expect(line.classes()).toContain('one-line--cut')
    expect(line.attributes('title')).toBe(FIFTY_TWO_WORDS)

    /* A part whose one line *was* written is not marked, and carries no title attribute. */
    const written = row(page, 'The scheduled export\'s own entry point').find('.one-line')
    expect(written.classes()).not.toContain('one-line--cut')
    expect(written.attributes('title')).toBeUndefined()

    /*
     * **One page, both kinds of part**, which is the arrangement the record will actually
     * be in while briefs written before the rule are still on the desk. Six written, one
     * not — and the page does not treat the odd one out as an error.
     */
    const cut = rows(page)
      .filter(one => one.find('.one-line').classes().includes('one-line--cut'))
    expect(cut, 'exactly the one part with no one line is drawn as a cut').toHaveLength(1)
    expect(rows(page)).toHaveLength(7)
  })

  /**
   * **A finding this round closed says so on its line.**
   *
   * The row's counts group by disposition, which is what makes them sum to the part's
   * findings — and a finding carried into this round as `fixed` keeps the disposition it
   * was carried with, so it sits in "to post" and will not be posted. The round status
   * beside its title is the only thing on the page that explains the difference.
   *
   * **This is a guard against a regression, not a new rule.** The cards this table
   * replaces drew the round status, DEV-892 having put it there after the same overclaim;
   * an earlier draft of the table dropped it, and reviewing this change found it gone.
   */
  it('says on a finding\u2019s line what became of it this round', async () => {
    const detail = fixture(STRUCTURED)
    const carried = detail.findings.find(
      finding => finding.file === 'app/Reporting/ColumnPolicy.php',
    )!
    expect(carried, 'the example has a finding on a part to carry').toBeTruthy()

    detail.findings = detail.findings.map(finding =>
      finding.seq === carried.seq
        ? { ...finding, round_status: 'fixed' as const, continues_review_id: 40 }
        : finding)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    /* Its disposition did not move, so the row still counts it where the record puts it. */
    expect(row(page, 'Allowed columns on the report definition').find('.cell--found').text())
      .toContain('2 to post')

    await open(page, 'Allowed columns on the report definition')
    const line = page.find('#part-2').findAll('.finding')
      .find(one => one.text().includes(carried.key))!
    expect(line.find('.carried').text(), 'the line says what became of it').toBe('Fixed')

    /*
     * **And a space separates it from the title.** Found reviewing this change: the two
     * text nodes touched, so the link read "…names no columnsFixed" to a screen reader
     * and to anyone copying it, with only a margin between them on screen.
     */
    expect(line.find('.what').text()).toContain(' Fixed')
    expect(line.find('.what').text()).not.toContain('columnsFixed')
  })

  it('says the same of a finding in no part', async () => {
    const detail = fixture(STRUCTURED)
    const loose = detail.findings.find(finding => finding.file == null)!
    detail.findings = detail.findings.map(finding =>
      finding.seq === loose.seq
        ? { ...finding, round_status: 'fixed' as const, continues_review_id: 40 }
        : finding)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    const line = page.findAll('.findings--loose .finding')
      .find(one => one.text().includes(loose.key))!
    expect(line.find('.carried').text()).toBe('Fixed')
    expect(line.find('.what').text()).toContain(' Fixed')
  })

  /* --------------------------- Findings in no part ---------------------------- */

  it('lists the findings no part accounts for, with the count as the heading', async () => {
    const page = await solution(STRUCTURED)
    /* F14 names no file; F15 names one no part claims. Two different ways in. */
    expect(page.text()).toContain('2 findings are not about any one part')

    const loose = page.findAll('.findings--loose .finding')
    expect(loose).toHaveLength(2)
    expect(loose.map(one => one.find('.key').text())).toEqual(['F14', 'F15'])
    /* And each one reaches that finding, the same way a row's own lines do. */
    expect(loose[1]!.attributes('href')).toBe(`/reviews/${STRUCTURED}/code-review#F15`)
  })

  it('agrees with itself about one finding in no part', async () => {
    const detail = fixture(STRUCTURED)
    const claimed = new Set(
      detail.brief!.parts.flatMap(part => part.files.map(file => file.path)),
    )
    /* One left over: everything else accounted for. */
    const loose = detail.findings.filter(
      finding => finding.file == null || !claimed.has(finding.file),
    )
    detail.findings = detail.findings.filter(
      finding => finding === loose[0] || (finding.file != null && claimed.has(finding.file)),
    )
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.text()).toContain('1 finding is not about any one part')
  })

  it('draws no such list when every finding is on a part', async () => {
    const detail = fixture(STRUCTURED)
    const claimed = new Set(
      detail.brief!.parts.flatMap(part => part.files.map(file => file.path)),
    )
    detail.findings = detail.findings.filter(
      finding => finding.file != null && claimed.has(finding.file),
    )
    await withReview(detail)

    const page = await solution(STRUCTURED)
    expect(page.text()).not.toContain('not about any one part')
  })

  /**
   * **Every finding of the review is on this page exactly once.**
   *
   * DEV-919's second criterion, counted against the record rather than against a number
   * written down here. It is the criterion the page was most likely to fail quietly: the
   * card version matched a finding to *every* part whose files named its path, so a file
   * claimed by two parts put its findings under both and the counts summed to more findings
   * than the review has.
   */
  it('shows every finding of the review exactly once, counted off the record', async () => {
    const page = await solution(STRUCTURED)
    const detail = fixture(STRUCTURED)

    /* Every row's fold, opened in turn, plus the list under the table. */
    const seen: string[] = page.findAll('.findings--loose .finding')
      .map(one => one.find('.key').text())

    for (const part of detail.brief!.parts) {
      await open(page, part.title)
      const opened = page.findAll('.opened')
      expect(opened, 'one row open at a time').toHaveLength(1)
      seen.push(...opened[0]!.findAll('.finding .key').map(one => one.text()))
    }

    const onTheRecord = detail.findings.map(finding => finding.key).sort()
    expect([...seen].sort(), 'each finding once, and all of them').toEqual(onTheRecord)

    /* And the row counts sum to the same total, which is what makes the cells checkable. */
    const counted = page.findAll('.cell--found .found')
      .reduce((total, one) => total + Number(/^\d+/.exec(one.text())?.[0] ?? 0), 0)
    expect(counted + seen.filter(key => ['F14', 'F15'].includes(key)).length)
      .toBe(detail.findings.length)
  })

  it('picks a finding up on a part the brief wrote before it existed', async () => {
    const detail = fixture(STRUCTURED)
    detail.findings = [
      ...detail.findings,
      {
        review_id: STRUCTURED,
        seq: 16,
        key: 'F16',
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
    /*
     * The join is a join, not a stored list — so a finding written after the brief appears
     * on its part, and in its row's count, without the brief being rewritten.
     */
    expect(row(page, 'The export context').find('.cell--found').text()).toContain('2 to post')
    await open(page, 'The export context')
    expect(page.find('#part-0').text()).toContain('F16')
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
    /* The band says the state, and says it once. */
    expect(page.find('.sub').text()).toBe(detail.brief!.approach_verdict)
    expect(
      page.find('#what-was-built').text().split(said).length - 1,
      `${said} is in the band once`,
    ).toBe(1)
    expect(page.text()).toContain(why)
    expect(page.text()).not.toContain('The change has not been described in parts yet.')
  })

  it('says the state in the band when there is no verdict on the page at all', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief!.parts = []
    detail.brief!.approach_verdict = null
    detail.layers = detail.layers.map(layer =>
      layer.name === 'solution_map' ? { ...layer, state: 'running' as const } : layer)
    await withReview(detail)

    const page = await solution(STRUCTURED)
    /* No verdict, so no line under the headline — and nothing standing in for one. */
    expect(page.find('.sub').exists()).toBe(false)
    expect(page.find('#what-was-built').text()).toContain('Being worked on now.')
    /*
     * And said once. "One sentence, once" is the issue's own wording about exactly this
     * pair, and on one page the pair is the band's notice and anything else that triaged
     * the same step.
     */
    expect(page.find('#what-was-built').text().split('Being worked on now.').length - 1)
      .toBe(1)
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

  /**
   * **Nothing the three pages drew is off the page.**
   *
   * The rule the Draft page's fold established and this change is the second application
   * of: a page may fold or reorder what it draws, and may never drop any of it. Three pages
   * became one and the obvious failure is a field that quietly went with a section — so
   * this walks every string the brief holds and asserts the page draws it, which is a
   * stronger statement than any list of sections could be.
   *
   * The parts' summaries and their file paths are behind a row, as they were on Solution, so
   * each row is opened in turn and the text accumulated.
   */
  it('draws every string the brief holds, somewhere on the page', async () => {
    const page = await understand(STRUCTURED)
    const brief = fixture(STRUCTURED).brief!

    /*
     * Opening every row of all three bands, one at a time — a problem's named detail, an
     * option's verdict lines and a part's summary and files are each behind their own row,
     * as they were on the three pages. One is open at a time, so the text is accumulated.
     */
    let text = page.text()
    for (const selector of ['button.problem', 'button.option', 'button.part']) {
      for (let at = 0; at < page.findAll(selector).length; at += 1) {
        await page.findAll(selector)[at]!.trigger('click')
        await settle(page)
        text += page.text()
      }
    }

    const said: string[] = [
      brief.headline!,
      brief.how_it_works_today!,
      brief.problem,
      brief.provenance!,
      brief.approach_verdict!,
      brief.flow_before_caption!,
      brief.flow_after_caption!,
      brief.support_table!.title!,
      ...brief.support_table!.columns,
      ...brief.support_table!.rows.flat(),
      ...brief.problems.flatMap(problem => [
        problem.label!,
        problem.was_wrong,
        problem.now_fixed!,
        problem.detail_title!,
        ...problem.detail_before,
        ...problem.detail_after,
      ]),
      ...brief.options.flatMap(option => [
        option.title,
        option.argument_for,
        option.argument_against,
        ...(option.verdicts ?? []).map(verdict => verdict.why).filter(why => why != null),
      ]),
      brief.blind_pass!.given,
      brief.blind_pass!.looked_up,
      brief.blind_pass!.pick_why!,
      ...brief.blind_pass!.questions,
      ...brief.parts.flatMap(part => [
        part.title,
        part.summary,
        ...part.files.map(file => file.path),
      ]),
      ...brief.open_choices.flatMap(choice => [choice.left_open, choice.chosen, choice.why]),
      ...brief.disagreements.flatMap(clash => [clash.description_says, clash.code_does]),
    ]

    /*
     * **Compared with the backtick pairs taken out of both sides.** Every one of these
     * fields goes through `TickedText`, which draws a `` `name` `` as a code chip — so the
     * rendered text holds the name and not the backticks around it. Three of the detail
     * lines in this fixture are nothing but code names.
     */
    const plain = (one: string) => one.replace(/`/g, '')
    const drawn = plain(text)
    const missing = said.filter(one => one != null && !drawn.includes(plain(one)))
    expect(missing, 'every field the three pages drew is still drawn').toEqual([])
    /* And the list is not vacuously empty. */
    expect(said.length).toBeGreaterThan(80)
  })

  it('says there is no brief at all differently again', async () => {
    const detail = fixture(STRUCTURED)
    detail.brief = null
    await withReview(detail)

    const page = await solution(STRUCTURED)
    /*
     * **One sentence, because there is one page.** Understand said "about this problem" and
     * Solution "about this change"; the page that draws both says the first, and the line
     * under it names all three steps.
     */
    expect(page.text()).toContain('Nothing has been written about this problem yet.')
    expect(page.text()).toContain('Steps 2 to 4 are where a session writes')
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

  /**
   * **A hidden label that ends in a colon is followed by an interpolated space.**
   *
   * `rd-visually-hidden` is text in the document, so `<span …>Severity:</span>{{ value }}`
   * reads `Severity:blocker`. A *newline* between the two is no better: Vue's
   * `whitespace: 'condense'` deletes a whitespace-only text node between two elements.
   *
   * **And `&#32;` is not the answer either, which is why this rule names the form it
   * wants.** Compiled with this repository's own `@vue/compiler-dom`, an entity at the end
   * of a line is merged with the following newline and indentation into one whitespace-only
   * node and deleted with it — so `</span>&#32;` followed by an element, or by a comment,
   * emits no text node at all. It happens to work when what follows is on the same line,
   * which is how four boundaries came to be written the broken way and a first version of
   * this very scan came to accept them. An interpolation is a text node wherever it is
   * written.
   *
   * **This is a scan because the rule was swept by hand three times.** DEV-958 found it on
   * its own page, fixed two components beside it, then three more sites on the Solution
   * page, then the four `&#32;` boundaries that compiled to nothing. A rule in a comment is
   * a rule somebody has to remember.
   *
   * Only labels ending in a colon, which is the shape that has a value after it. A hidden
   * span that *continues* a visible phrase — "The finding", then " F1 on the code review
   * page" — carries its own leading space and needs nothing here.
   */
  /**
   * The label, matched **over the whole file and per occurrence** rather than per line.
   *
   * Two things that cost a line-by-line version its coverage. The opening tag is allowed to
   * wrap, because this repository writes every span with two or more attributes that way —
   * `[^>]*` crosses a newline, so `<span\n  v-if="…"\n  class="rd-visually-hidden"\n>Size:</span>`
   * is seen. And the class list is allowed to hold more than the one class.
   *
   * Per occurrence, because a line with two of these passed as long as *one* of them was
   * separated: `LABEL.test(line) && !SEPARATED.test(line)` asks two questions about the
   * whole line and never about the same label twice.
   */
  const LABEL = /<span[^>]*class="[^"]*rd-visually-hidden[^"]*"[^>]*>[^<]*:<\/span>/g
  const SEPARATOR = '{{ \' \' }}'

  /**
   * **Every `.vue` under `app/`, at any depth** — its own list rather than the `sources`
   * the checks above share, which is components and the pages of a review.
   *
   * Those checks are about this feature's pages and are right to be. This rule is about
   * any markup that draws a hidden label, and the queue, the Reviews list, the rules page
   * and the layout all draw one. Widening the shared list would have changed what four
   * other tests are about; a list of its own changes nothing but this.
   */
  function everyTemplate(dir: string): Array<{ name: string, text: string }> {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const full = resolve(dir, entry.name)
      if (entry.isDirectory()) return everyTemplate(full)
      if (!entry.name.endsWith('.vue')) return []
      return [{ name: relative(resolve('app'), full), text: readFileSync(full, 'utf8') }]
    })
  }

  const templates = everyTemplate(resolve('app'))

  /** Every hidden label ending in a colon that nothing separates from its value. */
  function runTogether(files: Array<{ name: string, text: string }>): string[] {
    return files.flatMap(file =>
      [...file.text.matchAll(LABEL)]
        .filter(hit => !file.text.slice(hit.index + hit[0].length).startsWith(SEPARATOR))
        .map(hit => `${file.name}: ${hit[0].replace(/\s+/g, ' ')}`))
  }

  it('finds every template, not only this feature\u2019s', () => {
    const names = templates.map(file => file.name)
    expect(names.length).toBeGreaterThan(40)
    for (const outside of ['pages/index.vue', 'pages/rules.vue', 'layouts/default.vue']) {
      expect(names.map(name => name.split(sep).join('/'))).toContain(outside)
    }
  })

  it('separates a hidden label from the value after it', () => {
    expect(runTogether(templates)).toEqual([])
  })

  /**
   * **And the scan refuses every form that does not work**, run through the same predicate
   * rather than re-tested as regexes against a literal — which is what the version before
   * it did, and why it was green over four broken boundaries.
   *
   * The newline case really carries a newline. The version this replaces split the input on
   * `\n` first, so its "a newline" case was a single line ending at `</span>` — the same
   * input as "nothing at all", testing the same thing twice and the form the argument is
   * really about not at all.
   */
  it.each([
    ['nothing at all', '<span class="rd-visually-hidden">Severity:</span>{{ row.severity }}'],
    ['a newline', '<span class="rd-visually-hidden">Severity:</span>\n  {{ row.severity }}'],
    ['an entity', '<span class="rd-visually-hidden">Severity:</span>&#32;\n  <span>x</span>'],
    ['a wrapped opening tag', '<span\n  v-if="x"\n  class="rd-visually-hidden"\n>Size:</span>\n  <span>x</span>'],
    ['a second class', '<span class="rd-meta rd-visually-hidden">Size:</span>\n  <span>x</span>'],
  ])('reports a label separated by %s', (_what, text) => {
    expect(runTogether([{ name: 'invented.vue', text }])).toHaveLength(1)
  })

  it('reports both labels on a line that carries two', () => {
    const two = '<span class="rd-visually-hidden">A:</span>{{ one }}'
      + '<span class="rd-visually-hidden">B:</span>{{ two }}'
    expect(runTogether([{ name: 'invented.vue', text: two }])).toHaveLength(2)
  })

  it('accepts the one form that works', () => {
    const good = `<span class="rd-visually-hidden">Severity:</span>${SEPARATOR}{{ row.severity }}`
    expect(runTogether([{ name: 'invented.vue', text: good }])).toEqual([])
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
