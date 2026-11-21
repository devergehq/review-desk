/*
 * **A sign-off is of the draft the page showed** — DEV-1346, in a browser.
 *
 * The bug DEV-1345 names, end to end: the developer opens a draft, a session rewrites it
 * through the command line while they read, and they press "Sign off and post". The click
 * must not approve text they never saw. The server refuses with `changed_since_shown`, and
 * the page reads the draft again and says so in its own words.
 *
 * And the other half: the same press from a browser that never signed in is refused, which
 * is what an agent with `curl` and no cookie gets too.
 *
 * Its own server, for the reason `rounds.spec.ts` gives: rewriting a seeded draft under the
 * page would change what the acceptance path asserts.
 */
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test } from '@playwright/test'

import { cli, scratch, serve, signedIn, stop, type Serving } from './harness'

let serving: Serving
let directory: string
let database: string
let reviewId: number
/* A second review, for the browser with no cookie: the first is signed off by then. */
let anotherId: number
/* A third, for a draft re-drawn by a re-read the developer did not ask for. */
let redrawnId: number
/* A fourth, with a nit held back, for the write that does not move the draft. */
let ticketedId: number

function putDraft(text: string, review: number = reviewId) {
  const path = join(directory, `shown-body-${review}-${Date.now()}.json`)
  writeFileSync(path, JSON.stringify([{ kind: 'prose', text }]))
  cli(database, ['draft', 'put', '--review', String(review), '--sections', path])
}

function openReview(pr: string, head: string): number {
  const opened = cli(database, [
    'review', 'open',
    '--repo', 'acme/portal',
    '--pr', pr,
    '--head', head,
    '--title', 'Bind the sign-off to the draft that was shown',
  ])
  return Number(opened.match(/\d+/)?.[0])
}

test.beforeAll(async () => {
  directory = scratch()
  database = join(directory, 'shown.db')
  reviewId = openReview('734', 'a7b8c90')
  putDraft('The draft the developer opened.')
  anotherId = openReview('735', 'b8c9d01')
  putDraft('A draft for the browser that has no cookie.', anotherId)
  redrawnId = openReview('736', 'c9d0e12')
  putDraft('The draft as it was when the page opened.', redrawnId)
  ticketedId = openReview('737', 'd0e1f23')
  const nit = join(directory, 'shown-nit.json')
  writeFileSync(nit, JSON.stringify({
    lens: 'conventions',
    severity: 'nit',
    claim: 'Name the constant after what it holds',
    file: 'app/Exports/ScheduledExport.php',
    line: 12,
    disposition: 'held_back',
  }))
  cli(database, ['finding', 'add', '--review', String(ticketedId), '--file', nit])
  putDraft('The draft before the nit ticket was asked for.', ticketedId)
  serving = await serve(database)
})

test.beforeEach(async ({ context }) => {
  await signedIn(context, serving)
})

test.afterAll(async () => {
  await stop(serving)
})

test('a draft rewritten while it was being read is not signed off', async ({ page }) => {
  await page.goto(`${serving.url}reviews/${reviewId}/draft`)
  await expect(page.getByText('The draft the developer opened.')).toBeVisible()

  /* The session rewrites it behind the page. */
  putDraft('A different draft, written while the developer was reading.')

  await page.getByRole('button', { name: 'Sign off and post' }).click()

  await expect(
    page.getByText('The draft changed while you were reading it · review the change.'),
  ).toBeVisible()
  /* Read again, so what is on screen now is what the record holds. */
  await expect(page.getByText('A different draft, written while the developer was reading.'))
    .toBeVisible()
  const record = JSON.parse(
    cli(database, ['draft', 'show', '--review', String(reviewId), '--json']),
  ) as { draft: { status: string } }
  expect(record.draft.status).toBe('ready')

  /* The next press is on what the developer now sees, and goes through. */
  await page.getByRole('button', { name: 'Sign off and post' }).click()
  await expect(page.getByText(/Signed off\./).first()).toBeVisible()
})

test('a browser that never signed in cannot sign off', async ({ page, context }) => {
  await context.clearCookies()

  await page.goto(`${serving.url}reviews/${anotherId}/draft`)
  await page.getByRole('button', { name: 'Send back with a note' }).click()
  await page.locator('textarea#send-back-note').fill('Cut F1')
  await page.getByRole('button', { name: 'Send back', exact: true }).click()

  await expect(page.getByText(/only the developer can do this/i)).toBeVisible()
})

test('a draft rewritten and then re-read says so before anything is signed off', async ({ page }) => {
  await page.goto(`${serving.url}reviews/${redrawnId}/draft`)
  await expect(page.getByText('The draft as it was when the page opened.')).toBeVisible()

  putDraft('Rewritten behind the page.', redrawnId)

  /* Any write re-reads every open screen: here, asking the session something. */
  await page.getByText('Ask the agent about Draft').click()
  await page.getByPlaceholder('Ask why, or ask for rework').fill('Why F1?')
  await page.getByRole('button', { name: 'Ask', exact: true }).click()

  await expect(page.getByText('Rewritten behind the page.')).toBeVisible()
  await expect(
    page.getByText('The draft changed while you were reading it · review the change.'),
  ).toBeVisible()
})

/*
 * **Asking for the nit ticket does not move the draft, so its re-read is not the developer's
 * change.** The second round of DEV-1346's self-review found the notice muted here: the page
 * muted it for every one of its own writes, and this one writes nothing to the draft.
 */
test('a rewrite that arrives with the nit ticket\'s re-read says so too', async ({ page }) => {
  await page.goto(`${serving.url}reviews/${ticketedId}/draft`)
  await expect(page.getByText('The draft before the nit ticket was asked for.')).toBeVisible()

  putDraft('Rewritten before the nit ticket was asked for.', ticketedId)
  await page.getByRole('button', { name: 'File as one ticket for an agent' }).click()

  await expect(page.getByText('Rewritten before the nit ticket was asked for.')).toBeVisible()
  await expect(
    page.getByText('The draft changed while you were reading it · review the change.'),
  ).toBeVisible()
})
