/*
 * **The Replies tab: what came back, in the three groups S09 draws.** DEV-1330, under DEV-1322.
 * Auto-imported.
 *
 * ```
 *  NEEDS YOUR ANSWER 1
 *  [should] F3  A deleted tenant's schedule still runs                     Contested
 *           dana-okoye · 2 hours ago
 *           “Tenants are never hard-deleted in production…”
 *           [R Raise it again] [A Accept their reason] [W Withdraw]
 *  SETTLED 2
 *  [blocker] F9  The column policy is skipped when no columns are set          Fixed
 *  NO REPLY YET 9
 *  [should] F1 …   (the first four)
 *  › 5 more with no reply
 * ```
 *
 * Nothing here reads or writes a record: it takes the `ReviewDetail` the page already has.
 *
 * ## One grouping, not a second
 *
 * **The groups are Overview's.** `groupsOf` (DEV-1326, `overview.ts`) runs `sinceYouPosted`
 * over the findings the draft carried, and this reads exactly that call — so Overview's
 * *Since you posted* and these three groups are one answer about one record and cannot
 * disagree. Needs your answer is `contested`, which is `repliesToAnswer`, which is the tab's
 * count: one definition, three readers.
 *
 * | `sinceYouPosted` | This tab |
 * |---|---|
 * | contested | Needs your answer |
 * | fixed, answered | Settled |
 * | noReply | No reply yet |
 * | newCode | Not a group: a code change is not a reply. Each row's history says it |
 *
 * ## The three answers act where the record takes them, and nowhere else
 *
 * *Raise it again*, *Accept their reason* and *Withdraw* are offered on a finding
 * `answersTheAuthor` says they are for — carried in and settled — on a draft that still takes
 * decisions. That is the rule `FindingCall` offers them by on Findings, read from the same
 * function. On every other contested finding the row says why instead of drawing buttons
 * that would not work, and the reason is the record's, not the page's.
 */

import type { Finding, Outcome, ReviewDetail, Severity } from '~/types/desk'
import type { FindingRow } from './whatHappened'
import type { GroupedFinding } from './outcomes'
import type { RepliesWaiting } from './reviewTabs'

import { answersTheAuthor, draftTakesDecisions, isCarried, mostSeriousFirst } from './findings'
import { sentence, sinceLabel } from './format'
import { groupsOf } from './overview'
import { outcomeBy, outcomeSaid, outcomeWhen, repliesToAnswer } from './outcomes'
import { findingPath } from './reviewTabs'
import { rowFor } from './whatHappened'

/** How many findings with no reply are drawn before the rest fold. S09's "the first four". */
export const NO_REPLY_SHOWN = 4

/** What a row in Needs your answer or Settled stands at, as its word says it. */
export type ReplyStanding = 'contested' | 'fixed' | 'answered' | 'raised_again' | 'accepted' | 'withdrawn'

/** The word for each, and what it rests on — the words Overview's figures use. */
export const STANDING_WORDS: Record<ReplyStanding, { word: string, basis: string }> = {
  contested: { word: 'Contested', basis: 'told by the author' },
  fixed: { word: 'Fixed', basis: 'told by the author, not re-run' },
  answered: { word: 'Answered', basis: 'told by the author, not re-checked' },
  /*
   * **The developer's own answers, said as theirs.** After R, A or W the newest thing on the
   * finding is the developer's decision, so `repliesToAnswer` no longer counts it and the
   * shared grouping puts it with the answered. Drawn as "Answered · told by the author" it
   * credited the author with the developer's act — and said "settled" of a finding Raise it
   * again had just reopened. The word and the basis say what the record holds.
   */
  raised_again: { word: 'Raised again', basis: 'your answer: it posts again in this round' },
  accepted: { word: 'Reason accepted', basis: 'your answer: their reason is recorded' },
  withdrawn: { word: 'Withdrawn', basis: 'your answer: the record says you withdrew it' },
}

/** What the author said, as the row quotes it. */
export interface ReplySaid {
  /**
   * Who said it: the login the record holds, else `outcomeBy`'s "by the author". Never a
   * name the page made up.
   */
  who: string | null
  /** When, as `outcomeWhen` words it — "seen …" where the time is only when it was read. */
  when: string
  /** Their words, verbatim, or null for an outcome that carries none. */
  quote: string | null
  /** The record's sentence, for an outcome with no words of its own — a resolved thread. */
  sentence: string | null
}

/** Whether the three answers act on a contested finding here, and if not, why not. */
export interface ReplyAnswer {
  offered: boolean
  /** Null when offered. Otherwise one or two sentences, from the record. */
  why: string | null
  /**
   * **Where the answer really is**, when the record names it: a later round of this pull
   * request, whose Findings tab carries the three answers on its copy of the finding. A
   * finding keeps its number when carried, so `findingPath` to that round names it. Null
   * when no later round exists yet, and the sentence says it in words instead.
   */
  later: { round: number, to: string } | null
}

/** One finding in one of the three groups. */
export interface ReplyRow {
  seq: number
  key: string
  severity: Severity
  /** The finding's title, else its claim cut to a title's length (`shortened` says so). */
  line: string
  shortened: boolean
  /** Null in No reply yet. */
  standing: ReplyStanding | null
  /** The newest thing the author said that put it in its group. Null in No reply yet. */
  said: ReplySaid | null
  /** The three answers. Set in Needs your answer only. */
  answer: ReplyAnswer | null
  /** Everything the record holds on it — what the step-7 page drew — folded under the row. */
  history: FindingRow
  /**
   * What the row's standing rests on, as its meta line says it. **The developer's answer
   * carries its own time** (self-review round 2): the line read "the author · 17 hours ago ·
   * your answer: …", dating the developer's act by the author's reply.
   */
  basis: string | null
  /** The finding itself, for the answers' writes and for the history's links. */
  finding: Finding
}

export interface RepliesTab {
  needsAnswer: ReplyRow[]
  settled: ReplyRow[]
  noReply: ReplyRow[]
  /**
   * Findings with an outcome from somebody other than the developer that no group draws a
   * row for — a held-back nit whose code moved, say. Counted so the measurement is not lost.
   */
  elsewhere: number
}

/** The developer's three answers, by the outcome kind each records (`decision.rs`). */
const ANSWERED_AS: Partial<Record<Outcome['kind'], ReplyStanding>> = {
  raised_again: 'raised_again',
  rejected: 'accepted',
  withdrawn: 'withdrawn',
}

/**
 * Whether the newest thing on a finding is the developer answering the author — and which
 * answer — or null. By time, then by id, the order `repliesToAnswer` reads.
 */
function answeredBy(outcomes: readonly Outcome[], seq: number): { standing: ReplyStanding, at: string } | null {
  const newest = outcomes
    .filter(one => one.finding_seq === seq)
    .sort((one, two) => (Date.parse(two.occurred_at) - Date.parse(one.occurred_at)) || (two.id - one.id))[0]
  if (newest == null || newest.actor !== 'developer') return null
  const standing = ANSWERED_AS[newest.kind]
  return standing ? { standing, at: newest.occurred_at } : null
}

/** The words a standing rests on, with the time of the developer's own answer where it is theirs. */
function basisOf(standing: ReplyStanding, answeredAt: string | undefined, now: Date): string {
  const { basis } = STANDING_WORDS[standing]
  if (answeredAt == null || !basis.startsWith('your answer')) return basis
  return basis.replace('your answer', `your answer ${sinceLabel(answeredAt, now)}`)
}

/** The author's words on the outcome that grouped a finding, attributed. */
function saidOf(outcome: Outcome, now: Date): ReplySaid {
  const said = outcomeSaid(outcome)
  return {
    who: said.by ?? outcomeBy(said, outcome.actor),
    when: outcomeWhen(said, outcome.occurred_at, now),
    quote: said.quote,
    sentence: said.quote == null ? sentence(said.sentence) : null,
  }
}

/**
 * Why the three answers are not offered on this contested finding, or that they are.
 *
 * Ordered by what the developer can do about it, `FindingCall`'s order: the draft's state
 * first, because no finding on a posted or signed-off round takes a decision; then whether
 * this is a finding the three answers are for at all.
 */
export function answerFor(
  detail: Pick<ReviewDetail, 'review' | 'draft_status'> & Partial<Pick<ReviewDetail, 'rounds'>>,
  finding: Finding,
): ReplyAnswer {
  const round = detail.review.round
  const status = detail.draft_status ?? null
  const forThem = answersTheAuthor(finding)
  if (draftTakesDecisions(status) && forThem) return { offered: true, why: null, later: null }

  let lead: string
  if (status === 'posted') lead = `Round ${round} is posted, so its draft takes no more decisions.`
  else if (status === 'approved') lead = `Round ${round} is signed off, so its draft takes no more decisions until it is sent back.`
  else if (status == null) lead = `Round ${round} has no draft yet, so a decision would have nothing to change.`
  else if (!isCarried(finding)) lead = `${finding.key} was raised in this round.`
  else return {
    offered: false,
    why: `${finding.key} is still open in this round, so it posts as it stands. Findings is where it is decided.`,
    later: null,
  }

  /*
   * **Where the answer is, not only that it is not here** (DEV-1330 self-review, Question 4,
   * ruled by the coordinator). The record decides R/A/W per round and a round's outcomes are
   * its own, so the answer to a reply on a posted round is given on a later round's Findings
   * tab, on the finding that round carried in — never on its Replies tab.
   */
  const next = (detail.rounds ?? [])
    .filter(one => one.round > round && !one.dismissed)
    .sort((one, two) => two.round - one.round)[0]
  const later = next ? { round: next.round, to: findingPath(next.review_id, finding.key) } : null
  const where = later
    ? `Raise it again, Accept their reason and Withdraw are on round ${later.round}'s Findings tab, if it carries ${finding.key} in.`
    : `Raise it again, Accept their reason and Withdraw are on the Findings tab of the round that carries ${finding.key} in.`
  return { offered: false, why: `${lead} ${where}`, later }
}

/**
 * The tab, from the one response it has.
 *
 * Each group runs most serious first — the review's own grade, which is how the step-7 page
 * and the Findings list order their rows — rather than by what came back, which would be the
 * page grading the author's answer.
 */
export function repliesTab(detail: ReviewDetail, now: Date = new Date()): RepliesTab {
  const groups = groupsOf(detail)
  const bySeq = new Map(detail.findings.map(finding => [finding.seq, finding]))

  const rows = (
    grouped: GroupedFinding[],
    standing: (one: GroupedFinding) => ReplyStanding | null,
  ): ReplyRow[] => {
    const known = grouped.filter(one => bySeq.has(one.seq))
    const order = mostSeriousFirst(known.map(one => bySeq.get(one.seq)!))
    return order.map((finding) => {
      const one = known.find(each => each.seq === finding.seq)!
      const history = rowFor(detail, finding, now)
      const stands = standing(one)
      return {
        seq: finding.seq,
        key: finding.key,
        severity: finding.severity,
        line: history.line,
        shortened: history.shortened,
        standing: stands,
        said: one.outcome ? saidOf(one.outcome, now) : null,
        answer: stands === 'contested' ? answerFor(detail, finding) : null,
        basis: stands == null ? null : basisOf(stands, answeredBy(detail.outcomes, finding.seq)?.at, now),
        history,
        finding,
      }
    })
  }

  const needsAnswer = rows(groups.contested, () => 'contested')
  const settledSeqs = new Set(groups.fixed.map(one => one.seq))
  const settled = rows(
    [...groups.fixed, ...groups.answered],
    one => answeredBy(detail.outcomes, one.seq)?.standing ?? (settledSeqs.has(one.seq) ? 'fixed' : 'answered'),
  )
  const noReply = rows(groups.noReply, () => null)

  const drawn = new Set([...needsAnswer, ...settled, ...noReply].map(row => row.seq))
  const elsewhere = detail.outcomes
    .filter(outcome => outcome.actor !== 'developer' && !drawn.has(outcome.finding_seq))
    .length

  return { needsAnswer, settled, noReply, elsewhere }
}

/**
 * **The replies the developer can answer here and now**, by key, in finding order: the ones
 * `repliesToAnswer` counts that the record would also take an answer on (`answerFor`).
 *
 * The header's *Answer F3* reads this rather than the count, because an action that opens a
 * page where the record refuses it is not an action — `docs/design.md`: "a review with
 * nothing for the developer to do has no primary action". The Replies tab's **count** stays
 * `repliesToAnswer`: a contested reply waits on the developer whether or not this round can
 * take the answer, and the tab says why it cannot.
 */
export function answerableReplies(
  detail: Pick<ReviewDetail, 'review' | 'draft_status' | 'findings' | 'outcomes'>,
): string[] {
  const byKey = new Map(detail.findings.map(finding => [finding.key, finding]))
  return repliesToAnswer(detail.outcomes).filter((key) => {
    const finding = byKey.get(key)
    return finding != null && answerFor(detail, finding).offered
  })
}

/**
 * The replies waiting on the developer, as the header's action reads them: the ones the record
 * would take an answer on here, and all of them. DEV-1330 self-review, Should 3.
 */
export function repliesWaiting(
  detail: Pick<ReviewDetail, 'review' | 'draft_status' | 'findings' | 'outcomes'>,
): RepliesWaiting {
  return { answer: answerableReplies(detail), read: repliesToAnswer(detail.outcomes) }
}
