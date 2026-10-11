import { describe, expect, it } from 'vitest'

import { NOTHING_HERE, absenceOf, layerOf, nothingYet } from '../app/utils/steps'

import type { ReviewLayer, ReviewLayerName, ReviewLayerState } from '../app/types/desk'

/*
 * **What a step page says when the step it is about has written nothing.**
 *
 * DEV-892's defect 6, as rules rather than as renders: Alternatives said "No alternatives
 * are recorded… Nothing has written any" while the trail two inches above it drew step 3 as
 * running, and Solution said one sentence twice. Both came from a page deciding the question
 * from the emptiness of its own section instead of from the step's own state.
 */

function layer(
  name: ReviewLayerName,
  state: ReviewLayerState,
  number = 3,
): ReviewLayer {
  return {
    number,
    name,
    state,
    detail: null,
    updated_at: '2026-10-09T22:00:00Z',
    runs: [],
    seconds: null,
  }
}

describe('the four wordings of an absence', () => {
  /**
   * **The acceptance criterion, as a test.** "'Being worked on now', 'not written yet',
   * 'none recorded' and 'the blind pass did not run' are four wordings, and a test asserts
   * no two read alike."
   *
   * They are the easiest four sentences in the app to collapse into one grey "nothing here",
   * and they mean entirely different things: one will have an answer in minutes, one has
   * given its answer and the answer is none, one has not been asked, and one is a thing
   * Review Desk never does and no session reported doing.
   */
  it('reads as four different things', () => {
    const said = Object.values(NOTHING_HERE)
    expect(said).toHaveLength(4)
    expect(new Set(said).size, 'no two of the four are the same string').toBe(4)
    for (const words of said) {
      expect(words.trim(), 'each of the four says something').not.toBe('')
    }
  })

  /*
   * And no two of them are the same *reading*, which is a weaker claim than string equality
   * but the one the criterion is about: "none recorded" and "not written yet" differing by a
   * word would pass the test above and fail a reader.
   */
  it('shares no leading words between any two of them', () => {
    const said = Object.values(NOTHING_HERE).map(words => words.toLowerCase().split(' ')[0])
    expect(new Set(said).size, `the four open with ${said.join(', ')}`).toBe(4)
  })
})

describe('which absence a step’s state says it is', () => {
  it.each([
    ['running', NOTHING_HERE.being_worked_on],
    ['pending', NOTHING_HERE.not_written_yet],
    ['waiting_on_you', NOTHING_HERE.not_written_yet],
    ['done', NOTHING_HERE.none_recorded],
    ['skipped', NOTHING_HERE.none_recorded],
    /*
     * A failed step recorded none of this and nobody is coming to write it, so "not written
     * yet" would promise something that is not on its way.
     */
    ['failed', NOTHING_HERE.none_recorded],
  ] as const)('a step that is %s reads as “%s”', (state, said) => {
    expect(nothingYet([layer('alternatives', state)], 'alternatives')).toBe(said)
  })

  it('reads a step the record does not hold as not written yet', () => {
    /* A review whose layer 3 was never written has had nothing written for layer 3. */
    expect(nothingYet([], 'alternatives')).toBe(NOTHING_HERE.not_written_yet)
    expect(nothingYet([layer('solution_map', 'running', 4)], 'alternatives'))
      .toBe(NOTHING_HERE.not_written_yet)
  })

  it('reads the step it was asked about, not the first one it finds', () => {
    const layers = [
      layer('alternatives', 'done'),
      layer('solution_map', 'running', 4),
    ]
    expect(nothingYet(layers, 'alternatives')).toBe(NOTHING_HERE.none_recorded)
    expect(nothingYet(layers, 'solution_map')).toBe(NOTHING_HERE.being_worked_on)
    expect(layerOf(layers, 'solution_map')?.number).toBe(4)
    expect(layerOf(layers, 'draft')).toBe(null)
  })
})

describe('being worked on now is running and nothing else', () => {
  it('does not count a step that has stopped and is waiting', () => {
    expect(absenceOf([layer('alternatives', 'running')], 'alternatives'))
      .toBe('being_worked_on')
    /*
     * `waiting_on_you` is a step that has stopped and is waiting for somebody, which is a
     * different claim — and one the trail already makes, in its own colour.
     */
    expect(absenceOf([layer('alternatives', 'waiting_on_you')], 'alternatives'))
      .toBe('not_written_yet')
  })

  /*
   * **Every name `absenceOf` can answer has a phrase**, which is what lets a page take its
   * heading from one and its own sentence from the other off the same answer.
   */
  it('words every absence it can name', () => {
    for (const state of ['running', 'pending', 'waiting_on_you', 'done', 'failed', 'skipped'] as const) {
      const named = absenceOf([layer('alternatives', state)], 'alternatives')
      expect(NOTHING_HERE[named], `${state} is worded`).toBeTruthy()
    }
  })
})
