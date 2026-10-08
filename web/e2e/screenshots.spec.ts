import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { ADDRESSES } from './harness'

/*
 * The screenshots in `docs/screenshots/`, produced rather than taken by hand.
 *
 * **They come off the same servers the end-to-end test drives**, which is the point: a
 * screenshot taken by hand is a claim about what the app looked like once, and one produced
 * by this file is the same database, the same binary and the same browser the test asserts
 * against. If a screen changes, re-running this changes its picture, and the pull request
 * shows both.
 *
 *     pnpm run e2e:screenshots
 *
 * Its own Playwright project, so a CI run of the acceptance test does not rewrite nine
 * committed files on every build.
 */

const addresses = JSON.parse(readFileSync(ADDRESSES, 'utf8')) as {
  seeded: string
  empty: string
  reviewId: number
}

const { seeded, empty, reviewId } = addresses

const INTO = resolve('../docs/screenshots')

/**
 * Take one, with the page settled first.
 *
 * `fullPage`, because these screens are tall and a viewport-height crop would cut off the
 * findings — which are the thing the One review screen is about.
 */
async function shot(page: Page, name: string) {
  /*
   * Mermaid draws the brief's diagram after the page is otherwise ready, and the fonts are
   * bundled rather than fetched so they settle quickly. Waiting for the network to go quiet
   * and then a beat is enough, and is what keeps two runs of this producing the same image.
   */
  await page.waitForLoadState('networkidle')
  await page.waitForTimeout(400)
  await page.screenshot({ path: `${INTO}/${name}.png`, fullPage: true })
}

test.describe.configure({ mode: 'serial' })

test.describe('a seeded database', () => {
  test('the four pages', async ({ page }) => {
    await page.goto(seeded)
    await expect(page.getByRole('heading', { name: 'Waiting on you' })).toBeVisible()
    await shot(page, 'seeded-1-inbox')

    await page.goto(`${seeded}reviews/${reviewId}`)
    await expect(page.getByText('Confirm the problem before anything is posted')).toBeVisible()
    await shot(page, 'seeded-2-review')

    await page.goto(`${seeded}reviews/${reviewId}/draft`)
    await expect(
      page.getByText('Nothing has been posted. This is what GitHub will show.'),
    ).toBeVisible()
    await shot(page, 'seeded-3-draft')

    await page.goto(`${seeded}rules`)
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await shot(page, 'seeded-4-rules')
  })
})

test.describe('a fresh database', () => {
  test('the four pages', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await shot(page, 'fresh-1-inbox')

    /*
     * A review and a draft that are not there. These are the same two screens as above,
     * which is why they are in the set: "all four pages in their empty states" includes the
     * two whose empty state is a refusal rather than an empty list.
     */
    await page.goto(`${empty}reviews/1`)
    await expect(page.getByText('Not on the desk')).toBeVisible()
    await shot(page, 'fresh-2-review')

    await page.goto(`${empty}reviews/1/draft`)
    await expect(page.getByText('Not on the desk')).toBeVisible()
    await shot(page, 'fresh-3-draft')

    await page.goto(`${empty}rules`)
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await shot(page, 'fresh-4-rules')
  })
})

test.describe('the server stopped', () => {
  test('with a page already open', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}`)
    await expect(page.getByText('A webhook delivery that fails once')).toBeVisible()

    await page.route('**/api/v1/**', route => route.abort('connectionrefused'))
    await page.reload()
    await expect(page.getByText('Review Desk is not answering')).toBeVisible()

    await shot(page, 'server-stopped')
  })
})
