import { describe, expect, it } from 'vitest'

import type { Outcome, Review, ReviewLayer, ReviewLayerName, ReviewLayerState } from '../app/types/desk'
import { STATE_WORDS, stateOfReview, stateWord, type StateSource } from '../app/utils/stateWord'

/*
 * **One state word per review, the same everywhere it appears** — `docs/design.md`, *The
 * application shell*. The sidebar (DEV-1323) and the review header (DEV-1324) both draw it,
 * and both go through `stateWord`; these are the rules `stateOfReview` works one out by.
 */

const NAMES: ReviewLayerName[] = [
  'lint',
  'understand',
  'alternatives',
  'solution_map',
  'code_review',
  'draft',
  'harvest',
]

function layers(states: Partial<Record<ReviewLayerName, ReviewLayerState>> = {}): ReviewLayer[] {
  return NAMES.map((name, at) => ({ number: at + 1, name, state: states[name] ?? 'pending' }))
}

function source(over: {
  review?: Partial<Review>
  layers?: ReviewLayer[]
  outcomes?: Partial<Outcome>[]
  draft_status?: StateSource['draft_status']
} = {}): StateSource {
  return {
    review: {
      id: 1,
      repo: 'acme/portal',
      pr: 514,
      head_sha: '7f3c2d1',
      round: 1,
      ticket_url: null,
      created_at: '2026-11-20T09:00:00Z',
      pr_state: null,
      harvested_at: null,
      pr_state_at: null,
      dismissed_at: null,
      dismissed_reason: null,
      out_of_play: null,
      out_of_play_at: null,
      ...over.review,
    } as Review,
    layers: over.layers ?? layers(),
    outcomes: (over.outcomes ?? []) as Outcome[],
    draft_status: over.draft_status ?? null,
  }
}

describe('stateWord', () => {
  it('words every state, and no two alike', () => {
    const words = Object.values(STATE_WORDS)
    expect(new Set(words).size).toBe(words.length)
    expect(stateWord('draft_ready')).toBe('Draft ready')
    expect(stateWord('brief_ready')).toBe('Brief ready')
  })
})

describe('stateOfReview', () => {
  it('is not started while every layer is pending', () => {
    expect(stateOfReview(source())).toBe('not_started')
  })

  it('is being written once any layer has run', () => {
    expect(stateOfReview(source({ layers: layers({ lint: 'done', understand: 'running' }) })))
      .toBe('being_written')
  })

  it('is brief ready when the understand layer waits on the developer', () => {
    expect(stateOfReview(source({ layers: layers({ lint: 'done', understand: 'waiting_on_you' }) })))
      .toBe('brief_ready')
  })

  it('is draft ready when the draft layer waits, even with the brief still waiting', () => {
    expect(stateOfReview(source({
      layers: layers({ understand: 'waiting_on_you', draft: 'waiting_on_you' }),
    }))).toBe('draft_ready')
  })

  it('is posted once the draft is, and replies in once something came back', () => {
    expect(stateOfReview(source({ draft_status: 'posted' }))).toBe('posted')
    expect(stateOfReview(source({
      draft_status: 'posted',
      outcomes: [{ kind: 'kept', actor: 'developer' }],
    }))).toBe('posted')
    expect(stateOfReview(source({
      draft_status: 'posted',
      outcomes: [{ kind: 'replied', actor: 'author' }],
    }))).toBe('replies_in')
  })

  it('reads the pull request’s end over posting', () => {
    expect(stateOfReview(source({ draft_status: 'posted', review: { pr_state: 'merged' } })))
      .toBe('merged')
    expect(stateOfReview(source({ review: { pr_state: 'closed' } }))).toBe('closed')
    expect(stateOfReview(source({ review: { pr_state: 'open' }, draft_status: 'posted' })))
      .toBe('posted')
  })

  it('reads a dismissal over everything, as `LeftTheDesk` does', () => {
    expect(stateOfReview(source({
      review: { pr_state: 'merged', dismissed_at: '2026-11-20T10:00:00Z' },
    }))).toBe('dismissed')
  })
})
