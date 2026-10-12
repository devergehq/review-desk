import { join } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { cli, scratch, serve, stop, type Serving } from './harness'

/*
 * **The header's two links, measured at 1440 and 390 with a long repository name.**
 *
 * DEV-936's eighth acceptance criterion, and it is a criterion about *measurement*: "the
 * header fits at 1440 and 390 pixels wide with a long repository name and a ticket id,
 * measured in the repository's Playwright Chromium". Reasoning about a flex line that now
 * carries two links and a long unbroken token is not the same as looking at it, which is
 * the whole reason this file exists rather than a comment saying it should be fine.
 *
 * ## Its own database and its own server
 *
 * The acceptance suite's seed is asserted against by name and by count — "3 reviews need
 * you", the repository chips, the Reviews list's heading — so a fourth review added to it
 * for this one measurement would rewrite assertions that have nothing to do with these
 * links. This seeds a database of its own and starts a server of its own on port 0, in
 * `beforeAll`, and stops it in `afterAll`.
 *
 * ## One review, no brief, on purpose
 *
 * A review nothing has written a brief for draws the header and almost nothing else, which
 * is what makes a document-level overflow check an assertion *about the header*: on a page
 * full of findings and tables, a horizontal scrollbar could have come from anywhere.
 */

/**
 * A repository name long enough to be the thing that breaks.
 *
 * 48 characters, and — the part that matters — **no hyphen in the name**. The first
 * version of this was `review-desk-dashboard-and-contract-fixtures`, which is longer and
 * tests less: a hyphen is a line-break opportunity, so a hyphenated name wraps wherever it
 * needs to and never sets a min-content width. An unbroken 38-character name does, and it
 * found a real fault in `QueueRow` the moment it was used — the rows overflowed their
 * section by 12 pixels at 390, which the fix in that component's styles records.
 *
 * Whether a name like this is likely is not the question a measurement answers. It is
 * allowed — GitHub's own rule is `[A-Za-z0-9._-]` — so it is the case the layout has to
 * survive.
 */
const LONG_REPO = 'devergehq/reviewdeskdashboardandcontractfixtures'

/** The ticket, and the address the session was given for it. */
const TICKET = 'DUC-2045'
const TICKET_URL = 'https://linear.app/deverge/issue/DUC-2045'

/** The two widths the criterion names: the boards' own width, and a phone. */
const WIDTHS = [
  { name: '1440', width: 1440, height: 900 },
  { name: '390', width: 390, height: 844 },
]

let serving: Serving
let reviewId: number

test.beforeAll(async () => {
  const database = join(scratch(), 'header-links.db')

  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    LONG_REPO,
    '--pr',
    '1284',
    '--head',
    '9f3c1ab',
    '--title',
    'Give a scheduled export its own context rather than reading the web request’s',
    '--ticket',
    TICKET,
    '--ticket-url',
    TICKET_URL,
  ])
  reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) throw new Error(`review open printed no id: ${opened}`)

  serving = await serve(database)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

/**
 * Whether anything on the page is wider than the page.
 *
 * `documentElement` rather than `body`: a child that overflows its parent widens the
 * scrolling box, and that is the box a reader has to drag sideways.
 */
async function overflows(page: Page): Promise<{ scroll: number, client: number }> {
  return page.evaluate(() => ({
    scroll: document.documentElement.scrollWidth,
    client: document.documentElement.clientWidth,
  }))
}

/**
 * What is too wide, named so that a failure says which element to go and look at.
 *
 * `within` scopes it to a subtree — the row's line, when the thing being measured is the
 * row rather than the page. Omitted, it walks the whole document.
 *
 * **Every element, and `scrollWidth` as well as the box.** An earlier version looked at
 * `a, h1, p, span` and at the box alone, and on a real failure it named the off-screen skip
 * link and nothing else: the element that was too wide was a `div`, and inline text can
 * overflow a block box without the box moving. Reported widest first, since the outermost
 * offender is usually the symptom and the innermost the cause.
 *
 * Two things are deliberately not counted:
 *
 * | Skipped | Why |
 * |---|---|
 * | anything inside `.rd-visually-hidden` | a 1×1 clipped box holding a sentence, so its `scrollWidth` is always far past its `clientWidth` — by design, and it is what the earlier version reported instead of the real offender |
 * | anything whose box is entirely left of the viewport | the layout's skip link sits at -9999 on purpose |
 */
async function tooWide(page: Page, edge: number, within?: string): Promise<string[]> {
  return page.evaluate(({ edge, within }) => {
    const roots = within
      ? [...document.querySelectorAll(within)]
      : [document.documentElement]

    return roots
      .flatMap(root => [root, ...root.querySelectorAll('*')])
      .map((element) => {
        const box = element.getBoundingClientRect()
        const over = Math.max(
          box.right - edge,
          -box.left,
          element.scrollWidth - element.clientWidth,
        )
        return { element, box, over }
      })
      .filter(({ element, box, over }) =>
        box.width > 1
        && box.height > 1
        && box.right > 0
        && !element.closest('.rd-visually-hidden')
        && over > 0.5)
      .sort((a, b) => b.over - a.over)
      .map(({ element, box }) =>
        `${element.tagName.toLowerCase()}.${element.className || '-'} `
        + `box ${Math.round(box.left)}…${Math.round(box.right)} `
        + `scroll ${element.scrollWidth} in ${element.clientWidth}`)
      .slice(0, 6)
  }, { edge, within })
}

for (const { name, width, height } of WIDTHS) {
  test(`the header fits at ${name} with a long repository name and a ticket id`, async ({ page }) => {
    await page.setViewportSize({ width, height })
    await page.goto(`${serving.url}reviews/${reviewId}/understand`)

    const header = page.locator('.where')
    await expect(header).toBeVisible()

    /* The two links are there, and they are what the criterion is about. */
    const pull = header.getByRole('link', { name: /on GitHub/ })
    const ticket = header.getByRole('link', { name: /on the tracker/ })
    await expect(pull).toBeVisible()
    await expect(ticket).toBeVisible()
    await expect(pull).toHaveAttribute(
      'href',
      `https://github.com/${LONG_REPO}/pull/1284`,
    )
    await expect(ticket).toHaveAttribute('href', TICKET_URL)
    await expect(ticket).toContainText(TICKET)

    /*
     * **The address is never drawn**, only the id. Asserted here as well as in the unit
     * tests because this is the one place the real page is looked at.
     */
    await expect(header).not.toContainText('linear.app')

    /* Nothing is wider than the page. */
    const { scroll, client } = await overflows(page)
    expect(
      scroll,
      `the page scrolls sideways at ${name}: ${scroll} wide in ${client}. `
      + `Past the edge: ${(await tooWide(page, width)).join(', ')}`,
    ).toBeLessThanOrEqual(client)

    /* And both links sit inside the viewport rather than merely existing. */
    for (const [what, link] of [['the pull request', pull], ['the ticket', ticket]] as const) {
      const box = await link.boundingBox()
      expect(box, `${what} is drawn`).not.toBeNull()
      expect(box!.x, `${what} starts inside the viewport at ${name}`).toBeGreaterThanOrEqual(0)
      expect(
        box!.x + box!.width,
        `${what} ends inside the viewport at ${name}`,
      ).toBeLessThanOrEqual(width + 0.5)
    }
  })

  test(`a row's two links fit at ${name} with a long repository name`, async ({ page }) => {
    await page.setViewportSize({ width, height })

    /*
     * The Reviews list rather than the queue: this review has no brief and no draft, so
     * nothing is waiting on the developer and the queue has no row for it. The list holds
     * every review the record holds, which is what that page is for.
     */
    await page.goto(`${serving.url}reviews`)

    const row = page.locator('.queue-line').filter({ hasText: '#1284' })
    await expect(row).toBeVisible()
    await expect(row.getByRole('link', { name: /pull request #1284/ })).toHaveAttribute(
      'href',
      `https://github.com/${LONG_REPO}/pull/1284`,
    )
    await expect(row.getByRole('link', { name: /on the tracker/ })).toHaveAttribute(
      'href',
      TICKET_URL,
    )

    /*
     * **The row, not the page.** At 390 this page scrolls sideways for a reason that is
     * nothing to do with these links and predates them: the repository picker above the
     * list draws one tab per repository, and a 46-character name makes that tab wider than
     * the screen — `div.filter` measures 385 pixels of content in 358. Reported in the pull
     * request rather than fixed here, because `DecisionTrack` is another change's to make;
     * asserting the page fits would be asserting a fault, and skipping the measurement
     * altogether would give up on the thing this test is for. So the assertion is scoped to
     * the row and its links, which is what DEV-936 changed.
     */
    expect(
      await tooWide(page, width, '.queue-line'),
      `nothing in a row is wider than ${name}`,
    ).toEqual([])
  })
}

/**
 * **Pressing a row's link does not open the review**, which is DEV-936's seventh criterion
 * and the reason the links are a sibling of the row's own link rather than a cell inside it.
 *
 * The unit tests assert the structure — no link out is a descendant of the link to the
 * review — and this presses the thing. A structural assertion is the stronger of the two
 * because it cannot be satisfied by accident, but it is still an argument about HTML; this
 * is a browser doing it.
 *
 * **Nothing leaves the machine**, and that is what decides how the address is read.
 * Every `https` request is aborted before the click, because Review Desk never talks to
 * GitHub and a test of it must not either — and an aborted navigation leaves the popup
 * sitting at `chrome-error://chromewebdata/`, so `page.url()` cannot say where it was
 * going. The address the browser was **asked** for can, and it is the whole of what the
 * row promises.
 *
 * At one width only. Which element a click lands on is what the two widths above measure;
 * what this is about is where it goes, and that does not change with the viewport.
 */
test('pressing a row’s link opens that link, and leaves the review alone', async ({ page, context }) => {
  const asked: string[] = []
  context.on('request', request => asked.push(request.url()))
  await context.route('https://**', route => route.abort())

  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto(`${serving.url}reviews`)

  const row = page.locator('.queue-line').filter({ hasText: '#1284' })
  await expect(row).toBeVisible()
  const before = page.url()

  for (const [what, name, wanted] of [
    ['the pull request', /pull request #1284/, `https://github.com/${LONG_REPO}/pull/1284`],
    ['the ticket', /on the tracker/, TICKET_URL],
  ] as const) {
    const [opened] = await Promise.all([
      page.waitForEvent('popup'),
      row.getByRole('link', { name }).click(),
    ])

    /*
     * Polled, because `waitForEvent('popup')` resolves when the tab is *created* and the
     * request it was created for is issued after that — read once, the list is empty.
     */
    await expect
      .poll(() => asked, { message: `${what} is what the new tab asked for` })
      .toContain(wanted)
    await opened.close()

    /* And the review was not opened behind it. */
    expect(page.url(), `${what} left the list where it was`).toBe(before)
    await expect(row).toBeVisible()
  }

  /* The row's own link still does open the review, which is what the row is for. */
  await row.locator('.row').click()
  await expect(page).toHaveURL(new RegExp(`/reviews/${reviewId}/understand$`))
})
