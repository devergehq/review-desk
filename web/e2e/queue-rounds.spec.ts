import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, test, type Locator, type Page } from '@playwright/test'

import { cli, scratch, serve, stop, type Serving } from './harness'

/*
 * **The queue's single row for a pull request whose later round has posted. DEV-983, in a
 * browser.**
 *
 * Patrick, 14 October 2026, on the real queue: "it's showing a review… but also says its
 * Round 1 and when I click it, it goes to the round 1 review, but round 2 is there too that
 * has already been submitted." The row read "round 1 · The code under F1 changed · the file
 * changed in <B>, but not line 1122 · See what changed" and linked round 1, though round 2
 * had been posted the day before — and the change it reported was round 2's own head.
 *
 * Two faults met on that row. An earlier round's code change that a posted round had
 * already read was still counted as waiting, and the two arrays that know a pull request's
 * newest round could join onto a row without renumbering it. The store and the page each
 * have their own tests; what a browser adds is the row as a reader meets it — the number,
 * the words, and where the click goes.
 *
 * ## Its own database and its own server
 *
 * The acceptance suite's seed is asserted against by name and by count — "3 reviews need
 * you", the repository chips, the Reviews list's heading — and three two-round pull
 * requests added to it would move every one of them. So this seeds a database of its own
 * and starts a server of its own on port 0, exactly as `header-links.spec.ts`,
 * `what-happened.spec.ts` and `rounds.spec.ts` do and for the same reason. Each carries the
 * argument in its own header.
 *
 * ## Three pull requests, one per fixture
 *
 * One database rather than three, because the fixtures do not interact: each is a pull
 * request of its own, and a shared server is one process rather than three. The numbers are
 * invented and the repository is `acme/portal` — DEV-975: nothing here names a real
 * repository, ticket key or commit.
 *
 * | Pull request | What it is |
 * |---|---|
 * | `#751` | round 1 posted at A, the code changed A to B, round 2 posted at B |
 * | `#752` | the same, and a further change B to C |
 * | `#753` | the same as `#751`, and the author replies to a round-1 thread afterwards |
 * | `#754` | round 1 posted and replied to, and round 2 opened with nothing written to it |
 *
 * ## What this seed cannot do, said rather than worked around
 *
 * **It cannot give a round a posted date, so none here carries one.** `review link`
 * records the GitHub review, which is what `review_is_posted` asks and so what opens the
 * next round; the *draft's* posting needs an approved draft, and only the developer
 * approves one — through the dashboard and never the command line. So `#751`'s row reads
 * "Round 2 of 2 posted" with no "as" clause and no age, which is the same division
 * `rounds.spec.ts` makes and for the same reason.
 *
 * **And it cannot record a pull-request state**, which `what-happened.spec.ts` carries the
 * argument for: `pr_state` and `harvested_at` are written by the harvester and by nothing
 * else. An unharvested pull request reads as open, which is the rule the queue has always
 * used, so the rows are drawn; what is absent is the figure for what came back, and the
 * posted row says so rather than showing a nought.
 */

/** The commits the three fixtures share: round 1's head, round 2's head, and a later one. */
const FIRST = 'c1d2e31'
const SECOND = 'd2e3f42'
const THIRD = 'e3f4a53'

/** The file a finding points at, which is what a code change is recorded against. */
const FILE = 'app/Exports/ScheduledExport.php'

/** The pull request's title, and the one `#754`'s author renames it to after round 1. */
const TITLE = 'Hold a scheduled export’s tenant for the whole run'
const RENAMED = 'Hold the tenant for the whole of a scheduled export'

let serving: Serving
/** Each fixture's two review ids, by its pull request number. */
const rounds = new Map<string, { one: number, two: number, oneSeq: string, twoSeq: string }>()
let directory: string
let database: string

/** Open a round of this pull request and hand back its review id. */
function openRound(pr: string, head: string, title = TITLE): number {
  const opened = cli(database, [
    'review', 'open',
    '--repo', 'acme/portal',
    '--pr', pr,
    '--head', head,
    '--title', title,
    '--ticket', `ABC-${pr}`,
    '--ticket-url', `https://linear.app/acme/issue/ABC-${pr}`,
  ])
  const id = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(id)) throw new Error(`review open printed no id: ${opened}`)
  return id
}

/**
 * A finding on this round, and the sequence number it was given.
 *
 * **Not 1 on a later round.** A finding's `seq` runs across the pull request rather than
 * within a round, so that a finding carried into round 2 keeps the key the posted review
 * already named it by — round 2's own first finding is `F2`. An outcome recorded against a
 * seq the round has not got is refused, which is how this was found.
 */
function addFinding(reviewId: number, pr: string): string {
  const path = join(directory, `queue-rounds-finding-${reviewId}.json`)
  writeFileSync(path, JSON.stringify({
    lens: 'correctness',
    severity: 'should',
    claim: 'The export reads the tenant from the request that queued it',
    file: FILE,
    line: 1122,
    disposition: 'in_draft',
  }))
  const printed = cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  const seq = printed.trim().match(/\d+/)?.[0]
  if (seq == null) throw new Error(`finding add printed no seq for ${pr}: ${printed}`)
  return seq
}

/**
 * Record that this round posted, which is the one thing the command line says it with.
 *
 * `review link --github-review` is what `review_is_posted` reads, and it is also what makes
 * the next commit a round of its own: since DEV-962 a new head on an *unposted* round moves
 * that round in place. Load-bearing twice over — deleting it as redundant would collapse
 * each fixture's two heads into one round and leave nothing here to measure.
 */
function post(reviewId: number, pr: string, which: number) {
  cli(database, [
    'review', 'link',
    '--review', String(reviewId),
    '--github-review', `PRR_acme${pr}r${which}`,
  ])
}

/** Record a code change, with the detail `harvest_code_changes` writes. */
function codeChanged(reviewId: number, seq: string, commit: string, from: string, to: string) {
  cli(database, [
    'outcome', 'add',
    '--review', String(reviewId),
    '--finding', seq,
    '--kind', 'code_changed_after',
    '--by', 'harvester',
    '--detail', JSON.stringify({
      file: FILE,
      line: 1122,
      lines_changed: false,
      file_status: 'modified',
      commit,
      basis: `compare ${from}...${to}`,
    }),
  ])
}

/** Two rounds of this pull request, both posted, each with one finding. */
function twoPostedRounds(pr: string) {
  const one = openRound(pr, FIRST)
  const oneSeq = addFinding(one, pr)
  post(one, pr, 1)

  const two = openRound(pr, SECOND)
  const twoSeq = addFinding(two, pr)
  post(two, pr, 2)

  rounds.set(pr, { one, two, oneSeq, twoSeq })
  return { one, two, oneSeq, twoSeq }
}

test.beforeAll(async () => {
  directory = scratch()
  database = join(directory, 'queue-rounds.db')

  /* #751: the change Patrick's record held — round 1's comparison ends at round 2's head. */
  const covered = twoPostedRounds('751')
  codeChanged(covered.one, covered.oneSeq, SECOND, FIRST, SECOND)

  /*
   * #752: the same, and then the author pushes past round 2's head. The harvester reads
   * both rounds, so each records the change against its own comparison — and neither
   * comparison ends anywhere a round posted.
   */
  const further = twoPostedRounds('752')
  codeChanged(further.one, further.oneSeq, SECOND, FIRST, SECOND)
  codeChanged(further.one, further.oneSeq, THIRD, FIRST, THIRD)
  codeChanged(further.two, further.twoSeq, THIRD, SECOND, THIRD)

  /* #753: the author answers a round-1 thread after round 2 posted. */
  const replied = twoPostedRounds('753')
  codeChanged(replied.one, replied.oneSeq, SECOND, FIRST, SECOND)
  cli(database, [
    'outcome', 'add',
    '--review', String(replied.one),
    '--finding', replied.oneSeq,
    '--kind', 'replied',
    '--by', 'author',
    '--detail', 'The retry passes the tenant now.',
  ])

  /*
   * #754: round 2 opened and nothing written to it, which is what a dispatcher does the
   * moment GitHub shows a new head after a post. Round 2 is then in no inbox array at all
   * — not the newest posted round, no draft at a status, no unconfirmed brief, no running
   * step — so the row is in the answer only through round 1's reply, and the item's
   * `rounds_on_pr` is the only thing that can say which round the row is.
   */
  const unwritten = openRound('754', FIRST)
  const unwrittenSeq = addFinding(unwritten, '754')
  post(unwritten, '754', 1)
  cli(database, [
    'outcome', 'add',
    '--review', String(unwritten),
    '--finding', unwrittenSeq,
    '--kind', 'replied',
    '--by', 'author',
    '--detail', 'The retry passes the tenant now.',
  ])
  /*
   * Renamed between rounds, which is a thing authors do — and the reason the row's title is
   * chosen by round like its ticket. Round 2 is the only round that carries the new name,
   * and it is in no inbox array of its own.
   */
  rounds.set('754', {
    one: unwritten,
    two: openRound('754', SECOND, RENAMED),
    oneSeq: unwrittenSeq,
    twoSeq: '-',
  })

  serving = await serve(database)
})

test.afterAll(async () => {
  if (serving) await stop(serving)
})

/** The queue, loaded and settled. */
async function queue(page: Page) {
  await page.goto(serving.url)
  await expect(page.locator('.queue-line').first()).toBeVisible()
  await page.waitForLoadState('networkidle')
}

/** The rows under one heading, whatever that group draws them from. */
function rowsUnder(page: Page, heading: string): Locator {
  return page.locator('section').filter({ has: page.getByRole('heading', { name: heading }) })
    .locator('.queue-line')
}

/** The one row for a pull request, anywhere on the page. */
function rowFor(page: Page, pr: string): Locator {
  return page.locator('.queue-line').filter({ hasText: `#${pr}` })
}

test('draws no "code changed" row for a change the posted round read', async ({ page }) => {
  await queue(page)
  const fixture = rounds.get('751')!

  /*
   * **One row for the pull request, and it is the posted group's.** The change on round 1
   * is at the commit round 2 was posted on, so nothing about this pull request is waiting
   * on the developer — and before this change the queue drew it under "Needs you" saying
   * the code had changed.
   */
  await expect(rowFor(page, '751')).toHaveCount(1)
  await expect(rowsUnder(page, 'Needs you').filter({ hasText: '#751' })).toHaveCount(0)

  const row = rowsUnder(page, 'Waiting on the author').filter({ hasText: '#751' })
  await expect(row).toHaveCount(1)
  await expect(row).toContainText('Round 2 of 2 posted')
  await expect(row, 'no code change is reported on it').not.toContainText('changed')
  await expect(row.locator('.row')).toHaveAttribute(
    'href',
    `/reviews/${fixture.two}/understand`,
  )
})

test('draws a change after that head as the newest round’s', async ({ page }) => {
  await queue(page)
  const fixture = rounds.get('752')!

  const row = rowsUnder(page, 'Needs you').filter({ hasText: '#752' })
  await expect(row, 'one row for one pull request').toHaveCount(1)
  await expect(row).toContainText('round 2')
  await expect(row, 'the code change is the row’s next action').toContainText('changed')
  /*
   * **And the row's number and link are round 2's.** The change is on round 2 as well as on
   * round 1 — both rounds were harvested — and the row is the newest round, so the click
   * lands on the review that was written against the commit the change is measured from.
   */
  await expect(row.locator('.row')).toHaveAttribute(
    'href',
    `/reviews/${fixture.two}/understand`,
  )
  await expect(row, 'no earlier round is named, because the change is this round’s')
    .not.toContainText('round 1')
})

test('draws an earlier round’s reply on the newest round’s row, saying which',
  async ({ page }) => {
    await queue(page)
    const fixture = rounds.get('753')!

    const row = rowsUnder(page, 'Needs you').filter({ hasText: '#753' })
    await expect(row, 'one row for one pull request').toHaveCount(1)
    /*
     * **The row is round 2: its number and its link.** The reply is on round 1 and is still
     * drawn, with the round it was left on — DEV-957's wording, now reachable on a row whose
     * next action *is* the reply. Before this change the row read "round 1" and the click
     * went to round 1.
     */
    await expect(row).toContainText('round 2')
    await expect(row.locator('.row')).toHaveAttribute(
      'href',
      `/reviews/${fixture.two}/understand`,
    )
    await expect(row).toContainText('The author replied to')
    await expect(row).toContainText('replied to round 1')
    /*
     * **And no measurement about the round the reply is not on.** The guard this change
     * removed was what kept these apart — "round 1's reply beside round 2's counts", in its
     * own words — and renumbering the row makes that guard pass. Found by a review of the
     * fourth fix batch.
     */
    await expect(row).not.toContainText('Not harvested yet')
    /* And the ticket, read from the item about the round the row is. */
    await expect(row).toContainText('ABC-753')
  })

test('clicking the row opens the newest round, which names itself', async ({ page }) => {
  await queue(page)
  const fixture = rounds.get('753')!

  await rowFor(page, '753').locator('.row').click()
  await expect(page).toHaveURL(`${serving.url}reviews/${fixture.two}/understand`)
  /*
   * **And the round switcher is how round 1 is reached**, which is the ticket's own answer
   * to "nothing is deleted": the earlier round is a click away rather than the row's
   * destination. DEV-976 built it; this is the first thing that needs it.
   */
  await expect(page.getByRole('link', { name: /Round 1/ })).toBeVisible()
})

test('the earlier round’s What happened page still shows the change', async ({ page }) => {
  const fixture = rounds.get('751')!
  await page.goto(`${serving.url}reviews/${fixture.one}/what-happened`)
  await page.waitForLoadState('networkidle')

  /*
   * **Nothing is deleted.** The queue decides what to draw; the outcome row is untouched,
   * so round 1's own page still reports the change the harvester recorded — which is the
   * ticket's requirement and the reason the rule is a filter rather than a delete.
   */
  await expect(page.getByText('changed').first()).toBeVisible()
})

test('numbers the row for a newest round nothing else in the answer mentions',
  async ({ page }) => {
    await queue(page)
    const fixture = rounds.get('754')!

    const row = rowsUnder(page, 'Needs you').filter({ hasText: '#754' })
    await expect(row, 'one row for one pull request').toHaveCount(1)
    /*
     * **Round 2 is in no array of its own**, so this row exists only because round 1 was
     * replied to — and the item's `rounds_on_pr` is the only thing that can say the pull
     * request has reached round 2. Before that field the row said "round 1" and the click
     * went to round 1, which is the fault in the one shape the first fix does not reach.
     */
    await expect(row).toContainText('round 2')
    await expect(row).toContainText('replied to round 1')
    await expect(row.locator('.row')).toHaveAttribute(
      'href',
      `/reviews/${fixture.two}/understand`,
    )
    /*
     * **And the ticket is still there.** No item above the reply is about round 2, and a
     * row takes a ticket only from something about the round it is numbered for — DEV-957 —
     * so the first fix left this row with a ticket on the record and nothing on the page to
     * reach it with. The reply item carries the newest round's ticket now. Found by a
     * review of the first fix batch.
     */
    await expect(row).toContainText('ABC-754')
    /*
     * **And the title round 2 carries, not the one round 1 was opened with.** `#754` is
     * renamed between its rounds, and the title is chosen by round for the same reason the
     * ticket is. Found by a review of the third fix batch.
     */
    await expect(row).toContainText(RENAMED)
    await expect(row).not.toContainText(TITLE)
  })

test('the heading and the repository chip count the rows drawn', async ({ page }) => {
  await queue(page)

  /*
   * **The heading counts the rows, and the chip counts what pressing it shows.** Three
   * two-round pull requests, two of which need the developer — and a row that had failed to
   * merge would make both numbers one too high. Taken from the rows rather than written as
   * literals, which is the shape `test/screens.nuxt.spec.ts` uses and for the reason it
   * gives: a literal here has to move every time a fixture is added.
   */
  const needing = await rowsUnder(page, 'Needs you').count()
  expect(needing, 'three of the four pull requests need the developer').toBe(3)
  await expect(page.getByRole('heading', { level: 1 }))
    .toHaveText(`${needing} pull requests need you`)

  /*
   * **The chip's own count, read off the button rather than off the page's text.** The
   * number is a `span.count` inside the chip and is rendered as "· 3", so a text matcher
   * over the whole page is matching a string the markup never holds. `acme/portal` is the
   * only repository in this seed, so every row the page draws is one of its rows.
   */
  const drawn = await page.locator('.queue-line').count()
  const chip = page.getByRole('radio', { name: /acme\/portal/ })
  await expect(chip).toBeVisible()
  await expect(chip.locator('.count')).toHaveText(`· ${drawn}`)
})
