import { expect, test } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { fakeAllele, type FakeAllele } from './fakeAllele'
import { cli, fresh, scratch, seedReadyDraft, serve, signedIn, stop, type Serving } from './harness'

/*
 * **Who is answering, through the binary, in a browser.** DEV-1335.
 *
 * One review, one database of its own, served twice: once beside a fake Allele
 * (`./fakeAllele.ts`), which is two way, and once with no Allele at all. Before its session
 * writes anything the second is read only; after `review-desk message reply`, one way. The
 * mode is read off the socket and the record, so serving the same record two ways is the
 * whole test of it.
 *
 * Its own database, because it writes a question and a reply, and the seeded server's specs
 * count what is on theirs.
 */

let allele: FakeAllele
let twoWay: Serving
let noAllele: Serving
let reviewId: number
let directory: string

test.use({ viewport: { width: 1600, height: 900 } })
test.describe.configure({ mode: 'serial' })

test.beforeAll(async () => {
  directory = scratch()
  const database = fresh(directory)
  reviewId = seedReadyDraft(database, 'Answering, three ways').reviewId
  allele = await fakeAllele('idle')
  ;[twoWay, noAllele] = await Promise.all([serve(database, { allele: allele.socket }), serve(database)])
})

test.afterAll(async () => {
  await Promise.all([stop(twoWay), stop(noAllele)])
  await allele.close()
})

test('with Allele, the question is asked here and kept in the thread', async ({ page }) => {
  await signedIn(page.context(), twoWay)
  await page.goto(`${twoWay.url}reviews/${reviewId}`)
  const panel = page.locator('[data-testid="conversation-panel"]')
  await expect(panel).toHaveAttribute('data-mode', 'two_way')
  await expect(panel.getByRole('heading', { name: 'Ask this review' })).toBeVisible()
  await expect(panel.getByTestId('answering')).toHaveText('The session that wrote this review · idle · via Allele')
  expect(allele.asked()).toBeGreaterThan(0)

  await panel.locator('textarea').fill('Is the attempt count safe on a retry?')
  await panel.getByRole('button', { name: 'Ask', exact: true }).click()
  await expect(panel.locator('[data-author="developer"]')).toContainText('Is the attempt count safe on a retry?')
})

test('an unread question says when the session last checked, and when that was too long ago', async ({ page }) => {
  /*
   * DEV-1381. The question the test above asked is still unread. Nothing has checked yet;
   * then a watcher's pass, the real command, stamps the review; then the browser's clock is
   * twenty minutes on, past the fifteen-minute limit, without touching the record.
   */
  await signedIn(page.context(), twoWay)
  await page.goto(`${twoWay.url}reviews/${reviewId}`)
  const delivery = page.locator('[data-testid="conversation-panel"] [data-author="developer"] .delivery').first()
  await expect(delivery).toHaveText('Not read yet · nothing has checked for messages on this review')

  cli(twoWay.database, ['message', 'list', '--unread', '--review', String(reviewId)])
  await page.reload()
  await expect(delivery).toHaveText(/^Not read yet · the session last checked (just now|\d+ sec ago)$/)

  await page.clock.install({ time: new Date(Date.now() + 20 * 60 * 1000) })
  await page.reload()
  await expect(delivery).toHaveText(
    /^Not read yet · the session hasn't checked for messages in (19|20|21) min, so its watcher may have stopped$/,
  )
})

test('with no Allele and nothing written by a session, it is read only', async ({ page }) => {
  await signedIn(page.context(), noAllele)
  await page.goto(`${noAllele.url}reviews/${reviewId}`)
  const panel = page.locator('[data-testid="conversation-panel"]')
  await expect(panel).toHaveAttribute('data-mode', 'none')
  await expect(panel.getByTestId('answering')).toHaveText('No agent connected · read only')
  await expect(panel.locator('textarea')).toBeDisabled()
  await expect(panel.getByTestId('fresh-session')).toContainText(`review-desk message list --review ${reviewId}`)
})

test('once a session writes with message reply, it is one way and nothing can be sent', async ({ page }) => {
  const answer = join(directory, 'one-way-answer.json')
  writeFileSync(answer, JSON.stringify({
    kind: 'answer',
    headline: 'Yes: the count is written inside the retry’s transaction now.',
    cites: ['app/Jobs/DeliverWebhook.php:73'],
    asked: 'Is the attempt count safe on a retry?',
  }))
  cli(noAllele.database, ['message', 'reply', '--review', String(reviewId), '--step', 'draft', '--answer', answer])

  await signedIn(page.context(), noAllele)
  await page.goto(`${noAllele.url}reviews/${reviewId}`)
  const panel = page.locator('[data-testid="conversation-panel"]')
  await expect(panel).toHaveAttribute('data-mode', 'one_way')
  await expect(panel.getByRole('heading', { name: 'From your session' })).toBeVisible()
  await expect(panel.getByTestId('answering')).toHaveText('Your own Claude Code session · one way · no Allele')
  await expect(panel.getByText('Your question, as your session recorded it')).toBeVisible()
  await expect(panel.getByText('Yes: the count is written inside the retry’s transaction now.')).toBeVisible()
  await expect(panel.locator('textarea')).toHaveCount(0)
  await expect(panel.getByRole('button', { name: 'Ask', exact: true })).toHaveCount(0)
  await expect(panel.getByRole('button', { name: 'Copy a reference' })).toBeVisible()
  await expect(page.locator('#sidebar')).toContainText('Your own session · one way')
})
