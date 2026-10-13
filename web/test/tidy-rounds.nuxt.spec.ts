// @vitest-environment nuxt
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import TidyRounds from '../app/components/TidyRounds.vue'

import type { DuplicateRoundsPage, ExtraRound } from '../app/types/desk'

/*
 * **The tidy shows before it asks, and asks before it removes.**
 *
 * DEV-962's fourth item: it "shows each pull request's extra unposted rounds and folds them
 * into the newest (or dismisses them); nothing is deleted without confirmation". Three
 * claims are testable here and each one is a way the screen could quietly be wrong: a row
 * that did not say what it holds, a fold that removed a round on one press, and a fold
 * offered for a round the API would refuse.
 */

function extra(over: Partial<ExtraRound> & { round: number }): ExtraRound {
  return {
    review_id: 100 + over.round,
    head_sha: `${over.round}aaaaaa`,
    created_at: '2026-10-08T10:31:00Z',
    holds: 'nothing',
    act: 'fold',
    ...over,
  }
}

function page(extras: ExtraRound[]): DuplicateRoundsPage {
  return {
    pull_requests: [
      {
        repo: 'acme/portal',
        pr: 412,
        title: 'Retry failed webhook deliveries',
        newest: { review_id: 124, round: 4, head_sha: 'eeb41d8' },
        extras,
      },
    ],
    pull_requests_affected: 1,
    extra_rounds: extras.length,
  }
}

describe('the duplicate-round tidy', () => {
  it('says what every extra round holds before it offers anything', async () => {
    const screen = await mountSuspended(TidyRounds, {
      props: {
        page: page([
          extra({ round: 1, holds: 'a brief and 6 findings', act: 'dismiss' }),
          extra({ round: 2 }),
        ]),
      },
    })

    const rows = screen.findAll('.round')
    expect(rows).toHaveLength(2)
    expect(rows[0]!.text()).toContain('holds a brief and 6 findings')
    expect(rows[1]!.text()).toContain('holds nothing')
  })

  it('offers a fold only where the proposal says fold', async () => {
    // The server decides which act a round gets, from the question a delete asks. A page
    // that judged for itself would eventually offer a fold the API refuses.
    const screen = await mountSuspended(TidyRounds, {
      props: {
        page: page([
          extra({ round: 1, holds: 'a draft', act: 'dismiss' }),
          extra({ round: 2 }),
        ]),
      },
    })

    const rows = screen.findAll('.round')
    expect(rows[0]!.text()).toContain('Dismiss it')
    expect(rows[0]!.text()).not.toContain('Fold it')
    expect(rows[1]!.text()).toContain('Fold it into round 4')
  })

  it('asks a second time before folding, and names the round', async () => {
    // A fold cannot be undone, so it is the one act here that asks twice — and the second
    // press names the round, because "Fold" on its own is a word.
    const screen = await mountSuspended(TidyRounds, {
      props: { page: page([extra({ round: 2 })]) },
    })

    await screen.find('.round button').trigger('click')
    const row = screen.find('.round')
    expect(row.text()).toContain('cannot be undone')
    expect(row.text()).toContain('Fold round 2 into round 4')
    expect(row.text()).toContain('Keep it')
  })

  it('says how many pull requests and rounds it is about', async () => {
    const screen = await mountSuspended(TidyRounds, {
      props: { page: page([extra({ round: 1 }), extra({ round: 2 })]) },
    })
    expect(screen.text()).toContain('1 pull request')
    expect(screen.text()).toContain('2 extra rounds')
  })
})
