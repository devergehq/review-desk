// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import Reviews from '../app/pages/reviews/index.vue'
import Understand from '../app/pages/reviews/[id]/understand.vue'

import type { ReviewDetail } from '../app/types/desk'

/*
 * **Taking a review off the desk, through the screens.**
 *
 * DEV-945: a review is dismissed from its own page with a reason, it leaves the queue and
 * the Reviews list's default view, it can be brought back, and one that never got past
 * being requested can be deleted outright. The store and the API hold the rules —
 * `crates/store/tests/dismissal.rs` and `crates/server/tests/removal.rs` — and these are
 * the two things only a screen can be wrong about: which control is offered, and what the
 * reader is told.
 */

const STILL_READING = /Reading [^…]*…/

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
  options?: Parameters<typeof mountSuspended>[1],
) {
  const page = await mountSuspended(component, options)
  await settle(page)
  return page
}

function button(page: VueWrapper, label: string) {
  return page.findAll('button').find(item => item.text() === label)
}

async function click(page: VueWrapper, label: string) {
  const found = button(page, label)
  if (!found) {
    throw new Error(
      `there is no button labelled ${label}; the page has `
      + `${JSON.stringify(page.findAll('button').map(one => one.text()))}`,
    )
  }
  await found.trigger('click')
  await settle(page)
}

/** Choose one position of a `DecisionTrack` — a radio drawn as a button. */
async function choose(page: VueWrapper, label: string) {
  const found = page
    .findAll('[role="radio"]')
    .find(one => one.text() === label)
  if (!found) {
    throw new Error(
      `there is no choice labelled ${label}; the page has `
      + `${JSON.stringify(page.findAll('[role="radio"]').map(one => one.text()))}`,
    )
  }
  await found.trigger('click')
  await settle(page)
}

/** Mount the Understand page for one review. */
function understand(id: number) {
  return shown(Understand, { route: `/reviews/${id}/understand` })
}

/**
 * A review nobody started, added to the invented records.
 *
 * **The shape of the review DEV-945 was raised from**: a request recorded and never picked
 * up — no brief, no findings, no draft — which is the only review a delete is offered for.
 * Built from a fixture rather than typed out, so a field added to `ReviewDetail` cannot
 * leave this one short of it.
 */
function strayReview(): ReviewDetail {
  const [first] = loadSource().reviews
  return {
    ...first!,
    review: {
      ...first!.review,
      id: 777,
      pr: 777,
      title: null,
      ticket_id: null,
      session_id: null,
      dismissed_at: null,
      dismissed_reason: null,
    },
    brief: null,
    findings: [],
    outcomes: [],
    draft_status: null,
  }
}

/** The records, with the stray review beside them. */
async function withStray() {
  const source = loadSource()
  await resetDesk({ reviews: [...source.reviews, strayReview()] })
}

describe('the Reviews list’s dismissed filter', () => {
  beforeEach(async () => {
    await resetDesk()
    /* Review 16 is dismissed through the API, which is the only way one can be. */
    await useReviewDesk().dismissReview(16, 'the pull request was closed unmerged')
  })

  it('leaves a dismissed review out of the default view, and says what the view is', async () => {
    const page = await shown(Reviews)
    expect(page.text()).toContain('Every review in play')
    expect(page.text()).not.toContain('#9585')
    const rows = page.findAll('.row').map(row => row.text())
    expect(rows.some(row => row.includes('Dismissed'))).toBe(false)
  })

  it('shows the dismissed reviews on their own, with when and why', async () => {
    const page = await shown(Reviews)
    await choose(page, 'Dismissed')

    expect(page.text()).toContain('The reviews you have dismissed')
    const rows = page.findAll('.row')
    expect(rows).toHaveLength(1)
    expect(rows[0]!.text()).toContain('Dismissed')
    expect(rows[0]!.text()).toContain('just now')
    expect(rows[0]!.text()).toContain('the pull request was closed unmerged')
  })

  it('shows both together under Everything', async () => {
    const page = await shown(Reviews)
    const inPlay = page.findAll('.row').length

    await choose(page, 'Everything')
    expect(page.text()).toContain('whatever became of it')
    expect(page.findAll('.row')).toHaveLength(inPlay + 1)
    /* And the dismissed row still says so, beside reviews that are in play. */
    expect(page.findAll('.row').filter(row => row.text().includes('Dismissed')))
      .toHaveLength(1)
  })

  it('says no review has been dismissed rather than that the database is empty', async () => {
    // The fault this is against: `isEmpty` claimed the whole database was empty whenever
    // an answer held no rows, so asking for the dismissed reviews on a desk with none took
    // the pickers off the page and said "This database holds no reviews".
    await resetDesk()
    const page = await shown(Reviews)
    await choose(page, 'Dismissed')

    expect(page.text()).toContain('No review has been dismissed')
    expect(page.text()).not.toContain('This database holds no reviews')
    /* The pickers are still there, which is the only way back out. */
    expect(page.findAll('[role="radio"]').map(one => one.text())).toContain('In play')
  })
})

describe('dismissing a review from its page', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('asks why, and refuses a blank reason without writing anything', async () => {
    const page = await understand(17)
    await click(page, 'Dismiss this review')

    expect(page.text()).toContain('Why are you dismissing this review?')
    /* The box is empty, so the control refuses before anything is sent. */
    await click(page, 'Dismiss it')
    expect(page.text()).toContain('A reason is required')
    expect(page.text()).not.toContain('is dismissed and out of the queue')
  })

  it('dismisses it with the reason, and the page says what that means', async () => {
    const page = await understand(17)
    await click(page, 'Dismiss this review')

    const field = page.find('#dismiss-reason')
    await field.setValue('the pull request was closed unmerged')
    await click(page, 'Dismiss it')

    expect(page.text()).toContain('is dismissed and out of the queue')
    expect(page.text()).toContain('Dismissed just now')
    expect(page.text()).toContain('the pull request was closed unmerged')
    /* And the page says so at the top as well, not only in the card that did it. */
    expect(page.text()).toContain('This review is dismissed')
    expect(page.text()).toContain('Everything on it has been kept')
  })

  it('offers the undo, and brings it back', async () => {
    const page = await understand(17)
    await click(page, 'Dismiss this review')
    await page.find('#dismiss-reason').setValue('a duplicate of #413')
    await click(page, 'Dismiss it')

    await click(page, 'Bring it back')

    expect(page.text()).toContain('is back in the queue')
    expect(page.text()).not.toContain('This review is dismissed')
    expect(button(page, 'Dismiss this review')).toBeDefined()
  })
})

describe('deleting a review that never started', () => {
  it('is not offered for a review with work on it, and says what it holds', async () => {
    await resetDesk()
    const page = await understand(17)

    expect(page.text()).toContain('It cannot be deleted')
    expect(page.text()).toContain('a brief, 7 findings and a draft')
    expect(page.text()).toContain('Dismissing it takes it out of the queue')
    expect(button(page, 'Delete it')).toBeUndefined()
  })

  it('is offered for a review with no brief, no findings and no draft', async () => {
    await withStray()
    const page = await understand(777)

    expect(page.text()).toContain('This review never started')
    expect(button(page, 'Delete it')).toBeDefined()
  })

  it('asks a second time, naming the review, before it removes anything', async () => {
    await withStray()
    const page = await understand(777)

    await click(page, 'Delete it')
    expect(page.text()).toContain('Delete review 777')
    expect(page.text()).toContain('acme/portal #777')
    expect(page.text()).toContain('cannot be undone')

    /* And "Keep it" is a real way out that leaves the review alone. */
    await click(page, 'Keep it')
    expect(page.text()).not.toContain('Delete review 777')
    const list = await shown(Reviews)
    expect(list.text()).toContain('#777')
  })

  it('removes the review, and the list no longer holds it', async () => {
    await withStray()
    const page = await understand(777)

    await click(page, 'Delete it')
    await click(page, 'Delete it for good')

    const list = await shown(Reviews)
    expect(list.text()).not.toContain('#777')
    /* In no view, which is the difference between a delete and a dismissal. */
    await choose(list, 'Everything')
    expect(list.text()).not.toContain('#777')
  })
})
