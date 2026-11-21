import { expect, test } from '@playwright/test'

import { cli, fresh, scratch, seedRoundTwoDraft, serve, signedIn, stop, type Serving } from './harness'

/*
 * **DEV-1330: the header's *Answer F9* lands on that reply, and the three answers work.**
 *
 * The one state in which the record takes *Raise it again*, *Accept their reason* or
 * *Withdraw*: a finding carried into a later round, settled there, on a draft that still
 * takes decisions — with the author's reply the newest thing on it. Seeded through the
 * binary's own commands and API and nothing else, **on DEV-1329's `seedRoundTwoDraft`**
 * rather than a second round-2 seeder: its round 2 carries F9 in as `fixed`, so this adds
 *
 * 1. `outcome add` the author's reply on round 2's F9, and
 * 2. sends round 2's ready draft back through the API — a draft that still takes decisions
 *    and is not *Draft ready*, so the review reads *Replies in* and its action is *Answer F9*.
 *
 * Its own database, for the reason `what-happened.spec.ts` gives: a review with an author's
 * outcome is a row in the queue's groups, and the shared seed's counts are asserted elsewhere.
 */

const REPLY = 'Fixed in 3c1e2a9: the policy now runs when no columns are set, with a test for the empty list.'

let serving: Serving
let round2: number

/**
 * A developer's write, through the binary's own API, as the dashboard sends it — **signed in**,
 * with this server's developer cookie (DEV-1346: only the developer writes, from the dashboard).
 */
async function post(path: string, body: unknown) {
  const answer = await fetch(`${serving.url}api/v1${path}`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'accept': 'application/json',
      'cookie': `rd_developer=${serving.cookie}`,
    },
    body: JSON.stringify(body),
  })
  expect(answer.status, `POST ${path}: ${await answer.clone().text()}`).toBeLessThan(300)
}

test.beforeAll(async () => {
  const database = fresh(scratch())
  serving = await serve(database)

  round2 = seedRoundTwoDraft(database).reviewId
  cli(database, [
    'outcome', 'add', '--review', String(round2), '--finding', '9', '--kind', 'replied',
    '--by', 'author', '--detail', REPLY,
  ])
  /*
   * **Sent back over the draft as served**: every sign-off and send-back carries `shown`, the
   * hashes of the document the developer saw (DEV-1347), which is what the Draft page sends.
   */
  const draft = await fetch(`${serving.url}api/v1/reviews/${round2}/draft`, {
    headers: { accept: 'application/json', cookie: `rd_developer=${serving.cookie}` },
  })
  const { shown } = await draft.json() as { shown: unknown }
  await post(`/reviews/${round2}/draft/send-back`, { note: 'Hold it while I answer the reply on F9.', shown })
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

test('Answer F9 selects the reply on Replies, and W withdraws it in the record', async ({ page }) => {
  /* This server's developer, not the shared seed's: cookies ignore the port. */
  await signedIn(page.context(), serving)
  await page.setViewportSize({ width: 1600, height: 1000 })
  await page.goto(`${serving.url}reviews/${round2}`)

  /* The header offers the answer, because the record would take it. */
  const answer = page.getByRole('link', { name: 'Answer F9' })
  await expect(answer).toBeVisible()
  await answer.click()
  await expect(page).toHaveURL(new RegExp(`/reviews/${round2}/replies\\?finding=F9$`))

  /* The F9 reply is selected, quoted and attributed, with the three answers on it. */
  const row = page.locator('#reply-F9')
  await expect(row).toHaveAttribute('aria-current', 'true')
  await expect(row).toBeInViewport()
  await expect(row.locator('.quote')).toHaveText(REPLY)
  await expect(row.locator('.who')).toContainText('the author')
  const answers = row.getByRole('group', { name: 'How to answer the author on F9' })
  for (const label of ['Raise it again', 'Accept their reason', 'Withdraw']) {
    await expect(answers.getByRole('button', { name: label })).toBeEnabled()
  }

  /* One key sends the decision. */
  await page.locator('body').click({ position: { x: 5, y: 5 } })
  await page.keyboard.press('w')
  await expect(page.locator('main')).toContainText('F9 is withdrawn')
  await expect(page.locator('.reply-row--contested')).toHaveCount(0)
  await expect(page.locator('.reply-row--settled#reply-F9')).toBeVisible()

  /* And the record says so: F9 is withdrawn this round, by the developer. */
  const record = await page.request.get(`${serving.url}api/v1/reviews/${round2}`)
  const detail = await record.json() as { findings: Array<{ seq: number, round_status: string }>, outcomes: Array<{ finding_seq: number, kind: string, actor: string }> }
  expect(detail.findings.find(one => one.seq === 9)?.round_status).toBe('withdrawn')
  expect(detail.outcomes.some(one => one.finding_seq === 9 && one.kind === 'withdrawn' && one.actor === 'developer'))
    .toBe(true)

  /* With nothing left to answer, the header offers nothing. */
  await expect(page.getByRole('link', { name: /^Answer / })).toHaveCount(0)
})
