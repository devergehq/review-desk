/*
 * **What came back on a posted review, as the step-7 page draws it.** Auto-imported.
 *
 * Nothing here reads or writes a record: it takes the `ReviewDetail` the page already has
 * and answers the rows. That is what makes `test/what-happened.spec.ts` a test of the
 * rules rather than a test of a rendering, and it is why the page itself is mostly markup.
 *
 * ## It describes; it does not grade
 *
 * DEV-958 asks for a page that says what happened and stops. So:
 *
 * * **No ordering by anything but time and the record's own order.** The outcomes under a
 *   finding run oldest first, which is the order they happened in; the findings run in the
 *   record's order, which is the order they were raised in. Sorting by "how bad it looks"
 *   would be the page deciding, and the page is not in a position to.
 * * **A severity is the record's own word**, with no tag and no colour. `HeldBackRows`
 *   makes the same decision for the same reason and states it: a coloured severity tells a
 *   reader which of these to be afraid of, which is a judgement about what came back and
 *   not a report of it.
 * * **No sentence says a finding was fixed.** `outcomes.ts` argues this at length. The
 *   harvester compares two commits; it does not read a reply or a diff.
 *
 * ## What it can and cannot know
 *
 * Everything is derived from one response, `GET /reviews/{id}`, and nothing new is read
 * from GitHub — both of which the issue puts out of scope. Three consequences worth having
 * written down:
 *
 * | Question | What the record supports |
 * |---|---|
 * | When did the pull request merge? | Nothing. The store holds `pr_state_at` and the contract deliberately does not serve it, so the page says *that* it merged and not when |
 * | Which findings did a *later* round carry? | This review is one round. A finding carried **into** this one names the round it came from — `continues_round` — and `rounds` gives the pull request's whole timeline. A later round's own findings are a different review and a different read |
 * | Was a finding posted? | `willPost`, the store's `Finding::is_posted` restated: at disposition `in_draft` with an open round status. A `github_comment_id` is a stronger fact but a narrower one — it means posted **as a comment**, which is the only way a reply can come back, and most findings are posted in the body alone |
 */

import type {
  Finding,
  Outcome,
  ReviewDetail,
  RoundSummary,
  Severity,
} from '~/types/desk'
import type { OutcomeSaid } from './outcomes'

import { shortenClaim } from './draft'
import { isCarried, mostSeriousFirst, roundStatusLabel, whereOf, willPost } from './findings'
import { commentUrl } from './links'
import { count, sentence, sinceLabel } from './format'
import { outcomeSaid, outcomeWhen, prEndLabel } from './outcomes'

/** One thing that came back, under the finding it came back on. */
export interface CameBack extends OutcomeSaid {
  /** The store's own id, which is what a `v-for` keys on. */
  id: number
  /** When it happened, or — where GitHub does not say — when it was seen. */
  when: string
}

/** One posted finding, and what came back on it. */
export interface FindingRow {
  seq: number
  key: string
  /** The record's own word, drawn as a word. See the header: no tag and no colour. */
  severity: Severity
  /** The finding's title, else its claim cut to a title's length. */
  line: string
  /** True when `line` is the claim cut rather than a title the review wrote. */
  shortened: boolean
  /** `path:line`, or the sentence for a finding about the pull request itself. */
  where: string
  /** "Carried from round 1 · Still open", or null for a finding raised in this round. */
  carried: string | null
  /** Where this finding is on the code review page. Always drawn: the page always exists. */
  finding: string
  /** Where its comment is on GitHub, or null when no address can be built. See `commentUrl`. */
  comment: string | null
  /** What came back, oldest first. Empty for a finding nothing came back on. */
  cameBack: CameBack[]
}

/** One round of this pull request, as the page's last section lists it. */
export interface RoundRow {
  round: number
  /** Whether it is the round this page is about. */
  here: boolean
  /** "14 findings · 2 replies · 1 thread resolved", from the round's own counts. */
  said: string
  /** "4 carried from an earlier round", or null for a round that carried none. */
  carried: string | null
}

/** Everything the page draws. */
export interface WhatHappened {
  /** The page's one line: how the pull request ended, or that it is open. */
  headline: string
  /** The line under it: when it was last read, and what came back, counted. */
  sub: string
  /** Whether any harvest has read this review at all. */
  harvested: boolean
  /** The posted findings something came back on, in the record's order. */
  answered: FindingRow[]
  /** The posted findings nothing came back on. Counted and folded by the page. */
  silent: FindingRow[]
  /** The pull request's rounds, or empty for a response that carries none. */
  rounds: RoundRow[]
  /** Whether any row's time is an observation rather than the act. Footnoted once. */
  anyObserved: boolean
  /** Whether any posted finding has no comment this page could address. Footnoted once. */
  anyWithoutComment: boolean
}

/** The outcomes on one finding, worded, oldest first. */
function cameBackOn(outcomes: Outcome[], finding: Finding, now: Date): CameBack[] {
  return outcomes
    .filter(outcome => outcome.finding_seq === finding.seq)
    /*
     * **Oldest first, which is the issue's own criterion** — "a finding with several
     * outcomes shows them in time order". Sorted on the instant rather than left in the
     * record's order: the harvester writes a review's replies, then its resolutions, then
     * its code changes, so the record's order is the order of the *reads* and a reply
     * written after a resolution would be drawn before it.
     *
     * `localeCompare` on the stored strings, which are RFC 3339 and sort lexically in
     * their own order — the same comparison the queue's groups use.
     */
    .sort((one, two) => one.occurred_at.localeCompare(two.occurred_at))
    .map((outcome) => {
      const said = outcomeSaid(outcome)
      return {
        ...said,
        /*
         * **The path, only where it is not the one the row already carries.** The harvester
         * records the file it compared, which is the finding's own file in every ordinary
         * case — so drawing it here printed `app/Reporting/ScheduledExportColumnPolicyResolver.php`
         * twice in four lines, once under the title and once after the commit. Seen in the
         * browser at 390, where the two copies were four wrapped lines between them.
         *
         * It is kept rather than dropped because the two can differ: a file renamed after
         * the review was read is compared under one name and recorded under another, and
         * that difference is the whole of what a reader needs to know.
         */
        file: said.file === finding.file ? null : said.file,
        id: outcome.id,
        sentence: sentence(said.sentence),
        when: outcomeWhen(said, outcome.occurred_at, now),
      }
    })
}

/**
 * The developer's own decisions are not on this page.
 *
 * It is the page for what came back, and a decision the developer made is not something
 * that came back — the code review page draws those where they were made. `actor`, not
 * kind: a `rejected` recorded by the author is the author's reason and belongs here, and a
 * `cut` recorded by the developer does not. The same test `saidByOthers` uses, for the same
 * reason.
 */
function fromOthers(outcomes: Outcome[]): Outcome[] {
  return outcomes.filter(outcome => outcome.actor !== 'developer')
}

/** One posted finding's row, with whatever came back on it. */
function rowFor(detail: ReviewDetail, finding: Finding, now: Date): FindingRow {
  const cut = shortenClaim(finding.title ?? finding.claim)
  const status = roundStatusLabel(finding)
  return {
    seq: finding.seq,
    key: finding.key,
    severity: finding.severity,
    /*
     * The title where the review wrote one, else its claim cut to a title's length — the
     * `HeldBackRows` rule, and the row says when it did the cutting. A finding's claim runs
     * to 56 words on the review this app was measured against, and fourteen of those down
     * one column is the wall of prose the code review page was rebuilt to stop.
     */
    line: finding.title ?? cut.text,
    shortened: finding.title == null && cut.shortened,
    where: whereOf(finding),
    carried: isCarried(finding)
      ? [
          finding.continues_round == null
            ? 'Carried from an earlier round'
            : `Carried from round ${finding.continues_round}`,
          status,
        ].filter(Boolean).join(' · ')
      : null,
    finding: `/reviews/${detail.review.id}/code-review#${finding.key}`,
    comment: commentUrl(detail.review.repo, detail.review.pr, finding.github_comment_id),
    cameBack: cameBackOn(fromOthers(detail.outcomes), finding, now),
  }
}

/** One round of the pull request, from its own counts. */
function roundRow(summary: RoundSummary, here: number): RoundRow {
  const said = [
    count(summary.findings, 'finding'),
    summary.replies > 0 ? count(summary.replies, 'reply', 'replies') : null,
    summary.threads_resolved > 0 ? `${count(summary.threads_resolved, 'thread')} resolved` : null,
  ].filter(Boolean).join(' · ')

  /*
   * **What a later round carried, which is the part of the issue's table this answers.**
   * `RoundStatusCounts` holds the statuses of that round's own findings, and every status
   * but `new` is a finding it carried — a finding raised in a round is `new` there by
   * definition. So "4 carried, 2 of them still open" is readable from the counts without
   * reading that round's findings, which is a second request this page does not make.
   */
  const statuses = summary.statuses
  const carried = statuses.fixed + statuses.still_open + statuses.accepted
    + statuses.withdrawn + statuses.ticketed

  return {
    round: summary.round,
    here: summary.round === here,
    said,
    carried: carried === 0
      ? null
      : `${count(carried, 'finding')} carried in, ${statuses.still_open} still open`,
  }
}

/**
 * The line under the headline: when the review was last read, and what came back.
 *
 * **Counted here and not taken from the step's own facts.** `ReviewLayer.facts` carries the
 * same four figures and the trail's card draws them, but they are a frozen string the
 * harvest wrote — `record_harvest_step` composes them — and this page holds the outcomes
 * themselves. Drawing a count of rows that disagreed with the rows under it is the defect
 * this whole issue is about, one level up.
 */
function subLine(detail: ReviewDetail, answered: FindingRow[], posted: number, now: Date): string {
  const read = detail.review.harvested_at == null
    ? null
    : `Last read ${sinceLabel(detail.review.harvested_at, now)}`

  const kinds = fromOthers(detail.outcomes)
  const replies = new Set(
    kinds.filter(one => one.kind === 'replied').map(one => one.finding_seq),
  ).size
  /*
   * **Distinct findings, not rows.** The harvester keys a code change on the commit it saw
   * it in, so a finding whose file is touched by three pushes holds three outcomes — and
   * counting rows gives a figure about findings that can exceed the findings it is about.
   * `Store::posted_reviews_awaiting_author` counts it `DISTINCT` for the same reason and
   * says so.
   */
  const changed = new Set(
    kinds.filter(one => one.kind === 'code_changed_after').map(one => one.finding_seq),
  ).size

  const how = [
    replies > 0 ? `${replies} with a reply` : null,
    changed > 0 ? `${changed} whose code changed` : null,
  ].filter(Boolean).join(', ')

  const back = answered.length === 0
    ? 'Nothing has come back on any of them'
    : `${answered.length} of them had something come back${how === '' ? '' : `: ${how}`}`

  /*
   * **And it does not say a third time that nothing has read this.** The headline is
   * `prEndLabel`, which for a review with no harvest behind it already says exactly that,
   * and the page draws a notice under it saying what follows. A line here saying "recorded
   * without a harvest" put the same fact on the screen three times in four inches — which
   * is the fault `steps.ts` names in as many words: a page saying one thing and charging
   * for two.
   */
  return `${[read, `${count(posted, 'finding')} were posted`, back]
    .filter(Boolean)
    .join('. ')}.`
}

/**
 * Everything the step-7 page draws, from the one response it already has.
 *
 * `now` is a parameter so the test can fix the clock; every caller on a screen takes the
 * default, which is the convention `format.ts` sets for everything time-worded here.
 */
export function whatHappened(detail: ReviewDetail, now: Date = new Date()): WhatHappened {
  /*
   * **The posted findings, most serious first.** The record's order within a severity, and
   * the severities in the order the code review page ranks them — so a reader arriving from
   * that page's rows meets them in the same order. That is a ranking by the review's own
   * grade, which the review made; it is not this page grading what came back.
   */
  const posted = mostSeriousFirst(detail.findings.filter(willPost))
  const rows = posted.map(finding => rowFor(detail, finding, now))
  const answered = rows.filter(row => row.cameBack.length > 0)
  const silent = rows.filter(row => row.cameBack.length === 0)

  return {
    headline: prEndLabel(detail.review.pr_state),
    sub: subLine(detail, answered, rows.length, now),
    harvested: detail.review.harvested_at != null,
    answered,
    silent,
    /*
     * Only where the pull request has more than one round. A timeline of one round is a
     * restatement of the page's own heading, and every review is a round.
     */
    rounds: (detail.rounds ?? []).length > 1
      ? (detail.rounds ?? []).map(summary => roundRow(summary, detail.review.round))
      : [],
    anyObserved: answered.some(row => row.cameBack.some(one => one.observed)),
    anyWithoutComment: rows.some(row => row.comment == null),
  }
}
