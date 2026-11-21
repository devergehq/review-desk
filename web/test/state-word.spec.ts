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
import { rowFor, sidebarGroups } from '../app/utils/sidebar'
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
    not_for_signing: null,
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
 *
 * Two halves. A review the sidebar lists takes the sidebar's row word in its header —
 * `stateOfReview(detail, rowFor(...))` — which is agreement by construction. And the same
 * review's *own facts*, asked in the inbox's order, give that word too: the half that
 * would catch `stateOfReview` drifting from `crates/store/src/entities/inbox.rs`.
 *
 * **The desk is bent into F1's three shapes first** (#86's self-review), because the sample
 * desk as it stands holds none of them and an agreement test over it could not tell the
 * layer-reading function this replaced from this one:
 *
 * | Case | Review | Bent to |
 * | -- | -- | -- |
 * | A | 18 | draft approved, nothing running: `awaiting_session` |
 * | B | 15 | brief unconfirmed, draft ready and its layer waiting: the brief wins |
 * | C | 17 | brief confirmed, draft `ready`, draft layer still `pending` |
 */
function bentDesk() {
  const desk = loadSource()
  const byId = (id: number) => desk.reviews.find(one => one.review.id === id)!
  const draftOf = (id: number) => desk.drafts.find(one => one.draft.review_id === id)

  /* A: an approved draft with nothing running. */
  const a = byId(18)
  a.draft_status = 'approved'
  a.layers = a.layers.map(layer => ({ ...layer, state: layer.state === 'running' ? 'done' : layer.state }))
  if (draftOf(18)) draftOf(18)!.draft.status = 'approved'

  /* B: the brief unconfirmed under a waiting draft. */
  const b = byId(15)
  b.brief = { ...b.brief!, confirmed: false, confirmation_required: true }

  /* C: a ready draft whose layer has not moved. */
  const c = byId(17)
  c.brief = { ...c.brief!, confirmed: true, confirmation_required: true }
  c.layers = c.layers.map(layer => layer.name === 'understand' ? { ...layer, state: 'done' } : layer)

  return desk
}

describe('the sidebar and the header agree', () => {
  it('lists the three bent shapes where the inbox puts them', async () => {
    resetSource(bentDesk())
    const backend = sampleBackend()
    const groups = sidebarGroups(await backend.getInbox(), await backend.listReviews(null, 'finished'))
    const stateOf = (id: number) => groups.flatMap(g => g.rows).find(row => row.reviewId === id)?.state
    expect(stateOf(18), 'case A').toBe('waiting_for_session')
    expect(stateOf(15), 'case B').toBe('brief_ready')
    expect(stateOf(17), 'case C').toBe('draft_ready')
  })

  it('gives every listed review the row’s word, by construction and on its own facts', async () => {
    resetSource(bentDesk())
    const backend = sampleBackend()
    const groups = sidebarGroups(await backend.getInbox(), await backend.listReviews(null, 'finished'))
    const rows = groups.flatMap(group => group.rows).filter(row => row.reviewId != null)
    expect(rows.length).toBeGreaterThan(5)

    for (const row of rows) {
      const detail = await backend.getReview(row.reviewId!)
      const label = `review ${row.reviewId} (#${row.pr})`
      expect(stateWord(stateOfReview(detail, rowFor(groups, detail.review))), label)
        .toBe(stateWord(row.state))
      expect(stateWord(stateOfReview(detail)), `${label}, on its own facts`)
        .toBe(stateWord(row.state))
    }
  })

  /*
   * **A reply on round 1 while round 3 waits** (#86's self-review, round 2): the inbox
   * lists the pull request under Replies in and opens round 3, whose own facts hold no
   * reply. The header takes the row's word, which is the only way it can know.
   */
  it('takes the row’s word for the round the row opens, and only for that round', () => {
    const round3 = source({ review: { id: 3 } })
    expect(stateOfReview(round3)).toBe('waiting_for_session')
    expect(stateOfReview(round3, { reviewId: 3, state: 'replies_in' })).toBe('replies_in')
    expect(stateOfReview(round3, { reviewId: 1, state: 'replies_in' })).toBe('waiting_for_session')
  })

  /* N2: the pull request's end, read off the pull request rather than this round. */
  it('reads a finished or superseded round from `not_for_signing`', () => {
    expect(stateOfReview({ ...source(), not_for_signing: { reason: 'finished', how: 'merged' } }))
      .toBe('merged')
    expect(stateOfReview({ ...source(), not_for_signing: { reason: 'superseded', by_round: 2 } }))
      .toBe('superseded')
  })
})
