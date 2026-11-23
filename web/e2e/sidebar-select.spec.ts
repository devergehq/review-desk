import { join } from 'node:path'

import { expect, test } from '@playwright/test'

import { cli, scratch, serve, signedIn, stop, type Serving } from './harness'

/*
 * **A sidebar row with no round, clicked, selects its pull request in the list.** DEV-1373.
 *
 * `acme/portal#901` is a review GitHub asked for that no session has opened a round of
 * (`review requested`, DEV-996). It has no review page, so the sidebar drew it as text and
 * clicking it did nothing. It is a link now, to the Pull requests list with that row selected.
 *
 * In a browser because the click is the router's and the selection waits on the inbox read:
 * a mounted component sees neither the navigation nor the order the two arrive in.
 *
 * ## Its own database and its own server
 *
 * The acceptance suite's seed holds `#901` too, but the specs before this one change that
 * desk, and by the time this one ran `#901` was no longer drawn as *Not started* there. So it
 * seeds the two rows it needs — one round nobody is working on, one request with no round —
 * and serves them on port 0, as `header-links.spec.ts` does.
 */

const PR = 901
let serving: Serving
let seeded: string

test.beforeAll(async () => {
  const database = join(scratch(), 'sidebar-select.db')
  cli(database, [
    'review', 'open', '--repo', 'acme/portal', '--pr', '902', '--head', '3c1e2a9',
    '--title', 'Skip a scheduled export that has nothing to send',
  ])
  cli(database, [
    'review', 'requested', '--repo', 'acme/portal', '--pr', String(PR),
    '--requested-at', String(Math.floor(Date.now() / 1000) - 24 * 60 * 60),
  ])
  serving = await serve(database)
  seeded = serving.url.endsWith('/') ? serving.url : `${serving.url}/`
})

test.beforeEach(async ({ context }) => {
  await signedIn(context, serving)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

const pr = PR

test('clicking a not-started row in the sidebar selects it in the list', async ({ page }) => {
  /* From Rules, so the click is a navigation into the list rather than within it. */
  await page.goto(`${seeded}rules`)

  const group = page.locator('#sidebar .group--being_reviewed')
  await group.locator('summary').click()
  const row = group.locator('.side-row').filter({ hasText: `#${pr}` })
  await expect(row).toContainText('Not started')
  await expect(row).toHaveAttribute('href', `/?pr=acme/portal%23${pr}`)

  /* A link the keyboard reaches, as every other row is. */
  await row.focus()
  await expect(row).toBeFocused()

  await row.click()
  /* Taken on arrival, and replaced with the tab that draws it. */
  await expect(page).toHaveURL(`${seeded}?view=reviewing`)

  await expect(page.getByRole('tab', { name: /^Being reviewed/ })).toHaveAttribute('aria-selected', 'true')
  const selected = page.locator('#panel-reviewing .queue-line--selected')
  await expect(selected).toHaveCount(1)
  await expect(selected).toContainText(`#${pr}`)
  await expect(page.getByTestId('list-preview')).toHaveAttribute('aria-label', `#${pr}, previewed`)
})

test('loading the selection address directly selects the same row', async ({ page }) => {
  /* The ticket's own spelling, with the slash escaped, which the router reads the same. */
  await page.goto(`${seeded}?pr=acme%2Fportal%23${pr}`)
  await expect(page.locator('#panel-reviewing .queue-line--selected')).toContainText(`#${pr}`)
  await expect(page.getByTestId('list-preview')).toHaveAttribute('aria-label', `#${pr}, previewed`)
})

test('the same row clicked again after J or K has moved the selection selects it again', async ({ page }) => {
  /* Self-review round 1, F2: while the address still said `?pr=`, this click went nowhere. */
  await page.goto(`${seeded}?pr=acme%2Fportal%23${pr}`)
  const selected = page.locator('#panel-reviewing .queue-line--selected')
  await expect(selected).toContainText(`#${pr}`)

  /* Away from it, whichever way there is room: the tab sorts its two rows by age. */
  const first = await page.locator('#panel-reviewing .queue-line').first().getAttribute('data-row-key')
  await page.locator('body').press(first === `acme/portal#${pr}` ? 'j' : 'k')
  await expect(selected).not.toContainText(`#${pr}`)

  const group = page.locator('#sidebar .group--being_reviewed')
  if (!(await group.locator('details').evaluate(fold => (fold as HTMLDetailsElement).open))) {
    await group.locator('summary').click()
  }
  await group.locator('.side-row').filter({ hasText: `#${pr}` }).click()
  await expect(selected).toContainText(`#${pr}`)
})
