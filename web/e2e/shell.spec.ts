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
         * measures, read as the browser resolves them for that paragraph's own face and size.
         * A paragraph is a `<p>`, an `.rd-note` a flex column has made a block of, and the
         * line of a row a session wrote — a blind-pass question, a finding's line on the case,
         * a reply's line — measured by its own element, so the row's link is not counted.
         *
         * **The bound is whichever measure is larger at that paragraph's size, on purpose.**
         * Which one a paragraph is held to is its stylesheet's choice, and the DOM does not
         * record it, so one of the two kinds can run past its own measure by the difference
         * and pass. What this catches is a paragraph with no measure at all, which runs to the
         * band — 1,000px and more at 2,560.
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
            [...document.querySelectorAll<HTMLElement>('main p, main .rd-note, main .question > span, main .what, main .cell--line')]
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

/*
 * **The side columns have drag handles** (DEV-1375): the sidebar's on its right edge, the
 * conversation's and the Pull requests preview's on their left, each a focusable separator
 * that the pointer and the arrow keys move between the column's limits, a double-click puts
 * back, and the browser keeps across a reload. The arithmetic is `test/columns.spec.ts`'s;
 * this is what only a browser answers — a pointer, a reload, a storage that throws.
 */
test.describe('the side columns’ handles', () => {
  test.use({ viewport: { width: 1600, height: 900 } })

  const handle = (page: Page, column: string) => page.locator(`[role="separator"][data-column="${column}"]`)
  const width = async (page: Page, selector: string) =>
    Math.round((await page.locator(selector).boundingBox())!.width)

  /** Press on a handle's middle, move it `dx` across, and let go. */
  async function drag(page: Page, column: string, dx: number) {
    const box = (await handle(page, column).boundingBox())!
    const x = box.x + box.width / 2
    const y = box.y + box.height / 2
    await page.mouse.move(x, y)
    await page.mouse.down()
    await page.mouse.move(x + dx / 2, y, { steps: 4 })
    await page.mouse.move(x + dx, y, { steps: 4 })
    await page.mouse.up()
  }

  test('each is a separator a screen reader can read', async ({ page }) => {
    await open(page, `reviews/${reviewId}`)
    for (const [column, controls, now, max] of [
      ['sidebar', 'sidebar', '232', '480'],
      ['conversation', 'conversation', '380', '720'],
    ] as const) {
      const one = handle(page, column)
      await expect(one).toHaveAttribute('aria-orientation', 'vertical')
      await expect(one).toHaveAttribute('aria-controls', controls)
      await expect(one).toHaveAttribute('aria-valuenow', now)
      await expect(one).toHaveAttribute('tabindex', '0')
      await expect(one).toHaveAttribute('aria-valuemax', max)
      expect(await one.evaluate(el => getComputedStyle(el).cursor)).toBe('col-resize')
      expect(Math.round((await one.boundingBox())!.width), 'the hit area is 8px').toBe(8)
    }
    await expect(handle(page, 'sidebar')).toHaveAttribute('aria-valuemin', '180')
    await expect(handle(page, 'conversation')).toHaveAttribute('aria-valuemin', '300')
  })

  test('dragging the sidebar’s handle moves it between 180 and 480, and a reload keeps it', async ({ page }) => {
    await open(page, 'rules')
    await drag(page, 'sidebar', 100)
    expect(await width(page, '#sidebar')).toBe(332)
    expect((await page.locator('main#main').boundingBox())!.x, 'the main column follows').toBe(332)

    await drag(page, 'sidebar', 1000)
    expect(await width(page, '#sidebar'), 'no wider than 480').toBe(480)
    await drag(page, 'sidebar', -1000)
    expect(await width(page, '#sidebar'), 'no narrower than 180').toBe(180)

    await drag(page, 'sidebar', 120)
    await page.reload()
    await open(page, 'rules')
    expect(await width(page, '#sidebar'), 'kept across a reload').toBe(300)
    /* And on every page: the sidebar is one column, wherever it is drawn. */
    await open(page, `reviews/${reviewId}/findings`)
    expect(await width(page, '#sidebar')).toBe(300)

    await handle(page, 'sidebar').dblclick()
    expect(await width(page, '#sidebar'), 'a double-click is the default').toBe(232)
    await page.reload()
    await open(page, `reviews/${reviewId}/findings`)
    expect(await width(page, '#sidebar'), 'and the default is what is kept').toBe(232)
  })

  test('dragging the conversation’s handle moves it between 300 and the maximum, and a reload keeps it', async ({ page }) => {
    await open(page, `reviews/${reviewId}`)
    await drag(page, 'conversation', -100)
    expect(await width(page, '.shell-conversation')).toBe(480)
    expect(Math.round((await page.locator('main#main').boundingBox())!.width)).toBe(1600 - 232 - 480)

    /* At 1,600 the maximum is 720, which leaves the main column 648. */
    await drag(page, 'conversation', -1000)
    expect(await width(page, '.shell-conversation')).toBe(720)
    await drag(page, 'conversation', 1000)
    expect(await width(page, '.shell-conversation')).toBe(300)

    await drag(page, 'conversation', -150)
    await page.reload()
    await open(page, `reviews/${reviewId}/findings`)
    expect(await width(page, '.shell-conversation'), 'kept across a reload, on every tab').toBe(450)
    const box = (await page.locator('.shell-conversation').boundingBox())!
    expect(Math.round(box.x + box.width), 'still at the window’s edge').toBe(1600)

    await handle(page, 'conversation').dblclick()
    expect(await width(page, '.shell-conversation')).toBe(380)
  })

  test('dragging the preview’s handle moves it between the conversation’s limits', async ({ page }) => {
    await open(page, '')
    await expect(handle(page, 'preview')).toHaveAttribute('aria-controls', 'preview')
    await drag(page, 'preview', -100)
    expect(await width(page, '.preview-column')).toBe(480)
    await drag(page, 'preview', -1000)
    expect(await width(page, '.preview-column')).toBe(720)
    await drag(page, 'preview', 1000)
    expect(await width(page, '.preview-column')).toBe(300)
    const box = (await page.locator('.preview-column').boundingBox())!
    expect(Math.round(box.x + box.width), 'the preview meets the window’s edge').toBe(1600)

    await drag(page, 'preview', -60)
    await page.reload()
    await open(page, '')
    expect(await width(page, '.preview-column'), 'kept across a reload').toBe(360)
    /* The preview's width is its own: the conversation is still at its default. */
    await open(page, `reviews/${reviewId}`)
    expect(await width(page, '.shell-conversation')).toBe(380)
    await open(page, '')
    await handle(page, 'preview').dblclick()
    expect(await width(page, '.preview-column')).toBe(380)
  })

  test('the arrow keys move a handle, Shift further, and Home and End go to the limits', async ({ page }) => {
    await open(page, `reviews/${reviewId}`)
    const sidebar = handle(page, 'sidebar')
    await sidebar.focus()
    await page.keyboard.press('ArrowRight')
    await expect(sidebar).toHaveAttribute('aria-valuenow', '248')
    await page.keyboard.press('Shift+ArrowRight')
    await expect(sidebar).toHaveAttribute('aria-valuenow', '312')
    expect(await width(page, '#sidebar')).toBe(312)
    await page.keyboard.press('ArrowLeft')
    await expect(sidebar).toHaveAttribute('aria-valuenow', '296')
    await page.keyboard.press('Home')
    await expect(sidebar).toHaveAttribute('aria-valuenow', '180')
    await page.keyboard.press('End')
    await expect(sidebar).toHaveAttribute('aria-valuenow', '480')

    /* The conversation's handle is on its left edge, so Left widens it. */
    const conversation = handle(page, 'conversation')
    await conversation.focus()
    await page.keyboard.press('ArrowLeft')
    await expect(conversation).toHaveAttribute('aria-valuenow', '396')
    await page.keyboard.press('Shift+ArrowRight')
    await expect(conversation).toHaveAttribute('aria-valuenow', '332')
    /* 1,600 − 480 − 480: what the sidebar at its widest leaves, under the column's own 720. */
    await page.keyboard.press('End')
    await expect(conversation).toHaveAttribute('aria-valuenow', '640')
    await expect(sidebar).toHaveAttribute('aria-valuemax', '480')
    expect(await width(page, '.shell-conversation')).toBe(640)
  })

  test('at 1,100 with both side columns at their widest, the main column keeps 480px', async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem('review-desk:width:sidebar', '480')
      localStorage.setItem('review-desk:width:conversation', '720')
    })
    await page.setViewportSize({ width: 1100, height: 900 })
    await open(page, `reviews/${reviewId}/findings`)
    const main = Math.round((await page.locator('main#main').boundingBox())!.width)
    expect(main).toBeGreaterThanOrEqual(480)
    expect(await sideways(page)).toBe(0)

    /* The widths set are kept, and come back when there is room for them again. */
    await page.setViewportSize({ width: 2400, height: 900 })
    await expect.poll(() => width(page, '#sidebar')).toBe(480)
    await expect.poll(() => width(page, '.shell-conversation')).toBe(720)
  })

  test('a press that does not move leaves the width that was set alone', async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('review-desk:width:sidebar', '480'))
    await page.setViewportSize({ width: 1100, height: 900 })
    await open(page, `reviews/${reviewId}`)
    expect(await width(page, '#sidebar'), 'drawn narrower than set at 1,100').toBe(320)
    const box = (await handle(page, 'sidebar').boundingBox())!
    await page.mouse.move(box.x + 4, box.y + 300)
    await page.mouse.down()
    await page.mouse.move(box.x + 4, box.y + 302)
    await page.mouse.up()
    await page.setViewportSize({ width: 1920, height: 900 })
    await expect.poll(() => width(page, '#sidebar'), 'what was set comes back with the room').toBe(480)
  })

  test('with storage blocked the shell draws at its defaults, and nothing throws', async ({ page }) => {
    const thrown: string[] = []
    page.on('pageerror', error => thrown.push(error.message))
    await page.addInitScript(() => {
      const refuse = () => {
        throw new DOMException('The operation is insecure.', 'SecurityError')
      }
      Storage.prototype.getItem = refuse
      Storage.prototype.setItem = refuse
      Storage.prototype.removeItem = refuse
    })
    await open(page, `reviews/${reviewId}`)
    expect(await width(page, '#sidebar')).toBe(232)
    expect(await width(page, '.shell-conversation')).toBe(380)
    /* The handles still move, for this page. */
    await drag(page, 'sidebar', 40)
    expect(await width(page, '#sidebar')).toBe(272)
    await handle(page, 'sidebar').dblclick()
    expect(await width(page, '#sidebar')).toBe(232)
    expect(thrown).toEqual([])
  })

  test('under 1,100 the conversation has no handle, and under 700 nothing does', async ({ page }) => {
    await page.setViewportSize({ width: 1024, height: 800 })
    await open(page, `reviews/${reviewId}`)
    await expect(handle(page, 'sidebar')).toBeVisible()
    await expect(handle(page, 'conversation')).toBeHidden()

    await page.setViewportSize({ width: 390, height: 844 })
    await openOnAPhone(page, `reviews/${reviewId}`)
    await expect(page.locator('[role="separator"][data-column]:visible')).toHaveCount(0)
    await openOnAPhone(page, '')
    await expect(page.locator('[role="separator"][data-column]:visible')).toHaveCount(0)
  })
})

test.describe('with a classic scrollbar', () => {
  const width = async (page: Page, selector: string) =>
    Math.round((await page.locator(selector).boundingBox())!.width)

  /*
   * **A classic scrollbar arriving after the shell mounted** narrows what the grid divides
   * and fires no `resize` (#114's self-review, F1). Headless Chromium hides scrollbars and a
   * Mac draws them over the page, so this browser is launched without `--hide-scrollbars`
   * and a scrollbar that takes 15px is styled in, after load.
   */
  test('a scrollbar that arrives after load still leaves the main column 480px', async ({ playwright }) => {
    /* Its own browser: `--hide-scrollbars` is a launch flag, and the suite's browser has it. */
    const browser = await playwright.chromium.launch({ ignoreDefaultArgs: ['--hide-scrollbars'] })
    const context = await browser.newContext({ storageState: 'e2e/.signed-in.json', viewport: { width: 1100, height: 600 } })
    const page = await context.newPage()
    await page.addInitScript(() => {
      localStorage.setItem('review-desk:width:sidebar', '480')
      localStorage.setItem('review-desk:width:conversation', '720')
    })
    await open(page, `reviews/${reviewId}/findings`)
    expect(await width(page, '#sidebar'), 'no scrollbar yet: 1,100 − 480 − 300').toBe(320)
    await page.addStyleTag({ content: '::-webkit-scrollbar { width: 15px } ::-webkit-scrollbar-thumb { background: #888 }' })
    const gutter = await page.evaluate(() => window.innerWidth - document.documentElement.clientWidth)
    expect(gutter, 'the scrollbar takes width, or this test proves nothing').toBe(15)
    await expect.poll(async () => Math.round((await page.locator('main#main').boundingBox())!.width))
      .toBeGreaterThanOrEqual(480)
    await browser.close()
  })
})
