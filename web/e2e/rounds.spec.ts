import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import { cli, scratch, serve, stop, type Serving } from './harness'

/*
 * **The round switcher, in a browser, over three rounds of one pull request. DEV-976.**
 *
 * A review is one round and that was a dead end: round 2 is a new record and from inside it
 * there was no way back to round 1. Patrick, 14 October 2026: "there's no way for me to
 * actually go back to the previous round… especially when I go and look at the understand,
 * the alternatives, the solution map, and even the code review. It's really important to be
 * able to see where we've come from, not just this specific point in time."
 *
 * Every acceptance criterion the issue states is here except the one a browser cannot reach,
 * and that one is named below rather than left out.
 *
 * ## Its own database and its own server
 *
 * The acceptance suite's seed is asserted against by name and by count — "3 reviews need
 * you", the repository chips, the Reviews list's heading — and three rounds added to it
 * would move every one of them: round 3's ready draft is a row in "drafts to approve". So
 * this seeds a database of its own and starts a server of its own on port 0, exactly as
 * `header-links.spec.ts` and `what-happened.spec.ts` do and for the same reason. Each
 * carries the argument in its own header.
 *
 * ## Three rounds, and the brief in round 1 alone
 *
 * `acme/portal#631`. Three rather than two, because three is what makes "the round being
 * read is in the middle" a state at all — the round that is neither the first nor the newest.
 *
 * **The brief is in round 1 because nothing carries a brief between rounds.** `briefs` is
 * keyed by the review id, as is every record behind steps 1 to 4 — the problems, the
 * options, the flows, the parts, the step rows, their facts and the provenance — and
 * `Store::open_review` copies none of it; `GET /reviews/{id}` serves this round's brief with
 * no fallback. So rounds 2 and 3 holding none is the ordinary shape of a pull request whose
 * later rounds were reviewed by a session that wrote no brief of its own, not a contrivance.
 * It is the shape of all four rounds of `#9568` on Patrick's own record.
 *
 * Each round carries a finding, which is load-bearing: `review open` moves an **unstarted**
 * round's head rather than opening a round beside it — DEV-945 — so without something on the
 * round, the next head in the list would move this one instead of becoming round 2. Each
 * carries a draft, because the draft page is a step page and a page that refused its own
 * read would not be drawing the switcher this file exists to measure.
 *
 * ## What this seed cannot do, said rather than worked around
 *
 * **It cannot post a round, so no round here carries a posted date.** A round's `posted_at`
 * is its draft's; `Store::post_draft` refuses anything but an **approved** draft; and only
 * the developer approves one, through the dashboard and never the command line — which is
 * the rule, not a gap. So "posted 9 October 2026" is asserted in
 * `test/round-switcher.nuxt.spec.ts`, whose records hold a posted draft, and what is
 * measured here is the other two wordings. The same division `what-happened.spec.ts` makes
 * about `pr_state`.
 *
 * **And no round can be marked dismissed**, because `RoundSummary` carries no such flag on
 * master: DEV-954's arrives with DEV-962. `roundDismissed` is the one reader waiting for it
 * and `test/rounds.spec.ts` asserts both of its answers against a round that carries the
 * field. What is measured here is that nothing is marked meanwhile, which is the honest
 * state and not a silent "not dismissed".
 */

/** The two widths the issue names: the boards' own width, and a phone. */
const WIDTHS = [
  { name: '1440', width: 1440, height: 900 },
  { name: '390', width: 390, height: 844 },
]

/** The six step pages, by the suffix each sits at and the step the switcher names. */
const PAGES = [
  { step: 'Understand', suffix: 'understand' },
  { step: 'Alternatives', suffix: 'alternatives' },
  { step: 'Solution map', suffix: 'solution' },
  { step: 'Code review', suffix: 'code-review' },
  { step: 'Draft', suffix: 'draft' },
  { step: 'What happened', suffix: 'what-happened' },
]

/** The three the brief feeds, which are the three a later round can hold none of. */
const FROM_THE_BRIEF = PAGES.slice(0, 3)
/** The three whose record is the round's own, whatever any earlier round holds. */
const ITS_OWN = PAGES.slice(3)

/** What is typed into round 2's draft, for the leave guard. */
const TYPED = 'A sentence typed into round 2 and never saved.'

let serving: Serving
/** The three rounds of `acme/portal#631`, in the order they were opened. */
let rounds: number[] = []
/** A review of a pull request with one round, which must draw no switcher at all. */
let alone: number

function openReview(database: string, pr: string, head: string, title: string): number {
  const opened = cli(database, [
    'review', 'open',
    '--repo', 'acme/portal',
    '--pr', pr,
    '--head', head,
    '--title', title,
    '--ticket', `ABC-${pr}`,
    '--ticket-url', `https://linear.app/acme/issue/ABC-${pr}`,
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) throw new Error(`review open printed no id: ${opened}`)
  return reviewId
}

/** A finding, which is what makes a round started so the next head opens a round beside it. */
function addFinding(database: string, directory: string, reviewId: number, at: number) {
  const path = join(directory, `rounds-finding-${reviewId}.json`)
  writeFileSync(
    path,
    JSON.stringify({
      lens: 'correctness',
      severity: at === 0 ? 'blocker' : 'should',
      claim: at === 0
        ? 'The export reads the tenant from the request that queued it'
        : 'The tenant is passed to the job but not to the retry',
      file: 'app/Exports/ScheduledExport.php',
      line: 52 + at * 9,
      disposition: 'in_draft',
    }),
  )
  cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
}

function addDraft(database: string, directory: string, reviewId: number, text: string) {
  const path = join(directory, `rounds-body-${reviewId}.json`)
  writeFileSync(path, JSON.stringify([{ kind: 'prose', text }]))
  cli(database, ['draft', 'put', '--review', String(reviewId), '--sections', path])
}

test.beforeAll(async () => {
  const directory = scratch()
  const database = join(directory, 'rounds.db')
  const heads = ['c1d2e31', 'd2e3f42', 'e3f4a53']
  const title = 'Hold a scheduled export’s tenant for the whole run'

  rounds = []
  for (const [at, head] of heads.entries()) {
    const reviewId = openReview(database, '631', head, title)

    /* The brief on round 1 alone. See this file's header for why that is the real shape. */
    if (at === 0) {
      const brief = join(directory, 'rounds-brief.json')
      writeFileSync(
        brief,
        JSON.stringify({
          problem: 'A scheduled export reads the tenant from the request that queued it, so '
            + 'a run that outlives the request exports another tenant’s rows.',
          options: [
            {
              key: 'A',
              title: 'Pass the tenant to the job',
              argument_for: 'The smallest change, and nothing new to hold.',
              argument_against: 'Every caller has to remember to pass it.',
              chosen: true,
            },
            {
              key: 'B',
              title: 'Read the tenant inside the job',
              argument_for: 'No caller can forget.',
              argument_against: 'The job then needs a request, which a schedule has not got.',
              chosen: false,
            },
          ],
          approach_verdict: 'A is the shape this ticket asks for.',
        }),
      )
      cli(database, ['brief', 'put', '--review', String(reviewId), '--file', brief])
    }

    addFinding(database, directory, reviewId, at)
    addDraft(
      database,
      directory,
      reviewId,
      at === 0
        ? 'One blocker on the tenant the scheduled export reads.'
        : `Round ${at + 1}. The tenant is passed now; one finding is left on the retry path.`,
    )
    rounds.push(reviewId)
  }

  /* One round of a different pull request, so "no switcher" is measured and not assumed. */
  alone = openReview(database, '632', 'f4a5b64', 'Name the export’s temporary file once')
  addFinding(database, directory, alone, 1)
  addDraft(database, directory, alone, 'One finding on the temporary name.')

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
async function tooWide(page: Page, edge: number): Promise<string[]> {
  return page.evaluate(({ edge }) => {
    return [document.documentElement, ...document.querySelectorAll('*')]
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
  }, { edge })
}

/**
 * The switcher's chips: where each goes and what it is.
 *
 * **The text is the DOM's, condensed, and the three lines run together in it.** Vue's
 * `whitespace: 'condense'` deletes a whitespace-only text node between two elements, so
 * there is no literal space between a round's name, its standing and its commit — the
 * separation is the layout's, and for a screen reader it is the accessible name's, which the
 * accname algorithm separates because a flex item is blockified. So what each round *says*
 * is asserted through `toHaveAccessibleName` below rather than guessed from this string.
 * `what-happened.spec.ts` carries the same lesson about an interpolated space.
 */
async function chips(page: Page) {
  return page.locator('nav.rounds .round').evaluateAll(found =>
    found.map(one => ({
      text: one.textContent?.replace(/\s+/g, ' ').trim() ?? '',
      href: one.getAttribute('href'),
      current: one.getAttribute('aria-current'),
      tag: one.tagName,
    })))
}

/* ========================= Every step page, at both widths ========================= */

for (const { name, width, height } of WIDTHS) {
  /**
   * **The first acceptance criterion, over all six pages.** "Every step page of a
   * multi-round review shows the round switcher", which is a claim about the six pages
   * rather than about the component — so each is visited rather than one standing for all.
   *
   * Round 2 is the one looked at, because it is the round that is neither the first nor the
   * newest: two links out of it, in both directions.
   */
  test(`every step page of round 2 shows the switcher at ${name}`, async ({ page }) => {
    await page.setViewportSize({ width, height })

    for (const { step, suffix } of PAGES) {
      await page.goto(`${serving.url}reviews/${rounds[1]}/${suffix}`)

      /* The step page it says it is, so a wrong page cannot pass the rest of this. */
      await expect(page.locator('.face--here .name'), step).toContainText(step)

      const switcher = page.locator('nav.rounds')
      await expect(switcher, `${step} draws the switcher`).toBeVisible()
      await expect(switcher, `${step} says which round of how many`)
        .toContainText('Round 2 of 3')

      /*
       * **Each link opens the same step of the linked round**, which is the second
       * criterion and the whole of "no page loses its place".
       */
      expect(await chips(page), step).toEqual([
        {
          text: `Round 1, ${step}not postedc1d2e31`,
          href: `/reviews/${rounds[0]}/${suffix}`,
          current: null,
          tag: 'A',
        },
        {
          text: 'Round 2, the round you are readingnot postedd2e3f42',
          href: null,
          current: 'page',
          tag: 'SPAN',
        },
        {
          text: `Round 3, ${step}being worked one3f4a53`,
          href: `/reviews/${rounds[2]}/${suffix}`,
          current: null,
          tag: 'A',
        },
      ])

      /*
       * **And what each link is called, which is what a screen reader hears.** The three
       * lines run together in `textContent` above; the accessible name separates them,
       * because each is a flex item and a flex item is blockified, so the accname algorithm
       * puts a space at the boundary.
       *
       * **The space before the comma is that same rule and is not a typo.** The step's name
       * is in an `.rd-visually-hidden` span, which is `position: absolute` and therefore
       * blockified too, so accname puts a space on each side of it: `Round 1` + ` ` +
       * `, Understand`. It is inaudible — a screen reader says "Round 1, Understand" either
       * way — and it is recorded here because it was measured through the real build rather
       * than reasoned about, and because an `aria-label` written to tidy it up would make
       * the accessible name something other than the visible words.
       */
      await expect(
        page.getByRole('link', { name: `Round 1 , ${step}` }).first(),
        step,
      ).toHaveAccessibleName(`Round 1 , ${step} not posted c1d2e31`)

      /*
       * **And nothing on the page is wider than the page** — the fifth criterion, at 390
       * and at the boards' own width, on every one of the six.
       */
      const { scroll, client } = await overflows(page)
      expect(
        scroll,
        `${step} scrolls sideways at ${name}: ${scroll} wide in ${client}. `
        + `Past the edge: ${(await tooWide(page, width)).join(', ')}`,
      ).toBeLessThanOrEqual(client)
    }
  })

  /** A review whose pull request has one round draws none, on any of the six. */
  test(`a single-round review shows no switcher at ${name}`, async ({ page }) => {
    await page.setViewportSize({ width, height })

    for (const { step, suffix } of PAGES) {
      await page.goto(`${serving.url}reviews/${alone}/${suffix}`)
      await expect(page.locator('.face--here .name'), step).toContainText(step)
      await expect(page.locator('nav.rounds'), `${step} draws none`).toHaveCount(0)
      await expect(page.locator('main'), `${step} says no "of 1"`).not.toContainText('Round 1 of 1')
    }
  })
}

/* ============================= Where it stands, and where it was written ========== */

test('no round is marked dismissed while the wire carries no such flag', async ({ page }) => {
  await page.goto(`${serving.url}reviews/${rounds[1]}/understand`)
  await expect(page.locator('nav.rounds .round')).toHaveCount(3)
  await expect(page.locator('nav.rounds .tag')).toHaveCount(0)
})

/**
 * **The third acceptance criterion**: a step this round did not write says so and links to
 * the round that wrote it. Nothing carries a brief between rounds, so round 2's and round
 * 3's Understand, Alternatives and Solution hold nothing of their own.
 */
for (const at of [1, 2]) {
  test(`round ${at + 1} says where its understanding was written`, async ({ page }) => {
    for (const { step, suffix } of FROM_THE_BRIEF) {
      await page.goto(`${serving.url}reviews/${rounds[at]}/${suffix}`)

      /*
       * **The heading is the answer rather than an absence**, and it is asserted as the `h1`
       * because the point is which sentence is read first. It used to read "Nothing has been
       * written about this problem yet" over a pull request whose brief is one round back.
       * It claims a brief rather than a step, which is all `brief_written` measures.
       */
      await expect(page.locator('h1'), step)
        .toHaveText('The last brief before this round was written in round 1.')
      await expect(page.locator('main'), step).toContainText(
        `Round ${at + 1} wrote none of its own, and nothing carries a brief from one round `
        + 'to the next.',
      )

      /* The notice under it is a destination, and nothing the heading already said. */
      const back = page.getByRole('link', { name: `Open round 1’s ${step}` })
      await expect(back, step).toBeVisible()
      await expect(back, step).toHaveAttribute('href', `/reviews/${rounds[0]}/${suffix}`)
      await expect(page.locator('main'), step).not.toContainText('Nothing has been written')
    }
  })
}

/**
 * **And steps 5 to 7 never say it.** A round's findings, its draft and its harvest are its
 * own, written against its own head commit; "written in round 1" would be false about the
 * one thing rounds exist to separate. Asserted on the round that would otherwise say it.
 */
test('no later step claims to have been written in another round', async ({ page }) => {
  for (const { step, suffix } of ITS_OWN) {
    await page.goto(`${serving.url}reviews/${rounds[2]}/${suffix}`)
    await expect(page.locator('nav.rounds'), `${step} still draws the switcher`).toBeVisible()
    await expect(page.locator('main'), step).not.toContainText('The last brief before this round')
    await expect(page.locator('main'), step).not.toContainText('Open round 1’s')
  }
})

/** The round that wrote the brief has one of its own, so there is nothing to point at. */
test('the round that wrote the brief says nothing about another round', async ({ page }) => {
  for (const { step, suffix } of FROM_THE_BRIEF) {
    await page.goto(`${serving.url}reviews/${rounds[0]}/${suffix}`)
    await expect(page.locator('nav.rounds'), step).toBeVisible()
    await expect(page.locator('main'), step).not.toContainText('The last brief before this round')
  }
})

/* ================================ Keeping your place ============================== */

/**
 * **Moving between rounds keeps the step**, followed rather than asserted of an attribute:
 * what matters is where the browser lands and what the trail then says about it.
 */
test('following a round link lands on the same step of that round', async ({ page }) => {
  for (const { step, suffix } of PAGES) {
    await page.goto(`${serving.url}reviews/${rounds[1]}/${suffix}`)
    await page.getByRole('link', { name: `Round 3 , ${step}` }).click()
    await expect(page).toHaveURL(`${serving.url}reviews/${rounds[2]}/${suffix}`)
    await expect(page.locator('.face--here .name'), step).toContainText(step)
    await expect(page.locator('nav.rounds'), step).toContainText('Round 3 of 3')
  }
})

/**
 * **The fifth criterion's other half: it is keyboard reachable.** Every round is an anchor
 * or a span — no button and no handler — so this is the browser's own tab order and the
 * global focus ring, and it is followed with Enter rather than clicked.
 */
test('a round is reached and followed from the keyboard alone', async ({ page }) => {
  await page.goto(`${serving.url}reviews/${rounds[1]}/alternatives`)

  const link = page.getByRole('link', { name: 'Round 1 , Alternatives' })
  await link.focus()
  await expect(link).toBeFocused()
  await link.press('Enter')

  await expect(page).toHaveURL(`${serving.url}reviews/${rounds[0]}/alternatives`)
  await expect(page.locator('.face--here .name')).toContainText('Alternatives')
})

/**
 * **The fourth criterion: DEV-960's leave guard still applies.** A switcher link off the
 * draft page is another review's draft — the *same* page record with a new parameter, so it
 * is an `onBeforeRouteUpdate` and not a leave — and the guard has to hold on it.
 *
 * It was found to by reading `draft.vue`, and that is exactly why it is measured: "both
 * guards, because the two ways out are different route changes" is a sentence that is true
 * until a new way out is added.
 */
test('a round link off the draft page asks before it discards unsaved edits', async ({
  page,
}) => {
  /* **No browser dialog, ever** — the issue's own words, and why the guard is a card. */
  const dialogs: string[] = []
  page.on('dialog', async (dialog) => {
    dialogs.push(dialog.message())
    await dialog.dismiss()
  })

  await page.goto(`${serving.url}reviews/${rounds[1]}/draft`)
  await page.getByRole('tab', { name: 'Edit' }).click()

  const editor = page.locator('textarea#draft-body')
  const body = await editor.inputValue()
  await editor.fill(`${body}\n\n${TYPED}`)

  await page.getByRole('link', { name: 'Round 1 , Draft' }).click()

  /*
   * **The page asks, and it has not moved**: still round 2's draft, with the typed sentence
   * in it. Before DEV-960 the typing went without a word; this is that test's teeth, through
   * the one way out that did not exist when it was written.
   */
  await expect(page.getByText('You have unsaved edits to this review’s draft')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Keep editing' })).toBeFocused()
  await expect(editor).toHaveValue(`${body}\n\n${TYPED}`)
  await expect(page.locator('nav.rounds')).toContainText('Round 2 of 3')

  /* Keep editing: nothing moves and not a character is lost. */
  await page.getByRole('button', { name: 'Keep editing' }).click()
  await expect(page.getByRole('button', { name: 'Keep editing' })).toHaveCount(0)
  await expect(page).toHaveURL(`${serving.url}reviews/${rounds[1]}/draft`)
  await expect(editor).toHaveValue(`${body}\n\n${TYPED}`)

  /* Discard: it moves, and it moves to round 1's **draft** rather than anywhere else. */
  await page.getByRole('link', { name: 'Round 1 , Draft' }).click()
  await page.getByRole('button', { name: 'Discard the edits' }).click()
  await expect(page).toHaveURL(`${serving.url}reviews/${rounds[0]}/draft`)
  await expect(page.locator('.face--here .name')).toContainText('Draft')
  await expect(page.locator('nav.rounds')).toContainText('Round 1 of 3')

  expect(dialogs, 'the browser was never asked to put up a dialog').toEqual([])
})
