import { describe, expect, it } from 'vitest'

import {
  GROUP_HEADINGS,
  findingLink,
  findingsInNoPart,
  findingsOn,
  foundOn,
  groupOf,
  isInTheChange,
  labelOfProblem,
  noPartHeading,
  oneLineOf,
  partGroups,
  seqInHash,
  sizeOf,
  standsAt,
} from '../app/utils/solution'

import type { Brief, BriefPart, BriefPartFile, Disposition, Finding } from '../app/types/desk'

/*
 * The rules board A10's table is built out of, checked without a render.
 *
 * Every one of these decides something a reader is told — which group a part is announced
 * under, whether a row claims a size or says the pull request did not touch it, and whether
 * a finding is accounted for at all. `test/step-pages.nuxt.spec.ts` mounts the page against
 * the record; this is the arithmetic on its own, which is the shape `test/findings.spec.ts`
 * argues for: a count worked out inline is a count nothing can test.
 */

function file(path: string, extra: Partial<BriefPartFile> = {}): BriefPartFile {
  return { path, lines_added: 4, lines_removed: 1, in_diff: true, ...extra }
}

/** A file the pull request did not change, which has no lines either way. */
function outside(path: string): BriefPartFile {
  return { path, lines_added: 0, lines_removed: 0, in_diff: false }
}

function part(extra: Partial<BriefPart> = {}): BriefPart {
  return {
    title: 'a part',
    summary: 'what it did, at the length a session writes it',
    one_line: null,
    fixes: [],
    files: [file('app/One.php')],
    lines_added: 4,
    lines_removed: 1,
    ...extra,
  }
}

function finding(seq: number, extra: Partial<Finding> = {}): Finding {
  return {
    review_id: 1,
    seq,
    key: `F${seq}`,
    lens: 'correctness',
    severity: 'should',
    file: 'app/One.php',
    claim: 'something is wrong',
    disposition: 'in_draft' as Disposition,
    created_at: '2026-10-11T00:00:00Z',
    ...extra,
  } as Finding
}

describe('which group a part belongs to', () => {
  it('is "fixes a problem" when it names one and has a file in the change', () => {
    expect(groupOf(part({ fixes: [1] }))).toBe('fixes')
  })

  it('is "supporting" when it names none and has a file in the change', () => {
    expect(groupOf(part({ fixes: [] }))).toBe('supporting')
  })

  it('is "not changed" when none of its files is in the change', () => {
    expect(groupOf(part({ files: [outside('config/queue.php')] }))).toBe('unchanged')
  })

  /**
   * **The order the rule is applied in, which is the group's whole reason for existing.**
   *
   * A part the pull request did not touch is not something that was built, whatever it
   * claims to fix. Deciding on `fixes` first would announce it under "Fixes a problem" —
   * the group that says the opposite of what is true of it.
   */
  it('is "not changed" even when it claims to fix a problem', () => {
    expect(groupOf(part({ fixes: [3], files: [outside('config/queue.php')] })))
      .toBe('unchanged')
  })

  /*
   * **One file in the change is enough**, which is the rule's own wording — the group is
   * for a part *none* of whose files is in it. Something of this part was built, so it reads
   * as something that was built, and the file it also claims is a file it relies on.
   */
  it('is not "not changed" when one file of several is in the change', () => {
    const mixed = part({ files: [file('app/One.php'), outside('app/Two.php')] })
    expect(isInTheChange(mixed)).toBe(true)
    expect(groupOf(mixed)).toBe('supporting')
    expect(groupOf({ ...mixed, fixes: [1] })).toBe('fixes')
  })

  /* A part with no files at all: "none of its files is in the change" is true of it. */
  it('is "not changed" for a part with no files recorded', () => {
    expect(groupOf(part({ files: [] }))).toBe('unchanged')
    expect(isInTheChange(part({ files: [] }))).toBe(false)
  })
})

describe('the groups of the table', () => {
  it('keeps the record’s order inside a group, and numbers from the record', () => {
    const parts = [
      part({ title: 'first', fixes: [1] }),
      part({ title: 'tests', files: [file('tests/One.php')] }),
      part({ title: 'second', fixes: [2], files: [file('app/Two.php')] }),
    ]
    const groups = partGroups(parts)
    expect(groups.map(one => one.group)).toEqual(['fixes', 'supporting'])
    /* The two that fix something, in the order the session wrote them. */
    expect(groups[0]!.rows.map(one => one.part.title)).toEqual(['first', 'second'])
    /* And each carries the number it has in the record, not its place in the group. */
    expect(groups[0]!.rows.map(one => one.number)).toEqual([1, 3])
    expect(groups[0]!.rows.map(one => one.index)).toEqual([0, 2])
  })

  it('leaves out a group with no part in it, rather than drawing an empty one', () => {
    const groups = partGroups([part({ fixes: [1] })])
    expect(groups).toHaveLength(1)
    expect(groups[0]!.heading).toBe(GROUP_HEADINGS.fixes)
  })

  it('draws no groups at all for a brief with no parts', () => {
    expect(partGroups([])).toEqual([])
  })
})

describe('the one line a row carries', () => {
  it('is the part’s own where the session wrote one', () => {
    expect(oneLineOf(part({ one_line: 'The queue keeps the attempt.' })))
      .toEqual({ text: 'The queue keeps the attempt.', cut: false })
  })

  /**
   * **The summary stands in, and is marked as a cut.** The cutting itself is the column's,
   * because no character count here knows how wide that column is at 390px — so the whole
   * string comes back and the template says it is a cut.
   */
  it('is the summary, marked as a cut, where the session wrote none', () => {
    const long = 'a word '.repeat(52).trim()
    expect(oneLineOf(part({ one_line: null, summary: long })))
      .toEqual({ text: long, cut: true })
  })

  it('does not treat an absent field as a written one', () => {
    const { one_line: _absent, ...without } = part()
    expect(oneLineOf(without as BriefPart).cut).toBe(true)
  })
})

describe('the size of a part', () => {
  it('counts the files and sums the lines from the files themselves', () => {
    const size = sizeOf(part({
      files: [
        file('app/One.php', { lines_added: 112, lines_removed: 0 }),
        file('app/Two.php', { lines_added: 8, lines_removed: 61 }),
      ],
    }))
    expect(size).toEqual({ files: '2 files', lines: '+120 −61', unchanged: false })
  })

  it('says one file in the singular', () => {
    expect(sizeOf(part()).files).toBe('1 file')
  })

  /**
   * **"unchanged", not "+0 −0".** A pair of noughts reads as a part nobody wrote anything
   * in; this is a part the pull request did not touch, which is a different fact and the
   * reason its group exists.
   */
  it('says a part outside the change is unchanged rather than drawing noughts', () => {
    const size = sizeOf(part({ files: [outside('config/queue.php')] }))
    expect(size).toEqual({ files: '1 file', lines: null, unchanged: true })
  })
})

describe('what the code review found, on a row', () => {
  const mixed = [
    finding(1),
    finding(2),
    finding(3, { disposition: 'held_back' }),
    finding(4, { disposition: 'suppressed' }),
  ]

  it('is up to three counts, in the order the issue names them', () => {
    expect(foundOn(mixed)).toEqual([
      { label: 'to post', n: 2 },
      { label: 'held for a ticket', n: 1 },
      { label: 'set aside', n: 1 },
    ])
  })

  it('leaves out a count of nought rather than drawing it', () => {
    expect(foundOn([finding(1)])).toEqual([{ label: 'to post', n: 1 }])
  })

  /** Nothing at all, which the template says in a word rather than as an empty cell. */
  it('is empty for a part the review found nothing in', () => {
    expect(foundOn([])).toEqual([])
  })

  /**
   * **The three counts sum to the findings in the part**, which is the property that makes
   * the cell checkable against the record — and the reason this is keyed on the disposition
   * rather than on `willPost`. Every disposition the contract has falls in exactly one of
   * the three.
   */
  it('accounts for every finding exactly once', () => {
    const dispositions: Disposition[] = ['in_draft', 'held_back', 'suppressed']
    const every = dispositions.flatMap((disposition, at) =>
      [finding(at * 2 + 1, { disposition }), finding(at * 2 + 2, { disposition })])
    expect(foundOn(every).reduce((total, entry) => total + entry.n, 0)).toBe(every.length)
  })

  it('says where one finding stands, in the same three words capitalised', () => {
    expect(standsAt(finding(1))).toBe('To post')
    expect(standsAt(finding(2, { disposition: 'held_back' }))).toBe('Held for a ticket')
    expect(standsAt(finding(3, { disposition: 'suppressed' }))).toBe('Set aside')
  })
})

describe('which part a finding is on', () => {
  const parts = [
    part({ title: 'one', files: [file('app/One.php')] }),
    part({ title: 'two', files: [file('app/Two.php')] }),
  ]
  const findings = [
    finding(3, { file: 'app/Two.php' }),
    finding(1, { file: 'app/One.php' }),
    finding(2, { file: 'app/Elsewhere.php' }),
    finding(4, { file: null }),
  ]

  it('is the part whose files name it, in the review’s own order', () => {
    expect(findingsOn(findings, parts, 0).map(one => one.key)).toEqual(['F1'])
    expect(findingsOn(findings, parts, 1).map(one => one.key)).toEqual(['F3'])
  })

  it('is no part for a finding in a file no part claims, and for one with no file', () => {
    expect(findingsInNoPart(findings, parts).map(one => one.key)).toEqual(['F2', 'F4'])
  })

  /**
   * **Every finding is accounted for exactly once**, which is DEV-919's second criterion and
   * the property the card version did not have: it matched a finding to *every* part whose
   * files named its path, so a path claimed twice put its findings under both parts.
   */
  it('puts a file claimed by two parts on the first of them only', () => {
    const twice = [
      part({ title: 'one', files: [file('app/Shared.php')] }),
      part({ title: 'two', files: [file('app/Shared.php')] }),
    ]
    const one = [finding(1, { file: 'app/Shared.php' })]
    expect(findingsOn(one, twice, 0).map(f => f.key)).toEqual(['F1'])
    expect(findingsOn(one, twice, 1)).toEqual([])
    expect(findingsInNoPart(one, twice)).toEqual([])
  })

  it('accounts for every finding across the rows and the list under them', () => {
    const seen = [
      ...parts.flatMap((_, at) => findingsOn(findings, parts, at)),
      ...findingsInNoPart(findings, parts),
    ].map(one => one.key)
    expect([...seen].sort()).toEqual(['F1', 'F2', 'F3', 'F4'])
  })
})

describe('the heading over the findings in no part', () => {
  it('agrees with itself about one finding', () => {
    expect(noPartHeading(1)).toBe('1 finding is not about any one part')
  })

  it('agrees with itself about several', () => {
    expect(noPartHeading(7)).toBe('7 findings are not about any one part')
  })
})

describe('a problem’s name, for the tag on a row', () => {
  const brief = {
    problems: [
      { number: 1, label: 'The shared slot', was_wrong: 'a sentence', now_fixed: 'another' },
      { number: 2, label: null, was_wrong: 'a sentence', now_fixed: 'another' },
    ],
  } as unknown as Brief

  it('is the label the record holds', () => {
    expect(labelOfProblem(brief, 1)).toBe('The shared slot')
  })

  /* `ProblemTag` draws "Problem 2" for null, which is its own rule rather than this one's. */
  it('is null where the record holds none, and for a number no problem carries', () => {
    expect(labelOfProblem(brief, 2)).toBeNull()
    expect(labelOfProblem(brief, 9)).toBeNull()
    expect(labelOfProblem({} as Brief, 1)).toBeNull()
  })
})

describe('the link from a finding line to that finding', () => {
  it('names the finding in the hash, not just the page', () => {
    expect(findingLink(41, finding(9))).toBe('/reviews/41/code-review#F9')
  })

  it('reads its own hash back', () => {
    expect(seqInHash('#F9')).toBe(9)
    expect(seqInHash('#F15')).toBe(15)
  })

  /**
   * Anything else is nothing to arrive at. A hash the page did not write is a hash about
   * something else, and guessing at it would open a row the reader did not ask for.
   */
  it('reads nothing out of a hash that does not name a finding', () => {
    for (const hash of ['', '#', '#F', '#F0', '#Fx', '#9', 'F9', '#F9x', '#finding-9']) {
      expect(seqInHash(hash), hash).toBeNull()
    }
  })
})
