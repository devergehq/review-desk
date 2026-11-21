import { describe, expect, it } from 'vitest'

import type {
  Brief,
  Outcome,
  Review,
  ReviewLayer,
  ReviewLayerName,
  ReviewLayerState,
} from '../app/types/desk'
import { loadSource, resetSource, sampleBackend } from '../app/data/sample'
import { sidebarGroups } from '../app/utils/sidebar'
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
  brief?: Partial<Brief> | null
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
    brief: (over.brief ?? null) as Brief | null,
  }
}

const UNCONFIRMED = { confirmed: false, confirmation_required: true }

describe('stateWord', () => {
  it('words every state, and no two alike', () => {
    const words = Object.values(STATE_WORDS)
    expect(new Set(words).size).toBe(words.length)
    expect(stateWord('draft_ready')).toBe('Draft ready')
    expect(stateWord('brief_ready')).toBe('Brief ready')
  })
})

describe('stateOfReview', () => {
  /*
   * **The same facts `GET /inbox` asks, in the same order** — `crates/store/src/entities/
   * inbox.rs` — so the header says the sidebar's word. These cases are the three the
   * self-review of #86 found disagreeing when this read the layers instead.
   */
  it('is waiting for a session when nothing asks anything of anyone', () => {
    expect(stateOfReview(source())).toBe('waiting_for_session')
    /* An approved draft with no session working: the contract's `awaiting_session`, case A. */
    expect(stateOfReview(source({
      draft_status: 'approved',
      layers: layers({ lint: 'done', understand: 'done', draft: 'done' }),
    }))).toBe('waiting_for_session')
  })

  it('is being written while one of layers 1 to 6 runs, and not for a harvest', () => {
    expect(stateOfReview(source({ layers: layers({ lint: 'done', understand: 'running' }) })))
      .toBe('being_written')
    expect(stateOfReview(source({ draft_status: 'posted', layers: layers({ harvest: 'running' }) })))
      .toBe('posted')
  })

  it('is brief ready while a brief that needs confirming is not confirmed', () => {
    expect(stateOfReview(source({ brief: UNCONFIRMED }))).toBe('brief_ready')
    expect(stateOfReview(source({ brief: { confirmed: false, confirmation_required: false } })))
      .toBe('waiting_for_session')
    expect(stateOfReview(source({ brief: { confirmed: true, confirmation_required: true } })))
      .toBe('waiting_for_session')
  })

  it('reads the brief before the draft, as the queue does (case B)', () => {
    expect(stateOfReview(source({ brief: UNCONFIRMED, draft_status: 'ready' }))).toBe('brief_ready')
  })

  it('is draft ready on the draft’s status, whatever its layer says (case C)', () => {
    expect(stateOfReview(source({
      draft_status: 'ready',
      layers: layers({ lint: 'done', understand: 'done', draft: 'pending' }),
    }))).toBe('draft_ready')
  })

  it('is replies in once the author or the harvester recorded something', () => {
    expect(stateOfReview(source({
      draft_status: 'posted',
      outcomes: [{ kind: 'kept', actor: 'developer' }],
    }))).toBe('posted')
    expect(stateOfReview(source({
      draft_status: 'posted',
      outcomes: [{ kind: 'replied', actor: 'author' }],
    }))).toBe('replies_in')
    expect(stateOfReview(source({
      draft_status: 'posted',
      outcomes: [{ kind: 'code_changed_after', actor: 'harvester' }],
    }))).toBe('replies_in')
  })

  it('reads the ways off the desk over everything, a dismissal first', () => {
    expect(stateOfReview(source({ draft_status: 'posted', review: { pr_state: 'merged' } })))
      .toBe('merged')
    expect(stateOfReview(source({ review: { pr_state: 'closed' } }))).toBe('closed')
    expect(stateOfReview(source({ review: { out_of_play: 'withdrawn' }, draft_status: 'posted' })))
      .toBe('out_of_play')
    expect(stateOfReview(source({
      review: { pr_state: 'merged', dismissed_at: '2026-11-20T10:00:00Z' },
    }))).toBe('dismissed')
  })
})

/*
 * **The contract this file exists for: one review, one word, whichever path drew it.**
 * Every review the sample desk's sidebar lists, read whole and run through
 * `stateOfReview`, says what its sidebar row says.
 */
describe('the sidebar and the header agree', () => {
  it('gives every sample review the same word by both paths', async () => {
    resetSource(loadSource())
    const backend = sampleBackend()
    const groups = sidebarGroups(await backend.getInbox(), await backend.listReviews(null, 'finished'))
    const rows = groups.flatMap(group => group.rows).filter(row => row.reviewId != null)
    expect(rows.length).toBeGreaterThan(5)

    for (const row of rows) {
      const detail = await backend.getReview(row.reviewId!)
      expect(stateWord(stateOfReview(detail)), `review ${row.reviewId} (#${row.pr})`)
        .toBe(stateWord(row.state))
    }
  })
})
