import { describe, expect, it } from 'vitest'

import type { Inbox, ReviewList, ReviewListItem, SessionReading } from '../app/types/desk'
import { agentLine, githubReadAt, sidebarGroups } from '../app/utils/sidebar'

/*
 * The sidebar's groups, its GitHub line and its agent line — DEV-1323's criteria that the
 * groups come from the record, that an empty group still draws, that the GitHub line is
 * drawn only when a read time is recorded, and that the agent line reads DEV-1140's reading.
 */

const NOW = new Date('2026-11-21T12:00:00Z')

const pr = (n: number, title: string | null = `Pull request ${n}`) =>
  ({ repo: 'acme/portal', pr: n, title })

const round = (id: number, title: string | null = null) =>
  ({ review_id: id, round: 1, title, ticket_id: null, ticket_url: null })

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

function finishedRow(id: number, n: number, at: string | null, how: 'merged' | 'closed' = 'merged') {
  return {
    review: { id, repo: 'acme/portal', pr: n, title: `Finished ${n}`, harvested_at: null },
    left_the_desk: 'finished',
    finished_how: how,
    finished_at: at,
  } as unknown as ReviewListItem
}

const list = (...reviews: ReviewListItem[]) => ({ reviews } as unknown as ReviewList)

describe('sidebarGroups', () => {
  it('always answers the four groups, in order, empty ones included', () => {
    const groups = sidebarGroups(inbox(), null, NOW)
    expect(groups.map(group => group.label)).toEqual([
      'Waiting on you',
      'Being reviewed',
      'Waiting on the author',
      'Done this week',
    ])
    expect(groups.every(group => group.rows.length === 0)).toBe(true)
  })

  it('reads each group off the inbox array it names, with its state', () => {
    const groups = sidebarGroups(inbox({
      briefs_to_confirm: [{ review_id: 1, pull_request: pr(519) }],
      drafts_to_approve: [{ review_id: 2, pull_request: pr(514) }],
      author_replies: [{ review_id: 3, pull_request: pr(412), newest_round: round(4, 'Newest title') }],
      in_progress: [{ review_id: 5, pull_request: pr(600), newest_round: round(5) }],
      awaiting_session: [{ review_id: null, pull_request: pr(601), round: null }],
      posted_awaiting_author: [{ review_id: 6, pull_request: pr(700) }],
    } as unknown as Partial<Inbox>), null, NOW)

    const [waiting, reviewing, onAuthor] = groups
    expect(waiting!.rows.map(row => [row.pr, row.state, row.reviewId])).toEqual([
      [519, 'brief_ready', 1],
      [514, 'draft_ready', 2],
      [412, 'replies_in', 4],
    ])
    expect(waiting!.rows[2]!.title).toBe('Newest title')
    expect(reviewing!.rows.map(row => [row.pr, row.state, row.reviewId])).toEqual([
      [600, 'being_written', 5],
      [601, 'not_started', null],
    ])
    expect(onAuthor!.rows.map(row => [row.pr, row.state])).toEqual([[700, 'posted']])
  })

  it('draws one row per pull request, in the first group that claims it', () => {
    const groups = sidebarGroups(inbox({
      drafts_to_approve: [{ review_id: 2, pull_request: pr(514) }],
      in_progress: [{ review_id: 3, pull_request: pr(514), newest_round: round(3) }],
    } as unknown as Partial<Inbox>), null, NOW)
    expect(groups[0]!.rows).toHaveLength(1)
    expect(groups[1]!.rows).toHaveLength(0)
  })

  it('puts in Done this week only what finished in the last seven days, at a recorded time', () => {
    const groups = sidebarGroups(inbox(), list(
      finishedRow(10, 800, '2026-11-20T09:00:00Z'),
      finishedRow(11, 801, '2026-11-01T09:00:00Z'),
      finishedRow(12, 802, null),
      finishedRow(13, 803, '2026-11-19T09:00:00Z', 'closed'),
    ), NOW)
    expect(groups[3]!.rows.map(row => [row.pr, row.state])).toEqual([
      [800, 'merged'],
      [803, 'closed'],
    ])
  })
})

describe('githubReadAt', () => {
  it('is null when nothing records a read of GitHub', () => {
    expect(githubReadAt(inbox(), null)).toBeNull()
    expect(githubReadAt(null, null)).toBeNull()
  })

  it('is the newest harvest either read carries', () => {
    const posted = inbox({
      posted_awaiting_author: [
        { review_id: 1, pull_request: pr(1), harvested_at: '2026-11-21T10:00:00Z' },
        { review_id: 2, pull_request: pr(2), harvested_at: null },
      ],
    } as unknown as Partial<Inbox>)
    const finished = finishedRow(3, 3, '2026-11-20T00:00:00Z')
    ;(finished.review as { harvested_at: string }).harvested_at = '2026-11-21T11:30:00Z'
    expect(githubReadAt(posted, null)).toBe('2026-11-21T10:00:00Z')
    expect(githubReadAt(posted, list(finished))).toBe('2026-11-21T11:30:00Z')
  })
})

describe('agentLine', () => {
  const reading = (status: SessionReading['status']) =>
    ({ status, allele_state: null, state_age_secs: null, detail: null, read_at: NOW.toISOString() })

  it('draws nothing when there was nobody to ask', () => {
    expect(agentLine(null)).toBeNull()
    expect(agentLine(undefined)).toBeNull()
  })

  it('says Allele is running whenever Allele answered', () => {
    for (const status of ['running', 'awaiting_input', 'finished', 'gone', 'unknown'] as const) {
      expect(agentLine(reading(status))).toEqual({ tone: 'on', text: 'Allele running' })
    }
  })

  it('says when Allele could not be reached', () => {
    expect(agentLine(reading('unreachable'))).toEqual({ tone: 'off', text: 'Can\'t reach Allele' })
  })
})
