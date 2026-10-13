// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'

import Draft from '../app/pages/reviews/[id]/draft.vue'
import Reviews from '../app/pages/reviews/index.vue'
import Understand from '../app/pages/reviews/[id]/understand.vue'

/*
 * **A review whose pull request merged or closed, on the screens.**
 *
 * DEV-957. The store and the API hold the rules — `crates/store/tests/finished.rs` and
 * `crates/server/tests/removal.rs` — and these are the two things only a screen can be
 * wrong about: **which control is offered, and what the reader is told.**
 *
 * The record every assertion here reads is Patrick's own, as the fixtures carry it:
 *
 * | Pull request | What the fixture holds |
 * | -- | -- |
 * | `tc/portal#9541` | rounds 1 and 2, both unposted drafts |
 * | `tc/portal#9585` | round 1 posted and replied to, round 2 an unposted draft |
 * | `tc/portal#9568` | four rounds, one posted and three not, **merged on 11 October** |
 *
 * The merged one is where the defect was: three of its four rounds hold no `pr_state` of
 * their own, because the harvest reads the round that was posted. So every claim below
 * about #9568 is a claim about the derivation and not about a column.
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

/** Choose one position of a `DecisionTrack` — a radio drawn as a button. */
async function choose(page: VueWrapper, label: string) {
  const found = page.findAll('[role="radio"]').find(one => one.text() === label)
  if (!found) {
    throw new Error(
      `there is no choice labelled ${label}; the page has `
      + `${JSON.stringify(page.findAll('[role="radio"]').map(one => one.text()))}`,
    )
  }
  await found.trigger('click')
  await settle(page)
}

/** The draft page of one review, which is where a sign-off is offered. */
async function draftOf(reviewId: number) {
  return shown(Draft, { route: `/reviews/${reviewId}/draft` })
}

/** The four rounds of merged `tc/portal#9568`, round 1 being the one that posted. */
const MERGED = { posted: 64, unposted: [65, 66, 67] }

beforeEach(async () => {
  await resetDesk()
})

describe('the Reviews list’s Finished view', () => {
  it('offers Finished beside In play, Dismissed and Everything', async () => {
    const page = await shown(Reviews)
    expect(page.findAll('[role="radio"]').map(one => one.text()))
      .toEqual(expect.arrayContaining(['In play', 'Dismissed', 'Finished', 'Everything']))
  })

  it('leaves every round of a merged pull request out of In play', async () => {
    const page = await shown(Reviews)
    expect(page.text()).toContain('Every review in play')
    expect(page.text(), 'no round of a merged pull request is in play')
      .not.toContain('#9568')
    /* And the pull requests that are still open are there, so this is not a dead page. */
    expect(page.text()).toContain('#9541')
  })

  it('lists all four rounds under Finished, posted or not', async () => {
    /*
     * **Four, and only one of them was ever harvested.** The other three hold no
     * `pr_state`, so a page reading the round's own column would show one row here and
     * leave three rounds reachable from nowhere at all.
     */
    const page = await shown(Reviews)
    await choose(page, 'Finished')

    expect(page.text()).toContain('The reviews whose pull request merged or closed')
    const rows = page.findAll('.queue-line').filter(row => row.text().includes('#9568'))
    expect(rows, 'every round of #9568').toHaveLength(4)
    for (const id of [MERGED.posted, ...MERGED.unposted]) {
      expect(
        page.findAll('.queue-line .row').map(row => row.attributes('href')),
        `round ${id} is reachable from the list`,
      ).toContain(`/reviews/${id}/understand`)
    }
  })

  it('says the pull request merged, and when, rather than that it was dismissed', async () => {
    /*
     * **The two ways off the desk are told apart in words**, which is the issue's own
     * criterion. "Dismissed" over a merge would put a decision in the developer's mouth
     * that nobody made.
     */
    const page = await shown(Reviews)
    await choose(page, 'Finished')

    const row = page.findAll('.queue-line').find(one => one.text().includes('#9568'))!
    expect(row.text()).toContain('Finished — the pull request merged')
    expect(row.text()).toContain('No round of it can be posted')
    expect(row.text(), 'and nobody dismissed it').not.toContain('Dismissed')
    /* The day the record holds, worded as a day rather than as an elapsed figure. */
    expect(row.text()).toMatch(/merged on \d{1,2} [A-Z][a-z]+ \d{4}/)
  })

  it('shows nothing finished as a state of the record, not as an empty database', async () => {
    await resetDesk({ reviews: [], drafts: [] })
    const page = await shown(Reviews)
    /* An empty database draws the fresh card, which is the other sentence entirely. */
    expect(page.text()).toContain('No reviews in play')
  })
})

describe('the brief of a review that cannot be posted', () => {
  /*
   * **Confirming a brief is a sign-off too**, and it is one of the four acts the store
   * refuses on a review that has left the desk. The card went on offering the button after
   * that guard was widened — the one failure the issue's own invariant exists to prevent,
   * and the reason `GET /reviews/{id}` carries `not_for_signing` beside the draft page's.
   */
  it('offers no confirmation, and says why instead', async () => {
    const page = await shown(Understand, {
      route: `/reviews/${MERGED.unposted[0]}/understand`,
    })
    expect(page.findAll('button').map(one => one.text()))
      .not.toContain('Confirm the problem')
    expect(page.text()).toContain('Nothing to confirm')
    expect(page.text()).toContain('The pull request merged on')
  })

  it('refuses it in the API too, with the same reason', async () => {
    const refused = await useReviewDesk().confirmBrief(MERGED.unposted[0]!)
    expect(refused.ok).toBe(false)
    expect(refused.error?.code).toBe('conflict')
    expect(refused.error?.message).toContain('merged')
  })

  it('still offers it on a review that is in play', async () => {
    /* Review 17's brief is unconfirmed and its pull request is open. */
    const page = await shown(Understand, { route: '/reviews/17/understand' })
    expect(page.findAll('button').map(one => one.text())).toContain('Confirm the problem')
    expect(page.text()).not.toContain('Nothing to confirm')
  })
})

describe('the draft page of a review that cannot be posted', () => {
  it('says the pull request merged, and on which day', async () => {
    const page = await draftOf(MERGED.unposted[0]!)
    /*
     * **And the heading does not instruct.** "Read it, then sign it off." over a draft this
     * page is refusing to let anybody sign off is the page contradicting itself in its
     * largest type.
     */
    expect(page.text()).toContain('Nothing left to do.')
    expect(page.text()).not.toContain('Read it, then sign it off')
    expect(page.text()).toContain('Not posted — the pull request merged on')
    expect(page.text()).toMatch(/merged on \d{1,2} [A-Z][a-z]+ \d{4}/)
    expect(page.text()).toContain('No round of it can be posted now')
  })

  it('reads its step trail in the past tense too', async () => {
    /*
     * **"Nothing left to do." two lines above "6 Draft · waiting on you"** is the same
     * contradiction the Reviews list carried, on the page a developer is most likely to be
     * reading when they wonder what happened. The record is untouched — step 6 really is
     * `waiting_on_you` — and only the tense the page reads it in changes.
     */
    const page = await draftOf(MERGED.unposted[0]!)
    expect(page.text()).toContain('was waiting on you')
    expect(page.text()).not.toContain('Draftwaiting on you')
  })

  it('offers no sign-off, and no way to send it back either', async () => {
    /*
     * **Not a disabled button.** Each of the three acts the panel offers asks a session to
     * do something afterwards — post it, file a ticket and post it, rewrite it — and on a
     * merged pull request no session is coming for any of them. A greyed-out "Sign off and
     * post" would still be offering the act, and the API refuses it.
     */
    const page = await draftOf(MERGED.unposted[0]!)
    const labels = page.findAll('button').map(one => one.text())

    expect(page.text()).toContain('Nothing to sign off')
    expect(labels).not.toContain('Sign off and post')
    expect(labels.some(label => label.includes('Send back'))).toBe(false)
    expect(labels.some(label => label.includes('Ticket what is left'))).toBe(false)
    /* And the draft itself is still there to read, which is what "kept" means. */
    expect(page.text()).toContain('The draft, the findings and your decisions are all still')
  })

  it('still offers the sign-off on the round of a pull request that is open', async () => {
    /*
     * The converse, and the one that would fail silently: a guard that answered "finished"
     * for a pull request nobody has harvested would take the sign-off off every draft on a
     * fresh database. Round 2 of `#9541` is open and unharvested.
     */
    const page = await draftOf(61)
    expect(page.findAll('button').map(one => one.text())).toContain('Sign off and post')
    expect(page.text()).not.toContain('Nothing to sign off')
  })

  it('says a superseded round is superseded, and names the round that can post', async () => {
    /*
     * `tc/portal#9541` round 1, which drew the second of Patrick's two rows for one pull
     * request. Its pull request is open, so this is the supersession and not the merge.
     */
    const page = await draftOf(60)
    expect(page.text()).toContain('Not posted — superseded by round 2')
    expect(page.text()).toContain('The author pushed past the commit')
    expect(page.findAll('button').map(one => one.text())).not.toContain('Sign off and post')
  })

  it('says the pull request merged on a round that is both merged and superseded', async () => {
    /*
     * Round 1 of #9568 posted, so it is not superseded; rounds 2 and 3 are both superseded
     * *and* on a merged pull request. The merge is the bigger fact — once the pull request
     * is over, which round is newest no longer matters — and the page says that one.
     */
    const page = await draftOf(MERGED.unposted[0]!)
    expect(page.text()).toContain('the pull request merged')
    expect(page.text()).not.toContain('superseded by round')
  })

  it('refuses the sign-off itself, with the same reason the page draws', async () => {
    /*
     * **The page offers no button, so this is the race the hidden button cannot cover**: a
     * pull request that merges between the page's read and the press. Sample mode raises
     * the refusals the binary raises — that is what makes a refusal state tested here one
     * that works against the record — so the reason is asserted, not only the code.
     */
    const desk = useReviewDesk()

    const merged = await desk.approveDraft(MERGED.unposted[0]!, 'comment')
    expect(merged.ok).toBe(false)
    expect(merged.error?.code).toBe('conflict')
    expect(merged.error?.message).toContain('merged')

    const superseded = await desk.approveDraft(60, 'comment')
    expect(superseded.ok).toBe(false)
    expect(superseded.error?.code).toBe('conflict')
    expect(superseded.error?.message).toContain('round 2')

    /* And sending it back is refused the same way: it also asks a session to act. */
    const sentBack = await desk.sendBackDraft(MERGED.unposted[0]!, 'raise the retry finding')
    expect(sentBack.ok).toBe(false)
    expect(sentBack.error?.message).toContain('merged')

    /* The round that can post is refused nothing. */
    const live = await desk.approveDraft(61, 'comment')
    expect(live.ok, JSON.stringify(live.error)).toBe(true)
  })

  it('reads as posted rather than as not posted on the round that did post', async () => {
    /*
     * **A review that posted did post.** The sentence DEV-957 asks for is about a draft
     * that never will, and "Not posted" over a review that is on GitHub would be the page
     * contradicting the record.
     */
    const page = await draftOf(MERGED.posted)
    expect(page.text()).toContain('Posted.')
    expect(page.text()).not.toContain('Not posted —')
    expect(page.text()).not.toContain('Nothing to sign off')
  })
})
