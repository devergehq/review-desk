import { describe, expect, it } from 'vitest'
import {
  TITLE_WORDS,
  detectEdits,
  editDetail,
  editLines,
  heldBackGroups,
  heldBackRows,
  shortenClaim,
  splitMermaid,
  visibleWords,
  whereInCode,
  foldOpenIndex,
} from '../app/utils/draft'
import { INDEX_HEADING } from '../app/utils/houseStyle'
import type { DraftEdit, Finding, HeldBackFinding } from '../app/types/desk'

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

/**
 * One finding, with whatever this test needs set on it.
 *
 * **The file had two of these for a while**, and review pointed out that they disagreed on
 * every default that matters — severity, file, disposition — so a reader of a failing
 * assertion had to work out which one it came from. Declared here, above its first use, and
 * used by both halves of the file.
 */
function raised(seq: number, extra: Partial<Finding> = {}): Finding {
  return {
    review_id: 41,
    seq,
    key: `F${seq}`,
    lens: 'correctness',
    severity: 'nit',
    file: 'app/Reporting/ExportContext.php',
    line: 44,
    claim: 'something is worth saying about this',
    disposition: 'held_back',
    created_at: '2026-10-11T00:00:00Z',
    ...extra,
  } as Finding
}

const FINDINGS = [raised(1, { key: 'F1', severity: 'should', file: null, line: null }), raised(2, { key: 'F2', severity: 'should', file: null, line: null })]

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

/* ========================= The held-back findings, as rows ========================= */

/*
 * **The rules the aside's two panels are built out of, checked without a render.**
 *
 * DEV-928's first and third changes are almost entirely these: which line a row carries and
 * whether it says it was cut, which group it is announced under, and which of a finding's
 * five edits the one line about it reports. `test/screens.nuxt.spec.ts` mounts the page
 * against the fixtures; this is the arithmetic on its own.
 */

/** A claim of a given number of words, so a length is a length rather than a guess. */
function words(n: number): string {
  return Array.from({ length: n }, (_, at) => `word${at + 1}`).join(' ')
}

function held(seq: number, extra: Partial<HeldBackFinding> = {}): HeldBackFinding {
  return {
    finding_seq: seq,
    finding_key: `F${seq}`,
    claim: 'something is worth saying about this',
    severity: 'nit',
    ...extra,
  } as HeldBackFinding
}

describe('a claim standing in for a title the session never wrote', () => {
  it('is cut to twelve words, with the cut shown', () => {
    const { text, shortened } = shortenClaim(words(40))
    expect(text.split(' ')).toHaveLength(TITLE_WORDS)
    expect(text.endsWith('…')).toBe(true)
    expect(shortened).toBe(true)
  })

  it('is twelve words, which is the figure the issue gives', () => {
    expect(TITLE_WORDS).toBe(12)
  })

  /* Nothing was shortened, so nothing says it was. */
  it('says nothing was cut when the claim already fits', () => {
    expect(shortenClaim(words(12))).toEqual({ text: words(12), shortened: false })
    expect(shortenClaim('four words long enough').shortened).toBe(false)
  })

  it('reads a claim whose spacing is untidy as the words it holds', () => {
    expect(shortenClaim('  two   words  ')).toEqual({ text: 'two words', shortened: false })
  })
})

describe('where a held-back finding is, as one string', () => {
  it('is the path and the line', () => {
    expect(whereInCode(raised(1))).toBe('app/Reporting/ExportContext.php:44')
  })

  it('is the path alone when the record holds no line', () => {
    expect(whereInCode(raised(1, { line: null }))).toBe('app/Reporting/ExportContext.php')
  })

  /* A finding about the description names no file, and the row says nothing rather than ":". */
  it('is nothing for a finding about the description', () => {
    expect(whereInCode(raised(1, { file: null, line: null }))).toBeNull()
    expect(whereInCode(undefined)).toBeNull()
  })
})

describe('the held-back findings as rows', () => {
  it('carries the finding’s own title, and the claim only behind it', () => {
    const rows = heldBackRows([held(2)], [raised(2, { title: 'The accessor is still public' })])
    expect(rows[0]!.line).toBe('The accessor is still public')
    expect(rows[0]!.shortened).toBe(false)
    expect(rows[0]!.claim).toBe('something is worth saying about this')
  })

  /*
   * The join is what makes the title, the file and the line available at all: the contract's
   * `HeldBackFinding` holds none of the three, and DEV-928 changes no contract.
   */
  it('joins the review’s findings by sequence number for the title and the code', () => {
    const rows = heldBackRows(
      [held(7)],
      [raised(3), raised(7, { title: 'A title', file: 'app/Money.php', line: 71 })],
    )
    expect(rows[0]!.line).toBe('A title')
    expect(rows[0]!.where).toBe('app/Money.php:71')
  })

  it('falls back to twelve words of the claim, and says so, where no title was written', () => {
    const rows = heldBackRows([held(4, { claim: words(63) })], [raised(4, { title: null })])
    expect(rows[0]!.line).toBe(`${words(12)}…`)
    expect(rows[0]!.shortened).toBe(true)
    /* And the whole claim is still there to be read on opening. */
    expect(rows[0]!.claim).toBe(words(63))
  })

  it('treats a blank title as no title', () => {
    const rows = heldBackRows([held(4, { claim: words(63) })], [raised(4, { title: '   ' })])
    expect(rows[0]!.shortened).toBe(true)
  })

  /*
   * A short claim with no title is drawn once. Printing it as the line and again under the
   * row is the duplication this panel replaced.
   */
  it('does not draw a short claim twice when it is already the whole line', () => {
    const rows = heldBackRows([held(5, { claim: 'Unused import left in the job class' })], [])
    expect(rows[0]!.line).toBe('Unused import left in the job class')
    expect(rows[0]!.claim).toBeNull()
  })

  it('still draws a row for an entry the review’s findings do not account for', () => {
    const rows = heldBackRows([held(9, { claim: words(20), severity: 'question' })], [])
    expect(rows[0]!.key).toBe('F9')
    expect(rows[0]!.severity).toBe('question')
    expect(rows[0]!.where).toBeNull()
    expect(rows[0]!.shortened).toBe(true)
  })

  /* `severity` is optional in the contract, and a row must not print `undefined` for it. */
  it('reads the severity off the finding when the draft’s entry has none', () => {
    const entry = { finding_seq: 6, finding_key: 'F6', claim: 'a claim' } as HeldBackFinding
    expect(heldBackRows([entry], [raised(6, { severity: 'blocker' })])[0]!.severity)
      .toBe('blocker')
  })

  it('says nothing about a severity neither record names', () => {
    const entry = { finding_seq: 6, finding_key: 'F6', claim: 'a claim' } as HeldBackFinding
    expect(heldBackRows([entry], [])[0]!.severity).toBeNull()
  })

  it('keeps the record’s own order', () => {
    const rows = heldBackRows([held(7), held(2), held(5)], [])
    expect(rows.map(row => row.key)).toEqual(['F7', 'F2', 'F5'])
  })
})

describe('the rows in severity groups', () => {
  const rows = heldBackRows(
    [
      held(1, { severity: 'nit' }),
      held(2, { severity: 'question' }),
      held(3, { severity: 'nit' }),
      held(4, { severity: 'blocker' }),
    ],
    [],
  )

  it('counts each group in the severity’s own word', () => {
    expect(heldBackGroups(rows).map(group => group.heading))
      .toEqual(['1 blocker', '1 question', '2 nits'])
  })

  /* Board A8's order, imported rather than restated: a question is above a nit. */
  it('puts the most serious group first, and a question above a nit', () => {
    expect(heldBackGroups(rows).map(group => group.severity))
      .toEqual(['blocker', 'question', 'nit'])
  })

  it('leaves out a severity nothing is held back under', () => {
    expect(heldBackGroups(heldBackRows([held(1)], [])).map(group => group.severity))
      .toEqual(['nit'])
  })

  /* "should" has no plural, as the review's own tally has it. */
  it('does not pluralise "should"', () => {
    const two = heldBackRows([held(1, { severity: 'should' }), held(2, { severity: 'should' })], [])
    expect(heldBackGroups(two)[0]!.heading).toBe('2 should')
  })

  /*
   * A severity the record does not name is an absence, not a fifth severity: the group goes
   * last and is counted in findings, because that is what is true about it.
   */
  it('puts rows with no severity last, counted as findings', () => {
    const mixed = heldBackRows(
      [{ finding_seq: 9, finding_key: 'F9', claim: 'a claim' } as HeldBackFinding, held(1)],
      [],
    )
    const groups = heldBackGroups(mixed)
    expect(groups.map(group => group.heading)).toEqual(['1 nit', '1 finding'])
    expect(groups[1]!.severity).toBeNull()
  })

  it('accounts for every row exactly once', () => {
    const counted = heldBackGroups(rows).reduce((total, group) => total + group.rows.length, 0)
    expect(counted).toBe(rows.length)
  })
})

/* ====================== The developer's edits, as one line each ===================== */

function edit(extra: Partial<DraftEdit> = {}): DraftEdit {
  return {
    finding_seq: 1,
    finding_key: 'F1',
    kind: 'kept',
    summary: 'kept',
    occurred_at: '2026-10-11T09:00:00Z',
    ...extra,
  } as DraftEdit
}

describe('what an edit said beyond its own verb', () => {
  /* The defect itself: the server writes the kind into `summary` when nothing else was. */
  it('is nothing when the summary is only the kind', () => {
    expect(editDetail('kept', 'kept')).toBeNull()
    expect(editDetail('regraded', 'Regraded')).toBeNull()
  })

  it('is nothing when the summary is only the verb the screen prints', () => {
    expect(editDetail('ticketed', 'Moved to a ticket')).toBeNull()
  })

  it('drops a summary’s leading repetition of the kind and keeps the rest', () => {
    expect(editDetail('cut', 'cut from the body')).toBe('from the body')
  })

  it('keeps a summary that says something the verb does not', () => {
    expect(editDetail('regraded', 'Blocker to Should')).toBe('Blocker to Should')
    expect(editDetail('cut', 'rename suggestion')).toBe('rename suggestion')
  })

  it('is nothing for a summary of no words at all', () => {
    expect(editDetail('kept', '   ')).toBeNull()
  })
})

describe('the developer’s edits as one line a finding', () => {
  /*
   * The review DEV-928 was raised from: five edits on F1 inside twelve seconds, four of
   * them printing "Kept F1 · kept".
   */
  const FIVE = [
    edit({ occurred_at: '2026-10-11T09:00:00Z' }),
    edit({ occurred_at: '2026-10-11T09:00:03Z' }),
    edit({ occurred_at: '2026-10-11T09:00:06Z' }),
    edit({ kind: 'cut', summary: 'cut', occurred_at: '2026-10-11T09:00:09Z' }),
    edit({ occurred_at: '2026-10-11T09:00:12Z' }),
  ]

  it('draws one line for a finding toggled five times', () => {
    expect(editLines(FIVE)).toHaveLength(1)
  })

  it('reports the latest decision, and the verb once', () => {
    const [line] = editLines(FIVE)
    expect(line!.verb).toBe('Posted')
    expect(line!.detail).toBeNull()
  })

  it('says how many times it changed', () => {
    expect(editLines(FIVE)[0]!.changed).toBe('changed 5 times')
  })

  it('says nothing about a count for a finding edited once', () => {
    expect(editLines([edit()])[0]!.changed).toBeNull()
  })

  it('keeps one line per finding when several were edited', () => {
    const lines = editLines([
      edit({ finding_seq: 1, finding_key: 'F1', occurred_at: '2026-10-11T09:00:00Z' }),
      edit({ finding_seq: 3, finding_key: 'F3', kind: 'regraded', summary: 'Blocker to Should', occurred_at: '2026-10-11T09:01:00Z' }),
      edit({ finding_seq: 1, finding_key: 'F1', kind: 'cut', summary: 'cut', occurred_at: '2026-10-11T09:02:00Z' }),
    ])
    expect(lines.map(line => line.key)).toEqual(['F1', 'F3'])
    expect(lines[0]).toMatchObject({ verb: 'Set aside', changed: 'changed 2 times' })
    expect(lines[1]).toMatchObject({ verb: 'Regraded', detail: 'Blocker to Should', changed: null })
  })

  /* Newest first: this panel is a record of what the developer has just been doing. */
  it('puts the finding decided most recently first', () => {
    const lines = editLines([
      edit({ finding_seq: 9, finding_key: 'F9', occurred_at: '2026-10-11T08:00:00Z' }),
      edit({ finding_seq: 2, finding_key: 'F2', occurred_at: '2026-10-11T10:00:00Z' }),
    ])
    expect(lines.map(line => line.key)).toEqual(['F2', 'F9'])
  })

  /*
   * `occurred_at` is optional in the contract. With none, the server's own order stands —
   * oldest last within a finding — so the last entry is the latest.
   */
  it('falls back to the order the server served them in', () => {
    const lines = editLines([
      edit({ occurred_at: null }),
      edit({ kind: 'cut', summary: 'cut', occurred_at: null }),
    ])
    expect(lines).toHaveLength(1)
    expect(lines[0]!.verb).toBe('Set aside')
  })

  it('is empty for a draft nobody has edited', () => {
    expect(editLines([])).toEqual([])
  })
})

describe('an edit’s detail is never a stored record', () => {
  /*
   * **The third place an outcome's detail reaches a screen.** `DraftEdit.summary` is
   * `Outcome.detail` verbatim, so this panel prints one — and DEV-958's criterion is that
   * no page prints a stored detail as JSON, not that no page the harvester writes to does.
   * Reachable through `outcome add --kind cut --detail '{"…":…}'`.
   */
  it('shows nothing where the summary is a JSON object', () => {
    expect(editDetail('cut', '{"reason":"formatting","by":"a script"}')).toBeNull()
    expect(editDetail('regraded', '[1,2]')).toBeNull()
  })

  it('still shows the prose a developer typed', () => {
    expect(editDetail('regraded', 'Blocker to Should')).toBe('Blocker to Should')
    expect(editDetail('cut', 'the second sentence')).toBe('the second sentence')
  })
})

/**
 * Every line of the stored body that the drawn body does not carry.
 *
 * **The whole rule this fold is built to, as one list that has to be empty.** The preview
 * may fold what posts and may reorder it; it may not drop any of it, because this is the
 * page the draft is signed off on. Blank lines are not checked: the fold adds and collapses
 * them, and no reader is signing off on a blank line.
 */
function missingFrom(drawn: string, stored: string): string[] {
  return stored.split('\n').filter(line => line.trim() !== '' && !drawn.includes(line))
}

describe('the open index, which the preview folds rather than drops', () => {
  const INDEXED = [
    '**4 open, no blockers.** The shape is right.',
    '',
    '### Problem fit',
    '',
    'Every run on `dev` has failed since the gate went up.',
    '',
    INDEX_HEADING,
    '',
    '| | Where | Finding | Status |',
    '| -- | -- | -- | -- |',
    '| 🟠 Should · F1 | `auth.ts:194` | the guard is in one path | Open · on the code |',
    '| 🟠 Should · F3 | the pull request | nothing tested the real gate | Open · in the body |',
    '| ⚪ Nit · F7 | the pull request | the file list omits one | Open · in the body |',
    '',
    '### Should · F3 · Nothing has tested this against the real gate',
    '',
    '**What** · the branch does not exist on the remote.',
  ].join('\n')

  it('folds the heading and the table into one summary line', () => {
    const { body } = foldOpenIndex(INDEXED)

    expect(body).toContain('<summary>Index · 3 findings · 2 should, 1 nit</summary>')
  })

  /*
   * **The rule, stated as a test.** Patrick, 16 October 2026: "we shouldn't be signing off
   * on something that isn't what we're posting." An earlier version of this filter took the
   * index out and said so in a line; this one may fold it and nothing more, so every line of
   * the body the session wrote is still in what the rendered tab draws.
   */
  it('draws every part of the stored body', () => {
    const { body } = foldOpenIndex(INDEXED)

    expect(missingFrom(body, INDEXED)).toEqual([])
  })

  it('puts the index inside the fold and the rest of the body outside it', () => {
    const { body } = foldOpenIndex(INDEXED)
    const open = body.indexOf('<details>')
    const close = body.indexOf('</details>')

    expect(open).toBeGreaterThan(-1)
    expect(body.indexOf(INDEX_HEADING)).toBeGreaterThan(open)
    expect(body.indexOf('| 🟠 Should · F1 |')).toBeLessThan(close)
    expect(body.indexOf('### Problem fit')).toBeLessThan(open)
    expect(body.indexOf('### Should · F3 ·')).toBeGreaterThan(close)
  })

  /*
   * markdown-it reads `<details>` as a raw HTML block and ends that block at the first
   * blank line. Without one after the summary the index renders as the literal pipes of its
   * own source — which is the fold showing the developer something GitHub will not show.
   */
  it('leaves a blank line after the summary, so the table stays a table', () => {
    const { body } = foldOpenIndex(INDEXED)

    expect(body).toContain(`</summary>\n\n${INDEX_HEADING}`)
  })

  /*
   * The count is what the fold's one line says, so it has to be the number of findings and
   * not the number of lines: the header and the rule under it are not findings.
   */
  it('counts the findings it folded, not the rows of markup', () => {
    expect(foldOpenIndex(INDEXED).indexRows).toBe(3)
  })

  /*
   * A row the house style did not compose carries no severity word, and a breakdown adding
   * up to less than the count beside it reads as a miscount rather than as a gap.
   */
  it('leaves the tally out when a row carries no severity', () => {
    const ungraded = [
      INDEX_HEADING,
      '',
      '| | Where | Finding | Status |',
      '| -- | -- | -- | -- |',
      '| 🟠 Should · F1 | `auth.ts:194` | the guard is in one path | Open · on the code |',
      '| F4 | the pull request | written by hand | Open · in the body |',
    ].join('\n')

    const { body, indexRows } = foldOpenIndex(ungraded)

    expect(indexRows).toBe(2)
    expect(body).toContain('<summary>Index · 2 findings</summary>')
  })

  /*
   * Review 1 of the design loop's fixture: 17 findings, a 19,750-character body, and no
   * index at all, having been written before the house style grew one. A body with nothing
   * to fold must come back as it went in, or the filter is editing drafts rather than
   * de-duplicating a page.
   */
  it('leaves a body with no index exactly as it was', () => {
    const plain = '**Two open.**\n\n### Problem fit\n\nThe shape is right.'
    const { body, indexRows } = foldOpenIndex(plain)

    expect(body).toBe(plain)
    expect(indexRows).toBeNull()
  })

  /*
   * `### Resolved in this PR` is a different claim about different findings — what the
   * author has already fixed — and review 35's body carries both tables. Folding the wrong
   * one would close the only place the resolved findings are listed.
   */
  it('leaves the resolved table outside the fold', () => {
    const both = [
      INDEX_HEADING,
      '',
      '| | Where | Finding | Status |',
      '| -- | -- | -- | -- |',
      '| 🟠 Should · F35 | `Action.php:36` | a stray grant | Open · in the body |',
      '',
      '### Resolved in this PR',
      '',
      '| | Where | Finding | Fixed in |',
      '| -- | -- | -- | -- |',
      '| 🟠 Should · F28 | `Policy.php:155` | the gate threw | 2f1a5f1 |',
    ].join('\n')

    const { body, indexRows } = foldOpenIndex(both)

    expect(indexRows).toBe(1)
    expect(missingFrom(body, both)).toEqual([])
    expect(body.indexOf('| 🟠 Should · F35 |')).toBeLessThan(body.indexOf('</details>'))
    expect(body.indexOf('### Resolved in this PR')).toBeGreaterThan(body.indexOf('</details>'))
  })

  /*
   * A session that wrote prose under the heading rather than a table has said something no
   * other part of the page is saying. Nothing is folded away from it.
   */
  it('leaves the heading standing when there is no table under it', () => {
    const prose = `${INDEX_HEADING}\n\nNothing is open on this round.`

    const { body, indexRows } = foldOpenIndex(prose)

    expect(body).toBe(prose)
    expect(indexRows).toBeNull()
  })

  it('does not weld the paragraph after the fold to the one before it', () => {
    const { body } = foldOpenIndex(INDEXED)

    expect(body).toContain(
      'Every run on `dev` has failed since the gate went up.\n\n<details>',
    )
    expect(body).toContain('</details>\n\n### Should · F3 ·')
  })
})
