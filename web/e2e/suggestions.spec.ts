import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test } from '@playwright/test'

import {
  cli,
  fresh,
  scratch,
  seedRoundTwoDraft,
  seedStructuredReview,
  serve,
  signedIn,
  stop,
  type Serving,
} from './harness'

/*
 * **The session's suggestions, accepted and dismissed through the binary** — DEV-1334, S8.
 *
 * A server of its own, so a finding added here changes no count another spec reads. The
 * session's answer goes in the way a session sends it, `review-desk message reply --answer`,
 * which the server checks against the contract before it is kept. Everything after that is
 * the developer's, in the browser: Accept adds the finding through `addFinding` and Findings
 * marks it new; Dismiss keeps the dismissal and then the reason; a follow-up fills the box.
 */

let serving: Serving
let database: string
let reviewId: number
let round2: number

const DRAFTED = 'Fair. The tenant row is soft-deleted, so the schedule still finds it.'

const ANSWER = {
  kind: 'answer',
  headline: 'Yes. The web path has the same hole, and it isn’t in the findings yet.',
  points: [{ basis: 'ran', text: 'A web export with no columns set returned all 14.' }],
  cites: ['app/Http/Controllers/ExportController.php:41'],
  suggests: [
    {
      type: 'new_finding',
      severity: 'should',
      title: 'The web path exports private columns when none are set',
      where: 'app/Http/Controllers/ExportController.php:41',
      basis: 'ran',
    },
    { type: 'follow_up', question: 'Does the CSV download use a third path?' },
    { type: 'set_aside', finding: 'F1', why: 'The web path finding covers it.' },
  ],
}

test.beforeAll(async () => {
  const directory = scratch()
  database = fresh(directory)
  reviewId = seedStructuredReview(database, 'Give a scheduled export its own context').reviewId
  const answer = join(directory, 'answer.json')
  writeFileSync(answer, JSON.stringify(ANSWER))
  cli(database, ['message', 'reply', '--review', String(reviewId), '--step', 'code_review', '--answer', answer])

  /* A round 2 whose author answered F9, and a session that drafted the developer's reply. */
  round2 = seedRoundTwoDraft(database).reviewId
  cli(database, [
    'outcome', 'add', '--review', String(round2), '--finding', '9', '--kind', 'replied',
    '--by', 'author', '--detail', 'Tenants are never hard-deleted, so the schedule cannot outlive one.',
  ])
  const drafted = join(directory, 'drafted.json')
  writeFileSync(drafted, JSON.stringify({
    kind: 'answer',
    about: { reply: 'F9' },
    headline: 'The author is right: tenants are soft-deleted here.',
    cites: ['F9'],
    suggests: [{ type: 'draft_reply', finding: 'F9', call: 'accept_reason', words: DRAFTED }],
  }))
  cli(database, ['message', 'reply', '--review', String(round2), '--step', 'harvest', '--answer', drafted])
  serving = await serve(database)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

test('Accept adds the suggested finding to To post, and Findings marks it new', async ({ page }) => {
  await signedIn(page.context(), serving)
  await page.goto(`${serving.url}reviews/${reviewId}/findings`)
  const panel = page.getByTestId('conversation-panel')
  await expect(panel.getByText('New finding: The web path exports private columns')).toBeVisible()

  await panel.getByTestId('accept-new_finding').click()
  await expect(panel.getByText(/Added as F\d+ · should · ran/)).toBeVisible()
  await expect(page.getByTestId('finding-new')).toHaveCount(1)
  await expect(page.locator('.row', { has: page.getByTestId('finding-new') }))
    .toContainText('The web path exports private columns when none are set')

  /* And the record says it once: a reload draws the same, with no second Accept to press. */
  await page.reload()
  await expect(panel.getByText(/Added as F\d+ · should · ran/)).toBeVisible()
  await expect(panel.getByTestId('accept-new_finding')).toHaveCount(0)
})

test('Dismiss is one click, and the reason is kept after it', async ({ page }) => {
  await signedIn(page.context(), serving)
  await page.goto(`${serving.url}reviews/${reviewId}/findings`)
  const panel = page.getByTestId('conversation-panel')

  await panel.getByTestId('dismiss-set_aside').click()
  await expect(panel.getByText('Dismissed', { exact: true })).toBeVisible()
  await panel.getByRole('button', { name: 'Say why' }).click()
  await panel.getByLabel('Why you dismissed it').fill('F1 is about the scheduled path.')
  await panel.getByRole('button', { name: 'Keep the reason' }).click()
  await expect(panel.getByText('Dismissed · “F1 is about the scheduled path.”')).toBeVisible()
})

test('a follow-up fills the ask box and sends nothing', async ({ page }) => {
  await signedIn(page.context(), serving)
  await page.goto(`${serving.url}reviews/${reviewId}/findings`)
  const panel = page.getByTestId('conversation-panel')

  await panel.getByTestId('accept-follow_up').click()
  /*
   * This server has no Allele and the session has written, so the panel is one way
   * (DEV-1335): there is no textarea, and the question is held in the one-way box, which
   * Copy a reference carries to the developer's own session.
   */
  await expect(panel).toHaveAttribute('data-mode', 'one_way')
  await expect(panel.getByTestId('one-way-question')).toContainText('Does the CSV download use a third path?')
  await expect(panel.getByText('In the reference. Copy it and ask in your Claude Code session.')).toBeVisible()
  const listed = JSON.parse(cli(database, ['message', 'list', '--review', String(reviewId), '--json']))
  expect(JSON.stringify(listed)).not.toContain('"author":"developer"')
})

test('a drafted reply fills the field on Replies, and only the developer sends it', async ({ page }) => {
  await signedIn(page.context(), serving)
  await page.goto(`${serving.url}reviews/${round2}/replies`)
  const panel = page.getByTestId('conversation-panel')

  await panel.getByTestId('accept-draft_reply').click()
  const field = page.locator('#reason-9')
  /* The field keeps the author's reason; the session's draft is drawn above it as the session's. */
  await expect(field).toHaveValue('Tenants are never hard-deleted, so the schedule cannot outlive one.')
  await expect(page.locator('#reply-F9 .drafted-words')).toHaveText(DRAFTED)
  await expect(page.locator('#reply-F9 .drafted')).toContainText('nothing is sent until you do')
  /* Nothing written yet: F9 still waits on an answer. */
  await expect(page.locator('.reply-row--contested#reply-F9')).toBeVisible()

  await field.fill('Fair: soft deletes keep the tenant row, so the schedule still finds it.')
  await page.locator('#reply-F9').getByRole('button', { name: 'Accept it' }).click()
  await expect(page.locator('.reply-row--settled#reply-F9')).toBeVisible()
  await expect(panel.getByText('Answered F9 on Replies')).toBeVisible()

  const record = await page.request.get(`${serving.url}api/v1/reviews/${round2}`)
  const detail = await record.json() as { outcomes: Array<{ finding_seq: number, kind: string, actor: string, detail: string | null }> }
  expect(detail.outcomes.at(-1)).toMatchObject({
    finding_seq: 9,
    kind: 'rejected',
    actor: 'developer',
  })
})
