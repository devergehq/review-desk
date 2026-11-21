import { expect, test, type Page } from '@playwright/test'

import { readAddresses } from './harness'

/*
 * **Findings, in a browser.** DEV-1328, S4: the list beside the one selected finding.
 *
 * Four things only a browser can answer: an address naming a finding (the router's), the
 * old code review address keeping that name through its redirect, keys that belong to
 * one page and not the page before it, and whether the hunk forces the page sideways
 * once the conversation column takes its 380px.
 *
 * Read-only against the acceptance suite's seeded server: nothing here decides anything.
 */

const { seeded, structuredReviewId: id } = readAddresses()

const picked = (page: Page) => page.locator('.row--picked .key')

/** The finding's key on the second row of the list showing: never the one it opens on. */
async function secondKey(page: Page): Promise<string> {
  await page.goto(`${seeded}reviews/${id}/findings`)
  await expect(picked(page)).toBeVisible()
  return (await page.locator('.row .key').nth(1).textContent())!.trim()
}

test('?finding= puts that finding on the bench', async ({ page }) => {
  const key = await secondKey(page)
  await page.goto(`${seeded}reviews/${id}/findings?finding=${key}`)
  await expect(picked(page)).toHaveText(key)
  await expect(page.locator('#the-finding .finding-head')).toContainText(key)
})

test('#F9 still does, and so does the old code review address carrying it', async ({ page }) => {
  const key = await secondKey(page)
  await page.goto(`${seeded}reviews/${id}/findings#${key}`)
  await expect(picked(page)).toHaveText(key)

  /* The contract with DEV-1324's redirect: the fragment survives, and is read. */
  await page.goto(`${seeded}reviews/${id}/code-review#${key}`)
  await expect(page).toHaveURL(`${seeded}reviews/${id}/findings#${key}`)
  await expect(picked(page)).toHaveText(key)
})

/*
 * **J, K and Enter belong to the page they are pressed on.** The Pull requests list binds
 * all three (DEV-1325) and Findings binds J and K. Each adds its listener on mount and takes
 * it away on unmount, so arriving here from the list leaves the list's keys behind. Enter is
 * the observable one: were the list still listening, Enter at the page would open its
 * selected pull request and take the reader off this tab.
 */
test('the list’s keys do not follow the reader onto Findings', async ({ page }) => {
  await page.goto(seeded)
  await page.keyboard.press('j')

  /* Arrive by the router, not by a load, so the list's page is unmounted, not discarded. */
  await page.evaluate(path => window.history.pushState({}, '', path), `/reviews/${id}/findings`)
  await page.evaluate(() => window.dispatchEvent(new PopStateEvent('popstate')))
  await expect(page).toHaveURL(`${seeded}reviews/${id}/findings`)
  await expect(picked(page)).toBeVisible()
  const first = (await picked(page).textContent())!.trim()

  await page.locator('body').click({ position: { x: 1, y: 1 } })
  await page.keyboard.press('j')
  await expect(picked(page)).not.toHaveText(first)
  await page.keyboard.press('k')
  await expect(picked(page)).toHaveText(first)

  await page.keyboard.press('Enter')
  await page.waitForTimeout(300)
  /* Still Findings. J and K write the finding into the query, which is this page's own. */
  await expect(page).toHaveURL(new RegExp(`/reviews/${id}/findings\\?finding=F[0-9]+$`))
})

/*
 * **The hunk never forces sideways page scroll at 1,600 with the conversation column.**
 * DEV-1322's contract 4, which #86's first CI run failed on: CI is Linux and lays text out
 * differently from macOS. So the margin is asserted, not just the nought — the hunk ends at
 * least 16px inside the main column, which a font a few pixels wider cannot use up — and it
 * is asserted on every finding in every list, since one long line is all it takes.
 */
test('no finding’s hunk forces the page sideways at 1,600 beside the conversation', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 900 })
  await page.goto(`${seeded}reviews/${id}/findings`)
  await expect(page.locator('.shell-conversation')).toBeVisible()
  /* The findings are read after the shell draws: measure once they are on the bench. */
  await expect(picked(page)).toBeVisible()

  const lists = page.locator('.controls [role="radio"]')
  let hunks = 0
  for (let list = 0; list < await lists.count(); list += 1) {
    await lists.nth(list).click()
    await page.waitForTimeout(100)
    const rows = page.locator('.row')
    for (let row = 0; row < await rows.count(); row += 1) {
      await rows.nth(row).click()
      const measured = await page.evaluate(() => {
        const main = document.querySelector('#main')!.getBoundingClientRect()
        const hunk = document.querySelector('#the-finding .hunk')?.getBoundingClientRect()
        return {
          sideways: document.documentElement.scrollWidth - document.documentElement.clientWidth,
          room: hunk ? main.right - hunk.right : null,
        }
      })
      expect(measured.sideways, `list ${list}, row ${row}`).toBe(0)
      if (measured.room != null) {
        hunks += 1
        expect(measured.room, `list ${list}, row ${row}: the hunk's room inside the main column`)
          .toBeGreaterThanOrEqual(16)
      }
    }
  }
  expect(hunks, 'at least one finding drew a hunk').toBeGreaterThan(0)
})

/*
 * **Choosing a part leaves the keys working.** A focused `<select>` is a field, and a key
 * pressed at a field is not a key for the page, so the select gives focus back once a part
 * is chosen. Found by this branch's self-review.
 */
test('the keys still work after a part is chosen', async ({ page }) => {
  await page.goto(`${seeded}reviews/${id}/findings`)
  await expect(picked(page)).toBeVisible()
  const select = page.locator('.parts-select')
  test.skip(await select.count() === 0, 'this review has no parts')

  await select.selectOption({ index: 0 })
  await select.selectOption({ index: 1 })
  await expect(select).not.toBeFocused()
  const rows = page.locator('.row')
  test.skip(await rows.count() < 2, 'the part holds one row')
  const first = (await picked(page).textContent())!.trim()
  await page.keyboard.press('j')
  await expect(picked(page)).not.toHaveText(first)
})
