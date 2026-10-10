/*
 * The derivations the code review page makes, out of the page so each can be checked on its
 * own.
 *
 * Board A8 is almost entirely arithmetic over the findings — the headline from the counts,
 * a part's tally, which of three lists a finding is in, how long its reasoning runs. None of
 * it is in the record and none of it should be in a template: a count worked out inline is a
 * count nothing can test, and the one that matters most is the one Patrick will read first.
 *
 * **Nothing here reads a component, a route or a resource.** Every function takes what it
 * needs and answers a value, which is what makes `test/findings.spec.ts` a test of the rules
 * rather than of a render.
 */

import type {
  Brief,
  BriefPart,
  Finding,
  FindingHunk,
  RoundStatus,
  Severity,
  SeverityCounts,
} from '~/types/desk'

import { count } from './format'

/**
 * Most serious first, and then by number.
 *
 * **Question sits above nit**, which is board A8's order and not the order the contract
 * declares its enum in. A question is something the reviewer could not answer and the author
 * can; a nit is something neither of them has to do anything about. Sorting by the enum
 * would put the four nits above the three questions on the review this was measured against,
 * which is the wrong way round for the thing the page is for.
 */
const SEVERITY_ORDER: Record<Severity, number> = {
  blocker: 0,
  should: 1,
  question: 2,
  nit: 3,
}

/** Most serious first, then by number. A new array; the caller's order is not touched. */
export function mostSeriousFirst(findings: Finding[]): Finding[] {
  return [...findings].sort(
    (one, two) =>
      SEVERITY_ORDER[one.severity] - SEVERITY_ORDER[two.severity] || one.seq - two.seq,
  )
}

/**
 * Which of the three lists a finding is in.
 *
 * The segmented control's three groups, straight off `Disposition` — `in_draft` is what the
 * review posts, `held_back` is what goes to a ticket, `suppressed` is what was set aside.
 * Named for what the developer is choosing between rather than for the wire values, because
 * "to post" and "set aside" are the words on the control.
 */
export type FindingList = 'post' | 'ticket' | 'aside'

export function listOf(finding: Finding): FindingList {
  if (finding.disposition === 'in_draft') return 'post'
  if (finding.disposition === 'held_back') return 'ticket'
  return 'aside'
}

/** How the control labels each list. */
export const LIST_LABELS: Record<FindingList, string> = {
  post: 'To post',
  ticket: 'For a ticket',
  aside: 'Set aside',
}

/**
 * The headline, from the counts of what is going to be posted.
 *
 * "No blockers. 4 should change, 1 question, 6 small points." — board A8's own sentence,
 * which is the first thing on the page and the only thing a developer who trusts the review
 * may read. Three things about it are deliberate.
 *
 * **It counts the list that posts, not every finding.** A review with six nits set aside has
 * not found six small points to tell the author about; it has found none.
 *
 * **"No blockers" is said out loud.** It is the one count worth stating at zero, because it
 * is the answer to the question the developer opens the page with. Every other zero is left
 * out: "0 questions" is noise.
 *
 * **The words are the developer's, not the enum's.** A nit is "a small point" in a sentence
 * and "Nit" on a tag, because a tag is a label and this is prose.
 */
export function headline(posting: Finding[]): string {
  /*
   * Narrowed here rather than at the caller, so that every caller gets it right: a
   * `headline()` that trusted its argument would be one more place to pass the wrong list.
   */
  posting = posting.filter(willPost)

  const of = (severity: Severity) =>
    posting.filter(finding => finding.severity === severity).length

  const blockers = of('blocker')
  const parts: string[] = []
  if (of('should') > 0) parts.push(`${of('should')} should change`)
  if (of('question') > 0) parts.push(count(of('question'), 'question'))
  if (of('nit') > 0) parts.push(count(of('nit'), 'small point'))

  const lead = blockers === 0 ? 'No blockers' : count(blockers, 'blocker')

  /*
   * A review that found nothing to post still has a headline, and it is not the lead
   * sentence with a full stop after it: "No blockers." alone reads as a review that looked
   * and found one thing. This says what it means.
   */
  if (parts.length === 0) {
    return blockers === 0
      ? 'Nothing to post. The review found nothing it would say to the author.'
      : `${lead}, and nothing else.`
  }

  return `${lead}. ${parts.join(', ')}.`
}

/**
 * How many of the findings were reproduced by running something.
 *
 * The line under the headline — "6 of 14 findings were reproduced by running something. The
 * rest were read, not run." **`unknown` counts as neither**, and the sentence says "the
 * rest" rather than naming them: a finding written before the field existed has said nothing
 * about its method, and "read, not run" is a claim about how it was checked.
 */
export function reproducedLine(findings: Finding[]): string | null {
  if (findings.length === 0) return null
  const reproduced = findings.filter(
    finding => finding.verification === 'reproduced',
  ).length
  if (reproduced === 0) {
    return 'None of these was reproduced by running something; they were read.'
  }
  /*
   * **One finding is its own sentence.** "1 of 1 finding were reproduced … The rest were
   * read, not run." is the wrong verb and a "rest" that refers to nothing — and a review
   * with one finding is not a rare shape.
   */
  if (reproduced === findings.length) {
    return findings.length === 1
      ? 'It was reproduced by running something.'
      : `All ${count(findings.length, 'finding')} were reproduced by running something.`
  }
  /*
   * Each verb agrees with its own subject: "1 of 3 findings **was** reproduced" counts the
   * reproduced ones, not the three. Keyed on the total, the first verb is wrong exactly
   * when the second is right.
   */
  const rest = findings.length - reproduced
  return (
    `${reproduced} of ${count(findings.length, 'finding')} `
    + `${reproduced === 1 ? 'was' : 'were'} reproduced by running something. The rest `
    + `${rest === 1 ? 'was' : 'were'} read, not run.`
  )
}

/**
 * One tile of the parts strip.
 *
 * `number` is the part's position from 1, `0` for "Everything" and `-1` for "Outside the
 * parts" — board A8's own three cases, as one list so the strip is one loop.
 */
export interface PartTile {
  number: number
  name: string
  /** "4 to post · 2 not", or "Nothing found". */
  tally: string
  /** How many findings fall here at all, which is what decides the wording. */
  total: number
}

/** Which part a finding belongs to: `0` when no part claims its file. */
export function partOf(finding: Finding, parts: BriefPart[]): number {
  if (finding.file == null) return 0
  /*
   * **The first part that claims the file, not every part that does.** A file named by two
   * parts would otherwise put its findings in both, and the tiles' counts would sum to more
   * than the review has — which is the one thing a strip of counts must not do.
   */
  const at = parts.findIndex(part => part.files.some(file => file.path === finding.file))
  return at === -1 ? 0 : at + 1
}

/** Whether a finding is in the part a tile stands for. */
export function inPart(finding: Finding, tile: number, parts: BriefPart[]): boolean {
  if (tile === 0) return true
  const part = partOf(finding, parts)
  return tile === -1 ? part === 0 : part === tile
}

/**
 * The strip: "Everything", one tile per part, and "Outside the parts".
 *
 * **"Outside the parts" is drawn only when something is outside them.** A tile reading
 * "Nothing found" for a case that cannot arise is a filter nobody would press, where a part
 * reading it is a measurement — that part of the change has no findings, which is worth
 * seeing.
 */
export function partTiles(findings: Finding[], parts: BriefPart[]): PartTile[] {
  const tally = (tile: number): PartTile => {
    const mine = findings.filter(finding => inPart(finding, tile, parts))
    const posting = mine.filter(finding => listOf(finding) === 'post').length
    const rest = mine.length - posting
    return {
      number: tile,
      name:
        tile === 0
          ? 'Everything'
          : tile === -1
            ? 'Outside the parts'
            : (parts[tile - 1]?.title ?? ''),
      tally:
        mine.length === 0
          ? 'Nothing found'
          : `${posting} to post${rest > 0 ? ` · ${rest} not` : ''}`,
      total: mine.length,
    }
  }

  const tiles = [tally(0), ...parts.map((_, at) => tally(at + 1))]
  const outside = tally(-1)
  return outside.total > 0 ? [...tiles, outside] : tiles
}

/**
 * The reasoning, as the labelled blocks the page opens.
 *
 * A finding's `evidence` is written by a review session as paragraphs, and the ones that run
 * long open with their own label — "Mechanism: …", "How we know: …", "Limits: …". Board A8
 * draws them as a heading and a body, which is the whole difference between 221 words of
 * reasoning being readable and being the wall of text this page replaces.
 *
 * **A paragraph with no label of its own gets one, and the two defaults are not the same
 * word.** The first paragraph of a piece of evidence is the mechanism — what is actually
 * happening — and anything after it is detail. Labelling every unlabelled paragraph
 * "Mechanism" would put the heading on three blocks that are not one.
 *
 * The label is read off the paragraph only when it looks like a label: a capitalised phrase
 * of two to forty-four characters, no newline in it, followed by a colon and a space. The
 * bound is what stops a sentence containing a colon — "the rule is this: never" — becoming a
 * forty-word heading.
 */
export interface ReasoningBlock {
  label: string
  text: string
}

const LABEL = /^([A-Z][^:\n]{2,44}):[ \t]+/

export function reasoningBlocks(evidence: string | null | undefined): ReasoningBlock[] {
  const paragraphs = (evidence ?? '')
    .split(/\n\s*\n/)
    .map(paragraph => paragraph.trim())
    .filter(paragraph => paragraph.length > 0)

  return paragraphs.map((paragraph, at) => {
    const found = LABEL.exec(paragraph)
    if (found) {
      /* Sentence case: the record writes "How we know", not "How We Know". */
      const label = found[1]!
      return { label: label.charAt(0).toUpperCase() + label.slice(1), text: paragraph.slice(found[0].length) }
    }
    return { label: at === 0 ? 'Mechanism' : 'Detail', text: paragraph }
  })
}

/**
 * How long the reasoning runs, on the fold's own label.
 *
 * **Counted from the same string the blocks are built from**, which is the point of its
 * being here rather than in the template: a count taken from `evidence` while the blocks
 * were built from something else is a label that lies about what opens, and "326 words" is
 * the one number on this page a reader will check against what they then see.
 */
export function reasoningWords(evidence: string | null | undefined): number {
  return (evidence ?? '').split(/\s+/).filter(word => /[\p{L}\p{N}]/u.test(word)).length
}

/**
 * Whether a hunk draws no change at all — every line of it is `unchanged`.
 *
 * **This is a hunk from a file the pull request did not touch**, or from a part of a touched
 * file away from any change: the session read the lines around the finding out of the head
 * checkout and recorded them so the finding has its code, and none of them is an addition or
 * a removal. Drawn like any other hunk and labelled in words, because the alternative — no
 * code at all — is strictly less than the session recorded.
 *
 * `every`, not "no added and no removed": a hunk with no lines cannot exist (the contract
 * requires one) so the two agree, and `every` says the thing the label claims.
 */
export function drawsNoChange(hunk: FindingHunk): boolean {
  return hunk.lines.every(line => line.marker === 'unchanged')
}

/**
 * Why a finding shows no code, in the page's own words.
 *
 * DEV-878's criterion is that a finding with no code says which of three things is true: its
 * file is not in the change, it is about the description, or the session recorded none.
 *
 * **Two of those three are no-code cases, and the third now comes with code.** Since Locus
 * DEV-891 a session records the lines around a finding on a line the diff does not draw as a
 * hunk whose every marker is `unchanged`, so "not part of the change" is a hunk with a label
 * rather than an absence — `drawsNoChange` above is what says so. The branch for it is kept
 * here because a record written before that change has such findings with no hunk at all,
 * and the sentence is right for them.
 *
 * | What the record says | What the page says |
 * |---|---|
 * | no `file` at all | it is about the pull request, not a line of code |
 * | a `file` a part claims with `in_diff: false` | that file is not part of the change |
 * | anything else | the session recorded no code for it |
 *
 * **The first is a fact about the record, not an inference about the lens.** A finding with
 * no file is not about a line; the lens that found it is named in the sentence rather than
 * interpreted, so the page states what it knows and attributes the rest.
 */
export function whyNoCode(finding: Finding, brief: Brief | null | undefined): string {
  if (finding.file == null) {
    return (
      `This finding names no file — the “${finding.lens}” lens found it in the pull `
      + 'request itself rather than in a line of code.'
    )
  }

  const claimed = (brief?.parts ?? [])
    .flatMap(part => part.files)
    .find(file => file.path === finding.file)

  if (claimed?.in_diff === false) {
    return (
      `${finding.file} is not part of this change, so there is no changed line to show. `
      + 'The brief claims the file so the finding is not lost; the review read it and the '
      + 'pull request did not touch it.'
    )
  }

  return (
    'The session recorded no code for this finding. It names '
    + `${finding.file}${finding.line == null ? '' : `, line ${finding.line}`}, and nothing `
    + 'here fetches code.'
  )
}

/** Where a finding is, as the row's second line. */
export function whereOf(finding: Finding): string {
  if (finding.file == null) return 'The pull request itself'
  return finding.line == null ? finding.file : `${finding.file}:${finding.line}`
}

/** Whether a finding's round status means it is still open this round. */
export function isStillOpen(status: RoundStatus | null | undefined): boolean {
  return status == null || status === 'new' || status === 'still_open'
}

/**
 * Whether this finding will actually be posted — the store's `Finding::is_posted`.
 *
 * **Not the same as being in the "To post" list**, and the difference is the one number on
 * this page a reader will act on. A finding carried into this round as `fixed` keeps
 * whatever disposition it was carried with, so it can sit at `in_draft` — the contract says
 * so in as many words, and records the same overcount in `DraftLint.findings_expected` as a
 * known gap. `DraftSection.posted` requires the disposition **and** an open round status, so
 * the composition will not post it.
 *
 * The three lists stay keyed on the disposition, because that is what the developer decided
 * and what `DispositionCounts` serves. The **headline** counts this instead: it is the
 * page's one claim about what the author is going to receive, and a review announcing
 * "2 should change" about a finding that was fixed last round would be wrong about the only
 * thing it says out loud. The row carries its round status beside it, which is what explains
 * the difference to anyone who counts.
 */
export function willPost(finding: Finding): boolean {
  return finding.disposition === 'in_draft' && isStillOpen(finding.round_status)
}

/**
 * Whether this finding was carried from an earlier round.
 *
 * `continues_review_id`, not the round status: a finding raised in this round carries the
 * status `new`, and a finding carried from round 1 and found still open carries `still_open`
 * — but what makes it *carried* is the link, which is also what the contract keys
 * `Finding.late` on for the same reason.
 */
export function isCarried(finding: Finding): boolean {
  return finding.continues_review_id != null
}

/** How the page words a round status. */
const ROUND_STATUS: Record<RoundStatus, string> = {
  new: 'Raised this round',
  fixed: 'Fixed',
  still_open: 'Still open',
  accepted: 'Their reason accepted',
  withdrawn: 'Withdrawn',
  ticketed: 'Moved to a ticket',
}

/**
 * What became of a carried finding this round, in a phrase.
 *
 * Returns null for a finding raised in this round: "Raised this round" beside every row of a
 * first-round review is a column of the same words, and the round group's own heading
 * already says which round it is.
 */
export function roundStatusLabel(finding: Finding): string | null {
  if (!isCarried(finding)) return null
  return ROUND_STATUS[(finding.round_status ?? 'new') as RoundStatus]
}

/** The tally at the foot: "11 to post, 2 for a ticket, 1 set aside". */
export function tally(findings: Finding[]): string {
  const of = (list: FindingList) => findings.filter(finding => listOf(finding) === list).length
  return (
    `${of('post')} to post, ${of('ticket')} for a ticket, ${of('aside')} set aside`
  )
}

/** The severity counts of a list of findings, in the contract's own shape. */
export function severitiesOf(findings: Finding[]): SeverityCounts {
  const counts: SeverityCounts = { blocker: 0, should: 0, nit: 0, question: 0 }
  for (const finding of findings) counts[finding.severity] += 1
  return counts
}
