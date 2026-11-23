/*
 * **The Pull requests list: the rules that are not about any one row.** DEV-1325, under
 * DEV-1322.
 *
 * Queue and Reviews became one list (S1): tabs over one table, a preview of the selected
 * row beside it, and `J`, `K` and `Enter` to walk it. Which pull request goes in which tab
 * is still the queue's merge in `pages/index.vue`, untouched, and `test/screens.nuxt.spec.ts`
 * pins it to `sidebarGroups` so the sidebar and the list cannot disagree. What is here is
 * the rest: the tabs, the selection, the find field, and the two things S17 draws when
 * nothing waits on the developer.
 *
 * Nothing here reads or writes a record. `test/pull-requests.spec.ts` is a test of the rules.
 */

import type { Inbox, Instant, ReviewListItem } from '~/types/desk'

import { cameBackHeadline } from './outcomes'
import type { ReviewState } from './stateWord'

/** One row of the list, as the selection, the keys and the preview read it. */
export interface ListRow {
  /** `repo#pr`: one row per pull request, which is what every tab is keyed on. */
  key: string
  /** The round the row opens, or null for a pull request no round exists of. */
  reviewId: number | null
  pr: number
  repo: string
  title: string
  /**
   * The row's state word, when its tab is read off an inbox array — the sidebar's
   * derivation. Null where the read is not sorted into arrays (the *Every review* view and
   * the developer's own pull requests); the preview then works it out from the review.
   */
  state: ReviewState | null
  /** Where opening the row goes inside the app: the review's Overview. */
  to: string | null
  /** Where opening the row goes outside it, for a pull request with no review to open. */
  href: string | null
  /** The next step, in the row's own words, or null when there is none to name. */
  action: string | null
  /** What the record holds about the session, or null when it holds nothing. */
  session: string | null
}

/** The list's tabs, by the value the address carries in `?view=`. */
export type ListView = 'waiting' | 'reviewing' | 'author' | 'own' | 'done' | 'every'

/**
 * The tabs in the order S1 draws them, and a sixth.
 *
 * **The first four are the sidebar's groups, under the sidebar's own words**, so a pull
 * request is in the same place in both. *Your own* is the queue's *Your pull requests*.
 * *Done* is the sidebar's *Done this week*. **Every review** is the Reviews list, whole: the
 * dismissed, the finished before this week, the out of play and every earlier round are in
 * no group, and the one list must still reach every row the two lists drew.
 */
export const LIST_VIEWS: ReadonlyArray<{ view: ListView, label: string }> = [
  { view: 'waiting', label: 'Waiting on you' },
  { view: 'reviewing', label: 'Being reviewed' },
  { view: 'author', label: 'Waiting on the author' },
  { view: 'own', label: 'Your own' },
  { view: 'done', label: 'Done' },
  { view: 'every', label: 'Every review' },
]

/** The view an address asks for, or *Waiting on you* for anything else. */
export function viewFromQuery(value: unknown): ListView {
  const asked = Array.isArray(value) ? value[0] : value
  return LIST_VIEWS.some(one => one.view === asked) ? (asked as ListView) : 'waiting'
}

/**
 * **The selection an address carries, in `?pr=`: a row's `repo#pr`.** DEV-1373.
 *
 * The sidebar's way into the list for a pull request with no round, which has no review page
 * to open: `/?pr=acme/portal%23901` is the list with that row selected and its preview
 * drawn. An arrival instruction rather than a mirror of the selection — `J`, `K` and the
 * pointer move the selection without writing it back, so the address is not rewritten under
 * a reader walking the list.
 */
export function selectionQuery(key: string): { pr: string } {
  return { pr: key }
}

/** The `repo#pr` an address asks to have selected, or null when it asks for none. */
export function selectionFromQuery(value: unknown): string | null {
  const asked = Array.isArray(value) ? value[0] : value
  return typeof asked === 'string' && asked.includes('#') ? asked : null
}

/**
 * The tab that draws a pull request, for an address that names one and no tab.
 *
 * **The first tab in S1's order that holds the row**, so a link needs to know only which pull
 * request it means — the sidebar's groups and the tabs are one set of rows, and a pull request
 * is in one of them. *Every review* is never the answer: it is read only once opened, and
 * every row in a group is also in one of the tabs before it. Null while no tab holds it, which
 * includes before the inbox has answered.
 */
export function viewHolding(rows: Readonly<Record<ListView, readonly ListRow[]>>, key: string): ListView | null {
  return LIST_VIEWS.find(one => one.view !== 'every' && rows[one.view].some(row => row.key === key))?.view ?? null
}

/**
 * The row to select when the open tab's rows change: the current one while it is drawn, else
 * the one the address asks for when that is drawn, else the first.
 */
export function selectionOn(rows: readonly ListRow[], current: string | null, asked: string | null): string | null {
  if (current && rows.some(row => row.key === current)) return current
  if (asked && rows.some(row => row.key === asked)) return asked
  return rows[0]?.key ?? null
}

/**
 * The row `J` or `K` moves to: the next or previous, stopping at either end.
 *
 * **Stopping, not wrapping.** A key held down at the last row should stay there rather than
 * jump to the top of a list the reader was reading down. With nothing selected, either key
 * selects the first row.
 */
export function moveSelection(rows: readonly ListRow[], current: string | null, by: 1 | -1): string | null {
  if (rows.length === 0) return null
  const at = rows.findIndex(row => row.key === current)
  if (at < 0) return rows[0]!.key
  const next = Math.min(rows.length - 1, Math.max(0, at + by))
  return rows[next]!.key
}

/**
 * Whether what is in the find field is a link to start a review with, not words to find.
 *
 * The one field does both, as S1 draws it — "Find a PR, or paste a link to review one" —
 * so a link must never filter the list down to nothing while it is being pasted.
 */
export function looksLikeALink(text: string): boolean {
  const said = text.trim()
  return /^https?:\/\//i.test(said) || said.includes('/')
}

/**
 * Whether a row matches the find field: its number, or words from its title or repository.
 *
 * Empty, or a link, matches everything.
 */
export function rowMatches(row: Pick<ListRow, 'pr' | 'title' | 'repo'>, text: string): boolean {
  const said = text.trim().toLowerCase()
  if (said === '' || looksLikeALink(said)) return true
  const number = said.replace(/^#/, '')
  if (/^\d+$/.test(number)) return String(row.pr).startsWith(number)
  return row.title.toLowerCase().includes(said) || row.repo.toLowerCase().includes(said)
}

/** How far back *Since yesterday* reaches: a day, measured, not since midnight. */
export const SINCE_HOURS = 24

/** One line of *Since yesterday*, with the time it is sorted by. */
export interface SinceLine {
  at: Instant
  says: string
}

const within = (at: Instant | null | undefined, now: Date): at is Instant => {
  if (!at) return false
  const age = now.getTime() - new Date(at).getTime()
  return age >= 0 && age <= SINCE_HOURS * 60 * 60 * 1000
}

/**
 * What changed on the desk in the last day, newest first, **from recorded times only** (S17).
 *
 * Three kinds of event carry a time the record keeps: a round posted (`posted_at`),
 * something back from the author (`replied_at`, worded by `cameBackHeadline` as the queue
 * words it), and a pull request merging or closing (`finished_at`). Anything else that may
 * have happened has no time here, so it is not said. S17's *Next likely* line is a forecast,
 * which the record cannot make, and is left out.
 */
export function sinceYesterday(
  inbox: Inbox,
  done: readonly ReviewListItem[],
  now: Date = new Date(),
): SinceLine[] {
  const lines: SinceLine[] = []

  for (const item of inbox.posted_awaiting_author) {
    if (within(item.posted_at, now)) {
      lines.push({ at: item.posted_at, says: `Round ${item.round} of #${item.pull_request.pr} was posted` })
    }
  }
  for (const item of inbox.author_replies) {
    const first = item.events[0]
    if (first && within(item.replied_at, now)) {
      lines.push({ at: item.replied_at, says: `#${item.pull_request.pr}: ${cameBackHeadline(first)}` })
    }
  }
  for (const item of done) {
    if (within(item.finished_at, now)) {
      const how = item.finished_how === 'closed' ? 'closed' : item.finished_how === 'merged' ? 'merged' : 'finished'
      lines.push({ at: item.finished_at!, says: `#${item.review.pr} ${how}` })
    }
  }

  return lines.sort((one, two) => new Date(two.at).getTime() - new Date(one.at).getTime())
}

/**
 * The pull request numbers a line names, as a reader says them: "#514", "#514 and #507",
 * "#514, #507 and #499", and past four "#514, #507, #499 and 3 more".
 */
export function numbersSaid(prs: readonly number[]): string {
  const said = prs.map(pr => `#${pr}`)
  if (said.length <= 1) return said.join('')
  if (said.length <= 4) return `${said.slice(0, -1).join(', ')} and ${said.at(-1)}`
  return `${said.slice(0, 3).join(', ')} and ${said.length - 3} more`
}
