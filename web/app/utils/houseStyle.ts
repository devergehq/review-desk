/**
 * What GitHub receives: the posted body's shape, and the threads that go with it.
 *
 * **This is the store's `house_style.rs`, in TypeScript, and it exists for sample mode
 * alone.** The record composes the body and the threads, and the dashboard renders what it
 * is given — but sample mode has no record behind it, so a decision made against the
 * fixtures has to recompose the same way the binary would, or the development loop and the
 * unit tests would be looking at a shape the real thing never produces.
 *
 * **The fixtures are composed by the Rust implementation**, which is what keeps the two in
 * step rather than hoping: `web/test/draft.spec.ts` asserts that composing a fixture's own
 * blocks reproduces the body the fixture carries, and that body came out of the store.
 *
 * The shape itself, and the reason for each part of it, is documented once in
 * `crates/store/src/entities/house_style.rs`. What follows is deliberately a translation
 * and not an improvement on it.
 */
import type { DraftSection, DraftThread, Finding, Severity } from '~/types/desk'

import { SEVERITY_ORDER } from './findings'

/** The word a finding's heading, index row and thread title line are written with. */
export const SEVERITY_LABELS: Record<Severity, string> = {
  blocker: 'Blocker',
  should: 'Should',
  nit: 'Nit',
  question: 'Question',
}

/**
 * The glyph the house style puts before that word, in what posts.
 *
 * `house-style.md`'s own vocabulary table. It is never on its own: the word carries the
 * severity, and the pages grade a finding with `SeverityTag` rather than with this.
 */
export const SEVERITY_GLYPHS: Record<Severity, string> = {
  blocker: '🔴',
  should: '🟠',
  nit: '⚪',
  question: '🔵',
}

/** Which alert a severity picks, and null for the one that takes no rail. */
export const SEVERITY_ALERTS: Record<Severity, string | null> = {
  blocker: 'CAUTION',
  should: 'WARNING',
  question: 'NOTE',
  nit: null,
}

/** The heading the index sits under. */
export const INDEX_HEADING = '### Open — needs a decision'

const INDEX_HEAD = '| | Where | Finding | Status |\n| -- | -- | -- | -- |'

/** The widest a cell may be, and the longest unbroken token in one, from `review_lint.py`. */
const CELL_PROSE = 140
const CELL_TOKEN = 40

/**
 * The line a composed comment ends with, so a session can match it back to its finding.
 *
 * **The path and the line are not a key.** A Should and a Nit on one line is ordinary, and
 * GitHub answers a review call with the review rather than with the comments it made — so a
 * session reading them back and pairing on `(path, line)` would link the wrong comment to
 * the wrong finding, silently. An HTML comment is invisible on GitHub and comes back
 * verbatim in the comment's body.
 */
export function threadMarker(key: string): string {
  return `<!-- review-desk:${key} -->`
}

/** One finding's heading, as the posted body carries it. */
export function findingHeading(severity: Severity, seq: number, title: string): string {
  return `### ${SEVERITY_LABELS[severity]} · F${seq} · ${title}`
}

/** Where the review is, so that a row can link to the code it is about. */
export interface Place {
  repo: string
  pr: number
  head_sha: string
}

/**
 * Where GitHub can hang this finding's comment, or null when there is nowhere.
 *
 * Four facts, all of them already on the record: the finding names a file, it names the
 * line its comment attaches to, it carries a hunk, and that hunk is one the diff drew and
 * draws that line. A hunk whose every entry is `unchanged` came from `git show` and is a
 * line the change does not touch.
 */
export function anchorOf(finding: Finding): { path: string, line: number } | null {
  const line = finding.flagged_line
  const hunk = finding.hunk
  if (finding.file == null || line == null || hunk == null) return null
  if (!hunk.lines.some(entry => entry.marker !== 'unchanged')) return null
  if (!hunk.lines.some(entry => entry.line === line)) return null

  return { path: hunk.file, line }
}

/** Whether a finding's block posts: the store's `Finding::is_posted`, restated. */
export function isPosted(finding: Finding): boolean {
  const status = finding.round_status ?? 'new'
  return finding.disposition === 'in_draft' && (status === 'new' || status === 'still_open')
}

/** Where the finding stands this round, in the words the chip and the row share. */
function standing(finding: Finding): string {
  return (finding.round_status ?? 'new') === 'still_open' ? 'Still open' : 'Open'
}

/** The body these blocks compose to, in the house style's shape. */
export function composeBody(
  place: Place,
  sections: DraftSection[],
  findings: Finding[],
): string {
  const index = composeIndex(place, sections, findings)
  const parts: string[] = []
  let indexed = false

  for (const section of sections) {
    if (!section.posted) continue

    if (section.kind === 'prose') {
      parts.push(section.text)
      continue
    }

    /* The index goes where the findings begin, once. */
    if (!indexed) {
      if (index != null) parts.push(index)
      indexed = true
    }

    const finding = findings.find(one => one.seq === section.finding_seq)
    /* A finding with a line inside the diff is a row and a thread, not a section. */
    if (finding != null && anchorOf(finding) != null) continue

    const composed = composeSection(section)
    if (composed != null) parts.push(composed)
  }

  return parts.length === 0 ? '' : `${parts.join('\n\n')}\n`
}

/** One finding's section: the composed heading, a blank line, then the block's text. */
function composeSection(section: DraftSection): string | null {
  if (section.severity == null || section.finding_seq == null || section.title == null) {
    return null
  }
  const heading = findingHeading(section.severity, section.finding_seq, section.title)
  return section.text.length === 0 ? heading : `${heading}\n\n${section.text}`
}

/** The index: one row per posted finding, most serious first, then by number. */
function composeIndex(
  place: Place,
  sections: DraftSection[],
  findings: Finding[],
): string | null {
  const rows = sections
    .filter(section => section.posted && section.kind === 'finding')
    .flatMap((section) => {
      const finding = findings.find(one => one.seq === section.finding_seq)
      if (finding == null || section.severity == null || section.finding_seq == null) return []
      /*
       * **The title is asked for here as well**, because a row is a claim that the finding
       * posts somewhere and a block with no title posts nowhere: the section and the thread
       * both drop one.
       */
      if (section.title == null) return []
      return [{
        rank: SEVERITY_ORDER[section.severity],
        seq: section.finding_seq,
        row: composeRow(place, section.severity, section.finding_seq, section.title, finding),
      }]
    })
    .sort((one, two) => one.rank - two.rank || one.seq - two.seq)

  if (rows.length === 0) return null
  return `${INDEX_HEADING}\n\n${INDEX_HEAD}\n${rows.map(one => one.row).join('\n')}`
}

/** One index row. */
function composeRow(
  place: Place,
  severity: Severity,
  seq: number,
  title: string,
  finding: Finding,
): string {
  const anchor = anchorOf(finding)
  const where = anchor != null
    ? codeLink(place, anchor.path, anchor.line)
    : finding.file != null
      ? codeLink(place, finding.file, finding.line)
      /*
       * Nothing in the code to point at: a correction to the description, a missing
       * ticket, a wrong figure. The pull request is where such a finding is about.
       */
      : `[the pull request](https://github.com/${place.repo}/pull/${place.pr})`

  return `| ${SEVERITY_GLYPHS[severity]} ${SEVERITY_LABELS[severity]} · F${seq} `
    + `| ${where} | ${cell(title)} `
    + `| ${standing(finding)} · ${anchor != null ? 'on the code' : 'in the body'} |`
}

/** A link to the line at the head commit reviewed, with `basename:line` as its text. */
function codeLink(place: Place, path: string, line: number | null | undefined): string {
  const base = `https://github.com/${place.repo}/blob/${place.head_sha}/${path}`
  const name = path.split('/').pop() ?? path
  const target = line == null ? base : `${base}#L${line}`
  const text = line == null ? name : `${name}:${line}`

  return `[\`${scannable(text)}\`](${linkTarget(target)})`
}

/** A link target markdown can carry: wrapped in `<>` when it holds anything that ends it. */
function linkTarget(target: string): string {
  return /[ ()]/.test(target) ? `<${target}>` : target
}

/** A path short enough for a cell, keeping the end of it. */
function scannable(text: string): string {
  const letters = [...text]
  if (letters.length <= CELL_TOKEN) return text
  return `…${letters.slice(letters.length - (CELL_TOKEN - 1)).join('')}`
}

/** A cell's text: on one line, its pipes escaped, clamped to what GitHub can draw. */
function cell(text: string): string {
  const flat = text.replace(/[\n\r]/g, ' ').replace(/\|/g, '\\|')
  if (shown(flat).length <= CELL_PROSE) return flat

  const kept: string[] = []
  for (const word of flat.split(/\s+/).filter(word => word.length > 0)) {
    /* Two characters for the ellipsis and the space before it. */
    if (shown([...kept, word].join(' ')).length + 2 > CELL_PROSE) break
    kept.push(word)
  }
  while (kept.join(' ').split('`').length % 2 === 0) kept.pop()

  /*
   * **A clamp that kept nothing keeps the start of the text instead.** One word longer than
   * the whole cell leaves the loop above with an empty list, and a row reading `…` says less
   * about the finding than a sentence cut mid-word does.
   */
  if (kept.length === 0) {
    const start = [...flat].slice(0, CELL_PROSE - 2).join('')
    /*
     * **And not ending inside a code span.** A cell with an odd number of backticks has an
     * open span in it, and a table row is split on the pipes *outside* code spans — so one
     * unclosed backtick can swallow the two cells after it.
     */
    const whole = start.split('`').length % 2 === 0
      ? start.slice(0, start.lastIndexOf('`'))
      : start
    return `${whole.trimEnd()} …`
  }

  return `${kept.join(' ')} …`
}

/** What a cell renders as, so the clamp measures what a reader sees. */
function shown(text: string): string {
  return [...text.replace(/[`*_]/g, '')].join('')
}

/** The threads the draft hands over, most serious first, then by number. */
export function composeThreads(
  sections: DraftSection[],
  findings: Finding[],
): DraftThread[] {
  return sections
    .filter(section => section.posted && section.kind === 'finding')
    .flatMap((section) => {
      const finding = findings.find(one => one.seq === section.finding_seq)
      if (finding == null || section.severity == null || section.finding_seq == null) return []
      if (section.title == null) return []
      const anchor = anchorOf(finding)
      if (anchor == null) return []

      return [{
        finding_seq: section.finding_seq,
        finding_key: `F${section.finding_seq}`,
        severity: section.severity,
        title: section.title,
        path: anchor.path,
        line: anchor.line,
        side: 'RIGHT' as const,
        body: threadBody(
          section.severity,
          `F${section.finding_seq}`,
          section.title,
          standing(finding),
          section.text,
        ),
      }]
    })
    .sort((one, two) =>
      SEVERITY_ORDER[one.severity] - SEVERITY_ORDER[two.severity]
      || one.finding_seq - two.finding_seq)
}

/** One thread, as the house style shapes it. */
function threadBody(
  severity: Severity,
  key: string,
  title: string,
  standingNow: string,
  text: string,
): string {
  const titleLine = `**${SEVERITY_LABELS[severity]} · ${key}** — ${title} · \`${standingNow}\``
  const alert = SEVERITY_ALERTS[severity]
  /* Outside the rail, as the house style puts the `agent:` marker: it is not the finding. */
  const marker = threadMarker(key)

  if (alert == null) {
    /* The nit's lighter shape: the glyph does the work the rail does, and no labels. */
    return text.length === 0
      ? `${SEVERITY_GLYPHS[severity]} ${titleLine}\n\n${marker}`
      : `${SEVERITY_GLYPHS[severity]} ${titleLine}\n${text}\n\n${marker}`
  }

  const lines = [`> [!${alert}]`, `> ${titleLine}`]
  if (text.length > 0) {
    lines.push('>')
    lines.push(...text.split('\n').map(line => (line.length === 0 ? '>' : `> ${line}`)))
  }
  lines.push('', marker)
  return lines.join('\n')
}
