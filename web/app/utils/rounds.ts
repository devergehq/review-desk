/*
 * What a round's head has done, in the words the pages say it in.
 *
 * **One copy of each sentence, because three screens say them.** Since DEV-962 a new commit
 * on an unposted round moves that round instead of opening the next one, so a page has to be
 * able to say the round is no longer of the commit the developer was shown — Understand says
 * it as a notice, the code review says it over the findings, and the draft page says why a
 * sign-off went. Three spellings of one fact is how two screens come to disagree about the
 * record in front of them.
 *
 * Pure functions over the contract's own shapes, so they are unit-tested without a browser.
 */

import type { Draft, HeadUpdate, HeadUpdates, Review } from '~/types/desk'

/**
 * Nothing has happened to this round's head.
 *
 * **What a response written before DEV-962 means.** `head_updates` is not in the contract's
 * `required` list, for the reason `rounds` is not — a response from an older build is
 * missing it rather than wrong — so a screen needs one reading of its absence, and this is
 * it: the same answer the server serves for a round whose head has never moved. A page that
 * branched on `undefined` instead would have two ways of saying "nothing moved" and would
 * eventually word them differently.
 */
export function nothingMoved(): HeadUpdates {
  return {
    since_sha: null,
    moved: 0,
    base_only: 0,
    folded: 0,
    commits: null,
    last_at: null,
    updates: [],
  }
}

/** A commit as a reader quotes it: short, and never re-abbreviated. */
export function commitLabel(sha: string): string {
  return sha.slice(0, 7)
}

/**
 * "Updated for 3 commits since 4d9464f", or null when the head has never moved.
 *
 * **The count is dropped rather than guessed when the record has none.** `commits` is null
 * for a round whose commits nobody recorded, and "updated for 0 commits" would read as
 * "nothing changed" — which is the one thing a moved head is not. The sentence then says
 * what it knows: the round is of a newer commit than the one it was opened on.
 */
export function updatedLine(updates: HeadUpdates): string | null {
  if (updates.moved < 1 || !updates.since_sha) return null
  const since = commitLabel(updates.since_sha)
  if (updates.commits == null || updates.commits < 1) {
    return `Updated since ${since}`
  }
  const commits = updates.commits === 1 ? '1 commit' : `${updates.commits} commits`
  return `Updated for ${commits} since ${since}`
}

/**
 * "2 commits brought the base branch in and changed nothing", or null.
 *
 * **Said out loud, because the alternative is a silence that reads as a gap.** A base-branch
 * merge changes nothing in Review Desk by design — no round, no moved head, no session woken
 * — so without this line the page cannot tell "nothing happened" from "nobody looked", which
 * is the distinction the record keeps a `base_only` row for.
 */
export function baseOnlyLine(updates: HeadUpdates): string | null {
  if (updates.base_only < 1) return null
  const commits = updates.base_only === 1 ? '1 push' : `${updates.base_only} pushes`
  return `${commits} brought the base branch in and changed nothing`
}

/** "Round 1 was folded into this one", or null. The tidy's trace on the round it kept. */
export function foldedLine(updates: HeadUpdates): string | null {
  if (updates.folded < 1) return null
  const rounds = updates.folded === 1 ? 'An earlier round' : `${updates.folded} earlier rounds`
  const were = updates.folded === 1 ? 'was' : 'were'
  return `${rounds} ${were} folded into this one`
}

/**
 * The moves since the draft was last written, oldest first.
 *
 * **What the code review shows, and the reason it is the draft's own time.** A draft written
 * before a push describes a commit the pull request has moved past, so the question the page
 * answers is not "what has ever moved" but "what has moved since this body was written".
 *
 * `base_only` and `folded` rows are left out: neither changed the pull request's own diff, so
 * neither is something the draft is out of date about.
 */
export function movesSince(updates: HeadUpdates, when: string | null): HeadUpdate[] {
  const moves = updates.updates.filter(update => update.kind === 'moved')
  if (!when) return moves
  return moves.filter(update => update.at > when)
}

/**
 * Why a sign-off was withdrawn, in one sentence, or null when none was.
 *
 * **The developer approved different code**, which is DEV-962's own criterion: a signed-off
 * but unposted draft whose head moves is no longer signed off, and the page says why. The
 * commit they approved is on the draft; where the pull request is now is on the review.
 */
export function withdrawnLine(draft: Draft, review: Review): string | null {
  if (!draft.signoff_withdrawn_at || !draft.signoff_withdrawn_head) return null
  const approved = commitLabel(draft.signoff_withdrawn_head)
  const now = commitLabel(review.head_sha)
  return `You signed this off on ${approved}, and the pull request has moved to ${now}. `
    + 'The sign-off was withdrawn because the code you approved is not the code that '
    + 'would post.'
}
