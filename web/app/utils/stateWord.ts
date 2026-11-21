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

import type { Actor, ReviewDetail } from '~/types/desk'

/**
 * Where a review stands, as the record can say it.
 *
 * **Every value is a fact the record holds**, never an estimate. `not_started` is a pull
 * request GitHub asked for that no round exists of; `waiting_for_session` is a round on
 * the desk that no session is working on and nothing asks the developer about — which is
 * **not** the same as nothing having touched it (the contract's `awaiting_session`);
 * `being_written` is a session working through the steps; the two `_ready` states and
 * `replies_in` are the three that wait on the developer; `posted` is waiting on the author;
 * `merged`, `closed`, `dismissed` and `out_of_play` are the ways off the desk.
 */
export type ReviewState
  = | 'not_started'
    | 'waiting_for_session'
    | 'being_written'
    | 'brief_ready'
    | 'draft_ready'
    | 'posted'
    | 'replies_in'
    | 'merged'
    | 'closed'
    | 'dismissed'
    | 'out_of_play'
    | 'superseded'

/**
 * The words, written once.
 *
 * `Brief ready`, `Draft ready`, `Posted` and `Replies in` are the S screens' own words — the
 * S2 header's pill and the stages of *Where it stands*. `Waiting for a session` is the
 * queue's own word for the same rows. The rest are the plainest words for states the S
 * screens do not draw, and the PR that added them lists them under *Departs from the design*.
 */
export const STATE_WORDS: Record<ReviewState, string> = {
  not_started: 'Not started',
  waiting_for_session: 'Waiting for a session',
  being_written: 'Being written',
  brief_ready: 'Brief ready',
  draft_ready: 'Draft ready',
  posted: 'Posted',
  replies_in: 'Replies in',
  merged: 'Merged',
  closed: 'Closed',
  dismissed: 'Dismissed',
  out_of_play: 'Out of play',
  superseded: 'Superseded',
}

/** The one word for a state. The only route from a [`ReviewState`] to a screen. */
export function stateWord(state: ReviewState): string {
  return STATE_WORDS[state]
}

/**
 * Who has to have recorded something for a review to be in the inbox's `author_replies`:
 * the author, or the harvester reading what the author did. The same two actors
 * `reviews_with_author_activity` in `crates/store/src/entities/inbox.rs` asks for.
 */
const CAME_BACK_FROM: ReadonlySet<Actor> = new Set<Actor>(['author', 'harvester'])

/** The layer a harvest keeps `running` while a pull request is open, which is nobody working. */
const HARVEST = 'harvest'

/** The parts of one review this needs, so a test can hand it no more than that. */
export type StateSource = Pick<
  ReviewDetail,
  'review' | 'layers' | 'outcomes' | 'draft_status' | 'brief' | 'not_for_signing'
>

/**
 * What the sidebar lists for a pull request: the round its row opens, and the row's state.
 * `SidebarRow` in `utils/sidebar.ts` is one; `rowFor` there finds it.
 */
export interface ListedAs {
  reviewId: number | null
  state: ReviewState
}

/**
 * Where one review stands, as its header says it.
 *
 * **When the sidebar lists the review's pull request and this is the round its row opens,
 * the answer is the row's state** — `listed`, from `rowFor` in `utils/sidebar.ts`. That is
 * the one-word rule by construction rather than by agreement: the inbox decides a pull
 * request's state from more than one round (a reply left on round 1 while round 3 waits,
 * a running layer on an older round, what a later posted round already answered), and no
 * single `GET /reviews/{id}` can see those rounds. Asking the same answer twice is the only
 * way two places cannot differ.
 *
 * **Otherwise the review's own facts**, for a round the sidebar does not open (a superseded
 * one) or a pull request no group holds. They are asked in the inbox's order, so a review
 * that *is* listed would get the same word from them on its own facts:
 *
 * | Order | Fact | Inbox array | State |
 * | -- | -- | -- | -- |
 * | 1 | Dismissed; out of play; the pull request finished (`not_for_signing`, read off the pull request, not this round); superseded by a later round | none: off the desk | the way it left |
 * | 2 | A brief that needs confirming and is not confirmed | `briefs_to_confirm` | `brief_ready` |
 * | 3 | The draft's status is `ready` | `drafts_to_approve` | `draft_ready` |
 * | 4 | The author or the harvester recorded something | `author_replies` | `replies_in` |
 * | 5 | One of layers 1–6 is `running` | `in_progress` | `being_written` |
 * | 6 | The draft is posted | `posted_awaiting_author` | `posted` |
 * | 7 | Anything else | `awaiting_session` | `waiting_for_session` |
 *
 * Order 2 before 3 is the queue's: a review with an unconfirmed brief and a ready draft is
 * one row, and the brief is the next thing the developer does.
 */
export function stateOfReview(detail: StateSource, listed?: ListedAs | null): ReviewState {
  const { review, layers, outcomes, brief } = detail
  if (listed && listed.reviewId === review.id) return listed.state

  const draftStatus = detail.draft_status ?? null
  const stop = detail.not_for_signing ?? null

  if (review.dismissed_at || stop?.reason === 'dismissed') return 'dismissed'
  if (review.out_of_play) return 'out_of_play'
  /*
   * The pull request's end, read off the pull request — `not_for_signing.how` — before this
   * round's own `pr_state`, which three of four rounds of a merged pull request hold null.
   */
  const ended = stop?.reason === 'finished' ? stop.how : review.pr_state
  if (ended === 'merged') return 'merged'
  if (ended === 'closed') return 'closed'
  if (stop?.reason === 'superseded') return 'superseded'

  if (brief && brief.confirmation_required && !brief.confirmed) return 'brief_ready'
  if (draftStatus === 'ready') return 'draft_ready'
  if (outcomes.some(outcome => CAME_BACK_FROM.has(outcome.actor))) return 'replies_in'
  if (layers.some(layer => layer.name !== HARVEST && layer.state === 'running')) {
    return 'being_written'
  }
  if (draftStatus === 'posted' || review.github_review_id) return 'posted'
  return 'waiting_for_session'
}
