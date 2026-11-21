/*
 * **One state word per review, the same word everywhere that review appears.**
 *
 * `docs/design.md`, *The application shell*: the header's first rule is "one state word, the
 * same word everywhere that review appears". The sidebar draws it under every pull request
 * and the review header draws it beside the title, so the word is chosen here and nowhere
 * else — two places choosing it would be two chances to call one review "Draft ready" in the
 * sidebar and "Posted" in its own header.
 *
 * Two derivations and one vocabulary. The sidebar reads `GET /inbox`, which has already
 * sorted every pull request into arrays, so its state is the array the item came from
 * (`utils/sidebar.ts`). The header reads one `GET /reviews/{id}`, which is not sorted into
 * anything, so its state is worked out from the review's own layers by [`stateOfReview`]
 * below. Both answer a [`ReviewState`]; only [`stateWord`] turns one into words.
 *
 * Nothing here reads or writes a record. It takes what the API served and answers a value,
 * which is what makes `test/state-word.spec.ts` a test of the rules.
 */

import type { OutcomeKind, ReviewDetail } from '~/types/desk'

/**
 * Where a review stands, as the record can say it.
 *
 * **Every value is a fact the record holds**, never an estimate. `not_started` is a pull
 * request somebody asked for that no step has written to; `being_written` is a session
 * working through the steps; the two `_ready` states are the two layers that wait on the
 * developer; `posted` and `replies_in` are after the draft went to GitHub; `merged`,
 * `closed` and `dismissed` are the three ways off the desk.
 */
export type ReviewState
  = | 'not_started'
    | 'being_written'
    | 'brief_ready'
    | 'draft_ready'
    | 'posted'
    | 'replies_in'
    | 'merged'
    | 'closed'
    | 'dismissed'

/**
 * The words, written once.
 *
 * `Brief ready`, `Draft ready`, `Posted` and `Replies in` are the S screens' own words — the
 * S2 header's pill and the stages of *Where it stands*. The rest are the plainest words for
 * the state that the S screens do not draw.
 */
export const STATE_WORDS: Record<ReviewState, string> = {
  not_started: 'Not started',
  being_written: 'Being written',
  brief_ready: 'Brief ready',
  draft_ready: 'Draft ready',
  posted: 'Posted',
  replies_in: 'Replies in',
  merged: 'Merged',
  closed: 'Closed',
  dismissed: 'Dismissed',
}

/** The one word for a state. The only route from a [`ReviewState`] to a screen. */
export function stateWord(state: ReviewState): string {
  return STATE_WORDS[state]
}

/**
 * What came back from the author, as the harvester records it.
 *
 * The same three kinds `GET /inbox` builds its `author_replies` array from, so a review the
 * sidebar lists under *Replies in* says *Replies in* in its own header too.
 */
const CAME_BACK: ReadonlySet<OutcomeKind> = new Set<OutcomeKind>([
  'replied',
  'thread_resolved',
  'code_changed_after',
])

/** The parts of one review this needs, so a test can hand it no more than that. */
export type StateSource = Pick<ReviewDetail, 'review' | 'layers' | 'outcomes' | 'draft_status'>

/**
 * Where one review stands, from `GET /reviews/{id}`.
 *
 * **Read in the order a review leaves the desk, newest fact first**, so the answer is the
 * latest thing true of it: a dismissal outranks everything (it is an act the developer took,
 * the rule `LeftTheDesk` states), then the pull request's end, then posting, then the two
 * layers that wait on the developer, then any step having run at all.
 *
 * The draft is checked before the brief: a brief left `waiting_on_you` under a draft that
 * is also ready is a round whose newer question is the draft.
 */
export function stateOfReview(detail: StateSource): ReviewState {
  const { review, layers, outcomes, draft_status: draftStatus } = detail

  if (review.dismissed_at) return 'dismissed'
  if (review.pr_state === 'merged') return 'merged'
  if (review.pr_state === 'closed') return 'closed'

  if (draftStatus === 'posted') {
    return outcomes.some(outcome => CAME_BACK.has(outcome.kind)) ? 'replies_in' : 'posted'
  }

  const stateOf = (name: string) => layers.find(layer => layer.name === name)?.state
  if (stateOf('draft') === 'waiting_on_you') return 'draft_ready'
  if (stateOf('understand') === 'waiting_on_you') return 'brief_ready'

  return layers.some(layer => layer.state !== 'pending') ? 'being_written' : 'not_started'
}
