import { describe, expect, it } from 'vitest'

import {
  headline,
  inPart,
  isCarried,
  listOf,
  measuredAt,
  mostSeriousFirst,
  partOf,
  partTiles,
  reasoningBlocks,
  reasoningWords,
  reproducedLine,
  roundStatusLabel,
  sameCommit,
  tally,
  whereOf,
  whyNoCode,
  willPost,
} from '../app/utils/findings'

import type { Brief, Finding, Review, Severity } from '../app/types/desk'

/*
 * The derivations the code review page makes, each checked on its own.
 *
 * **Board A8 is almost entirely arithmetic over the findings** — the headline, a part's
 * tally, which of three lists a finding is in, how long its reasoning runs — and none of it
 * is in the record. A count worked out in a template is a count nothing can test, and the
 * one that matters most is the sentence Patrick reads first.
 */

/** A finding with only the fields a given test cares about. */
function finding(over: Partial<Finding> & { seq: number }): Finding {
  return {
    review_id: 1,
    key: `F${over.seq}`,
    lens: 'correctness',
    severity: 'should' as Severity,
    file: 'app/Thing.php',
    line: 10,
    line_sha: null,
    claim: 'something is wrong',
    title: null,
    evidence: null,
    disposition: 'in_draft',
    disposition_reason: null,
    rule_id: null,
    rule_key: null,
    hunk: null,
    flagged_line: null,
    inline_comment: null,
    why_it_matters: null,
    proof: null,
    proof_note: null,
    verification: 'unknown',
    suggested_fix: null,
    fix_references: [],
    created_at: '2026-10-09T00:00:00Z',
    round_status: 'new',
    round_status_by: null,
    round_status_checked: null,
    round_status_at: null,
    round_status_commit: null,
    continues_review_id: null,
    continues_round: null,
    rounds_open: null,
    introduced_by_commit: null,
    introduced_by_seq: null,
    introduced_by_key: null,
    late: null,
    ...over,
  } as Finding
}

describe('the order findings are read in', () => {
  /*
   * **A question sits above a nit**, which is board A8's order and not the order the
   * contract declares its enum in. A question is something the reviewer could not answer
   * and the author can; a nit is something neither has to do anything about.
   */
  it('puts a question above a nit, and a blocker above everything', () => {
    const order = mostSeriousFirst([
      finding({ seq: 1, severity: 'nit' }),
      finding({ seq: 2, severity: 'question' }),
      finding({ seq: 3, severity: 'blocker' }),
      finding({ seq: 4, severity: 'should' }),
    ]).map(one => one.severity)

    expect(order).toEqual(['blocker', 'should', 'question', 'nit'])
  })

  it('breaks a tie by number, and leaves the caller’s array alone', () => {
    const given = [finding({ seq: 9 }), finding({ seq: 2 })]
    expect(mostSeriousFirst(given).map(one => one.seq)).toEqual([2, 9])
    expect(given.map(one => one.seq)).toEqual([9, 2])
  })
})

describe('the headline', () => {
  it('says the counts, and says "No blockers" out loud', () => {
    expect(headline([
      finding({ seq: 1, severity: 'should' }),
      finding({ seq: 2, severity: 'should' }),
      finding({ seq: 3, severity: 'question' }),
      finding({ seq: 4, severity: 'nit' }),
      finding({ seq: 5, severity: 'nit' }),
    ])).toBe('No blockers. 2 should change, 1 question, 2 small points.')
  })

  /* Zero is worth saying for exactly one severity: it is the question the page answers. */
  it('leaves out every other zero', () => {
    expect(headline([finding({ seq: 1, severity: 'should' })]))
      .toBe('No blockers. 1 should change.')
  })

  it('leads with the blockers when there are any', () => {
    expect(headline([
      finding({ seq: 1, severity: 'blocker' }),
      finding({ seq: 2, severity: 'nit' }),
    ])).toBe('1 blocker. 1 small point.')
  })

  it('says something readable when there is nothing to post at all', () => {
    expect(headline([])).toBe(
      'Nothing to post. The review found nothing it would say to the author.',
    )
  })

  /*
   * **A finding carried as fixed is not counted**, even at disposition `in_draft`. The
   * contract allows that pair and the composition will not post it, so a headline counting
   * it would be wrong about the one thing the page says out loud.
   */
  it('counts what will be posted, not what the disposition says', () => {
    const findings = [
      finding({ seq: 1, severity: 'should' }),
      finding({
        seq: 2,
        severity: 'should',
        continues_review_id: 40,
        round_status: 'fixed',
      }),
      finding({ seq: 3, severity: 'should', disposition: 'held_back' }),
    ]
    expect(headline(findings)).toBe('No blockers. 1 should change.')
    expect(willPost(findings[0]!)).toBe(true)
    expect(willPost(findings[1]!), 'fixed last round').toBe(false)
    expect(willPost(findings[2]!), 'held back').toBe(false)
  })
})

describe('how the findings were checked', () => {
  it('counts the reproduced ones and says the rest were read', () => {
    expect(reproducedLine([
      finding({ seq: 1, verification: 'reproduced' }),
      finding({ seq: 2, verification: 'read_not_run' }),
      finding({ seq: 3, verification: 'unknown' }),
    ])).toBe('1 of 3 findings was reproduced by running something. The rest were read, not run.')
  })

  /*
   * **One finding is its own sentence.** "1 of 1 finding were reproduced … The rest were
   * read, not run." is the wrong verb and a "rest" that refers to nothing, and a review
   * with one finding is not a rare shape.
   */
  it('does not say "the rest" when there is no rest', () => {
    expect(reproducedLine([finding({ seq: 1, verification: 'reproduced' })]))
      .toBe('It was reproduced by running something.')
    expect(reproducedLine([
      finding({ seq: 1, verification: 'reproduced' }),
      finding({ seq: 2, verification: 'reproduced' }),
    ])).toBe('All 2 findings were reproduced by running something.')
  })

  it('says none were, rather than "0 of 3"', () => {
    expect(reproducedLine([finding({ seq: 1 }), finding({ seq: 2 })]))
      .toBe('None of these was reproduced by running something; they were read.')
  })

  /*
   * And the same agreement here, which the branches above were fixed for and this one was
   * not: "None of these … they were read" is three plurals about one finding, and a single
   * finding reaches this branch before either of them.
   */
  it('keeps the agreement when one finding was not reproduced', () => {
    expect(reproducedLine([finding({ seq: 1 })]))
      .toBe('It was not reproduced by running something; it was read.')
  })

  it('says nothing at all about a review with no findings', () => {
    expect(reproducedLine([])).toBeNull()
  })
})

describe('the parts strip', () => {
  const brief = {
    parts: [
      { title: 'The context', files: [{ path: 'a.php' }, { path: 'shared.php' }] },
      { title: 'The pipeline', files: [{ path: 'b.php' }, { path: 'shared.php' }] },
      { title: 'Tests', files: [{ path: 'c.php' }] },
    ],
  } as unknown as Brief
  const parts = brief.parts!

  it('puts a finding in the first part that claims its file, never in two', () => {
    const shared = finding({ seq: 1, file: 'shared.php' })
    expect(partOf(shared, parts)).toBe(1)
    expect(inPart(shared, 1, parts)).toBe(true)
    expect(inPart(shared, 2, parts)).toBe(false)
  })

  it('puts a finding no part claims outside them, including one with no file', () => {
    expect(partOf(finding({ seq: 1, file: 'elsewhere.php' }), parts)).toBe(0)
    expect(partOf(finding({ seq: 2, file: null }), parts)).toBe(0)
    expect(inPart(finding({ seq: 2, file: null }), -1, parts)).toBe(true)
  })

  /** The counts must sum to the review's total, or the strip is lying about the review. */
  it('accounts for every finding exactly once', () => {
    const findings = [
      finding({ seq: 1, file: 'a.php' }),
      finding({ seq: 2, file: 'shared.php' }),
      finding({ seq: 3, file: 'b.php', disposition: 'held_back' }),
      finding({ seq: 4, file: null }),
    ]
    const tiles = partTiles(findings, parts)
    const counted = tiles.slice(1).reduce((total, tile) => total + tile.total, 0)
    expect(counted).toBe(findings.length)
    expect(tiles[0]!.total, 'and "Everything" holds all of them').toBe(findings.length)
  })

  it('says "Nothing found" for a part with none, and tallies the rest', () => {
    const tiles = partTiles(
      [finding({ seq: 1, file: 'a.php' }), finding({ seq: 2, file: 'a.php', disposition: 'suppressed' })],
      parts,
    )
    expect(tiles.find(one => one.name === 'The context')!.tally).toBe('1 to post · 1 not')
    expect(tiles.find(one => one.name === 'Tests')!.tally).toBe('Nothing found')
  })

  /*
   * "Outside the parts" is drawn only when something is outside them. A tile reading
   * "Nothing found" for a case that cannot arise is a filter nobody would press, where a
   * *part* reading it is a measurement.
   */
  it('draws "Outside the parts" only when something is', () => {
    expect(partTiles([finding({ seq: 1, file: 'a.php' })], parts).map(one => one.name))
      .not.toContain('Outside the parts')
    expect(partTiles([finding({ seq: 1, file: 'nowhere.php' })], parts).map(one => one.name))
      .toContain('Outside the parts')
  })
})

describe('the reasoning', () => {
  const evidence = 'Mechanism: The cast answers an empty array.\n\n'
    + 'How we know: The run above.\n\n'
    + 'A paragraph with no label of its own.'

  it('reads a label off a paragraph that has one', () => {
    expect(reasoningBlocks(evidence).map(one => one.label))
      .toEqual(['Mechanism', 'How we know', 'Detail'])
    expect(reasoningBlocks(evidence)[0]!.text).toBe('The cast answers an empty array.')
  })

  /* The first unlabelled paragraph is the mechanism; anything after it is detail. */
  it('labels an unlabelled first paragraph differently from a later one', () => {
    expect(reasoningBlocks('One.\n\nTwo.').map(one => one.label))
      .toEqual(['Mechanism', 'Detail'])
  })

  /* A sentence containing a colon is not a forty-word heading. */
  it('does not read a sentence with a colon in it as a label', () => {
    const long = 'The rule is this one and it runs well past any heading anybody would '
      + 'write: never do that.'
    expect(reasoningBlocks(long)[0]!.label).toBe('Mechanism')
    expect(reasoningBlocks(long)[0]!.text).toBe(long)
  })

  /*
   * **The count and the blocks come from one string**, which is why both live here: a
   * label that lies about what opens is the one number on the page a reader checks.
   */
  it('counts the words of the string it splits', () => {
    expect(reasoningWords(evidence)).toBe(evidence.split(/\s+/).length)
    expect(reasoningWords(null)).toBe(0)
    expect(reasoningBlocks(null)).toEqual([])
  })
})

describe('what a finding has instead of code', () => {
  const brief = {
    parts: [{
      title: 'A part',
      files: [{ path: 'touched.php', in_diff: true }, { path: 'untouched.php', in_diff: false }],
    }],
  } as unknown as Brief

  it('says a finding with no file is about the pull request, and names the lens', () => {
    const said = whyNoCode(finding({ seq: 1, file: null, lens: 'description' }), brief)
    expect(said).toContain('names no file')
    expect(said).toContain('“description” lens')
  })

  it('says a file a part claims with in_diff false is not part of the change', () => {
    expect(whyNoCode(finding({ seq: 1, file: 'untouched.php' }), brief))
      .toContain('is not part of this change')
  })

  it('otherwise says the session recorded none, and names the line', () => {
    const said = whyNoCode(finding({ seq: 1, file: 'touched.php', line: 12 }), brief)
    expect(said).toContain('The session recorded no code')
    expect(said).toContain('touched.php, line 12')
  })

  it('names where a finding is, with or without a line', () => {
    expect(whereOf(finding({ seq: 1, file: 'a.php', line: 4 }))).toBe('a.php:4')
    expect(whereOf(finding({ seq: 1, file: 'a.php', line: null }))).toBe('a.php')
    expect(whereOf(finding({ seq: 1, file: null }))).toBe('The pull request itself')
  })
})

describe('the three lists, and a finding across rounds', () => {
  it('keys the lists on the disposition, which is what the developer decided', () => {
    expect(listOf(finding({ seq: 1, disposition: 'in_draft' }))).toBe('post')
    expect(listOf(finding({ seq: 2, disposition: 'held_back' }))).toBe('ticket')
    expect(listOf(finding({ seq: 3, disposition: 'suppressed' }))).toBe('aside')
  })

  it('tallies the three', () => {
    expect(tally([
      finding({ seq: 1 }),
      finding({ seq: 2, disposition: 'held_back' }),
      finding({ seq: 3, disposition: 'suppressed' }),
      finding({ seq: 4 }),
    ])).toBe('2 to post, 1 for a ticket, 1 set aside')
  })

  /*
   * **Carried is the link, not the status.** A finding raised this round carries `new`; one
   * carried from round 1 and still open carries `still_open`. What makes it *carried* is
   * `continues_review_id`, which is also what the contract keys `Finding.late` on.
   */
  it('says a carried finding’s round status, and nothing for a fresh one', () => {
    expect(isCarried(finding({ seq: 1 }))).toBe(false)
    expect(roundStatusLabel(finding({ seq: 1 }))).toBeNull()

    const carried = finding({ seq: 2, continues_review_id: 40, round_status: 'still_open' })
    expect(isCarried(carried)).toBe(true)
    expect(roundStatusLabel(carried)).toBe('Still open')
    expect(roundStatusLabel(finding({ seq: 3, continues_review_id: 40, round_status: 'fixed' })))
      .toBe('Fixed')
  })
})

/** A review at a head commit, for the one comparison `measuredAt` makes. */
function reviewAt(head: string): Review {
  return { id: 1, repo: 'globex/portal', pr: 731, head_sha: head, round: 2 } as Review
}

describe('which commit a line was measured on', () => {
  /*
   * **DEV-978.** The row's link goes to the commit the line was counted on, and the pull
   * request may have moved past it. The sentence names both, which is `withdrawnLine`'s
   * rule: naming one of two leaves the reader to find the other.
   */
  it('names both commits when the line was measured on an earlier one', () => {
    const said = measuredAt(
      finding({ seq: 1, line: 40, line_sha: 'a1b2c3d' }),
      reviewAt('e4f5a6b'),
    )

    expect(said).toContain('Measured at a1b2c3d')
    expect(said).toContain('e4f5a6b')
    expect(said).toContain('has not been re-read')
  })

  it('says nothing when the line was measured on the head the review is at', () => {
    expect(
      measuredAt(finding({ seq: 1, line: 40, line_sha: 'e4f5a6b' }), reviewAt('e4f5a6b')),
    ).toBeNull()
  })

  /*
   * **Not "the line was not measured", which is a different and untrue claim.** The line
   * was measured — its number is on the same row — and what the record is missing is which
   * commit on. This is also why `NotMeasured` is not the component for it: its words
   * default to "Not measured yet", and nothing is going to come along and measure a row
   * written before the column existed.
   */
  it('says the commit was not recorded when the record has none', () => {
    const said = measuredAt(finding({ seq: 1, line: 40, line_sha: null }), reviewAt('e4f5a6b'))

    expect(said).toContain('was not recorded')
    expect(said).toContain('e4f5a6b')
    expect(said).not.toContain('not measured')
  })

  it('says nothing about a finding with no line for a commit to be about', () => {
    expect(
      measuredAt(
        finding({ seq: 1, file: null, line: null, flagged_line: null, line_sha: null }),
        reviewAt('e4f5a6b'),
      ),
    ).toBeNull()
  })

  /*
   * A head is "full or abbreviated" by the contract's own words and a finding's commit is
   * 7 to 40 characters, so the same commit arrives written two ways. Equality would put a
   * notice on a row where nothing moved.
   */
  it('reads two spellings of one commit as one commit', () => {
    expect(sameCommit('d41c8e2', 'd41c8e2f6b1904a7c3e8d2b5f0a9c1e4d7b63a28')).toBe(true)
    expect(sameCommit('D41C8E2', 'd41c8e2')).toBe(true)
    expect(sameCommit('d41c8e2', 'a1b2c3d')).toBe(false)
    expect(sameCommit('', 'd41c8e2')).toBe(false)
  })

  it('draws no notice where a full head and an abbreviated commit are the same one', () => {
    expect(
      measuredAt(
        finding({ seq: 1, line: 40, line_sha: 'd41c8e2' }),
        reviewAt('d41c8e2f6b1904a7c3e8d2b5f0a9c1e4d7b63a28'),
      ),
    ).toBeNull()
  })
})
