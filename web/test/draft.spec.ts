import { describe, expect, it } from 'vitest'
import { detectEdits, splitMermaid, visibleWords } from '../app/utils/draft'
import type { Finding } from '../app/types/desk'

/*
 * Reading a draft body. Two of DEV-855's acceptance criteria rest on this: the preview
 * has to draw a mermaid diagram and keep a folded section folded, and an edit has to turn
 * up in the list of the developer's edits.
 */

const BODY = `Intro prose.

### Blocker · F1 · A thing

Why it matters.

\`\`\`mermaid
flowchart LR
  A --> B
\`\`\`

<details>
<summary>Proof</summary>

\`\`\`php
$x = 1;
\`\`\`

</details>

### Should · F2 · Another thing

More prose.
`

describe('splitting a body into prose and diagrams', () => {
  it('lifts a mermaid fence out as its own block', () => {
    const blocks = splitMermaid(BODY)
    const diagrams = blocks.filter(block => block.kind === 'mermaid')
    expect(diagrams).toHaveLength(1)
    expect(diagrams[0]!.content).toBe('flowchart LR\n  A --> B')
  })

  it('keeps the fold whole, with its opening and closing tags in one block', () => {
    const blocks = splitMermaid(BODY)
    const withFold = blocks.find(block => block.content.includes('<details>'))
    expect(withFold).toBeDefined()
    expect(withFold!.content).toContain('</details>')
  })

  it('leaves a code fence that is not mermaid as markdown', () => {
    const blocks = splitMermaid(BODY)
    expect(blocks.some(block => block.kind === 'markdown' && block.content.includes('```php')))
      .toBe(true)
  })

  it('leaves a mermaid fence inside a fold alone, so the fold is not torn apart', () => {
    const body = `<details>\n<summary>S</summary>\n\n\`\`\`mermaid\nflowchart LR\n  A --> B\n\`\`\`\n\n</details>\n`
    const blocks = splitMermaid(body)
    expect(blocks.filter(block => block.kind === 'mermaid')).toHaveLength(0)
    expect(blocks[0]!.content).toContain('<details>')
    expect(blocks[0]!.content).toContain('</details>')
  })

  it('does not lose a body with no diagram in it', () => {
    const blocks = splitMermaid('Just prose.\n')
    expect(blocks).toHaveLength(1)
    expect(blocks[0]!.content.trim()).toBe('Just prose.')
  })

  it('survives an unclosed fence rather than swallowing the rest', () => {
    const blocks = splitMermaid('Before.\n\n```mermaid\nflowchart LR\n')
    expect(blocks.map(block => block.kind)).not.toContain('mermaid')
    expect(blocks.map(block => block.content).join('\n')).toContain('flowchart LR')
  })
})

describe('the word budget counts what is read', () => {
  it('ignores fenced code and folded proof', () => {
    expect(visibleWords('one two three')).toBe(3)
    expect(visibleWords('one two\n\n```\nnot counted at all here\n```\n')).toBe(2)
    expect(visibleWords('one two\n\n<details>\n<summary>x</summary>\nnope\n</details>\n')).toBe(2)
  })

  it('does not count markdown punctuation as words', () => {
    expect(visibleWords('### A heading')).toBe(2)
    expect(visibleWords('`code` and **bold**')).toBe(3)
  })
})

function finding(seq: number, key: string): Finding {
  return {
    review_id: 1,
    seq,
    key,
    lens: 'test',
    severity: 'should',
    file: null,
    line: null,
    claim: 'a claim',
    evidence: null,
    disposition: 'in_draft',
    disposition_reason: null,
    rule_id: null,
    rule_key: null,
    created_at: '2026-10-08T11:00:00Z',
  }
}

const FINDINGS = [finding(1, 'F1'), finding(2, 'F2')]

describe('what changed between two versions of a draft', () => {
  it('reports nothing when nothing changed', () => {
    expect(detectEdits(BODY, BODY, FINDINGS)).toEqual([])
  })

  it('calls a section that has gone a cut', () => {
    const after = BODY.slice(0, BODY.indexOf('### Should · F2'))
    const edits = detectEdits(BODY, after, FINDINGS)
    expect(edits).toEqual([
      { finding_seq: 2, finding_key: 'F2', kind: 'cut', summary: 'cut from the body' },
    ])
  })

  it('calls changed words a rewrite, against the finding it is about', () => {
    const after = BODY.replace('Why it matters.', 'Why it matters, in different words.')
    const edits = detectEdits(BODY, after, FINDINGS)
    expect(edits).toHaveLength(1)
    expect(edits[0]).toMatchObject({ finding_key: 'F1', kind: 'rewritten', summary: 'wording' })
  })

  it('notices a changed heading as well as changed words', () => {
    const after = BODY.replace('F1 · A thing', 'F1 · A sharper thing')
    const edits = detectEdits(BODY, after, FINDINGS)
    expect(edits[0]).toMatchObject({ finding_key: 'F1', kind: 'rewritten' })
    expect(edits[0]!.summary).toBe('heading and wording')
  })

  it('ignores a change to the intro, which is about no finding', () => {
    const after = BODY.replace('Intro prose.', 'A different intro.')
    expect(detectEdits(BODY, after, FINDINGS)).toEqual([])
  })
})
