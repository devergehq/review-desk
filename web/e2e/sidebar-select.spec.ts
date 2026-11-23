import { expect, test } from '@playwright/test'

import { readAddresses } from './harness'

/*
 * **A sidebar row with no round, clicked, selects its pull request in the list.** DEV-1373.
 *
 * `acme/portal#901` is the seed's review GitHub asked for that no session has opened a round
 * of (`reviewRequestPr`, DEV-996). It has no review page, so the sidebar drew it as text and
 * clicking it did nothing. It is a link now, to the Pull requests list with that row selected.
 *
 * In a browser because the click is the router's and the selection waits on the inbox read:
 * a mounted component sees neither the navigation nor the order the two arrive in.
 *
 * Read-only against the acceptance suite's seeded server, at the config's 1440 wide, where
 * the sidebar and the preview are both drawn.
 */

const { seeded, reviewRequestPr: pr } = readAddresses()

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
  await expect(page).toHaveURL(`${seeded}?pr=acme/portal%23${pr}`)

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
