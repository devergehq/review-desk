/*
 * The sentences a page says about a round's head, and the one decision in them: what to say
 * when the record cannot count the commits.
 */

import { describe, expect, it } from 'vitest'

import {
  baseOnlyLine,
  commitLabel,
  foldedLine,
  movesSince,
  updatedLine,
  withdrawnLine,
} from '~/utils/rounds'
import type { Draft, HeadUpdate, HeadUpdates, Review } from '~/types/desk'

function updates(over: Partial<HeadUpdates> = {}): HeadUpdates {
  return {
    since_sha: null,
    moved: 0,
    base_only: 0,
    folded: 0,
    commits: null,
    last_at: null,
    updates: [],
    ...over,
  }
}

function move(from: string, to: string, at: string): HeadUpdate {
  return { kind: 'moved', from_sha: from, to_sha: to, at }
}

describe('what a page says about a head that moved', () => {
  it('says nothing at all when the head has never moved', () => {
    expect(updatedLine(updates())).toBeNull()
    expect(baseOnlyLine(updates())).toBeNull()
    expect(foldedLine(updates())).toBeNull()
  })

  it('counts the commits when the record knows them', () => {
    expect(updatedLine(updates({ moved: 1, since_sha: '4d9464f', commits: 3 })))
      .toBe('Updated for 3 commits since 4d9464f')
  })

  it('says one commit in the singular', () => {
    expect(updatedLine(updates({ moved: 1, since_sha: '4d9464f', commits: 1 })))
      .toBe('Updated for 1 commit since 4d9464f')
  })

  it('drops the count rather than printing a nought', () => {
    // A null count is a round whose commits nobody recorded. "Updated for 0 commits" would
    // read as "nothing changed", which is the one thing a moved head is not.
    expect(updatedLine(updates({ moved: 2, since_sha: '4d9464f', commits: null })))
      .toBe('Updated since 4d9464f')
    expect(updatedLine(updates({ moved: 2, since_sha: '4d9464f', commits: 0 })))
      .toBe('Updated since 4d9464f')
  })

  it('abbreviates a full commit to the seven characters a reader quotes', () => {
    expect(commitLabel('4d9464f1b7c4e0aa41c9e2c7730bd9d02e713f9c')).toBe('4d9464f')
    expect(commitLabel('4d9464f')).toBe('4d9464f')
  })

  it('says a base-branch merge changed nothing', () => {
    expect(baseOnlyLine(updates({ base_only: 1 })))
      .toBe('1 push brought the base branch in and changed nothing')
    expect(baseOnlyLine(updates({ base_only: 2 })))
      .toBe('2 pushes brought the base branch in and changed nothing')
  })

  it('says how many earlier rounds the tidy folded in', () => {
    expect(foldedLine(updates({ folded: 1 }))).toBe('An earlier round was folded into this one')
    expect(foldedLine(updates({ folded: 3 }))).toBe('3 earlier rounds were folded into this one')
  })
})

describe('what changed since the draft was written', () => {
  const held = updates({
    moved: 2,
    since_sha: '4d9464f',
    commits: 3,
    updates: [
      move('4d9464f', 'a41c9e2', '2026-10-10T09:00:00Z'),
      { kind: 'base_only', from_sha: 'a41c9e2', to_sha: '7e2a9d3', at: '2026-10-10T10:00:00Z' },
      move('a41c9e2', '1b7c4e0', '2026-10-11T08:00:00Z'),
    ],
  })

  it('takes the moves after the draft was last written', () => {
    const since = movesSince(held, '2026-10-10T12:00:00Z')
    expect(since.map(update => update.to_sha)).toEqual(['1b7c4e0'])
  })

  it('leaves out the pushes that only brought the base branch in', () => {
    // Neither changed the pull request's own diff, so neither is something the draft is
    // out of date about.
    expect(movesSince(held, null).every(update => update.kind === 'moved')).toBe(true)
    expect(movesSince(held, null)).toHaveLength(2)
  })

  it('takes every move when there is no draft to date from', () => {
    expect(movesSince(held, null)).toHaveLength(2)
  })

  it('keeps the move that withdrew the sign-off, which is written in the same second', () => {
    /*
     * **The case the section exists for.** Withdrawing a sign-off writes the draft in the
     * same transaction as the move, and the record keeps whole seconds — so the draft's
     * `updated_at` equals the move's time, and a strict comparison left the page silent
     * about the one push that took a developer's sign-off away.
     */
    const withdrawn = updates({
      moved: 1,
      since_sha: '4d9464f',
      commits: 2,
      updates: [move('4d9464f', 'a41c9e2', '2026-10-11T08:00:00Z')],
    })
    expect(movesSince(withdrawn, '2026-10-11T08:00:00Z')).toHaveLength(1)
  })
})

describe('why a sign-off went', () => {
  const review = { head_sha: 'a41c9e2' } as Review

  it('says which commit was approved and where the pull request is now', () => {
    const draft = {
      signoff_withdrawn_at: '2026-10-11T08:00:00Z',
      signoff_withdrawn_head: '4d9464f',
    } as Draft
    const line = withdrawnLine(draft, review)
    expect(line).toContain('4d9464f')
    expect(line).toContain('a41c9e2')
    expect(line).toContain('withdrawn')
  })

  it('says nothing about a draft nobody signed off', () => {
    const draft = { signoff_withdrawn_at: null, signoff_withdrawn_head: null } as Draft
    expect(withdrawnLine(draft, review)).toBeNull()
  })
})
