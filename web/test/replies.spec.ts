import { describe, expect, it } from 'vitest'

import { loadSource } from '../app/data/sample'
import { answersTheAuthor, draftTakesDecisions } from '../app/utils/findings'
import { repliesToAnswer } from '../app/utils/outcomes'
import { groupsOf } from '../app/utils/overview'
import { answerableReplies, answerFor, NO_REPLY_SHOWN, repliesTab, repliesWaiting, STANDING_WORDS } from '../app/utils/replies'
import { primaryAction, replyPath, tabPath } from '../app/utils/reviewTabs'

import type { DraftStatus, Finding, Outcome, ReviewDetail } from '../app/types/desk'

/*
 * DEV-1330: the Replies tab's rules, as functions of the record. The page is asserted in
 * `test/what-happened.nuxt.spec.ts` (the file is named for the step-7 page it replaced).
 *
 * The AC asks for **one test per group**, and for R / A / W to act only where the record
 * already takes them. Both are here.
 */

/** #398, posted: F1 replied to and its thread resolved, F2 replied to and then rejected. */
const REPLIES = 12

function fixture(id: number = REPLIES): ReviewDetail {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return structuredClone(found)
}

let nextId = 9500
function outcome(fields: Partial<Outcome> & Pick<Outcome, 'finding_seq' | 'kind' | 'actor'>): Outcome {
  nextId += 1
  return {
    id: nextId,
    review_id: REPLIES,
    finding_key: `F${fields.finding_seq}`,
    occurred_at: '2026-11-20T10:00:00Z',
    detail: null,
    ...fields,
  }
}

/** The reply the harvester writes, in its own shape. */
function reply(seq: number, body: string, at = '2026-11-20T12:00:00Z'): Outcome {
  return outcome({
    finding_seq: seq,
    kind: 'replied',
    actor: 'author',
    occurred_at: at,
    detail: JSON.stringify({ comment_id: 77 + seq, in_reply_to: 66 + seq, login: 'dana-ruiz', body, time_basis: 'github' }),
  })
}

const NOW = new Date('2026-11-20T14:00:00Z')

/* ================================ The three groups ================================ */

describe('the harvest’s outcomes, in the three groups', () => {
  it('puts a finding whose newest outcome is the author replying in Needs your answer', () => {
    const detail = fixture()
    detail.outcomes.push(reply(1, 'Tenants are never hard-deleted in production.', '2026-11-20T13:00:00Z'))
    const tab = repliesTab(detail, NOW)
    expect(tab.needsAnswer.map(row => row.key)).toEqual(['F1'])
    expect(tab.needsAnswer[0]!.standing).toBe('contested')
    expect(tab.settled.map(row => row.key)).not.toContain('F1')
  })

  it('puts a resolved thread in Settled as Fixed, and a reply followed by more as Answered', () => {
    const tab = repliesTab(fixture(), NOW)
    expect(tab.settled.map(row => [row.key, row.standing])).toEqual([
      ['F1', 'fixed'],
      ['F2', 'answered'],
    ])
    expect(tab.needsAnswer).toEqual([])
  })

  it('puts a posted finding nothing came back on from the author in No reply yet', () => {
    const detail = fixture()
    const tab = repliesTab(detail, NOW)
    const posted = detail.findings.filter(one => one.disposition === 'in_draft'
      && (one.round_status == null || one.round_status === 'new' || one.round_status === 'still_open'))
    const touched = new Set(detail.outcomes.filter(one => one.actor === 'author').map(one => one.finding_seq))
    const quiet = posted.filter(one => !touched.has(one.seq)).map(one => one.key).sort()
    expect(tab.noReply.map(row => row.key).sort()).toEqual(quiet)
    expect(tab.noReply.every(row => row.standing == null && row.said == null)).toBe(true)
    /* A code change is not a reply: a quiet finding whose code moved stays quiet. */
    const moved = detail.outcomes.find(one => one.kind === 'code_changed_after')
    if (moved && !touched.has(moved.finding_seq)) {
      expect(tab.noReply.map(row => row.seq)).toContain(moved.finding_seq)
    }
  })

  it('draws the first four with no reply, and folds the rest', () => {
    expect(NO_REPLY_SHOWN).toBe(4)
  })

  it('is Overview’s grouping, so Since you posted and these groups cannot disagree', () => {
    for (const detail of loadSource().reviews) {
      const groups = groupsOf(detail)
      const tab = repliesTab(detail, NOW)
      const keys = (rows: Array<{ key: string }>) => rows.map(one => one.key).sort()
      expect(keys(tab.needsAnswer), `review ${detail.review.id}`).toEqual(keys(groups.contested))
      expect(keys(tab.settled), `review ${detail.review.id}`).toEqual(keys([...groups.fixed, ...groups.answered]))
      expect(keys(tab.noReply), `review ${detail.review.id}`).toEqual(keys(groups.noReply))
    }
  })

  /* Contract 1: the tab's count is the number in Needs your answer, and nothing else counts. */
  it('counts on the tab exactly what Needs your answer lists', () => {
    for (const detail of loadSource().reviews) {
      expect(repliesTab(detail, NOW).needsAnswer.length, `review ${detail.review.id}`)
        .toBe(repliesToAnswer(detail.outcomes).length)
    }
    const detail = fixture()
    detail.outcomes.push(reply(1, 'No.', '2026-11-20T13:00:00Z'), reply(2, 'Also no.', '2026-11-20T13:30:00Z'))
    expect(repliesTab(detail, NOW).needsAnswer.length).toBe(repliesToAnswer(detail.outcomes).length)
    expect(repliesTab(detail, NOW).needsAnswer.length).toBe(2)
  })

  it('runs each group most serious first, by the review’s own grade', () => {
    const detail = fixture()
    const tab = repliesTab(detail, NOW)
    const order = { blocker: 0, should: 1, question: 2, nit: 3 } as const
    const ranks = tab.noReply.map(row => order[row.severity])
    expect(ranks).toEqual([...ranks].sort((one, two) => one - two))
  })
})

/* ============================ Quoted and attributed ============================ */

describe('the author’s words', () => {
  it('are quoted verbatim and attributed to the login the record holds', () => {
    const words = 'Tenants are never hard-deleted in production, only soft-deleted,\n\nso this path can’t happen. `<script>` stays text.'
    const detail = fixture()
    detail.outcomes.push(reply(1, words, '2026-11-20T13:00:00Z'))
    const said = repliesTab(detail, NOW).needsAnswer[0]!.said!
    expect(said.quote).toBe(words)
    expect(said.who).toBe('dana-ruiz')
    expect(said.when).toBe('1 hour ago')
  })

  it('credits “the author” where the record names no login, and never the harvester', () => {
    const detail = fixture()
    detail.outcomes.push(outcome({ finding_seq: 1, kind: 'replied', actor: 'author', occurred_at: '2026-11-20T13:00:00Z', detail: 'Their own words.' }))
    const said = repliesTab(detail, NOW).needsAnswer[0]!.said!
    expect(said.quote).toBe('Their own words.')
    expect(said.who).toBe('the author')
  })

  it('says a resolution was seen, rather than dating the harvest as the act', () => {
    const fixed = repliesTab(fixture(), NOW).settled.find(row => row.standing === 'fixed')!
    expect(fixed.said!.quote).toBeNull()
    expect(fixed.said!.sentence).toMatch(/resolved/)
  })
})

/* ============================ Raise it again / Accept / Withdraw ============================ */

function carried(over: Partial<Finding> = {}): Finding {
  return {
    ...fixture().findings[0]!,
    continues_review_id: 11,
    continues_round: 1,
    round_status: 'fixed',
    ...over,
  }
}

function at(status: DraftStatus | null, round = 2): Pick<ReviewDetail, 'review' | 'draft_status'> {
  const detail = fixture()
  return { review: { ...detail.review, round }, draft_status: status }
}

describe('the three answers act only where the record takes them', () => {
  it('are offered on a carried, settled finding on a draft that takes decisions', () => {
    for (const status of ['drafting', 'ready', 'sent_back'] as const) {
      expect(answerFor(at(status), carried()), status).toEqual({ offered: true, why: null, later: null })
    }
    for (const settled of ['fixed', 'accepted', 'withdrawn', 'ticketed'] as const) {
      expect(answerFor(at('ready'), carried({ round_status: settled })).offered, settled).toBe(true)
    }
  })

  it('are not offered on a posted or signed-off round, and say why', () => {
    const posted = answerFor(at('posted', 1), carried())
    expect(posted.offered).toBe(false)
    expect(posted.why).toContain('Round 1 is posted')
    const signed = answerFor(at('approved'), carried())
    expect(signed.offered).toBe(false)
    expect(signed.why).toContain('signed off')
    expect(answerFor(at(null), carried()).why).toContain('no draft yet')
  })

  it('are not offered on a finding raised this round, and name where they would be', () => {
    const raised = answerFor(at('ready', 1), carried({ continues_review_id: null, continues_round: null, round_status: 'new' }))
    expect(raised.offered).toBe(false)
    expect(raised.why).toContain('raised in this round')
    expect(raised.why).toContain('on the Findings tab of the round that carries F1 in')
    expect(raised.later).toBeNull()
  })

  /* Question 4, ruled: the answer is on the later round's Findings, linked where the record names it. */
  it('links the later round’s Findings tab where the record names one', () => {
    const detail = fixture()
    const here = { ...at('posted', 1), rounds: [
      { ...(detail.rounds?.[0] ?? {}), review_id: detail.review.id, round: 1, dismissed: false },
      { ...(detail.rounds?.[0] ?? {}), review_id: 4242, round: 2, dismissed: false },
    ] } as unknown as Parameters<typeof answerFor>[0]
    const answer = answerFor(here, carried({ continues_review_id: null, round_status: 'new' }))
    expect(answer.offered).toBe(false)
    expect(answer.why).toContain('on round 2\'s Findings tab, if it carries F1 in')
    expect(answer.later).toEqual({ round: 2, to: '/reviews/4242/findings?finding=F1' })
  })

  it('are not offered on a carried finding still open, which Post decides', () => {
    const open = answerFor(at('ready'), carried({ round_status: 'still_open' }))
    expect(open.offered).toBe(false)
    expect(open.why).toContain('still open')
  })

  /* One rule, read by FindingCall and by Replies: the store's `is_writable` and design.md. */
  it('read the same two rules Findings reads', () => {
    expect(answersTheAuthor(carried())).toBe(true)
    expect(answersTheAuthor(carried({ round_status: 'still_open' }))).toBe(false)
    expect(answersTheAuthor(carried({ continues_review_id: null }))).toBe(false)
    expect(draftTakesDecisions('drafting')).toBe(true)
    expect(draftTakesDecisions('sent_back')).toBe(true)
    expect(draftTakesDecisions('approved')).toBe(false)
    expect(draftTakesDecisions('posted')).toBe(false)
    expect(draftTakesDecisions(null)).toBe(false)
  })
})

/* ============================ The header's Answer F3 ============================ */

describe('the header’s action, gated by the same record test', () => {
  it('offers Answer F3, to that reply on Replies, where the record takes the answer', () => {
    const detail = fixture()
    detail.review = { ...detail.review, round: 2 }
    detail.draft_status = 'ready'
    detail.findings[0] = carried({ seq: detail.findings[0]!.seq, key: detail.findings[0]!.key })
    detail.outcomes.push(reply(detail.findings[0]!.seq, 'It is fixed.', '2026-11-20T13:00:00Z'))
    expect(answerableReplies(detail)).toEqual(['F1'])
    expect(primaryAction(detail.review.id, 'replies_in', answerableReplies(detail)))
      .toEqual({ label: 'Answer F1', to: replyPath(detail.review.id, 'F1') })
    expect(replyPath(detail.review.id, 'F1')).toBe(`${tabPath(detail.review.id, 'replies')}?finding=F1`)
  })

  /* Should 3, ruled: a review waiting on you has a next act; where no answer is taken, it is reading. */
  it('offers Read F1’s reply where the record would refuse every answer, and the tab still counts it', () => {
    const detail = fixture()
    detail.outcomes.push(reply(1, 'No.', '2026-11-20T13:00:00Z'))
    expect(detail.draft_status).toBe('posted')
    expect(repliesToAnswer(detail.outcomes)).toEqual(['F1'])
    expect(answerableReplies(detail)).toEqual([])
    expect(primaryAction(detail.review.id, 'replies_in', repliesWaiting(detail)))
      .toEqual({ label: 'Read F1’s reply', to: replyPath(detail.review.id, 'F1') })
  })

  it('counts several replies to read, and sends them to the tab', () => {
    expect(primaryAction(5, 'replies_in', { answer: [], read: ['F1', 'F3'] }))
      .toEqual({ label: 'Read 2 replies', to: tabPath(5, 'replies') })
    expect(primaryAction(5, 'replies_in', { answer: ['F3'], read: ['F1', 'F3'] }))
      .toEqual({ label: 'Answer F3', to: replyPath(5, 'F3') })
  })

  it('offers nothing where no reply waits at all', () => {
    expect(primaryAction(5, 'replies_in', { answer: [], read: [] })).toBeNull()
    expect(primaryAction(5, 'replies_in', repliesWaiting(fixture()))).toBeNull()
  })
})

/* =================== The developer's answers, said as theirs =================== */

describe('a reply the developer has answered', () => {
  /* Self-review Should 2: after R, A or W the row said "Answered · told by the author". */
  for (const [kind, standing] of [['raised_again', 'raised_again'], ['rejected', 'accepted'], ['withdrawn', 'withdrawn']] as const) {
    it(`reads as ${STANDING_WORDS[standing].word} after ${kind}, on the developer's word`, () => {
      const detail = fixture()
      /* F2: replied to and then rejected by the author — answered, with no resolved thread. */
      const quiet = { seq: 2, key: 'F2' }
      detail.outcomes.push(
        reply(quiet.seq, 'No.', '2026-11-20T13:00:00Z'),
        outcome({ finding_seq: quiet.seq, kind, actor: 'developer', occurred_at: '2026-11-20T13:30:00Z' }),
      )
      const tab = repliesTab(detail, NOW)
      expect(tab.needsAnswer.map(row => row.key)).not.toContain(quiet.key)
      const row = tab.settled.find(one => one.key === quiet.key)!
      expect(row.standing).toBe(standing)
      expect(STANDING_WORDS[standing].basis).toMatch(/^your answer/)
      /* The author's words are still the ones quoted: their reply, not the developer's act. */
      expect(row.said!.quote).toBe('No.')
      expect(row.said!.who).toBe('dana-ruiz')
    })
  }

  it('is the author’s again once the author answers after it', () => {
    const detail = fixture()
    detail.outcomes.push(
      reply(1, 'No.', '2026-11-20T13:00:00Z'),
      outcome({ finding_seq: 1, kind: 'raised_again', actor: 'developer', occurred_at: '2026-11-20T13:30:00Z' }),
      reply(1, 'Still no.', '2026-11-20T13:45:00Z'),
    )
    expect(repliesTab(detail, NOW).needsAnswer.map(row => row.key)).toEqual(['F1'])
  })
})
