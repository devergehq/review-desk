import { describe, expect, it } from 'vitest'

import {
  anchorOf,
  composeBody,
  composeThreads,
  INDEX_HEADING,
} from '../app/utils/houseStyle'

import type { DraftSection, Finding, FindingHunk, Severity } from '../app/types/desk'

/*
 * **The shape GitHub receives, composed in sample mode.**
 *
 * This is `crates/store/src/entities/house_style.rs` in TypeScript — sample mode has no
 * record behind it, so a decision made against the fixtures has to recompose the same way
 * the binary would. The two are held together by the fixtures themselves:
 * `test/example-fixtures.spec.ts` composes each one's own blocks and asserts the body and
 * the threads it serves, and review 41's fixture is the contract example, whose body came
 * out of the store.
 *
 * What is asserted here is the part a fixture cannot reach on its own: the four ways a
 * finding ends up with nowhere to anchor, and the two thread shapes.
 */

/** A hunk as `git diff -U3` drew it: context, a removal, and what replaced it. */
function diffHunk(file: string, start: number): FindingHunk {
  return {
    file,
    start_line: start,
    lines: [
      { marker: 'unchanged', line: start, text: '    public function handle($payload)' },
      { marker: 'unchanged', line: start + 1, text: '    {' },
      { marker: 'removed', line: null, text: '        $client->post($this->url);' },
      { marker: 'added', line: start + 2, text: '        $response = $client->post($this->url);' },
      { marker: 'unchanged', line: start + 3, text: '    }' },
    ],
  }
}

/** A hunk as `git show <head>:<path>` drew it: the code at head, with no change in it. */
function shownHunk(file: string, start: number): FindingHunk {
  return {
    file,
    start_line: start,
    lines: [
      { marker: 'unchanged', line: start, text: '    public function url(): string' },
      { marker: 'unchanged', line: start + 1, text: '        return $this->url;' },
    ],
  }
}

function finding(seq: number, severity: Severity, over: Partial<Finding> = {}): Finding {
  return {
    review_id: 41,
    seq,
    key: `F${seq}`,
    lens: 'correctness',
    severity,
    file: null,
    line: null,
    claim: `what F${seq} says is wrong`,
    title: `what F${seq} says is wrong`,
    disposition: 'in_draft',
    round_status: 'new',
    verification: 'unknown',
    fix_references: [],
    created_at: '2026-10-12T07:00:00Z',
    ...over,
  } as unknown as Finding
}

function block(seq: number, severity: Severity, title: string, text: string): DraftSection {
  return {
    position: seq,
    kind: 'finding',
    finding_seq: seq,
    finding_key: `F${seq}`,
    severity,
    heading: `### ${severity} · F${seq} · ${title}`,
    title,
    text,
    agent_title: title,
    agent_text: text,
    rewritten: false,
    posted: true,
  } as unknown as DraftSection
}

function prose(position: number, text: string): DraftSection {
  return {
    position,
    kind: 'prose',
    finding_seq: null,
    finding_key: null,
    severity: null,
    heading: null,
    title: null,
    text,
    agent_title: null,
    agent_text: text,
    rewritten: false,
    posted: true,
  } as unknown as DraftSection
}

const PLACE = { repo: 'acme/portal', pr: 514, head_sha: 'd41c8e2' }

describe('whether a line is inside the diff', () => {
  it('anchors a flagged line the diff drew, on the new file’s side', () => {
    const one = finding(1, 'blocker', {
      file: 'jobs/DeliverWebhook.php',
      line: 16,
      hunk: diffHunk('jobs/DeliverWebhook.php', 13),
      flagged_line: 15,
    })
    expect(anchorOf(one)).toEqual({ path: 'jobs/DeliverWebhook.php', line: 15 })
  })

  it('anchors an unchanged context line inside a hunk the diff drew', () => {
    const one = finding(1, 'should', {
      file: 'jobs/DeliverWebhook.php',
      line: 13,
      hunk: diffHunk('jobs/DeliverWebhook.php', 13),
      flagged_line: 13,
    })
    expect(anchorOf(one)).toEqual({ path: 'jobs/DeliverWebhook.php', line: 13 })
  })

  it('anchors nothing when every line of the hunk is unchanged', () => {
    const one = finding(1, 'should', {
      file: 'jobs/Webhook.php',
      line: 40,
      hunk: shownHunk('jobs/Webhook.php', 40),
      flagged_line: 40,
    })
    expect(anchorOf(one)).toBeNull()
  })

  it('anchors nothing without a flagged line, a hunk, or a file', () => {
    const noLine = finding(1, 'should', {
      file: 'jobs/Webhook.php',
      hunk: diffHunk('jobs/Webhook.php', 13),
    })
    const noHunk = finding(2, 'should', { file: 'jobs/Webhook.php', flagged_line: 13 })
    const noFile = finding(3, 'should', {})

    expect([anchorOf(noLine), anchorOf(noHunk), anchorOf(noFile)]).toEqual([null, null, null])
  })
})

describe('the composed body', () => {
  const findings = [
    finding(1, 'blocker', {
      file: 'app/Policy.php',
      line: 52,
      hunk: diffHunk('app/Policy.php', 50),
      flagged_line: 52,
    }),
    finding(2, 'should', {
      file: 'app/Audit.php',
      line: 22,
      hunk: shownHunk('app/Audit.php', 22),
      flagged_line: 22,
    }),
    finding(3, 'nit', {}),
  ]
  const sections = [
    prose(1, '**1 Blocker · 1 Should · 1 Nit — 3 open.** The shape is right.'),
    block(1, 'blocker', 'The policy is skipped', '**in diff** · `Open`\n\n**What** · it is.'),
    block(2, 'should', 'The audit entry may not fit', '**pre-existing** · `Open`\n\nIt may not.'),
    block(3, 'nit', 'The comment describes a branch that is gone', 'Delete it.'),
    prose(5, '**Not checked:** the front end.'),
  ]

  const body = composeBody(PLACE, sections, findings)

  it('puts the index where the findings begin, under the prose that opens the body', () => {
    expect(body.indexOf(INDEX_HEADING)).toBeGreaterThan(body.indexOf('The shape is right.'))
    expect(body.indexOf(INDEX_HEADING)).toBeLessThan(body.indexOf('### Should · F2 ·'))
  })

  it('carries one row per posted finding, most serious first', () => {
    const rows = body.split('\n').filter(line => line.startsWith('| 🔴') || line.startsWith('| 🟠')
      || line.startsWith('| ⚪') || line.startsWith('| 🔵'))
    expect(rows).toHaveLength(3)
    expect(rows.map(row => row.split(' | ')[0])).toEqual([
      '| 🔴 Blocker · F1',
      '| 🟠 Should · F2',
      '| ⚪ Nit · F3',
    ])
  })

  it('links a row to the line at the head commit, and a fileless finding to the pull request', () => {
    expect(body).toContain(
      '[`Policy.php:52`](https://github.com/acme/portal/blob/d41c8e2/app/Policy.php#L52)',
    )
    expect(body).toContain('[the pull request](https://github.com/acme/portal/pull/514)')
  })

  it('says of each row whether the finding is on the code or in the body', () => {
    expect(body).toContain('| Open · on the code |')
    expect(body).toContain('| Open · in the body |')
  })

  it('leaves an anchored finding out of the body and keeps the others as sections', () => {
    expect(body).not.toContain('### Blocker · F1 ·')
    expect(body).toContain('### Should · F2 · The audit entry may not fit')
    expect(body).toContain('### Nit · F3 · The comment describes a branch that is gone')
  })

  it('carries a prose block’s mermaid fence character for character', () => {
    const fence = '### Problem fit\n\nBefore and after.\n\n```mermaid\nflowchart LR\n'
      + '  A[Due] --> B[Context]\n```'
    const withFence = composeBody(PLACE, [prose(1, fence), ...sections.slice(1)], findings)
    expect(withFence).toContain(fence)
  })

  it('keeps the start of a title too long to clamp at a word', () => {
    /*
     * One word of 180 characters — a claim that is a pasted identifier. Clamping at a word
     * boundary keeps nothing, and a row reading `…` says less than a sentence cut mid-word.
     */
    const long = 'a'.repeat(180)
    const one = finding(1, 'nit', {})
    const body = composeBody(PLACE, [block(1, 'nit', long, 'It is a long name.')], [one])
    const cell = body.split('\n').find(line => line.startsWith('| ⚪'))!.split(' | ')[2]!

    expect(cell.endsWith(' …')).toBe(true)
    expect(cell.length).toBeLessThanOrEqual(140)
    expect(cell.startsWith('aaaa')).toBe(true)
  })

  it('leaves a block with no title out of the index, as it leaves it out of the body', () => {
    /*
     * A row is a claim that the finding posts somewhere, and a block with no title posts
     * nowhere — the section and the thread both drop one. A row with an empty Finding cell
     * would point at a section that is not there.
     */
    const one = finding(1, 'should', {})
    const untitled = { ...block(1, 'should', '', 'Something.'), title: null }
    const body = composeBody(PLACE, [untitled], [one])

    expect(body).not.toContain(INDEX_HEADING)
    expect(body).toBe('')
  })

  it('composes nothing new for a draft with no finding in it', () => {
    expect(composeBody(PLACE, [prose(1, 'One thing to say.')], findings))
      .toBe('One thing to say.\n')
  })
})

describe('the threads handed over', () => {
  const findings = [
    finding(1, 'blocker', {
      file: 'app/Policy.php',
      line: 52,
      hunk: diffHunk('app/Policy.php', 50),
      flagged_line: 52,
    }),
    finding(2, 'nit', {
      file: 'app/Report.php',
      line: 28,
      hunk: diffHunk('app/Report.php', 26),
      flagged_line: 28,
    }),
    finding(3, 'question', {
      file: 'app/Pipe.php',
      line: 71,
      hunk: diffHunk('app/Pipe.php', 69),
      flagged_line: 71,
    }),
  ]
  const threads = composeThreads(
    [
      block(1, 'blocker', 'The policy is skipped', '**in diff** · `Open`\n\n**What** · it is.'),
      block(2, 'nit', 'The comment is stale', 'Delete it with the branch.'),
      block(3, 'question', 'Could two run at once?', '**in diff** · `Open`\n\n**What** · could they?'),
    ],
    findings,
  )

  it('is most serious first, with a question above a nit', () => {
    expect(threads.map(thread => thread.finding_key)).toEqual(['F1', 'F3', 'F2'])
  })

  it('sits on the line the hunk names, on the right-hand side', () => {
    expect(threads[0]).toMatchObject({ path: 'app/Policy.php', line: 52, side: 'RIGHT' })
  })

  it('puts a blocker in a caution rail and a question in a note', () => {
    expect(threads[0]!.body.startsWith('> [!CAUTION]\n> **Blocker · F1** — The policy is skipped · `Open`\n>\n')).toBe(true)
    expect(threads[1]!.body.startsWith('> [!NOTE]\n> **Question · F3** — ')).toBe(true)
  })

  it('gives a nit the lighter shape, with a glyph and no rail', () => {
    expect(threads[2]!.body).toBe(
      '⚪ **Nit · F2** — The comment is stale · `Open`\nDelete it with the branch.',
    )
  })

  it('carries the block’s own text through, blockquoted and otherwise untouched', () => {
    expect(threads[0]!.body).toContain('> **in diff** · `Open`')
    expect(threads[0]!.body).toContain('> **What** · it is.')
  })
})
