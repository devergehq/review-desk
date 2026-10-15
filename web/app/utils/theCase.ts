/*
 * **The case for a change, as the one page draws it.**
 *
 * Steps 2, 3 and 4 were a page each — Understand, Alternatives, Solution — and on review
 * #9639 they came to 3,653 + 3,132 + 2,768 pixels and three navigations for one review.
 * Patrick, 16 October 2026, on all three: the content is good, but they are "quite broad,
 * quite large, you've got to dig into them". He picked the second of three options put to
 * him: one page, a band per question, each band the shape of its own answer rather than a
 * summary of it.
 *
 * Nothing here reads or writes a record. Each function takes a brief the review already
 * served and answers a value, which is what makes `test/the-case.spec.ts` a test of the
 * rules rather than of a rendering.
 *
 * ## The answer line
 *
 * The page opens with the brief's own headline sentence and then **one line of facts
 * derived from what is already stored** — "4 problems · option B built · fixes 3 of 4 ·
 * blind pass would pick D". Patrick's wording for it, and his condition: it is derived from
 * existing review data, there is **no new session-written summary and no new field**, and a
 * fact that cannot be derived is left out of the line rather than guessed at.
 *
 * That condition is the whole design of `answerFacts`. Each fact is a separate test of the
 * record, so a brief half way through being written draws the facts it has earned and no
 * others: the problem half alone draws "4 problems" and stops.
 */

import type { Brief, BriefOption, BriefPart, ReviewLayerName } from '~/types/desk'

import { count } from './format'

/** The three steps that are one page, in the order the trail draws them. */
export const CASE_STEPS: ReviewLayerName[] = ['understand', 'alternatives', 'solution_map']

/**
 * The id of the band each step lands at, so the trail's three steps reach three places.
 *
 * **Here rather than in `PAGE_OF_STEP`**, which the round switcher reads as well: a link to
 * the same step of another *round* wants that round's page, and a fragment it appended there
 * would scroll a reader into the middle of a page they have just arrived at.
 */
export const BAND_OF_STEP: Partial<Record<ReviewLayerName, string>> = {
  alternatives: 'what-was-chosen',
  solution_map: 'what-was-built',
}

/** Whether this step is one of the three the case page draws. */
export function isCaseStep(name: ReviewLayerName): boolean {
  return CASE_STEPS.includes(name)
}

/* ------------------------------ Reading the brief ------------------------------- */

/*
 * **Every one of these needs its `?? []`.** The contract serves a brief written the old way
 * with every new array empty and every new string null — but the invented fixtures in sample
 * mode are briefs recorded *before* those fields existed, so at runtime the arrays are
 * `undefined` rather than empty. Typed as present, absent in fact; this is `understand.vue`'s
 * own measurement, moved to where every band reads from it.
 */
export function problemsOf(brief: Brief) {
  return brief.problems ?? []
}

export function optionsOf(brief: Brief): BriefOption[] {
  return brief.options ?? []
}

export function partsOf(brief: Brief): BriefPart[] {
  return brief.parts ?? []
}

/** The option that was built, when the brief names one. */
export function builtOption(brief: Brief): BriefOption | null {
  return optionsOf(brief).find(option => option.chosen) ?? null
}

/**
 * The option the blind pass would have picked, when it named one of the brief's own.
 *
 * A key the option list does not carry answers null: the contract says the key has to be one
 * of the brief's options, and a page that drew a letter it could not find would be asserting
 * an option that is not on the page under it.
 */
export function blindPick(brief: Brief): BriefOption | null {
  const key = brief.blind_pass?.pick_key
  if (!key) return null
  return optionsOf(brief).find(option => option.key === key) ?? null
}

/**
 * The parts that fix one numbered problem, by the number their row carries.
 *
 * **The join band 1 is worth having for.** A numbered problem and the part of the change
 * that fixes it were on two different pages, and the only thing that connected them was the
 * reader's memory of a number. The part's number is its position in `brief.parts` from 1,
 * which is what `partGroups` draws on the row — so the two columns name the same thing.
 *
 * An empty answer is a problem no part claims to fix, and the band says so in words. That is
 * a fact about the change rather than a gap in the record, and it is the one thing on this
 * page that nothing drew before.
 */
export function partsFixing(brief: Brief, problem: number): number[] {
  return partsOf(brief)
    .map((part, index) => ({ part, number: index + 1 }))
    .filter(row => (row.part.fixes ?? []).includes(problem))
    .map(row => row.number)
}

/** How the "Fixed by" cell reads: "part 3", "parts 2, 4", or nothing at all. */
export function fixedByLabel(brief: Brief, problem: number): string | null {
  const numbers = partsFixing(brief, problem)
  if (!numbers.length) return null
  return `${numbers.length === 1 ? 'part' : 'parts'} ${numbers.join(', ')}`
}

/* -------------------------------- The answer line -------------------------------- */

/** Which fact this is, so a test names one rather than matching a sentence. */
export type AnswerFactKind = 'problems' | 'built' | 'fixes' | 'blind'

export interface AnswerFact {
  kind: AnswerFactKind
  /** What the line says, as it is drawn. */
  says: string
}

/**
 * How many of the numbered problems the option that was built says it fixes.
 *
 * **Read off the built option's verdicts, not off the parts.** The two answer different
 * questions and on review #9639 they answer differently: the built option's verdicts are
 * three `fixed` and one `partly`, which is Patrick's own "fixes 3 of 4", while the parts
 * between them claim all four. A part claiming a problem says the change touched it; a
 * verdict says the approach solves it, and the approach is what the line is about.
 *
 * It is also the figure a reader can check without leaving the page: the built option's row
 * in band 2 draws those four verdicts as four dots, directly under this line.
 *
 * `null` when there is nothing to divide — no built option, no numbered problems, or an
 * option that recorded no verdicts — because "fixes 0 of 4" is a claim about the change and
 * an option with no verdicts is an absence of judgement about it.
 */
export function fixesOf(brief: Brief): { fixed: number, of: number } | null {
  const built = builtOption(brief)
  const problems = problemsOf(brief).length
  if (!built || !problems) return null
  const verdicts = built.verdicts ?? []
  if (!verdicts.length) return null
  return { fixed: verdicts.filter(verdict => verdict.verdict === 'fixed').length, of: problems }
}

/**
 * The derived facts the page opens with, in reading order, and only the ones it has.
 *
 * Four separate tests of the record rather than one sentence, so each fact stands or falls
 * on its own. The order is the order the bands are in: how many problems, what was built
 * about them, how far that goes, and what something independent would have done instead.
 */
export function answerFacts(brief: Brief): AnswerFact[] {
  const facts: AnswerFact[] = []

  const problems = problemsOf(brief).length
  if (problems) facts.push({ kind: 'problems', says: count(problems, 'problem') })

  const built = builtOption(brief)
  if (built) facts.push({ kind: 'built', says: `option ${built.key} built` })

  const fixes = fixesOf(brief)
  if (fixes) {
    facts.push({
      kind: 'fixes',
      /*
       * **"fixes all 4", not "fixes 4 of 4".** The second is the same number twice and
       * reads as a ratio a reader has to compare; this is the answer.
       */
      says: fixes.fixed === fixes.of
        ? `fixes all ${fixes.of}`
        : `fixes ${fixes.fixed} of ${fixes.of}`,
    })
  }

  /*
   * What a pass that saw no ticket, no pull request and no code would have picked.
   *
   * **Three wordings, and a fourth state that draws nothing.** A pass that agreed with the
   * build is the cheapest reassurance on the page and a pass that did not is the most
   * useful line on it, so they do not read alike. A pass that ran and recommended none of
   * the options is a result about the option list. A pass that never ran is not an absence
   * in the record — Review Desk runs none and no session reported one — so the line leaves
   * it out rather than reporting on a thing that did not happen.
   */
  const state = brief.blind_pass_state ?? 'not_run'
  if (state !== 'not_run') {
    const pick = blindPick(brief)
    if (pick) {
      facts.push({
        kind: 'blind',
        says: pick.chosen
          ? 'blind pass agrees'
          : `blind pass would pick ${pick.key}`,
      })
    }
    /*
     * **`pick_key` null, not `blindPick` null.** A pass that recommended nothing recorded no
     * key; a key the option list does not carry is a record this page cannot read, and
     * "picked none of them" would be a claim about the pass that the pass contradicts. The
     * second is the case that has to draw nothing, and the two arrive at `blindPick` as the
     * same null.
     */
    else if (brief.blind_pass && brief.blind_pass.pick_key == null) {
      facts.push({ kind: 'blind', says: 'blind pass picked none of them' })
    }
  }

  return facts
}

/* ------------------------------- What the bands head --------------------------------- */

/**
 * The size of the change, as band 3's heading carries it.
 *
 * **It counts every file the parts name, including the ones outside the diff**, and says how
 * many those are. That is `solution.vue`'s own measurement and its reasoning is unchanged:
 * counting only the changed files makes the heading disagree with the column of rows under
 * it, which each count claimed files, and a reader adding the column up is then out by two
 * with nothing to tell them why.
 */
export function builtHeading(brief: Brief): string {
  const parts = partsOf(brief)
  const files = parts.flatMap(part => part.files ?? [])
  const outside = files.filter(file => !file.in_diff).length
  const added = parts.reduce((total, part) => total + part.lines_added, 0)
  const removed = parts.reduce((total, part) => total + part.lines_removed, 0)
  return `${count(parts.length, 'part')} · ${count(files.length, 'file')}`
    + (outside ? ` (${outside} not changed)` : '')
    + ` · +${added} −${removed}`
}

/**
 * What one fold's summary names, as a list of counts: "4 choices the ticket left open · …".
 *
 * **Counted rather than asserted, so the summary cannot promise a section that is empty.**
 * That is the Understand page's own rule for the two figures it drew about the Solution
 * page's lists, and it matters more here: the three lists are now inside the control whose
 * line names them.
 */
export function foldSummary(figures: Array<{ count: number, says: string }>): string | null {
  const said = figures.filter(figure => figure.count > 0)
    .map(figure => `${figure.count} ${figure.says}`)
  return said.length ? said.join(' · ') : null
}
