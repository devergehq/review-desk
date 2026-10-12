import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { cli, scratch, serve, stop, type Serving } from './harness'

/*
 * **Step 7's page at real density, measured at 1440 and 390.**
 *
 * DEV-958's last two acceptance criteria, and both are criteria about *measurement*:
 * fixtures at 14 findings, 2 replies, 7 code changes and one reply over 80 words, looked at
 * in the repository's Playwright Chromium at both widths. Reasoning about a 108-word
 * quotation inside a row that also carries a path, a commit id and two links is not the
 * same as looking at it — which is the whole reason this file exists rather than a comment
 * saying it should be fine. The same lesson this repository learned the expensive way: a
 * green unit suite is not evidence about what a page looks like.
 *
 * ## Its own database and its own server
 *
 * The acceptance suite's seed is asserted against by name and by count — "3 reviews need
 * you", the repository chips, the Reviews list's heading — and a harvested review added to
 * it would move every one of them: `Store::reviews_with_author_activity` has no `pr_state`
 * filter, so any review carrying an outcome by the author or the harvester is a row in the
 * queue's "Author replied". So this seeds a database of its own and starts a server of its
 * own on port 0, exactly as `header-links.spec.ts` does and for the same reason.
 *
 * ## What this seed cannot do, said rather than worked around
 *
 * **It cannot record a pull-request state.** `pr_state` and `harvested_at` are written by
 * one statement in `crates/harvest/src/run.rs` and by nothing else — there is no
 * `review-desk` command for either, deliberately, because they are GitHub's facts and the
 * harvester is the only thing that reads GitHub. Running a real harvest is out of the
 * question here: it would need `gh` and a real pull request.
 *
 * So the review measured below is one no harvest has read, whose outcomes were recorded by
 * `outcome add` — which is a real state, is what the page's own notice is about, and is
 * exactly the shape the rows take in any other state. The three other headlines (merged,
 * closed, open) are one sentence each from `prEndLabel` and are measured in
 * `test/outcomes.spec.ts` and `test/what-happened.nuxt.spec.ts`. What is measured here is
 * the layout, which is what a browser is for.
 */

/** The reply DEV-958 asks for: 108 words, which is over its 80. */
const LONG_REPLY
  = 'I pushed a fix for this one but I want to flag that it is not the fix you suggested. '
    + 'Taking the column policy out of the query builder means the scheduled export and the '
    + 'interactive export stop sharing it, and the interactive one is the path with the test '
    + 'coverage, so I would rather keep them together and gate the scheduled path on the '
    + 'same policy object. That is what the new commit does. If you still want them split I '
    + 'can do it, but it should be its own change with its own tests rather than something I '
    + 'slip into a fix for a reporting bug.'

const SHORT_REPLY = 'Moved to a temporary name and renamed on success.'

/**
 * A path long enough to be the thing that breaks, with **no break opportunity in its last
 * segment**.
 *
 * `header-links.spec.ts` carries the argument for the shape: a hyphen is a line-break
 * opportunity, so a hyphenated token wraps wherever it needs to and never sets a
 * min-content width. This is 53 characters whole and 39 in its last segment, with no
 * hyphen and no underscore in either — so the segment sets one, and a finding's row draws
 * it as `path:line` beside a commit id and two links.
 */
const LONG_PATH = 'app/Reporting/ScheduledExportColumnPolicyResolver.php'

/** The sixteen findings' severities, as a review of this size really divides. */
const SEVERITIES = [
  'blocker',
  'should', 'should', 'should', 'should', 'should', 'should',
  'question', 'question', 'question',
  'nit', 'nit', 'nit', 'nit',
  /* The two held back for a ticket, which are nits as a held-back finding usually is. */
  'nit', 'nit',
]

const FILES = [
  LONG_PATH,
  'app/Reporting/ScheduledExport.php',
  'app/Reporting/ExportContext.php',
  'app/Http/Controllers/ExportController.php',
  'jobs/ExportTenant.php',
  'app/Reporting/QueryBuilder.php',
  'app/Reporting/Columns.php',
]

/** The two widths the criterion names: the boards' own width, and a phone. */
const WIDTHS = [
  { name: '1440', width: 1440, height: 900 },
  { name: '390', width: 390, height: 844 },
]

let serving: Serving
let reviewId: number

/** The harvester's own `replied` detail, in the shape `run.rs` writes it. */
function repliedDetail(seq: number, body: string): string {
  return JSON.stringify({
    comment_id: 2_315_000_000 + seq,
    in_reply_to: 2_314_990_000 + seq,
    login: 'dana-okoye',
    body,
    time_basis: 'github',
  })
}

/** The harvester's own `code_changed_after` detail. */
function changedDetail(seq: number, linesChanged: boolean): string {
  return JSON.stringify({
    file: FILES[(seq - 1) % FILES.length],
    line: 40 + seq * 7,
    lines_changed: linesChanged,
    file_status: 'modified',
    commit: `65eff6${seq}a04f7b9c0d8e5a1b2c3d4e5f6071829`,
    basis: 'compare 633e04a3f9b...65eff6321a0',
  })
}

function at(when: string): string {
  return String(Math.floor(Date.parse(when) / 1000))
}

test.beforeAll(async () => {
  const directory = scratch()
  const database = join(directory, 'what-happened.db')

  const opened = cli(database, [
    'review', 'open',
    '--repo', 'acme/portal',
    '--pr', '601',
    '--head', '65eff63',
    '--title', 'Give a scheduled export its own context rather than reading the web request’s',
    '--ticket', 'ABC-402',
  ])
  reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) throw new Error(`review open printed no id: ${opened}`)

  /*
   * Fourteen findings, every one of them in the draft, which is what makes them posted.
   *
   * `finding add` takes a JSON document rather than flags — the shape is `NewFinding` —
   * so each is written to the scratch directory and handed over by path, which is what
   * `harness.ts` does for the acceptance seed's own findings.
   */
  for (let seq = 1; seq <= 16; seq += 1) {
    const document = {
      lens: 'correctness',
      severity: SEVERITIES[seq - 1]!,
      file: FILES[(seq - 1) % FILES.length]!,
      line: 40 + seq * 7,
      claim: `What F${seq} says is wrong, written at the length a session really writes a `
        + 'claim at, which is a sentence or two and well past the twelve words a row has '
        + 'room for.',
      /*
       * **F15 and F16 are held back for a ticket, and their files change too.** The
       * harvester writes against any finding of the review — `harvest_code_changes`
       * iterates all of them and asks only for a file — so a held-back nit carries an
       * outcome, and this page draws a row only for what was posted. Without them the
       * section that says what the record holds about the findings it draws no row for
       * is never drawn in a browser.
       */
      ...(seq >= 15
        ? {
            disposition: 'held_back',
            disposition_reason: 'a naming nit; it goes to the ticket with the rest',
          }
        : {}),
      /*
       * F4 carries no title, so the page has one row drawing a claim cut to a title's
       * length — and saying that it cut it, which is the rule `HeldBackRows` states.
       */
      ...(seq === 4 ? {} : { title: `What F${seq} says is wrong` }),
    }
    const path = join(directory, `finding-${seq}.json`)
    writeFileSync(path, JSON.stringify(document))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])

    /*
     * **The comment the finding was posted as, as GitHub's numeric id.** It is what the
     * harvester matches a thread by and what `commentUrl` builds the row's link from.
     * F13 and F14 carry none: a finding the review raised in its body alone has no comment
     * to link and no thread a reply could be read from, which is what the page's footnote
     * is about.
     */
    if (seq <= 12) {
      cli(database, [
        'finding', 'link',
        '--review', String(reviewId),
        '--finding', String(seq),
        '--github-comment', String(2_314_990_000 + seq),
      ])
    }
  }

  /* F1: a reply, its thread resolved, and its code changed — in that order in time. */
  const outcomes: Array<[number, string, string, string, string]> = [
    [1, 'replied', 'author', repliedDetail(1, LONG_REPLY), '2026-10-11T11:02:00Z'],
    [1, 'thread_resolved', 'author', JSON.stringify({
      thread: 2_314_990_001,
      resolved_by: 'dana-okoye',
      time_basis: 'observed',
    }), '2026-10-12T09:15:00Z'],
    [1, 'code_changed_after', 'harvester', changedDetail(1, true), '2026-10-12T16:40:00Z'],

    /* The second reply. */
    [2, 'replied', 'author', repliedDetail(2, SHORT_REPLY), '2026-10-11T11:30:00Z'],

    /* Six more findings whose code changed: seven in all with F1's. */
    [2, 'code_changed_after', 'harvester', changedDetail(2, true), '2026-10-12T10:00:00Z'],
    [3, 'code_changed_after', 'harvester', changedDetail(3, true), '2026-10-12T11:00:00Z'],
    [5, 'code_changed_after', 'harvester', changedDetail(5, true), '2026-10-12T12:00:00Z'],
    /* F6's file changed and its own lines did not, which is its own sentence. */
    [6, 'code_changed_after', 'harvester', changedDetail(6, false), '2026-10-12T13:00:00Z'],
    [8, 'code_changed_after', 'harvester', changedDetail(8, true), '2026-10-12T14:00:00Z'],
    [11, 'code_changed_after', 'harvester', changedDetail(11, true), '2026-10-12T15:00:00Z'],

    /* And one decision of the developer's own, which this page must not draw. */
    [12, 'cut', 'developer', 'formatting nit', '2026-10-10T08:00:00Z'],

    /* The two held-back findings' files changed in the same push. */
    [15, 'code_changed_after', 'harvester', changedDetail(15, true), '2026-10-12T16:00:00Z'],
    [16, 'code_changed_after', 'harvester', changedDetail(16, true), '2026-10-12T16:05:00Z'],
  ]

  for (const [finding, kind, by, detail, when] of outcomes) {
    cli(database, [
      'outcome', 'add',
      '--review', String(reviewId),
      '--finding', String(finding),
      '--kind', kind,
      '--by', by,
      '--detail', detail,
      '--at', at(when),
    ])
  }

  /* The seven steps, with step 7 running — which is the state a posted review sits in. */
  const steps: Array<[number, string, string]> = [
    [1, 'done', 'passes'],
    [2, 'done', 'confirmation not required'],
    [3, 'done', '3 options, one built'],
    [4, 'done', '4 parts'],
    [5, 'done', '14 findings, 1 blocker'],
    [6, 'done', 'signed off'],
    [7, 'running', '2 replies back'],
  ]
  for (const [number, state, detail] of steps) {
    cli(database, [
      'layer', 'set',
      '--review', String(reviewId),
      '--layer', String(number),
      '--state', state,
      '--detail', detail,
    ])
  }

  serving = await serve(database)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

/** Whether anything on the page is wider than the page. `header-links.spec.ts`' helper. */
async function overflows(page: Page): Promise<{ scroll: number, client: number }> {
  return page.evaluate(() => ({
    scroll: document.documentElement.scrollWidth,
    client: document.documentElement.clientWidth,
  }))
}

/** What is too wide, named so that a failure says which element to go and look at. */
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
  test(`the page holds fourteen findings at ${name}`, async ({ page }) => {
    await page.setViewportSize({ width, height })
    await page.goto(`${serving.url}reviews/${reviewId}/what-happened`)

    /* The trail is there, step 7 is where you are, and it is called what it holds. */
    await expect(page.locator('.face--here .name')).toHaveText('7 What happened')

    /* Seven findings had something come back; the other seven are counted and folded. */
    await expect(page.locator('.answered-row')).toHaveCount(7)
    await expect(page.locator('.silent-row')).toHaveCount(0)
    await expect(page.locator('.fold')).toContainText('7 findings nothing came back on')

    /* And the fourteen posted are what the figures are about, not the sixteen recorded. */
    await expect(page.locator('.sub')).toContainText('14 findings were posted')
    await expect(page.locator('.sub')).toContainText(
      '7 of them had something come back: 2 replies, 7 whose code changed',
    )
    await expect(page.locator('main')).toContainText(
      'Outcomes on findings this page does not list',
    )
    await expect(page.locator('main')).toContainText('2 outcomes')
    await expect(page.locator('main')).toContainText(
      'Against findings this review did not post',
    )

    /*
     * The 108-word reply is drawn whole — and the length is asserted here rather than
     * taken on trust, because "one reply over 80 words" is the acceptance criterion this
     * seed exists to satisfy and a fixture that quietly shrank would leave the page
     * measured at a length no author writes at.
     */
    expect(LONG_REPLY.trim().split(/\s+/).length).toBeGreaterThan(80)
    await expect(page.locator('.quote').first()).toContainText(LONG_REPLY)

    /*
     * **The severity's hidden label keeps its space through the real build.** Vue's
     * `whitespace: 'condense'` deletes a whitespace-only text node between two elements, so
     * the separator is `&#32;` — and whether an entity survives the compiler, the bundler
     * and the binary is a question for the thing that serves it rather than for a test
     * environment's renderer.
     */
    expect(await page.locator('.answered-row .severity').first().textContent())
      .toBe('Severity: blocker')

    /* And nothing on the page is wider than the page — the measurement, at both widths. */
    const { scroll, client } = await overflows(page)
    expect(
      scroll,
      `the page scrolls sideways at ${name}: ${scroll} wide in ${client}. `
      + `Past the edge: ${(await tooWide(page, width)).join(', ')}`,
    ).toBeLessThanOrEqual(client)
  })

  test(`the folded half opens and fits at ${name}`, async ({ page }) => {
    await page.setViewportSize({ width, height })
    await page.goto(`${serving.url}reviews/${reviewId}/what-happened`)

    await page.locator('.fold').click()
    await expect(page.locator('.silent-row')).toHaveCount(7)

    /* A row opens in place and says that nothing came back, rather than showing a blank. */
    await page.locator('.silent-row').first().click()
    await expect(page.locator('.silent-opened')).toContainText('Nothing has come back on')

    const { scroll, client } = await overflows(page)
    expect(
      scroll,
      `the page scrolls sideways with the fold open at ${name}: ${scroll} in ${client}. `
      + `Past the edge: ${(await tooWide(page, width)).join(', ')}`,
    ).toBeLessThanOrEqual(client)
  })

  test(`a row's own content fits at ${name}`, async ({ page }) => {
    await page.setViewportSize({ width, height })
    await page.goto(`${serving.url}reviews/${reviewId}/what-happened`)

    /*
     * **The row, not the page.** A row carries the longest unbroken tokens on the screen —
     * a 53-character path with a line number after it, and a commit id — so it is the thing
     * worth measuring on its own as well as inside the document.
     */
    expect(
      await tooWide(page, width, '.answered-row'),
      `nothing in a row is wider than ${name}`,
    ).toEqual([])
  })
}

/**
 * **No outcome's stored detail reaches the page**, measured against the rendered document
 * rather than against a component's text.
 *
 * This is DEV-958's first acceptance criterion and the one the issue is named after. It is
 * asserted in the unit tests too; what this adds is that it holds of the real page served
 * by the real binary out of a real database, where the details went in as the JSON the
 * harvester writes.
 *
 * At one width only. Whether a brace is on the page does not change with the viewport.
 */
test('prints no outcome’s stored detail as JSON', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto(`${serving.url}reviews/${reviewId}/what-happened`)
  await expect(page.locator('.answered-row').first()).toBeVisible()
  await page.locator('.fold').click()
  await expect(page.locator('.silent-row')).toHaveCount(7)

  const said = await page.locator('main').innerText()
  for (const token of ['comment_id', 'in_reply_to', 'time_basis', 'lines_changed', '"basis"']) {
    expect(said, `the page printed ${token}`).not.toContain(token)
  }
  /*
   * A brace, checked over the document's text rather than its markup. The reply itself
   * holds none, and a finding's claim holds none, so any brace here came from a detail.
   */
  expect(said).not.toContain('{')
})

/**
 * The two links a row carries, pressed rather than read.
 *
 * **Nothing leaves the machine.** Every `https` request is aborted before the click, for
 * the reason `header-links.spec.ts` states: Review Desk never talks to GitHub and a test
 * of it must not either. The address the browser was *asked* for is the whole of what the
 * row promises.
 */
test('a row reaches its finding and its comment on GitHub', async ({ page, context }) => {
  const asked: string[] = []
  context.on('request', request => asked.push(request.url()))
  await context.route('https://**', route => route.abort())

  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto(`${serving.url}reviews/${reviewId}/what-happened`)

  const row = page.locator('.answered-row').first()
  await expect(row).toBeVisible()

  const comment = row.getByRole('link', { name: /comment, opens in a new tab/ })

  /*
   * **The fragment is checked on the attribute, and the request on the network.** A
   * fragment is never sent to a server, so `request.url()` reports the address without
   * it — measured here, where asserting the `#discussion_r…` form against the request
   * list failed with the same URL minus its hash. The hash is the half that says *which*
   * comment and the request is the half that says the browser really went, so both are
   * asserted, each where it is observable.
   */
  await expect(comment).toHaveAttribute(
    'href',
    'https://github.com/acme/portal/pull/601#discussion_r2314990001',
  )

  const [opened] = await Promise.all([
    page.waitForEvent('popup'),
    comment.click(),
  ])
  await expect
    .poll(() => asked, { message: 'the new tab asked for the comment on GitHub' })
    .toContain('https://github.com/acme/portal/pull/601')
  await opened.close()

  /* And the finding, which is on the code review page and reaches the finding itself. */
  await row.getByRole('link', { name: /on the code review page/ }).click()
  await expect(page).toHaveURL(new RegExp(`/reviews/${reviewId}/code-review#F1$`))
})

/**
 * **A review that is not on the desk reads as not on the desk**, rather than as a blank.
 *
 * A new route is a new way to reach a dead end, and this repository has met one: DEV-878
 * records a step page that answered a refusal with a panel carrying no trail and no way
 * back, and the trail's own rule is that a link to a page that does not exist is worse than
 * a label. `DeskState` owns the answer and every step page is inside it, so what this
 * asserts is that the new page really is — not that the component works, which its own
 * tests cover.
 */
test('a review that is not on the desk says so', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto(`${serving.url}reviews/987654/what-happened`)

  /*
   * **A notice, not a heading**, which is what this looked at rather than assumed: the
   * first version of this test asked for a heading named "Not on the desk" and failed over
   * a page that says exactly that in a `<strong>` inside `DeskState`'s own notice. The
   * screenshot settled which of the two was wrong.
   */
  await expect(page.locator('main')).toContainText('Not on the desk')
  await expect(page.locator('main')).toContainText('No review with id 987654')

  /* And the two ways out: the header's navigation, and the notice's own retry. */
  await expect(page.getByRole('link', { name: 'Queue' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible()
})

/**
 * **The trail's step 7 opens it**, which is the criterion in as many words.
 *
 * From another step's page, because that is how a reader reaches it: every row in the queue
 * opens Understand, and the trail is the only way to five of the seven steps.
 */
test('the trail’s step 7 opens the page', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto(`${serving.url}reviews/${reviewId}/understand`)

  await page.locator('.trail').getByRole('link', { name: '7 What happened' }).click()
  await expect(page).toHaveURL(new RegExp(`/reviews/${reviewId}/what-happened$`))
  await expect(page.locator('h1')).toHaveText(
    'Nothing has read this pull request back from GitHub',
  )
})
