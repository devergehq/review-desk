import { describe, expect, it } from 'vitest'

import { loadSource } from '../app/data/sample'
import { BASIS_WORDS, repliesToAnswer, sinceYouPosted } from '../app/utils/outcomes'
import {
  findingsGraded,
  fiveQuestions,
  groupsOf,
  needsYourEye,
  overviewState,
  readyBottomLine,
  sinceBottomLine,
  whatCameBack,
  whereItStands,
} from '../app/utils/overview'
import { primaryAction } from '../app/utils/reviewTabs'
import { stateOfReview } from '../app/utils/stateWord'

import type { Outcome, ReviewDetail } from '../app/types/desk'

/*
 * DEV-1326: Overview's rules, as functions of the record. The page is asserted per state in
 * `test/screens.nuxt.spec.ts`; this holds what each state is chosen from and what may and
 * may not be claimed in it.
 */

/** #514, draft ready, with a structured brief and fifteen findings. */
const READY = 41
/** #93, a session weighing the options. */
const WRITING = 19
/** #398, posted, with replies, a resolution and a code change come back. */
const REPLIES = 12

function fixture(id: number): ReviewDetail {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return structuredClone(found)
}

let nextId = 9000
function outcome(fields: Partial<Outcome> & Pick<Outcome, 'finding_seq' | 'kind' | 'actor'>): Outcome {
  nextId += 1
  return {
    id: nextId,
    review_id: 1,
    finding_key: `F${fields.finding_seq}`,
    occurred_at: '2026-11-20T10:00:00Z',
    detail: null,
    ...fields,
  }
}

describe('which Overview a review draws', () => {
  it('is chosen from the record alone, one state per fixture', () => {
    expect(overviewState(fixture(READY))).toBe('ready')
    expect(overviewState(fixture(WRITING))).toBe('being_written')
    expect(overviewState(fixture(REPLIES))).toBe('replies_in')
  })

  it('follows stateOfReview, so the header and Overview cannot disagree', () => {
    for (const review of loadSource().reviews) {
      const state = stateOfReview(review)
      const overview = overviewState(review)
      if (state === 'replies_in') expect(overview, `review ${review.review.id}`).toBe('replies_in')
      else if (['being_written', 'not_started', 'waiting_for_session'].includes(state)) {
        expect(overview, `review ${review.review.id}`).toBe('being_written')
      }
      else expect(overview, `review ${review.review.id}`).toBe('ready')
    }
  })

  it('draws no primary action while a review is being written', () => {
    for (const review of loadSource().reviews.filter(one => overviewState(one) === 'being_written')) {
      expect(primaryAction(review.review.id, stateOfReview(review), repliesToAnswer(review.outcomes)),
        `review ${review.review.id}`).toBeNull()
    }
  })
})

describe('where it stands', () => {
  it('marks Draft ready as current on a draft-ready review, and the two before it done', () => {
    expect(whereItStands(fixture(READY)).map(stage => stage.mark))
      .toEqual(['done', 'done', 'current', 'todo', 'todo'])
  })

  it('marks the brief as writing while its steps run', () => {
    const stages = whereItStands(fixture(WRITING))
    expect(stages[0]).toEqual({ label: 'Brief · writing', mark: 'writing' })
    expect(stages.slice(1).every(stage => stage.mark === 'todo')).toBe(true)
  })

  it('marks Replies in as current once replies are in', () => {
    expect(whereItStands(fixture(REPLIES)).map(stage => stage.mark))
      .toEqual(['done', 'done', 'done', 'done', 'current'])
  })
})

describe('the bottom line', () => {
  it('is the brief\'s headline and its derived facts when ready', () => {
    const detail = fixture(READY)
    const line = readyBottomLine(detail, 'ready')
    expect(line.sentence).toBe(detail.brief!.headline)
    expect(line.cells.map(cell => cell.label)).toContain('findings')
  })

  it('says it is not written yet, and grades nothing, while being written', () => {
    const detail = fixture(WRITING)
    detail.findings = fixture(READY).findings
    const line = readyBottomLine(detail, 'being_written')
    expect(line.sentence).toMatch(/^Not written yet\./)
    const findings = line.cells.find(cell => cell.label.startsWith('finding'))!
    expect(findings.label).toBe('findings so far')
    expect(findings.sub).toBe('not graded yet')
  })

  it('leaves out a fact the brief has not earned rather than drawing a blank', () => {
    const detail = fixture(WRITING)
    const line = readyBottomLine(detail, 'being_written')
    for (const cell of line.cells) expect(cell.figure).not.toMatch(/^(—|-|)$/)
  })
})

describe('needs your eye', () => {
  it('draws at most three, most serious first, with their severity once graded', () => {
    const cards = needsYourEye(fixture(READY))
    expect(cards.length).toBeLessThanOrEqual(3)
    expect(cards.every(card => card.severity != null)).toBe(true)
    expect(cards[0]!.severity).toBe('blocker')
    expect(cards.every(card => card.to === '/reviews/41/findings')).toBe(true)
  })

  it('draws no severity while the review is being written', () => {
    const detail = fixture(WRITING)
    detail.findings = fixture(READY).findings
    const cards = needsYourEye(detail)
    expect(cards.length).toBeGreaterThan(0)
    expect(cards.every(card => card.severity === null)).toBe(true)
  })

  /*
   * Self-review F3: a review between steps — code review done, no draft session yet — draws
   * the being-written Overview, and its findings are graded all the same.
   */
  it('grades by the code review step, not by the Overview state', () => {
    const detail = fixture(READY)
    detail.draft_status = null
    detail.layers = detail.layers.map(layer => layer.name === 'draft' ? { ...layer, state: 'pending' } : layer)
    expect(overviewState(detail)).toBe('being_written')
    expect(findingsGraded(detail)).toBe(true)
    expect(needsYourEye(detail).every(card => card.severity != null)).toBe(true)
    const findings = readyBottomLine(detail, 'being_written').cells.find(cell => cell.label.startsWith('finding'))!
    expect(findings.label).toBe('findings')
    expect(findings.sub).not.toBe('not graded yet')
  })
})

describe('the five questions', () => {
  it('answers each from the record, linking to the tab that holds it', () => {
    const questions = fiveQuestions(fixture(READY))
    expect(questions.map(one => one.asks)).toEqual([
      'What does it do?', 'Is it the right way?', 'What\'s wrong with it?', 'What am I saying?', 'What wasn\'t checked?',
    ])
    expect(questions[0]!.link!.to).toBe('/reviews/41/understanding')
    expect(questions[1]!.link!.to).toBe('/reviews/41/understanding#what-was-chosen')
    expect(questions[2]!.link!.to).toBe('/reviews/41/findings')
    expect(questions[3]!.link!.to).toBe('/reviews/41/draft')
    expect(questions[4]!.link).toEqual({
      label: 'Understanding › Not verified',
      to: '/reviews/41/understanding#not-verified',
    })
    expect(questions.every(one => one.mark === 'written' || one.mark === 'none_recorded')).toBe(true)
  })

  /* Self-review N1: a finished step whose field the brief lacks is not "Not started". */
  it('says the brief records no answer when a finished step wrote none', () => {
    const detail = fixture(READY)
    detail.brief = { ...detail.brief!, flow_after_caption: null, approach_verdict: null, options: [] }
    const [does, right] = fiveQuestions(detail)
    expect(does!.answer).toBe('The record holds no answer to this.')
    expect(right!.answer).toBe('The record holds no answer to this.')
    /* Round 2, N3: true of the record, and not counted as written. */
    expect(does!.mark).toBe('none_recorded')
    expect(right!.mark).toBe('none_recorded')
  })

  /* Round 2, N4: a skipped step is over, so it is not "Not started". */
  it('treats a skipped step as over when the brief holds no answer', () => {
    const detail = fixture(READY)
    detail.brief = { ...detail.brief!, flow_after_caption: null, options: [] }
    detail.layers = detail.layers.map(layer => layer.name === 'solution_map' ? { ...layer, state: 'skipped' } : layer)
    expect(fiveQuestions(detail)[0]!.mark).toBe('none_recorded')
  })

  /* Round 2, S1: a skipped code review did not grade anything, or check anything. */
  it('never reads a skipped code review as graded', () => {
    const detail = fixture(READY)
    detail.not_checked = null
    detail.layers = detail.layers.map(layer => layer.name === 'code_review' ? { ...layer, state: 'skipped' } : layer)
    expect(findingsGraded(detail)).toBe(false)
    expect(needsYourEye(detail).every(card => card.severity === null)).toBe(true)
    const [, , wrong, , unchecked] = fiveQuestions(detail)
    expect(wrong!.mark).not.toBe('written')
    expect(unchecked!.answer).not.toBe('The review records nothing it left unchecked.')
    expect(unchecked!.mark).not.toBe('written')
  })

  it('says Writing now or Not started for what the brief has not reached', () => {
    const marks = fiveQuestions(fixture(WRITING)).map(one => one.mark)
    expect(marks).toContain('writing')
    expect(marks).toContain('not_started')
    for (const question of fiveQuestions(fixture(WRITING)).filter(one => one.mark !== 'written')) {
      expect(question.answer).toMatch(/^(Writing now\.|Not started\.)/)
    }
  })
})

describe('since you posted: the outcome groups', () => {
  const posted = [1, 2, 3, 4, 5].map(seq => ({ seq, key: `F${seq}` }))

  it('puts each posted finding in exactly one of four groups', () => {
    const groups = sinceYouPosted(posted, [
      outcome({ finding_seq: 1, kind: 'replied', actor: 'author', occurred_at: '2026-11-20T10:00:00Z' }),
      outcome({ finding_seq: 2, kind: 'thread_resolved', actor: 'author' }),
      outcome({ finding_seq: 3, kind: 'replied', actor: 'author', occurred_at: '2026-11-20T09:00:00Z' }),
      outcome({ finding_seq: 3, kind: 'raised_again', actor: 'developer', occurred_at: '2026-11-20T11:00:00Z' }),
    ])
    expect(groups.contested.map(one => one.key)).toEqual(['F1'])
    expect(groups.fixed.map(one => one.key)).toEqual(['F2'])
    expect(groups.answered.map(one => one.key)).toEqual(['F3'])
    expect(groups.noReply.map(one => one.key)).toEqual(['F4', 'F5'])
    const all = [...groups.contested, ...groups.fixed, ...groups.answered, ...groups.noReply].map(one => one.key).sort()
    expect(all).toEqual(posted.map(one => one.key).sort())
  })

  it('agrees with repliesToAnswer about what is contested, on every fixture', () => {
    for (const review of loadSource().reviews) {
      expect(groupsOf(review).contested.map(one => one.key), `review ${review.review.id}`)
        .toEqual(repliesToAnswer(review.outcomes))
    }
  })

  it('states a basis on every outcome, and never claims a re-run', () => {
    const detail = fixture(REPLIES)
    const groups = groupsOf(detail)
    const everyGroup = [...groups.fixed, ...groups.answered, ...groups.contested, ...groups.noReply, ...groups.newCode]
    expect(everyGroup.length).toBeGreaterThan(0)
    for (const one of everyGroup) expect(one.basis).not.toBe('ran')
    for (const row of whatCameBack(detail, groups)) {
      expect(Object.keys(BASIS_WORDS)).toContain(row.basis)
      expect(row.basis).not.toBe('ran')
    }
    for (const cell of sinceBottomLine(detail, groups).cells) {
      expect(cell.sub ?? '').not.toMatch(/\bran\b/)
    }
  })

  it('draws New code under Fn only when the comparison says the lines moved', () => {
    const moved = (lines: unknown) => outcome({
      finding_seq: 2,
      kind: 'code_changed_after',
      actor: 'harvester',
      detail: JSON.stringify({ commit: '3c1e2a9f00', file: 'a.php', line: 41, lines_changed: lines }),
    })
    const detail = fixture(REPLIES)

    const yes = sinceYouPosted(posted, [moved(true)])
    expect(yes.newCode.map(one => one.key)).toEqual(['F2'])
    expect(yes.newCode[0]!.basis).toBe('read')
    expect(whatCameBack(detail, yes).map(row => row.title)).toContain('New code under F2')

    for (const lines of [false, undefined, 'yes']) {
      const no = sinceYouPosted(posted, [moved(lines)])
      expect(no.newCode, `lines_changed: ${String(lines)}`).toEqual([])
      expect(whatCameBack(detail, no).some(row => row.title.startsWith('New code'))).toBe(false)
      expect(no.commits, 'the commit is still counted').toEqual(['3c1e2a9'])
    }
  })

  /* Self-review F1: the harvester's newest comparison is the current answer. */
  it('reads the newest comparison per finding, so a later "not moved" wins', () => {
    const compared = (lines: boolean, at: string, commit: string) => outcome({
      finding_seq: 2,
      kind: 'code_changed_after',
      actor: 'harvester',
      occurred_at: at,
      detail: JSON.stringify({ commit, file: 'a.php', line: 41, lines_changed: lines }),
    })
    const groups = sinceYouPosted(posted, [
      compared(true, '2026-11-20T10:00:00Z', '1111111aaa'),
      compared(false, '2026-11-20T11:00:00Z', '2222222bbb'),
    ])
    expect(groups.newCode).toEqual([])
    expect(groups.commits).toEqual(['2222222'])
  })

  /* Self-review F2: the harvester compares every finding with a file, posted or not. */
  it('draws New code only for the findings the draft posted', () => {
    const groups = sinceYouPosted(posted, [outcome({
      finding_seq: 7,
      kind: 'code_changed_after',
      actor: 'harvester',
      detail: JSON.stringify({ commit: '7777777ccc', file: 'b.php', line: 9, lines_changed: true }),
    })])
    expect(groups.newCode).toEqual([])
    expect(groups.commits).toEqual([])
  })

  it('counts no commits when no comparison named one', () => {
    const groups = sinceYouPosted(posted, [])
    expect(groups.commits).toEqual([])
    expect(sinceBottomLine(fixture(REPLIES), groups).cells.some(cell => /commit/.test(cell.label))).toBe(false)
  })
})
