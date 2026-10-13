// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'

import Understand from '../app/pages/reviews/[id]/understand.vue'
import Alternatives from '../app/pages/reviews/[id]/alternatives.vue'
import Solution from '../app/pages/reviews/[id]/solution.vue'
import CodeReview from '../app/pages/reviews/[id]/code-review.vue'
import Draft from '../app/pages/reviews/[id]/draft.vue'
import WhatHappened from '../app/pages/reviews/[id]/what-happened.vue'

/*
 * **The round switcher, on every step page. DEV-976.**
 *
 * A review is one round of a pull request and that was a dead end: round 2 is a new record
 * and from inside it there was no way back to round 1. Patrick, 14 October 2026: "there's
 * no way for me to actually go back to the previous round… especially when I go and look at
 * the understand, the alternatives, the solution map, and even the code review. It's really
 * important to be able to see where we've come from, not just this specific point in time."
 *
 * ## It is mounted on all six pages rather than on the component
 *
 * The acceptance criterion is "**every** step page of a multi-round review shows the round
 * switcher", and that is a claim about the six pages, not about `RoundSwitcher`. A component
 * test would pass with the switcher wired into five of them. So each case here runs the
 * whole list, and a page added to `ReviewStepPage` later is one line from being covered.
 *
 * ## The fixtures are the ones `app/data/reviews.json` already holds
 *
 * No invented round is needed, because the sample records already carry three pull requests
 * with more than one round and the backend derives `rounds` from them — which is also what
 * makes this a test of the derivation and not of a literal:
 *
 * | Fixture | Rounds | What it is here for |
 * |---|---|---|
 * | `globex/portal#9568` | 4 (reviews 64–67) | round 1 posted, round 2 wrote the brief, round 4 unposted and newest |
 * | `globex/portal#9541` | 2 (reviews 60, 61) | round 2 points back at round 1 |
 * | `globex/portal#9585` | 2 (reviews 62, 63) | no round wrote a brief, so no page claims one did |
 * | `acme/portal#514` | 1 (review 41) | a single-round review draws no switcher at all |
 *
 * **What a jsdom test cannot check is the layout.** "It fits at 390 px with no horizontal
 * scroll" is computed geometry and would pass here against any stylesheet;
 * `e2e/rounds.spec.ts` measures it in a real browser. This file checks that the rule is
 * stated and the links are right; the browser checks that it holds.
 */

/** #9568 round 3: no brief of its own, and round 2 is the round that wrote one. */
const THIRD_OF_FOUR = 66
/** #9568 round 2: the round that wrote the brief. */
const SECOND_OF_FOUR = 65
/** #9568 round 1: posted, no brief, and nothing before it to point at. */
const FIRST_OF_FOUR = 64
/** #9568 round 4: the newest round, unposted — the one being worked on. */
const FOURTH_OF_FOUR = 67
/** #9541 round 2, whose round 1 wrote the brief. */
const SECOND_OF_TWO = 61
/** #9585 round 2, where no round of the pull request wrote a brief. */
const NEITHER_WROTE_ONE = 63
/** #514, the real-length review, and the only round of its pull request. */
const ONE_ROUND = 41

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

/** The six step pages, each with the suffix its route carries. */
const PAGES = [
  { step: 'Understand', suffix: 'understand', component: Understand },
  { step: 'Alternatives', suffix: 'alternatives', component: Alternatives },
  { step: 'Solution map', suffix: 'solution', component: Solution },
  { step: 'Code review', suffix: 'code-review', component: CodeReview },
  { step: 'Draft', suffix: 'draft', component: Draft },
  { step: 'What happened', suffix: 'what-happened', component: WhatHappened },
] as const

/** The three pages the brief feeds, which are the three a later round can hold none of. */
const FROM_THE_BRIEF = PAGES.slice(0, 3)
/** The three whose record is the round's own, whatever any earlier round holds. */
const ITS_OWN = PAGES.slice(3)

async function shown(page: (typeof PAGES)[number], id: number) {
  const mounted = await mountSuspended(page.component, { route: `/reviews/${id}/${page.suffix}` })
  await settle(mounted)
  return mounted
}

/** The switcher on a page, or null where it drew none. */
function switcher(page: VueWrapper) {
  const found = page.find('nav.rounds')
  return found.exists() ? found : null
}

/** Every round the switcher drew, as its text and where it goes. */
function chips(page: VueWrapper) {
  return page.findAll('nav.rounds .round').map(one => ({
    text: one.text(),
    to: one.attributes('href') ?? null,
    current: one.attributes('aria-current') ?? null,
    tag: one.element.tagName,
  }))
}

beforeEach(async () => {
  await resetDesk()
})

describe('every step page of a multi-round review', () => {
  /** The acceptance criterion, run over the six pages rather than asserted of one. */
  it('shows the round switcher, saying which round of how many', async () => {
    for (const page of PAGES) {
      const mounted = await shown(page, THIRD_OF_FOUR)
      const found = switcher(mounted)
      expect(found, `${page.step} draws the switcher`).not.toBeNull()
      expect(found!.text(), `${page.step} says which round`).toContain('Round 3 of 4')
    }
  })

  it('names the pull request’s rounds in its own landmark', async () => {
    const mounted = await shown(PAGES[0], THIRD_OF_FOUR)
    expect(switcher(mounted)!.attributes('aria-label'))
      .toBe('The rounds of this pull request')
  })

  /**
   * **Every link opens the same step of the linked round.** Keeping your place is the whole
   * request: a reader on Alternatives in round 3 wants Alternatives in round 1.
   */
  it('links each other round to that round’s copy of this same step', async () => {
    for (const page of PAGES) {
      const mounted = await shown(page, THIRD_OF_FOUR)
      expect(chips(mounted).map(one => one.to), `${page.step}`).toEqual([
        `/reviews/${FIRST_OF_FOUR}/${page.suffix}`,
        `/reviews/${SECOND_OF_FOUR}/${page.suffix}`,
        null,
        `/reviews/${FOURTH_OF_FOUR}/${page.suffix}`,
      ])
    }
  })

  /** A link to the page you are on does nothing, and a keyboard user cannot tell. */
  it('draws the round being read as text, marked as the page you are on', async () => {
    for (const page of PAGES) {
      const here = chips(await shown(page, THIRD_OF_FOUR))[2]!
      expect(here.tag, `${page.step}`).toBe('SPAN')
      expect(here.current, `${page.step}`).toBe('page')
    }
  })

  /**
   * **Keyboard reach is the browser's**, which is the reason there is no button and no
   * handler here: every round is an anchor or a span, so the tab order and the global
   * `:focus-visible` ring are what they already are everywhere else, and Back works.
   */
  it('reaches every other round with an anchor rather than a handler', async () => {
    const mounted = await shown(PAGES[0], THIRD_OF_FOUR)
    const anchors = mounted.findAll('nav.rounds a')
    expect(anchors).toHaveLength(3)
    expect(mounted.findAll('nav.rounds button')).toHaveLength(0)
    for (const anchor of anchors) expect(anchor.attributes('href')).toMatch(/^\/reviews\/\d+\//)
  })

  describe('what each round says about where it stands', () => {
    /*
     * **`#9568` merged**, which is why its newest unposted round reads "not posted" rather
     * than "being worked on": nothing is being worked on once the pull request is over, and
     * three rounds of this very pull request sitting in "drafts to approve" was DEV-957.
     * The open pull request's wording is asserted below.
     */
    it('gives the posted round its day, and the rest of a merged one no promise', async () => {
      const drawn = chips(await shown(PAGES[0], THIRD_OF_FOUR)).map(one => one.text)
      expect(drawn[0], 'round 1 posted').toContain('posted 9 October 2026')
      expect(drawn[1], 'round 2 was pushed past').toContain('not posted')
      expect(drawn[2], 'round 3 was pushed past').toContain('not posted')
      expect(drawn[3], 'round 4 of a merged pull request').toContain('not posted')
      expect(drawn.join(' '), 'and none of them claims to be in hand')
        .not.toContain('being worked on')
    })

    /** The newest unposted round of a pull request still open is the one in hand. */
    it('calls the newest unposted round of an open pull request the one in hand', async () => {
      const drawn = chips(await shown(PAGES[0], SECOND_OF_TWO)).map(one => one.text)
      expect(drawn).toHaveLength(2)
      expect(drawn[1]).toContain('being worked on')
    })

    it('carries each round’s head commit at seven characters', async () => {
      const drawn = chips(await shown(PAGES[0], THIRD_OF_FOUR)).map(one => one.text)
      expect(drawn[0]).toContain('e5f6a75')
      expect(drawn[1]).toContain('f6a7b86')
      expect(drawn[2]).toContain('a7b8c97')
      expect(drawn[3]).toContain('b8c9d08')
    })

    /*
     * `RoundSummary` carries no dismissed flag on master — DEV-954's arrives with DEV-962 —
     * so nothing is marked, and nothing is marked "not dismissed" either. The seam is
     * `roundDismissed`, and `test/rounds.spec.ts` asserts both answers against a round that
     * carries the field.
     */
    it('marks no round dismissed while the wire carries no such flag', async () => {
      const mounted = await shown(PAGES[0], THIRD_OF_FOUR)
      expect(mounted.findAll('nav.rounds .tag')).toHaveLength(0)
    })
  })
})

describe('a review whose pull request has one round', () => {
  /** Every review is a round, so a timeline of one restates the page's own header. */
  it('shows no switcher on any of the six pages', async () => {
    for (const page of PAGES) {
      const mounted = await shown(page, ONE_ROUND)
      expect(switcher(mounted), `${page.step} draws none`).toBeNull()
      expect(mounted.text(), `${page.step} says no "of 1"`).not.toContain('Round 1 of 1')
    }
  })
})

describe('where a step was written, when this round did not write it', () => {
  /**
   * **The acceptance criterion, and the measurement behind it.** Nothing carries a brief
   * between rounds: `briefs` is keyed by the review id, as is every record behind steps 1
   * to 4, and opening a round copies none of it. So the state the record has is "this round
   * wrote none and an earlier round did" — said in words, with a link to that round.
   *
   * Round 2 rather than round 1, which is the case the contract field exists for: round 1 of
   * `#9568` wrote no brief and round 2 did, so guessing the first round would be wrong here.
   */
  it('says so and links to the round that wrote the last one', async () => {
    for (const page of FROM_THE_BRIEF) {
      const mounted = await shown(page, THIRD_OF_FOUR)
      const said = mounted.text()
      expect(said, `${page.step} says this round wrote none`)
        .toContain(`This round has written no ${page.step} of its own.`)
      expect(said, `${page.step} names round 2`).toContain('Round 2 wrote the last one')
      expect(said, `${page.step} says nothing has rewritten it`)
        .toContain('no round since has written another')
      const link = mounted.findAll('a')
        .find(one => one.text().includes(`Read ${page.step} in round 2`))
      expect(link, `${page.step} links to round 2`).toBeDefined()
      expect(link!.attributes('href')).toBe(`/reviews/${SECOND_OF_FOUR}/${page.suffix}`)
    }
  })

  /** The round that wrote the brief has one of its own, so there is nothing to point at. */
  it('says nothing on the round that wrote the brief', async () => {
    for (const page of FROM_THE_BRIEF) {
      const mounted = await shown(page, SECOND_OF_FOUR)
      expect(mounted.text(), `${page.step}`).not.toContain('has written no')
    }
  })

  /** A first round has nothing before it, whatever it holds. */
  it('says nothing on a first round', async () => {
    for (const page of FROM_THE_BRIEF) {
      const mounted = await shown(page, FIRST_OF_FOUR)
      expect(mounted.text(), `${page.step}`).not.toContain('has written no')
    }
  })

  it('says nothing where no round of the pull request wrote one', async () => {
    for (const page of FROM_THE_BRIEF) {
      const mounted = await shown(page, NEITHER_WROTE_ONE)
      expect(mounted.text(), `${page.step}`).not.toContain('has written no')
    }
  })

  it('points a two-round pull request’s second round back at its first', async () => {
    const mounted = await shown(PAGES[0], SECOND_OF_TWO)
    expect(mounted.text()).toContain('Round 1 wrote the last one')
    const link = mounted.findAll('a').find(one => one.text().includes('Read Understand in round 1'))
    expect(link).toBeDefined()
    expect(link!.attributes('href')).toBe('/reviews/60/understand')
  })

  /**
   * **Steps 5 to 7 never say they were written in another round.** A round's findings, its
   * draft and its harvest are its own, written against its own head commit; "written in
   * round 2" would be false about the one thing rounds exist to separate. Asserted on the
   * round that would otherwise say it, so the test fails if the allowlist widens.
   */
  it('never says a later step was written in another round', async () => {
    for (const page of ITS_OWN) {
      const mounted = await shown(page, THIRD_OF_FOUR)
      expect(switcher(mounted), `${page.step} still draws the switcher`).not.toBeNull()
      expect(mounted.text(), `${page.step}`).not.toContain('has written no')
      expect(mounted.text(), `${page.step}`).not.toContain('wrote the last one')
    }
  })
})
