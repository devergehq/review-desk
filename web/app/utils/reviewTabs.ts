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
import type { ReviewDetail } from '~/types/desk'
import type { ReviewState } from './stateWord'

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
 * The count on the Findings tab: every finding the review recorded.
 *
 * `null` when there are none, so a review whose code review has not run draws the tab's
 * name alone rather than a "0" that reads as an answer.
 */
export function findingsCount(detail: Pick<ReviewDetail, 'findings'>): number | null {
  return detail.findings.length > 0 ? detail.findings.length : null
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
 * ways off the desk (*Merged*, *Closed*, *Dismissed*, *Out of play*).
 *
 * **It links to the tab, and that is all it does yet.** DEV-1329 moves the Draft page's
 * sign-off binding into the draft-ready action, and DEV-1328 adds `?finding=` so a single
 * reply's action can open its finding; both repoint what this answers rather than adding a
 * second place that chooses an action.
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
