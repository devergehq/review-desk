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

import type { DraftEdit, Finding, HeldBackFinding, OutcomeKind, Severity } from '~/types/desk'

import { SEVERITY_ORDER, whereOf } from './findings'
import { SEVERITY_WORDS, count, editVerb } from './format'
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
