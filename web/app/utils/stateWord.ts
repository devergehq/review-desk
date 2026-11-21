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
  'review' | 'layers' | 'outcomes' | 'draft_status' | 'brief'
>

/**
 * Where one review stands, from `GET /reviews/{id}`.
 *
 * **The same facts as `GET /inbox`, asked in the same order**, so a review's header says
 * the word the sidebar says for it. The sidebar's word is the inbox array the review was
 * served in (`utils/sidebar.ts`), and `crates/store/src/entities/inbox.rs` is what chooses
 * the array — so this reads what that file reads, not the layers' states, which can say
 * something else: a draft can be `ready` with its layer still `pending`.
 *
 * | Order | Fact | Inbox array | State |
 * | -- | -- | -- | -- |
 * | 1 | Dismissed, out of play, merged or closed | none: off the desk | the way it left |
 * | 2 | A brief that needs confirming and is not confirmed | `briefs_to_confirm` | `brief_ready` |
 * | 3 | The draft's status is `ready` | `drafts_to_approve` | `draft_ready` |
 * | 4 | The author or the harvester recorded something | `author_replies` | `replies_in` |
 * | 5 | One of layers 1–6 is `running` | `in_progress` | `being_written` |
 * | 6 | The draft is posted | `posted_awaiting_author` | `posted` |
 * | 7 | Anything else | `awaiting_session` | `waiting_for_session` |
 *
 * Order 2 before 3 is the queue's: a review with an unconfirmed brief and a ready draft is
 * one row, and the brief is the next thing the developer does.
 *
 * **Two things the inbox knows that one review does not**, and where this can differ:
 * `author_replies` leaves out what a later posted round has already answered
 * (`ALREADY_REVIEWED`), and the brief and draft arrays hold only a pull request's newest
 * round. Both need other rounds of the pull request to decide, which `GET /reviews/{id}`
 * does not serve; on a superseded round the header can say *Brief ready* where the
 * sidebar lists the newer round instead. `test/state-word.spec.ts` runs every sample
 * review through both paths and asserts the same word.
 */
export function stateOfReview(detail: StateSource): ReviewState {
  const { review, layers, outcomes, brief } = detail
  const draftStatus = detail.draft_status ?? null

  if (review.dismissed_at) return 'dismissed'
  if (review.out_of_play) return 'out_of_play'
  if (review.pr_state === 'merged') return 'merged'
  if (review.pr_state === 'closed') return 'closed'

  if (brief && brief.confirmation_required && !brief.confirmed) return 'brief_ready'
  if (draftStatus === 'ready') return 'draft_ready'
  if (outcomes.some(outcome => CAME_BACK_FROM.has(outcome.actor))) return 'replies_in'
  if (layers.some(layer => layer.name !== HARVEST && layer.state === 'running')) {
    return 'being_written'
  }
  if (draftStatus === 'posted' || review.github_review_id) return 'posted'
  return 'waiting_for_session'
}
