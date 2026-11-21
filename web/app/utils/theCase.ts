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

import type {
  AcceptanceCriterion,
  Brief,
  BriefOption,
  BriefPart,
  ReviewLayer,
  ReviewLayerName,
} from '~/types/desk'

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

/**
 * **The bands of the Understanding tab, in the order it draws them**, with the id each one
 * is reached at and the words its *On this tab* rail and its heading use. DEV-1327, to S3.
 *
 * **The ids are a contract with two callers and outlive the words.** `RETIRED_ROUTES` sends
 * the old Alternatives and Solution addresses to `what-was-chosen` and `what-was-built`, and
 * Overview's questions link to `what-was-chosen` and `not-verified`. So S3 renaming *What was
 * wrong* to *What changes* and *What was chosen* to *Approach* renames the heading and keeps
 * the id: a link anybody holds still lands on its band.
 *
 * **S3 draws a sixth, *Taken as true*, and this does not.** The record holds no assumptions
 * — no field says what a review relied on without proving it, nor its basis — and a fact the
 * record cannot derive is left out rather than guessed. *What was built* is not on S3 and is
 * kept, because a page may fold or reorder what it draws and never removes any of it.
 */
export const CASE_BANDS = [
  { id: 'what-was-wrong', label: 'What changes' },
  { id: 'ticket-against-what-was-built', label: 'Ticket against what was built' },
  { id: 'what-was-chosen', label: 'Approach' },
  { id: 'what-was-built', label: 'What was built' },
  { id: 'not-verified', label: 'Not verified' },
] as const

export type CaseBandId = typeof CASE_BANDS[number]['id']

/** The words a band is headed by, from the one list the rail reads too. */
export function bandLabel(id: CaseBandId): string {
  return CASE_BANDS.find(band => band.id === id)!.label
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
  /**
   * **The same fact split for a cell**: the figure, then what it counts. Overview's bottom
   * line draws each fact as a figure over a label (S2), and the case page's line draws
   * `says`. One fact, two layouts, so the two cannot come to disagree. Added by DEV-1326.
   */
  figure: string
  label: string
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
  if (problems) {
    facts.push({
      kind: 'problems',
      says: count(problems, 'problem'),
      figure: String(problems),
      label: problems === 1 ? 'problem' : 'problems',
    })
  }

  const built = builtOption(brief)
  if (built) {
    const options = optionsOf(brief).length
    facts.push({
      kind: 'built',
      says: `option ${built.key} built`,
      figure: `Option ${built.key}`,
      label: `built, of ${count(options, 'option')} weighed`,
    })
  }

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
      figure: fixes.fixed === fixes.of ? `All ${fixes.of}` : `${fixes.fixed} of ${fixes.of}`,
      label: fixes.of === 1 ? 'problem it fixes' : 'problems it fixes',
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
        figure: pick.chosen ? 'Agrees' : `Would pick ${pick.key}`,
        label: 'blind pass',
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
      facts.push({ kind: 'blind', says: 'blind pass picked none of them', figure: 'None', label: 'blind pass picked' })
    }
  }

  return facts
}

/** A review's bottom line: the brief's own headline sentence, then the derived facts. */
export interface BottomLine {
  /** The brief's headline, as the session wrote it, or null while it is unwritten. */
  headline: string | null
  /** `answerFacts`, in reading order: only the facts the brief has earned. */
  facts: AnswerFact[]
}

/**
 * **The bottom line, chosen once.** DEV-1325's preview pane draws it beside the list and
 * DEV-1326's Overview draws it at the top of a review; both call this, so a review cannot
 * have one bottom line in the list and another on its own page.
 *
 * It is the case page's answer line by another name, and its condition is unchanged: the
 * headline is the brief's own sentence and every fact is `answerFacts`'s, so nothing here
 * is written for the occasion and nothing is guessed. `null` when the brief has neither —
 * no brief at all, or one with no headline and no fact yet — so a caller says *no brief
 * yet* in its own words rather than drawing an empty heading.
 */
export function bottomLine(brief: Brief | null | undefined): BottomLine | null {
  if (!brief) return null
  const headline = brief.headline?.trim() ? brief.headline : null
  const facts = answerFacts(brief)
  if (headline == null && facts.length === 0) return null
  return { headline, facts }
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

/**
 * What the *Approach* heading carries beside it: "option C of 8 · blind pass agrees".
 *
 * **The blind pass's words are `answerFacts`'s own**, so the answer line and this line
 * cannot say two things about one pass. The built option is said as *of how many*, which is
 * what a heading over the matrix of those options is for. `null` when there are no options.
 */
export function approachMeta(brief: Brief): string | null {
  const options = optionsOf(brief).length
  if (!options) return null
  const built = builtOption(brief)
  const said = [built ? `option ${built.key} of ${options}` : count(options, 'option')]
  const blind = answerFacts(brief).find(fact => fact.kind === 'blind')
  if (blind) said.push(blind.says)
  return said.join(' · ')
}

/* ----------------------------- Ticket against what was built ------------------------------ */

/** How one criterion reads in the *Observed* column, and the mark beside it. */
export interface CriterionObserved {
  mark: AcceptanceCriterion['status']
  says: string
  /** The finding that says it is not met, for a link to it. */
  finding: string | null
}

/**
 * **What the review observed about one thing the ticket asked for**, in S3's words: *yes*,
 * *no, see F9*, or *not in this PR*. Three values because `CriterionStatus` has three and
 * refuses a fourth.
 *
 * S3 draws a *Claimed* column beside it and a *How* column after. **Neither is in the
 * record**: a criterion carries what the review observed and a pointer, not what the pull
 * request claimed and not whether the review ran or read it. Both are left out rather than
 * guessed — a tick under *Claimed* the record cannot back is the one thing this page is
 * built not to draw.
 */
export function criterionObserved(criterion: AcceptanceCriterion): CriterionObserved {
  switch (criterion.status) {
    case 'met':
      return { mark: 'met', says: 'yes', finding: null }
    case 'not_met': {
      /* The contract's own key: the dashboard never computes an identifier. */
      const key = criterion.finding_key ?? null
      return { mark: 'not_met', says: key ? `no, see ${key}` : 'no', finding: key }
    }
    default:
      return { mark: 'not_in_this_pr', says: 'not in this PR', finding: null }
  }
}

/**
 * **What answers for one criterion**, the contract's one pointer: a file and its line, or
 * the session's note. A finding is said in *Observed* already, so it is not said twice.
 */
export function criterionWhere(criterion: AcceptanceCriterion): string | null {
  if (criterion.file) return criterion.line != null ? `${criterion.file}:${criterion.line}` : criterion.file
  return criterion.note?.trim() || null
}

/* ------------------------------------ Not verified ----------------------------------------- */

/** What the review did not look at, as Overview's question and the band both say it. */
export interface NotVerified {
  says: string
  /**
   * `written` for the session's words only; `none_recorded` for a code review that is over
   * and wrote none; `writing` while it runs; `not_started` before it has. Overview's
   * `QuestionMark`, value for value — `none_recorded` is not counted as written there.
   */
  mark: 'written' | 'writing' | 'not_started' | 'none_recorded'
}

/**
 * **What the review did not look at, chosen once**: Overview's *What wasn't checked?* and the
 * Understanding tab's *Not verified* band, which that question links to, both call this — so
 * the answer and the band it lands on cannot word one state two ways.
 *
 * The session's `not_checked` when it wrote one. Otherwise by the code review's own state:
 * finished, it recorded nothing left unchecked; running, it is being written; failed or
 * skipped, it never got as far as saying; not yet run, it has not started.
 *
 * **Only the session's words are `written`.** A finished or skipped code review that wrote
 * none is `none_recorded`, as Overview marks a step that is over and holds no answer; a
 * failed one is `not_started`, as Overview's other questions read it. Neither is counted in
 * *N of 5 written*. Skipped and failed keep sentences of their own, because "no answer" over
 * a step that never looked says less than why.
 */
export function notVerified(
  notChecked: string | null | undefined,
  layers: readonly Pick<ReviewLayer, 'name' | 'state'>[],
): NotVerified {
  const said = notChecked?.trim()
  if (said) return { says: said, mark: 'written' }
  switch (layers.find(layer => layer.name === 'code_review')?.state) {
    case 'done':
    case 'waiting_on_you':
      return { says: 'The review records nothing it left unchecked.', mark: 'none_recorded' }
    case 'running':
      return { says: 'Writing now.', mark: 'writing' }
    case 'failed':
      return { says: 'The code review failed before it listed what it did not look at.', mark: 'not_started' }
    case 'skipped':
      return { says: 'The code review was skipped, so nothing lists what it did not look at.', mark: 'none_recorded' }
    default:
      return { says: 'Not started. Listed once the checks finish.', mark: 'not_started' }
  }
}
