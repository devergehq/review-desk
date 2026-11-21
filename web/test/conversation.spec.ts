import { describe, expect, it } from 'vitest'

import type { Answer, MessageAbout, ReviewDetail, ReviewMessage, SessionReading, Suggestion } from '../app/types/desk'
import { answering, answeringMode, canAsk } from '../app/utils/answering'
import { citeTarget, type CiteSource } from '../app/utils/cites'
import {
  aboutHere,
  aboutLabel,
  askAgainInTheFormat,
  askedOnLabel,
  askedSoFar,
  askPlaceholder,
  foldedReply,
  freshSessionCommand,
  referenceFor,
  suggestedFrom,
  suggestedQuestions,
  suggestionLine,
  threadOf,
  writtenSoFar,
} from '../app/utils/conversation'

/*
 * The rules the conversation beside a review draws by (DEV-1333): who is answering, where a
 * cite goes, and what the thread, the counts and the About chip say.
 */

const FINDINGS = [
  { key: 'F1', seq: 1, file: 'app/Reporting/ExportContext.php', line: 44 },
  { key: 'F9', seq: 9, file: 'app/Policies/ColumnPolicy.php', line: 52 },
  { key: 'F11', seq: 11, file: 'app/Http/Controllers/ExportController.php', line: 41 },
  { key: 'F14', seq: 14, file: null, line: null },
] as unknown as ReviewDetail['findings']

const SOURCE: CiteSource = {
  review: { id: 41, repo: 'acme/portal', pr: 514, head_sha: '7f3c2d1a9b8e' } as ReviewDetail['review'],
  findings: FINDINGS,
  criteria: [{ position: 1 }, { position: 3 }] as ReviewDetail['criteria'],
  brief: {
    options: [{ key: 'A', chosen: false }, { key: 'C', chosen: true }],
  } as unknown as ReviewDetail['brief'],
}

let seq = 0
function message(over: Partial<ReviewMessage>): ReviewMessage {
  seq += 1
  return {
    review_id: 41,
    seq,
    step: 'code_review',
    author: 'developer',
    kind: 'ask',
    body: 'Why?',
    in_reply_to: null,
    created_at: '2026-11-21T06:00:00Z',
    read_at: null,
    read_by_session: null,
    about: { tab: 'findings', finding: null },
    parsed: false,
    answer: null,
    superseded_by: null,
    suggestion_answers: [],
    ...over,
  }
}

function reply(answer: Answer | null, over: Partial<ReviewMessage> = {}): ReviewMessage {
  return message({
    author: 'session',
    kind: 'reply',
    parsed: answer != null,
    answer,
    body: answer ? JSON.stringify(answer) : 'So looking at this more broadly…',
    ...over,
  })
}

function reading(over: Partial<SessionReading>): SessionReading {
  return {
    status: 'finished',
    allele_state: 'idle',
    state_age_secs: 12,
    detail: null,
    read_at: '2026-11-21T06:00:00Z',
    ...over,
  }
}

describe('who is answering', () => {
  it('reads no reading and nothing a session wrote as nobody: read only', () => {
    expect(answering(null)).toEqual({
      mode: 'none',
      title: 'Ask this review',
      who: 'No agent connected',
      state: 'read only',
      tone: 'quiet',
    })
    expect(answering(null, [message({ author: 'developer' })]).mode).toBe('none')
    expect(canAsk('none')).toBe(false)
  })

  it('reads no reading and a session’s reply in the record as one way, in S13’s words', () => {
    expect(answering(null, [reply(null)])).toEqual({
      mode: 'one_way',
      title: 'From your session',
      who: 'Your own Claude Code session',
      state: 'one way · no Allele',
      tone: 'quiet',
    })
    expect(canAsk('one_way')).toBe(false)
  })

  it('reads any reading as two way, whatever the record holds', () => {
    for (const status of ['running', 'finished', 'awaiting_input', 'gone', 'unknown', 'unreachable'] as const) {
      expect(answeringMode(reading({ status }), [reply(null)])).toBe('two_way')
      expect(answeringMode(reading({ status }), [])).toBe('two_way')
    }
    expect(canAsk('two_way')).toBe(true)
  })

  it('says S6’s words for the state, and that it is via Allele', () => {
    expect(answering(reading({ status: 'finished' }))).toMatchObject({
      mode: 'two_way',
      title: 'Ask this review',
      who: 'The session that wrote this review',
      state: 'idle · via Allele',
    })
    expect(answering(reading({ status: 'running' })).state).toBe('working · via Allele')
  })

  it('says a suspended session from a gone one, as the strip does', () => {
    expect(answering(reading({ status: 'gone', allele_state: 'suspended' })).state).toBe('suspended in Allele · via Allele')
    expect(answering(reading({ status: 'gone', allele_state: 'done' })).state).toBe('gone · via Allele')
    expect(answering(reading({ status: 'unreachable' })).state).toBe('can’t reach Allele')
  })
})

describe('one way', () => {
  it('counts what the session wrote by tab, the newest word on each question only', () => {
    const written = writtenSoFar([
      message({ author: 'developer', kind: 'ask', about: { tab: 'findings', finding: null } }),
      reply(null, { seq: 2, about: { tab: 'findings', finding: 9 }, superseded_by: 3 }),
      reply(null, { seq: 3, about: { tab: 'findings', finding: 9 } }),
      reply(null, { seq: 4, about: { tab: 'understanding', finding: null } }),
    ])
    expect(written.map(one => [one.label, one.count])).toEqual([['Understanding', 1], ['Findings', 1]])
  })

  it('writes a reference with the review, the reply command and the README’s contract section', () => {
    const reference = referenceFor(41, 'code_review', { tab: 'findings', finding: 9 }, [])
    expect(reference).toContain('Review Desk review 41, about F9.')
    expect(reference).toContain('review-desk message reply --review 41 --step code_review --answer -')
    expect(reference).toContain('README.md, "Talking to the review\'s session"')
    expect(reference).not.toContain('--to')
    expect(reference).not.toContain('The question:')
  })

  it('carries the question the ask box holds, as an accepted follow_up leaves it', () => {
    expect(referenceFor(41, 'code_review', { tab: 'overview', finding: null }, [], { question: ' And the CSV? ' }))
      .toContain('The question: And the CSV?')
  })

  it('asks for an unparsed reply again in the format', () => {
    expect(referenceFor(41, 'draft', { tab: 'draft', finding: null }, [], { again: { seq: 7 } }))
      .toContain('Reply 7 didn’t follow the answer format; send it again in the format.')
  })

  it('shows a fresh session as a command, which starts nothing', () => {
    expect(freshSessionCommand(41)).toBe('review-desk message list --review 41')
  })
})

describe('where a cite goes', () => {
  it('links a finding the review has to Findings with it selected', () => {
    expect(citeTarget('F9', SOURCE)).toEqual({ kind: 'page', text: 'F9', to: '/reviews/41/findings?finding=F9' })
  })

  it('draws a finding the review does not have as text, never a link', () => {
    expect(citeTarget('F99', SOURCE)).toEqual({ kind: 'text', text: 'F99' })
  })

  it('links a path and line to the finding at exactly that place', () => {
    expect(citeTarget('app/Http/Controllers/ExportController.php:41', SOURCE))
      .toEqual({ kind: 'page', text: 'app/Http/Controllers/ExportController.php:41', to: '/reviews/41/findings?finding=F11' })
  })

  it('draws a path no finding sits at as text, partial paths included', () => {
    expect(citeTarget('ExportController.php:41', SOURCE).kind).toBe('text')
    expect(citeTarget('app/Http/Controllers/ExportController.php:42', SOURCE).kind).toBe('text')
  })

  it('links a criterion the review records, and not one it does not', () => {
    expect(citeTarget('criterion 3', SOURCE))
      .toEqual({ kind: 'page', text: 'criterion 3', to: '/reviews/41/understanding#ticket-against-what-was-built' })
    expect(citeTarget('Criterion 2', SOURCE).kind).toBe('text')
  })

  it('links an option the brief weighed, and not one it did not', () => {
    expect(citeTarget('option C', SOURCE))
      .toEqual({ kind: 'page', text: 'option C', to: '/reviews/41/understanding#what-was-chosen' })
    expect(citeTarget('option Z', SOURCE).kind).toBe('text')
    expect(citeTarget('option C', { ...SOURCE, brief: null }).kind).toBe('text')
  })

  it('links a commit the record holds to the repository on GitHub', () => {
    expect(citeTarget('7f3c2d1', SOURCE))
      .toEqual({ kind: 'away', text: '7f3c2d1', href: 'https://github.com/acme/portal/commit/7f3c2d1' })
    const rounds = [{ head_sha: 'abc1234def' }] as unknown as ReviewDetail['rounds']
    expect(citeTarget('abc1234', { ...SOURCE, rounds }).kind).toBe('away')
  })

  it('links a commit a finding’s line was read at, and a full SHA the record holds short', () => {
    const findings = [...FINDINGS, { key: 'F20', seq: 20, file: 'a.php', line: 1, line_sha: 'c0ffee1234' }] as unknown as ReviewDetail['findings']
    expect(citeTarget('c0ffee1', { ...SOURCE, findings }).kind).toBe('away')
    const short = { ...SOURCE, review: { ...SOURCE.review, head_sha: '7f3c2d1' } }
    expect(citeTarget('7f3c2d1a9b8e0011', short).kind).toBe('away')
  })

  it('draws a commit the record does not hold as text, never a link to a 404', () => {
    expect(citeTarget('1234567', SOURCE)).toEqual({ kind: 'text', text: '1234567' })
    expect(citeTarget('deadbee', SOURCE).kind).toBe('text')
  })

  it('draws a commit in a repository it cannot address as text', () => {
    const odd = { ...SOURCE, review: { ...SOURCE.review, repo: 'not a repo' } }
    expect(citeTarget('7f3c2d1', odd).kind).toBe('text')
  })

  it('draws anything else as the session wrote it', () => {
    expect(citeTarget('The ticket', SOURCE)).toEqual({ kind: 'text', text: 'The ticket' })
  })
})

describe('the thread', () => {
  it('leaves out a reply something later replaced', () => {
    const ask = message({})
    const working = reply({ kind: 'working', headline: 'Looking', activity: ['Read it'] }, { superseded_by: 99 })
    const done = reply({ kind: 'answer', headline: 'Yes', cites: ['F9'] })
    expect(threadOf([ask, working, done]).map(one => one.seq)).toEqual([ask.seq, done.seq])
  })

  it('counts the developer’s questions by the tab each was about, in the strip’s order', () => {
    const messages = [
      message({ about: { tab: 'findings', finding: 9 } }),
      message({ about: { tab: 'understanding', finding: null } }),
      message({ about: { tab: 'findings', finding: null } }),
      message({ kind: 'send_back', about: { tab: 'draft', finding: null } }),
      reply({ kind: 'answer', headline: 'Yes', cites: ['F9'] }, { about: { tab: 'draft', finding: null } }),
    ]
    expect(askedSoFar(messages)).toEqual([
      { tab: 'understanding', label: 'Understanding', count: 1 },
      { tab: 'findings', label: 'Findings', count: 2 },
    ])
  })

  it('draws no counts when nothing has been asked', () => {
    expect(askedSoFar([])).toEqual([])
  })
})

describe('what a question is about', () => {
  it('takes the selected finding on Findings and on Replies', () => {
    expect(aboutHere('findings', 'F9', FINDINGS)).toEqual({ tab: 'findings', finding: 9 })
    expect(aboutHere('replies', 'F9', FINDINGS)).toEqual({ tab: 'replies', finding: 9 })
  })

  it('takes no finding on another tab, or one the review does not have', () => {
    expect(aboutHere('draft', 'F9', FINDINGS)).toEqual({ tab: 'draft', finding: null })
    expect(aboutHere('findings', 'F99', FINDINGS)).toEqual({ tab: 'findings', finding: null })
    expect(aboutHere('findings', null, FINDINGS)).toEqual({ tab: 'findings', finding: null })
  })

  it('words the chip as S6 does', () => {
    const on = (about: MessageAbout) => aboutLabel(about, FINDINGS)
    expect(on({ tab: 'findings', finding: 9 })).toBe('F9 · app/Policies/ColumnPolicy.php:52')
    expect(on({ tab: 'findings', finding: 14 })).toBe('F14')
    expect(on({ tab: 'replies', finding: 9 })).toBe('the reply on F9')
    expect(on({ tab: 'overview', finding: null })).toBe('the whole review')
    expect(on({ tab: 'understanding', finding: null })).toBe('Understanding')
  })

  it('heads a question with what it was asked on', () => {
    expect(askedOnLabel({ tab: 'findings', finding: 9 })).toBe('on F9')
    expect(askedOnLabel({ tab: 'understanding', finding: null })).toBe('on Understanding')
    expect(askedOnLabel({ tab: 'overview', finding: null })).toBeNull()
  })

  it('puts the finding or the pull request in the ask box', () => {
    expect(askPlaceholder({ tab: 'findings', finding: 9 }, 514)).toBe('Ask about F9…')
    expect(askPlaceholder({ tab: 'overview', finding: null }, 514)).toBe('Ask about #514…')
  })
})

describe('the questions an empty conversation starts with', () => {
  const brief = (options: Array<[string, boolean]>, blind: string | null) => ({
    options: options.map(([key, chosen]) => ({ key, chosen })),
    blind_pass: blind ? { pick_key: blind } : null,
  }) as unknown as ReviewDetail['brief']

  it('offers all four when the record holds each one’s reason', () => {
    const questions = suggestedQuestions({
      brief: brief([['A', false], ['B', false], ['C', true]], 'A'),
      not_checked: 'The CSV download path.',
    })
    expect(questions.map(one => one.asks)).toEqual([
      'What would you expect to go wrong after this merges?',
      'Why option C over the other two?',
      'Where did the second reader disagree?',
      'Does what wasn’t checked matter here?',
    ])
    expect(suggestedFrom(questions))
      .toBe('Written from this review: its approach, its disagreements, and what it didn’t check.')
  })

  it('offers only the one that asserts nothing when the record holds none of it', () => {
    const questions = suggestedQuestions({ brief: null, not_checked: null })
    expect(questions.map(one => one.asks)).toEqual(['What would you expect to go wrong after this merges?'])
    expect(suggestedFrom(questions)).toBeNull()
  })

  it('does not say the second reader disagreed when it picked the built option', () => {
    const questions = suggestedQuestions({ brief: brief([['A', false], ['C', true]], 'C'), not_checked: null })
    expect(questions.map(one => one.from)).toEqual(['risk', 'approach'])
  })
})

describe('a reply that did not follow the format', () => {
  it('draws a short one whole, with nothing folded', () => {
    expect(foldedReply('Short.')).toEqual({ opening: 'Short.', rest: 0, restText: '' })
  })

  it('cuts a long one at a word near 200 characters, and folds every word after it', () => {
    const words = Array.from({ length: 120 }, (_, i) => `word${i}`)
    const folded = foldedReply(words.join(' '))
    expect(folded.opening.length).toBeLessThanOrEqual(200)
    expect(folded.opening.endsWith(' ')).toBe(false)
    expect(`${folded.opening} ${folded.restText}`).toBe(words.join(' '))
    expect(folded.rest).toBe(folded.restText.split(' ').length)
  })

  it('asks again naming the reply and the contract', () => {
    expect(askAgainInTheFormat({ seq: 7 })).toContain('reply 7')
    expect(askAgainInTheFormat({ seq: 7 })).toContain('docs/answer-contract.md')
  })
})

describe('a suggestion in a line', () => {
  it('words each of the six', () => {
    const lines = ([
      { type: 'new_finding', severity: 'should', title: 'Retries have no limit', where: 'Run.php:40', basis: 'ran' },
      { type: 'regrade', finding: 'F9', from: 'should', to: 'blocker', why: 'Both paths leak.' },
      { type: 'add_to_draft', finding: 'F9', text: 'It covers both paths.' },
      { type: 'set_aside', finding: 'F4', why: 'Naming only.' },
      { type: 'draft_reply', finding: 'F3', call: 'accept_reason', words: 'Fair.' },
      { type: 'follow_up', question: 'And the CSV path?' },
    ] as Suggestion[]).map(one => suggestionLine(one).line)
    expect(lines).toEqual([
      'New finding: Retries have no limit',
      'Regrade F9 from should to blocker',
      'Add to F9 in the draft',
      'Set F4 aside',
      'Accept their reason on F3',
      'Ask next: And the CSV path?',
    ])
  })
})
