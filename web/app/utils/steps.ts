/*
 * What a step page says when the step it is about has written nothing yet.
 *
 * **A page with nothing to draw still has to say which nothing it is**, and before DEV-892
 * two of these pages said the wrong one: Alternatives read "No alternatives are recorded for
 * this change… Nothing has written any" while the trail two inches above it said the blind
 * pass was running, and Solution said the same sentence twice, once as its headline and
 * once as a notice under it. The first is a page contradicting the record it is drawn from;
 * the second is a page saying one thing and charging for two.
 *
 * Nothing here reads or writes a record. Each function takes the layers the review already
 * served and answers a value, which is what makes `test/steps.spec.ts` a test of the rules.
 */

import type { ReviewLayer, ReviewLayerName } from '~/types/desk'

/**
 * The six steps that have somewhere to go, by the suffix their page sits at.
 *
 * A suffix each, now that every one of them is a suffix: step 5 was the base route while
 * the one-page review was where the findings were, and `/reviews/{id}` is a redirect to
 * Understand since DEV-878 retired it. Step 7 joined in DEV-958, which is the issue that
 * gave it a page — before it the only trace of a harvest anywhere was the four facts on
 * that step's own card, which is a tooltip, and what the developer said of the queue's
 * version of the same four was "I don't really know what that means".
 *
 * **Here rather than in `StepTrail`, because two things now answer the same question.**
 * The trail links each step of *this* round; the round switcher links the same step of
 * *every other* round. Two copies of this map would be two chances for one of them to send
 * a reader to a page the other knows is not there.
 */
export const PAGE_OF_STEP: Partial<Record<ReviewLayerName, string>> = {
  understand: 'understand',
  alternatives: 'alternatives',
  solution_map: 'solution',
  code_review: 'code-review',
  draft: 'draft',
  harvest: 'what-happened',
}

/**
 * Where the pull request stops being settled once and the round starts repeating.
 *
 * Four and three, which is the split the trail draws its two groups along. Named here
 * rather than inline so it has one statement, and read by the round switcher too: steps 1
 * to 4 are the ones a later round can be holding none of.
 */
export const SETTLED_THROUGH = 4

/**
 * The four things a step page has to be able to say about an absence.
 *
 * **They are four, and no two of them read alike.** That is DEV-892's acceptance criterion
 * and `test/steps.spec.ts` asserts it, because these are the easiest four sentences in the
 * app to collapse into one grey "nothing here" — and they mean entirely different things to
 * whoever is reading. A step being worked on now will have an answer in minutes. A step that
 * finished and recorded none has given its answer, and the answer is none. A step that has
 * not started has not been asked. And a blind pass that did not run is not an absence in the
 * record at all: it is a thing Review Desk never does and no session reported doing.
 *
 * Phrases rather than sentences, because two of the four are also rubrics. A page that wants
 * a sentence passes one through `sentence()`, which is where the capital and the full stop
 * are decided for everything else on these screens.
 */
export const NOTHING_HERE = {
  /** The step is running: something is writing this now. */
  being_worked_on: 'Being worked on now',
  /** The step has not started, or is waiting on the developer. */
  not_written_yet: 'Not written yet',
  /** The step finished, and what it finished with was none of this. */
  none_recorded: 'None was recorded',
  /** No session reported running a blind pass. Review Desk runs none itself. */
  blind_pass_not_run: 'No blind pass was run',
} as const

/**
 * Which of the three a step's own state is. The fourth is not a state of a step.
 *
 * **Named rather than worded**, so a page can take the phrase for its heading and its own
 * sentence for the line under it from the same answer. Two functions each triaging the
 * state would be two places for the pair to disagree, which is how a headline and a notice
 * came to say the same thing on the Solution page.
 */
export type Absence = 'being_worked_on' | 'not_written_yet' | 'none_recorded'

/** One of the review's steps, by name, or `null` when the record holds no such layer. */
export function layerOf(
  layers: ReviewLayer[],
  name: ReviewLayerName,
): ReviewLayer | null {
  return layers.find(layer => layer.name === name) ?? null
}

/**
 * Which of the three absences a step's own state says this is.
 *
 * **From the step's state, which is the whole fix.** A page that decided this from the
 * emptiness of its own section could only ever say "there is nothing here", and that was
 * being said over a step the same page's trail drew as running.
 *
 * `running` and nothing else is being worked on: `waiting_on_you` is a step that has
 * stopped and is waiting for somebody, which is a different claim and one the trail already
 * makes in its own colour.
 *
 * A step with no layer on the record reads as not written yet: a review whose layer 3 was
 * never written has had nothing written for layer 3, which is what the phrase says.
 */
export function absenceOf(layers: ReviewLayer[], name: ReviewLayerName): Absence {
  const state = layerOf(layers, name)?.state
  if (state === 'running') return 'being_worked_on'
  /*
   * The finished states, the same three `stepDurationLabel` calls finished. A failed step
   * recorded none of this and is not going to; saying it is "not written yet" would promise
   * something nobody is coming to write.
   */
  if (state === 'done' || state === 'failed' || state === 'skipped') return 'none_recorded'
  return 'not_written_yet'
}

/** The phrase for a step's absence, which a page heads itself with. */
export function nothingYet(layers: ReviewLayer[], name: ReviewLayerName): string {
  return NOTHING_HERE[absenceOf(layers, name)]
}
