import { readFileSync } from 'node:fs'
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

/** The real-length review: #514, fourteen findings, six parts. */
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
     * And the three reasons a finding shows no code, each with an example — the page has a
     * different sentence for each and all three have to be reachable.
     */
    const noCode = detail.findings.filter(finding => finding.hunk == null)
    const claimed = new Map(
      (detail.brief?.parts ?? []).flatMap(part => part.files.map(file => [file.path, file])),
    )
    expect(noCode.some(finding => finding.file == null), 'one is about the description')
      .toBe(true)
    expect(
      noCode.some(finding => claimed.get(finding.file ?? '')?.in_diff === false),
      'one names a file outside the change',
    ).toBe(true)
    expect(
      noCode.some(finding =>
        finding.file != null && claimed.get(finding.file)?.in_diff !== false),
      'one names a changed file the session recorded no code for',
    ).toBe(true)
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
