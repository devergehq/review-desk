/*
 * **The Overview tab, as rules.** DEV-1326, under DEV-1322; the target is S2 (ready), S12
 * (being written) and S15 (replies in) in `docs/mockups/s-structured`.
 *
 * Overview opens on the bottom line and where the review stands, then the five questions a
 * developer would be asked about the change, then up to three findings. When replies have
 * come in, the bottom line becomes *Since you posted* and *What came back* takes the
 * questions' place.
 *
 * **Every word here is read off `GET /reviews/{id}`**, and a fact the record cannot derive
 * is left out rather than drawn as a guess — `docs/design.md`, *the case*. Nothing here
 * reads or writes a record: each function takes the review and answers a value, which is
 * what makes `test/overview.spec.ts` a test of the rules rather than of a template.
 */

import type { Finding, ReviewDetail, ReviewLayerName, Severity } from '~/types/desk'

import { mostSeriousFirst, willPost } from './findings'
import { count, severityTally } from './format'
import { BASIS_WORDS, outcomeSaid, sinceYouPosted } from './outcomes'
import type { GroupedFinding, ReplyBasis, SinceYouPosted } from './outcomes'
import { tabPath } from './reviewTabs'
import { stateOfReview } from './stateWord'
import type { ListedAs, ReviewState } from './stateWord'
import { bottomLine, builtOption, notVerified, optionsOf } from './theCase'

/** The parts of a review Overview reads. */
export type OverviewSource = Pick<
  ReviewDetail,
  'review' | 'layers' | 'outcomes' | 'draft_status' | 'brief' | 'findings' | 'finding_severities' | 'not_checked'
  | 'not_for_signing'
>

/* ------------------------------------ The state ------------------------------------- */

/**
 * Which of the three Overviews a review draws.
 *
 * | Overview | When | Picture |
 * |---|---|---|
 * | `replies_in` | the review is *Replies in* | S15 |
 * | `being_written` | a session is still writing it, or nothing has run yet | S12 |
 * | `ready` | everything else: brief or draft ready, posted, or off the desk | S2 |
 *
 * **Read off [`stateOfReview`]**, the one place a review's state is chosen, and with the
 * same `listed` row the header passes it, so Overview cannot draw *Since you posted* under a
 * header saying *Draft ready*.
 */
export type OverviewState = 'ready' | 'being_written' | 'replies_in'

export function overviewState(
  detail: Parameters<typeof stateOfReview>[0],
  listed?: ListedAs | null,
): OverviewState {
  const state = stateOfReview(detail, listed)
  if (state === 'replies_in') return 'replies_in'
  if (UNWRITTEN.has(state)) return 'being_written'
  return 'ready'
}

/**
 * The states whose review is not written yet: a session writing it, or none picked up.
 *
 * **`waiting_for_session` draws the being-written Overview too**, with nothing marked as
 * writing: what is on the record is partial in the same way, and only *Writing now* — which
 * reads the steps' own `running` — tells the two apart.
 */
const UNWRITTEN: ReadonlySet<ReviewState> = new Set<ReviewState>(['being_written', 'not_started', 'waiting_for_session'])

/* ------------------------------ One rule for a step ------------------------------- */

type LayerState = OverviewSource['layers'][number]['state']

/**
 * **A step that ran to its end**: done, or waiting on the developer. Never `skipped`.
 *
 * The contract: "a skipped step cannot read as a finished one". So grading, and any claim
 * that a step *wrote* something, read this.
 */
function ran(state: LayerState | undefined): boolean {
  return state === 'done' || state === 'waiting_on_you'
}

/**
 * **A step that is over**: it ran, or it was deliberately skipped. This is about progress
 * only — where the review has got to, and whether anything is still coming — and never
 * that the step did its work.
 */
function over(state: LayerState | undefined): boolean {
  return ran(state) || state === 'skipped'
}

/* -------------------------------- Where it stands ---------------------------------- */

/** How one stage of *Where it stands* is marked. */
export type StageMark = 'done' | 'current' | 'writing' | 'todo'

export interface Stage {
  label: string
  mark: StageMark
}

const STAGE_LABELS = ['Brief ready', 'Code review', 'Draft ready', 'Posted', 'Replies in'] as const

/** The steps that write each of the first three stages. Posted and Replies in have none. */
const STAGE_LAYERS: ReadonlyArray<readonly ReviewLayerName[]> = [
  ['understand', 'alternatives', 'solution_map'],
  ['code_review'],
  ['draft'],
]

/** The stage a review's state puts it at, when the state names one. */
const STAGE_OF_STATE: Partial<Record<ReviewState, number>> = {
  brief_ready: 0,
  draft_ready: 2,
  posted: 3,
  replies_in: 4,
}

/**
 * **Brief ready → Code review → Draft ready → Posted → Replies in**, with the current one
 * marked. It replaces the seven-step trail as the answer to *where is this review*; the
 * trail itself is folded under it, for what else it says about each step.
 *
 * Each stage is reached when the record says so — the brief's steps finished, the code
 * review finished, the draft written, the draft posted, something come back — and the
 * current stage is the one [`stateOfReview`] names. A review being written has no such
 * stage: its current one is the first stage not reached, marked *writing* when one of the
 * steps behind it is running and left plain when none is.
 */
export function whereItStands(detail: OverviewSource, listed?: ListedAs | null): Stage[] {
  const state = stateOfReview(detail, listed)
  const stateOf = (name: ReviewLayerName) => detail.layers.find(layer => layer.name === name)?.state
  const finished = (name: ReviewLayerName) => over(stateOf(name))

  const reached = [
    STAGE_LAYERS[0]!.every(finished),
    finished('code_review'),
    finished('draft'),
    detail.draft_status === 'posted',
    state === 'replies_in',
  ]
  const furthest = reached.lastIndexOf(true)
  const current = STAGE_OF_STATE[state] ?? (UNWRITTEN.has(state) ? furthest + 1 : furthest)

  return STAGE_LABELS.map((label, index) => {
    if (index < current) return { label, mark: 'done' }
    if (index > current) return { label, mark: 'todo' }
    if (!UNWRITTEN.has(state)) return { label, mark: 'current' }
    const running = (STAGE_LAYERS[index] ?? []).some(name => stateOf(name) === 'running')
    /* "Brief · writing", as S12 draws it: the stage's noun, and what is happening to it. */
    return running
      ? { label: `${label.replace(/ ready$/, '')} · writing`, mark: 'writing' }
      : { label, mark: 'current' }
  })
}

/* -------------------------------- The bottom line ---------------------------------- */

/** One figure in the bottom line's row. */
export interface BottomCell {
  figure: string
  label: string
  /** The line under it, when the record holds one. */
  sub: string | null
}

export interface OverviewBottomLine {
  /** *The bottom line*, or *Since you posted*. */
  heading: string
  sentence: string
  cells: BottomCell[]
}

/** The findings this review would post, or has posted. */
function postedFindings(detail: OverviewSource): Finding[] {
  return detail.findings.filter(willPost)
}

/**
 * **Whether the findings are graded: the code review step has finished.**
 *
 * Read off the step, never off the Overview's state. A review between steps — code review
 * done, no draft session yet — draws the being-written Overview, and its findings are
 * graded all the same; saying *not graded yet* there is a claim the record contradicts.
 */
export function findingsGraded(detail: Pick<OverviewSource, 'layers'>): boolean {
  return ran(detail.layers.find(layer => layer.name === 'code_review')?.state)
}

/**
 * The bottom line of a review that has not had replies: the brief's headline and its
 * derived facts ([`bottomLine`], DEV-1325's), and the findings as one figure.
 *
 * **While it is being written, the sentence says so** — S12's own words — and the findings
 * are counted *so far* and not graded, because grading is what the code review step does.
 */
export function readyBottomLine(detail: OverviewSource, state: OverviewState): OverviewBottomLine {
  const line = bottomLine(detail.brief)
  const writing = state === 'being_written'
  const graded = findingsGraded(detail)

  const cells: BottomCell[] = (line?.facts ?? []).map(fact => ({ figure: fact.figure, label: fact.label, sub: null }))

  if (detail.findings.length > 0) {
    const tally = severityTally(detail.finding_severities)
    cells.push({
      figure: String(detail.findings.length),
      label: writing && !graded
        ? (detail.findings.length === 1 ? 'finding so far' : 'findings so far')
        : (detail.findings.length === 1 ? 'finding' : 'findings'),
      sub: graded ? (tally || null) : 'not graded yet',
    })
  }

  const sentence = line?.headline
    ?? (writing
      ? 'Not written yet. The bottom line comes last, once every finding is graded.'
      : 'This review records no bottom line.')

  return { heading: 'The bottom line', sentence, cells }
}

/** "F3", "F3 and F5", "F3, F5 and F9". */
function keys(group: readonly GroupedFinding[]): string {
  const names = group.map(one => one.key)
  if (names.length <= 1) return names.join('')
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/** The blockers among a group, by key. */
function blockersIn(detail: OverviewSource, group: readonly GroupedFinding[]): GroupedFinding[] {
  const blocker = new Set(detail.findings.filter(one => one.severity === 'blocker').map(one => one.seq))
  return group.filter(one => blocker.has(one.seq))
}

/**
 * *Since you posted*: what came back, as a sentence and five figures (S15).
 *
 * **The sentence is composed from the groups and says no more than they do.** S15's own
 * sentence names the author and says "the code says she's wrong"; the record holds neither
 * a judgement on a reply nor anyone's pronouns, so this one says which findings are in
 * which group and stops.
 */
export function sinceBottomLine(detail: OverviewSource, groups: SinceYouPosted): OverviewBottomLine {
  const clauses: string[] = []
  if (groups.fixed.length) clauses.push(`${keys(groups.fixed)} fixed`)
  if (groups.answered.length) clauses.push(`${keys(groups.answered)} answered`)
  if (groups.contested.length) clauses.push(`${keys(groups.contested)} contested`)

  const blockers = blockersIn(detail, groups.noReply)
  const silent = groups.noReply.length
    ? `${count(groups.noReply.length, 'finding')} ${groups.noReply.length === 1 ? 'has' : 'have'} no reply yet`
    + (blockers.length ? `, including ${blockers.length === 1 ? 'blocker' : 'blockers'} ${keys(blockers)}` : '')
    : null

  const opening = clauses.length
    ? `${clauses.join(', ').replace(/^./, first => first.toUpperCase())}.`
    : null
  const sentence = [opening, silent ? `${silent.replace(/^./, first => first.toUpperCase())}.` : null]
    .filter(Boolean)
    .join(' ') || 'Nothing has come back on a posted finding.'

  const basisOf = (basis: ReplyBasis, rest?: string) =>
    rest ? `${BASIS_WORDS[basis]} · ${rest}` : BASIS_WORDS[basis]

  const cells: BottomCell[] = [
    { figure: String(groups.fixed.length), label: 'fixed', sub: basisOf('told', 'not re-run') },
    { figure: String(groups.answered.length), label: 'answered', sub: basisOf('told', 'not re-checked') },
    {
      figure: String(groups.contested.length),
      label: 'contested',
      sub: groups.contested.length ? basisOf('told', keys(groups.contested)) : BASIS_WORDS.told,
    },
    {
      figure: String(groups.noReply.length),
      label: 'no reply yet',
      sub: blockers.length
        ? basisOf('not_rechecked', `including ${blockers.length === 1 ? 'blocker' : 'blockers'} ${keys(blockers)}`)
        : BASIS_WORDS.not_rechecked,
    },
  ]
  /*
   * **Commits only when a comparison named one.** "0 new commits" would be a claim that
   * nothing was pushed, and the record holds only the commits a comparison read.
   */
  if (groups.commits.length) {
    cells.push({
      figure: String(groups.commits.length),
      label: groups.commits.length === 1 ? 'new commit seen' : 'new commits seen',
      /*
       * **No baseline named.** The harvester compares against each thread's own posted
       * commit, which this response does not carry, so "since {sha}" would name the wrong
       * one; and a count of commits a comparison named is not a count of pushes.
       */
      sub: basisOf('read', groups.newCode.length ? `under ${keys(groups.newCode)}` : 'no posted finding\'s lines moved'),
    })
  }

  return { heading: 'Since you posted', sentence, cells }
}

/* ------------------------------ What you'd be asked -------------------------------- */

/** Where a question's answer has got to. */
/**
 * Where a question's answer has got to. `none_recorded` is a step that is over and wrote no
 * such field: true of the record, and **not** counted as written.
 */
export type QuestionMark = 'written' | 'writing' | 'not_started' | 'none_recorded'

export interface Question {
  asks: string
  answer: string
  mark: QuestionMark
  /** The tab that holds the answer in full, or null when no tab does yet. */
  link: { label: string, to: string } | null
}

/**
 * **The five questions a developer would be asked about the change**, each with its one-line
 * answer from the record and the tab that holds it in full.
 *
 * | Question | Answer | Written by |
 * |---|---|---|
 * | What does it do? | the brief's after-flow caption, else the built option's title | the brief |
 * | Is it the right way? | the brief's verdict on the approach | the options |
 * | What's wrong with it? | the most serious finding the draft carries | the code review |
 * | What am I saying? | the draft's findings, by severity | the draft |
 * | What wasn't checked? | the review's own `not_checked` | the code review |
 *
 * A question whose answer is not written yet says *Writing now* while the step behind it is
 * running, and *Not started* otherwise.
 */
export function fiveQuestions(detail: OverviewSource): Question[] {
  const id = detail.review.id
  const brief = detail.brief ?? null
  const stateOf = (name: ReviewLayerName) => detail.layers.find(layer => layer.name === name)?.state
  const finished = (name: ReviewLayerName) => ran(stateOf(name))
  /*
   * **Three answers for an answer the record lacks**: the step behind it is running, it has
   * finished and wrote no such field, or it has not started. "Not started" over a step that
   * finished would be a claim about the step the record contradicts.
   */
  const pending = (steps: ReviewLayerName[], notStarted: string): { answer: string, mark: QuestionMark } => {
    if (steps.some(name => stateOf(name) === 'running')) return { answer: 'Writing now.', mark: 'writing' }
    if (steps.every(name => over(stateOf(name)))) {
      return { answer: 'The record holds no answer to this.', mark: 'none_recorded' }
    }
    return { answer: notStarted, mark: 'not_started' }
  }

  const understanding = tabPath(id, 'understanding')

  const does = brief?.flow_after_caption?.trim() || builtOption(brief ?? ({} as never))?.title || null
  const right = brief?.approach_verdict?.trim() || null

  const posted = mostSeriousFirst(postedFindings(detail))
  const graded = findingsGraded(detail)
  const worst = posted[0] ?? null

  const draftWritten = finished('draft') || detail.draft_status === 'posted'
  const draftSays = (() => {
    if (!posted.length) return 'The draft carries no findings.'
    const tally = severityTally(postedTally(posted))
    return `${count(posted.length, 'finding')} in the draft: ${tally}.`
  })()

  return [
    {
      asks: 'What does it do?',
      ...(does ? { answer: does, mark: 'written' as const } : pending(['understand', 'alternatives', 'solution_map'], 'Not started.')),
      link: { label: 'Understanding', to: understanding },
    },
    {
      asks: 'Is it the right way?',
      ...(right
        ? { answer: right, mark: 'written' as const }
        : pending(['alternatives'], `Not started. ${optionsOf(brief ?? ({} as never)).length ? '' : 'No options weighed yet.'}`.trim())),
      link: { label: 'Understanding › Approach', to: `${understanding}#what-was-chosen` },
    },
    {
      asks: 'What\'s wrong with it?',
      ...(graded
        ? { answer: worst ? worst.claim : 'Nothing the draft would post.', mark: 'written' as const }
        : pending(['code_review'], 'Not started. Findings are graded after the options.')),
      link: {
        label: detail.findings.length ? `Findings · ${detail.findings.length}` : 'Findings',
        to: tabPath(id, 'findings'),
      },
    },
    {
      asks: 'What am I saying?',
      ...(draftWritten ? { answer: draftSays, mark: 'written' as const } : pending(['draft'], 'Not started. Needs the graded findings.')),
      link: { label: 'Draft', to: tabPath(id, 'draft') },
    },
    {
      asks: 'What wasn\'t checked?',
      /* One reading with the band it links to: `notVerified` in `utils/theCase.ts`. */
      ...(({ says, mark }) => ({ answer: says, mark }))(notVerified(detail.not_checked, detail.layers)),
      /*
       * **Understanding › Not verified, as S2 points it.** DEV-1327 draws `not_checked` there
       * under `#not-verified`, on a brief of any age and with no brief at all, and says what
       * an empty field means — so the link always lands on the answer it names.
       */
      link: { label: 'Understanding › Not verified', to: `${understanding}#not-verified` },
    },
  ]
}

/** Severity counts over a list of findings, in the contract's shape. */
function postedTally(findings: readonly Finding[]) {
  const tally = { blocker: 0, should: 0, nit: 0, question: 0 }
  for (const finding of findings) tally[finding.severity] += 1
  return tally
}

/* --------------------------------- Needs your eye ---------------------------------- */

export interface EyeCard {
  key: string
  claim: string
  /** `null` until the code review step has finished: a finding is not graded before then. */
  severity: Severity | null
  /** Where the finding points, when it points somewhere. */
  where: string | null
  to: string
}

/**
 * **Up to three findings**, most serious first, from the ones the draft carries.
 *
 * Until the code review step has finished, **no severity**: the step grades findings as it
 * finishes, and a severity drawn before then is a guess the step may overturn (S12's
 * *Early · not graded yet*). Read off the step, not the Overview's state — see
 * [`findingsGraded`]. Every card links to the Findings tab; DEV-1328 adds
 * `?finding=` and repoints these at the finding itself.
 */
export function needsYourEye(detail: OverviewSource): EyeCard[] {
  const graded = findingsGraded(detail)
  const pool = graded ? postedFindings(detail) : detail.findings
  return mostSeriousFirst(pool).slice(0, 3).map(finding => ({
    key: finding.key,
    claim: finding.claim,
    severity: graded ? finding.severity : null,
    where: finding.file ? (finding.line == null ? finding.file : `${finding.file}:${finding.line}`) : null,
    to: tabPath(detail.review.id, 'findings'),
  }))
}

/* --------------------------------- What came back ---------------------------------- */

export interface CameBackRow {
  /** "F3 · Contested", "New code under F6", "9 with no reply". */
  title: string
  says: string
  basis: ReplyBasis
  tone: 'contested' | 'fixed' | 'answered' | 'moved' | 'silent'
  link: { label: string, to: string }
}

/** The author's words on an outcome, or what the record says about it. */
function saidOf(grouped: GroupedFinding): string {
  if (!grouped.outcome) return ''
  const said = outcomeSaid(grouped.outcome)
  /*
   * **A login as GitHub spells it, or "The author"** — the one phrase `outcomeSaid` composes
   * itself, capitalised here because it opens the row. A login is never capitalised: it is
   * a value, and `dana-okoye` is not `Dana-okoye`.
   */
  const by = said.by === 'the author' ? 'The author' : said.by
  if (said.quote) return by ? `${by}: ${said.quote}` : said.quote
  return `${said.sentence.replace(/^./, first => first.toUpperCase())}.`
}

/**
 * *What came back* (S15), in the order a developer acts on it: contested first, then fixed,
 * answered, the code that moved, and the findings nothing came back on, as one row.
 *
 * **Each row states its basis**, from the group it came from — nothing here says a check
 * was re-run, because the record holds no re-run.
 */
export function whatCameBack(detail: OverviewSource, groups: SinceYouPosted): CameBackRow[] {
  const id = detail.review.id
  const replies = tabPath(id, 'replies')
  const rows: CameBackRow[] = []

  for (const one of groups.contested) {
    rows.push({ title: `${one.key} · Contested`, says: saidOf(one), basis: one.basis, tone: 'contested', link: { label: `Replies › ${one.key}`, to: replies } })
  }
  for (const one of groups.fixed) {
    rows.push({
      title: `${one.key} · Fixed`,
      says: `${saidOf(one)} Not re-run.`.trim(),
      basis: one.basis,
      tone: 'fixed',
      link: { label: `Replies › ${one.key}`, to: replies },
    })
  }
  for (const one of groups.answered) {
    rows.push({
      title: `${one.key} · Answered`,
      says: `${saidOf(one)} Not re-checked yet.`.trim(),
      basis: one.basis,
      tone: 'answered',
      link: { label: `Replies › ${one.key}`, to: replies },
    })
  }
  /*
   * **Only where the comparison says the finding's lines moved.** `groups.newCode` holds
   * exactly those; a file that changed around an unmoved line has no row.
   */
  for (const one of groups.newCode) {
    rows.push({
      title: `New code under ${one.key}`,
      says: `${saidOf(one)} Re-anchor it before round 2.`,
      basis: one.basis,
      tone: 'moved',
      link: { label: 'Findings', to: tabPath(id, 'findings') },
    })
  }
  if (groups.noReply.length) {
    const blockers = blockersIn(detail, groups.noReply)
    rows.push({
      title: `${groups.noReply.length} with no reply`,
      says: (blockers.length ? `Including ${blockers.length === 1 ? 'blocker' : 'blockers'} ${keys(blockers)}. ` : '')
        + 'Ask again in round 2, or wait.',
      basis: 'not_rechecked',
      tone: 'silent',
      link: { label: 'Findings', to: tabPath(id, 'findings') },
    })
  }
  return rows
}

/** The groups for a review, from the findings its draft carried. */
export function groupsOf(detail: OverviewSource): SinceYouPosted {
  return sinceYouPosted(postedFindings(detail), detail.outcomes)
}
