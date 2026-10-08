/*
 * Reading a draft body. No data access; a draft's markdown in, facts out.
 */

import type { Finding, OutcomeKind } from '~/types/desk'

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
 * What changed between two versions of a draft body, finding by finding.
 *
 * The API records an edit against the finding it is about, so a saved body has to say
 * which findings it touched. A section that has gone is a cut; a section whose words
 * changed is a rewrite. This is the signal the harvester weighs most heavily, so it is
 * worth getting from the text rather than asking the developer to label it.
 */
export function detectEdits(before: string, after: string, findings: Finding[]): DetectedEdit[] {
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
