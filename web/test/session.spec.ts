import { describe, expect, it } from 'vitest'

import { deliveryLine, sessionAnswered, sessionLine } from '../app/utils/session'
import type { ReviewMessage, SessionReading, SessionStatus } from '../app/types/desk'

function reading(status: SessionStatus, over: Partial<SessionReading> = {}): SessionReading {
  return {
    status,
    allele_state: status === 'gone' ? 'done' : null,
    state_age_secs: 372,
    detail: null,
    read_at: '2026-10-30T11:05:12Z',
    ...over,
  }
}

describe('the session strip', () => {
  it('draws nothing when there was nobody to ask', () => {
    expect(sessionLine(null)).toBeNull()
    expect(sessionLine(undefined)).toBeNull()
  })

  it('says a session blocked on a prompt is waiting, for how long, and where to answer it', () => {
    expect(sessionLine(reading('awaiting_input'))).toEqual({
      tone: 'waiting',
      label: 'Waiting on a permission prompt',
      facts: '6 min · answer it in Allele',
    })
  })

  it('says how long a working session has been at it', () => {
    expect(sessionLine(reading('running'))?.facts).toBe('for 6 min')
  })

  it('tells a suspended session from a gone one', () => {
    expect(sessionLine(reading('gone', { allele_state: 'suspended' }))?.label)
      .toBe('Session not running')
    expect(sessionLine(reading('gone', { allele_state: 'done' }))?.label).toBe('Session gone')
    expect(sessionLine(reading('gone', { allele_state: null }))?.facts)
      .toContain('Allele has no session with this id')
  })

  it('says unknown and passes on what Allele said, rather than guessing', () => {
    const line = sessionLine(reading('unknown', { detail: 'Allele answered internal: it broke' }))
    expect(line?.label).toBe('Unknown')
    expect(line?.facts).toBe('Allele answered internal: it broke')
  })

  it('draws a strip for an Allele it cannot reach, so the silence is not mistaken for calm', () => {
    expect(sessionLine(reading('unreachable'))?.label).toBe('Can\'t reach Allele')
  })
})

describe('whether Allele has answered', () => {
  it.each([
    ['running', true],
    ['awaiting_input', true],
    ['finished', true],
    ['gone', true],
    ['unknown', false],
    ['unreachable', false],
  ] as const)('%s → %s', (status, answered) => {
    expect(sessionAnswered(reading(status))).toBe(answered)
  })

  it('is no answer when there is no Allele', () => {
    expect(sessionAnswered(null)).toBe(false)
  })
})

describe('whether a message has reached the session', () => {
  const asked: ReviewMessage = {
    review_id: 41,
    seq: 1,
    step: 'draft',
    author: 'developer',
    kind: 'ask',
    body: 'Why?',
    in_reply_to: null,
    created_at: '2026-10-30T11:00:00Z',
    read_at: null,
    read_by_session: null,
  }
  const now = new Date('2026-10-30T11:05:00Z')

  it('says who read it and when', () => {
    expect(deliveryLine(
      { ...asked, read_at: '2026-10-30T11:04:00Z', read_by_session: 'allele-1' },
      null,
      now,
    )).toBe('Read by allele-1 1 min ago')
  })

  it('says not delivered only where Allele gives a reason', () => {
    expect(deliveryLine(asked, reading('gone'), now)).toContain('Not delivered')
    expect(deliveryLine(asked, reading('awaiting_input'), now)).toContain('Not delivered')
    expect(deliveryLine(asked, reading('running'), now)).toBe('Not read yet')
    expect(deliveryLine(asked, null, now)).toBe('Not read yet')
  })

  it('says nothing about a reply, which is read by being on the page', () => {
    expect(deliveryLine({ ...asked, author: 'session', kind: 'reply' }, null, now)).toBeNull()
  })
})
