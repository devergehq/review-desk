// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import {
  resetDesk,
  useDeskSource,
  useReviewDesk,
} from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import Inbox from '../app/pages/index.vue'
import OneReview from '../app/pages/reviews/[id]/index.vue'
import DraftPreview from '../app/pages/reviews/[id]/draft.vue'
import Rules from '../app/pages/rules.vue'
import Layout from '../app/layouts/default.vue'

/*
 * The four screens, mounted. These check that each one renders from the data layer and
 * that the controls are real, labelled elements — not that it looks right, which is what
 * the screenshots in the pull request are for.
 *
 * **Every read is a request now**, so a mount is not finished when `mountSuspended`
 * resolves: the resource's first read is in flight. `shown` is a mount plus a settle, and
 * `settle` is what every interaction is followed by. A test that forgot it would assert on
 * the loading state and say so clearly, which is why that state has a test of its own.
 */

/**
 * What `DeskState` renders while a read is in flight — `Reading the inbox…`.
 *
 * Named here because `settle` waits for it to go away, and a test helper that waits for a
 * sentence should say which sentence rather than hiding it in a regular expression.
 */
const STILL_READING = /Reading [^…]*…/

/**
 * Let every pending read and re-read finish.
 *
 * **Waits for the loading state to go away, with a bound** — not for a fixed number of
 * rounds, and not merely for the rendered text to stop changing.
 *
 * Both of the simpler instruments have now failed here, and the second one is mine. A
 * fixed twelve rounds is a count tuned to how long the work happened to take, which this
 * file already recorded failing once: "a count tuned to the shape before that change left
 * one of these tests asserting on the loading state". Waiting for the text to stop
 * changing looks like the principled replacement and is worse — the text is *perfectly
 * stable* at "Reading the inbox…" for as long as the dynamic `import()` takes, so it
 * returns early with more confidence and no more information. Both failed the same CI test
 * the same way.
 *
 * The condition that actually matters is that the screen is no longer reading. Once it is
 * not, two more quiet rounds flush whatever the answer renders.
 *
 * A macrotask as well as the microtasks, each round. Choosing the API backend is a dynamic
 * `import()` — the HTTP client is deliberately not in the app shell's static graph — and
 * the first one in a run has a module to load from disk, which no number of
 * `flushPromises` can advance.
 */
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

/** Mount a screen and wait for its first read. */
async function shown(
  component: Parameters<typeof mountSuspended>[0],
  options?: Parameters<typeof mountSuspended>[1],
) {
  const page = await mountSuspended(component, options)
  await settle(page)
  return page
}

/** The button with this exact label. */
function button(page: VueWrapper, label: string) {
  return page.findAll('button').find(item => item.text() === label)
}

/** Click the button with this label, and let what it started finish. */
async function click(page: VueWrapper, label: string) {
  const found = button(page, label)
  if (!found) throw new Error(`there is no button labelled ${label!}`)
  await found.trigger('click')
  await settle(page)
}

describe('the shell', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('marks the records as invented on every screen', async () => {
    const page = await shown(Layout)
    expect(page.text()).toContain('Sample data')
  })

  it('offers a way past the navigation for a keyboard', async () => {
    const page = await shown(Layout)
    expect(page.find('a[href="#main"]').exists()).toBe(true)
    expect(page.find('main#main').exists()).toBe(true)
  })

  it('names the three places you can be', async () => {
    const page = await shown(Layout)
    const places = page.findAll('nav a, nav span').map(link => link.text())
    /*
     * "Queue", not "Inbox". The approved boards name it that, and so does DEV-876: it is a
     * queue of reviews with one row each, and "inbox" was the name of the screen that
     * listed one review three times.
     */
    expect(places).toEqual(['Queue', 'Reviews', 'Rules'])
  })

  it('does not link to a review when the desk holds none', async () => {
    await resetDesk({ reviews: [], drafts: [], rules: [] })
    const page = await shown(Layout)
    /*
     * The item stays, so the navigation keeps its three places; it is not a link, because
     * on a fresh database there is no review to link to. It used to point at `/reviews/1`,
     * which was a link to a review that does not exist.
     */
    const reviews = page.findAll('nav a').map(link => link.text())
    expect(reviews).not.toContain('Reviews')
    expect(page.find('nav span[aria-disabled="true"]').text()).toBe('Reviews')
  })
})

describe('the queue', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * **The defect DEV-876 names, and the fixture that has it.** Review 17 is
   * `acme/portal#412`: its brief is unconfirmed, its session wrote a draft anyway, and it
   * is therefore in two of the Inbox's five arrays at once. The screen this replaces drew
   * one group per array and showed it twice.
   */
  it('shows one row for a review with two pending actions', async () => {
    const page = await shown(Inbox)
    const rows = page.findAll('.row')
    const forFour12 = rows.filter(row => row.text().includes('#412'))
    expect(forFour12).toHaveLength(1)
  })

  it('shows that row’s next action, and says what is behind it', async () => {
    const page = await shown(Inbox)
    const row = page.findAll('.row').find(one => one.text().includes('#412'))!
    /* Layer 2 comes before layer 6, so the brief is the next thing the developer does. */
    expect(row.text()).toContain('Problem brief ready')
    expect(row.text()).toContain('nothing posts until you confirm')
    expect(row.text()).toContain('Read the brief')
    /* And the thing that is not the next action is still on the row, not lost. */
    expect(row.text()).toContain('a draft is ready behind it')
    expect(row.text()).not.toContain('Read and sign off')
  })

  it('leads with how many reviews need the developer, counting each once', async () => {
    const page = await shown(Inbox)
    /*
     * 17 (brief and draft), 15 (draft), 16 (draft), 41 (draft), 12 (the author replied).
     * Five, and 17 is counted once rather than twice.
     */
    expect(page.text()).toContain('5 reviews need you')
    expect(page.text()).toContain('2 with the agent')
    expect(page.text()).toContain('2 of your own')
  })

  it('draws the board’s four groups, and hides a group with nothing in it', async () => {
    const page = await shown(Inbox)
    const text = page.text()
    expect(text).toContain('Needs you')
    expect(text).toContain('With the agent')
    expect(text).toContain('Waiting on the author')
    expect(text).toContain('Your pull requests')

    /* With no own pull requests, that group is not drawn at all. */
    await resetDesk({ own_prs: [] })
    const fewer = await shown(Inbox)
    expect(fewer.findAll('h2').map(one => one.text())).not.toContain('Your pull requests')
    expect(fewer.findAll('h2').map(one => one.text())).toContain('With the agent')
  })

  /*
   * The board draws this group; `GET /inbox` has no source for it, and DEV-876 adds no API
   * field. Drawn in the unmeasured state rather than hidden: hiding it would say no review
   * is waiting on an author, which is a claim nothing here can make.
   */
  it('says that reviews waiting on the author are not measured, rather than showing none', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('Waiting on the author')
    expect(page.text()).toContain('Not measured yet')
    expect(page.text()).toContain('serves no group for a posted review awaiting a reply')
  })

  it('shows which layer each running session is on, as a bar and in words', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('Code review')
    const bars = page.findAll('[role="img"]')
    const layers = bars.filter(bar => bar.attributes('aria-label')?.startsWith('Layer '))
    expect(layers).toHaveLength(2)
    expect(layers[0]!.attributes('aria-label')).toBe('Layer 5 of 7, code review')
    expect(layers[0]!.findAll('.seg')).toHaveLength(7)
  })

  it('says what it knows about each session, and says when it knows nothing', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('Session running')
    expect(page.text()).toContain('Session state unknown')
  })

  it('says a row has no recorded time rather than inventing one', async () => {
    const page = await shown(Inbox)
    /* Only a brief carries a ready time. A draft, a reply and a session carry none. */
    expect(page.text()).toContain('no time recorded')
    expect(page.text()).toContain('and the rows with no recorded time last')
  })

  it('filters by repository, and each tab carries the rows it would show', async () => {
    const page = await shown(Inbox)
    const tabs = page.findAll('[role="radiogroup"] [role="radio"]').map(tab => tab.text())
    expect(tabs[0]).toBe('All repositories')
    expect(tabs).toContain('acme/portal· 6')
    expect(tabs).toContain('acme/billing-api· 3')

    const billing = page
      .findAll('[role="radio"]')
      .find(tab => tab.text().startsWith('acme/billing-api'))!
    await billing.trigger('click')
    await settle(page)

    expect(page.text()).toContain('1 review needs you')
    expect(page.text()).not.toContain('#412')
    expect(page.text()).toContain('#88')
  })

  /*
   * The three typefaces, where the boards put them: Newsreader for a review's title,
   * JetBrains Mono for a pull request number. Asserted on the elements rather than on the
   * stylesheet, because what matters is which words wear which face.
   */
  it('sets a review’s title in the display face and its number in the mono one', async () => {
    const page = await shown(Inbox)
    const row = page.findAll('.row').find(one => one.text().includes('#412'))!
    expect(row.find('.rd-display-row').text()).toBe('Retry failed webhook deliveries')
    expect(row.find('.pr').classes()).toContain('rd-mono')
    expect(row.find('.pr').text()).toBe('#412')
  })

  it('has a labelled field for starting a review', async () => {
    const page = await shown(Inbox)
    const field = page.find('input#start')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="start"]').text()).toBe('Start a review')
  })

  it('refuses a link that is not a pull request, and says what it wanted', async () => {
    const page = await shown(Inbox)
    await page.find('input#start').setValue('nonsense')
    await page.find('.go').trigger('click')
    await settle(page)

    expect(page.text()).toContain('Paste a link like github.com/acme/portal/pull/412.')
    expect(page.text()).toContain('Accepted: a pull request link')
    /* A refused link is the developer's typing, so the field says so rather than a box. */
    expect(page.find('.shell--refused').exists()).toBe(true)
    expect(page.find('.notice--blocked').exists()).toBe(false)
  })

  it('says differently when GitHub could not be read, and names the fix', async () => {
    await resetDesk({ github_available: false })
    const page = await shown(Inbox)
    await page.find('input#start').setValue('https://github.com/acme/portal/pull/501')
    await page.find('.go').trigger('click')
    await settle(page)

    expect(page.text()).toContain('gh auth login')
    /* The link stays in the field: nothing is wrong with it, so retyping it is waste. */
    expect((page.find('input#start').element as HTMLInputElement).value).toBe(
      'https://github.com/acme/portal/pull/501',
    )
  })

  it('links a pull request of the developer’s own out to GitHub rather than writing to it', async () => {
    const page = await shown(Inbox)
    const out = page
      .findAll('a')
      .find(link => link.attributes('href')?.startsWith('https://github.com/'))
    expect(out).toBeDefined()
    expect(out!.attributes('href')).toBe('https://github.com/acme/portal/pull/415')
    expect(out!.attributes('rel')).toContain('noopener')
    expect(out!.text()).toContain('Open on GitHub')
  })

  it('merges a review out of the queue once its brief is confirmed', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('Problem brief ready')

    /*
     * Confirmed from the data layer rather than from this screen, which has no control for
     * it — and then the screen re-reads itself, because a write refreshes every open read.
     */
    await useReviewDesk().confirmBrief(17)
    await settle(page)

    expect(page.text()).not.toContain('Problem brief ready')
    /* Still five rows: #412's next action is now the draft its session had written. */
    expect(page.text()).toContain('5 reviews need you')
    const row = page.findAll('.row').find(one => one.text().includes('#412'))!
    expect(row.text()).toContain('Draft ready')
    expect(row.text()).toContain('Read and sign off')
    expect(page.findAll('.row').filter(one => one.text().includes('#412'))).toHaveLength(1)
    /*
     * **And it still goes to the review, not to the draft.** Patrick, on tc-portal #9584:
     * "when we click a review, we want to go to the review, not to the draft." In a
     * page-per-step review the review starts at Understand, and the draft is one click
     * along the trail — which is also the click that walks past the alternatives and the
     * solution on the way.
     */
    expect(row.attributes('href')).toBe('/reviews/17/understand')
  })

  it('opens Understand, whatever the row is waiting for', async () => {
    const page = await shown(Inbox)

    /*
     * **Every row, whatever it is waiting for.** A brief to confirm, a draft to sign off
     * and an author's reply are three different steps, and all three land on Understand:
     * the trail is what says which step is waiting, and it says it about all seven at once
     * where a destination could only say it about one.
     */
    const brief = page.findAll('.row').find(one => one.text().includes('Problem brief ready'))!
    expect(brief.attributes('href')).toBe('/reviews/17/understand')

    const reply = page.findAll('.row').find(one => one.text().includes('The author replied'))
    expect(reply, 'the fixtures hold a review whose only waiting thing is a reply').toBeTruthy()
    expect(reply!.attributes('href')).toMatch(/^\/reviews\/\d+\/understand$/)

    /*
     * And no row anywhere goes anywhere else. Asserted over every row rather than as a
     * predicate that could pass on an empty list.
     */
    const everywhere = page
      .findAll('.row')
      .map(one => one.attributes('href'))
      .filter((href): href is string => href != null)
      .filter(href => href.startsWith('/reviews/'))
    expect(everywhere.length).toBeGreaterThan(2)
    for (const href of everywhere) expect(href).toMatch(/^\/reviews\/\d+\/understand$/)
  })

  it('leaves a way back from the two screens the trail reaches', async () => {
    /*
     * Steps 5 and 6 of the trail land on the one-page review and the draft, and neither
     * carries a trail of its own until DEV-878 rebuilds them. One link back each, so the
     * queue — which lands every row on Understand — does not lead somewhere you cannot
     * leave. The draft screen has carried one all along; only where it goes has moved.
     */
    for (const [component, route] of [
      [OneReview, '/reviews/17'],
      [DraftPreview, '/reviews/17/draft'],
    ] as const) {
      const screen = await shown(component, { route })
      const back = screen
        .findAll('a')
        .find(one => /^Back to (the review|Understand)$/.test(one.text()))
      expect(back, `${route} has no way back`).toBeTruthy()
      expect(back!.attributes('href')).toBe('/reviews/17/understand')
    }
  })

  it('sends every "the review" link to Understand, not to the findings', async () => {
    /*
     * The queue, the two screens' back links, the header nav and a rule's origin all
     * meant `/reviews/{id}` before DEV-885. Once "the review" means Understand, an item
     * still landing on the findings takes a reader somewhere the page it lands on calls
     * not-the-review — which is what the header's "Reviews" was doing.
     */
    const layout = await shown(Layout)
    const reviews = layout.findAll('nav a').find(one => one.text() === 'Reviews')
    expect(reviews!.attributes('href')).toMatch(/^\/reviews\/\d+\/understand$/)
  })

  it('keeps the figures the boards do not draw, rather than dropping them', async () => {
    const page = await shown(Inbox)
    /* `handled_without_you` and a waiting rule proposal both reach the screen. */
    expect(page.text()).toContain('Handled without you this week')
    expect(page.text()).toContain('9 description problems caught by the CI check')
    expect(page.text()).toContain('11 nits filed as 3 tickets')
    expect(page.text()).toContain('1 rule proposal waiting on you')
  })

  it('reads as new, not as broken, on a database with nothing in it', async () => {
    await resetDesk({
      reviews: [],
      drafts: [],
      rules: [],
      repos: [],
      own_prs: [],
      session_liveness: {},
    })
    const page = await shown(Inbox)
    const text = page.text()

    expect(text).toContain('Nothing here yet')
    expect(text).toContain('This database holds no reviews')
    /* And it says the one thing that fills it, with the field to do it from. */
    expect(text).toContain('review-desk')
    expect(page.find('input#start-empty').exists()).toBe(true)
    /* Not an error, and not a count of nought reviews. */
    expect(page.find('.notice--blocked').exists()).toBe(false)
    expect(text).not.toContain('0 reviews need you')
  })
})

describe('one review', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  async function open(id: number) {
    return shown(OneReview, { route: `/reviews/${id}` })
  }

  it('renders the seven layers', async () => {
    const page = await open(17)
    const layers = page.findAll('ol li').map(item => item.text())
    expect(layers).toHaveLength(7)
    expect(layers[0]).toContain('Lint')
    expect(layers[3]).toContain('Solution map')
    expect(layers[6]).toContain('Harvest')
  })

  it('renders the problem and hands its diagram to mermaid', async () => {
    const page = await open(17)
    expect(page.text()).toContain('a webhook delivery fails once and is never tried again')
    expect(page.find('figure').exists()).toBe(true)
  })

  /*
   * **Recorded text keeps the shape its session gave it.** A session writes a finding's
   * evidence in paragraphs and a brief's problem at length, and the page used to join the
   * evidence into a `·`-separated meta line and run both the full width of the card. Both
   * now carry `rd-recorded`, which is `white-space: pre-wrap` at about 70 characters — and
   * is a class rather than markup, because the text is a record. DEV-878 rebuilds this
   * page; this is so it reads in the meantime.
   */
  describe('text a session recorded', () => {
    it('gives a finding\'s evidence its own block, with its paragraphs kept', async () => {
      const page = await open(17)
      const evidence = page.findAll('.evidence').find(one => one.text().includes('PROVED by'))
      expect(evidence).toBeTruthy()
      expect(evidence!.classes()).toContain('rd-recorded')
      /* The blank lines between the session's three paragraphs survive to the DOM. */
      expect(evidence!.text()).toContain('\n\n')
      expect(evidence!.text()).toContain('Mechanism:')
      expect(evidence!.text()).toContain('Bounded:')
    })

    it('keeps the evidence out of the one-line facts beside it', async () => {
      const page = await open(17)
      const facts = page.findAll('.where').find(one => one.text().includes('sibling diff'))!
      expect(facts.text()).toContain('lens: sibling diff')
      expect(facts.text()).not.toContain('PROVED by')
    })

    it('holds the problem and its provenance to a readable measure', async () => {
      const page = await open(17)
      expect(page.find('.problem').classes()).toContain('rd-recorded')
      expect(page.find('.panel-head .rd-meta').classes()).toContain('rd-recorded')
    })

    it('is text, never markup', async () => {
      /*
       * The class exists so that a paragraph break needs no HTML to survive, which is why
       * a session's evidence can be interpolated. The rule itself is asserted in
       * `controls.nuxt.spec.ts`, where the stylesheet is read.
       */
      const page = await open(17)
      const evidence = page.findAll('.evidence').find(one => one.text().includes('PROVED by'))!
      expect(evidence.element.querySelector('p, br, strong')).toBeNull()
      expect(evidence.html()).toContain('PROVED by')
    })
  })

  it('marks the option that was built', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('A · Retry inside the request')
    expect(text).toContain('B · Queue with backoff')
    expect(text).toContain('C · Dead-letter and manual replay')
    expect(text).toContain('What they built')
    expect(text).toContain('Verdict:')
  })

  it('lists every finding with its severity and disposition', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('3 in the draft · 3 held back · 1 suppressed')
    expect(text).toContain('F1 · A retried delivery is sent without an idempotency key')
    expect(text).toContain('Blocker')
    expect(text).toContain('Question')
    expect(text).toContain('Suppressed')
    expect(text).toContain('jobs/DeliverWebhook.php:58')
    expect(text).toContain('cites R-09')
  })

  it('says what was not checked', async () => {
    const page = await open(17)
    expect(page.text()).toContain('Not checked')
    expect(page.text()).toContain('under a deploy restart was not run')
  })

  it('offers the brief confirmation while the brief is unconfirmed', async () => {
    const page = await open(17)
    expect(button(page, 'Confirm the problem')).toBeDefined()

    await click(page, 'Confirm the problem')

    expect(page.text()).toContain('Problem confirmed')
    expect(button(page, 'Confirm the problem')).toBeUndefined()
  })

  it('reads as done on a review whose brief was already confirmed', async () => {
    const page = await open(15)
    expect(page.text()).toContain('Problem confirmed')
    expect(button(page, 'Confirm the problem')).toBeUndefined()
  })

  /*
   * **The third state of layer 2.** A brief recorded as needing no confirmation is not
   * waiting on the developer and was not signed by them either, so the screen must say
   * something other than both of the two sentences above — and must not offer the button,
   * because there is nothing for it to do.
   */
  it('says confirmation was not required, and why, without offering the button', async () => {
    const page = await open(16)
    const text = page.text()

    expect(text).toContain('Confirmation not required')
    expect(text).toContain('61 changed lines, under this caller\'s 200-line threshold')
    expect(text).not.toContain('Problem confirmed')
    expect(text).not.toContain('Confirm the problem before anything is posted')
    expect(button(page, 'Confirm the problem')).toBeUndefined()

    /* And the brief itself is still on screen: the developer can read it. */
    expect(page.text()).toContain('The problem')
  })

  it('moves a finding between the draft and held back, with a reason', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await settle(page)

    const select = page.find('select#disposition-1')
    expect(page.find('label[for="disposition-1"]').exists()).toBe(true)
    await select.setValue('held_back')
    await page.find('input#reason-1').setValue('Covered by the ticket')
    await click(page, 'Save')

    expect(page.text()).toContain('F1 is now held back for a ticket')
    expect(page.text()).toContain('Covered by the ticket')
  })

  it('will not move a finding out of the draft with no reason, and keeps the panel open', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await settle(page)
    await page.find('select#disposition-1').setValue('suppressed')
    await page.find('input#reason-1').setValue('')
    await click(page, 'Save')

    expect(page.text()).toContain('Say why it is not going in the draft.')
    expect(page.text()).toContain('Accepted: a sentence saying why.')

    /*
     * The panel stays open so the developer can answer the refusal where they are. A
     * refused write changes nothing on the record, so nothing tells the row to close —
     * which is why the row watches the finding's *values* rather than its identity.
     */
    expect(page.find('select#disposition-1').exists()).toBe(true)
    expect(page.find('input#reason-1').exists()).toBe(true)
  })

  it('closes the panel once the record agrees, in sample mode as over HTTP', async () => {
    /*
     * The failure this pins: the row used to close on a *new finding object*, which HTTP
     * produces on every read and sample mode never did — so in the dev loop and in every
     * component test a successful move left the panel open for ever.
     */
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await settle(page)
    expect(page.find('select#disposition-1').exists()).toBe(true)

    await page.find('select#disposition-1').setValue('held_back')
    await page.find('input#reason-1').setValue('Covered by the ticket')
    await click(page, 'Save')

    expect(page.text()).toContain('F1 is now held back for a ticket')
    expect(page.find('select#disposition-1').exists()).toBe(false)
  })

  it('shows the author’s replies on a posted review', async () => {
    const page = await open(12)
    const text = page.text()
    expect(text).toContain('The author replied')
    expect(text).toContain('Rejected')
    expect(text).toContain('by the author')
    expect(text).toContain('Code changed after')
  })

  it('says the desk holds no such review, rather than drawing an empty one', async () => {
    const page = await shown(OneReview, { route: '/reviews/999' })
    expect(page.text()).toContain('Not on the desk')
    expect(button(page, 'Retry')).toBeDefined()
  })
})

describe('the draft preview', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  async function open(id: number) {
    return shown(DraftPreview, { route: `/reviews/${id}/draft` })
  }

  it('says that nothing has been posted', async () => {
    const page = await open(17)
    expect(page.text()).toContain('Nothing has been posted. This is what GitHub will show.')
  })

  it('renders the body, keeping the folded section folded', async () => {
    const page = await open(17)
    expect(page.text()).toContain('The queue-with-backoff approach fits the ticket')
    const fold = page.find('details')
    expect(fold.exists()).toBe(true)
    expect(fold.attributes('open')).toBeUndefined()
    expect(page.find('summary').text()).toContain('Proof')
  })

  it('hands the body’s mermaid fence to a diagram rather than a code block', async () => {
    const page = await open(17)
    expect(page.find('figure').exists()).toBe(true)
    expect(page.html()).not.toContain('class="language-mermaid"')
  })

  it('offers the three views as tabs', async () => {
    const page = await open(17)
    const tabs = page.findAll('[role="tab"]')
    expect(tabs.map(tab => tab.text())).toEqual(['Rendered', 'Markdown', 'Edit'])
    expect(tabs[0]!.attributes('aria-selected')).toBe('true')

    await tabs[1]!.trigger('click')
    expect(page.find('pre').text()).toContain('```mermaid')

    await tabs[2]!.trigger('click')
    expect(page.find('textarea#draft-body').exists()).toBe(true)
  })

  it('shows the checks, counted from the body as it stands', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Visible words')
    expect(text).toContain('of 150')
    expect(text).toContain('Findings in the body')
    expect(text).toContain('3 of 3')
    expect(text).toContain('Review lint')
    expect(text).toContain('Passes')
  })

  it('lists the developer’s edits and the findings held back', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Your edits to this draft')
    expect(text).toContain('Cut')
    expect(text).toContain('F6 · rename suggestion')
    expect(text).toContain('Regraded')
    expect(text).toContain('Held back · 3 findings')
    expect(text).toContain('F4 · Backoff schedule is repeated in two files')
  })

  it('records an edit and updates the checks without a reload', async () => {
    const page = await open(17)
    expect(page.text()).not.toContain('F3 · wording')

    await page.findAll('[role="tab"]')[2]!.trigger('click')
    const field = page.find('textarea#draft-body')
    await field.setValue(
      (field.element as HTMLTextAreaElement).value.replace(
        'I reverted the retry and',
        'Reverting the retry, and',
      ),
    )
    await click(page, 'Save the draft')

    expect(page.text()).toContain('1 edit recorded for the harvester')
    expect(page.text()).toContain('F3 · wording')
  })

  it('signs off with the chosen event, and will not sign off twice', async () => {
    const page = await open(17)
    await page.find('select#event').setValue('comment')
    await click(page, 'Sign off and post')

    expect(page.text()).toContain(
      'Signed off. Will be posted as "Comment" — waiting for the session to post it.',
    )
    expect(page.text()).toContain('The review session posts it under your name')
    expect(button(page, 'Sign off and post')!.attributes('disabled')).toBeDefined()
  })

  it('will not sign off a draft with unsaved changes', async () => {
    const page = await open(17)
    await page.findAll('[role="tab"]')[2]!.trigger('click')
    await page.find('textarea#draft-body').setValue('Replaced entirely.')
    await page.vm.$nextTick()

    expect(page.text()).toContain('Save or discard your changes before signing off.')
    expect(button(page, 'Sign off and post')!.attributes('disabled')).toBeDefined()
  })

  /*
   * DEV-870: "approve" is GitHub's word for one of three review kinds, and the screen's
   * word for what the developer does is "sign off". The two read as one thing when the
   * button said "Approve and post" above a control offering "Approve" — signing off a
   * comment review read as "approve as Comment".
   *
   * Asserted over the screen's own copy rather than over the strings that were changed: a
   * sentence added later with the wrong word in it is exactly what this is for, and a list
   * of known strings would not see it.
   *
   * **Two regions are subtracted, and both are deliberate.** The event control holds
   * GitHub's three names, which is the whole point. The preview pane holds the agent's
   * review body, which comes from the record and is not this screen's wording — a fixture
   * that said "approve once F1 is fixed" would otherwise fail here and send whoever reads
   * the failure to the wrong file.
   *
   * **It covers the `ready` draft, which is the state this screen is for.** The notice
   * shown when a write is refused quotes the server, which says "an approved draft is what
   * the review session reads and posts" (`crates/server/src/api.rs`); DEV-870 leaves the
   * server's own prose alone, so a state that shows it is out of this assertion's reach
   * rather than silently passing it.
   */
  it('uses "approve" nowhere in the draft page’s own copy but in the event control', async () => {
    const page = await open(17)
    const control = page.find('select#event')

    /* The control still offers GitHub's three, under a label that names what they are. */
    expect(control.text()).toContain('Approve')
    expect(control.text()).toContain('Request changes')
    expect(control.text()).toContain('Comment')
    expect(page.find('label[for="event"]').text()).toBe('Post as')

    const copy = page.text()
      .replace(control.text(), '')
      .replace(page.find('.pane').text(), '')
    expect(copy).not.toMatch(/approv/i)
    expect(copy).toContain('Sign off and post')
  })

  it('asks for a note before sending a draft back, and keeps it when refused', async () => {
    const page = await open(17)
    await click(page, 'Send back with a note')

    const field = page.find('textarea#send-back-note')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="send-back-note"]').exists()).toBe(true)

    await click(page, 'Send back')
    expect(page.text()).toContain('A draft sent back with no note teaches it nothing.')

    await field.setValue('Fold the proof and shorten F2.')
    await click(page, 'Send back')
    expect(page.text()).toContain('Sent back. The session has the note.')
    expect(page.text()).toContain('Fold the proof and shorten F2.')
  })

  it('keeps what the developer typed when a save is refused', async () => {
    /*
     * An approved draft cannot be edited — it is what the review session posts — and a
     * refusal must not take the text away with it. DEV-862 asks for this by name: a write
     * that is refused shows why and keeps what was typed.
     */
    const page = await open(15)
    await useReviewDesk().approveDraft(15, 'comment')
    await settle(page)

    await page.findAll('[role="tab"]')[2]!.trigger('click')
    const field = page.find('textarea#draft-body')
    await field.setValue('An edit the record will refuse.')
    await click(page, 'Save the draft')

    expect(page.text()).toContain('cannot be edited')
    expect((page.find('textarea#draft-body').element as HTMLTextAreaElement).value)
      .toBe('An edit the record will refuse.')
  })

  it('files the held-back findings as one ticket', async () => {
    const page = await open(17)
    await click(page, 'File as one ticket for an agent')
    expect(page.text()).toContain('3 findings queued as one ticket')
    expect(page.text()).toContain('Queued as one ticket')
  })

  it('says the desk holds no such draft when the session has written none', async () => {
    const page = await shown(DraftPreview, { route: '/reviews/18/draft' })
    expect(page.text()).toContain('Not on the desk')
    expect(button(page, 'Retry')).toBeDefined()
  })

  it('says the lint has not run, rather than drawing it as failing', async () => {
    /*
     * `lint` is null until the review-craft lint has run over the body. `DraftLint`
     * requires `passes` and `checked_at`, so there is no shape of it that means "not
     * measured" — which is why the object itself is nullable, and why a screen that
     * assumed otherwise would have shown an unmeasured draft as one that fails.
     *
     * Produced by replacing the fixture's lint rather than by finding a draft that happens
     * to have none: every fixture draft was written while the field was required.
     */
    const { drafts } = loadSource()
    drafts.find(item => item.draft.review_id === 15)!.lint = null
    await resetDesk({ drafts })

    const page = await open(15)
    expect(page.text()).toContain('The review lint has not run over this body')
    expect(page.text()).not.toContain('Review lint')
  })
})

describe('the rules', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /** The repository tab with this label, from the board's own filter. */
  function tab(page: VueWrapper, startsWith: string) {
    return page.findAll('[role="radio"]').find(one => one.text().startsWith(startsWith))
  }

  it('renders the one headline figure that has a source', async () => {
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('rules in force')
    expect(text).toContain('of at most 20 for a repository')
  })

  /*
   * **Three of the board's four figures are null on the server today** — the replay check
   * does not exist, nothing has harvested a posted review, and no review has a second round
   * to compare. The contract made every one of them nullable for exactly this reason, and
   * the fixtures only fill two, so both states are reachable here.
   */
  it('shows a figure with no measurement as words, with the reason, never as a nought', async () => {
    const page = await shown(Rules)
    expect(page.text()).toContain('same answer on a re-run')
    expect(page.text()).toContain('Not measured yet')
    const unknown = page.findAll('.unknown').map(one => one.attributes('title'))
    expect(unknown.some(title => title?.includes('Review Desk has no replay yet'))).toBe(true)
  })

  it('says it of all three when the server answers null for all three, as it does today', async () => {
    const summary = loadSource().summary
    await resetDesk({
      summary: {
        ...summary,
        replay_matched: null,
        replay_total: null,
        findings_kept: null,
        findings_total: null,
        findings_window_days: null,
        raised_twice_on_one_pr: null,
      },
    })
    const page = await shown(Rules)
    const text = page.text()

    /* Three unmeasured, and the one that is measured is still a number. */
    expect(page.findAll('.unknown')).toHaveLength(3)
    expect(text).toContain('rules in force')
    for (const reason of [
      'no posted review has been harvested',
      'Review Desk has no replay yet',
      'no review here has a second round',
    ]) {
      expect(page.findAll('.unknown').some(one => one.attributes('title')?.includes(reason)))
        .toBe(true)
    }
    /* And nowhere does an absent measurement read as nought. */
    expect(text).not.toContain('0 of 0')
  })

  it('shows a figure the fixtures do measure as the figure', async () => {
    const page = await shown(Rules)
    /* `findings_kept` is 31 of 40 in the fixtures, so it is a number rather than a shrug. */
    expect(page.text()).toContain('31 of 40')
    expect(page.text()).toContain('findings you kept')
  })

  it('renders the proposed, in-force and going-stale groups', async () => {
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('Proposed')
    expect(text).toContain('waiting on you')
    expect(text).toContain('Do not raise rename-for-clarity suggestions')
    expect(text).toContain('Why it is proposed.')
    expect(text).toContain('In force')
    expect(text).toContain('A new test must fail when the fix is reverted')
    expect(text).toContain('Prevents.')
    expect(text).toContain('Going stale')
    expect(text).toContain('Legacy importer calls need the tenant guard')
  })

  it('sets a rule’s statement in the display face and its key in the mono one', async () => {
    const page = await shown(Rules)
    const statement = page.findAll('.rd-display-statement')[0]!
    expect(statement.find('.rd-mono').text()).toMatch(/^R-\d+$/)
    expect(statement.text()).toContain('A new test must fail when the fix is reverted')
    /* And the headline figure is the display face too, which is what the board draws. */
    expect(page.find('.value').classes()).toContain('value')
  })

  it('names the findings a proposal came out of, each as a place to go and look', async () => {
    const page = await shown(Rules)
    const chips = page.findAll('.chip').map(chip => chip.text())
    expect(chips.length).toBeGreaterThan(0)
    expect(chips[0]).toMatch(/^F\d+$/)
  })

  /*
   * The four-stage bar. The dispatch brief listed it as having no source; `Rule.stage` is
   * the contract's enum and this is it being drawn from the record.
   */
  it('draws how far each rule has graduated, from its stage', async () => {
    const page = await shown(Rules)
    const bars = page
      .findAll('[role="img"]')
      .filter(bar => bar.attributes('aria-label')?.includes('of 4'))
    expect(bars.length).toBeGreaterThan(0)
    expect(bars[0]!.findAll('.seg')).toHaveLength(4)

    /* R-09 is `active`: two of four, in ink. */
    const active = bars.find(bar => bar.attributes('aria-label')?.startsWith('Repository rule'))!
    expect(active.attributes('aria-label')).toContain('stage 2 of 4')
    expect(active.findAll('.seg--done')).toHaveLength(2)
    expect(active.findAll('.seg--todo')).toHaveLength(2)

    /* R-03 is `ci`: all four, and in the accent, because no model is spending tokens. */
    const check = bars.find(bar => bar.attributes('aria-label')?.startsWith('CI check'))!
    expect(check.attributes('aria-label')).toContain('stage 4 of 4')
    expect(check.findAll('.seg--now')).toHaveLength(4)

    expect(page.text()).toContain(
      'The four bars are the stages a rule moves through: noticed, repository rule, lint '
      + 'check, CI check.',
    )
  })

  it('shows the counts the stage has, and leaves out the ones nothing measured', async () => {
    const page = await shown(Rules)
    const text = page.text()
    /* A rule a review applies: raised, kept, rejected. */
    expect(text).toContain('7 raised')
    expect(text).toContain('7 kept by you')
    expect(text).toContain('0 rejected')
    /* A rule that graduated: what the check caught instead. */
    expect(text).toContain('9 caught by the check')
    expect(text).toContain('CI check')
    expect(text).toContain('Repository rule · 1 exception')
  })

  it('says when a check has graduated and a model is still raising it', async () => {
    const page = await shown(Rules)
    expect(page.text()).toContain('1 raised in a review since')
    expect(page.text()).toContain('a model is still spending tokens on it')
  })

  it('reads the provenance line as one sentence, with no gaps before its separators', async () => {
    const page = await shown(Rules)
    const line = page.findAll('.provenance').map(one => one.text())
    expect(line.length).toBeGreaterThan(0)
    for (const said of line) {
      expect(said).not.toMatch(/\s{2,}/)
      expect(said).not.toMatch(/\s·\s*$/)
    }
  })

  it('has a repository filter that changes the list', async () => {
    const page = await shown(Rules)
    expect(page.text()).toContain('In force')
    expect(page.text()).toContain('A new test must fail when the fix is reverted')

    await tab(page, 'acme/billing-api')!.trigger('click')
    await settle(page)

    expect(page.text()).toContain('A cache bust names every key the permission reaches')
    expect(page.text()).not.toContain('Do not raise rename-for-clarity suggestions')
  })

  it('promotes a proposal into the in-force list', async () => {
    const page = await shown(Rules)
    await click(page, 'Make it a rule')
    expect(page.text()).toContain('R-22 promoted')
    expect(page.text()).not.toContain('Why it is proposed.')
  })

  it('asks why before dismissing a proposal', async () => {
    const page = await shown(Rules)
    await click(page, 'Dismiss, and say why')

    const field = page.find('input#dismiss-22')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="dismiss-22"]').text()).toBe('Why is this not a rule?')

    await field.setValue('A formatter should do this.')
    await click(page, 'Dismiss the proposal')
    expect(page.text()).toContain('R-22 dismissed')
  })

  it('asks why before retiring a stale rule', async () => {
    const page = await shown(Rules)
    await click(page, 'Retire it')
    expect(page.find('input#retire-5').exists()).toBe(true)

    await page.find('input#retire-5').setValue('The importer is gone in #415.')
    await click(page, 'Retire the rule')
    expect(page.text()).toContain('R-05 retired')
    expect(page.text()).not.toContain('Going stale')
  })

  it('keeps a stale rule for another sixty days', async () => {
    const page = await shown(Rules)
    await click(page, 'Keep 60 more days')
    expect(page.text()).toContain('R-05 kept for 60 days')
    expect(page.text()).not.toContain('Going stale')
  })

  it('keeps the chosen repository when a rule action refreshes the screen', async () => {
    /*
     * A write refreshes every open read, and `listRepos` hands back a freshly mapped array
     * each time. The watcher that picks a first repository on load used to re-run on that,
     * so pressing Promote while "All repositories" was selected moved the developer back to
     * the first repository — reading a different page of rules than the one they acted on.
     */
    const page = await shown(Rules)
    await tab(page, 'All repositories')!.trigger('click')
    await settle(page)
    expect(page.text()).toContain('A cache bust names every key the permission reaches')

    await click(page, 'Make it a rule')

    expect(tab(page, 'All repositories')!.attributes('aria-checked')).toBe('true')
    expect(page.text()).toContain('R-22 promoted')
  })

  it('reads as nothing learned yet when there are no rules at all', async () => {
    await resetDesk({ rules: [], repos: [] })
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('No rules yet')
    expect(text).toContain('A rule comes out of a finding you kept')
    expect(page.find('.notice--blocked').exists()).toBe(false)
  })
})

describe('every error code in the contract has its own state on screen', () => {
  /*
   * DEV-862 asks that no code be swallowed. `test/api.spec.ts` reaches all eight — the
   * contract's six, plus the two it cannot describe — and this is what each one becomes on
   * a screen. The three that mean different things to the developer read differently: one
   * is their machine, one is the record, one is what they typed.
   */
  const CODES: Array<[string, number, string, string]> = [
    ['store_unavailable', 500, 'the database could not be opened',
      'Review Desk could not read its database'],
    ['github_unavailable', 502, 'gh is not installed',
      'A read of GitHub could not be made'],
    ['not_found', 404, 'no review with id 1', 'Not on the desk'],
    ['conflict', 409, 'this draft is approved', 'The record is not in that state'],
    ['bad_request', 400, 'this operation needs a JSON body', 'That was refused'],
    ['invalid_value', 400, '"nonsense" is not a commit hash', 'That was refused'],
  ]

  beforeEach(() => {
    useDeskSource('api')
  })

  afterEach(async () => {
    vi.unstubAllGlobals()
    await resetDesk()
  })

  it.each(CODES)('%s reads as "%s"', async (code, status, message, heading) => {
    vi.stubGlobal('fetch', vi.fn(() =>
      Promise.resolve({
        ok: false,
        status,
        text: () => Promise.resolve(JSON.stringify({ code, message })),
      } as Response)))

    const page = await shown(Inbox)
    expect(page.text()).toContain(heading)
    /* The server's own words, never swallowed in favour of a heading. */
    expect(page.text()).toContain(message.replace(/^./, first => first.toUpperCase()))
    expect(button(page, 'Retry')).toBeDefined()
  })

  it('refuses to trust a code the contract does not declare', async () => {
    vi.stubGlobal('fetch', vi.fn(() =>
      Promise.resolve({
        ok: false,
        status: 418,
        text: () => Promise.resolve(JSON.stringify({ code: 'teapot', message: 'I am a teapot' })),
      } as Response)))

    const page = await shown(Inbox)
    expect(page.text()).toContain('Something else is answering')
    expect(page.text()).not.toContain('teapot')
  })
})

describe('a read that is still in flight', () => {
  /*
   * A read that never answers, which is what a slow one looks like for as long as it
   * lasts. Sample mode cannot be caught in this state — it resolves in a microtask, inside
   * the mount — so this reads the real API with a `fetch` that is still waiting.
   */
  let answer: (body: string) => void = () => {}

  beforeEach(() => {
    useDeskSource('api')
    vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>((resolve) => {
      answer = (body: string) =>
        resolve({ ok: true, status: 200, text: () => Promise.resolve(body) } as Response)
    })))
  })

  afterEach(async () => {
    vi.unstubAllGlobals()
    await resetDesk()
  })

  it('reserves the space and says what it is reading', async () => {
    const page = await shown(Inbox)

    expect(page.text()).toContain('Reading the queue…')
    expect(page.find('[role="status"][aria-live="polite"]').exists()).toBe(true)
    /*
     * The reserved height is the point: without it the content lands and everything below
     * it jumps, which DEV-862 calls out by name.
     */
    expect(page.find('.loading').exists()).toBe(true)

    answer(JSON.stringify({
      totals: { items: 0, repos: 0 },
      briefs_to_confirm: [],
      drafts_to_approve: [],
      author_replies: [],
      own_prs_without_reviewer: [],
      proposed_rules: [],
      in_progress: [],
      handled_without_you: { window_days: 7 },
    }))
    await settle(page)

    expect(page.text()).not.toContain('Reading the inbox…')
    expect(page.text()).toContain('Nothing here yet')
  })
})

describe('the server stops while a page is open', () => {
  /*
   * The one state sample mode cannot produce: it is always answering. So this reads the
   * real API with a `fetch` that fails, which is exactly what a stopped `review-desk serve`
   * looks like from the page.
   */
  let answering = false

  beforeEach(() => {
    useDeskSource('api')
    vi.stubGlobal('fetch', vi.fn(() => {
      if (!answering) return Promise.reject(new TypeError('Failed to fetch'))
      return Promise.resolve({
        ok: true,
        status: 200,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              totals: { items: 0, repos: 0 },
              briefs_to_confirm: [],
              drafts_to_approve: [],
              author_replies: [],
              own_prs_without_reviewer: [],
              proposed_rules: [],
              in_progress: [],
              handled_without_you: { window_days: 7 },
            }),
          ),
      } as Response)
    }))
  })

  afterEach(async () => {
    vi.unstubAllGlobals()
    answering = false
    await resetDesk()
  })

  it('says Review Desk is not answering, and names the command that starts it', async () => {
    const page = await shown(Inbox)
    const text = page.text()

    expect(text).toContain('Review Desk is not answering')
    expect(text).toContain('Nothing is listening on this address')
    expect(text).toContain('review-desk serve')
    /*
     * Not "the database is unreadable". That is `store_unavailable`, and it means Review
     * Desk is broken — sending the developer to debug a database when they have simply not
     * started the server.
     */
    expect(text).not.toContain('could not read its database')
    /* And not a blank page. */
    expect(page.find('.notice--blocked').exists()).toBe(true)
  })

  it('offers invented records to look at, since the binary is not running', async () => {
    /*
     * Only useful at this moment, and only in a development build: a developer who has not
     * started the binary can still see a screen. A built dashboard has no fixtures to
     * switch to, so the control is compiled out with them.
     */
    const page = await shown(Inbox)
    expect(button(page, 'Use sample records')).toBeDefined()
  })

  it('offers a retry that works once the server is back', async () => {
    const page = await shown(Inbox)
    expect(button(page, 'Retry')).toBeDefined()

    /* The developer starts the server, then clicks the retry. */
    answering = true
    await click(page, 'Retry')

    expect(page.text()).not.toContain('Review Desk is not answering')
    expect(page.text()).toContain('Nothing here yet')
  })
})
