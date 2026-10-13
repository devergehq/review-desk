/*
 * A review's rounds, as the pages ask about them — **two questions, one file.**
 *
 * **What this round's head has done** (DEV-962): since a new commit on an unposted round
 * moves that round instead of opening the next one, a page has to be able to say the round
 * is no longer of the commit the developer was shown — Understand says it as a notice, the
 * code review says it over the findings, and the draft page says why a sign-off went.
 *
 * **Where the pull request's other rounds are** (DEV-976): a review is one round and that
 * was a dead end. Patrick, 14 October 2026: "there's no way for me to actually go back to
 * the previous round… especially when I go and look at the understand, the alternatives,
 * the solution map, and even the code review. It's really important to be able to see where
 * we've come from, not just this specific point in time."
 *
 * **One copy of each sentence, because several screens say them**, which is also why the two
 * halves share a file rather than sitting in `rounds.ts` and a second file named almost the
 * same thing: three spellings of one fact is how two screens come to disagree about the
 * record in front of them, and a reader who finds one file does not know to look for the
 * other. `commitLabel` below is the one abbreviation of a commit either half uses.
 *
 * Pure functions over the contract's own shapes, so they are unit-tested without a browser.
 */

import { dayLabel } from './format'
import { PAGE_OF_STEP } from './steps'

import type {
  Draft,
  HeadUpdate,
  HeadUpdates,
  Review,
  ReviewDetail,
  ReviewLayerName,
  RoundSummary,
} from '~/types/desk'

/* ===================== What this round's head has done ====================== */

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
 *
 * **`>=`, not `>`, and the one case it is the whole of.** A move that withdraws a sign-off
 * writes the draft in the same transaction — the status goes back to `drafting` and
 * `updated_at` moves with it — and the record keeps whole seconds, so the draft's time is
 * never *less* than the move's. With a strict comparison the section was empty for exactly
 * the review it exists for: the signed-off draft whose head moved under it. Found by a
 * review of this branch.
 *
 * What `>=` costs is a move in the same second as an ordinary draft write being shown as
 * since it, which is the safe direction: the page says the body may be out of date, and it
 * is better to say so a second early than to go silent on the case that matters.
 */
export function movesSince(updates: HeadUpdates, when: string | null): HeadUpdate[] {
  const moves = updates.updates.filter(update => update.kind === 'moved')
  if (!when) return moves
  return moves.filter(update => update.at >= when)
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
/* ================== Where the pull request's other rounds are ================= */


/** One round of the pull request, as the switcher draws it. */
export interface RoundLink {
  round: number
  reviewId: number
  /** Where it goes, or `null` for the round being read — which is not a link. */
  to: string | null
  /** Whether this is the round being read. */
  here: boolean
  /**
   * The posting fact, in words: "posted 9 October 2026", "being worked on", "not posted".
   *
   * One of three rather than a date or nothing, because an unposted round that is the
   * newest one is not the same thing as an unposted round somebody pushed past.
   */
  standing: string
  /**
   * Whether the round was dismissed — `null` when nothing served the answer.
   *
   * See [`roundDismissed`]. Three-valued on purpose: a switcher that drew `false` as "not
   * dismissed" would be saying something it has not been told.
   */
  dismissed: boolean | null
  /** The head commit, at the seven characters a commit is read by. */
  head: string
}


/**
 * Whether a round was dismissed. `null` when the wire does not say.
 *
 * **This is the seam for DEV-962, and it is one function on purpose.** DEV-954's dismissed
 * flag on `RoundSummary` arrives with DEV-962, which is in progress on another branch; it is
 * not on master, so today this answers `null` for every round and the switcher marks none.
 * The moment the field lands, this reads it and nothing else changes — the component already
 * draws the tag, and `test/rounds.spec.ts` already asserts both answers against a round
 * carrying the field.
 *
 * `null` rather than `false` for "nothing said", because a review's own `dismissed_at` is
 * served and an *other* round's is not: answering `false` would turn "I was not told" into
 * "it is not dismissed", which is the claim a reader would act on.
 */
export function roundDismissed(round: RoundSummary): boolean | null {
  return (round as RoundSummary & { dismissed?: boolean | null }).dismissed ?? null
}

/**
 * The rounds of this pull request, oldest first, as links to this same step.
 *
 * Empty for a review whose pull request the record holds one round of, and empty when
 * `rounds` was not served at all — `ReviewDetail` does not require it. Both mean there is no
 * switcher to draw, and the component asks that question of the length rather than of the
 * server.
 *
 * **The same step, which is the whole point.** A reader looking at Alternatives in round 3
 * wants Alternatives in round 1, not the top of round 1. A step with no page of its own —
 * step 1 — answers no links at all rather than links to somewhere else.
 */
export function roundLinks(detail: ReviewDetail, step: ReviewLayerName): RoundLink[] {
  const page = PAGE_OF_STEP[step]
  const rounds = detail.rounds ?? []
  if (page == null || rounds.length < 2) return []

  return [...rounds]
    .sort((one, two) => one.round - two.round)
    .map((round) => {
      const here = round.review_id === detail.review.id
      return {
        round: round.round,
        reviewId: round.review_id,
        to: here ? null : `/reviews/${round.review_id}/${page}`,
        here,
        standing: standingOf(round, rounds, finished(detail)),
        dismissed: roundDismissed(round),
        /* `commitLabel`, the one abbreviation of a commit this file has. */
        head: commitLabel(round.head_sha),
      }
    })
}

/**
 * The posting fact about one round, in words.
 *
 * **"Being worked on" is the newest unposted round and only that one.** An older unposted
 * round is one the author pushed past: nobody is working on it and nobody can post it, which
 * is DEV-957's whole subject, so calling it "being worked on" would promise something that
 * is never coming. It reads "not posted", which is what it is.
 *
 * **And nothing is being worked on once the pull request is over**, which is the same
 * correction `StepTrail` makes to a step's tense: a round of a merged pull request that was
 * never posted will not be now. DEV-957's record is exactly this shape —
 * `globex/portal#9568` merged with three unposted rounds behind it.
 */
function standingOf(round: RoundSummary, rounds: RoundSummary[], over: boolean): string {
  const posted = dayLabel(round.posted_at)
  if (posted) return `posted ${posted}`
  if (over) return 'not posted'

  const newest = rounds.reduce((high, one) => Math.max(high, one.round), 0)
  return round.round === newest ? 'being worked on' : 'not posted'
}

/**
 * Whether this review's pull request is over — merged or closed.
 *
 * **`finished` alone of the three reasons.** It is the only one that is a fact about the
 * *pull request*, so it is the only one true of every round of it. `dismissed` is this
 * review's own, and a newer round of a pull request whose earlier round was dismissed may
 * well be being worked on; `superseded` says a newer round exists, and that round is then
 * the newest one, so the rule above already answers it.
 */
function finished(detail: ReviewDetail): boolean {
  return detail.not_for_signing?.reason === 'finished'
}

/** Which round this is of how many, or `null` where there is only the one. */
export function roundsLabel(detail: ReviewDetail): string | null {
  const rounds = detail.rounds ?? []
  if (rounds.length < 2) return null
  return `Round ${detail.review.round} of ${rounds.length}`
}

/** An earlier round's step, and where to read it. */
export interface WrittenEarlier {
  round: number
  to: string
}

/**
 * The three steps the brief feeds, and so the three a later round can be holding none of.
 *
 * Steps 5 to 7 are not among them and must not be: a round's findings, its draft and its
 * harvest are its own, written against its own head commit, and a page saying one of them
 * was "written in round 1" would be false about the one thing rounds exist to separate.
 */
const FROM_THE_BRIEF: ReviewLayerName[] = ['understand', 'alternatives', 'solution_map']

/**
 * Which earlier round wrote the brief this round has none of, and where that step is.
 *
 * `null` when this round wrote one of its own, when the step is not one the brief feeds, and
 * when no earlier round wrote one either — a pull request whose first round has no brief has
 * nowhere to send anybody.
 *
 * **Measured, not inferred, and that is what the contract field is for.** Nothing carries a
 * brief forward: `briefs` is keyed by the review id, as is every record behind steps 1 to 4,
 * and opening a round copies none of it. So a step page on a later round draws nothing, and
 * before `brief_written` the only thing a page could have said was "round 1", which is wrong
 * the moment a round after the first rewrites the brief.
 *
 * **The latest earlier round that wrote one**, which is also what makes "and no round since"
 * true in the sentence the page shows: every round between the one named and this one has
 * `brief_written` false, or it would have been named instead.
 */
export function briefWrittenEarlier(
  detail: ReviewDetail,
  step: ReviewLayerName,
): WrittenEarlier | null {
  if (!FROM_THE_BRIEF.includes(step)) return null
  if (detail.brief != null) return null

  const page = PAGE_OF_STEP[step]
  if (page == null) return null

  const wrote = (detail.rounds ?? [])
    .filter(round => round.brief_written && round.round < detail.review.round)
    .sort((one, two) => two.round - one.round)[0]

  return wrote == null ? null : { round: wrote.round, to: `/reviews/${wrote.review_id}/${page}` }