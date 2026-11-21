import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test } from '@playwright/test'

import { cli, fresh, scratch, seedReadyDraft, serve, stop, type Serving } from './harness'

/*
 * **DEV-1330: the header's *Answer F1* lands on that reply, and the three answers work.**
 *
 * The one state in which the record takes *Raise it again*, *Accept their reason* or
 * *Withdraw*: a finding carried into a later round, settled there, on a draft that still
 * takes decisions — with the author's reply the newest thing on it. Seeded through the
 * binary's own commands and nothing else:
 *
 * 1. Round 1: `seedReadyDraft` — a review, one finding, a ready draft — approved through the
 *    API as the developer would, and reported posted with `draft posted`.
 * 2. `review open` on a new head commit, which on a pull request somebody has reviewed starts
 *    round 2. (On an unposted round it moves the head instead: `head_moved`.)
 * 3. `finding carry` F1 into round 2 as `fixed`, decided by the author.
 * 4. `outcome add` the author's reply on round 2's F1.
 * 5. `draft put` round 2's draft, which marks it ready, then sent back through the API — a
 *    draft that still takes decisions and is not *Draft ready*, so the review reads *Replies
 *    in* and its one action is *Answer F1*.
 *
 * Its own database, for the reason `what-happened.spec.ts` gives: a review with an author's
 * outcome is a row in the queue's groups, and the shared seed's counts are asserted elsewhere.
 */

const REPLY = 'The export is chunked upstream now, so this path no longer runs on a deleted tenant.'

let serving: Serving
let round2: number

/** A developer's write, through the binary's own API, as the dashboard sends it. */
async function post(path: string, body: unknown) {
  const answer = await fetch(`${serving.url}api/v1${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'accept': 'application/json' },
    body: JSON.stringify(body),
  })
  expect(answer.status, `POST ${path}: ${await answer.clone().text()}`).toBeLessThan(300)
}

test.beforeAll(async () => {
  const directory = scratch()
  const database = fresh(directory)
  serving = await serve(database)

  const { reviewId: round1, pr } = seedReadyDraft(database, 'Give a scheduled export its own context')
  await post(`/reviews/${round1}/draft/approve`, { event: 'request_changes' })
  cli(database, ['draft', 'posted', '--review', String(round1), '--github-review', 'PRR_kwDOround1'])

  round2 = Number(cli(database, [
    'review', 'open', '--repo', 'acme/portal', '--pr', String(pr), '--head', 'c4f0a11',
  ]).trim())
  expect(round2, 'a new head on a posted pull request opens round 2').not.toBe(round1)

  cli(database, [
    'finding', 'carry', '--review', String(round2), '--from', String(round1), '--finding', '1',
    '--status', 'fixed', '--by', 'author', '--commit', 'c4f0a11',
  ])
  cli(database, [
    'outcome', 'add', '--review', String(round2), '--finding', '1', '--kind', 'replied',
    '--by', 'author', '--detail', REPLY,
  ])

  const sections = join(directory, 'round-2-sections.json')
  writeFileSync(sections, JSON.stringify([
    { kind: 'prose', text: '**Round 2.** One finding carried in, which the author says is fixed.' },
  ]))
  cli(database, ['draft', 'put', '--review', String(round2), '--sections', sections])
  await post(`/reviews/${round2}/draft/send-back`, { note: 'Hold it while I answer the reply on F1.' })
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

test('Answer F1 selects the reply on Replies, and W withdraws it in the record', async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 1000 })
  await page.goto(`${serving.url}reviews/${round2}`)

  /* The header offers the answer, because the record would take it. */
  const answer = page.getByRole('link', { name: 'Answer F1' })
  await expect(answer).toBeVisible()
  await answer.click()
  await expect(page).toHaveURL(new RegExp(`/reviews/${round2}/replies\\?finding=F1$`))

  /* The F1 reply is selected, quoted and attributed, with the three answers on it. */
  const row = page.locator('#reply-F1')
  await expect(row).toHaveAttribute('aria-current', 'true')
  await expect(row).toBeInViewport()
  await expect(row.locator('.quote')).toHaveText(REPLY)
  await expect(row.locator('.who')).toContainText('the author')
  const answers = row.getByRole('group', { name: 'How to answer the author on F1' })
  for (const label of ['Raise it again', 'Accept their reason', 'Withdraw']) {
    await expect(answers.getByRole('button', { name: label })).toBeEnabled()
  }

  /* One key sends the decision. */
  await page.locator('body').click({ position: { x: 5, y: 5 } })
  await page.keyboard.press('w')
  await expect(page.locator('main')).toContainText('F1 is withdrawn')
  await expect(page.locator('.reply-row--contested')).toHaveCount(0)
  await expect(page.locator('.reply-row--settled#reply-F1')).toBeVisible()

  /* And the record says so: F1 is withdrawn this round, by the developer. */
  const record = await page.request.get(`${serving.url}api/v1/reviews/${round2}`)
  const detail = await record.json() as { findings: Array<{ seq: number, round_status: string }>, outcomes: Array<{ finding_seq: number, kind: string, actor: string }> }
  expect(detail.findings.find(one => one.seq === 1)?.round_status).toBe('withdrawn')
  expect(detail.outcomes.some(one => one.finding_seq === 1 && one.kind === 'withdrawn' && one.actor === 'developer'))
    .toBe(true)

  /* With nothing left to answer, the header offers nothing. */
  await expect(page.getByRole('link', { name: /^Answer / })).toHaveCount(0)
})
