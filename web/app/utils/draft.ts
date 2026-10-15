/*
 * Reading a draft. No data access: a draft's markdown and its own records in, facts out.
 *
 * Three jobs, and the second and third arrived with DEV-928. Reading the body is the
 * original one — splitting the mermaid out of it, counting the words a reader reads, and
 * working out which findings an edit touched. The other two are the derivations the aside's
 * two panels draw: the held-back findings as rows, and the developer's edits as one line a
 * finding.
 *
 * **Nothing here reads a component, a route or a resource**, which is what makes
 * `test/draft.spec.ts` a test of the rules rather than of a render — the shape
 * `utils/solution.ts` argues for, for the same reason: a rule worked out inline in a
 * template is a rule nothing can test, and two of these decide what a reader is told about
 * a finding they cannot see the claim of.
 */

import type {
  DraftEdit,
  DraftThread,
  Finding,
  HeldBackFinding,
  OutcomeKind,
  Severity,
  SeverityCounts,
} from '~/types/desk'

import { SEVERITY_ORDER, whereOf } from './findings'
import { SEVERITY_WORDS, count, editVerb, severityTally } from './format'
import { INDEX_HEADING, SEVERITY_LABELS } from './houseStyle'
import { freeDetail } from './outcomes'

export interface MarkdownBlock {
  kind: 'markdown' | 'mermaid'
  content: string
}

/**
 * Split markdown into prose and the mermaid diagrams inside it, so each diagram can be
 * drawn by mermaid and the prose by the markdown renderer.
 *
 * A mermaid fence inside a folded section stays a code block: lifting it out would part
 * the fold's opening tag from its closing one.
 */
export function splitMermaid(markdown: string): MarkdownBlock[] {
  const lines = markdown.split('\n')
  const blocks: MarkdownBlock[] = []
  let prose: string[] = []
  let diagram: string[] | null = null
  let fence: string | null = null
  let foldDepth = 0

  const flushProse = () => {
    if (prose.join('').trim().length > 0) {
      blocks.push({ kind: 'markdown', content: prose.join('\n') })
    }
    prose = []
  }

  const closes = (marker: string) =>
    fence != null && marker.startsWith(fence[0]!) && marker.length >= fence.length

  for (const line of lines) {
    const marker = line.match(/^(\s*)(`{3,}|~{3,})\s*([\w-]*)\s*$/)

    if (diagram) {
      if (marker && closes(marker[2]!)) {
        blocks.push({ kind: 'mermaid', content: diagram.join('\n') })
        diagram = null
        fence = null
      }
      else {
        diagram.push(line)
      }
      continue
    }

    if (fence) {
      prose.push(line)
      if (marker && closes(marker[2]!)) fence = null
      continue
    }

    if (marker) {
      fence = marker[2]!
      if (marker[3] === 'mermaid' && foldDepth === 0) {
        flushProse()
        diagram = []
      }
      else {
        prose.push(line)
      }
      continue
    }

    if (/<details\b/i.test(line)) foldDepth += 1
    if (/<\/details>/i.test(line)) foldDepth = Math.max(0, foldDepth - 1)
    prose.push(line)
  }

  if (diagram) prose.push(...diagram)
  flushProse()
  return blocks
}

/**
 * The words a reader actually reads: prose outside fenced code and folded sections. A
 * review's budget counts these, so a long proof costs nothing.
 */
export function visibleWords(markdown: string): number {
  const prose = markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/<details>[\s\S]*?<\/details>/g, ' ')
    .replace(/[`*_#>[\]()]/g, ' ')
  return prose.split(/\s+/).filter(word => /[A-Za-z0-9]/.test(word)).length
}

/** A body as the rendered tab draws it, and what it folded to draw it that way. */
export interface PreviewBody {
  /** The markdown to render: the body, with its open index folded to one line. */
  body: string
  /** How many findings that index lists, or null where there was no index to fold. */
  indexRows: number | null
}

/**
 * The body with the open index folded, for the rendered tab and for it alone.
 *
 * ## The one thing on this page that is printed twice
 *
 * Every row of the index is a finding drawn in full further down — as its own
 * `### Severity · F3 · …` section when it posts in the body, and as a thread when it posts
 * on a line. Nine rows of severity, place and claim sit above nine blocks that say the
 * same three things at length, and the design pass measured what that costs: the Draft
 * page ran 4,305px at 1440 and 8,555px at 390.
 *
 * ## Folded, and not removed
 *
 * **An earlier version of this took the index out of the preview** and said so in a line
 * naming the tab that still had it. Patrick ruled against it on 16 October 2026: *"we
 * shouldn't be signing off on something that isn't what we're posting."* The index is
 * worth having on GitHub and it keeps posting, so the page he signs off on has to hold it.
 *
 * **So the rule this page is built to: it may fold or reorder what posts, and it may never
 * remove any of it.** The whole index is still here, heading and every row, inside a fold
 * one click opens — which is what a reader does when they want the table and not what they
 * have to undo to be sure nothing was taken.
 *
 * **The fold is markdown, not a control of the app's.** It is written into the body as the
 * `<details>` GitHub itself renders, so `MarkdownView` draws it with the same border and
 * the same summary as a fold a session wrote by hand. A preview whose job is fidelity
 * should fold the way the thing it previews folds.
 *
 * ## What it folds, and what it refuses to touch
 *
 * The heading `INDEX_HEADING`, and **the contiguous markdown table under it**. Nothing
 * else. Not the prose some sessions write under the same heading, not the `### Resolved in
 * this PR` table, which is a different claim about different findings, and not a body that
 * carries no index at all — review 1 of the design loop's fixture is exactly that, having
 * been written before the house style grew one, and it comes back byte-identical.
 *
 * A heading with no table under it is left standing: there is no duplication under it to
 * fold, and folding a sentence away would be the page hiding something that is said once.
 */
export function foldOpenIndex(body: string): PreviewBody {
  const lines = body.split('\n')
  const heading = lines.findIndex(line => line.trim() === INDEX_HEADING)
  if (heading === -1) return { body, indexRows: null }

  /* Past the blank line markdown needs between a heading and the table under it. */
  let start = heading + 1
  while (start < lines.length && lines[start]!.trim() === '') start += 1

  let end = start
  while (end < lines.length && lines[end]!.trim().startsWith('|')) end += 1

  /* No table under it, so there is no duplication here to fold. */
  if (end === start) return { body, indexRows: null }

  /*
   * Two of those lines are the header and the rule under it, and the rest are findings.
   * `Math.max` rather than a subtraction taken on trust: a malformed table of one line is
   * still a table this should fold, and a negative count would print as one.
   */
  const indexRows = Math.max(0, end - start - 2)

  /*
   * **The blank line after `</summary>` is what makes the table a table.** markdown-it
   * reads `<details>` as a raw HTML block and ends that block at the first blank line, so
   * the heading and the rows after it are parsed as markdown and the closing tag as HTML
   * of its own. Without it the index would render as the literal pipes of its own source.
   * This is GitHub's own idiom for a fold with markdown inside it.
   */
  const folded = [
    ...lines.slice(0, heading),
    '',
    '<details>',
    `<summary>${indexSummary(lines.slice(start, end), indexRows)}</summary>`,
    '',
    ...lines.slice(heading, end),
    '',
    '</details>',
    '',
    ...lines.slice(end),
  ]
  return { body: folded.join('\n').replace(/\n{3,}/g, '\n\n').trim(), indexRows }
}

/**
 * The one line the fold carries while it is closed: what it is, how much of it, and how
 * serious.
 *
 * **The severities are read off the rows and not off the review's findings.** The findings
 * are a second source for the same fact and the two can disagree — a developer may have
 * edited the body, and what this folds is the body. This module's contract is a draft's
 * markdown in and facts out, which is the same reason it reads the count from the table
 * rather than asking the page for one.
 *
 * **The tally is shown only when every row was graded.** A row whose first cell carries no
 * severity word is a row the house style did not compose, and a breakdown that added up to
 * less than the count printed beside it would read as a miscount rather than as a gap.
 * `severityTally` writes the words, so this and the queue cannot disagree about whether
 * three shoulds are "3 should".
 */
function indexSummary(rows: string[], findings: number): string {
  const counts: SeverityCounts = { blocker: 0, should: 0, nit: 0, question: 0 }
  let graded = 0

  for (const row of rows) {
    /* The row's first cell, which is where the house style puts the glyph and the word. */
    const first = row.split('|')[1] ?? ''
    const severity = (Object.keys(SEVERITY_LABELS) as Severity[])
      .find(key => new RegExp(`\\b${SEVERITY_LABELS[key]}\\b`).test(first))
    if (severity == null) continue
    counts[severity] += 1
    graded += 1
  }

  const parts = ['Index', count(findings, 'finding')]
  if (graded > 0 && graded === findings) parts.push(severityTally(counts))
  return parts.join(' · ')
}

/** The part of the body that raises one finding, from its heading to the next. */
function sectionFor(body: string, key: string): string | null {
  const lines = body.split('\n')
  const start = lines.findIndex(
    line => /^#{2,4}\s/.test(line) && new RegExp(`\\b${key}\\b`).test(line),
  )
  if (start === -1) return null
  const rest = lines.slice(start + 1)
  const end = rest.findIndex(line => /^#{2,4}\s/.test(line))
  return [lines[start], ...(end === -1 ? rest : rest.slice(0, end))].join('\n').trim()
}

export interface DetectedEdit {
  finding_seq: number
  finding_key: string
  kind: OutcomeKind
  summary: string
}

/**
 * A finding, as far as reading a draft body is concerned: a number and the key a person
 * quotes it by.
 *
 * Narrower than `Finding` on purpose. A `Finding[]` satisfies it, so a caller that has the
 * whole review can pass it — and [`findingsInBody`] can produce one from the body alone,
 * which is what lets the draft screen record its edits without reading the review as well.
 * The contract's shape is one GET per screen, and the draft screen's GET does not carry
 * the findings.
 */
export interface FindingRef {
  seq: number
  key: string
}

/**
 * The findings a draft body raises, read out of its own headings.
 *
 * Every finding a body can have been edited *away from* has a section in the body as it
 * was, and a section is a heading carrying the finding's key — `### Blocker · F1 · …`.
 * So the keys in the old body are exactly the findings an edit could be about, and nothing
 * else needs to be read to record one.
 *
 * `F12` and `F1` are told apart: the number is the whole run of digits, so a body that
 * raises both does not report two edits against the first.
 */
export function findingsInBody(body: string): FindingRef[] {
  const found = new Map<number, FindingRef>()
  for (const line of body.split('\n')) {
    if (!/^#{2,4}\s/.test(line)) continue
    for (const match of line.matchAll(/\bF(\d+)\b/g)) {
      const seq = Number(match[1])
      if (seq >= 1) found.set(seq, { seq, key: `F${seq}` })
    }
  }
  return [...found.values()].sort((a, b) => a.seq - b.seq)
}

/**
 * What changed between two versions of a draft body, finding by finding.
 *
 * The API records an edit against the finding it is about, so a saved body has to say
 * which findings it touched. A section that has gone is a cut; a section whose words
 * changed is a rewrite. This is the signal the harvester weighs most heavily, so it is
 * worth getting from the text rather than asking the developer to label it.
 */
export function detectEdits(
  before: string,
  after: string,
  findings: FindingRef[],
): DetectedEdit[] {
  const edits: DetectedEdit[] = []
  for (const finding of findings) {
    const was = sectionFor(before, finding.key)
    const now = sectionFor(after, finding.key)
    if (was && !now) {
      edits.push({
        finding_seq: finding.seq,
        finding_key: finding.key,
        kind: 'cut',
        summary: 'cut from the body',
      })
    }
    else if (was && now && was !== now) {
      edits.push({
        finding_seq: finding.seq,
        finding_key: finding.key,
        kind: 'rewritten',
        summary: was.split('\n')[0] === now.split('\n')[0] ? 'wording' : 'heading and wording',
      })
    }
  }
  return edits
}

/* ========================= The held-back findings, as rows ========================= */

/**
 * How many words of a claim stand in for a title the session never wrote.
 *
 * Twelve, which is DEV-928's own figure and the length DEV-878 set for a title: "the one
 * line a row of the code review page carries". A row carrying 41 to 63 words is the defect
 * this whole panel is about, so the stand-in is held to what a real title would have been.
 */
export const TITLE_WORDS = 12

/**
 * One held-back finding, as its row draws it.
 *
 * **The title, the file and the line are not in `HeldBackFinding`.** The contract gives this
 * panel a sequence number, a key, a claim and a severity, and DEV-928 changes no record and
 * no contract — so the three fields a row needs come from the review's own findings, joined
 * by sequence number. The draft page already reads the review, for the frame and the trail,
 * which is why this is a join and not a new field.
 */
export interface HeldBackRow {
  seq: number
  key: string
  /**
   * The severity as the record holds it, or null when neither the draft nor the review
   * names one — `HeldBackFinding.severity` is optional in the contract, and a row that
   * printed `undefined` for it would be worse than one that says nothing.
   */
  severity: Severity | null
  /** The one line the row carries while it is closed. */
  line: string
  /**
   * True when `line` is the claim's first [`TITLE_WORDS`] words rather than a title of the
   * finding's own. The row says so: a reader has to know that the line they are reading was
   * cut by the page and is not what the review wrote.
   */
  shortened: boolean
  /**
   * The claim, drawn when the row opens — and **null when `line` is already the whole
   * claim**, which is a finding with no title whose claim is short enough to print entire.
   * Drawing it twice, once as the line and again under it, is the duplication this panel
   * was replacing.
   */
  claim: string | null
  /** Where it is in the code, as `path:line` or just `path`. Null where the record names none. */
  where: string | null
}

/**
 * A claim cut to the length of a title, and whether cutting it did anything.
 *
 * **Cut here rather than in CSS**, which is the opposite of the Solution page's one line and
 * is the right way round for this one: there, a column's width decides what fits and a
 * character count could not know it; here the issue gives a figure in words — "the first 12
 * words of its claim" — and a reader is told the figure. An ellipsis is appended so the cut
 * is visible rather than silent.
 *
 * `shortened` is false for a claim already inside the budget. Nothing was shortened, so
 * nothing says it was.
 */
export function shortenClaim(claim: string): { text: string, shortened: boolean } {
  const words = claim.trim().split(/\s+/).filter(Boolean)
  if (words.length <= TITLE_WORDS) return { text: words.join(' '), shortened: false }
  return { text: `${words.slice(0, TITLE_WORDS).join(' ')}…`, shortened: true }
}

/**
 * Where a finding is, as one string. `null` for a finding about the description.
 *
 * **`whereOf` does the composing**, which is the code review page's own answer to the same
 * question — found by review, which pointed out that this had written `path:line` a second
 * time. The difference is only what to do with a finding that names no file: that page says
 * "The pull request itself" in a row that always has a second line, and this panel draws
 * nothing and lets the caller say so, because a row that is not open has no room for a
 * sentence. So the absence is decided here and the composing is not.
 */
export function whereInCode(finding: Finding | undefined): string | null {
  if (!finding?.file) return null
  return whereOf(finding)
}

/**
 * The held-back findings as the rows the panel draws, in the record's own order.
 *
 * **A held-back entry the review's findings do not account for still draws a row.** It falls
 * back to its own claim, with no title, no file and no line — which is a row that says less
 * rather than a row that is missing. The two reads are separate requests and the draft's can
 * be the fresher of them, so a finding held back a second ago is exactly the case this
 * covers.
 */
export function heldBackRows(held: HeldBackFinding[], findings: Finding[]): HeldBackRow[] {
  const bySeq = new Map(findings.map(finding => [finding.seq, finding]))
  return held.map((item) => {
    const finding = bySeq.get(item.finding_seq)
    const title = finding?.title?.trim()
    const stand = shortenClaim(item.claim)
    return {
      seq: item.finding_seq,
      key: item.finding_key,
      severity: item.severity ?? finding?.severity ?? null,
      line: title || stand.text,
      shortened: !title && stand.shortened,
      /*
       * The claim is drawn on opening unless the line already is the claim, whole. With a
       * title there is always more to read; without one, only a claim that was cut has
       * anything left to show.
       */
      claim: title || stand.shortened ? item.claim : null,
      where: whereInCode(finding),
    }
  })
}

/** One severity's rows, with the count the group's heading carries. */
export interface HeldBackGroup {
  severity: Severity | null
  /** "6 nits", "2 questions" — the severity as a word, counted. */
  heading: string
  rows: HeldBackRow[]
}

/**
 * The rows in severity groups, most serious first, each group counted.
 *
 * **`SEVERITY_ORDER` is imported rather than restated**, so this panel and the code review
 * page cannot disagree about whether a question is more serious than a nit.
 *
 * A group with no rows is absent rather than present and empty, which is what lets the
 * template draw its heading without asking a second question — `partGroups` on the Solution
 * page does the same, for the same reason.
 *
 * **Rows a severity cannot be read off go last, under a count of findings.** The contract
 * permits a held-back entry with no severity and the review may not hold the finding either;
 * "3 findings" is what is true about them, and inventing a severity for the sake of a
 * heading would be the page claiming something the record does not say.
 */
export function heldBackGroups(rows: HeldBackRow[]): HeldBackGroup[] {
  /*
   * Read off `SEVERITY_ORDER` rather than written out again. A literal list here would be a
   * second opinion about the order, and the one thing this import is for is that there is
   * not one. `null` is appended, not ranked: it is the absence of a severity, not a fifth.
   */
  const ranked = (Object.keys(SEVERITY_ORDER) as Severity[])
    .sort((one, two) => SEVERITY_ORDER[one] - SEVERITY_ORDER[two])
  const order: Array<Severity | null> = [...ranked, null]
  return order
    .map((severity) => {
      const mine = rows.filter(row => row.severity === severity)
      const words = severity == null ? ['finding', 'findings'] : SEVERITY_WORDS[severity]
      return {
        severity,
        heading: count(mine.length, words[0]!, words[1]!),
        rows: mine,
      }
    })
    .filter(group => group.rows.length > 0)
}

/* ====================== The developer's edits, as one line each ===================== */

/** One finding's place in "Your edits to this draft". */
export interface EditLine {
  seq: number
  key: string
  /** The verb of the latest decision — "Kept", "Cut", "Regraded". Drawn once. */
  verb: string
  /** What the latest edit said beyond its verb, or null when it said only the verb again. */
  detail: string | null
  /** "changed 5 times", or null for a finding edited once. */
  changed: string | null
}

/**
 * What an edit said beyond its own verb.
 *
 * **This is the "Kept F1 · kept" defect, in one function.** `summary` is required by the
 * contract and `Outcome.detail` is not, so the server writes the kind into it when nothing
 * else was recorded — `wire::draft_edit` says so in as many words. The screen then printed
 * the kind as a verb and the same word again as the summary, four times over on the review
 * this was measured against.
 *
 * Two rules, and the second is the narrower of the two readings of "the verb is printed
 * once":
 *
 * 1. A summary that is only the kind, or only the verb, says nothing. Dropped.
 * 2. A summary that **opens** with the kind keeps the rest, so "cut from the body" reads
 *    "Cut F6 · from the body" rather than "Cut F6 · cut from the body", which is the same
 *    defect in a longer coat.
 *
 *    **That one string is written by `detectEdits`, forty lines up this file** — so for
 *    `cut` this is a shim for wording this module chose, not a rule about the server. The
 *    deeper fix is to write "from the body" there; it is not taken here because every
 *    outcome already stored carries the old wording and would still have to be read, so the
 *    shim narrows rather than disappears, and narrowing it is worth its own issue. Raised
 *    by review, which was right that the comment read as though the duplication arrived
 *    from somewhere else.
 *
 * Anything else is kept as the record wrote it. "Blocker to Should" is not a repetition of
 * "Regraded" and trimming real detail to satisfy a rule about repetition would lose what the
 * harvester is given this panel to show.
 */
export function editDetail(kind: OutcomeKind | string, summary: string): string | null {
  /*
   * **And a summary that is a stored record is not shown at all.** `DraftEdit.summary` is
   * `Outcome.detail` verbatim — `wire::draft_edit` copies it and falls back to the kind
   * when there is none — so this panel is a third place an outcome's detail reaches a
   * screen, after the queue's row and the code review page's "What came back". It is the
   * least likely of the three to meet one, because a developer's own decision carries
   * prose; it is reachable all the same through
   * `outcome add --kind cut --detail '{"…":…}'`, and DEV-958's criterion is that no page
   * prints a stored detail as JSON rather than that no page the harvester writes to does.
   * `freeDetail` is the one rule, in the one place that decides it.
   */
  if (freeDetail(summary) == null && summary.trim() !== '') return null

  const plain = (text: string) => text.trim().toLowerCase().replace(/[^a-z0-9 ]+/g, '')
  const said = plain(summary)
  const word = plain(String(kind))
  const verb = plain(editVerb(kind))
  if (said === '' || said === word || said === verb) return null
  const rest = said.startsWith(`${word} `) ? summary.trim().slice(word.length).trim() : summary.trim()
  return rest === '' ? null : rest
}

/**
 * The developer's edits as one line a finding: the latest decision, and how many there were.
 *
 * **Every toggle was its own line**, and on the review DEV-928 was raised from that meant
 * `kept`, `kept`, `kept`, `cut`, `kept` — five lines about F1, all within twelve seconds,
 * four of them reading "Kept F1 · kept". The record keeps every edit and still does; only
 * this list changes. Which is the right division: the harvester weighs the whole series, and
 * a developer reading the aside wants to know where each finding ended up.
 *
 * **The latest, by when it happened and then by the order the server served it.**
 * `occurred_at` is optional in the contract, so a series with none falls back to the array's
 * own order — which is `ORDER BY finding_seq, occurred_at, id` in the store, oldest last
 * within a finding.
 *
 * **Newest first**, because this panel is a record of what the developer has just been
 * doing. The server's own order is by sequence number, which would put a finding decided
 * ten minutes ago above the one decided a second ago.
 */
export function editLines(edits: DraftEdit[]): EditLine[] {
  const byFinding = new Map<number, DraftEdit[]>()
  for (const edit of edits) {
    const mine = byFinding.get(edit.finding_seq) ?? []
    mine.push(edit)
    byFinding.set(edit.finding_seq, mine)
  }

  const at = (edit: DraftEdit) => (edit.occurred_at == null ? null : Date.parse(edit.occurred_at))

  const lines = [...byFinding.values()].map((mine) => {
    const latest = mine.reduce((kept, edit) => {
      const one = at(edit)
      const two = at(kept)
      if (one == null || two == null || Number.isNaN(one) || Number.isNaN(two)) return edit
      return one >= two ? edit : kept
    })
    return {
      seq: latest.finding_seq,
      key: latest.finding_key,
      verb: editVerb(latest.kind),
      detail: editDetail(latest.kind, latest.summary),
      changed: mine.length > 1 ? `changed ${mine.length} times` : null,
      /* Not part of the line; the sort below is the only thing that reads it. */
      when: at(latest),
    }
  })

  return lines
    .sort((one, two) => (two.when ?? 0) - (one.when ?? 0) || two.seq - one.seq)
    .map(({ when: _when, ...line }) => line)
}

/* ===================== The body as blocks, and the outline over it ================== */

/** One block of a body: everything from one heading to the next. */
export interface BodyBlock {
  /**
   * Where the preview draws it, and what an outline row scrolls to. `body-F3` for a
   * finding's own section, `body-1`, `body-2` … for everything else, counted in order.
   */
  anchor: string
  /** The finding this block raises, or null for prose. */
  key: string | null
  /** The heading's own words, less the hashes. Null for the block before the first one. */
  heading: string | null
  /** The markdown, exactly as the body holds it, heading included. */
  markdown: string
}

/**
 * The body cut at its own headings, so that each part of it can be reached.
 *
 * ## Why the body has to be in pieces at all
 *
 * Step 1.3 of the design loop puts an outline down the left of the draft, and an outline
 * whose rows do not go anywhere is a table of contents for a document with no page numbers.
 * The rendered tab was one `MarkdownView` over the whole body, which is one element and one
 * anchor — so the page could say "nine findings post in the body" and not reach any of
 * them.
 *
 * **Nothing is added, removed or reordered.** `blocks.map(block => block.markdown)` joined
 * by a newline is the body, character for character, which `test/draft.spec.ts` asserts
 * directly rather than by sampling. The same rule as the index's fold, for the same reason:
 * this is the page a draft is signed off on.
 *
 * ## Where it cuts, and where it refuses to
 *
 * A line that opens a heading at **top level**: `##`, `###` or `####` outside every fenced
 * block and every `<details>`. The two exclusions are the ones that bite — a session's
 * proof is a fenced block and may hold a line of markdown that looks like a heading, and
 * the house style's "How this review was made" is a fold that carries headings of its own.
 * `splitMermaid` above tracks both for the same reason; this is the third reader of that
 * rule and the first one to cut on it.
 */
export function bodyBlocks(body: string): BodyBlock[] {
  const lines = body.split('\n')
  const starts: number[] = []
  let fence: string | null = null
  let folds = 0

  lines.forEach((line, at) => {
    const marker = line.match(/^(\s*)(`{3,}|~{3,})\s*([\w-]*)\s*$/)
    if (marker) {
      const found = marker[2]!
      if (fence == null) fence = found
      else if (found.startsWith(fence[0]!) && found.length >= fence.length) fence = null
      return
    }
    if (fence != null) return
    if (/<details\b/i.test(line)) folds += 1
    if (/<\/details>/i.test(line)) folds = Math.max(0, folds - 1)
    if (folds === 0 && /^#{2,4}\s/.test(line)) starts.push(at)
  })

  /* The run before the first heading is a block of its own: the verdict line and Method. */
  const cuts = starts[0] === 0 ? starts : [0, ...starts]
  let prose = 0

  return cuts.map((start, at) => {
    const end = cuts[at + 1] ?? lines.length
    const first = lines[start]!
    const heading = /^#{2,4}\s/.test(first) ? first.replace(/^#{2,4}\s+/, '').trim() : null
    const key = heading == null ? null : (/\bF(\d+)\b/.exec(heading)?.[0] ?? null)
    if (key == null) prose += 1
    return {
      anchor: key == null ? `body-${prose}` : `body-${key}`,
      key,
      heading,
      markdown: lines.slice(start, end).join('\n'),
    }
  })
}

/** One line of the outline: what posts, where it lands, and what reaches it. */
export interface OutlineRow {
  anchor: string
  /** The finding's number, or null for a block of prose. */
  key: string | null
  severity: Severity | null
  /** What the row reads: a finding's title, or a heading. */
  label: string
  /** Which half of the post this is. Null for the prose the body opens with. */
  lands: 'in the body' | 'on the code' | null
}

/**
 * The outline: everything this draft posts, in the order it posts it.
 *
 * **It is the index, as navigation.** Patrick, 16 October 2026: the index becomes the
 * outline pane of the three-pane draft, "so it is navigation there rather than a
 * duplicate". It carries more than the index does — the body's prose blocks are in it too,
 * because `### Problem fit` is a place a reader wants to get back to and the index never
 * listed it — and it is built from the body and the threads rather than from the index's
 * own table, so a body whose index was never written still has one.
 *
 * **The severity and the title are read off the heading the house style composed.** The
 * same decision as the fold's tally: the body is what posts, a developer may have rewritten
 * a heading, and this is a view of the body rather than of the record beside it. A heading
 * the style did not compose keeps its own words and takes no severity.
 */
export function outlineRows(blocks: BodyBlock[], threads: DraftThread[]): OutlineRow[] {
  const composed = /^(Blocker|Should|Nit|Question)\s+·\s+F\d+\s+·\s+(.+)$/

  const body = blocks.map((block) => {
    const found = block.heading == null ? null : composed.exec(block.heading)
    return {
      anchor: block.anchor,
      key: block.key,
      severity: found == null
        ? null
        : (found[1]!.toLowerCase() as Severity),
      label: found?.[2] ?? block.heading ?? 'The verdict, and how the review was made',
      lands: block.key == null ? null : ('in the body' as const),
    }
  })

  return [
    ...body,
    ...threads.map(thread => ({
      anchor: `thread-${thread.finding_key}`,
      key: thread.finding_key,
      severity: thread.severity,
      label: thread.title,
      lands: 'on the code' as const,
    })),
  ]
}
