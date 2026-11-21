import { expect, test } from '@playwright/test'

import { readAddresses } from './harness'

/*
 * **The five step addresses the tabs replaced, each followed in a browser.** DEV-1324, and
 * DEV-1322's criterion that every old URL redirects to its tab.
 *
 * In a browser because a redirect is the router's: `mountSuspended` mounts a component, not
 * a route, so a unit test of these pages would see a middleware that never ran. One test
 * per route, as the ticket asks, and each asserts both halves — where it lands, and that a
 * fragment the link carried is still on the address when it gets there.
 *
 * Read-only against the acceptance suite's seeded server.
 */

const { seeded, structuredReviewId: id } = readAddresses()

const ROUTES = [
  { from: 'understand', to: 'understanding', band: '', hash: '#what-was-wrong' },
  { from: 'alternatives', to: 'understanding', band: '#what-was-chosen', hash: '#what-was-built' },
  { from: 'solution', to: 'understanding', band: '#what-was-built', hash: '#what-was-chosen' },
  { from: 'code-review', to: 'findings', band: '', hash: '#F9' },
  { from: 'what-happened', to: 'replies', band: '', hash: '#nothing-back' },
] as const

for (const route of ROUTES) {
  test(`/${route.from} redirects to the ${route.to} tab, keeping a fragment`, async ({ page }) => {
    /* Without a fragment: the tab, at the step's own band where it had one. */
    await page.goto(`${seeded}reviews/${id}/${route.from}`)
    await expect(page).toHaveURL(`${seeded}reviews/${id}/${route.to}${route.band}`)
    await expect(page.locator('.tabs [aria-current="page"]')).toContainText(
      route.to.charAt(0).toUpperCase() + route.to.slice(1),
    )

    /* With one: the same tab, and the fragment the link carried rather than the band. */
    await page.goto(`${seeded}reviews/${id}/${route.from}${route.hash}`)
    await expect(page).toHaveURL(`${seeded}reviews/${id}/${route.to}${route.hash}`)
  })
}

/* And the query, which `retiredRouteTarget` carries over beside the fragment. */
test('a redirect keeps the query as well as the fragment', async ({ page }) => {
  await page.goto(`${seeded}reviews/${id}/code-review?from=queue#F9`)
  await expect(page).toHaveURL(`${seeded}reviews/${id}/findings?from=queue#F9`)
})

test('the review’s own address is Overview, not a redirect', async ({ page }) => {
  await page.goto(`${seeded}reviews/${id}`)
  await expect(page).toHaveURL(`${seeded}reviews/${id}`)
  await expect(page.locator('.tabs [aria-current="page"]')).toHaveText('Overview')
})
