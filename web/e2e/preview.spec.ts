import { expect, test, type Locator, type Page } from '@playwright/test'

import { fresh, scratch, seedStructuredReview, serve, signedIn, stop, type Serving } from './harness'

/*
 * **The list's preview keeps up with the pointer** — DEV-1374. Hovering a row used to wait
 * 120ms and then read the whole review again, every time, so coming back to a row drew
 * *Reading the review…* for an answer the page already had.
 *
 * In a browser, against the real server, because both claims are about requests and frames:
 * a return visit draws with every review read held back by the network, and a sweep down
 * the list asks the server for next to nothing once the tab has been read ahead.
 *
 * Its own server and twelve structured reviews, so a sweep has ten rows to cross and the
 * acceptance suite's counts are untouched.
 */

let serving: Serving

test.beforeAll(async () => {
  const database = fresh(scratch())
  for (let n = 1; n <= 12; n += 1) seedStructuredReview(database, `Preview row ${n}`)
  serving = await serve(database)
})

test.beforeEach(async ({ context }) => {
  await signedIn(context, serving)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

const REVIEW_READ = /\/api\/v1\/reviews\/\d+$/

/** The list on its *Every review* tab, with the tab read ahead and the page quiet. */
async function everyRow(page: Page): Promise<Locator> {
  await page.setViewportSize({ width: 1600, height: 1400 })
  await page.goto(`${serving.url}?view=every`)
  const rows = page.locator('#panel-every .queue-line')
  await expect(rows).toHaveCount(12)
  await page.waitForLoadState('networkidle')
  return rows
}

async function hover(page: Page, row: Locator) {
  const box = (await row.boundingBox())!
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 2 })
}

const preview = (page: Page) => page.locator('[data-testid="list-preview"]')

test('a row visited before draws at once, without Reading the review…', async ({ page }) => {
  const rows = await everyRow(page)
  const first = await rows.nth(1).locator('.pr').innerText()

  await hover(page, rows.nth(1))
  await expect(preview(page).getByTestId('bottom-line')).toBeVisible()
  await hover(page, rows.nth(2))
  await expect(preview(page)).toContainText(await rows.nth(2).locator('.pr').innerText())

  /*
   * **Every review read from here on takes five seconds**, so whatever the return visit
   * draws inside the next second can only have come from what the page already holds.
   */
  await page.route(REVIEW_READ, async (route) => {
    await new Promise(resolve => setTimeout(resolve, 5_000))
    await route.continue().catch(() => {})
  })
  await page.evaluate(() => {
    const flag = window as unknown as { sawReading: boolean }
    flag.sawReading = false
    new MutationObserver(() => {
      if (document.querySelector('[data-testid="preview-reading"]')) flag.sawReading = true
    }).observe(document.querySelector('[data-testid="list-preview"]')!.parentElement!, {
      childList: true, subtree: true, characterData: true,
    })
  })

  await hover(page, rows.nth(1))
  await expect(preview(page).locator('.number')).toHaveText(first, { timeout: 1_000 })
  await expect(preview(page).getByTestId('bottom-line')).toBeVisible({ timeout: 1_000 })
  expect(await page.evaluate(() => (window as unknown as { sawReading: boolean }).sawReading),
    'the loading line never appeared').toBe(false)
  await page.unrouteAll({ behavior: 'ignoreErrors' })
})

test('a sweep down ten rows asks the server for at most two reviews', async ({ page }) => {
  const rows = await everyRow(page)
  await hover(page, rows.nth(0))
  await page.waitForLoadState('networkidle')

  const asked: string[] = []
  page.on('request', (request) => {
    if (REVIEW_READ.test(request.url())) asked.push(request.url())
  })

  const started = Date.now()
  for (let at = 1; at <= 10; at += 1) {
    await hover(page, rows.nth(at))
    await expect(rows.nth(at), 'the highlight follows the pointer').toHaveClass(/queue-line--selected/)
  }
  expect(Date.now() - started, 'a sweep, not a stroll').toBeLessThan(1_000)

  await page.waitForTimeout(500)
  expect(asked.length, asked.join('\n')).toBeLessThanOrEqual(2)
})
