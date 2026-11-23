import { expect, test } from '@playwright/test'

import { fakeAllele, type FakeAllele } from './fakeAllele'
import { fresh, scratch, seedReadyDraft, serve, signedIn, stop, type Serving } from './harness'

/*
 * **A question asked on the page is handed to the session through Allele.** DEV-1383.
 *
 * One review whose session is linked, served beside a fake Allele that grants `prompt` and
 * answers `sessions_prompt` with `prompted` (`./fakeAllele.ts`). The page asks; the server
 * hands Allele a notice off the request; the next read of the conversation says so.
 *
 * Its own database, because it writes a question, and the seeded server's specs count what
 * is on theirs.
 */

let allele: FakeAllele
let serving: Serving
let reviewId: number

test.use({ viewport: { width: 1600, height: 900 } })

test.beforeAll(async () => {
  const database = fresh(scratch())
  reviewId = seedReadyDraft(database, 'Handed over through Allele').reviewId
  allele = await fakeAllele('idle', { prompt: true })
  serving = await serve(database, { allele: allele.socket })
})

test.afterAll(async () => {
  await stop(serving)
  await allele.close()
})

test('an ask is handed to the session as a notice, and the line says so', async ({ page }) => {
  await signedIn(page.context(), serving)
  await page.goto(`${serving.url}reviews/${reviewId}`)
  const panel = page.locator('[data-testid="conversation-panel"]')
  await expect(panel).toHaveAttribute('data-mode', 'two_way')

  const question = 'Is the attempt count safe on a retry?'
  await panel.locator('textarea').fill(question)
  await panel.getByRole('button', { name: 'Ask', exact: true }).click()
  const asked = panel.locator('[data-author="developer"]').last()
  await expect(asked).toContainText(question)

  /* The notice points at the record, and never carries the developer's words. */
  await expect.poll(() => allele.prompts().length).toBe(1)
  const [handed] = allele.prompts()
  expect(handed!.session_id).toBe('ses-dev-932')
  expect(handed!.prompt).toMatch(new RegExp(
    `^Review Desk: the developer asked a question on review ${reviewId} \\(message \\d+, on the \\w+ tab\\)`,
  ))
  expect(handed!.prompt).toContain(`review-desk message list --unread --review ${reviewId}`)
  expect(handed!.prompt).not.toContain(question)

  /* The panel reads the conversation again every few seconds, and then draws the handover. */
  await expect(asked.locator('.delivery')).toHaveText(/^Not read yet · handed to the session (just now|\d+ sec ago)$/)
})
