// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import {
  resetDesk,
  useDeskSource,
  useReviewDesk,
} from '../app/composables/useReviewDesk'
import { currentSource, loadSource } from '../app/data/sample'

import Inbox from '../app/pages/index.vue'
import Reviews from '../app/pages/reviews/index.vue'
import OneReview from '../app/pages/reviews/[id]/index.vue'
import DraftPreview from '../app/pages/reviews/[id]/draft.vue'
import Rules from '../app/pages/rules.vue'
import Layout from '../app/layouts/default.vue'

/*
 * The five screens, mounted. These check that each one renders from the data layer and
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

  it('keeps its three places, and all three are links, on a database with nothing in it', async () => {
    await resetDesk({ reviews: [], drafts: [], rules: [] })
    const page = await shown(Layout)
    /*
     * **This asserted the opposite until DEV-930**, and the change is the point. "Reviews"
     * used to go to whichever review happened to be waiting, so on a fresh database there
     * was no review to point at and the item was a disabled label. It is a list now, and a
     * list is somewhere worth being on an empty database — it says the database is empty.
     */
    expect(page.findAll('nav a').map(link => link.text()))
      .toEqual(['Queue', 'Reviews', 'Rules'])
    expect(page.find('nav span[aria-disabled="true"]').exists()).toBe(false)
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

  it('leads with how many pull requests need the developer, counting each once', async () => {
    const page = await shown(Inbox)
    /*
     * **The heading counts the rows it draws, and the assertion is that identity rather
     * than a number.** DEV-957's comment: "The queue heading counts pull requests, not
     * rounds" — Patrick's screen read "12 reviews need you" over a list naming #9541
     * twice, #9585 twice and #9568 at least twice. Taken from the rows, the claim survives
     * a fixture being added, which is what a literal here does not: this number moved
     * three times in two issues.
     */
    const drawn = needsYouRows(page)
    expect(drawn.length, 'the Needs you group has rows').toBeGreaterThan(0)
    expect(page.text()).toContain(
      drawn.length === 1 ? '1 pull request needs you' : `${drawn.length} pull requests need you`,
    )
    expect(page.text()).toContain('2 with the agent')
    expect(page.text()).toContain('2 of your own')

    /*
     * **And each pull request once**, which is the half the number alone cannot say: five
     * rows and five pull requests is the same count as five rows and three pull requests.
     * #412 is the one that earns it — a brief to confirm *and* a draft to approve.
     */
    const numbers = drawn.map(row => row.text().match(/#(\d+)/)?.[1])
    expect(numbers.filter(Boolean)).toHaveLength(drawn.length)
    expect(new Set(numbers).size, 'no pull request twice').toBe(numbers.length)
    expect(numbers).toContain('412')
  })

  it('draws one row for a pull request with two unposted rounds', async () => {
    /*
     * **`globex/portal#9541`, which is Patrick's own record from DEV-957's comment**: rounds 1
     * and 2, both "Draft ready", two rows. Round 1's draft can never be the review that
     * posts, so the store no longer offers it and one row is left — round 2's, which is
     * where its link goes.
     */
    const page = await shown(Inbox)
    const rows = needsYouRows(page).filter(row => row.text().includes('#9541'))
    expect(rows, 'one row for #9541').toHaveLength(1)
    expect(rows[0]!.text()).toContain('round 2')
    expect(rows[0]!.find('.row').attributes('href')).toBe('/reviews/61/understand')
  })

  it('draws one row for a pull request whose posted round was replied to', async () => {
    /*
     * **`globex/portal#9585`**, which drew two rows: its posted round 1, where the author had
     * replied, and its round 2 draft. One row now, numbered for round 2 — and the reply is
     * still on it, with the round it was left on, because the author answering is a real
     * thing the developer has to read and the row it is drawn on is numbered differently.
     */
    const page = await shown(Inbox)
    const rows = needsYouRows(page).filter(row => row.text().includes('#9585'))
    expect(rows, 'one row for #9585').toHaveLength(1)
    expect(rows[0]!.text()).toContain('round 2')
    expect(rows[0]!.text()).toContain('the author has replied to round 1')
  })

  it('keeps the newest round’s reply when two rounds have author activity', async () => {
    /*
     * **The one inbox query with no newest-round restriction**, deliberately — a reply on a
     * posted round is a real thing the developer has to read — so a pull request can be in
     * `author_replies` twice. The merge took whichever arrived last, which the store serves
     * newest id *first*, so the row held the **older** round's events and, through the row's
     * age, the older round's time. Nothing said the newer one had been dropped. Found by a
     * review of this branch.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        /* An outcome on round 2 of #9585, beside the one its round 1 already carries. */
        if (detail.review.id !== 63) return detail
        return {
          ...detail,
          outcomes: [{
            id: 9100,
            review_id: 63,
            finding_seq: 1,
            finding_key: 'F1',
            kind: 'code_changed_after',
            actor: 'harvester',
            occurred_at: '2026-10-13T06:00:00Z',
            detail: null,
          }],
        }
      }) as typeof source.reviews,
    })

    const page = await shown(Inbox)
    const row = needsYouRows(page).find(one => one.text().includes('#9585'))
    expect(row, 'one row for #9585').toBeDefined()
    /*
     * Round 2 is the row's own round, so its reply is the row's reply — and the row says
     * nothing about an earlier round, because the reply it holds is this round's.
     */
    expect(row!.text()).toContain('round 2')
    expect(row!.text()).not.toContain('replied to round 1')
  })

  it('says the newest round’s session on a row a session is working two rounds of', async () => {
    /*
     * **Which round's session the row describes.** `reviews_in_progress` has no newest-round
     * restriction and serves newest id first, so joining it with a plain assignment left the
     * row holding the *oldest* round's item — and the row then printed "a session is on step
     * 4 of 7" about round 1 under a number saying round 2. Keyed on the review it could not
     * happen; keying on the pull request is what made it possible. Found by a review of this
     * branch.
     */
    const source = loadSource()
    const running = (id: number, number: number, detail: string) => ({
      number,
      name: number === 4 ? ('solution_map' as const) : ('code_review' as const),
      state: 'running' as const,
      detail,
      updated_at: '2026-10-13T06:00:00Z',
      review_id: id,
    })
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        /* Round 1 on step 4, round 2 on step 5 — two different sessions, two steps. */
        if (detail.review.id === 60) {
          return { ...detail, layers: detail.layers.map(layer =>
            layer.number === 4 ? { ...layer, ...running(60, 4, 'solution map') } : layer) }
        }
        if (detail.review.id === 61) {
          return { ...detail, layers: detail.layers.map(layer =>
            layer.number === 5 ? { ...layer, ...running(61, 5, 'code review') } : layer) }
        }
        return detail
      }) as typeof source.reviews,
    })

    const page = await shown(Inbox)
    const row = needsYouRows(page).find(one => one.text().includes('#9541'))
    expect(row, 'one row for #9541').toBeDefined()
    expect(row!.text(), 'the round the row is about').toContain('round 2')
    expect(row!.text(), 'and that round’s session').toContain('step 5 of 7')
    expect(row!.text(), 'not the older round’s').not.toContain('step 4 of 7')
  })

  it('says nothing about a session on a round the row is not about', async () => {
    /*
     * **The other direction, and the one the first fix for this could not answer.** It took
     * the highest review id, which is a guess at the newest round; a session running on an
     * *older* round only — round 2's own work finished, round 1's never did — would still
     * have been drawn on round 2's row. `InboxInProgressItem` carries a round now, so the
     * row says nothing rather than something about the wrong round. Found by a review of
     * this branch.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        if (detail.review.id !== 60) return detail
        return { ...detail, layers: detail.layers.map(layer =>
          layer.number === 4
            ? { ...layer, state: 'running' as const, detail: 'solution map' }
            : layer) }
      }) as typeof source.reviews,
    })

    const page = await shown(Inbox)
    const row = needsYouRows(page).find(one => one.text().includes('#9541'))
    expect(row, 'one row for #9541').toBeDefined()
    expect(row!.text()).toContain('round 2')
    expect(row!.text(), 'round 1’s session is not this row’s').not.toContain('step 4 of 7')
  })

  it('draws one row for a pull request a session is working two rounds of', async () => {
    /*
     * `reviews_in_progress` asks only whether a layer is `running` and says nothing about
     * which round — the group's own comment says so — so two rounds of one pull request can
     * both be in it. "With the agent" drew them as two rows and its note counted rounds,
     * which is the defect this issue names, in the one group the needs-you merge does not
     * reach. Found by a review of this branch.
     */
    const source = loadSource()
    const running = (id: number) => ({
      number: 5,
      name: 'code_review' as const,
      state: 'running' as const,
      detail: 'code review',
      updated_at: '2026-10-13T06:00:00Z',
      review_id: id,
    })
    const twoRounds = [60, 61]
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        /* Rounds 1 and 2 of #9541, each with a layer running. */
        if (!twoRounds.includes(detail.review.id)) return detail
        return {
          ...detail,
          brief: null,
          draft_status: null,
          layers: detail.layers.map(layer =>
            layer.number === 5 ? { ...layer, ...running(detail.review.id) } : layer),
        }
      }) as typeof source.reviews,
      /*
       * **And no draft on either**, or they are waiting on the developer and the merge
       * above subtracts them from this group before it can be measured. What is being
       * measured is the group's own deduplication.
       */
      drafts: source.drafts.filter(one => !twoRounds.includes(one.draft.review_id)),
    })

    const page = await shown(Inbox)
    const group = page.findAll('section').find(one => one.text().startsWith('With the agent'))
    expect(group, 'the With the agent group is on the page').toBeDefined()
    expect(
      group!.findAll('.queue-line').filter(one => one.text().includes('#9541')),
      'one row for two rounds',
    ).toHaveLength(1)
  })

  it('takes the row’s ticket from the newest round that offered one', async () => {
    /*
     * **The guard's own blind spot, found by a review of the commit that added it.** It
     * asked whether the *row's* round differed, and `row.round` is set only by
     * `drafts_to_approve` — so a row made by a brief has none, the guard never fired, and an
     * older round's reply decided the ticket. The question is which round the ticket came
     * from.
     *
     * The shape: #9585's round 2 has an unconfirmed brief (so it is in `briefs_to_confirm`,
     * which carries no round) and round 1 is posted with the author's reply on it. The two
     * rounds are given different tickets here, which is what makes the fault visible — on
     * the real record they are usually the same, which is why it was invisible.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        if (detail.review.id === 62) {
          /* Round 1: posted, replied to, and carrying the *old* ticket. */
          return { ...detail, review: {
            ...detail.review,
            ticket_id: 'DEV-OLD',
            ticket_url: 'https://linear.app/deverge/issue/DEV-OLD',
          } }
        }
        if (detail.review.id === 63) {
          /* Round 2: the row's own round, with a brief waiting and the current ticket. */
          return {
            ...detail,
            review: { ...detail.review, ticket_id: 'DEV-NEW' },
            brief: {
              review_id: 63,
              problem: 'A guest session survives the redirect to the staff page.',
              diagram: null,
              options: [],
              approach_verdict: null,
              provenance: 'from the ticket and the code',
              confirmed: false,
              confirmed_at: null,
              confirmation_required: true,
              confirmation_not_required_because: null,
              updated_at: '2026-10-12T16:10:00Z',
            },
          }
        }
        return detail
      }) as typeof source.reviews,
      /* No draft on round 2, so the row's own round is unknown — the case that failed. */
      drafts: source.drafts.filter(one => one.draft.review_id !== 63),
    })

    const page = await shown(Inbox)
    const row = needsYouRows(page).find(one => one.text().includes('#9585'))
    expect(row, 'one row for #9585').toBeDefined()
    expect(row!.text(), 'the newer round’s ticket').toContain('DEV-NEW')
    expect(row!.text(), 'and not the posted round’s').not.toContain('DEV-OLD')
  })

  it('drops the author’s figures rather than showing a newer round’s on an older row', async () => {
    /*
     * **The join that could attach the wrong round's measurement.**
     * `posted_awaiting_author` serves a pull request's newest round only, which rules out
     * attaching an older round's posting to a newer row — and not the reverse. A row's
     * round comes from the three slotting arrays, and `author_replies` has no newest-round
     * restriction, so a pull request whose round 1 was posted and replied to and whose
     * round 2 is posted with nothing back yet draws one row numbered round 1 and took round
     * 2's counts. The row then read "The author replied to F1 · No reply yet". Found by a
     * review of this branch.
     *
     * Dropping the figures is the right failure: a measurement about the wrong round is
     * worse than none.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        /* Round 2 of #9585: posted, nothing back, and nothing waiting on the developer. */
        if (detail.review.id !== 63) return detail
        return {
          ...detail,
          review: { ...detail.review, github_review_id: 'PRR_globex9585r2' },
          brief: null,
        }
      }) as typeof source.reviews,
      /* No draft on round 2, so only round 1's reply can give the row its round. */
      drafts: source.drafts.filter(one => one.draft.review_id !== 63),
    })

    const page = await shown(Inbox)
    const row = needsYouRows(page).find(one => one.text().includes('#9585'))
    expect(row, 'one row for #9585').toBeDefined()
    expect(row!.text(), 'the round the reply is on').toContain('round 1')
    expect(row!.text()).toContain('The author replied to')
    expect(row!.text(), 'and not a newer round’s measurement').not.toContain('No reply yet')
  })

  it('draws no row for any round of a merged pull request', async () => {
    /*
     * **The four rounds of `globex/portal#9568`**, one posted and three not, on a pull request
     * that merged on 11 October. Five of the ten drafts in Patrick's "drafts to approve"
     * were rounds like these, and none of them could ever be posted.
     */
    const page = await shown(Inbox)
    expect(page.text(), 'no group draws a merged pull request').not.toContain('#9568')
  })

  it('draws the board’s five groups, and hides a group with nothing in it', async () => {
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
   * **The group the board drew and nothing could fill.** It said "Not measured yet" with
   * the reason beside it until DEV-930 gave `GET /inbox` a group for a posted review whose
   * pull request is still open. The words are gone because the absence is.
   */
  it('lists the posted reviews waiting on their author, and no longer says they are unmeasured', async () => {
    const page = await shown(Inbox)
    const text = page.text()
    expect(text).toContain('Waiting on the author')
    expect(text).not.toContain('Not measured yet')
    expect(text).not.toContain('serves no group for a posted review awaiting a reply')

    /* Review 43: posted, pull request open, and nothing has read it back. */
    const row = page.findAll('.row').find(one => one.text().includes('#403'))!
    expect(row, 'the posted review has a row').toBeDefined()
    expect(row.text()).toContain('Posted as Comment')
    expect(row.text()).toContain('Not harvested yet')
    expect(row.text()).toContain('pull request state not established')
    expect(row.attributes('href')).toBe('/reviews/43/understand')
  })

  /*
   * **One row per review, across groups as well as within them.** Review 12 is posted with
   * its pull request open, so `GET /inbox` serves it in the posted group — and its author
   * has replied, so it is also under "Needs you". Drawing it in both is the defect DEV-876
   * exists to prevent, so the group leaves it out and says it did.
   */
  it('draws a posted review whose author has replied once, under Needs you', async () => {
    const page = await shown(Inbox)
    const forThreeNineEight = page.findAll('.row').filter(row => row.text().includes('#398'))
    expect(forThreeNineEight).toHaveLength(1)
    expect(forThreeNineEight[0]!.text()).toContain('The author replied')

    /*
     * And the group says where it went, so its count cannot read as the whole truth.
     * "drawn above" rather than a named group: the subtraction also removes reviews a
     * session is working on, which is not something the author has done.
     */
    expect(page.text()).toContain('1 more is posted and is already drawn above')
  })

  /*
   * And the figures about the author's response are on the row that is *about* the author's
   * response. They would otherwise be drawn nowhere: a review only leaves the posted group
   * when something has come back on it, which is exactly when the figures are non-zero.
   */
  it('puts what came back on the row that says the author replied', async () => {
    const page = await shown(Inbox)
    const row = page.findAll('.row').find(one => one.text().includes('#398'))!
    expect(row.text()).toContain('2 replies')
    expect(row.text()).toContain('1 thread resolved')
    expect(row.text()).toContain('1 finding with code changed since')
  })

  /**
   * **The row's button says what the outcome was, not always "Read the reply".**
   *
   * The group is built from outcomes by the author *or the harvester*, and `events[0]` is
   * whichever is first by finding number then time — so a `code_changed_after` heads the
   * row whenever the author pushed a fix and wrote nothing. The heading was worded from the
   * kind and the button was not, so the row read "The code under F1 changed · Read the
   * reply", offering an act that did not happen.
   */
  it('offers what the outcome really was, not a reply that was never written', async () => {
    const source = loadSource()
    const twelve = source.reviews.find(one => one.review.id === 12)!
    const changed = twelve.outcomes.find(one => one.kind === 'code_changed_after')!
    await resetDesk({
      reviews: source.reviews.map(one =>
        one.review.id === 12 ? { ...one, outcomes: [changed] } : one),
    })

    const page = await shown(Inbox)
    const row = page.findAll('.row').find(one => one.text().includes('#398'))!
    expect(row.text()).toContain('The code under F1 changed')
    expect(row.text()).toContain('See what changed')
    expect(row.text()).not.toContain('Read the reply')
  })

  /*
   * **The same rule, one group earlier.** Review 12 is posted, its author has replied, and
   * a session is back on its code review step working those replies — so `GET /inbox`
   * serves it in `author_replies` *and* in `in_progress`. One row, under "Needs you", with
   * what the other group would have said about the session on it; and "With the agent"
   * says it held one back rather than quietly counting one fewer.
   *
   * **It was the harvest step that put it in both groups until DEV-933.** A harvest now
   * sets step 7 `running` for as long as the pull request stays open, and
   * `reviews_in_progress` leaves step 7 out for that reason — otherwise every posted
   * review would sit under "with the agent" for weeks. So the shape this rule needs is a
   * session on a step of its own, which is the step a review goes back to when the author
   * has replied.
   */
  it('draws a review that needs you and has a session running once, saying both', async () => {
    const page = await shown(Inbox)
    const rows = page.findAll('.row').filter(row => row.text().includes('#398'))
    expect(rows).toHaveLength(1)
    expect(rows[0]!.text()).toContain('The author replied')
    expect(rows[0]!.text()).toContain('a session is on step 5 of 7')
    expect(page.text()).toContain('1 more is with the agent and is already drawn above')
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

  it('says what it knows about each session, and how long the record has been silent', async () => {
    // **"Session state unknown" was what this row said until DEV-890**, on every review,
    // because Allele owns session liveness and tells Review Desk nothing. What the record
    // holds is when anything last wrote to the review, and that is what the row says now
    // — while an answer from Allele, when there is one, still outranks it.
    const page = await shown(Inbox)
    expect(page.text()).toContain('Session running')
    expect(page.text()).toContain('Last activity')
    expect(page.text()).not.toContain('Session state unknown')
  })

  it('says how long each review has been in the state it is in', async () => {
    const page = await shown(Inbox)
    /* Beside the step the session is on: "Code review · 2 days 1 hr". */
    expect(page.text()).toMatch(/Code review\s+·\s+\d/)
  })

  it('says a row has no recorded time rather than inventing one', async () => {
    const page = await shown(Inbox)
    /*
     * A pull request nothing has dated. The three rows that said this before DEV-890 — a
     * draft waiting, an author's reply and a running session — have a time now, and that
     * is asserted above; what is asserted here is that a row with nothing still says so
     * rather than drawing a nought.
     */
    expect(page.text()).toContain('no time recorded')
    expect(page.text()).toContain('and the rows with no recorded time last')
  })

  it('filters by repository, and each tab carries the rows it would show', async () => {
    const page = await shown(Inbox)
    const tabs = page.findAll('[role="radiogroup"] [role="radio"]').map(tab => tab.text())
    expect(tabs[0]).toBe('All repositories')
    /*
     * Seven on portal, not six: review 43 is posted with its pull request open, so it is a
     * row under "Waiting on the author" and the chip counts it. A chip whose number does
     * not match the rows pressing it shows is worse than no chip.
     */
    expect(tabs).toContain('acme/portal· 7')
    expect(tabs).toContain('acme/billing-api· 3')

    const billing = page
      .findAll('[role="radio"]')
      .find(tab => tab.text().startsWith('acme/billing-api'))!
    await billing.trigger('click')
    await settle(page)

    expect(page.text()).toContain('1 pull request needs you')
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
    /*
     * **Found by what it says, not by being the first GitHub link on the page.** It was
     * the first until DEV-936 put a "GitHub" link on every row above it, and "the first
     * one" was never what this test is about: the claim is that *this* row's own
     * destination is the pull request, which is what its action cell says out loud.
     */
    const out = page
      .findAll('a')
      .find(link => link.text().includes('Open on GitHub'))
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
    const before = needsYouRows(page).length
    await useReviewDesk().confirmBrief(17)
    await settle(page)

    expect(page.text()).not.toContain('Problem brief ready')
    /*
     * **The same rows as before, which is the claim** — #412's next action is now the draft
     * its session had written, so the row stays and only its words change. Read from before
     * and after rather than written as a number, for the reason the heading test gives.
     */
    expect(needsYouRows(page)).toHaveLength(before)
    const row = page.findAll('.row').find(one => one.text().includes('#412'))!
    expect(row.text()).toContain('Draft ready')
    expect(row.text()).toContain('Read and sign off')
    expect(page.findAll('.row').filter(one => one.text().includes('#412'))).toHaveLength(1)
    /*
     * **And it still goes to the review, not to the draft.** Patrick, on a review:
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

  it('leaves a way back from the draft, which is the trail itself', async () => {
    /*
     * **The back link is gone, because the trail replaced it.** It existed because steps 5
     * and 6 landed on two screens that carried no trail of their own, so the queue — which
     * lands every row on Understand — would otherwise have led somewhere you could not
     * leave. Both of those screens are inside the shell now: the draft is step 6 and the
     * one-page review is a redirect. A link saying "back to the review" beside a trail
     * whose first step is the review would be the same door twice.
     */
    const screen = await shown(DraftPreview, { route: '/reviews/17/draft' })
    const trail = screen.findAll('.trail a').map(one => one.attributes('href'))
    expect(trail, 'the draft carries the trail').toContain('/reviews/17/understand')
    expect(trail).toContain('/reviews/17/code-review')
    expect(
      screen.findAll('a').some(one => /^Back to /.test(one.text())),
      'and no back link beside it',
    ).toBe(false)

    /*
     * **And DEV-936's first criterion on step 6**, which names each of the six pages. The
     * header is `ReviewStepPage`'s and is tested there; each page a spec already mounts
     * says it for itself rather than inheriting the claim from the component. Review 17
     * holds a ticket address, so both links are here.
     */
    const out = screen.findAll('.where a.header-out').map(one => one.attributes('href'))
    expect(out).toEqual([
      'https://github.com/acme/portal/pull/412',
      'https://linear.app/acme/issue/ABC-231',
    ])
  })

  it('sends the header’s Reviews item to the list, and every row from it to Understand', async () => {
    /*
     * **The nav item changed kind in DEV-930 and the rule it was asserting did not.** It
     * used to point at whichever review was waiting, so the thing being checked was that
     * it landed on Understand rather than on the findings — DEV-885's rule. It points at
     * the list now, and the rule moves to the list's rows: a review is still read from
     * Understand, which is where a review starts.
     */
    const layout = await shown(Layout)
    const reviews = layout.findAll('nav a').find(one => one.text() === 'Reviews')
    expect(reviews!.attributes('href')).toBe('/reviews')

    const page = await shown(Reviews)
    /*
     * **Scoped to a queue row's wrapper**, not every `.row` on the page. `.row` belongs to
     * `QueueRow`, `OpenRow`, `DeskTable`, `FlowDiagram` and two pages, and this assertion
     * reads `href` off every match — so the first non-anchor `.row` to reach this screen
     * would throw for a reason with nothing to do with the rule being asserted. The same
     * unscoped-shared-selector shape as the `.line` rename in `QueueRow.vue`, which that
     * component's comment states as a rule; this one fails loudly rather than passing
     * wrongly, which is why it is a narrower problem and still worth not having.
     */
    const rows = page.findAll('.queue-line .row')
    expect(rows.length).toBeGreaterThan(0)
    for (const row of rows) {
      expect(row.attributes('href')).toMatch(/^\/reviews\/\d+\/understand$/)
    }
  })

  /**
   * The rows of the "Needs you" group, and only that group's.
   *
   * `page.findAll('.queue-line')` is every row on the screen — needs-you, with the agent,
   * waiting on the author, and the developer's own pull requests — so a count taken from it
   * would answer a different question from the heading's. The group's own `DeskSection` is
   * the scope.
   */
  function needsYouRows(page: VueWrapper) {
    const group = page.findAll('section').find(one => one.text().startsWith('Needs you'))
    expect(group, 'the Needs you group is on the page').toBeDefined()
    return group!.findAll('.queue-line')
  }

  /* ------------- The two ways out of a row (DEV-936) ------------- */

  /**
   * The links beside a row, and the row they are beside.
   *
   * Found by the row's wrapper rather than by `.row`, because the point of the assertion
   * below is that they are **not** inside the row. `.queue-line` and not `.line`: two other
   * components have a `.line`, and this locator is not scoped to one component.
   */
  function lines(page: VueWrapper) {
    return page.findAll('.queue-line').map(line => ({
      row: line.find('.row'),
      /*
       * **The repository line, which left the anchor in DEV-957.** The links moved into it
       * — "it belongs inline with the repository and ticket line under the title" — and the
       * line had to leave `.row` to take them: an anchor inside an anchor is the thing the
       * component's own note forbids. So a test asking what the row *says* about its
       * repository, its round or its ticket asks this, and one asking where the row *goes*
       * asks `row`.
       */
      meta: line.find('.row-meta'),
      out: line.findAll('.row-out-link'),
      /* Everything on the line, for an assertion that does not care which half said it. */
      text: () => line.text(),
    }))
  }

  it('carries a pull request link and a ticket link on a queue row', async () => {
    const page = await shown(Inbox)
    /* Review 17 is `acme/portal#412`, ticket ABC-231, and the record holds its address. */
    const first = lines(page).find(line => line.row.text().includes('#412'))
    expect(first, 'the needs-you row for #412').toBeDefined()

    const [pull, ticket] = first!.out
    expect(pull!.attributes('href')).toBe('https://github.com/acme/portal/pull/412')
    expect(pull!.text()).toContain('GitHub')
    expect(ticket!.attributes('href')).toBe('https://linear.app/acme/issue/ABC-231')
    expect(ticket!.text()).toContain('ABC-231')

    for (const link of [pull!, ticket!]) {
      expect(link.attributes('target')).toBe('_blank')
      expect(link.attributes('rel')).toBe('noopener noreferrer')
      expect(link.find('.rd-visually-hidden').text()).toContain('opens in a new tab')
    }
  })

  it('keeps the ticket link on a row whose brief has been confirmed', async () => {
    /*
     * **The defect this is a regression test for.** The ticket and its address were served
     * on `briefs_to_confirm` alone, and the queue reads one row per review — so the moment
     * review 17's brief was confirmed, its row moved to being about the draft, the brief
     * item went away, and the ticket link went with it. The row was still there, the
     * address was still on the record, and the Reviews list still showed the link for the
     * same review. All three arrays that make a row carry the pair now.
     */
    const page = await shown(Inbox)
    const ticketOn = (where: VueWrapper) =>
      lines(where)
        .find(line => line.row.text().includes('#412'))
        ?.out.map(link => link.attributes('href')) ?? []

    expect(ticketOn(page)).toContain('https://linear.app/acme/issue/ABC-231')

    await useReviewDesk().confirmBrief(17)
    await settle(page)

    /* The row is still here and is now about the draft. */
    const row = lines(page).find(line => line.row.text().includes('#412'))
    expect(row!.row.text()).toContain('Draft ready')
    expect(ticketOn(page)).toEqual([
      'https://github.com/acme/portal/pull/412',
      'https://linear.app/acme/issue/ABC-231',
    ])
  })

  it('puts a row’s links beside the row’s own link, never inside it', async () => {
    /*
     * **The whole criterion, and it holds by construction rather than by a handler.** An
     * `<a>` inside an `<a>` is not something a browser will draw — the parser closes the
     * first — and a click handler that stopped the event would be a thing that can be got
     * wrong and a thing a keyboard can get round. So the assertion is structural: no link
     * out is a descendant of the link to the review.
     */
    for (const page of [await shown(Inbox), await shown(Reviews)]) {
      const withLinks = lines(page).filter(line => line.out.length > 0)
      expect(withLinks.length).toBeGreaterThan(0)

      for (const line of withLinks) {
        for (const link of line.out) {
          expect(
            line.row.element.contains(link.element),
            'a link out is not inside the row',
          ).toBe(false)
        }
        /* And the row still goes to the review, which is what it is for. */
        expect(line.row.attributes('href') ?? line.row.attributes('to') ?? '')
          .toMatch(/\/reviews\/\d+\//)
      }
    }
  })

  it('carries the same two links on a Reviews list row', async () => {
    const page = await shown(Reviews)
    const first = lines(page).find(line => line.row.text().includes('#412'))
    expect(first, 'the list row for #412').toBeDefined()

    expect(first!.out.map(link => link.attributes('href'))).toEqual([
      'https://github.com/acme/portal/pull/412',
      'https://linear.app/acme/issue/ABC-231',
    ])
  })

  it('carries the two links on a row no developer act is waiting on', async () => {
    /*
     * **The gap this closed, and the test that did not catch it.** The pair was served only
     * on the three arrays a needs-you row is built from, so a review in *none* of them — a
     * session working on it, or an author who has not answered — drew a row with a link to
     * its pull request and none to its ticket, while the Reviews list linked both for the
     * same review.
     *
     * The first version of this test named review 16, "a session running on it" — and
     * review 16 has no `running` layer at all. It is in `drafts_to_approve`, so the test
     * measured a needs-you row that had carried the pair for several commits and would have
     * passed with both new bindings deleted. Reviews **18** and **19** are the ones with a
     * running layer, and 43 is the posted one; the fixtures gave all three a null address,
     * so there was nothing to draw either. Both halves are fixed here: 18 and 43 have
     * addresses, and the rows asserted are theirs.
     */
    const page = await shown(Inbox)

    /* Review 18 is `acme/portal#409`, step 5 running, in no needs-you array. */
    const agent = lines(page).find(line => line.row.text().includes('#409'))
    expect(agent, 'the With the agent row for #409').toBeDefined()
    expect(agent!.row.text()).toContain('Code review')
    expect(agent!.out.map(link => link.attributes('href'))).toEqual([
      'https://github.com/acme/portal/pull/409',
      'https://linear.app/acme/issue/ABC-244',
    ])

    /* Review 43 is `acme/portal#403`, posted and not yet answered. */
    const author = lines(page).find(line => line.row.text().includes('#403'))
    expect(author, 'the Waiting on the author row for #403').toBeDefined()
    expect(author!.out.map(link => link.attributes('href'))).toEqual([
      'https://github.com/acme/portal/pull/403',
      'https://linear.app/acme/issue/ABC-238',
    ])
  })

  it('names the ticket on those rows even when it cannot link it', async () => {
    /*
     * The other half of the same gap. The contract says of `ticket_url` that null is "a
     * review with an id and no address, which the row then names on its own line and does
     * not link" — which was true of the needs-you rows and the Reviews list, and false of
     * these two: their meta line was the repository alone. So serving them the pair closed
     * "cannot link its ticket" and left "cannot even name it" open, under text saying
     * otherwise.
     */
    const page = await shown(Inbox)

    /* Review 19 is `acme/billing-api#93`, step 3 running, ticket ABC-246 and no address. */
    const row = lines(page).find(line => line.row.text().includes('#93'))
    expect(row, 'the With the agent row for #93').toBeDefined()
    expect(row!.meta.text(), 'the id is named on the row').toContain('ABC-246')
    expect(
      row!.out.map(link => link.attributes('href')),
      'and the pull request is the only link',
    ).toEqual(['https://github.com/acme/billing-api/pull/93'])
  })

  it('names the ticket on a posted row that has no address either', async () => {
    /*
     * **The same half-gap on the other group, which nothing reached.** Proved by mutation:
     * reverting "Waiting on the author"'s meta to the repository alone left the whole suite
     * green, because no posted fixture had the id-without-address shape *and* was drawn in
     * that group — review 12 has it and is subtracted into "Needs you", and review 43 has
     * an address. The case is live in the real binary, where the end-to-end seed's posted
     * review carries `ABC-244` and no `--ticket-url`.
     *
     * The address is taken off review 43 for this test rather than out of the fixtures,
     * because the test above needs it there: the two assertions want the same row in its
     * two states.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map(detail =>
        detail.review.id === 43
          ? { ...detail, review: { ...detail.review, ticket_url: null } }
          : detail),
    })

    const page = await shown(Inbox)
    const row = lines(page).find(line => line.row.text().includes('#403'))
    expect(row, 'the Waiting on the author row for #403').toBeDefined()
    expect(row!.meta.text(), 'the id is named on the row').toContain('ABC-238')
    expect(
      row!.out.map(link => link.attributes('href')),
      'and the pull request is the only link',
    ).toEqual(['https://github.com/acme/portal/pull/403'])
  })

  it('draws no ticket link on a row whose review has an id and no address', async () => {
    const page = await shown(Reviews)
    /* Review 41 is `#514`, ticket ABC-402, and the record holds no address for it. */
    const row = lines(page).find(line => line.row.text().includes('#514'))
    expect(row, 'the list row for #514').toBeDefined()

    /* The pull request alone, and the ticket still named on the row's own meta line. */
    expect(row!.out).toHaveLength(1)
    expect(row!.out[0]!.attributes('href')).toBe('https://github.com/acme/portal/pull/514')
    expect(row!.meta.text()).toContain('ABC-402')
  })

  it('does not offer to open a pull request it cannot build a link to', async () => {
    /*
     * **A row whose only purpose is a link has to say when there is no link.** `prUrl`
     * refuses a `repo` that is not a repository name — a released Review Desk could store
     * `acme/portal#x`, and nothing can repair the column, because `repo` is identity — and
     * `QueueRow` then draws the row as a `div` rather than an anchor. The row went on saying
     * "Open on GitHub" in the accent colour with the "opens in a new tab" hint gone:
     * advertising an action it could not perform.
     */
    const source = loadSource()
    await resetDesk({
      own_prs: source.own_prs.map((item, at) =>
        at === 0
          ? { ...item, pull_request: { ...item.pull_request, repo: 'acme/portal#x' } }
          : item),
    })

    const page = await shown(Inbox)
    const row = lines(page).find(line => line.text().includes('acme/portal#x'))
    expect(row, 'the row for the unlinkable name').toBeDefined()
    expect(row!.row.element.tagName.toLowerCase(), 'it is not an anchor').toBe('div')
    expect(row!.row.text()).not.toContain('Open on GitHub')
    expect(row!.row.text()).toContain('not a repository name')

    /* And a row whose name is fine still offers it. */
    const fine = lines(page).find(line => line.row.text().includes('Open on GitHub'))
    expect(fine, 'the other own-pull-request rows are unaffected').toBeDefined()
    expect(fine!.row.attributes('href')).toMatch(/^https:\/\/github\.com\//)
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

/*
 * **The queue's single row for a pull request whose later round has posted. DEV-983.**
 *
 * Patrick's queue on 14 October 2026 drew `globex/portal#9585`'s two rounds as one row
 * saying "round 1" and opening round 1, though round 2 had been posted the day before. Two
 * faults met on that row: an earlier round's code change that a posted round had already
 * read was still counted as waiting, and the two arrays that know a pull request's newest
 * round could join onto a row without renumbering it.
 *
 * Its own describe rather than cases inside "the queue", because every test there asserts
 * over the seeded desk as it stands — the heading's count, the repository chips — and these
 * each need round 2 posted, which changes both.
 */
describe('a pull request whose later round has posted', () => {
  const ROUND_ONE = 62
  const ROUND_TWO = 63
  const ROUND_ONE_HEAD = 'c3d4e53'
  const ROUND_TWO_HEAD = 'd4e5f64'
  /** A commit after round 2's head, which no round of the fixture records. */
  const LATER_HEAD = 'e5f6a75'

  /** The detail the harvester writes for a code change, at this commit and comparison. */
  const changeDetail = (commit: string, from: string, to: string) => JSON.stringify({
    file: 'app/Providers/FortifyServiceProvider.php',
    line: 72,
    lines_changed: false,
    file_status: 'modified',
    commit,
    basis: `compare ${from}...${to}`,
  })

  const codeChanged = (review: number, id: number, detail: string) => ({
    id,
    review_id: review,
    finding_seq: 1,
    finding_key: 'F1' as const,
    kind: 'code_changed_after' as const,
    actor: 'harvester' as const,
    occurred_at: '2026-10-13T06:00:00Z',
    detail,
  })

  /**
   * The desk with round 2 posted, and these outcomes on each round.
   *
   * Posting round 2's draft is also what takes it out of "drafts to approve", which is the
   * state the issue's fixtures are in: both rounds reviewed and posted, nothing drafted.
   */
  async function twoRounds(options: {
    onRoundOne?: ReturnType<typeof codeChanged>[]
    onRoundTwo?: ReturnType<typeof codeChanged>[]
    keepRoundOneReply?: boolean
  }) {
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        if (detail.review.id === ROUND_ONE) {
          return {
            ...detail,
            outcomes: [
              ...(options.keepRoundOneReply ? detail.outcomes : []),
              ...(options.onRoundOne ?? []),
            ],
          }
        }
        if (detail.review.id === ROUND_TWO) {
          return {
            ...detail,
            review: { ...detail.review, github_review_id: 'PRR_globex9585r2' },
            outcomes: options.onRoundTwo ?? [],
          }
        }
        return detail
      }) as typeof source.reviews,
      drafts: source.drafts.map(detail =>
        detail.draft.review_id === ROUND_TWO
          ? {
              ...detail,
              draft: {
                ...detail.draft,
                status: 'posted' as const,
                event: 'comment' as const,
                posted_at: '2026-10-13T05:00:00Z',
              },
            }
          : detail) as typeof source.drafts,
    })
  }

  /** The rows under a heading, whatever that group draws them from. */
  function rowsUnder(page: VueWrapper, heading: string) {
    const group = page.findAll('section').find(one => one.text().startsWith(heading))
    return group ? group.findAll('.queue-line') : []
  }

  /** The rows of `globex/portal` the page draws anywhere, for the chip's count. */
  function globexRows(page: VueWrapper) {
    return [
      ...rowsUnder(page, 'Needs you'),
      ...rowsUnder(page, 'With the agent'),
      ...rowsUnder(page, 'Waiting on the author'),
    ].filter(row => row.text().includes('globex/portal'))
  }

  it('draws no "code changed" row for a change the posted round read', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
      )],
    })
    const page = await shown(Inbox)

    const needing = rowsUnder(page, 'Needs you').filter(row => row.text().includes('#9585'))
    expect(needing, 'nothing about #9585 is waiting on the developer').toHaveLength(0)

    const posted = rowsUnder(page, 'Waiting on the author')
      .filter(row => row.text().includes('#9585'))
    expect(posted, 'one row, under the group that says who has it').toHaveLength(1)
    expect(posted[0]!.text()).toContain('Round 2 of 2 posted')
    expect(posted[0]!.text()).not.toContain('changed')
    expect(posted[0]!.find('.row').attributes('href')).toBe(`/reviews/${ROUND_TWO}/understand`)
  })

  it('draws a change after that head as the newest round’s', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(LATER_HEAD, ROUND_ONE_HEAD, LATER_HEAD),
      )],
      onRoundTwo: [codeChanged(
        ROUND_TWO,
        9201,
        changeDetail(LATER_HEAD, ROUND_TWO_HEAD, LATER_HEAD),
      )],
    })
    const page = await shown(Inbox)

    const rows = rowsUnder(page, 'Needs you').filter(row => row.text().includes('#9585'))
    expect(rows, 'one row for one pull request').toHaveLength(1)
    expect(rows[0]!.text()).toContain('round 2')
    expect(rows[0]!.text()).toContain('The code under F1 changed')
    expect(rows[0]!.text(), 'the change is this round’s, so no earlier round is named')
      .not.toContain('round 1')
    expect(rows[0]!.find('.row').attributes('href')).toBe(`/reviews/${ROUND_TWO}/understand`)
  })

  it('draws an earlier round’s reply on the newest round’s row, saying which', async () => {
    await twoRounds({
      keepRoundOneReply: true,
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
      )],
    })
    const page = await shown(Inbox)

    const rows = rowsUnder(page, 'Needs you').filter(row => row.text().includes('#9585'))
    expect(rows, 'one row for one pull request').toHaveLength(1)
    const row = rows[0]!
    /*
     * **The row is round 2: its number and its link.** The reply is on round 1 and is still
     * drawn, with the round it was left on — DEV-957's wording for exactly this, now
     * reachable on a row whose next action *is* the reply.
     */
    expect(row.text()).toContain('round 2')
    expect(row.find('.row').attributes('href')).toBe(`/reviews/${ROUND_TWO}/understand`)
    expect(row.text()).toContain('The author replied to F1')
    expect(row.text()).toContain('replied to round 1')
    /* And the ticket, which is read from the item about the round the row is. */
    expect(row.text()).toContain('DEV-958')
  })

  it('numbers the row for a newest round nothing else in the answer mentions', async () => {
    /*
     * **Round 2 opened and nothing written to it**, which is what a dispatcher does the
     * moment GitHub shows a new head after a post. It is then in no array of its own — not
     * the newest posted round, no draft at a status, no unconfirmed brief and no running
     * step — so the row is in the answer only through round 1's reply, and the item's
     * `rounds_on_pr` is the only thing that can say which round the row is. Before that
     * field the row said "round 1" and the click went to round 1. Found by a review of this
     * branch.
     */
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        if (detail.review.id !== ROUND_TWO) return detail
        return {
          ...detail,
          review: { ...detail.review, github_review_id: null },
          outcomes: [],
          /* Nothing running, so `in_progress` does not mention it either. */
          layers: detail.layers.map(layer =>
            layer.state === 'running' ? { ...layer, state: 'pending' as const } : layer),
        }
      }) as typeof source.reviews,
      drafts: source.drafts.map(detail =>
        detail.draft.review_id === ROUND_TWO
          ? { ...detail, draft: { ...detail.draft, status: 'drafting' as const } }
          : detail) as typeof source.drafts,
    })

    const page = await shown(Inbox)
    const rows = rowsUnder(page, 'Needs you').filter(row => row.text().includes('#9585'))
    expect(rows, 'one row for one pull request').toHaveLength(1)
    expect(rows[0]!.text()).toContain('round 2')
    expect(rows[0]!.text()).toContain('replied to round 1')
    expect(rows[0]!.find('.row').attributes('href')).toBe(`/reviews/${ROUND_TWO}/understand`)
    /*
     * **And the ticket, which the first fix lost here.** No item above the reply is about
     * round 2, and a row takes a ticket only from something about the round it is numbered
     * for — DEV-957 — so the row had a ticket on the record and nothing to reach it with.
     * Found by a review of the first fix batch.
     */
    expect(rows[0]!.text()).toContain('DEV-958')
  })

  it('titles and marks a raised row for the round it is, not the round that raised it',
    async () => {
      /*
       * **A pull request renamed between rounds, with the row raised past the item that
       * made it.** The title was taken from whichever item did the raising, and a reply
       * item raising a row to the newest round is an item about the *older* one — so the
       * row drew round 1's title under round 2's number, and the equal-round posted item
       * could not correct it because `raise` returns early. And `is_delta` came from the
       * one array that carries it, so a row with no draft said nothing about being a
       * review of changes since the last one. Both found by a review of the second fix
       * batch.
       */
      await twoRounds({
        keepRoundOneReply: true,
        onRoundOne: [codeChanged(
          ROUND_ONE,
          9200,
          changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
        )],
      })
      /*
       * Renamed after round 1, which is a thing authors do. Written onto the live desk
       * rather than passed to `resetDesk`, because `twoRounds` has already built the
       * overrides this case needs and a second reset would undo them.
       */
      for (const detail of currentSource().reviews) {
        if (detail.review.id === ROUND_ONE) detail.review.title = 'The old name'
        if (detail.review.id === ROUND_TWO) detail.review.title = 'The name it has now'
      }

      const page = await shown(Inbox)
      const rows = rowsUnder(page, 'Needs you').filter(row => row.text().includes('#9585'))
      expect(rows, 'one row for one pull request').toHaveLength(1)
      expect(rows[0]!.text()).toContain('The name it has now')
      expect(rows[0]!.text(), 'and not the name the round that made the row carries')
        .not.toContain('The old name')
      expect(rows[0]!.text(), 'round 2 is a review of changes since round 1')
        .toContain('changes since your last review')
    })

  it('counts the row it draws, in the heading and in the repository chip', async () => {
    await twoRounds({
      keepRoundOneReply: true,
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
      )],
    })
    const page = await shown(Inbox)

    const drawn = rowsUnder(page, 'Needs you')
    expect(page.text()).toContain(
      drawn.length === 1 ? '1 pull request needs you' : `${drawn.length} pull requests need you`,
    )
    /*
     * **And the chip counts the rows pressing it shows**, which is the rule `repoTabs`
     * states. A row that had failed to merge would show here as a number one too high.
     */
    const chip = page.findAll('button, label, span')
      .map(one => one.text())
      .find(text => text.startsWith('globex/portal'))
    expect(chip, 'the repository has a chip').toBeDefined()
    expect(chip).toContain(String(globexRows(page).length))
  })
})

/*
 * **`/reviews/{id}` is a redirect, and the tests that were here went with the page.**
 *
 * It drew the whole of a review as paragraphs — the problem, the options, every finding and
 * its reasoning — and DEV-885 and DEV-878 replaced it with a page per step. What it asserted
 * is asserted on the pages that draw those things now:
 *
 * | What was tested here | Where it is now |
 * |---|---|
 * | the seven layers, the problem, its diagram, the option that was built | `step-pages.nuxt.spec.ts` |
 * | recorded text keeping its paragraphs and its measure | `step-pages.nuxt.spec.ts`, and `code-review.nuxt.spec.ts` for a finding's evidence |
 * | every finding with its severity and disposition, and what was not checked | `code-review.nuxt.spec.ts` |
 * | the brief confirmation's three states | `step-pages.nuxt.spec.ts` |
 * | the author's replies on a posted review | `code-review.nuxt.spec.ts`, on the finding each is about |
 * | "not on the desk" for a review that is not there | `step-pages.nuxt.spec.ts` |
 *
 * **One is not carried over, and it is the disposition panel.** The old page moved a finding
 * between the draft and held back through `setFindingDisposition`, with a free-text reason.
 * The code review page decides through `decideFinding` instead — eight decisions, each
 * leaving an outcome that says the developer made it — so the panel has no equivalent to
 * port. `setFindingDisposition` is still the contract's operation for an agent and is still
 * covered in `desk.spec.ts`; nothing in the dashboard calls it any more.
 */
describe('the one-page review, retired', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  /*
   * The address is in links people have, in bookmarks and in anything written down, so it
   * keeps working. A route that 404s is worse than one that arrives somewhere useful, and
   * Understand is where every queue row already lands.
   */
  it('redirects to Understand, rather than answering not-found', async () => {
    const redirect = (OneReview as { definePageMeta?: unknown }) as Record<string, unknown>
    expect(redirect, 'the page still exists, as a redirect').toBeTruthy()

    const page = await shown(OneReview, { route: '/reviews/17' })
    /*
     * Mounted directly, the middleware does not run — `mountSuspended` mounts the component,
     * not the route — so what is checked here is that nothing of the old page survives to be
     * drawn. **The redirect itself is asserted in the browser**, in
     * `web/e2e/review-desk.spec.ts`, where there is a router to redirect.
     */
    expect(page.text()).not.toContain('Findings')
    expect(page.text()).not.toContain('Confirm the problem')
    expect(page.text()).not.toContain('Not checked')
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

  /*
   * **The one way out of a sign-off, offered where the sign-off was made.** A draft the
   * hand-over refuses — one approved before the composition changed shape — could not be
   * posted, rewritten or sent back, which the session building the posting half measured.
   * The page offers taking the sign-off back, and says why on a draft that is stuck.
   */
  it('offers taking the sign-off back on an approved draft, and says why on a stale one', async () => {
    expect((await useReviewDesk().approveDraft(41, 'request_changes')).ok).toBe(true)

    const page = await open(41)
    const back = page.findAll('button').find(one =>
      one.text() === 'Take back the sign-off and send it back')
    expect(back, 'the button is offered on an approved draft').toBeDefined()
    expect(back!.attributes('disabled')).toBeUndefined()
    expect(page.text()).toContain('Signed off. Will be posted as')

    /* And the sign-off goes with it: the record holds no event once it is taken back. */
    expect((await useReviewDesk().sendBackDraft(41, 'the index is in the old shape')).ok)
      .toBe(true)
    const after = await open(41)
    expect(after.text()).toContain('Sent back.')
    expect(after.text()).toContain('the index is in the old shape')
  })

  /*
   * **The body is an index, and the threads are the rest of what posts.** DEV-929: a real
   * review went out as 309 lines with no index and two threads because the posted body
   * *was* the blocks joined. The page's own job did not change — nothing is posted unread —
   * but what it has to draw did: a finding with a line inside the diff is a row in the body
   * and a comment on the code.
   *
   * Review 41 is the real-length example: twelve posted findings, nine of them anchored.
   */
  it('draws the index in the body and the threads beside it', async () => {
    const page = await open(41)
    const body = page.find('.preview').text()

    expect(body).toContain('Open — needs a decision')
    expect(body).toContain('🔴 Blocker · F9')
    expect(body).toContain('Open · on the code')
    /* F9 is anchored, so the body indexes it and does not carry its section. */
    expect(body).not.toContain('Blocker · F9 · The column policy')

    const threads = page.find('.threads')
    expect(threads.find('.rd-label').text()).toContain('9 threads')
    expect(threads.findAll('.thread')).toHaveLength(9)
    expect(threads.text()).toContain('app/Reporting/ColumnPolicy.php:52')
    /* And the comment as the author will read it, with the severity before a word of it. */
    expect(threads.text()).toContain('Blocker · F9')
  })

  /*
   * **And a draft with nothing to anchor says so.** Review 17's is written as one block of
   * markdown — the state the per-finding decisions refuse against, seeded deliberately — so
   * it has no block about any finding, nothing to index and nothing to hang a comment on.
   * That is a state rather than an absence, and the card says which it is rather than being
   * left empty.
   */
  it('says when no finding has a line inside the diff', async () => {
    const page = await open(17)
    const threads = page.find('.threads')

    expect(threads.find('.rd-label').text()).toContain('0 threads')
    expect(threads.find('.rd-empty').text())
      .toContain('has a line inside the diff to hang a comment on')
    /* One prose block, so the body is what the session wrote and carries no index. */
    expect(page.find('.preview').text()).not.toContain('Open — needs a decision')
  })

  /*
   * **One line a finding, and the verb once.** DEV-928: the panel printed every toggle as
   * its own line and the verb twice — "Kept F1 · kept", four times over on the review it was
   * raised from. These three were each edited once, so each is one line; the five-edit case
   * is `test/draft.spec.ts`'s, and the browser measures it on review 42.
   */
  it('lists the developer’s edits, one line a finding', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Your edits to this draft')
    expect(text).toContain('Rewrote F1 · first sentence')
    expect(text).toContain('Regraded F3 · Blocker to Should')
    expect(text).toContain('Cut F6 · rename suggestion')
    expect(page.findAll('.edit')).toHaveLength(3)
    /* Nothing changed twice here, so nothing claims it did. */
    expect(text).not.toContain('changed')
  })

  /*
   * **The findings held back are rows, not paragraphs.** The claim is behind the row, the
   * severity is a word, and nothing on the row grades the finding: this is the list of what
   * the developer has decided *not* to post, and a red blocker on it would be telling them
   * to be afraid of something they have already settled.
   */
  it('draws the findings held back as rows, grouped and counted', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Held back · 3 findings')
    /* Board A8's order: the question above the nits. */
    expect(page.findAll('.held-group').map(group => group.text()))
      .toEqual(['1 question', '2 nits'])
    expect(page.findAll('.held-row')).toHaveLength(3)

    const first = page.findAll('.held-row')[0]!
    expect(first.find('.cell--key').text()).toBe('F7')
    expect(first.find('.cell--severity').text()).toContain('question')
    expect(first.find('.cell--line').text())
      .toContain('Is the 24-hour ceiling the ticket\'s number or the queue\'s?')
    expect(first.attributes('aria-expanded')).toBe('false')
    /* No severity tag and no severity colour anywhere in the panel. */
    expect(page.find('.held').findAll('.tag')).toHaveLength(0)
  })

  /*
   * **That a row opens, and nothing more.** One-row-at-a-time, the `aria-controls` pairing
   * and the detail's contents are the panel's own rule and are asserted on the panel, in
   * `test/held-back-rows.nuxt.spec.ts`. This is the page test, so it asserts the one thing
   * only the page can answer: that the panel is mounted against the draft's own record and
   * opens against it. Raised by review, which found the same four facts in both files.
   */
  it('opens a held-back row against the draft’s own record', async () => {
    const page = await open(17)
    await page.findAll('.held-row')[0]!.trigger('click')
    expect(page.find('.held-opened').text()).toContain('config/webhooks.php:31')
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

    /*
     * **Scoped to what the developer presses and reads as a label**, rather than to every
     * character on the page. The rule is that the dashboard calls this signing off and
     * GitHub calls it approving, and conflating the two makes a developer think the button
     * approves the pull request — so what must not say "approve" is the thing they press.
     *
     * It was every character until DEV-878, which added "Ticket what is left, and sign
     * off": the contract calls that operation `ticketRestAndApprove`, and the sentence
     * explaining it has to name the event the session will post, exactly as the control
     * does. Widening the exemption to one more string would have been the third exemption;
     * saying what the rule is actually about is better than listing what it is not.
     */
    const pressable = page.findAll('button, label, h1, h2, .rd-label')
      .map(one => one.text())
      .join(' · ')
    expect(pressable).not.toMatch(/approv/i)
    expect(pressable).toContain('Sign off and post')
    expect(pressable).toContain('Ticket what is left, and sign off')
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

  /*
   * **A review with no draft is a page, not a refusal**, and that is what lets step 6 of
   * the trail be a link whatever the draft status says. `GET …/draft` still answers 404;
   * what changed is that the answer is drawn inside the frame, so the trail is above it and
   * every other step is one click away. The blocking "Not on the desk" panel this used to
   * show carried no trail and no way back — the dead end the trail exists to prevent.
   */
  it('says no draft has been written, inside the frame, rather than refusing', async () => {
    const page = await shown(DraftPreview, { route: '/reviews/18/draft' })
    expect(page.text()).toContain('No draft yet.')
    expect(page.text()).toContain('No session has written a draft for this review')
    expect(page.text()).not.toContain('Not on the desk')
    /* And the trail is there, which is the whole point. */
    expect(page.findAll('.trail a').map(one => one.attributes('href')))
      .toContain('/reviews/18/understand')
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
      posted_awaiting_author: [],
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
              posted_awaiting_author: [],
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
