import { writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { fakeAllele } from './fakeAllele'
import {
  cli,
  cliJson,
  draftStamps,
  fresh,
  openSteps,
  pinClockAt,
  readAddresses,
  scratch,
  seedReadyDraft,
  seedRoundTwoDraft,
  seedStructuredReview,
  serve,
  signedIn,
  stop,
} from './harness'

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

const {
  seeded,
  seededDatabase,
  empty,
  reviewId,
  waivedReviewId,
  structuredReviewId,
  heldBackReviewId,
  mergedReviewIds,
  movedHeadReviewId,
} = addresses

const INTO = resolve('../docs/screenshots')

/**
 * Take one, with the page settled first.
 *
 * `fullPage`, because these screens are tall and a viewport-height crop would cut off the
 * findings — which are the thing the One review screen is about.
 */
async function shot(page: Page, name: string, fullPage = true) {
  /*
   * Mermaid draws the brief's diagram after the page is otherwise ready, and the fonts are
   * bundled rather than fetched so they settle quickly. Waiting for the network to go quiet
   * and then a beat is enough, and is what keeps two runs of this producing the same image.
   */
  await page.waitForLoadState('networkidle')
  await page.waitForTimeout(400)
  await page.screenshot({ path: `${INTO}/${name}.png`, fullPage })
}

test.describe.configure({ mode: 'serial' })

test.describe('a seeded database', () => {
  test('the four pages', async ({ page }) => {
    await page.goto(seeded)
    await expect(page.getByRole('tab', { name: /^Waiting on you/ })).toBeVisible()
    await shot(page, 'seeded-1-pull-requests')

    /*
     * **The code review page, which is what "the review" is in this set now.**
     *
     * This pointed at `/reviews/{id}` and was repointed to `/understand` when that address
     * became a redirect — which made it a byte-identical copy of
     * `understanding-3-old-brief.png` (then `dev-885-4-understand-old-brief.png`), since the seeded review's brief is an old-style
     * one. Two names for one picture, and neither of them showed a finding. Step 5 is the
     * page this set was missing.
     */
    /*
     * **Scoped to the pane**, because step 1.2 of the design loop made this page a dense
     * list beside the finding it has chosen — so a finding's claim is now drawn on its row
     * and again in the pane, and the bare text matched three elements. This project is not
     * in CI, which is why that went unnoticed until the case page was built.
     *
     * **F1 by its address** (DEV-1336). This looked for `#finding-1`, the heading of the
     * finding shown — which was F1 only while the page opened on the first finding.
     * DEV-1328 opens it on the first of *To post*, so there was no such heading and the
     * wait timed out. `?finding=F1` is the address every other page links a finding with
     * (`findingPath`), and the heading is looked for inside the pane, because the pane's
     * claim repeats the title's words.
     */
    await page.goto(`${seeded}reviews/${reviewId}/findings?finding=F1`)
    await expect(
      page.locator('#the-finding #finding-1').getByText('A retried delivery is sent without'),
    ).toBeVisible()
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
     * **The Reviews list**, which DEV-930 adds: every review the record holds, including
     * the one that was posted and had left every group the queue serves. Since DEV-1325 it
     * is the Pull requests list's *Every review* view, which `/reviews` redirects to.
     */
    await page.goto(`${seeded}reviews`)
    await expect(page.getByRole('heading', { name: /\d+ reviews?$/ })).toBeVisible()
    await shot(page, 'seeded-7-every-review')

    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(`${seeded}reviews`)
    await expect(page.getByRole('heading', { name: /\d+ reviews?$/ })).toBeVisible()
    await shot(page, 'seeded-8-every-review-390')
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * **The queue at a phone's width**, which DEV-876 asks for by name: "Both pages are
     * usable at 390 pixels wide." The assertion that nothing scrolls sideways is in the
     * acceptance suite; this is the picture of it.
     */
    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(seeded)
    await expect(page.getByRole('tab', { name: /^Waiting on you/ })).toBeVisible()
    await shot(page, 'seeded-6-pull-requests-390')
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * The third state of layer 2, which no screenshot could show before DEV-864: a brief
     * the session recorded as needing no confirmation. It is the one review state that is
     * settled without anybody having signed it, and the card says which — a developer
     * looking at this screen has to be able to tell it from one they confirmed
     * themselves.
     */
    await page.goto(`${seeded}reviews/${waivedReviewId}/understanding`)
    await expect(page.getByText('Confirmation not required.')).toBeVisible()
    await shot(page, 'seeded-5-review-no-confirmation')
  })
})

/*
 * **The shell, DEV-1323**: the sidebar and the three columns, at the S screens' own width
 * and at a phone's. The list (S1's place) and a review (S2's), and on the phone the sidebar
 * opened from its menu button — the one state of the shell a closed page cannot show.
 * Viewport-high rather than full-page at 1,600, so the sidebar's foot — the GitHub and
 * agent lines — is in the picture where S1 and S2 draw it.
 */
test.describe('the shell', () => {
  test('the list and a review, at 1,600 and 390', async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 })
    await page.goto(seeded)
    await expect(page.locator('#sidebar .wordmark')).toBeVisible()
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(400)
    await page.screenshot({ path: `${INTO}/shell-1-pull-requests.png` })

    await page.goto(`${seeded}reviews/${reviewId}/findings`)
    await expect(page.locator('.shell-conversation')).toBeVisible()
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(400)
    await page.screenshot({ path: `${INTO}/shell-2-review.png` })

    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(seeded)
    await expect(page.getByRole('button', { name: 'Menu' })).toBeVisible()
    await shot(page, 'shell-1-pull-requests-390')

    await page.goto(`${seeded}reviews/${reviewId}/findings`)
    await expect(page.getByRole('button', { name: 'Menu' })).toBeVisible()
    await shot(page, 'shell-2-review-390')

    await page.getByRole('button', { name: 'Menu' }).click()
    await expect(page.locator('#sidebar')).toBeVisible()
    await page.waitForTimeout(200)
    await page.screenshot({ path: `${INTO}/shell-3-menu-open-390.png` })
    await page.keyboard.press('Escape')

    /*
     * **S14: the conversation as a bottom sheet** (DEV-1336), opened from the bar's *Ask*
     * over the dimmed review. Viewport-high: a sheet is fixed to the window, and a
     * full-page capture would draw it over the middle of a tall page. This server has no
     * Allele, so it is read only; the two-way sheet is `sheet-1-asked-390.png` below.
     */
    await page.locator('.topbar .ask').click()
    await expect(page.locator('#conversation')).toBeVisible()
    await page.waitForTimeout(200)
    await page.screenshot({ path: `${INTO}/shell-5-conversation-sheet-390.png` })
    await page.keyboard.press('Escape')

    /*
     * **S17: nothing waits on the developer** (DEV-1325). The seed always has something
     * waiting, so the inbox's three waiting arrays are emptied in the browser — the page is
     * what is pictured, and it is drawn from exactly the answer it is given.
     */
    await page.route('**/api/v1/inbox', async (route) => {
      const answer = await route.fetch()
      const inbox = await answer.json()
      await route.fulfill({
        response: answer,
        json: {
          ...inbox,
          briefs_to_confirm: [],
          drafts_to_approve: [],
          author_replies: [],
          totals: { ...inbox.totals, items: 0 },
        },
      })
    })
    await page.setViewportSize({ width: 1600, height: 1000 })
    await page.goto(seeded)
    await expect(page.getByText('Nothing needs you')).toBeVisible()
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(400)
    await page.screenshot({ path: `${INTO}/shell-4-nothing-needs-you.png` })

    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(seeded)
    await expect(page.getByText('Nothing needs you')).toBeVisible()
    await shot(page, 'shell-4-nothing-needs-you-390')
    await page.unroute('**/api/v1/inbox')
    await page.setViewportSize({ width: 1440, height: 900 })
  })
})

/*
 * **The review header and its five tabs** (DEV-1324), beside S2: Overview — the review's
 * own address — and Understanding, at 1,600 and at 390. Viewport-high at 1,600, so the
 * picture is the header and what sits under it, as S2 draws it.
 */
test.describe('the review header', () => {
  test('Overview and Understanding, at 1,600 and 390', async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 })
    for (const [tab, name] of [['', 'header-1-overview'], ['/understanding', 'header-2-understanding']] as const) {
      await page.goto(`${seeded}reviews/${structuredReviewId}${tab}`)
      await expect(page.locator('[data-testid="review-header"]')).toBeVisible()
      await page.waitForLoadState('networkidle')
      await page.waitForTimeout(400)
      await page.screenshot({ path: `${INTO}/${name}.png` })
    }

    await page.setViewportSize({ width: 390, height: 844 })
    for (const [tab, name] of [['', 'header-1-overview-390'], ['/understanding', 'header-2-understanding-390']] as const) {
      await page.goto(`${seeded}reviews/${structuredReviewId}${tab}`)
      await expect(page.locator('[data-testid="review-header"]')).toBeVisible()
      await page.waitForLoadState('networkidle')
      await page.waitForTimeout(400)
      await page.screenshot({ path: `${INTO}/${name}.png` })
    }
    await page.setViewportSize({ width: 1440, height: 900 })
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
test.describe('the case page', () => {
  /**
   * **Three pages became one on 16 October 2026, and one tab under S3** (DEV-1327), so the
   * pictures are two states of the Understanding tab: as it opens, and with everything a
   * reader can open opened. At 1,600 and 390, the widths S3 is put beside.
   *
   * The `dev-885` pictures of the three pages this replaced are gone with the pages: their
   * addresses redirect here, so a picture of one was a picture of this tab.
   */
  test('the three bands, closed and open', async ({ page }) => {
    for (const width of [1600, 390] as const) {
      await page.setViewportSize({ width, height: width === 1600 ? 1000 : 844 })
      const suffix = width === 1600 ? '' : '-390'

      await page.goto(`${seeded}reviews/${structuredReviewId}/understanding`)
      await expect(page.locator('h2.headline')).toBeVisible()
      /*
       * **As it opens.** The headline, the derived answer line, the verdict on the
       * approach, and the three bands — which is the whole point of the page and the thing
       * to compare against the three it replaced.
       */
      await expect(page.locator('.answer')).toBeVisible()
      await shot(page, `understanding-1-the-case${suffix}`)

      /*
       * **And with every fold open**, because a fold is half of what this page is for:
       * Patrick wants to "go down the rabbit holes when I need to", and a picture with
       * everything closed shows only the half that is always visible. The first row of each
       * band is opened too, so each of the three kinds of detail is in the picture.
       */
      for (const fold of await page.locator('.band > details > summary').all()) {
        await fold.click()
      }
      for (const selector of ['button.problem', 'button.option', 'button.part']) {
        await page.locator(selector).first().click()
      }
      await shot(page, `understanding-2-the-case-open${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * The one state the three pages have that the invented example cannot show: a brief
     * written before the structured one existed. The seeded review's own brief is three
     * free-text fields, and the criterion is that Understand still shows it.
     */
    await page.goto(`${seeded}reviews/${reviewId}/understanding`)
    await expect(page.getByText('Written before the structured brief.')).toBeVisible()
    await shot(page, 'understanding-3-old-brief')
  })
})

/*
 * **What the last two rounds fixed, where the fix is the picture.**
 *
 * Some of these defects are things a reader sees and a test can only approximate: a step's
 * one line with its facts on a card over the page, a matrix headed by numbers and two-word
 * labels instead of by sentences, and a grid of part cards whose heads all read the same way
 * and whose rows line up. The `understanding-` pictures above show them in place; these
 * show what opens.
 */
test.describe('what DEV-892 and DEV-917 fixed', () => {
  test('a step’s card opened, and every number said in full', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}`)
    /* The trail is folded under *Where it stands* on Overview since DEV-1326. */
    await openSteps(page)

    /*
     * Step 2's detail is 34 words on the seeded record, drawn on one ellipsised line.
     * Pressing that line opens the card of facts over the page — which is the picture:
     * nothing below it has moved, and the step's duration is on the card rather than on
     * the trail.
     */
    await page.locator('.steps li').nth(1).locator('.outcome').click()
    await expect(page.locator('.floating')).toBeVisible()
    await shot(page, 'dev-917-1-step-card-open')

    /*
     * **And no picture of "What each number is, in full" any more.** That fold was the
     * Alternatives page's answer to a matrix headed by bare numbers on a page away from the
     * sentences; on the case page the sentences are the rows of band 1, three inches above
     * the matrix and open. `dev-892-2-numbers-in-full.png` is left as the record of it.
     */
  })
})

/*
 * **Findings, S4's bench, at 1,600 and 390.** DEV-1328. These were the code review page's
 * pictures (`dev-878-1`, `-2` and `-4`); the page is the Findings tab now, so they are taken
 * at DEV-1322's two widths with the conversation column beside them at 1,600, and the old
 * names are gone with the page.
 *
 * The open one is the point: the code with the comment on the flagged line, the proof, and
 * the reasoning behind its own word count, with the call pinned at the pane's foot.
 */
test.describe('the Findings tab', () => {
  test('the list, a finding open, and a draft in one block, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1600, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 1000 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/findings`)
      await expect(page.locator('h2.headline')).toBeVisible()
      /*
       * **One window at 1,600, as S4 is**, not the whole page: the call is pinned to the
       * window's foot there, and a full-page capture freezes it wherever the first window
       * ended, over the fields. At 390 nothing is pinned and the page is taken whole.
       */
      await shot(page, `findings-1-list${suffix}`, width === 390)

      await page.goto(`${seeded}reviews/${structuredReviewId}/findings?finding=F9`)
      await expect(page.locator('.code')).toBeVisible()
      await page.getByRole('button', { name: /The full reasoning/ }).click()
      await expect(page.locator('.blocks')).toBeVisible()
      await shot(page, `findings-2-finding-open${suffix}`, width === 390)
    }

    /*
     * A draft written as one block of markdown rather than in parts — every draft written
     * before the composition existed, and the one the per-finding decisions refuse against.
     */
    await page.setViewportSize({ width: 1600, height: 1000 })
    await page.goto(`${seeded}reviews/${reviewId}/findings`)
    await page.locator('.row').first().click()
    await expect(page.getByText('one block of markdown')).toBeVisible()
    await shot(page, 'findings-3-one-block-draft', false)
  })
})

/*
 * **Step 6, which DEV-878 builds**, from the same invented example, at the width of a
 * laptop and of a phone.
 */
test.describe('the draft', () => {
  test('step 6 at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })
      await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
      await expect(page.locator('h2.headline')).toBeVisible()
      await shot(page, `dev-878-3-draft${suffix}`)
    }
  })
})

/*
 * **The Draft tab, round 1 and round 2** (DEV-1329): S05 and S16, at the laptop width the
 * targets are drawn at and at a phone's. Round 1 is the seeded structured review; round 2
 * is a second round of a posted pull request, carrying five findings with what became of
 * each.
 */
test.describe('the Draft tab', () => {
  test('round 1 and round 2, at 1,600 and 390', async ({ page }) => {
    const { reviewId: roundTwo } = seedRoundTwoDraft(seededDatabase)
    for (const [width, suffix] of [[1600, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 1000 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
      await expect(page.getByRole('button', { name: 'Sign off and post' })).toBeVisible()
      await shot(page, `draft-1-round-1${suffix}`)

      await page.goto(`${seeded}reviews/${roundTwo}/draft`)
      await expect(page.getByRole('button', { name: 'Sign off round 2 and post' })).toBeVisible()
      await expect(page.getByTestId('since-round')).toContainText('F9 fixed in 3c1e2a9')
      await shot(page, `draft-2-round-2${suffix}`)
    }
  })
})

/*
 * **What GitHub receives, in two pictures at each width.**
 *
 * DEV-929: a real review went out as 309 lines with no index and two threads. The thing
 * that changed is the shape of what posts, so the pictures are of the two halves of the
 * preview — the body, where the index now sits under the verdict line and `### Problem
 * fit`, and the threads, which are not in the body at all.
 *
 * The real-length example is twelve posted findings with nine of them anchored, so the index
 * is at a length a reader has to scan rather than read.
 */
test.describe('the posted shape, after DEV-929', () => {
  test('the index and the threads, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
      await expect(page.locator('h2.headline')).toBeVisible()
      /*
       * **The index is folded in the body since step 1.1 of the design loop**, which is a
       * `<details>` in the markdown the session wrote — the same markup GitHub renders — so
       * the table is behind its summary rather than open. The picture is of the index, so
       * the fold is opened for it.
       */
      await page.locator('.preview details > summary').first().click()
      await expect(page.locator('.preview table')).toBeVisible()
      await shot(page, `dev-929-1-index${suffix}`)

      /* The threads card, scrolled to, so the picture is of the card and not of the body. */
      await page.locator('.threads').scrollIntoViewIfNeeded()
      await shot(page, `dev-929-2-threads${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })

    /*
     * And a draft with nothing to anchor, which is a state rather than an absence: the
     * seeded review's draft is one block of markdown, so it has no block about any finding,
     * nothing to index and nothing to hang a comment on. The card says which it is.
     */
    await page.goto(`${seeded}reviews/${reviewId}/draft`)
    await expect(page.locator('.threads .rd-empty')).toBeVisible()
    await shot(page, 'dev-929-3-nothing-to-anchor')
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

      /* The tab's top, which is where `/solution` landed: the picture is the whole page. */
      await page.goto(`${seeded}reviews/${structuredReviewId}/understanding`)
      await expect(page.locator('h2.headline')).toBeVisible()
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

/**
 * A problem's Before and After lines, open. DEV-939's picture.
 *
 * **Nothing in this set showed one.** Every other screenshot of the Understand page has the
 * problems closed, which is why a line that drew as eight narrow columns went nine days
 * without appearing in a picture anybody looked at. The detail is where a problem's lines
 * live, so the detail is open here.
 *
 * Both widths, because the names are what break: the longest in the record is 46 characters,
 * which has room at a laptop's width and has to break at a phone's.
 */
test.describe('a problem’s Before and After, after DEV-939', () => {
  test('the detail open, with its code names, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })

      await page.goto(`${seeded}reviews/${structuredReviewId}/understanding`)
      await expect(page.locator('h2.headline')).toBeVisible()

      await page.getByRole('button', { name: /A scheduled export read its tenant/ }).click()
      await expect(page.locator('#problem-1')).toBeVisible()
      /* The line this is about: six code names, one of them 46 characters. */
      await expect(
        page.locator('#problem-1 .lines li').filter({ hasText: 'Six call sites read it' }),
      ).toHaveCount(1)
      await shot(page, `dev-939-1-problem-detail${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })
  })
})

/**
 * The draft page's aside, and a drawing opened larger. DEV-928's three changes, in pictures.
 *
 * **Both widths for the panel, because the complaint was about a width.** Eight findings
 * were printed as 41-to-63-word paragraphs in a column about 290 pixels wide; a picture of
 * eight rows in that column is the measurement, and the acceptance suite's own assertion
 * that nothing leaves the card is the proof. The fourth seeded review is the one that holds
 * eight of them.
 */
test.describe('the draft page, after DEV-928', () => {
  test('the held-back rows, closed and open, at both widths', async ({ page }) => {
    for (const [width, suffix] of [[1440, ''], [390, '-390']] as const) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })

      await page.goto(`${seeded}reviews/${heldBackReviewId}/draft`)
      await expect(page.locator('h2.headline')).toBeVisible()
      await expect(page.locator('.held-row')).toHaveCount(8)
      await shot(page, `dev-928-1-held-back${suffix}`)

      /*
       * The row with the longest claim open — 61 words, which is what the panel used to
       * print eight of — beside the row whose line the page had to cut.
       */
      await page.locator('.held-row').filter({ hasText: 'F6' }).first().click()
      await expect(page.locator('.held-opened')).toHaveCount(1)
      await shot(page, `dev-928-2-row-open${suffix}`)
    }
    await page.setViewportSize({ width: 1440, height: 900 })
  })

  /**
   * A drawing over the page, zoomed.
   *
   * At 169%, because 100% is the fit and a picture of the fit says nothing about whether it
   * can be zoomed. The bar reads the scale, which is the one thing in the picture that
   * cannot be mistaken for the inline figure.
   */
  test('a drawing opened larger, and zoomed', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(`${seeded}reviews/${heldBackReviewId}/draft`)
    await expect(page.locator('h2.headline')).toBeVisible()
    await page.waitForLoadState('networkidle')

    await page.locator('button.canvas').first().click()
    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()
    await shot(page, 'dev-928-3-diagram-larger')

    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await expect(sheet.locator('.reading')).toContainText('169%')
    await shot(page, 'dev-928-4-diagram-zoomed')

    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
  })
})

test.describe('a fresh database', () => {
  test('the four pages', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await shot(page, 'fresh-1-pull-requests')

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
    await page.goto(`${seeded}reviews/${reviewId}/understanding`)
    await expect(page.getByText('A webhook delivery that fails once')).toBeVisible()

    await page.route('**/api/v1/**', route => route.abort('connectionrefused'))
    await page.reload()
    await expect(page.getByText('Review Desk is not answering')).toBeVisible()

    await shot(page, 'server-stopped')
  })
})

/*
 * **The four states of the gap between a sign-off and a posting**, which is DEV-932, at the
 * width of a laptop and at the width of a phone.
 *
 * Each one is reached the way the real thing reaches it. The sign-off is a click, because
 * that is the only way a draft can be signed off; the posting is `review-desk draft posted`,
 * which is the command a review session runs; the take-back is a click on the control
 * DEV-929 added. Nothing here writes a status into a database to get a picture of it.
 *
 * `body_is_stale` is not among them and cannot be: `draft put` with no blocks stores the
 * whole body as one prose block, so the composition always matches what is stored and the
 * command line cannot reach that state. `web/test/posting-watch.nuxt.spec.ts` is where it
 * is measured.
 */
test.describe('waiting for the session that posts it', () => {
  /** Both widths of one state, so a state is never pictured at one width only. */
  async function bothWidths(page: Page, name: string) {
    await page.setViewportSize({ width: 1440, height: 900 })
    await shot(page, name)
    await page.setViewportSize({ width: 390, height: 844 })
    await shot(page, `${name}-390`)
    await page.setViewportSize({ width: 1440, height: 900 })
  }

  /**
   * Sign off in the browser, which is the only place a draft can be signed off.
   *
   * The clock is installed first so that `pinAt` below can stop it. `install` on its own
   * changes nothing a picture would show: measured, three real seconds move it 3,004 ms,
   * so the page runs as it always does until something asks the clock to move or stop.
   */
  async function signOff(page: Page, id: number) {
    await page.clock.install()
    await page.goto(`${seeded}reviews/${id}/draft`)
    await page.locator('label.verdict-segment', { hasText: 'Request changes' }).click()
    await page.getByRole('button', { name: 'Sign off and post' }).click()
    /* The status line, which is drawn; the live region is hidden and always was 1px. */
    await expect(page.getByText('— waiting for the session to post it.')).toBeVisible()
  }

  /**
   * Stop the clock a stated number of seconds after the sign-off.
   *
   * **So that two runs produce the same picture, which is this file's whole claim.** The
   * panel's figures are in seconds, and `bothWidths` takes the second shot a moment after
   * the first — so on a live clock the 1440 and the 390 picture of one state disagreed
   * with each other, and every regeneration was a diff nobody had asked for.
   */
  async function pinAfterSignOff(page: Page, id: number, seconds: number) {
    await pinClockAt(page, draftStamps(seededDatabase, id).approvedAt, seconds)
  }

  test('signed off, and then posted', async ({ page }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Carry an idempotency key on a retry',
    )
    await signOff(page, id)
    await pinAfterSignOff(page, id, 25)
    await expect(page.locator('.watch')).toContainText('25 sec ago')
    await bothWidths(page, 'dev-932-1-waiting')

    /*
     * **The clock stays stopped, and the posting is still noticed.** Resuming it put a
     * live five-second tick in the window between the command below and the pin — and a
     * tick that found the posting would stop the timer, leaving `pauseAt` with nothing due
     * to fire and the figures wherever that tick had left them. Paused, nothing fires
     * until the pin asks it to, and the pin's own jump is what wakes the interval.
     */
    cli(seededDatabase, [
      'draft',
      'posted',
      '--review',
      String(id),
      '--github-review',
      '1842003517',
    ])

    /*
     * **Pinned before the page notices, not after.** `now` moves on a timer tick, a change
     * of state or a return to the tab, and the timer stops the moment the draft is posted
     * — so a clock pinned *after* the page had noticed would fire nothing and leave the
     * figure wherever the last live tick put it. Pinned while the page is still waiting,
     * the jump fires the running interval once: it re-reads, finds the posting, and words
     * "Posted" from the pinned clock.
     *
     * **Forty, not twelve, and the clock is why.** `pauseAt` refuses to go backwards, and
     * the clock is already at the sign-off plus the 25 the picture above was pinned to —
     * so the offset has to clear that from `posted_at`, which the session reports a second
     * or two after the sign-off. Forty does, with room for a slow command.
     */
    await pinClockAt(page, draftStamps(seededDatabase, id).postedAt, 40)

    /* Noticed on the page's own timer. Nothing is reloaded to take this picture. */
    await expect(page.locator('.watch')).toContainText('Posted on GitHub.', {
      timeout: 20_000,
    })
    await expect(page.locator('.watch')).toContainText('40 sec ago')
    await bothWidths(page, 'dev-932-3-posted')
  })

  test('signed off, and nothing has posted it', async ({ page }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Skip a schedule whose tenant row has gone',
    )

    /* Forty-five seconds after the sign-off, and stopped there. Nothing is waited out. */
    await signOff(page, id)
    await pinAfterSignOff(page, id, 45)

    await expect(page.locator('.watch')).toContainText('may not be running')
    await expect(page.locator('.watch')).toContainText('in the 45 sec since you signed off')
    await bothWidths(page, 'dev-932-2-nothing-posted')
  })

  test('the sign-off taken back while the page was watching', async ({ page }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Name the helper this change deletes',
    )
    await signOff(page, id)

    await page.getByRole('button', { name: 'Take back the sign-off and send it back' }).click()
    await page.locator('textarea#send-back-note').fill('Fold F1 into the prose; it has no line')
    await page.getByRole('button', { name: 'Send back', exact: true }).click()

    await expect(page.locator('.watch')).toContainText('The sign-off is taken back')
    /* Nothing in this state carries a figure, so there is no clock to pin. */
    await bothWidths(page, 'dev-932-4-sent-back')
  })
})

/*
 * **What DEV-957 changed, at both widths.**
 *
 * Four pictures: the queue with one row per pull request and the links in each row's
 * repository line; the Reviews list's Finished view, which is where the four rounds of a
 * merged pull request now live; and the draft page of a round nothing can post, which says
 * so where the sign-off used to be.
 */
test.describe('a finished review, and a row’s links', () => {
  for (const { suffix, width, height } of [
    { suffix: '', width: 1440, height: 900 },
    { suffix: '-390', width: 390, height: 844 },
  ]) {
    test(`the queue, the finished view and a draft nothing can post at ${width}`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height })

      await page.goto(seeded)
      await expect(page.locator('.queue-line').filter({ hasText: '#9541' })).toHaveCount(1)
      await shot(page, `dev-957-1-queue${suffix}`)

      /*
       * **One row for the merged pull request, with its other three rounds opened.** Four
       * rounds of #9568 were four rows until 16 October 2026; they are one row and a fold
       * now. The fold is opened before the picture because a picture of a closed fold does
       * not show what is in it, and what this shot is for is that every round is still
       * reachable with its own sentence.
       */
      await page.goto(`${seeded}reviews`)
      await page.getByRole('radio').filter({ hasText: 'Finished' }).click()
      const merged = page.locator('.queue-line').filter({ hasText: '#9568' })
      await expect(merged).toHaveCount(1)
      await merged.locator('.earlier summary').click()
      await expect(merged.locator('.earlier-round'))
        .toHaveCount(mergedReviewIds.length - 1)
      await shot(page, `dev-957-2-finished${suffix}`)

      /* Round 4: the one round of the four that never posted. */
      await page.goto(`${seeded}reviews/${mergedReviewIds[3]}/draft`)
      await expect(page.getByText('Nothing to sign off')).toBeVisible()
      await shot(page, `dev-957-3-merged-draft${suffix}`)

      /*
       * **DEV-962's shape A, where the fourth of these used to be a superseded round.** A
       * second unposted round of one pull request is not a thing the binary can record any
       * more, so what is drawn here is what replaced it: one round, whose head moved and
       * whose base-branch merges changed nothing.
       */
      await page.goto(`${seeded}reviews/${movedHeadReviewId}/findings`)
      await expect(page.getByText('Rounds')).toBeVisible()
      await shot(page, `dev-962-1-moved-head${suffix}`)
    })
  }
})

/*
 * **Overview in its three states** (DEV-1326), beside S2, S12 and S15 at 1,600 and 390.
 *
 * *Ready* is the seeded structured review. *Being written* and *Replies in* are seeded here,
 * in a database of their own, so the shared seed's counts — which other specs assert —
 * do not move: the same structured review twice, one with its code review still running and
 * one posted with the author's replies, a resolved thread and a code change come back.
 */
test.describe('Overview', () => {
  test('ready, being written and replies in, and Replies, at 1,600 and 390', async ({ page }) => {
    const directory = scratch()
    const database = fresh(directory)

    const writing = seedStructuredReview(database, 'Give a scheduled export its own context').reviewId
    for (const [layer, state] of [[5, 'running'], [6, 'pending'], [7, 'pending']] as const) {
      cli(database, ['layer', 'set', '--review', String(writing), '--layer', String(layer), '--state', state, '--detail', state === 'running' ? 'reading the diff' : 'not yet'])
    }

    const replies = seedStructuredReview(database, 'Give a scheduled export its own context').reviewId
    cli(database, ['review', 'link', '--review', String(replies), '--github-review', 'PRR_kwDOoverview'])
    for (const [layer, state] of [[6, 'done'], [7, 'running']] as const) {
      cli(database, ['layer', 'set', '--review', String(replies), '--layer', String(layer), '--state', state, '--detail', state === 'done' ? 'posted' : 'replies back'])
    }
    const outcomes: Array<[number, string, string, string, string]> = [
      [1, 'replied', 'author', 'Tenants are only soft-deleted, so the purge never reaches them.', '2026-11-20T09:00:00Z'],
      [2, 'replied', 'author', 'Fixed in 3c1e2a: the empty list now exports no columns.', '2026-11-20T09:10:00Z'],
      [2, 'thread_resolved', 'author', JSON.stringify({ resolved_by: 'dana-ruiz', time_basis: 'observed' }), '2026-11-20T09:12:00Z'],
      [3, 'replied', 'author', 'A unique index on (tenant, schedule).', '2026-11-20T09:20:00Z'],
      [3, 'raised_again', 'developer', 'Is a unique index enough to stop two runs?', '2026-11-20T10:00:00Z'],
      [4, 'code_changed_after', 'harvester', JSON.stringify({ commit: '3c1e2a9d41', file: 'app/Reporting/ScheduleRunner.php', line: 41, lines_changed: true }), '2026-11-20T09:30:00Z'],
    ]
    for (const [finding, kind, by, detail, when] of outcomes) {
      cli(database, ['outcome', 'add', '--review', String(replies), '--finding', String(finding), '--kind', kind, '--by', by, '--detail', detail, '--at', String(Math.floor(Date.parse(when) / 1000))])
    }

    const own = await serve(database)
    try {
      const states = [
        [`${seeded}reviews/${structuredReviewId}`, 'overview-1-ready', 'ready'],
        [`${own.url}reviews/${writing}`, 'overview-2-being-written', 'being_written'],
        [`${own.url}reviews/${replies}`, 'overview-3-replies-in', 'replies_in'],
      ] as const
      for (const [width, height, suffix] of [[1600, 1000, ''], [390, 844, '-390']] as const) {
        await page.setViewportSize({ width, height })
        for (const [address, name, state] of states) {
          await page.goto(address)
          await expect(page.getByTestId('bottom-line')).toHaveAttribute('data-state', state)
          await shot(page, `${name}${suffix}`)
        }
        /*
         * **Replies (DEV-1330), beside S09**, off the same seed: F1 contested, F2 fixed, F3
         * answered, and the rest with no reply. Round 1 and posted, so F1's row gives the
         * record's reason in place of the three answers — the departure the PR names.
         */
        await page.goto(`${own.url}reviews/${replies}/replies`)
        await expect(page.locator('.reply-row--contested')).toHaveCount(1)
        await shot(page, `replies-1-groups${suffix}`)
        await page.locator('#reply-F1 .rd-fold-line').click()
        await expect(page.locator('#history-1')).toBeVisible()
        await shot(page, `replies-2-history-open${suffix}`)
      }
    }
    finally {
      await stop(own)
      await page.setViewportSize({ width: 1440, height: 900 })
    }
  })
})

/*
 * **S14: the sheet two way, with an answer in it** (DEV-1336), beside the target. The
 * structured review is S14's own case — scheduled exports, F9 — served from a database of
 * its own beside `fakeAllele`, so the sheet's panel is two way: the question is asked in the
 * browser, through the sheet's composer, and the session's answer is written with
 * `review-desk message reply` in the contract's shape, a regrade of F9 suggested in it.
 * Nothing is stubbed in the browser, and nothing here starts or drives a session.
 */
test.describe('the conversation on a phone', () => {
  test('asked from the sheet, and answered', async ({ page }) => {
    const directory = scratch()
    const database = fresh(directory)
    const review = seedStructuredReview(database, 'Give a scheduled export its own context').reviewId
    cli(database, ['review', 'link', '--review', String(review), '--session', 'ses-dev-1336'])
    const allele = await fakeAllele('idle')
    const own = await serve(database, { allele: allele.socket })
    try {
      await signedIn(page.context(), own)
      await page.setViewportSize({ width: 390, height: 844 })
      await page.goto(`${own.url}reviews/${review}/findings?finding=F9`)
      await page.waitForLoadState('networkidle')

      await page.locator('.topbar .ask').click()
      const panel = page.locator('#conversation [data-testid="conversation-panel"]')
      await expect(panel).toHaveAttribute('data-mode', 'two_way')
      await panel.locator('textarea').fill('Does it also happen on the web path, or only scheduled?')
      await panel.getByRole('button', { name: 'Ask', exact: true }).click()
      await expect(panel.locator('[data-author="developer"]')).toHaveCount(1)

      const listed = cliJson(database, ['message', 'list', '--review', String(review)]) as unknown as {
        messages: Array<{ seq: number, author: string }>
      }
      const asked = Math.max(...listed.messages.filter(one => one.author === 'developer').map(one => one.seq))
      const answer = join(directory, 'sheet-answer.json')
      writeFileSync(answer, JSON.stringify({
        kind: 'answer',
        about: { finding: 'F9' },
        headline: 'Both. The web path calls the same policy, so an empty list exports every column there too.',
        points: [{ basis: 'read', text: 'ExportController.php:41 passes the definition straight through.' }],
        cites: ['F9', 'app/Http/Controllers/ExportController.php:41'],
        suggests: [{ type: 'regrade', finding: 'F9', from: 'should', to: 'blocker', why: 'Both paths leak private columns.' }],
      }))
      cli(database, ['message', 'reply', '--review', String(review), '--step', 'code_review', '--to', String(asked), '--answer', answer])

      await page.goto(`${own.url}reviews/${review}/findings?finding=F9`)
      await page.waitForLoadState('networkidle')
      await page.locator('.topbar .ask').click()
      await expect(panel.getByText('Both. The web path calls the same policy')).toBeVisible()
      await page.waitForTimeout(400)
      await page.screenshot({ path: `${INTO}/sheet-1-asked-390.png` })
    }
    finally {
      await stop(own)
      await allele.close()
      await page.setViewportSize({ width: 1440, height: 900 })
    }
  })
})
