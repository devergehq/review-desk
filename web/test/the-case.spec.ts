import { describe, expect, it } from 'vitest'

import { loadSource } from '../app/data/sample'
import {
  answerFacts,
  BAND_OF_STEP,
  builtHeading,
  CASE_STEPS,
  fixedByLabel,
  fixesOf,
  foldSummary,
  isCaseStep,
  partsFixing,
} from '../app/utils/theCase'

import type { Brief } from '../app/types/desk'

/*
 * **The rules behind the case page's opening line, tested without rendering anything.**
 *
 * Patrick asked for the line on 16 October 2026 and set one condition on it: it is derived
 * from existing review data — no new session-written summary and **no new field** — and a
 * fact that cannot be derived is left out rather than guessed at. That condition is what
 * every test here is about: each fact is a separate test of the record, so a brief half way
 * through being written draws the facts it has earned and no others.
 *
 * Review 41 is the contract's invented example at real length, the same brief
 * `contract/examples/confirmBrief.response.real-length.json` holds. The synthetic briefs
 * below are for the states no fixture has.
 */

/** The review carrying the structured brief at real length. */
const STRUCTURED = 41

function fixture(id: number): Brief {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found?.brief) throw new Error(`the fixtures hold no brief for review ${id}`)
  return found.brief
}

/** A brief with nothing in it, to add exactly the fields a test is about. */
function bare(over: Partial<Brief> = {}): Brief {
  return {
    review_id: 1,
    problem: 'something was wrong',
    options: [],
    problems: [],
    parts: [],
    open_choices: [],
    disagreements: [],
    flow_before: [],
    flow_after: [],
    confirmed: false,
    confirmation_required: true,
    blind_pass_state: 'not_run',
    updated_at: '2026-10-16T00:00:00Z',
    ...over,
  } as Brief
}

function problem(number: number, label: string) {
  return { number, label, was_wrong: `${label} was wrong`, detail_before: [], detail_after: [] }
}

function option(
  key: string,
  over: Partial<Brief['options'][number]> = {},
): Brief['options'][number] {
  return {
    key,
    title: `option ${key}`,
    argument_for: 'for',
    argument_against: 'against',
    chosen: false,
    proposed_by: 'ticket',
    verdicts: [],
    ...over,
  }
}

function part(over: Partial<Brief['parts'][number]> = {}): Brief['parts'][number] {
  return {
    title: 'a part',
    summary: 'what it did',
    fixes: [],
    files: [],
    lines_added: 0,
    lines_removed: 0,
    ...over,
  }
}

describe('the answer line', () => {
  it('names the four facts, in the order the bands are in', () => {
    const facts = answerFacts(fixture(STRUCTURED))
    expect(facts.map(fact => fact.kind)).toEqual(['problems', 'built', 'fixes', 'blind'])
    expect(facts.map(fact => fact.says)).toEqual([
      '3 problems',
      'option C built',
      'fixes all 3',
      'blind pass agrees',
    ])
  })

  it('counts the problems, which is the one fact a problem half alone has earned', () => {
    /*
     * A brief arrives in two writes: the problem half, then everything else. For the minutes
     * in between there is no option, no verdict and no pass — and one fact.
     */
    const facts = answerFacts(bare({ problems: [problem(1, 'one'), problem(2, 'two')] }))
    expect(facts.map(fact => fact.says)).toEqual(['2 problems'])
  })

  it('says nothing at all about a brief that has recorded none of it', () => {
    expect(answerFacts(bare())).toEqual([])
  })

  it('leaves the built option out when the brief names none', () => {
    const facts = answerFacts(bare({
      problems: [problem(1, 'one')],
      options: [option('A'), option('B')],
    }))
    expect(facts.map(fact => fact.kind)).toEqual(['problems'])
  })

  it('reads how far it goes off the built option’s verdicts, not off the parts', () => {
    /*
     * **The distinction the line turns on.** On review #9639 the built option's verdicts are
     * three `fixed` and one `partly` — "fixes 3 of 4" — while the parts between them claim
     * all four problems. A part claiming a problem says the change touched it; a verdict
     * says the approach solves it, and the approach is what the line is about.
     */
    const brief = bare({
      problems: [problem(1, 'one'), problem(2, 'two')],
      options: [option('A', {
        chosen: true,
        verdicts: [
          { problem: 1, verdict: 'fixed', why: 'it does' },
          { problem: 2, verdict: 'partly', why: 'only here' },
        ],
      })],
      parts: [part({ fixes: [1, 2] })],
    })
    expect(fixesOf(brief)).toEqual({ fixed: 1, of: 2 })
    expect(answerFacts(brief).find(fact => fact.kind === 'fixes')?.says).toBe('fixes 1 of 2')
  })

  it('says "all" rather than the same number twice', () => {
    const brief = bare({
      problems: [problem(1, 'one'), problem(2, 'two')],
      options: [option('A', {
        chosen: true,
        verdicts: [
          { problem: 1, verdict: 'fixed', why: 'yes' },
          { problem: 2, verdict: 'fixed', why: 'yes' },
        ],
      })],
    })
    expect(answerFacts(brief).find(fact => fact.kind === 'fixes')?.says).toBe('fixes all 2')
  })

  it('leaves the figure out for an option nobody judged against the problems', () => {
    /*
     * An option with no verdicts is an absence of judgement, and "fixes 0 of 2" is a claim
     * about the change. The two are not the same statement.
     */
    const brief = bare({
      problems: [problem(1, 'one'), problem(2, 'two')],
      options: [option('A', { chosen: true, verdicts: [] })],
    })
    expect(fixesOf(brief)).toBeNull()
    expect(answerFacts(brief).map(fact => fact.kind)).toEqual(['problems', 'built'])
  })

  it('leaves the figure out for a brief that numbers no problems', () => {
    const brief = bare({ options: [option('A', { chosen: true })] })
    expect(fixesOf(brief)).toBeNull()
    expect(answerFacts(brief).map(fact => fact.kind)).toEqual(['built'])
  })

  it('names the option an independent pass would have picked instead', () => {
    const brief = bare({
      problems: [problem(1, 'one')],
      options: [option('A', { chosen: true }), option('D')],
      blind_pass_state: 'ran',
      blind_pass: { given: 'the problem', pick_key: 'D', pick_why: 'cleaner', looked_up: 'nothing', questions: [] },
    })
    expect(answerFacts(brief).find(fact => fact.kind === 'blind')?.says)
      .toBe('blind pass would pick D')
  })

  it('says a pass that ran and recommended nothing did exactly that', () => {
    const brief = bare({
      options: [option('A', { chosen: true })],
      blind_pass_state: 'ran',
      blind_pass: { given: 'the problem', pick_key: null, pick_why: null, looked_up: 'nothing', questions: [] },
    })
    expect(answerFacts(brief).find(fact => fact.kind === 'blind')?.says)
      .toBe('blind pass picked none of them')
  })

  it('says nothing about a pass that never ran', () => {
    /*
     * Not an absence in the record: Review Desk runs no blind pass and no session reported
     * one. A line reporting on a thing that did not happen is worse than a shorter line.
     */
    const brief = bare({
      options: [option('A', { chosen: true })],
      blind_pass_state: 'not_run',
    })
    expect(answerFacts(brief).some(fact => fact.kind === 'blind')).toBe(false)
  })

  it('reads an absent blind state as a pass that never ran', () => {
    /*
     * Every brief recorded before the field existed carries `undefined`, and the sample
     * fixtures are such briefs. `!== 'not_run'` is true of `undefined`, which is how three
     * places on the old page each claimed a pass had run.
     */
    const brief = bare({
      options: [option('A', { chosen: true })],
      blind_pass_state: undefined as unknown as 'not_run',
    })
    expect(answerFacts(brief).some(fact => fact.kind === 'blind')).toBe(false)
  })

  it('ignores a pick the option list does not carry', () => {
    const brief = bare({
      options: [option('A', { chosen: true })],
      blind_pass_state: 'ran',
      blind_pass: { given: 'x', pick_key: 'Z', pick_why: 'why', looked_up: 'nothing', questions: [] },
    })
    expect(answerFacts(brief).find(fact => fact.kind === 'blind')).toBeUndefined()
  })
})

describe('which part fixes which problem', () => {
  const brief = bare({
    problems: [problem(1, 'one'), problem(2, 'two'), problem(3, 'three')],
    parts: [part({ fixes: [2] }), part({ fixes: [1, 2] }), part({ fixes: [] })],
  })

  it('answers the part numbers the rows of band 3 carry', () => {
    expect(partsFixing(brief, 1)).toEqual([2])
    expect(partsFixing(brief, 2)).toEqual([1, 2])
  })

  it('reads one as "part" and more than one as "parts"', () => {
    expect(fixedByLabel(brief, 1)).toBe('part 2')
    expect(fixedByLabel(brief, 2)).toBe('parts 1, 2')
  })

  it('answers nothing for a problem no part claims', () => {
    /* A fact about the change rather than a gap in the record, and the band says so. */
    expect(partsFixing(brief, 3)).toEqual([])
    expect(fixedByLabel(brief, 3)).toBeNull()
  })
})

describe('what the bands head themselves with', () => {
  it('counts every file the parts name, and how many are outside the diff', () => {
    /*
     * `solution.vue`'s own measurement, and the obvious fix is wrong: counting only changed
     * files makes the heading disagree with the column of rows under it, each of which
     * counts claimed files.
     */
    const brief = bare({
      parts: [
        part({
          lines_added: 10,
          lines_removed: 2,
          files: [{ path: 'a.ts', lines_added: 10, lines_removed: 2, in_diff: true }],
        }),
        part({
          lines_added: 0,
          lines_removed: 0,
          files: [{ path: 'b.ts', lines_added: 0, lines_removed: 0, in_diff: false }],
        }),
      ],
    })
    expect(builtHeading(brief)).toBe('2 parts · 2 files (1 not changed) · +10 −2')
  })

  it('says nothing about files outside the diff when there are none', () => {
    const brief = bare({
      parts: [part({
        lines_added: 3,
        lines_removed: 1,
        files: [{ path: 'a.ts', lines_added: 3, lines_removed: 1, in_diff: true }],
      })],
    })
    expect(builtHeading(brief)).toBe('1 part · 1 file · +3 −1')
  })

  it('leaves a nought off a fold’s line rather than promising an empty list', () => {
    expect(foldSummary([
      { count: 2, says: 'findings about no one part' },
      { count: 0, says: 'choices the ticket left open' },
    ])).toBe('2 findings about no one part')
    expect(foldSummary([{ count: 0, says: 'anything' }])).toBeNull()
  })
})

describe('which steps are one page', () => {
  it('is steps 2, 3 and 4, and no others', () => {
    expect(CASE_STEPS).toEqual(['understand', 'alternatives', 'solution_map'])
    expect(isCaseStep('understand')).toBe(true)
    expect(isCaseStep('code_review')).toBe(false)
    expect(isCaseStep('lint')).toBe(false)
  })

  it('lands steps 3 and 4 at their own band, and step 2 at the top', () => {
    /*
     * Step 2 carries no fragment on purpose: it is the page, and a reader arriving at the
     * review should meet the headline and the answer line rather than be scrolled past
     * them.
     */
    expect(BAND_OF_STEP.understand).toBeUndefined()
    expect(BAND_OF_STEP.alternatives).toBe('what-was-chosen')
    expect(BAND_OF_STEP.solution_map).toBe('what-was-built')
  })
})
