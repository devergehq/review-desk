/*
 * **A review is one header and five tabs.** DEV-1324, under DEV-1322.
 *
 * `docs/design.md`, *The application shell*: the step trail is replaced by a header — the
 * crumb, the number and title, one state word, one facts line, one next action — and five
 * tabs. Everything the header decides is decided here, so `test/review-tabs.spec.ts` is a
 * test of the rules rather than of a template.
 *
 * Nothing here reads or writes a record. Each function takes what `GET /reviews/{id}`
 * served and answers a value.
 */

import type { LocationQuery } from 'vue-router'
import type { ReviewDetail, ReviewTimings } from '~/types/desk'
import type { ReviewState } from './stateWord'
import { durationLabel } from './format'

import { listCounts } from './findings'

/** The five tabs, by the suffix their page sits at. */
export type ReviewTab = 'overview' | 'understanding' | 'findings' | 'draft' | 'replies'

/**
 * The tabs in the order the S2 strip draws them, with the suffix each sits at.
 *
 * Overview is the review's own address, `/reviews/{id}`, which is why its suffix is empty.
 */
export const REVIEW_TABS: ReadonlyArray<{ tab: ReviewTab, label: string, suffix: string }> = [
  { tab: 'overview', label: 'Overview', suffix: '' },
  { tab: 'understanding', label: 'Understanding', suffix: 'understanding' },
  { tab: 'findings', label: 'Findings', suffix: 'findings' },
  { tab: 'draft', label: 'Draft', suffix: 'draft' },
  { tab: 'replies', label: 'Replies', suffix: 'replies' },
]

/** Where one tab of one review is. */
export function tabPath(reviewId: number, tab: ReviewTab): string {
  const suffix = REVIEW_TABS.find(one => one.tab === tab)!.suffix
  return suffix === '' ? `/reviews/${reviewId}` : `/reviews/${reviewId}/${suffix}`
}

/**
 * The step routes the tabs replaced, and where each one goes now.
 *
 * **A fragment of its own for the two that landed part way down the case page.** The trail
 * linked Alternatives and Solution with `#what-was-chosen` and `#what-was-built`, so an old
 * link to either with no fragment of its own still lands at its band. A link that carries
 * a fragment keeps it — `…/code-review#F9` arrives at F9 on Findings.
 */
export const RETIRED_ROUTES: Readonly<Record<string, { tab: ReviewTab, band?: string }>> = {
  'understand': { tab: 'understanding' },
  'alternatives': { tab: 'understanding', band: 'what-was-chosen' },
  'solution': { tab: 'understanding', band: 'what-was-built' },
  'code-review': { tab: 'findings' },
  'what-happened': { tab: 'replies' },
}

/**
 * Where an old step address goes: its tab, its query, and its own fragment or its band's.
 *
 * Shaped for `navigateTo`, so the five redirect pages share one answer.
 */
export function retiredRouteTarget(
  reviewId: string | number,
  retired: keyof typeof RETIRED_ROUTES,
  hash: string,
  query: LocationQuery = {},
): { path: string, hash: string, query: LocationQuery } {
  const { tab, band } = RETIRED_ROUTES[retired]!
  return {
    path: tabPath(Number(reviewId), tab),
    hash: hash !== '' ? hash : band ? `#${band}` : '',
    query,
  }
}

/**
 * The count on the Findings tab: **the findings in the *To post* list**, as S4 draws it —
 * "Findings 12" over "To post · 12".
 *
 * Read off `listCounts`, the same grouping the Findings page's three tabs count, so the tab
 * and the list cannot disagree. `null` when the review recorded no findings at all, so a
 * review whose code review has not run draws the tab's name alone rather than a "0" that
 * reads as an answer; a review whose findings were all ticketed or set aside draws 0, which
 * is the answer.
 */
export function findingsCount(detail: Pick<ReviewDetail, 'findings'>): number | null {
  return detail.findings.length > 0 ? listCounts(detail.findings).post : null
}

/**
 * **Where one finding is: the Findings tab with that finding selected.** DEV-1328.
 *
 * `?finding=F9`, which is the address every other tab links a finding by. The page also
 * reads `#F9`, the older form the Solution page wrote and `…/code-review#F9` still carries
 * through its redirect, so a link already held keeps working. `findingAsked` reads both.
 */
export function findingPath(reviewId: number, key: string): string {
  return `${tabPath(reviewId, 'findings')}?finding=${encodeURIComponent(key)}`
}

/**
 * The finding an address asks for, as its `seq`: `?finding=F9` first, then `#F9`, else null.
 *
 * A value that does not name a finding at all is null, not an error: a stale or mistyped
 * link lands on the list as it opens.
 */
export function findingAsked(query: LocationQuery, hash: string): number | null {
  const raw = query.finding
  const value = Array.isArray(raw) ? raw[0] : raw
  if (typeof value === 'string') {
    const seq = seqOfKey(value)
    if (seq != null) return seq
  }
  return hash.startsWith('#') ? seqOfKey(hash.slice(1)) : null
}

/** `F9` read back as 9; anything else is null. */
function seqOfKey(key: string): number | null {
  const found = /^F([0-9]+)$/.exec(key)
  if (!found) return null
  const seq = Number(found[1])
  return Number.isSafeInteger(seq) && seq > 0 ? seq : null
}

/**
 * **How long the developer has been waiting on now**, or `null` when the record cannot say.
 *
 * The open wait, not the total: `timings.waiting_on_you_seconds` sums the waits that have
 * *ended* — the contract says "a wait still going has no length" — so drawing it beside
 * *Draft ready* would say "2 h waiting on you" over a draft ready for five days. Found by
 * the self-review of #89.
 *
 * Only *Draft ready* has a start the record holds, `timings.draft_ready_at`. A brief waiting
 * to be confirmed and a reply waiting for an answer have none, so they draw no time rather
 * than a guessed one — `docs/design.md`: "A time the record does not hold is left out".
 */
export function openWait(
  state: ReviewState,
  timings: Pick<ReviewTimings, 'draft_ready_at'> | null | undefined,
  now: Date = new Date(),
): string | null {
  if (state !== 'draft_ready' || !timings?.draft_ready_at) return null
  const since = Date.parse(timings.draft_ready_at)
  if (Number.isNaN(since)) return null
  return durationLabel(Math.max(0, Math.floor((now.getTime() - since) / 1000)))
}

/** The one next action the header draws, top right. */
export interface PrimaryAction {
  /** The words on it, naming what it acts on. Never "Continue". */
  label: string
  /** The tab that holds what it acts on. */
  to: string
}

/**
 * **The one next action, or `null` for a review with nothing for the developer to do.**
 *
 * `docs/design.md`: "One next action, top right, naming what it acts on… A review with
 * nothing for the developer to do has no primary action." Only the three states that wait
 * on the developer have one:
 *
 * | State | Action | Tab |
 * |---|---|---|
 * | Brief ready | Confirm the brief | Understanding, where the brief's card is |
 * | Draft ready | Sign off and post | Draft, where the sign-off is |
 * | Replies in, with a reply to answer | Answer F3, or Answer 2 replies | Replies |
 *
 * Every other state is nothing for the developer to do, so no action: a session working
 * or awaited (*Being written*, *Waiting for a session*), a review posted and quiet, and the
 * ways off the desk (*Merged*, *Closed*, *Dismissed*, *Out of play*, *Superseded* — a round a
 * later one replaced, whose work is the later round's).
 *
 * **A reply is answered on Replies, so that is where both go**, one reply or several. DEV-1328
 * considered sending *Answer F3* to F3 on Findings (`?finding=F3`) and did not: a reply comes
 * in on a posted review, and Findings refuses every decision on one ("This review is posted").
 * The action would have opened a page where it could not be done. DEV-1330 builds the three
 * replies on Replies and may point this at the one reply there. DEV-1329 moves the Draft
 * page's sign-off binding into the draft-ready action.
 *
 * `toAnswer` is the finding keys [`repliesToAnswer`] answers, passed in so this stays a
 * function of values.
 */
export function primaryAction(
  reviewId: number,
  state: ReviewState,
  toAnswer: readonly string[],
): PrimaryAction | null {
  switch (state) {
    case 'brief_ready':
      return { label: 'Confirm the brief', to: tabPath(reviewId, 'understanding') }
    case 'draft_ready':
      return { label: 'Sign off and post', to: tabPath(reviewId, 'draft') }
    case 'replies_in':
      if (toAnswer.length === 0) return null
      return {
        label: toAnswer.length === 1 ? `Answer ${toAnswer[0]}` : `Answer ${toAnswer.length} replies`,
        to: tabPath(reviewId, 'replies'),
      }
    default:
      return null
  }
}
