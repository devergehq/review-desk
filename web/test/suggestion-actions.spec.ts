import { describe, expect, it } from 'vitest'

import {
  accepting,
  acceptLabel,
  answerAt,
  answeredLine,
  clip,
  dismissLabel,
  turnIntoFinding,
  turnedInto,
} from '../app/utils/suggestionActions'
import type { Answer, Suggestion, SuggestionAnswer } from '../app/types/desk'

/*
 * What Accept and Dismiss do on one suggestion. DEV-1334.
 *
 * **Every Accept is a control the page already has**: a decision through `decideFinding`, a
 * finding through `addFinding`, or the ask box or the reply filled with nothing written.
 */

const AT = { message: 5, suggestion: 2 }

const ALL: Suggestion[] = [
  { type: 'new_finding', severity: 'should', title: 'The web path leaks', where: 'ExportController.php:41', basis: 'ran' },
  { type: 'regrade', finding: 'F6', from: 'should', to: 'blocker', why: 'Both paths leak.' },
  { type: 'add_to_draft', finding: 'F9', text: 'The web path has it too.' },
  { type: 'set_aside', finding: 'F4', why: 'Covered by F9.' },
  { type: 'draft_reply', finding: 'F3', call: 'accept_reason', words: 'Fair: tenants are soft-deleted.' },
  { type: 'follow_up', question: 'Does the CSV download use a third path?' },
]

describe('accepting', () => {
  it('writes only through the page’s own calls, or fills a box', () => {
    const kinds = ALL.map(one => accepting(one, AT, 'Private columns leave.').kind)
    expect(kinds).toEqual(['add', 'decide', 'decide', 'decide', 'fill-reply', 'fill-ask'])
  })

  it('names the reply and the suggestion on every write, and never a session', () => {
    for (const suggestion of ALL) {
      const acting = accepting(suggestion, AT, 'Private columns leave.')
      if (acting.kind === 'add' || acting.kind === 'decide') {
        expect(acting.request.prompted_by).toEqual(AT)
        expect(JSON.stringify(acting.request)).not.toMatch(/agent|session/)
      }
    }
  })

  it('regrades to what the session suggested, as G would', () => {
    expect(accepting(ALL[1]!, AT)).toEqual({
      kind: 'decide',
      seq: 6,
      request: { decision: 'regrade', severity: 'blocker', prompted_by: AT },
    })
  })

  it('adds the sentence to the finding’s own paragraph, as Edit wording would', () => {
    expect(accepting(ALL[2]!, AT, 'Private columns leave.  ')).toEqual({
      kind: 'decide',
      seq: 9,
      request: { decision: 'rewrite', text: 'Private columns leave. The web path has it too.', prompted_by: AT },
    })
  })

  it('opens the place to do it when there is no one-click write behind it', () => {
    expect(accepting(ALL[2]!, AT, null)).toEqual({ kind: 'open', to: 'findings', finding: 'F9' })
    expect(accepting({ type: 'add_to_draft', text: 'One more line.' }, AT)).toEqual({ kind: 'open', to: 'draft', finding: null })
  })

  it('sets aside with the session’s reason as the note, as S would', () => {
    expect(accepting(ALL[3]!, AT)).toMatchObject({ kind: 'decide', seq: 4, request: { decision: 'cut', note: 'Covered by F9.' } })
  })

  it('fills the reply on Replies with the call it drafted, and sends nothing', () => {
    expect(accepting(ALL[4]!, AT)).toEqual({
      kind: 'fill-reply',
      fill: { seq: 3, call: 'accept_reason', words: 'Fair: tenants are soft-deleted.', prompted_by: AT },
    })
  })
})

describe('the words', () => {
  it('words a dismissed regrade as keeping what it is, as S8 does', () => {
    expect(dismissLabel(ALL[1]!)).toBe('Keep as should')
    expect(dismissLabel(ALL[0]!)).toBe('Dismiss')
    expect(acceptLabel(ALL[1]!)).toBe('Regrade')
    expect(acceptLabel(ALL[5]!)).toBe('Put in the ask box')
  })

  it('draws an accepted finding as S8’s line', () => {
    const accepted: SuggestionAnswer = {
      message: 5, suggestion: 1, verdict: 'accepted', reason: null,
      finding_seq: 16, finding_key: 'F16', outcome_id: null, answered_at: '2026-11-21T09:00:00Z',
    }
    expect(answeredLine(ALL[0]!, accepted)).toBe('Added as F16 · should · ran')
    expect(answeredLine(ALL[1]!, { ...accepted, finding_seq: null, finding_key: null })).toBe('Regraded F6 to blocker')
    expect(answeredLine(ALL[1]!, { ...accepted, verdict: 'dismissed', reason: 'F6 is fine' })).toBe('Dismissed · “F6 is fine”')
  })

  it('finds the answer at a place, and the whole answer’s at none', () => {
    const one: SuggestionAnswer = {
      message: 5, suggestion: null, verdict: 'accepted', reason: null,
      finding_seq: 16, finding_key: 'F16', outcome_id: null, answered_at: '2026-11-21T09:00:00Z',
    }
    expect(answerAt({ suggestion_answers: [one] }, 1)).toBeNull()
    expect(answerAt({ suggestion_answers: [one] }, null)).toBe(one)
    expect(turnedInto({ suggestion_answers: [one] }, [])).toEqual({ seq: 16, key: 'F16' })
  })
})

describe('turn into a finding', () => {
  const answer: Answer = {
    kind: 'answer',
    headline: 'One the review didn’t raise: a scheduled export that fails is retried every minute, with no limit at all.',
    points: [{ basis: 'ran', text: 'It retried 30 times in half an hour.' }],
    cites: ['F9', 'RunScheduledExport.php:40'],
  }

  it('takes the headline, the first path cited and the first basis, as a should', () => {
    const request = turnIntoFinding(answer, { seq: 7 })!
    expect(request.where).toBe('RunScheduledExport.php:40')
    expect(request.basis).toBe('ran')
    expect(request.severity).toBe('should')
    expect([...request.title].length).toBeLessThanOrEqual(100)
    expect(request.title.endsWith('…')).toBe(true)
    expect(request.prompted_by).toEqual({ message: 7, suggestion: null })
  })

  it('is not offered on working or can’t, or with nowhere to put it', () => {
    expect(turnIntoFinding({ ...answer, kind: 'cant' }, { seq: 7 })).toBeNull()
    expect(turnIntoFinding({ ...answer, cites: ['F9'] }, { seq: 7 })).toBeNull()
  })

  it('clips at a word', () => {
    expect(clip('short', 100)).toBe('short')
    expect(clip('one two three four', 10)).toBe('one two…')
  })
})
