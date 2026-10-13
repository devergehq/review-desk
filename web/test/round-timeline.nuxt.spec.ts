// @vitest-environment nuxt
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import RoundTimeline from '../app/components/RoundTimeline.vue'
import { nothingMoved } from '../app/utils/rounds'

import type { HeadUpdates, RoundSummary } from '../app/types/desk'

/*
 * **The two things the record could say about a round and no screen did.**
 *
 * DEV-954: the timeline drew a dismissed round exactly like a live one, because every other
 * field on a round summary is a count and a dismissed round's counts look the same. DEV-962:
 * a round's head moves now instead of a second round opening, so a page has to be able to
 * say the round is no longer of the commit the developer was last shown.
 *
 * The arithmetic is `test/rounds.spec.ts`; this is the component, and the real browser
 * measurement at 1440 is `e2e/review-desk.spec.ts` against the seeded binary.
 */

function round(over: Partial<RoundSummary> & { round: number }): RoundSummary {
  return {
    review_id: 100 + over.round,
    head_sha: `${over.round}aaaaaa`,
    created_at: '2026-10-08T10:31:00Z',
    dismissed: false,
    posted_at: null,
    event: null,
    findings: 0,
    commits: 0,
    replies: 0,
    threads_resolved: 0,
    statuses: { new: 0, fixed: 0, still_open: 0, accepted: 0, withdrawn: 0, ticketed: 0 },
    ...over,
  }
}

async function timeline(rounds: RoundSummary[], updates: HeadUpdates = nothingMoved()) {
  return mountSuspended(RoundTimeline, {
    props: { rounds, current: rounds[rounds.length - 1]!.review_id, updates },
  })
}

describe('the round timeline', () => {
  it('draws a dismissed round as dismissed, in words and not only in styling', async () => {
    const page = await timeline([
      round({ round: 1, posted_at: '2026-10-09T08:40:00Z', findings: 6 }),
      round({ round: 2, dismissed: true }),
      round({ round: 3 }),
    ])

    const chips = page.findAll('.chip')
    expect(chips).toHaveLength(3)
    expect(chips[1]!.classes()).toContain('chip--dismissed')
    expect(chips[1]!.text()).toContain('dismissed')
    /*
     * **The word, not the line through it.** A reader with no colour and a screen reader get
     * the same sentence, which is the rule every pill on this desk follows — the styling is
     * the glance and the text is the meaning.
     */
    expect(chips[0]!.classes()).not.toContain('chip--dismissed')
    expect(chips[0]!.text()).toContain('posted')
    expect(chips[2]!.text()).toContain('this one')
  })

  it('keeps the dismissed round in the list rather than leaving a hole', async () => {
    // A hole where round 2 was would read as a gap in the record, which is why
    // `list_reviews` keeps dismissed rounds and why the word goes on the chip instead.
    const page = await timeline([
      round({ round: 1 }),
      round({ round: 2, dismissed: true }),
      round({ round: 3 }),
    ])
    expect(page.findAll('.chip').map(chip => chip.attributes('data-round')))
      .toEqual(['1', '2', '3'])
  })

  it('says it was updated for the commits since the round was opened', async () => {
    const page = await timeline([round({ round: 1 })], {
      ...nothingMoved(),
      since_sha: '4d9464f',
      moved: 1,
      commits: 3,
      last_at: '2026-10-11T07:12:00Z',
      updates: [
        { kind: 'moved', from_sha: '4d9464f', to_sha: 'a41c9e2', at: '2026-10-11T07:12:00Z' },
      ],
    })
    expect(page.text()).toContain('Updated for 3 commits since 4d9464f')
  })

  it('says a base-branch merge changed nothing', async () => {
    const page = await timeline([round({ round: 1 })], {
      ...nothingMoved(),
      base_only: 2,
      updates: [
        { kind: 'base_only', from_sha: '4d9464f', to_sha: '7e2a9d3', at: '2026-10-11T07:12:00Z' },
        { kind: 'base_only', from_sha: '4d9464f', to_sha: '8f3b1c4', at: '2026-10-11T09:00:00Z' },
      ],
    })
    expect(page.text()).toContain('brought the base branch in and changed nothing')
    expect(page.text()).not.toContain('Updated for')
  })

  it('draws nothing at all for a first round nobody has pushed to', async () => {
    // A timeline of one chip saying "this one" is furniture.
    const page = await timeline([round({ round: 1 })])
    expect(page.find('.timeline').exists()).toBe(false)
  })
})
