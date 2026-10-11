/*
 * The derivations the Solution page's table makes, out of the page so each can be checked
 * on its own.
 *
 * Board A10 is almost entirely arithmetic over the parts and the findings — which of three
 * groups a part belongs to, the one line its row carries, how big it is, and where the
 * review looked in it. None of it is in the record and none of it should be in a template:
 * a rule worked out inline is a rule nothing can test, and two of these decide what a
 * reader is told about the completeness of the brief.
 *
 * **Nothing here reads a component, a route or a resource.** Every function takes what it
 * needs and answers a value, which is what makes `test/solution.spec.ts` a test of the
 * rules rather than of a render — the shape `utils/findings.ts` argues for.
 */

import type { Brief, BriefPart, BriefPartFile, Disposition, Finding } from '~/types/desk'

import { partOf, roundStatusLabel } from './findings'
import { count } from './format'

/* --------------------------------- The three groups -------------------------------- */

/**
 * Which of board A10's three groups a part belongs to.
 *
 * The issue's own rule table, in order, and the order is what makes it a rule rather than
 * three overlapping tests:
 *
 * | Group | A part belongs here when |
 * |---|---|
 * | `fixes` | It names at least one problem in `fixes` **and** has a file in the change |
 * | `supporting` | It names no problem and has a file in the change |
 * | `unchanged` | **None of its files is in the change** |
 *
 * **`unchanged` is tested first, and that is the whole point of the group.** A part the
 * pull request did not touch is not something that was built, whatever it claims to fix —
 * so a part with `fixes: [3]` and one file at `in_diff: false` reads under "Not changed,
 * but the change relies on it" rather than being announced as the thing that fixed problem
 * 3. Deciding on `fixes` first would put it in the group that says the opposite.
 *
 * **A part with no files at all is `unchanged` too**, because "none of its files is in the
 * change" is true of it. The contract permits one; it is not a state anything writes today,
 * and the honest reading is that nothing of the change has been attributed to it.
 */
export type PartGroup = 'fixes' | 'supporting' | 'unchanged'

/** Whether any of this part's files is one the pull request changed. */
export function isInTheChange(part: BriefPart): boolean {
  return files(part).some(file => file.in_diff)
}

export function groupOf(part: BriefPart): PartGroup {
  if (!isInTheChange(part)) return 'unchanged'
  return part.fixes.length > 0 ? 'fixes' : 'supporting'
}

/** The heading over each group. A group with no part in it is not drawn at all. */
export const GROUP_HEADINGS: Record<PartGroup, string> = {
  fixes: 'Fixes a problem',
  supporting: 'Supporting',
  unchanged: 'Not changed, but the change relies on it',
}

/** One row of the table: a part, and the number it carries in the record. */
export interface PartRow {
  part: BriefPart
  /** Its position in `brief.parts`, from 1. The number drawn in the first column. */
  number: number
  /** Its index in `brief.parts`, which is what `openPart` and `aria-controls` key on. */
  index: number
}

/** One group of the table, drawn only when it holds a row. */
export interface PartGrouping {
  group: PartGroup
  heading: string
  rows: PartRow[]
}

/**
 * The table: the three groups, each holding its parts **in the record's order**.
 *
 * The record's order within a group, never a re-sort — the session chose the order it
 * described the change in, and a part's number is drawn beside it, so a reader who reads
 * "5" above "2" would be looking at a page that renumbered the brief.
 *
 * A group with no parts is absent from the list rather than present and empty, which is
 * what lets the template draw no heading for it without asking a second question.
 */
export function partGroups(parts: BriefPart[]): PartGrouping[] {
  const order: PartGroup[] = ['fixes', 'supporting', 'unchanged']
  return order
    .map(group => ({
      group,
      heading: GROUP_HEADINGS[group],
      rows: parts
        .map((part, index) => ({ part, number: index + 1, index }))
        .filter(row => groupOf(row.part) === group),
    }))
    .filter(grouping => grouping.rows.length > 0)
}

/* ----------------------------------- The one line ---------------------------------- */

/**
 * What a row carries under the part's title, and whether it is the part's own words.
 *
 * `one_line` where the session wrote one. Where it did not, the `summary` — **cut to a
 * single line by the column it is in, not by this function.** A character count cannot know
 * how wide that column is at 390px, and one that guessed would clip a sentence that fitted
 * and leave a cut one that did not. The whole string stays in the document either way, so a
 * screen reader gets all of it; that is the bargain `ProblemTag`'s label and the step
 * trail's one line already strike.
 *
 * `cut` is what the template needs in order to say so: a derived line gets the `title`
 * attribute carrying the whole summary, which is the code review page's own answer for a
 * finding with no title of its own.
 */
export interface OneLine {
  text: string
  /** True when this is the summary standing in for a one line the session did not write. */
  cut: boolean
}

export function oneLineOf(part: BriefPart): OneLine {
  return part.one_line == null
    ? { text: part.summary, cut: true }
    : { text: part.one_line, cut: false }
}

/* -------------------------------------- The size ----------------------------------- */

/** How big a part is: its files, and what the change did to them. */
export interface PartSize {
  /** "3 files", or "1 file". */
  files: string
  /** "+134 −70", or null for a part with nothing in the change. */
  lines: string | null
  /** True when none of its files is in the change, which the row says in words. */
  unchanged: boolean
}

/**
 * The size cell: the number of files, then the lines added and removed.
 *
 * **Summed from the part's own files rather than read off `lines_added`.** The two agree —
 * the server derives those fields by summing exactly this list — and summing here is what
 * keeps them agreeing when the row and the fold inside it are read against each other.
 *
 * A part with no file in the change says "unchanged" instead of a pair of noughts, which is
 * the issue's own wording. `+0 −0` is a measurement that reads as a part nobody wrote
 * anything in, and this is a part the pull request did not touch — a different fact.
 */
export function sizeOf(part: BriefPart): PartSize {
  const own = files(part)
  const added = own.reduce((total, file) => total + file.lines_added, 0)
  const removed = own.reduce((total, file) => total + file.lines_removed, 0)
  return {
    files: own.length === 1 ? '1 file' : `${own.length} files`,
    lines: isInTheChange(part) ? `+${added} −${removed}` : null,
    unchanged: !isInTheChange(part),
  }
}

/* --------------------------- What the code review found ---------------------------- */

/**
 * The row's account of the review, as up to three plain counts.
 *
 * To post, held for a ticket, set aside — which are the three dispositions, and the three
 * lists the code review page owns. Two things about that are deliberate.
 *
 * **It is keyed on the disposition, not on `willPost`**, and the limit of that is stated
 * here rather than left for a reader to find. These are the three lists the code review
 * page owns, under the names its own segmented control gives them — so "2 to post" means
 * "two in the To post list", a statement about where the review put them, not a prediction
 * about what the author receives. That second claim belongs to the code review page's
 * headline, which counts `willPost` and is the page that makes it.
 *
 * Keying this on `willPost` would also break the one property the cell has to have: the
 * three counts sum to the findings in the part, so every finding on the page is accounted
 * for exactly once, which is what makes the count checkable against the record. `willPost`
 * splits `in_draft` in two and there is no fourth bucket for the half that falls out.
 *
 * **Where the two differ, the line inside the row says so.** A finding carried into this
 * round as `fixed` keeps whatever disposition it was carried with, so it sits in "to post"
 * and `DraftSection.posted` will not compose it. Its round status is drawn beside its
 * title — which the cards this replaces did, DEV-892 having put it there, and which an
 * earlier draft of this page dropped. The count alone would overstate by one and nothing
 * on the page would explain it.
 *
 * **Nothing here grades the part.** No severity, no colour, no ordering by seriousness:
 * "the pages give understanding, not verdicts". A cell that put the blocker first and
 * painted it red would be telling the reader which part to be afraid of, which is a
 * judgement the record does not hold and the developer has not made.
 */
export interface FoundCount {
  label: string
  n: number
}

const WHERE_IT_STANDS: Array<[Disposition, string]> = [
  ['in_draft', 'to post'],
  ['held_back', 'held for a ticket'],
  ['suppressed', 'set aside'],
]

export function foundOn(findings: Finding[]): FoundCount[] {
  return WHERE_IT_STANDS.map(([disposition, label]) => ({
    label,
    n: findings.filter(finding => finding.disposition === disposition).length,
  })).filter(entry => entry.n > 0)
}

/** How a finding line says where that finding stands. The same three words, capitalised. */
export function standsAt(finding: Finding): string {
  const found = WHERE_IT_STANDS.find(([disposition]) => disposition === finding.disposition)
  const label = found ? found[1] : 'to post'
  return label.charAt(0).toUpperCase() + label.slice(1)
}

/* ---------------------- The findings on a part, and on no part --------------------- */

/**
 * The findings this part owns, in the review's own order.
 *
 * **`partOf`, which is the first part that claims the file.** The page used to filter on
 * "any part whose files include this path", and the two are not the same rule: a path named
 * by two parts put its findings under both, and the counts then summed to more findings
 * than the review has. `partOf` is the rule the parts strip on the code review page already
 * uses, and the store's own primary key `(review_id, path)` says the same thing — so there
 * is one answer to "which part owns this file" and three places agree on it.
 *
 * In `seq` order, not severity order: a reader works down the findings of a part the way
 * they work down the findings of a review, and the row above deliberately does not grade
 * the part, so neither does the list inside it.
 */
export function findingsOn(findings: Finding[], parts: BriefPart[], index: number): Finding[] {
  return bySeq(findings.filter(finding => partOf(finding, parts) === index + 1))
}

/**
 * The findings no part accounts for.
 *
 * Shown rather than dropped. A brief whose parts do not between them name every file a
 * finding sits in has described the change incompletely, and that is a fact about the brief
 * rather than about the page — so the count is the heading.
 *
 * `partOf` answers `0` both for a finding naming no file and for one in a file no part
 * claims, which is the right pairing: neither is about any one part of the build.
 */
export function findingsInNoPart(findings: Finding[], parts: BriefPart[]): Finding[] {
  return bySeq(findings.filter(finding => partOf(finding, parts) === 0))
}

/** The heading over that list, which has to agree with itself about one finding. */
export function noPartHeading(n: number): string {
  return `${count(n, 'finding')} ${n === 1 ? 'is' : 'are'} not about any one part`
}

/* ------------------------------------- Plumbing ------------------------------------ */

/** A named problem's label, for the tag on a row. Null where the record holds none. */
export function labelOfProblem(brief: Brief, number: number): string | null {
  return (brief.problems ?? []).find(problem => problem.number === number)?.label ?? null
}

/**
 * Where a finding line goes: that finding on the code review page, not just the page.
 *
 * The hash is the finding's key, and the code review page reads it on arrival — it chooses
 * the list the finding is in, clears the part filter and opens the row. Linking to the page
 * alone would leave the reader to find F9 among fifteen rows across three lists, which is
 * the work the line was supposed to have done for them.
 */
export function findingLink(reviewId: number, finding: Finding): string {
  return `/reviews/${reviewId}/code-review#${finding.key}`
}

/**
 * A carried finding's round status, with the space that separates it from the title.
 *
 * **The space is in the string, not in the stylesheet.** `margin-left` moves the box and
 * leaves the text nodes touching, so the line's accessible name came out as "…names no
 * columnsFixed" and copying it gave the same — found reviewing this change. The pieces of
 * a finding line are drawn edge to edge, as `TickedText` explains, because a newline
 * between two tags in a Vue template is condensed away; so the separator has to be part of
 * what is interpolated.
 *
 * Null for a finding **not carried in** — which is `roundStatusLabel`'s own rule, and not
 * the same as "raised this round" however much it reads like it. That helper keys on
 * `continues_review_id`, because `round_status` is `NOT NULL DEFAULT 'new'`: a carried
 * finding nobody has decided about still carries `new`, and still says "Raised this round"
 * beside its title. An earlier draft of this sentence said the other thing.
 */
export function carriedAfterTitle(finding: Finding): string | null {
  const label = roundStatusLabel(finding)
  return label == null ? null : ` ${label}`
}

/** The hash a finding line writes, read back: the `seq` it names, or null. */
export function seqInHash(hash: string): number | null {
  const found = /^#F([0-9]+)$/.exec(hash)
  if (!found) return null
  const seq = Number(found[1])
  return Number.isSafeInteger(seq) && seq > 0 ? seq : null
}

function files(part: BriefPart): BriefPartFile[] {
  return part.files ?? []
}

/** A new array in `seq` order; the caller's order is not touched. */
function bySeq(findings: Finding[]): Finding[] {
  return [...findings].sort((one, two) => one.seq - two.seq)
}
