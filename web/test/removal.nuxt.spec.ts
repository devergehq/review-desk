// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'
import { loadSource, sampleBackend } from '../app/data/sample'

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
    /*
     * **Nothing on it but the request**, which is what the fixture has to be for the
     * page to say so: seven steps none of them begun, and none of the things a session
     * writes on its way to a brief. Built by emptying a real one rather than by typing a
     * `ReviewDetail` out, so a field added to the shape cannot leave this one short of
     * it — and emptied field by field, because the card reads every one of them.
     */
    layers: first!.layers.map(layer => ({
      ...layer,
      state: 'pending' as const,
      detail: null,
      runs: [],
    })),
    brief: null,
    findings: [],
    outcomes: [],
    draft_status: null,
    not_checked: null,
    sources: [],
    criteria: [],
    description_lint: null,
    coverage: { source: 'not_recorded' as const, files: [] },
    commits_since_previous_round: [],
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
    /*
     * **`#88`, which is the pull request of the review this block dismisses.** It read
     * `#9585` and was therefore vacuous: no fixture had that number, so the assertion
     * passed whether or not the dismissed review was drawn. DEV-957's fixtures gave
     * `globex/portal#9585` to a review that is in play, and the test failed — which is how the
     * hole was found. The lesson is the one the queue's own comments keep: an assertion
     * about a fixture has to name a fixture that exists.
     */
    const page = await shown(Reviews)
    expect(page.text()).toContain('Every review in play')
    expect(page.text()).not.toContain('#88')
    const rows = page.findAll('.row').map(row => row.text())
    expect(rows.some(row => row.includes('Dismissed'))).toBe(false)
  })

  it('shows the dismissed reviews on their own, with when and why', async () => {
    const page = await shown(Reviews)
    await choose(page, 'Dismissed')

    expect(page.text()).toContain('The reviews you have dismissed')
    expect(
      page.findAll('[role="radio"]')
        .filter(one => one.attributes('aria-checked') === 'true')
        .map(one => one.text()),
    ).toContain('Dismissed')
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
    /*
     * **The in-play rows, the one dismissed review, and the finished ones.** Counted from
     * the Finished view rather than written as a number, so a fixture added later moves
     * all three figures together — the count of finished rows is exactly what DEV-957's
     * fixtures changed, and a literal here would have to be edited by anybody who adds a
     * review whose pull request has merged.
     */
    const everything = page.findAll('.row').length
    await choose(page, 'Finished')
    const finished = page.findAll('.row').length
    expect(finished, 'the fixtures hold some finished reviews').toBeGreaterThan(0)
    expect(everything).toBe(inPlay + 1 + finished)

    await choose(page, 'Everything')
    /* And the dismissed row still says so, beside reviews that are in play. */
    expect(page.findAll('.row').filter(row => row.text().includes('Dismissed')))
      .toHaveLength(1)
  })

  it('can reach a review off the desk from a desk whose only review has left it', async () => {
    // **The one page that is supposed to find a review whatever became of it**, on the
    // database DEV-945 was raised from: one review, dismissed rather than deleted.
    // `DeskState` draws the empty card *instead of* the default slot, pickers and all, so
    // the card has to carry the way out or the review is unreachable.
    //
    // **The button goes to Everything since DEV-957**, and the label with it. There are two
    // ways off the desk now and the card cannot say which this database's one review took
    // — the answer it is drawn from is the one that leaves both out — so sending the reader
    // to Dismissed would be a guess that is wrong half the time, on the one page that is
    // supposed to be able to find a review whatever became of it.
    const [only] = loadSource().reviews
    await resetDesk({ reviews: [only] })
    await useReviewDesk().dismissReview(only!.review.id, 'a duplicate of #413')

    const page = await shown(Reviews)
    expect(page.text()).toContain('No reviews in play')
    expect(page.text()).not.toContain('This database holds no reviews')

    await click(page, 'Look at the reviews that have left the desk')
    expect(page.findAll('.row')).toHaveLength(1)
    expect(page.text()).toContain('a duplicate of #413')
    /* And the pickers are back, because the default slot is drawing again. */
    expect(page.findAll('[role="radio"]').map(one => one.text())).toContain('In play')
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

  it('cannot be sent twice by two presses', async () => {
    // The second press would lose the store's already-dismissed race and show a refusal
    // over a dismissal that had in fact succeeded.
    const page = await understand(17)
    await click(page, 'Dismiss this review')
    await page.find('#dismiss-reason').setValue('the pull request was closed unmerged')

    const confirm = page.findAll('button').find(one => one.text() === 'Dismiss it')!
    await confirm.trigger('click')
    /* Mid-flight: the control is refusing further presses. */
    expect(page.find('#dismiss-reason').attributes('disabled')).toBeDefined()
    await settle(page)

    expect(page.text()).toContain('acme/portal#412 is dismissed.')
    expect(page.text()).not.toContain('already dismissed')
  })

  it('dismisses it with the reason, and the page says what that means', async () => {
    const page = await understand(17)
    await click(page, 'Dismiss this review')

    const field = page.find('#dismiss-reason')
    await field.setValue('the pull request was closed unmerged')
    await click(page, 'Dismiss it')

    expect(page.text()).toContain('acme/portal#412 is dismissed.')
    expect(page.text()).toContain('Dismissed just now')
    expect(page.text()).toContain('the pull request was closed unmerged')
    /* And the page says so at the top as well, not only in the card that did it. */
    expect(page.text()).toContain('This review is dismissed')
    expect(page.text()).toContain('Everything on it has been kept')
  })

  it('shows one control at a time, whichever was opened', async () => {
    // **Asserted on the whole set of buttons, not on one label.** The first version of
    // this test asked whether "Delete it" was gone and passed while the card was showing
    // the delete *confirmation* — a different label — so it was green about the wrong
    // element. `asking` is one slot and each branch has to name the state it draws:
    // sharing a `v-else` put a live "Delete it for good" beside the reason box, and
    // `ReasonPrompt` autofocuses, so Enter dismissed the review instead.
    const card = (page: VueWrapper) =>
      page.find('#remove-review').findAll('button').map(one => one.text())

    await withStray()
    const page = await understand(777)
    expect(card(page)).toEqual(['Dismiss this review', 'Delete it'])

    await click(page, 'Dismiss this review')
    await page.find('#dismiss-reason').setValue('a duplicate of #413')
    expect(card(page)).toEqual(['Dismiss it', 'Cancel'])

    await click(page, 'Cancel')
    expect(card(page)).toEqual(['Dismiss this review', 'Delete it'])

    await click(page, 'Delete it')
    expect(card(page)).toEqual(['Delete it for good', 'Keep it'])
    expect(page.find('#dismiss-reason').exists()).toBe(false)

    await click(page, 'Keep it')
    expect(card(page)).toEqual(['Dismiss this review', 'Delete it'])
  })

  it('stops saying a reason is required once one is typed', async () => {
    // The complaint was set on a blank press and never cleared, so it survived the
    // developer answering it — and on a refusal, which leaves the box open, the card
    // showed it under the real message.
    await resetDesk()
    const page = await understand(17)
    await click(page, 'Dismiss this review')
    await click(page, 'Dismiss it')
    expect(page.text()).toContain('A reason is required')

    await page.find('#dismiss-reason').setValue('the pull request was closed unmerged')
    await click(page, 'Dismiss it')
    expect(page.text()).not.toContain('A reason is required')
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

describe('what a dismissed review refuses', () => {
  it('refuses a sign-off rather than succeeding into nothing', async () => {
    // Sample mode refuses the way the binary refuses, which is what makes a refusal
    // state tested here one that works against the record. The four acts are the ones
    // that are items in `work list` — the list a dismissed review is not in.
    await resetDesk()
    const desk = useReviewDesk()
    await desk.dismissReview(17, 'the pull request was closed unmerged')

    for (const result of [
      await desk.confirmBrief(17),
      await desk.approveDraft(17, 'comment'),
      await desk.sendBackDraft(17, 'cut the nits'),
      await desk.requestNitTicket(17),
    ]) {
      expect(result.ok).toBe(false)
      if (!result.ok) {
        expect(result.error.code).toBe('conflict')
        expect(result.error.message).toContain('bring it back first')
      }
    }

    /* And what needs nobody afterwards still works. */
    const moved = await desk.setFindingDisposition(17, 1, 'held_back', 'a nit')
    expect(moved.ok).toBe(true)
  })

  it('asks in the binary’s order, so the refusal is about the same thing', async () => {
    /*
     * **The dismissal is the last question, not the first.** In the binary the guard is
     * inside the *store*'s write, and the handler reaches it only after the review, the
     * draft and the draft's status — so a dismissed review with a posted draft answers
     * "this draft is posted …" and one with no brief answers "has no brief to confirm".
     * Asked first, as these records did, both answered "is dismissed": the same refusal
     * code about a different thing, which is the one way a fixture backend can make a
     * refusal state tested here mean nothing. Found by a review of this branch.
     */
    await resetDesk()
    const desk = useReviewDesk()
    await desk.dismissReview(12, 'the pull request was closed unmerged')
    await desk.dismissReview(43, 'superseded by a rewrite')

    const signedOff = await desk.approveDraft(12, 'comment')
    expect(signedOff.ok).toBe(false)
    if (!signedOff.ok) {
      expect(signedOff.error.message).toContain('This draft is posted')
      expect(signedOff.error.message).not.toContain('dismissed')
    }

    const confirmed = await desk.confirmBrief(43)
    expect(confirmed.ok).toBe(false)
    if (!confirmed.ok) {
      expect(confirmed.error.message).toContain('no brief to confirm')
      expect(confirmed.error.message).not.toContain('dismissed')
    }

    /*
     * **And an id that is no review answers about the review, not about its draft.**
     * Every one of these handlers starts with `records.review`, which is the 404 the
     * guard used to draw here by standing first; moving it had the three draft calls
     * reaching for a draft on a review that does not exist. The same wrong subject, one
     * entity up — found by the review of the commit that moved it.
     */
    const unknown = await desk.approveDraft(999, 'comment')
    expect(unknown.ok).toBe(false)
    if (!unknown.ok) {
      expect(unknown.error.code).toBe('not_found')
      expect(unknown.error.message).toContain('no review 999')
    }
  })

  it('still takes a session’s report of a ticket it already filed', async () => {
    // **The ask and the report are two roles of one call**, and only the ask needs an
    // agent: the ticket exists in the tracker, and refusing the report would leave the
    // record never knowing which one. The store makes that split and these records have
    // to make the same one, because "it refuses where the binary refuses" is what lets a
    // refusal state tested on fixtures mean anything.
    //
    // **Reached through the backend rather than the composable**, which is the honest
    // place for it: `useReviewDesk.requestNitTicket` makes only the developer's ask — it
    // takes no ticket id — so the report role is the command line's and the API's, and a
    // test through the composable would be testing an argument nothing passes.
    await resetDesk()
    const desk = useReviewDesk()
    await desk.setFindingDisposition(17, 1, 'held_back', 'a nit')
    await desk.dismissReview(17, 'the pull request was closed unmerged')

    const records = sampleBackend()
    const reported = await records.requestNitTicket(17, 'ABC-500')
    expect(reported.status).toBe('filed')
    expect(reported.ticket_id).toBe('ABC-500')

    await expect(records.requestNitTicket(17)).rejects.toThrow(/bring it back first/)
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
    /*
     * The rows, not the whole page: the receipt the delete hands over names the review
     * it removed, which is the point of it.
     */
    expect(list.findAll('.row').map(row => row.text()).join(' ')).not.toContain('#777')
    /* In no view, which is the difference between a delete and a dismissal. */
    await choose(list, 'Everything')
    expect(list.findAll('.row').map(row => row.text()).join(' ')).not.toContain('#777')
  })

  it('says what went even when the deleted review was the last one in play', async () => {
    // **The database DEV-945 was raised from**: one review, and it is the stray. The
    // receipt was drawn inside the default slot, and `DeskState` draws the empty card
    // instead of that slot — so the one act with no undo lost its only acknowledgement
    // in exactly the case the feature is for.
    await resetDesk({ reviews: [strayReview()], drafts: [] })
    const page = await understand(777)
    await click(page, 'Delete it')
    await click(page, 'Delete it for good')

    const list = await shown(Reviews)
    expect(list.text()).toContain('No reviews in play')
    expect(list.text()).toContain('is deleted')
  })

  it('says what went, on the page the developer lands on', async () => {
    // **The one act with no undo is the one that must not be silent.** The sentence
    // cannot be said where it was done — that page is about a review that is no longer
    // there — so it is handed to the list. Read once: a reload says nothing, because it
    // is a receipt rather than a state.
    await withStray()
    const page = await understand(777)
    await click(page, 'Delete it')
    await click(page, 'Delete it for good')

    const list = await shown(Reviews)
    expect(list.text()).toContain('Review 777')
    expect(list.text()).toContain('is deleted')
    expect(list.text()).toContain('nothing of it is left')

    const again = await shown(Reviews)
    expect(again.text()).not.toContain('is deleted')
  })

  it('is not offered for a review posted outside Review Desk', async () => {
    // A review whose GitHub id was recorded by `review link` holds no brief, no findings
    // and no draft, and the Reviews list draws it as posted. The card told the developer
    // it had "nothing on it but the request".
    const source = loadSource()
    const posted = strayReview()
    posted.review.github_review_id = 'PRR_kwDOseeded'
    await resetDesk({ reviews: [...source.reviews, posted] })

    const page = await understand(777)
    expect(page.text()).toContain('It cannot be deleted')
    expect(page.text()).toContain('the review it posted on GitHub')
    expect(page.text()).not.toContain('nothing on it but the request')
    expect(button(page, 'Delete it')).toBeUndefined()
  })

  it('says "them" about several findings and "that" about one thing', async () => {
    // The plural was read off the rendered sentence by testing for " and ", so a review
    // holding only findings — which needs no brief — said "holds 2 findings, and
    // deleting it would lose that".
    const source = loadSource()
    const two = strayReview()
    two.findings = source.reviews[0]!.findings.slice(0, 2)
    const one = { ...strayReview(), review: { ...strayReview().review, id: 778, pr: 778 } }
    one.findings = source.reviews[0]!.findings.slice(0, 1)
    await resetDesk({ reviews: [two, one] })

    /* Whitespace collapsed: the sentence is wrapped across lines in the template. */
    const said = (page: VueWrapper) => page.text().replace(/\s+/g, ' ')

    expect(said(await understand(777)))
      .toContain('holds 2 findings, and deleting it would lose them')
    expect(said(await understand(778)))
      .toContain('holds 1 finding, and deleting it would lose that')
  })

  it('says a session has it for every sign the record reads, not only two', async () => {
    // The card read `session_id` and the layers while the store reads eight signs, so a
    // review holding a per-file change map — or its sources, the ticket's criteria, the
    // description lint, a round's commits, or a not-checked sentence — was still being
    // described as having nothing on it but the request, with a delete offered that
    // takes all of those rows with it.
    const source = loadSource()
    const mapped = strayReview()
    mapped.coverage = {
      source: 'files',
      files: [{
        position: 1,
        path: 'app/Reporting/ColumnPolicy.php',
        summary: 'the column policy',
        lines_added: 12,
        lines_removed: 3,
        checked: 'read_in_full',
        how_checked: null,
        finding_seqs: [],
      }],
    }
    await resetDesk({ reviews: [...source.reviews, mapped] })

    const page = await understand(777)
    expect(page.text()).toContain('A session has this review')
    expect(page.text()).not.toContain('nothing on it but the request')
  })

  it('says a session has it, rather than that nothing has started', async () => {
    // The card reads the same two signs the store reads before it refuses to move a
    // review's head. The delete is still offered — the store asks only about the brief,
    // the findings and the draft — but the sentence beside it has to be true.
    const source = loadSource()
    const claimed = strayReview()
    claimed.review.session_id = 'allele-ae03f8ce'
    await resetDesk({ reviews: [...source.reviews, claimed] })

    const page = await understand(777)
    expect(page.text()).toContain('A session has this review')
    expect(page.text()).not.toContain('nothing on it but the request')
    expect(button(page, 'Delete it')).toBeDefined()

    await click(page, 'Delete it')
    expect(page.text()).toContain('A session is still at work on it')
  })
})
