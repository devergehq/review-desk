import { resolve } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { readAddresses } from './harness'

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

const addresses = readAddresses()

const { seeded, empty, reviewId, waivedReviewId, structuredReviewId } = addresses

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
    await expect(page.getByRole('heading', { name: '2 reviews need you' })).toBeVisible()
    await shot(page, 'seeded-1-queue')

    await page.goto(`${seeded}reviews/${reviewId}/understand`)
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

    /*
     * **The queue at a phone's width**, which DEV-876 asks for by name: "Both pages are
     * usable at 390 pixels wide." The assertion that nothing scrolls sideways is in the
     * acceptance suite; this is the picture of it.
     */
    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(seeded)
    await expect(page.getByRole('heading', { name: '2 reviews need you' })).toBeVisible()
    await shot(page, 'seeded-6-queue-390')
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * The third state of layer 2, which no screenshot could show before DEV-864: a brief
     * the session recorded as needing no confirmation. It is the one review state that is
     * settled without anybody having signed it, and the card says which — a developer
     * looking at this screen has to be able to tell it from one they confirmed
     * themselves.
     */
    await page.goto(`${seeded}reviews/${waivedReviewId}/understand`)
    await expect(page.getByText('Confirmation not required.')).toBeVisible()
    await shot(page, 'seeded-5-review-no-confirmation')
  })
})

/*
 * **The three step pages DEV-885 builds**, at the width of a laptop and at the width of a
 * phone, from the contract's invented example.
 *
 * `confirmBrief.response.real-length.json` is what the seed feeds the binary, so these are
 * pictures of the real pages reading a real record at the length the approved boards were
 * drawn at: three numbered problems, flows of five and six rows with a split in each, eight
 * options from two sources, six parts over twenty-six files, and thirteen findings joined
 * to those parts by file.
 *
 * **Nothing here comes from the reference boards.** They hold a real review of a client's
 * private repository, and nothing from them is committed to this repository — which is why
 * the example was invented at the same lengths in the first place.
 */
test.describe('the three step pages', () => {
  test('Understand, Alternatives and Solution', async ({ page }) => {
    const steps = ['understand', 'alternatives', 'solution'] as const

    for (const [at, step] of steps.entries()) {
      await page.goto(`${seeded}reviews/${structuredReviewId}/${step}`)
      await expect(page.locator('h1')).toBeVisible()
      /*
       * One fold open on each, because a fold is half of what these pages are for —
       * Patrick wants to "go down the rabbit holes when I need to", and a picture of three
       * pages with everything closed shows only the half that is always visible.
       */
      await page.locator('button[aria-expanded="false"]').first().click()
      await shot(page, `dev-885-${at + 1}-${step}`)
    }

    /*
     * And at a phone's width. The assertion that nothing scrolls sideways is in the
     * acceptance suite; these are the pictures of what it looks like when it does not —
     * the six-column matrix as a card per option, and the four-column tables as labelled
     * lists.
     */
    await page.setViewportSize({ width: 390, height: 844 })
    for (const [at, step] of steps.entries()) {
      await page.goto(`${seeded}reviews/${structuredReviewId}/${step}`)
      await expect(page.locator('h1')).toBeVisible()
      await page.locator('button[aria-expanded="false"]').first().click()
      await shot(page, `dev-885-${at + 1}-${step}-390`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * The one state the three pages have that the invented example cannot show: a brief
     * written before the structured one existed. The seeded review's own brief is three
     * free-text fields, and the criterion is that Understand still shows it.
     */
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.getByText('Written before the structured brief.')).toBeVisible()
    await shot(page, 'dev-885-4-understand-old-brief')
  })
})

/*
 * **Step 5 and step 6, which DEV-878 builds**, from the same invented example — at the
 * width of a laptop and at the width of a phone, closed and with the blocker open.
 *
 * The open one is the point. Board A8 is a page of one-line rows, and the picture that
 * matters is what happens when a reader presses one: the code with the comment on the
 * flagged line, the proof, and the reasoning behind its own word count. A picture of
 * fourteen closed rows would show the half that is always visible and none of the half
 * Patrick said he loved.
 */
test.describe('the code review page and the draft', () => {
  test('step 5 and step 6, closed and open, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)
      await expect(page.locator('h1')).toBeVisible()
      await shot(page, `dev-878-1-code-review${suffix}`)

      /*
       * The blocker, open, with its reasoning unfolded — every part of the open row in one
       * picture, which is what "the code with the comments, the inline style" is about.
       */
      await page.locator('.row').filter({ hasText: 'F9' }).first()
        .getByRole('button').first().click()
      await expect(page.locator('.code')).toBeVisible()
      await page.getByRole('button', { name: /The full reasoning/ }).click()
      await expect(page.locator('.blocks')).toBeVisible()
      await shot(page, `dev-878-2-finding-open${suffix}`)

      await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
      await expect(page.locator('h1')).toBeVisible()
      await shot(page, `dev-878-3-draft${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * The two states of the page that the real-length example cannot show at once: a
     * review whose step 5 has not written anything, and a draft written as one block of
     * markdown rather than in parts — which is every draft written before the composition
     * existed, and the one the per-finding decisions refuse against.
     */
    await page.goto(`${seeded}reviews/${reviewId}/code-review`)
    await page.locator('.row').first().getByRole('button').first().click()
    await expect(page.getByText('one block of markdown')).toBeVisible()
    await shot(page, 'dev-878-4-one-block-draft')
  })
})

test.describe('a fresh database', () => {
  test('the four pages', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await shot(page, 'fresh-1-queue')

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
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.getByText('A webhook delivery that fails once')).toBeVisible()

    await page.route('**/api/v1/**', route => route.abort('connectionrefused'))
    await page.reload()
    await expect(page.getByText('Review Desk is not answering')).toBeVisible()

    await shot(page, 'server-stopped')
  })
})
