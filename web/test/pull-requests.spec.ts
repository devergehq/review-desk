import { describe, expect, it } from 'vitest'

import {
  LIST_VIEWS,
  looksLikeALink,
  moveSelection,
  numbersSaid,
  rowMatches,
  selectionFromQuery,
  selectionOn,
  selectionQuery,
  sinceYesterday,
  viewFromQuery,
  viewHolding,
  type ListRow,
} from '../app/utils/pullRequests'

import type { Inbox, ReviewListItem } from '../app/types/desk'

/*
 * **The Pull requests list's rules that are not about one row**, tested without rendering
 * anything. DEV-1325. Which pull request is in which tab is the queue's merge and is tested
 * where it is drawn, in `test/screens.nuxt.spec.ts`.
 */

function row(key: string, pr: number, over: Partial<ListRow> = {}): ListRow {
  return {
    key,
    reviewId: pr,
    pr,
    repo: 'acme/portal',
    title: `title ${pr}`,
    state: null,
    to: `/reviews/${pr}`,
    href: null,
    action: null,
    session: null,
    ...over,
  }
}

describe('the tabs', () => {
  it('are the sidebar’s groups in the sidebar’s words, then the developer’s own, Done and Every review', () => {
    expect(LIST_VIEWS.map(one => one.label)).toEqual([
      'Waiting on you',
      'Being reviewed',
      'Waiting on the author',
      'Your own',
      'Done',
      'Every review',
    ])
  })

  it('opens Waiting on you for an address that names no tab, or one that does not exist', () => {
    expect(viewFromQuery(undefined)).toBe('waiting')
    expect(viewFromQuery('nonsense')).toBe('waiting')
    expect(viewFromQuery('every')).toBe('every')
    expect(viewFromQuery(['author', 'own'])).toBe('author')
  })
})

describe('J and K', () => {
  const rows = [row('a#1', 1), row('a#2', 2), row('a#3', 3)]

  it('move down and up one row', () => {
    expect(moveSelection(rows, 'a#1', 1)).toBe('a#2')
    expect(moveSelection(rows, 'a#2', -1)).toBe('a#1')
  })

  it('stop at either end rather than wrapping', () => {
    expect(moveSelection(rows, 'a#3', 1)).toBe('a#3')
    expect(moveSelection(rows, 'a#1', -1)).toBe('a#1')
  })

  it('select the first row when nothing on the page is selected', () => {
    expect(moveSelection(rows, null, 1)).toBe('a#1')
    expect(moveSelection(rows, 'gone#9', -1)).toBe('a#1')
    expect(moveSelection([], null, 1)).toBeNull()
  })
})

describe('a selection in the address', () => {
  /* DEV-1373: how the sidebar opens the list on a pull request with no round. */
  const a = row('acme/portal#1', 1)
  const b = row('acme/portal#2', 2)

  it('carries a row’s key and reads it back, and nothing that is not one', () => {
    expect(selectionQuery('acme/portal#901')).toEqual({ pr: 'acme/portal#901' })
    expect(selectionFromQuery('acme/portal#901')).toBe('acme/portal#901')
    expect(selectionFromQuery(['acme/portal#901', 'x#2'])).toBe('acme/portal#901')
    expect(selectionFromQuery(undefined)).toBeNull()
    expect(selectionFromQuery('901')).toBeNull()
  })

  it('finds the first tab that draws the row, never Every review', () => {
    const rows = {
      waiting: [a], reviewing: [b], author: [], own: [], done: [], every: [a, b, row('x/y#3', 3)],
    }
    expect(viewHolding(rows, 'acme/portal#2')).toBe('reviewing')
    expect(viewHolding(rows, 'acme/portal#1')).toBe('waiting')
    expect(viewHolding(rows, 'x/y#3')).toBeNull()
  })

  it('keeps the selection while it is drawn, else takes the asked-for row, else the first', () => {
    expect(selectionOn([a, b], 'acme/portal#1', 'acme/portal#2')).toBe('acme/portal#1')
    expect(selectionOn([a, b], null, 'acme/portal#2')).toBe('acme/portal#2')
    expect(selectionOn([a, b], 'gone#9', 'gone#8')).toBe('acme/portal#1')
    expect(selectionOn([], null, 'acme/portal#2')).toBeNull()
  })
})

describe('the find field', () => {
  const one = { pr: 514, title: 'Give a scheduled export its own context', repo: 'acme/portal' }

  it('finds by number, with or without its hash', () => {
    expect(rowMatches(one, '#514')).toBe(true)
    expect(rowMatches(one, '51')).toBe(true)
    expect(rowMatches(one, '519')).toBe(false)
  })

  it('finds by words from the title or the repository, in any case', () => {
    expect(rowMatches(one, 'Scheduled EXPORT')).toBe(true)
    expect(rowMatches(one, 'portal')).toBe(true)
    expect(rowMatches(one, 'webhook')).toBe(false)
  })

  it('narrows nothing while a link is in it, which is a review being started', () => {
    expect(looksLikeALink('https://github.com/acme/portal/pull/9')).toBe(true)
    expect(looksLikeALink('acme/portal#9')).toBe(true)
    expect(rowMatches(one, 'https://github.com/acme/other/pull/1')).toBe(true)
    expect(rowMatches(one, '   ')).toBe(true)
  })
})

describe('Since yesterday', () => {
  const now = new Date('2026-11-21T12:00:00Z')
  const hoursAgo = (hours: number) => new Date(now.getTime() - hours * 3_600_000).toISOString()

  function inbox(over: Partial<Inbox> = {}): Inbox {
    return {
      totals: { items: 0, repos: 0 },
      briefs_to_confirm: [],
      drafts_to_approve: [],
      author_replies: [],
      own_prs_without_reviewer: [],
      proposed_rules: [],
      in_progress: [],
      awaiting_session: [],
      posted_awaiting_author: [],
      handled_without_you: { window_days: 7 },
      ...over,
    } as Inbox
  }

  const pull = (pr: number) => ({ repo: 'acme/portal', pr })

  it('says what was posted, what came back and what finished in the last day, newest first', () => {
    const lines = sinceYesterday(
      inbox({
        posted_awaiting_author: [{ pull_request: pull(514), round: 2, posted_at: hoursAgo(3) }],
        author_replies: [{
          pull_request: pull(519),
          replied_at: hoursAgo(1),
          events: [{ kind: 'replied', finding_key: 'F2' }],
        }],
      } as unknown as Partial<Inbox>),
      [{
        review: { pr: 507 },
        finished_how: 'merged',
        finished_at: hoursAgo(5),
      } as unknown as ReviewListItem],
      now,
    )
    expect(lines.map(line => line.says)).toEqual([
      expect.stringMatching(/^#519: /),
      'Round 2 of #514 was posted',
      '#507 merged',
    ])
  })

  it('leaves out anything older than a day, and anything with no time at all', () => {
    const lines = sinceYesterday(
      inbox({
        posted_awaiting_author: [
          { pull_request: pull(1), round: 1, posted_at: hoursAgo(25) },
          { pull_request: pull(2), round: 1, posted_at: null },
        ],
      } as unknown as Partial<Inbox>),
      [],
      now,
    )
    expect(lines).toEqual([])
  })
})

describe('a group’s pull requests, said', () => {
  it('names up to four, and counts the rest', () => {
    expect(numbersSaid([])).toBe('')
    expect(numbersSaid([514])).toBe('#514')
    expect(numbersSaid([514, 507])).toBe('#514 and #507')
    expect(numbersSaid([1, 2, 3, 4])).toBe('#1, #2, #3 and #4')
    expect(numbersSaid([1, 2, 3, 4, 5])).toBe('#1, #2, #3 and 2 more')
  })
})
