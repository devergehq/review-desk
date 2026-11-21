import { describe, expect, it } from 'vitest'

import {
  REVIEW_TABS,
  findingsCount,
  openWait,
  primaryAction,
  retiredRouteTarget,
  tabPath,
} from '../app/utils/reviewTabs'
import { repliesToAnswer } from '../app/utils/outcomes'
import type { ReviewState } from '../app/utils/stateWord'
import type { Outcome } from '../app/types/desk'

/*
 * DEV-1324: a review is one header and five tabs, and the step routes redirect into them.
 * The rules the header draws are functions of values, so they are asserted here; the
 * header itself is asserted in `review-header.nuxt.spec.ts` and the redirects in the browser,
 * in `e2e/redirects.spec.ts`, where there is a router to redirect.
 */

describe('the five tabs', () => {
  it('are Overview, Understanding, Findings, Draft and Replies, in that order', () => {
    expect(REVIEW_TABS.map(one => one.label))
      .toEqual(['Overview', 'Understanding', 'Findings', 'Draft', 'Replies'])
  })

  it('put Overview at the review’s own address and the rest under it', () => {
    expect(REVIEW_TABS.map(one => tabPath(7, one.tab))).toEqual([
      '/reviews/7',
      '/reviews/7/understanding',
      '/reviews/7/findings',
      '/reviews/7/draft',
      '/reviews/7/replies',
    ])
  })

  it('count the findings, and draw no count before there are any', () => {
    expect(findingsCount({ findings: [{}, {}, {}] as never })).toBe(3)
    expect(findingsCount({ findings: [] })).toBeNull()
  })
})

describe('the step routes the tabs replaced', () => {
  it.each([
    ['understand', '/reviews/9/understanding', ''],
    ['alternatives', '/reviews/9/understanding', '#what-was-chosen'],
    ['solution', '/reviews/9/understanding', '#what-was-built'],
    ['code-review', '/reviews/9/findings', ''],
    ['what-happened', '/reviews/9/replies', ''],
  ] as const)('send %s to its tab, at its band when it has one', (route, path, hash) => {
    expect(retiredRouteTarget('9', route, '')).toEqual({ path, hash, query: {} })
  })

  it('keep a fragment the link carried, over the band', () => {
    expect(retiredRouteTarget('9', 'code-review', '#F9').hash).toBe('#F9')
    expect(retiredRouteTarget('9', 'alternatives', '#what-was-built').hash).toBe('#what-was-built')
  })

  it('keep the query', () => {
    expect(retiredRouteTarget('9', 'code-review', '', { round: '2' }).query).toEqual({ round: '2' })
  })
})

describe('the open wait', () => {
  const now = new Date('2026-11-21T12:00:00Z')

  it('runs from when the draft became ready', () => {
    expect(openWait('draft_ready', { draft_ready_at: '2026-11-21T09:00:00Z' }, now)).toBe('3 hr')
  })

  it('is nothing for a state the record holds no start for', () => {
    for (const state of ['brief_ready', 'replies_in', 'posted'] as const) {
      expect(openWait(state, { draft_ready_at: '2026-11-21T09:00:00Z' }, now), state).toBeNull()
    }
  })

  it('is nothing when the draft’s ready time was not recorded', () => {
    expect(openWait('draft_ready', { draft_ready_at: null }, now)).toBeNull()
    expect(openWait('draft_ready', null, now)).toBeNull()
  })
})

describe('the one next action', () => {
  it('names the brief, and goes to Understanding, while the brief waits', () => {
    expect(primaryAction(5, 'brief_ready', []))
      .toEqual({ label: 'Confirm the brief', to: '/reviews/5/understanding' })
  })

  /* The contract DEV-1329 builds on: the draft-ready action is the sign-off. */
  it('is Sign off and post, on Draft, while the draft waits', () => {
    expect(primaryAction(5, 'draft_ready', []))
      .toEqual({ label: 'Sign off and post', to: '/reviews/5/draft' })
  })

  it('names the one reply to answer, or counts them, and goes to Replies', () => {
    expect(primaryAction(5, 'replies_in', ['F3']))
      .toEqual({ label: 'Answer F3', to: '/reviews/5/replies' })
    expect(primaryAction(5, 'replies_in', ['F3', 'F9'])?.label).toBe('Answer 2 replies')
  })

  it('is nothing at all for a review with nothing for the developer to do', () => {
    const nothing: ReviewState[] = [
      'not_started', 'being_written', 'waiting_for_session', 'posted',
      'merged', 'closed', 'dismissed', 'out_of_play', 'superseded',
    ]
    for (const state of nothing) expect(primaryAction(5, state, ['F3']), state).toBeNull()
    expect(primaryAction(5, 'replies_in', []), 'replies with none to answer').toBeNull()
  })

  it('never says Continue', () => {
    const states: ReviewState[] = ['brief_ready', 'draft_ready', 'replies_in']
    for (const state of states) {
      expect(primaryAction(5, state, ['F1'])?.label).not.toMatch(/continue/i)
    }
  })
})

describe('the replies that wait on an answer', () => {
  let id = 0
  const outcome = (
    seq: number,
    kind: Outcome['kind'],
    actor: Outcome['actor'],
    at: string,
  ) => ({ id: (id += 1), finding_seq: seq, finding_key: `F${seq}`, kind, actor, occurred_at: at })

  it('are the findings whose newest outcome is the author replying', () => {
    expect(repliesToAnswer([
      outcome(3, 'replied', 'author', '2026-11-01T10:00:00Z'),
      outcome(1, 'replied', 'author', '2026-11-01T09:00:00Z'),
      outcome(2, 'kept', 'developer', '2026-11-01T09:00:00Z'),
    ])).toEqual(['F1', 'F3'])
  })

  it('stop waiting once something later is recorded on the finding', () => {
    expect(repliesToAnswer([
      outcome(4, 'replied', 'author', '2026-11-01T10:00:00Z'),
      outcome(4, 'raised_again', 'developer', '2026-11-02T10:00:00Z'),
      outcome(5, 'replied', 'author', '2026-11-01T10:00:00Z'),
      outcome(5, 'thread_resolved', 'harvester', '2026-11-03T10:00:00Z'),
    ])).toEqual([])
  })

  it('read the order an outcome was written in when two share a time', () => {
    expect(repliesToAnswer([
      outcome(6, 'code_changed_after', 'harvester', '2026-11-01T10:00:00Z'),
      outcome(6, 'replied', 'author', '2026-11-01T10:00:00Z'),
    ])).toEqual(['F6'])
  })

  it('are none when nothing came back', () => {
    expect(repliesToAnswer([])).toEqual([])
  })
})
