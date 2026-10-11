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

    /*
     * **The code review page, which is what "the review" is in this set now.**
     *
     * This pointed at `/reviews/{id}` and was repointed to `/understand` when that address
     * became a redirect — which made it a byte-identical copy of
     * `dev-885-4-understand-old-brief.png`, since the seeded review's brief is an old-style
     * one. Two names for one picture, and neither of them showed a finding. Step 5 is the
     * page this set was missing.
     */
    await page.goto(`${seeded}reviews/${reviewId}/code-review`)
    await expect(page.getByText('A retried delivery is sent without')).toBeVisible()
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
 * options from two sources, seven parts over twenty-seven files, and fifteen findings —
 * thirteen joined to those parts by file and two belonging to no part at all.
 *
 * **Nothing here comes from the reference boards.** They hold a real review of a client's
 * private repository, and nothing from them is committed to this repository — which is why
 * the example was invented at the same lengths in the first place.
 */
/**
 * A fold in the page's own body, which is to say not the step trail's.
 *
 * DEV-892 gave a long step note a control that opens it, and the trail is above everything
 * — so "the first closed fold on the page" became the trail's. These pictures are of the
 * pages, and the trail has one of its own.
 */
const BODY_FOLD = 'button[aria-expanded="false"]:not(.rest)'

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
       *
       * **Not the trail's**, which since DEV-892 is the first fold on the page in document
       * order. It has a picture of its own below; what these three are about is the fold in
       * the page's own body.
       */
      await page.locator(BODY_FOLD).first().click()
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
      await page.locator(BODY_FOLD).first().click()
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
 * **What the last two rounds fixed, where the fix is the picture.**
 *
 * Some of these defects are things a reader sees and a test can only approximate: a step's
 * one line with its facts on a card over the page, a matrix headed by numbers and two-word
 * labels instead of by sentences, and a grid of part cards whose heads all read the same way
 * and whose rows line up. The retaken `dev-885` pictures above show them in place; these
 * show what opens.
 */
test.describe('what DEV-892 and DEV-917 fixed', () => {
  test('a step’s card opened, and every number said in full', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()

    /*
     * Step 2's detail is 34 words on the seeded record, drawn on one ellipsised line.
     * Pressing that line opens the card of facts over the page — which is the picture:
     * nothing below it has moved, and the step's duration is on the card rather than on
     * the trail.
     */
    await page.locator('.steps li').nth(1).locator('.outcome').click()
    await expect(page.locator('.floating')).toBeVisible()
    await shot(page, 'dev-917-1-step-card-open')

    await page.goto(`${seeded}reviews/${structuredReviewId}/alternatives`)
    await expect(page.locator('h1')).toBeVisible()
    /*
     * The matrix headed by its labels, with the fold under it open: three numbers, their
     * labels and the sentences that used to be the headings.
     */
    await page.locator('details.numbers summary').click()
    await shot(page, 'dev-892-2-numbers-in-full')
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

/*
 * **"What was built" as a table of rows, which is DEV-919.**
 *
 * Three pictures, because the thing that changed is what a reader does with the section
 * rather than how one row looks: the table closed, so the seven rows read down a column
 * under their three group headings; one row open, which is where the summary, the findings
 * and the files fold live; and the same at a phone's width, where six columns become a
 * stack.
 *
 * The cards these replace carried the part's whole summary — 33 to 52 words each on the
 * review this was measured against — and reprinted the files on one side and the findings
 * on the other. A picture of the table closed is the measurement.
 */
test.describe('the solution as a table', () => {
  test('the rows, one open, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/solution`)
      await expect(page.locator('h1')).toBeVisible()
      await expect(page.locator('.part')).toHaveCount(7)
      await shot(page, `dev-919-1-table${suffix}`)

      /*
       * One row open, with its files fold open too: the one picture that shows everything
       * a row holds — the full summary, a line per finding, and the paths behind the fold.
       */
      await page.getByRole('button', { name: /Allowed columns on the report definition/ }).click()
      await expect(page.locator('#part-2')).toBeVisible()
      await page.locator('#part-2').locator('summary').click()
      await expect(page.locator('#part-2').locator('details[open]')).toHaveCount(1)
      await shot(page, `dev-919-2-row-open${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })
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
