// @vitest-environment nuxt
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import DraftThreads from '../app/components/DraftThreads.vue'

import type { DraftThread } from '../app/types/desk'

/*
 * **The other half of what posts, at the length a real review has.**
 *
 * The preview beside this card is the body, and since DEV-929 the body is an index: a
 * finding with a line inside the diff is a row in it and a comment on the code. So the
 * question this file is at is whether the developer can see, before they sign off, what the
 * author will read on each line — and the one fact the body cannot carry is which line that
 * is.
 *
 * `test/house-style.spec.ts` holds the composition, `test/example-fixtures.spec.ts` holds
 * the fixtures to it, and the measurement in a real browser at 1440 and 390 is
 * `e2e/review-desk.spec.ts` against the seeded binary. This is the card on its own, with the
 * nine threads the real-length example composes.
 */

/** Where the pull request is in these tests, and where most lines were counted. */
const HEAD = 'd41c8e2'

function thread(seq: number, over: Partial<DraftThread> = {}): DraftThread {
  return {
    finding_seq: seq,
    finding_key: `F${seq}`,
    severity: 'should',
    title: `what F${seq} says is wrong`,
    path: 'app/Reporting/ScheduleToExportContext.php',
    line: 40 + seq,
    line_sha: HEAD,
    side: 'RIGHT',
    body: `> [!WARNING]\n> **Should · F${seq}** — what F${seq} says is wrong · \`Open\`\n>\n`
      + '> **in diff** · `Open — needs a decision`\n>\n> **What** · the mechanism.'
      + `\n\n<!-- review-desk:F${seq} -->`,
    ...over,
  } as DraftThread
}

describe('the threads the draft hands over', () => {
  it('lists every thread with the line it sits on', async () => {
    const threads = [1, 3, 5, 6, 9, 11, 12, 13, 14].map(seq => thread(seq))
    const panel = await mountSuspended(DraftThreads, { props: { threads, headSha: HEAD } })
    const text = panel.text()

    expect(panel.findAll('.thread')).toHaveLength(9)
    expect(panel.find('.rd-label').text()).toContain('9 threads')
    for (const one of threads) {
      expect(text).toContain(`${one.path}:${one.line}`)
      expect(text).toContain(one.finding_key)
    }
  })

  it('draws the comment as GitHub will, markdown and all', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: {
        headSha: HEAD,
        threads: [thread(9, {
          severity: 'blocker',
          body: '> [!CAUTION]\n> **Blocker · F9** — the policy is skipped · `Open`\n>\n'
            + '> **What** · the array cast answers `[]`.\n>\n> ```mermaid\n> flowchart LR\n'
            + '>   A[Due] --> B[Export]\n> ```',
        })],
      },
    })

    /*
     * The comment is rendered rather than printed: a blockquote with the alert's own marker
     * in it, and the backticked value as code — the same renderer the body preview uses,
     * which is the point of using it here.
     */
    expect(panel.find('.comment blockquote').exists()).toBe(true)
    expect(panel.findAll('.comment code').map(code => code.text())).toContain('[]')
    expect(panel.text()).toContain('[!CAUTION]')

    /*
     * **A fence inside the alert is drawn as its source, and that is the honest rendering.**
     * `splitMermaid` lifts a fence that opens a line into a diagram, and every line of a
     * comment is inside the alert's blockquote, so this one is drawn as the characters it
     * is. Whether GitHub turns a mermaid fence inside an alert into a diagram is not
     * something this page can know — and what the page is for is that nothing is posted
     * unread, which the source satisfies. A diagram the developer must see is one the
     * session draws under `### Problem fit` or under a finding that has no thread, and the
     * body preview draws those.
     */
    expect(panel.find('.comment figure, .comment .mermaid, .comment svg').exists()).toBe(false)
    expect(panel.text()).toContain('flowchart LR')
  })

  it('shows the line GitHub does not draw, because it is posted all the same', async () => {
    const panel = await mountSuspended(DraftThreads, { props: { threads: [thread(9)], headSha: HEAD } })

    /*
     * The marker renders as nothing inside the comment — it is an HTML comment, which is
     * the point of it — so the card says it in words instead. What the developer signs off
     * is what posts, invisible parts included.
     */
    expect(panel.find('.marker').text()).toContain('<!-- review-desk:F9 -->')
    /* The reason is said once, in the card's own sentence, and not under every comment. */
    expect(panel.find('.help').text()).toContain('GitHub does not draw')
    expect(panel.find('.marker').text()).not.toContain('GitHub')
  })

  it('speaks of a posted review in the past, and says what has moved since', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: { threads: [thread(9)], headSha: HEAD, posted: true },
    })

    /*
     * The body is stored and is what went to GitHub; these are composed from the record
     * every time they are read, and after the post the record moves on — the harvest reads
     * the author's replies back. A card saying "posts as a comment" there would be
     * claiming to show something it cannot.
     */
    const help = panel.find('.help').text()
    expect(help).toContain('put on the code')
    expect(help).toContain('the harvest has settled')
    expect(help).not.toContain('posts as a comment')
  })

  it('says what it means for a review with nothing to anchor', async () => {
    const panel = await mountSuspended(DraftThreads, { props: { threads: [], headSha: HEAD } })

    expect(panel.findAll('.thread')).toHaveLength(0)
    expect(panel.find('.rd-empty').text())
      .toContain('has a line inside the diff to hang a comment on')
    expect(panel.find('.rd-label').text()).toContain('0 threads')
  })

  it('names the severity as the record’s own word, and grades nothing', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: { threads: [thread(9, { severity: 'blocker' })], headSha: HEAD },
    })

    expect(panel.find('.severity').text()).toContain('blocker')
    /*
     * **And the hidden label is separated from it.** `rd-visually-hidden` is text in the
     * document, so "Severity:" ran straight into the word and the whole thing read
     * `Severity:blocker`. A newline between the two is a whitespace-only node Vue's
     * `whitespace: 'condense'` deletes, and so is a `&#32;` at the end of one — so the
     * separator is an interpolated space. `what-happened.vue` carries the argument, and
     * `test/step-pages.nuxt.spec.ts` the scan that holds every template to it.
     */
    expect(panel.find('.severity').element.textContent).toBe('Severity: blocker')
    expect(panel.find('.tag').exists()).toBe(false)
  })

  /*
   * **DEV-978.** GitHub anchors a comment against the pull request's head, and a thread's
   * line may have been counted on an earlier commit — a carried finding keeps the earlier
   * round's numbering, and since DEV-962 a head moves under findings nobody has re-read.
   * The card says which, on the thread it is true of and on no other: rule 2 is that
   * nothing is printed twice, and nine threads repeating this head would bury the one.
   */
  it('says which commit a line was measured on, where that is not the head', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: {
        headSha: HEAD,
        threads: [
          thread(1, { line_sha: 'a1b2c3d' }),
          thread(2),
        ],
      },
    })

    const rows = panel.findAll('.thread')
    expect(rows[0]!.text()).toContain('measured at a1b2c3d, not at d41c8e2')
    expect(rows[1]!.text()).not.toContain('measured at')
  })

  it('says the commit was not recorded rather than that the line was not measured', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: { headSha: HEAD, threads: [thread(1, { line_sha: null })] },
    })

    const text = panel.find('.thread').text()
    expect(text).toContain('the commit this line was measured on was not recorded')
    /*
     * The line *was* measured — its number is printed on the same line of the same row —
     * and what is missing is which commit on. "Not measured" beside a line number
     * contradicts itself.
     */
    expect(text).not.toContain('not measured')
  })

  it('reads a head written at full length as the same commit', async () => {
    const panel = await mountSuspended(DraftThreads, {
      props: {
        headSha: 'd41c8e2f6b1904a7c3e8d2b5f0a9c1e4d7b63a28',
        threads: [thread(1, { line_sha: 'd41c8e2' })],
      },
    })

    expect(panel.find('.thread').text()).not.toContain('measured at')
  })
})
