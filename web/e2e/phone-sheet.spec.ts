import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { fakeAllele, type FakeAllele } from './fakeAllele'
import { cli, cliJson, fresh, scratch, seedReadyDraft, serve, signedIn, stop, type Serving } from './harness'

/*
 * **On a phone the conversation is a bottom sheet over the review** — DEV-1336, S14.
 *
 * At 390 wide the shell is one column and the conversation is not beside anything: the bar's
 * *Ask* opens it as a sheet over the dimmed review. The sheet is the shell's own
 * `#conversation`, which the page teleports its one `ConversationPanel` into — so asking
 * from the sheet is asking from the panel, two way beside the fake Allele, as
 * `answering.spec.ts` does at 1,600.
 *
 * Its own database, because it writes a question, and the seeded server's specs count what
 * is on theirs.
 */

let allele: FakeAllele
let twoWay: Serving
let reviewId: number
let database: string
let directory: string

test.use({ viewport: { width: 390, height: 844 } })
test.describe.configure({ mode: 'serial' })

test.beforeAll(async () => {
  directory = scratch()
  database = fresh(directory)
  reviewId = seedReadyDraft(database, 'Asked from a phone').reviewId
  allele = await fakeAllele('idle')
  twoWay = await serve(database, { allele: allele.socket })
})

test.afterAll(async () => {
  await stop(twoWay)
  await allele.close()
})

/** How far the document scrolls sideways. Nought is the only right answer. */
const sideways = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)

async function openReview(page: Page, path = '') {
  await signedIn(page.context(), twoWay)
  await page.goto(`${twoWay.url}reviews/${reviewId}${path}`)
  await page.waitForLoadState('networkidle')
}

test('Ask opens the sheet, and a question asked in it is kept in the thread', async ({ page }) => {
  await openReview(page)

  const ask = page.getByRole('button', { name: 'Ask', exact: true }).and(page.locator('.topbar .ask'))
  const sheet = page.locator('#conversation')
  await expect(ask).toHaveAttribute('aria-expanded', 'false')
  await expect(sheet).toBeHidden()

  await ask.click()
  await expect(ask).toHaveAttribute('aria-expanded', 'true')
  await expect(sheet).toBeVisible()
  await expect(sheet).toHaveAttribute('role', 'dialog')
  await expect(sheet.getByRole('button', { name: 'Close the conversation' })).toBeFocused()

  /* The same panel the column holds, and only one of it: the sheet hosts it, not a copy. */
  await expect(page.locator('[data-testid="conversation-panel"]')).toHaveCount(1)
  const panel = sheet.locator('[data-testid="conversation-panel"]')
  await expect(panel).toHaveAttribute('data-mode', 'two_way')
  await expect(panel.getByRole('heading', { name: 'Ask this review' })).toBeVisible()
  await expect(panel.getByTestId('answering')).toHaveText('The session that wrote this review · idle · via Allele')

  /* Over a dimmed review the developer cannot reach until it closes. */
  await expect(page.locator('.scrim')).toBeVisible()
  await expect(page.locator('main#main')).toHaveAttribute('inert', '')

  /* The sheet meets the window's foot, and the composer is the last thing in it. */
  const box = (await sheet.boundingBox())!
  expect(Math.round(box.x)).toBe(0)
  expect(Math.round(box.width)).toBe(390)
  expect(Math.round(box.y + box.height)).toBe(844)
  const composer = (await panel.locator('form.compose').boundingBox())!
  expect(Math.round(composer.y + composer.height)).toBe(844)

  await panel.locator('textarea').fill('Does it also happen on the web path?')
  await panel.getByRole('button', { name: 'Ask', exact: true }).click()
  await expect(panel.locator('[data-author="developer"]')).toContainText('Does it also happen on the web path?')
  expect(await sideways(page), 'the page scrolls sideways with the sheet open').toBe(0)

  /* The chevron closes it and gives focus back to Ask; the question is still there next time. */
  await sheet.getByRole('button', { name: 'Close the conversation' }).click()
  await expect(sheet).toBeHidden()
  await expect(ask).toBeFocused()
  await expect(page.locator('main#main')).not.toHaveAttribute('inert')
  await ask.click()
  await expect(panel.locator('[data-author="developer"]')).toContainText('Does it also happen on the web path?')
})

test('Escape, the dimmed review, going somewhere and widening each close it', async ({ page }) => {
  await openReview(page, '/findings')
  const ask = page.locator('.topbar .ask')
  const sheet = page.locator('#conversation')

  await ask.click()
  await expect(sheet).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(sheet).toBeHidden()
  await expect(ask).toBeFocused()

  await ask.click()
  /* Above the sheet's top edge, where only the scrim is. */
  await page.mouse.click(195, 60)
  await expect(sheet).toBeHidden()

  await ask.click()
  await page.goto(`${twoWay.url}reviews/${reviewId}/draft`)
  await page.waitForLoadState('networkidle')
  await expect(sheet).toBeHidden()

  /* Turned to landscape with it open: it closes, and the column is back beside the page. */
  await ask.click()
  await expect(sheet).toBeVisible()
  await page.setViewportSize({ width: 844, height: 390 })
  await expect(page.locator('main#main')).not.toHaveAttribute('inert')
  await expect(sheet).not.toHaveAttribute('role', 'dialog')
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(ask).toHaveAttribute('aria-expanded', 'false')
  await expect(sheet).toBeHidden()
})

/*
 * **The sheet opens on the newest message.** The panel is mounted while the sheet is still
 * `display: none`, when its thread has no height to scroll; it scrolls to the newest when
 * it is shown, so an answer's suggestion is in reach of a thumb rather than below the fold.
 */
test('opened, the sheet shows the newest answer and its suggestion', async ({ page }) => {
  const listed = cliJson(database, ['message', 'list', '--review', String(reviewId)]) as unknown as {
    messages: Array<{ seq: number, author: string }>
  }
  const asked = Math.max(...listed.messages.filter(one => one.author === 'developer').map(one => one.seq))
  const answer = join(directory, 'phone-answer.json')
  writeFileSync(answer, JSON.stringify({
    kind: 'answer',
    headline: 'Yes: the web path calls the same delivery job, so a failure there counts twice as well.',
    points: [
      { basis: 'read', text: 'WebhookController.php:30 dispatches the same DeliverWebhook job with the same retry policy.' },
      { basis: 'read', text: 'DeliverWebhook.php:73 increments the count before the request, outside the transaction.' },
      { basis: 'read', text: 'The audit row at DeliverWebhook.php:88 is written from the count, so it is ahead too.' },
    ],
    cites: ['app/Jobs/DeliverWebhook.php:73', 'app/Http/Controllers/WebhookController.php:30'],
    not_sure: 'Did not check the queue worker that replays dead letters, which may be a third path.',
    suggests: [{ type: 'new_finding', severity: 'should', title: 'The web path counts a failed attempt twice', where: 'app/Http/Controllers/WebhookController.php:30', basis: 'read' }],
  }))
  cli(database, ['message', 'reply', '--review', String(reviewId), '--step', 'draft', '--to', String(asked), '--answer', answer])

  await openReview(page)
  await page.locator('.topbar .ask').click()
  const panel = page.locator('#conversation [data-testid="conversation-panel"]')
  await expect(panel.getByText('Yes: the web path calls the same delivery job')).toBeVisible()
  await expect(panel.getByTestId('accept-new_finding')).toBeInViewport()
  const thread = await panel.locator('.body').evaluate(el => ({ top: el.scrollTop, room: el.scrollHeight - el.clientHeight }))
  expect(thread.room, 'the thread overflows, so the scroll is a real test').toBeGreaterThan(0)
  expect(Math.round(thread.top)).toBe(thread.room)
})
