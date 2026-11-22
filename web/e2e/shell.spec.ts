import { expect, test, type Page } from '@playwright/test'

import { readAddresses } from './harness'

/*
 * **The shell, measured in a browser** — DEV-1323's criteria that only a layout engine can
 * answer: every existing page renders inside the three columns at 2,560, 1,920, 1,600 and
 * 1,280 wide without scrolling sideways, and fills the window at every one of them
 * (DEV-1372), no element but the wordmark computes a serif face, and under
 * 700px the sidebar is behind a menu button and the conversation is a sheet behind *Ask*
 * (DEV-1336). Since DEV-1333 the column holds *Ask this review*, under the page between 700
 * and 1,100.
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
  { name: 'overview', path: `reviews/${structuredReviewId}`, review: true },
  { name: 'the case', path: `reviews/${structuredReviewId}/understanding`, review: true },
  { name: 'code review', path: `reviews/${reviewId}/findings`, review: true },
  { name: 'the draft', path: `reviews/${reviewId}/draft`, review: true },
  { name: 'what happened', path: `reviews/${reviewId}/replies`, review: true },
]

/** The page, loaded and settled: fonts in, Mermaid drawn, nothing still reading. */
async function open(page: Page, path: string) {
  await page.goto(`${seeded}${path}`)
  await page.waitForLoadState('networkidle')
  await expect(page.locator('#sidebar .wordmark')).toBeVisible()
  await expect(page.locator('main#main')).toBeVisible()
}

/** The page at a phone's width, settled: the sidebar is behind the menu there, so the bar is waited for. */
async function openOnAPhone(page: Page, path: string) {
  await page.goto(`${seeded}${path}`)
  await page.waitForLoadState('networkidle')
  await expect(page.getByRole('button', { name: 'Menu' })).toBeVisible()
  await expect(page.locator('main#main')).toBeVisible()
}

/** How far the document scrolls sideways. Nought is the only right answer. */
const sideways = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)

/** The pages whose prose is read down, and so must stop at its measure however wide the window. */
const DOCUMENTS = ['overview', 'the case', 'what happened']

for (const width of [2560, 1920, 1600, 1280] as const) {
  test.describe(`at ${width} wide`, () => {
    test.use({ viewport: { width, height: 900 } })

    for (const one of PAGES) {
      test(`${one.name} sits in the shell`, async ({ page }) => {
        await open(page, one.path)

        const sidebar = await page.locator('#sidebar').boundingBox()
        expect(sidebar!.x).toBe(0)
        expect(Math.round(sidebar!.width)).toBe(232)

        /*
         * **The shell fills the window** (DEV-1372): no outer cap and no centring, so the
         * main column is the window less the sidebar and, on a review, the conversation —
         * on a document's page as much as a workbench's. 1,308px at 1,920 on a review,
         * 2,328px at 2,560 on Rules.
         */
        const main = await page.locator('main#main').boundingBox()
        expect(main!.x, 'the main column starts at the sidebar').toBe(232)
        expect(Math.round(main!.width), 'the main column takes everything between')
          .toBe(width - 232 - (one.review ? 380 : 0))

        const conversation = page.locator('.shell-conversation')
        if (one.review) {
          await expect(conversation).toBeVisible()
          const box = (await conversation.boundingBox())!
          expect(Math.round(box.width)).toBe(380)
          expect(Math.round(box.x + box.width), 'the conversation meets the window\'s right edge').toBe(width)
          /* *Ask this review* is in it, on every tab (DEV-1333). */
          await expect(conversation.getByRole('heading', { name: 'Ask this review' })).toBeVisible()
        }
        else {
          await expect(conversation).toHaveCount(0)
        }

        /* The list's preview is its own third column, and meets the window's edge too. */
        if (one.path === '') {
          const preview = (await page.locator('.preview-column').boundingBox())!
          expect(Math.round(preview.x + preview.width), 'the preview meets the window\'s right edge').toBe(width)
        }

        /*
         * **The bands fill and the prose does not** (DEV-1372). However wide the window, no
         * paragraph on a document's page runs its text wider than the larger of the two
         * measures, read as the browser resolves them for that paragraph's own face and size:
         * the page's copy stops at `--rd-measure`, a session's at `--rd-measure-recorded`.
         *
         * **What is measured is the text, not the box.** A row of a key and a title, or a
         * line of figures, is a `<p>` whose box takes the band as a rule does, and that is
         * the band filling; the words in it are what must not stretch. The review header is
         * left to its own caps, which hold at every width: the title at 940px and its lead
         * line at 800px (`ReviewStepPage.vue`). A pixel's grace, because a line's last glyph
         * can overhang its box: the legend capped at 720 measures 721.
         */
        if (DOCUMENTS.includes(one.name)) {
          const wider = await page.evaluate(() =>
            [...document.querySelectorAll<HTMLElement>('main p')]
              .filter(el => el.offsetParent !== null && !el.closest('[data-testid="review-header"]'))
              .map((el) => {
                const probe = document.createElement('span')
                probe.style.display = 'block'
                probe.style.maxWidth = 'var(--rd-measure-recorded)'
                el.append(probe)
                const recorded = Number.parseFloat(getComputedStyle(probe).maxWidth)
                probe.remove()
                const measure = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--rd-measure'))
                const range = document.createRange()
                range.selectNodeContents(el)
                const lines = [...range.getClientRects()]
                const text = lines.length === 0 ? 0 : Math.max(...lines.map(line => line.right)) - Math.min(...lines.map(line => line.left))
                return { at: `${el.className} < ${el.parentElement!.className}`, text: Math.round(text), cap: Math.ceil(Math.max(measure, recorded)) }
              })
              .filter(one => one.text > one.cap + 1))
          expect(wider, `paragraphs wider than their measure on ${one.name}`).toEqual([])
        }

        /*
         * **A workbench still divides its columns in fractions** once its own container is
         * wide enough, which beside the conversation is from 1,600 (at 1,280 both stack by
         * design): Findings' list beside the finding, Draft's preview beside what you decide.
         */
        if (width >= 1600 && (one.name === 'code review' || one.name === 'the draft')) {
          const tracks = await page.locator('.bench').first()
            .evaluate(el => getComputedStyle(el).gridTemplateColumns.split(' ').length)
          expect(tracks, `${one.name}'s bench is two columns`).toBe(2)
        }

        expect(await sideways(page), 'the page scrolls sideways').toBe(0)

        /*
         * **The review header is never taller than 120px at 1,600** — DEV-1324's first
         * criterion and `docs/design.md`'s header rule. Measured on every tab, because each
         * one's facts and action differ.
         */
        if (one.review && width === 1600) {
          const header = await page.locator('[data-testid="review-header"]').boundingBox()
          expect(header, 'the review header is drawn').not.toBeNull()
          expect(header!.height, 'the header is no taller than 120px').toBeLessThanOrEqual(120)
        }
      })
    }
  })
}

/*
 * **Between 700 and 1,100 the conversation sits under the page** (DEV-1333): beside it, at
 * 1,024, it would leave a review's page 332px (the self-review of #86, F4). So the page keeps
 * the main column's full width and the conversation takes that same width below it.
 */
test.describe('at 1024 wide', () => {
  test.use({ viewport: { width: 1024, height: 800 } })

  for (const one of PAGES) {
    test(`${one.name} keeps its width`, async ({ page }) => {
      await open(page, one.path)
      const main = await page.locator('main#main').boundingBox()
      expect(Math.round(main!.width)).toBe(1024 - 232)
      if (one.review) {
        const conversation = await page.locator('.shell-conversation').boundingBox()
        expect(Math.round(conversation!.width)).toBe(1024 - 232)
        expect(conversation!.y).toBeGreaterThanOrEqual(main!.y + main!.height - 1)
      }
      else {
        await expect(page.locator('.shell-conversation')).toHaveCount(0)
      }
      expect(await sideways(page), 'the page scrolls sideways').toBe(0)
    })
  }
})

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

  test('the sidebar is behind a menu button, and the conversation waits behind Ask', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/findings`)
    await page.waitForLoadState('networkidle')

    const menu = page.getByRole('button', { name: 'Menu' })
    await expect(menu).toBeVisible()
    await expect(menu).toHaveAttribute('aria-expanded', 'false')
    await expect(page.locator('#sidebar')).toBeHidden()
    await expect(page.locator('.shell-conversation')).toBeHidden()

    await menu.click()
    await expect(menu).toHaveAttribute('aria-expanded', 'true')
    await expect(page.locator('#sidebar')).toBeVisible()
    /* Focus goes into it; Escape closes it and gives focus back (#86's self-review, F5). */
    await expect(page.locator('#sidebar a').first()).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(page.locator('#sidebar')).toBeHidden()
    await expect(menu).toBeFocused()
    /* And while it is open, the page behind it is out of reach. */
    await menu.click()
    await expect(page.locator('main#main')).toHaveAttribute('inert', '')

    /* Turned to landscape with it open: it closes, and the page is reachable again. */
    await page.setViewportSize({ width: 844, height: 390 })
    await expect(page.locator('main#main')).not.toHaveAttribute('inert')
    await page.setViewportSize({ width: 390, height: 844 })
    await expect(menu).toHaveAttribute('aria-expanded', 'false')
    await menu.click()

    /* Going somewhere is what the menu is for, so arriving closes it. */
    await page.locator('#sidebar').getByRole('link', { name: /Rules/ }).click()
    await expect(page).toHaveURL(/\/rules$/)
    await expect(page.locator('#sidebar')).toBeHidden()

    expect(await sideways(page), 'the page scrolls sideways').toBe(0)
  })

  /*
   * **The conversation is a bottom sheet** (DEV-1336, S14): *Ask* in the bar opens it over
   * the review, on every tab, and the window scrolls sideways neither with it closed nor
   * open. The two-way ask from the sheet is `phone-sheet.spec.ts`'s.
   */
  for (const one of PAGES.filter(page => page.review)) {
    test(`${one.name}: no sideways scroll with the sheet closed or open`, async ({ page }) => {
      await openOnAPhone(page, one.path)
      const ask = page.locator('.topbar .ask')
      const sheet = page.locator('#conversation')

      await expect(ask).toBeVisible()
      await expect(sheet).toBeHidden()
      expect(await sideways(page), 'closed, the page scrolls sideways').toBe(0)

      await ask.click()
      await expect(sheet).toBeVisible()
      await expect(sheet.getByRole('heading', { name: 'Ask this review' })).toBeVisible()
      expect(await sideways(page), 'open, the page scrolls sideways').toBe(0)
      /* Nothing in the sheet is wider than the sheet. */
      expect(await sheet.evaluate(el => el.scrollWidth - el.clientWidth)).toBe(0)
      expect(await sheet.locator('[data-testid="conversation-panel"]').evaluate(el => el.scrollWidth - el.clientWidth)).toBe(0)
    })
  }

  test('Ask is drawn on a review only', async ({ page }) => {
    for (const one of PAGES.filter(page => !page.review)) {
      await openOnAPhone(page, one.path)
      await expect(page.locator('.topbar .ask'), one.name).toHaveCount(0)
    }
  })
})
