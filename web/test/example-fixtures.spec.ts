import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

import drafts from '../app/data/drafts.json'
import reviews from '../app/data/reviews.json'

import type { DraftDetail, ReviewDetail } from '../app/types/desk'

/*
 * **The invented example lives in `contract/examples/` and the fixtures are copies of it.**
 *
 * Three places read the same invented review: `contract/examples/` is the contract's own
 * statement of what a response at real length looks like, `web/app/data/` is what sample
 * mode serves the unit tests and the development loop, and `web/e2e/harness.ts` feeds the
 * example to the binary through the command line. The third reads the example file directly
 * and so cannot drift. The second cannot read it — the fixtures are bundled JSON imports and
 * a build must not reach outside `web/` — so it is a copy, and this is what stops a copy
 * becoming a second opinion.
 *
 * The failure it prevents is specific and has happened before on this project in another
 * form: a page built against the fixture, a contract example edited to match a different
 * idea of the same review, and a page that then worked on fixtures and drew the wrong thing
 * against the binary. Asserted whole rather than field by field, because the point is that
 * there is no difference at all.
 */

const examples = resolve(__dirname, '../../contract/examples')

function example<T>(name: string): T {
  return JSON.parse(readFileSync(resolve(examples, name), 'utf8')) as T
}

/** The real-length review: #514, fifteen findings, seven parts. */
const REAL_LENGTH = 41

describe('the invented example and the fixtures', () => {
  it('serves the same review the contract example holds', () => {
    const fixture = (reviews as unknown as ReviewDetail[])
      .find(detail => detail.review.id === REAL_LENGTH)
    expect(fixture, `review ${REAL_LENGTH} is the real-length example`).toBeDefined()
    expect(fixture).toEqual(example<ReviewDetail>('getReview.response.code-review.json'))
  })

  it('serves the same draft the contract example holds', () => {
    const fixture = (drafts as unknown as DraftDetail[])
      .find(detail => detail.draft.review_id === REAL_LENGTH)
    expect(fixture, `review ${REAL_LENGTH} has the composed draft`).toBeDefined()
    expect(fixture).toEqual(example<DraftDetail>('getDraft.response.real-length.json'))
  })

  /*
   * **Every draft fixture carries its blocks**, because `DraftDetail.sections` is required
   * and a fixture missing it is a fixture the server could not have served.
   *
   * This was found the hard way: four of the five had no `sections` key at all, and the
   * first thing to read it was the code review page's own refusal path — which crashed
   * instead of refusing, and reported "nothing is listening on this address" for a draft
   * that was right there. A one-block draft has **one prose block**, which is migration 9's
   * own backfill for a body nothing wrote in parts, and no block about any finding — which
   * is exactly why the per-finding decisions refuse against it.
   */
  it('gives every draft fixture the blocks its body is composed of', () => {
    for (const detail of drafts as unknown as DraftDetail[]) {
      const where = `the draft of review ${detail.draft.review_id}`
      expect(Array.isArray(detail.sections), `${where} carries its blocks`).toBe(true)

      const composed = detail.sections
        .filter(section => section.posted)
        .map(section =>
          section.kind === 'prose'
            ? section.text
            : section.text === ''
              ? section.heading
              : `${section.heading}\n\n${section.text}`)
        .join('\n\n')
      expect(
        detail.draft.body,
        `${where} composes to the body it serves`,
      ).toBe(composed === '' ? '' : `${composed}\n`)
    }
  })

  /*
   * And the example is at the length the page was designed against, measured rather than
   * asserted. DEV-878 asks for "at least twelve findings, eight with a hunk, across all
   * three lists"; an example that quietly shrank below that would leave the page's hardest
   * states — a row whose claim has to be clamped, a code block that scrolls, a list with
   * something in it — untested while every test still passed.
   */
  it('is at the length the code review page was designed against', () => {
    const detail = example<ReviewDetail>('getReview.response.code-review.json')

    expect(detail.findings.length).toBeGreaterThanOrEqual(12)
    expect(detail.findings.filter(finding => finding.hunk != null).length)
      .toBeGreaterThanOrEqual(8)

    for (const list of ['in_draft', 'held_back', 'suppressed'] as const) {
      expect(
        detail.findings.filter(finding => finding.disposition === list).length,
        `the ${list} list has something in it`,
      ).toBeGreaterThan(0)
    }

    /*
     * And the three things the page says about a finding whose code is not a changed line,
     * each with an example — one sentence per case, and each has to be reachable.
     *
     * **Two of the three are no-code cases and the third comes with code.** A session
     * records the lines around a finding on a line the diff does not draw as a hunk whose
     * every marker is `unchanged`, so "not part of the change" is a hunk with a label
     * rather than an absence; F12, in a file the pull request never touched, is that one.
     */
    const noCode = detail.findings.filter(finding => finding.hunk == null)
    expect(noCode.some(finding => finding.file == null), 'one is about the description')
      .toBe(true)
    expect(
      noCode.some(finding => finding.file != null),
      'one names a file the session recorded no code for',
    ).toBe(true)
    expect(
      detail.findings.some(
        finding =>
          finding.hunk != null
          && finding.hunk.lines.every(line => line.marker === 'unchanged'),
      ),
      'one carries the code around a line the pull request did not change',
    ).toBe(true)
  })

  /**
   * And it is at the length the **three step pages** were measured against, which is a
   * different measurement from the one above and was the whole of DEV-892's second
   * acceptance criterion.
   *
   * Two defects on the real review were invisible against the invented example because the
   * example was shorter than any record: a step note printed whole in a column 150px wide,
   * and a matrix column headed with a problem's entire sentence. Both are fixed; neither
   * can be *shown* fixed by an example whose notes are three words and whose problems have
   * no labels, and the next person to rewrite this example would quietly take the evidence
   * away with them. So the lengths are asserted here, where the example lives.
   */
  it('is at the length the three step pages were designed against', () => {
    const detail = example<ReviewDetail>('getReview.response.code-review.json')

    /* Every numbered problem has the two or three words a column heading can be. */
    const problems = detail.brief?.problems ?? []
    expect(problems.length, 'the brief numbers its problems').toBeGreaterThan(0)
    for (const problem of problems) {
      expect(problem.label, `problem ${problem.number} has a short label`).toBeTruthy()
      expect(
        problem.label,
        `problem ${problem.number}'s label is not its sentence`,
      ).not.toBe(problem.was_wrong)
    }

    /*
     * And one step's note runs long. Forty words is the criterion's own floor and well
     * past what two lines of a trail column hold, so the clamp and its control are drawn
     * by the example rather than only by a test that builds its own record.
     */
    const longest = Math.max(
      ...detail.layers.map(layer => (layer.detail ?? '').trim().split(/\s+/).filter(Boolean).length),
    )
    expect(longest, 'a step note runs to at least 40 words').toBeGreaterThanOrEqual(40)
  })

  /**
   * **The review's two tallies are counted from its own findings.**
   *
   * The server derives both — `wire::severity_counts` and `disposition_counts` count every
   * finding of the review — so an example whose tallies disagree with its `findings` array
   * is one the server it documents could not have produced.
   *
   * **Found by review, after this file passed.** DEV-919 added a fifteenth finding to the
   * example and left `nit: 4` and `suppressed: 1` behind it. The fixture and the example
   * were stale in the same way, so the whole-document comparison above compared two copies
   * of the same wrong number and said nothing. Every review here is checked, not only the
   * one the step pages are drawn from, because the next one to go stale will be another.
   */
  it('counts every review fixture\u2019s tallies off its own findings', () => {
    /*
     * **The contract's own `getReview` examples as well as the fixtures**, which is the
     * layer the first version of this guard was short of. The whole-document comparison
     * above covers only review 41, so the other documented responses — hand-written, and
     * not regenerated from anything — had no check at all. Found reviewing the commit that
     * added this.
     */
    const documented = readdirSync(examples)
      .filter(name => /^getReview\.response.*\.json$/.test(name))
      .map(name => ({ name, detail: example<ReviewDetail>(name) }))
    expect(documented.length, 'the contract documents several review responses')
      .toBeGreaterThan(1)

    /*
     * **Named by their file, not only by their review id.** Three of these documents are
     * review 17 — the fixture and two contract responses — so "review 17's tally is
     * stale" would not say which of the three to open, which is the one thing the
     * failure has to carry.
     */
    const all = [
      ...(reviews as unknown as ReviewDetail[]).map(detail => ({ name: 'reviews.json', detail })),
      ...documented,
    ]

    for (const { name, detail } of all) {
      const where = `review ${detail.review.id} in ${name}`
      const severities = detail.finding_severities
      const dispositions = detail.finding_dispositions

      if (severities != null) {
        const counted = { blocker: 0, should: 0, nit: 0, question: 0 }
        for (const finding of detail.findings) counted[finding.severity] += 1
        expect(severities, `${where}'s severity tally`).toEqual(counted)
      }
      if (dispositions != null) {
        const counted = { in_draft: 0, held_back: 0, suppressed: 0 }
        for (const finding of detail.findings) counted[finding.disposition] += 1
        expect(dispositions, `${where}'s disposition tally`).toEqual(counted)
        expect(
          Object.values(counted).reduce((total, n) => total + n, 0),
          `${where} accounts for each of its findings once`,
        ).toBe(detail.findings.length)
      }
    }
  })

  /*
   * **A hunk's flagged line is one the hunk draws**, which the store refuses a write for and
   * which a hand-written example gets wrong. It did: two of these were off by two and by
   * one, and the binary caught them when the end-to-end seed fed the example through
   * `finding add`. This catches them in a second.
   */
  it('anchors every inline comment to a line its hunk draws', () => {
    const detail = example<ReviewDetail>('getReview.response.code-review.json')

    for (const finding of detail.findings) {
      if (finding.hunk == null) continue
      const drawn = finding.hunk.lines
        .map(line => line.line)
        .filter((line): line is number => line != null)
      expect(drawn, `${finding.key} draws numbered lines`).not.toHaveLength(0)
      expect(drawn, `${finding.key}'s comment is on a line its hunk draws`)
        .toContain(finding.flagged_line)
    }
  })
})
