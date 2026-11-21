import { expect, test, type Page } from '@playwright/test'

import { readAddresses } from './harness'

/*
 * **The shell, measured in a browser** — DEV-1323's criteria that only a layout engine can
 * answer: every existing page renders inside the three columns at 1,600 and at 1,280 wide
 * without scrolling sideways, no element but the wordmark computes a serif face, and under
 * 700px the sidebar is behind a menu button and the conversation column is not drawn.
 *
 * Read-only against the acceptance suite's seeded server, so it adds nothing to the seed
 * the other files assert against by count.
 */

const { seeded, reviewId, structuredReviewId } = readAddresses()

/** Every page there is, by the address a reader would type. */
const PAGES = [
  { name: 'pull requests', path: '', review: false },
  { name: 'reviews', path: 'reviews', review: false },
  { name: 'rules', path: 'rules', review: false },
  { name: 'a review', path: `reviews/${reviewId}`, review: true },
  { name: 'the case', path: `reviews/${structuredReviewId}/understand`, review: true },
  { name: 'code review', path: `reviews/${reviewId}/code-review`, review: true },
  { name: 'the draft', path: `reviews/${reviewId}/draft`, review: true },
  { name: 'what happened', path: `reviews/${reviewId}/what-happened`, review: true },
]

/** The page, loaded and settled: fonts in, Mermaid drawn, nothing still reading. */
async function open(page: Page, path: string) {
  await page.goto(`${seeded}${path}`)
  await page.waitForLoadState('networkidle')
  await expect(page.locator('#sidebar .wordmark')).toBeVisible()
  await expect(page.locator('main#main')).toBeVisible()
}

/** How far the document scrolls sideways. Nought is the only right answer. */
const sideways = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)

for (const width of [1600, 1280] as const) {
  test.describe(`at ${width} wide`, () => {
    test.use({ viewport: { width, height: 900 } })

    for (const one of PAGES) {
      test(`${one.name} sits in the shell`, async ({ page }) => {
        await open(page, one.path)

        const sidebar = await page.locator('#sidebar').boundingBox()
        expect(sidebar!.x).toBe(0)
        expect(Math.round(sidebar!.width)).toBe(232)

        const conversation = page.locator('.shell-conversation')
        if (one.review) {
          await expect(conversation).toBeVisible()
          expect(Math.round((await conversation.boundingBox())!.width)).toBe(380)
        }
        else {
          await expect(conversation).toHaveCount(0)
        }

        expect(await sideways(page), 'the page scrolls sideways').toBe(0)
      })
    }
  })
}

/*
 * **No serif anywhere but the wordmark.** Asked of every element on every page, after the
 * fonts have loaded, by the face the browser resolved the element's `font-family` to first.
 * The screens spec reads the stylesheets; this reads what they came to.
 */
test('no element but the wordmark is set in a serif', async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 900 })
  for (const one of PAGES) {
    await open(page, one.path)
    await page.evaluate(() => document.fonts.ready)
    const serifs = await page.evaluate(() =>
      [...document.querySelectorAll<HTMLElement>('body *')]
        .filter((el) => {
          const first = getComputedStyle(el).fontFamily.split(',')[0]!.trim().replace(/["']/g, '')
          return /^(newsreader|georgia|serif|times)/i.test(first)
        })
        .filter(el => !el.classList.contains('wordmark'))
        .map(el => `${el.tagName.toLowerCase()}.${[...el.classList].join('.')}`))
    expect(serifs, `serif on ${one.name}`).toEqual([])
    expect(await page.locator('.wordmark').evaluate(el => getComputedStyle(el).fontFamily))
      .toMatch(/Newsreader/)
  }
})

test.describe('under 700 wide', () => {
  test.use({ viewport: { width: 390, height: 844 } })

  test('the sidebar is behind a menu button, and the conversation is not drawn', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/code-review`)
    await page.waitForLoadState('networkidle')

    const menu = page.getByRole('button', { name: 'Menu' })
    await expect(menu).toBeVisible()
    await expect(menu).toHaveAttribute('aria-expanded', 'false')
    await expect(page.locator('#sidebar')).toBeHidden()
    await expect(page.locator('.shell-conversation')).toBeHidden()

    await menu.click()
    await expect(menu).toHaveAttribute('aria-expanded', 'true')
    await expect(page.locator('#sidebar')).toBeVisible()

    /* Going somewhere is what the menu is for, so arriving closes it. */
    await page.locator('#sidebar').getByRole('link', { name: /Rules/ }).click()
    await expect(page).toHaveURL(/\/rules$/)
    await expect(page.locator('#sidebar')).toBeHidden()

    expect(await sideways(page), 'the page scrolls sideways').toBe(0)
  })
})
