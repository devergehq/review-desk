import { rmSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { expect, test, type Page } from '@playwright/test'

import {
  cli,
  cliJson,
  draftStamps,
  pinClockAt,
  readAddresses,
  scratch,
  seedReadyDraft,
  seedStructuredReview,
  serversUnderTest,
  stillServing,
  STRUCTURED_LONG_NOTE,
  STRUCTURED_STEP_SEVEN_DETAIL,
} from './harness'

/*
 * **Review Desk, end to end.** The command line records a review, the binary serves it,
 * and a real browser drives the two acts only the developer may perform.
 *
 * This is the first thing in the project to exercise that boundary for real. Confirming a
 * brief and approving a draft exist in the API and nowhere else — decided on 9 October
 * 2026 — so until now nothing had ever performed either one the way a developer does, and
 * nothing had checked that the record then says it was the developer who did it.
 *
 * The order is the acceptance criterion's order, and it is a sequence rather than four
 * independent tests: the draft cannot be approved until the brief is confirmed, because
 * confirming it is what puts the draft in the Inbox.
 */

/**
 * The 44px this app gives anything you press, as `--rd-control-min` sets it.
 *
 * The trail's one line is a control as well as a label, so its box is taller than its text
 * — and a height assertion about the text has to allow for that rather than fail on it.
 */
const CONTROL_MIN = 44

const addresses = readAddresses()

const {
  seeded,
  empty,
  seededDatabase,
  reviewId,
  waivedReviewId,
  structuredReviewId,
  heldBackReviewId,
  postedReviewId,
  requestedReviewId,
  mergedReviewIds,
  supersededReviewIds,
} = addresses

/**
 * The step trail, one string per step, in number order.
 *
 * **Read off `StepTrail`, which every page of a review now carries.** It used to be the
 * One review screen's `LayerTrail`, and that screen is a redirect since DEV-878 — so this
 * reads the shell's own trail instead, which is on all five pages rather than on one.
 *
 * Two `ol`s, because the trail draws the seven steps in the two groups a review actually
 * has: four settled once for the pull request and three repeating every round. They are
 * concatenated here, in order, so a caller still indexes a step by its number minus one.
 *
 * Waited for rather than read straight away. The page's first read is a request, so a
 * `goto` followed immediately by `allTextContents` returns an empty list — which fails as
 * `undefined` on an index and points at the trail rather than at the timing.
 */
async function trail(page: import('@playwright/test').Page): Promise<string[]> {
  const steps = page.locator('nav[aria-label="The seven steps of this review"] .steps li')
  await expect(steps).toHaveCount(7)
  return steps.allTextContents()
}

/** What `work list` says is waiting for an agent, read through the binary. */
function workList(): Array<{
  kind: string
  review_id: number
  sign_off: string | null
  note: string | null
}> {
  const document = cliJson(seededDatabase, ['work', 'list']) as unknown as {
    work: Array<{ kind: string, review_id: number, sign_off: string | null, note: string | null }>
  }
  return document.work
}

/**
 * How many findings the structured review's draft posts, read off the record.
 *
 * A finding posts when it is in the draft and its round status is still open, which is the
 * store's `Finding::is_posted` — and it is the number of rows the index carries.
 */
function postedFindings(): number {
  const document = cliJson(seededDatabase, [
    'review',
    'show',
    '--review',
    String(structuredReviewId),
  ]) as unknown as {
    findings: Array<{ disposition: string, round_status: string | null }>
  }

  return document.findings.filter(
    finding => finding.disposition === 'in_draft'
      && ['new', 'still_open'].includes(finding.round_status ?? 'new'),
  ).length
}

/** The review as the command line reads it, which is how a session reads it. */
function reviewRecord() {
  return cliJson(seededDatabase, ['review', 'show', '--review', String(reviewId)]) as unknown as {
    review: { title: string | null, changed_lines: number | null }
    brief: { confirmed_at: number | null, confirmed_by: string | null }
    draft: { status: string, event: string | null, approved_by: string | null }
  }
}

/*
 * **Both servers are checked before each test, so a death is reported where it happens.**
 *
 * One CI run failed with three asset requests dying at the transport level, no API request
 * made at all, and a blank page — a server that had stopped answering mid-page-load. The
 * assertion that timed out ten seconds later pointed at the Inbox's empty state, which was
 * not what was wrong. This turns that into an immediate failure carrying the server's own
 * output. It has not reproduced in four runs since; what it buys is that the next one will
 * say something.
 */
test.beforeEach(() => stillServing(serversUnderTest()))

test.describe.configure({ mode: 'serial' })

test.describe('the whole path, on a seeded database', () => {
  test('the queue lists the review once, with its title and nothing from a fixture', async ({ page }) => {
    await page.goto(seeded)

    /*
     * **Three, and they are three pull requests rather than one thing three times.** The
     * structured review gained a draft in DEV-878 — the code review page's decisions change
     * a finding's *block*, so a review with no draft has nothing to decide against — and its
     * own layer trail already claimed one by saying step 6 was waiting on the developer.
     *
     * The third is DEV-928's: a review whose draft holds eight findings back, which is the
     * state that issue was raised from and the only one in this seed where the aside's panel
     * can be measured at that volume. A review with a ready draft is waiting on the
     * developer, so it is in this count.
     *
     * **Four since DEV-957, and the noun changed with the count.** The fourth is
     * `globex/portal#9541`, which has *two* rounds with unposted drafts and draws **one** row:
     * a row is one pull request, and the heading counts rows. The four rounds of merged
     * `globex/portal#9568` add nothing at all — a finished review is in no group, which is that
     * issue and is asserted below.
     */
    await expect(
      page.getByRole('heading', { name: '4 pull requests need you' }),
    ).toBeVisible()

    /* One row for #9541, and it is the newer round's. */
    const superseded = page.locator('.queue-line').filter({ hasText: '#9541' })
    await expect(superseded, 'one row for two rounds').toHaveCount(1)
    await expect(superseded).toContainText('round 2')
    await expect(superseded.locator('.row')).toHaveAttribute(
      'href',
      `/reviews/${supersededReviewIds[1]}/understand`,
    )

    /*
     * **DEV-876's first acceptance criterion, on a real database.** This session wrote the
     * draft without waiting for the brief to be confirmed, so the review is waiting on the
     * developer for two things at once — and `GET /inbox` really does serve it in two of
     * its five arrays. One row, showing the earlier layer's action, saying the other is
     * behind it. The screen this replaced drew the title twice.
     */
    await expect(page.getByText('Retry failed webhook deliveries')).toHaveCount(1)
    /*
     * **Five rows for four pull requests that need you**, which is not a contradiction: the
     * fifth is the posted review under "Waiting on the author", and nothing there is
     * waiting on the developer. The headline counts what is.
     */
    await expect(page.locator('.row')).toHaveCount(5)
    await expect(page.getByText('Problem brief ready')).toBeVisible()
    await expect(page.getByText('nothing posts until you confirm')).toBeVisible()
    await expect(page.getByText('a draft is ready behind it')).toBeVisible()
    await expect(page.getByText('Read the brief')).toBeVisible()
    /*
     * **The ticket is on the row's own line, and named once.** DEV-936 gave the row a link
     * to the ticket beside a meta line that also named it; DEV-957 put the two on one line,
     * where the id read twice — "acme/portal · round 1 · ABC-231 · GitHub · ABC-231" — so
     * the component names it and the page's meta no longer does. This asserts the line as
     * it now reads, with the id in it exactly once.
     */
    const line = page.locator('.queue-line').filter({ hasText: '#412' }).locator('.row-meta')
    await expect(line).toContainText('acme/portal · round 1')
    /*
     * Counted in the line's own text rather than by locator: the ticket anchor carries a
     * visually hidden ", on the tracker, opens in a new tab", so an exact-text locator
     * matches nothing and a loose one would match whatever else held the id.
     */
    const said = (await line.textContent()) ?? ''
    expect(said.split('ABC-231').length - 1, `the id once, in: ${said}`).toBe(1)

    /*
     * **And the row opens the review, which starts at Understand** — Patrick on
     * a review, "when we click a review, we want to go to the review, not to the
     * draft". Every row lands here however much is waiting behind it; the trail is what
     * says which step that is, and it says it about all seven at once.
     */
    await expect(page.locator('.row').first()).toHaveAttribute(
      'href',
      `/reviews/${reviewId}/understand`,
    )

    /*
     * The groups with nothing in them are not drawn: no session is running on this
     * database and the server has no source for the developer's own pull requests.
     */
    await expect(page.getByRole('heading', { name: 'With the agent' })).toHaveCount(0)
    await expect(page.getByRole('heading', { name: 'Your pull requests' })).toHaveCount(0)

    /*
     * **The group the board drew and the API could not fill, filled.** It said "not
     * measured yet" with its reason until DEV-930, and the words are gone because the
     * absence is: `GET /inbox` serves a group for a posted review whose pull request has
     * not been seen merged or closed.
     */
    await expect(page.getByRole('heading', { name: 'Waiting on the author' })).toBeVisible()
    await expect(page.getByText('Not measured yet')).toHaveCount(0)
    await expect(
      page.getByText('serves no group for a posted review awaiting a reply'),
    ).toHaveCount(0)

    const posted = page.locator('.row').filter({ hasText: 'Record the delivery attempt' })
    await expect(posted).toHaveCount(1)
    /*
     * **Posted, and nothing holds when.** Its GitHub id was recorded by `review link`,
     * which needs no draft — so there is no `posted_at`, and the row says so where the age
     * goes rather than inventing one.
     */
    await expect(posted).toContainText('Posted')
    /*
     * And nothing has harvested it, so the three figures are absent rather than nought,
     * and the pull request's state is not claimed. Both halves of the rule the brief
     * states: a figure with no source is shown as unknown, never as a number.
     */
    await expect(posted).toContainText('Not harvested yet')
    await expect(posted).toContainText('pull request state not established')
    await expect(posted).toHaveAttribute('href', `/reviews/${postedReviewId}/understand`)

    /* And it is one click from the queue, which is the whole of what was asked for. */
    await posted.click()
    await expect(page).toHaveURL(new RegExp(`/reviews/${postedReviewId}/understand$`))
    await expect(page.getByText('Record the delivery attempt count')).toBeVisible()
    await page.goBack()

    /*
     * And no marker, because these are real records. The marker is the one thing on screen
     * that says otherwise, and a production build has no fixtures for it to be about.
     */
    await expect(page.getByText('Sample data')).toHaveCount(0)
    await expect(page.getByText('acme/portal', { exact: false }).first()).toBeVisible()
  })

  test('the brief is confirmed in the browser, and the record says the developer did it', async ({
    page,
  }) => {
    /* Nobody has confirmed it: the command line has no way to. One read, two fields. */
    const before = reviewRecord()
    expect(before.brief.confirmed_at).toBeNull()
    expect(before.brief.confirmed_by).toBeNull()

    /*
     * **On Understand, which is where confirming lives since DEV-878.** It was on
     * `/reviews/{id}`, and retiring that page without moving this would have left the
     * dashboard unable to confirm a brief at all — one of only two acts in the whole
     * system that record the developer's sign-off.
     */
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.getByText('Confirm the problem before anything is posted')).toBeVisible()
    await expect(
      page.getByText('A webhook delivery that fails once is never tried again'),
    ).toBeVisible()

    await page.getByRole('button', { name: 'Confirm the problem' }).click()

    /* On the banner and in the notice, which are two different sentences about it. */
    await expect(page.getByText('Problem confirmed.', { exact: true })).toBeVisible()
    await expect(page.getByText('Problem confirmed. The draft is yours to sign off.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Confirm the problem' })).toHaveCount(0)

    /*
     * **The actor, which is the whole point of the boundary.** Only the developer may sign
     * off, and the store refuses any other actor — but nothing had ever checked that the
     * dashboard's own path records the right one, because nothing had ever used that path.
     */
    const record = reviewRecord()
    expect(record.brief.confirmed_at).not.toBeNull()
    expect(record.brief.confirmed_by).toBe('developer')

    /*
     * **And the trail moved with it.** DEV-862 shipped this screen reading
     * "2 Understand · waiting on you" under a brief that had just been confirmed, and
     * recorded it as a known risk. This is the assertion that says it no longer does —
     * from the screen, which is where the fault was visible.
     */
    const layers = await trail(page)
    expect(layers[1]).toContain('Understand')
    expect(layers[1]).toContain('confirmed by the developer')
    expect(layers[1]).not.toContain('waiting on you')

    /*
     * **And `work list` still says nothing about this review**, which is the right
     * answer and worth asserting rather than assuming. This session ran ahead and wrote
     * its draft before the confirmation, so the draft is `ready` — waiting on the
     * developer to approve it — and a review waiting on the developer is not an agent's
     * work whatever layer 2 now says. `brief_settled` is for a review with no draft
     * yet, which is the waived one further down.
     */
    expect(workList().some(item => item.review_id === reviewId)).toBe(false)
  })

  test('the agent’s draft is what is left waiting to be signed off', async ({ page }) => {
    /*
     * Written by `draft put` during the seed, before any of this ran — and the one thing
     * left on the Inbox now that the brief is confirmed. An agent wrote it and cannot
     * sign it off; this is the group that says so.
     */
    await page.goto(seeded)
    /*
     * **Still five rows, and the same five.** What moved is this one's next action: from
     * the brief to the draft, because layer 2 is settled and layer 6 is not. That is what
     * "one row showing its next action" buys — a review waiting for two things does not
     * become two rows, and does not stop being a row when one of them is answered.
     */
    await expect(page.locator('.row')).toHaveCount(5)
    const webhooks = page.locator('.row').filter({ hasText: 'Retry failed webhook deliveries' })
    await expect(webhooks).toHaveCount(1)
    await expect(webhooks.getByText('Draft ready')).toBeVisible()
    await expect(page.getByText('Problem brief ready')).toHaveCount(0)
    await expect(page.getByText('a draft is ready behind it')).toHaveCount(0)
    /* Scoped to this row: the second review's own tally also ends in a nit to a ticket. */
    await expect(webhooks.getByText('1 blocker, 1 should')).toBeVisible()
    await expect(webhooks.getByText('1 nit to a ticket')).toBeVisible()

    /*
     * **A row goes to the review, and the draft is one click along the trail.**
     *
     * Patrick, on a review: "when we click a review, we want to go to the review,
     * not to the draft." In a page-per-step review the review starts at Understand, so
     * that is where every row lands however much is waiting behind it — and the trail,
     * which says which step is waiting, is what carries you on.
     *
     * This is the path that proves the queue does not dead-end: queue, Understand, trail,
     * draft. Linking step 6 is what makes it exist.
     */
    /*
     * This review's row, not the first one on the queue: two reviews are waiting since the
     * structured one gained a draft, and `.first()` picked whichever the ordering put on
     * top — which is a fact about the sort, not about this review.
     */
    await expect(webhooks.getByText('Read and sign off')).toBeVisible()
    await webhooks.click()
    await expect(page).toHaveURL(`${seeded}reviews/${reviewId}/understand`)

    await page.locator('.trail').getByRole('link', { name: /6 Draft/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${reviewId}/draft`)

    /*
     * **And back by the trail, which the draft now carries.** It used to carry a "Back to
     * the review" link instead, because it was outside the shell; inside it, that link
     * beside a trail whose first step is the review would be the same door twice.
     */
    await page.locator('.trail').getByRole('link', { name: /2 Understand/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${reviewId}/understand`)
    await page.locator('.trail').getByRole('link', { name: /6 Draft/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${reviewId}/draft`)
    await expect(
      page.getByText('Nothing has been posted. This is what GitHub will show.'),
    ).toBeVisible()
  })

  test('the draft is edited in the browser, and the edit is recorded against its finding', async ({
    page,
  }) => {
    await page.goto(`${seeded}reviews/${reviewId}/draft`)

    /* The checks the session's lint reported, read off the record. */
    await expect(page.getByText('96 of 150')).toBeVisible()
    await expect(page.getByText('2 of 2')).toBeVisible()
    await expect(page.getByText('You have not changed this draft.')).toBeVisible()

    await page.getByRole('tab', { name: 'Edit' }).click()
    const body = page.locator('textarea#draft-body')
    await expect(body).toBeVisible()

    const before = await body.inputValue()
    await body.fill(
      before.replace(
        'A second delivery of the same event is indistinguishable from a new one',
        'A retry of the same event cannot be told from a new one',
      ),
    )
    await page.getByRole('button', { name: 'Save the draft' }).click()

    /*
     * The edit is the strongest signal in the feedback loop, and it is recorded against the
     * finding it is about rather than as a diff — which is why the screen names F1.
     */
    await expect(page.getByText('1 edit recorded for the harvester')).toBeVisible()
    await expect(page.getByText('F1 · wording')).toBeVisible()

    /* And it is on the record, where the harvester will find it. */
    const outcomes = cli(seededDatabase, ['review', 'show', '--review', String(reviewId)])
    expect(outcomes).toContain('rewritten')
  })

  test('the draft is sent back in the browser, and layer 6 goes back to in progress', async ({
    page,
  }) => {
    /*
     * The developer's other act at layer 6, and the one that had never been performed end
     * to end. Sending a draft back is not a rejection: it is the layer going back to the
     * session with an instruction, which is why the trail reads `in progress` carrying the
     * note rather than `waiting on you`.
     */
    await page.goto(`${seeded}reviews/${reviewId}/draft`)
    await page.getByRole('button', { name: 'Send back with a note' }).click()

    const note = 'F2 is the same point as F1; cut one of them'
    await page.locator('textarea#send-back-note').fill(note)
    await page.getByRole('button', { name: 'Send back', exact: true }).click()

    /*
     * Scoped to the card, because the note is legitimately in two places now that this
     * screen carries the trail: the card the developer reads, and step 6's own outcome
     * line in the trail — which is the trail doing exactly what it is for.
     */
    await expect(page.locator('.sent-back')).toContainText(note)
    await expect(page.locator('.trail')).toContainText(note)

    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    const layers = await trail(page)
    expect(layers[5]).toContain('Draft')
    expect(layers[5]).toContain(note)

    /* And the session is told what to do about it, with the note travelling as data. */
    const waiting = workList().find(item => item.review_id === reviewId)
    expect(waiting?.kind).toBe('draft_sent_back')
    expect(waiting?.note).toBe(note)

    /*
     * The session revises it, which is what puts it back to `ready`. The body is read off
     * the record and written back unchanged, so the edit the previous test made is still
     * what the later tests find — a draft the dashboard saves stays `drafting`, because
     * returning a sent-back draft to `ready` is the session saying it has finished.
     */
    const shown = cliJson(seededDatabase, [
      'draft',
      'show',
      '--review',
      String(reviewId),
    ]) as unknown as { draft: { body: string } }
    const revised = resolve('e2e/.revised-draft.md')
    writeFileSync(revised, shown.draft.body)
    cli(seededDatabase, ['draft', 'put', '--review', String(reviewId), '--file', revised])
    rmSync(revised, { force: true })

    expect(workList().some(item => item.review_id === reviewId)).toBe(false)
  })

  test('the draft is signed off in the browser, and the record says the developer did it', async ({
    page,
  }) => {
    const before = reviewRecord()
    expect(before.draft.status).toBe('ready')
    expect(before.draft.approved_by).toBeNull()

    await page.goto(`${seeded}reviews/${reviewId}/draft`)
    await page.locator('select#event').selectOption('request_changes')
    await page.getByRole('button', { name: 'Sign off and post' }).click()

    await expect(
      page.getByText(
        'Signed off. Will be posted as "Request changes" — waiting for the session to post it.',
      ),
    ).toBeVisible()
    await expect(
      page.getByText('The review session posts it under your name, with no bot banner.'),
    ).toBeVisible()
    await expect(page.getByRole('button', { name: 'Sign off and post' })).toBeDisabled()

    const record = reviewRecord()
    expect(record.draft.status).toBe('approved')
    expect(record.draft.event).toBe('request_changes')
    /* The second sign-off, and the second actor the record now has to be right about. */
    expect(record.draft.approved_by).toBe('developer')

    /* Layer 6 is finished with, and the note the developer wrote earlier is gone from it. */
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    const layers = await trail(page)
    expect(layers[5]).toContain('signed off by the developer')

    /*
     * **And this is the link DEV-864 exists to make.** The dispatcher polls one command
     * and is told that a review it has never heard of needs an agent to post a draft the
     * developer approved in a browser.
     */
    const waiting = workList().find(item => item.review_id === reviewId)
    expect(waiting?.kind).toBe('draft_approved')
    expect(waiting?.note).toBeNull()
  })

  test('`review-desk draft show` reports it approved, with the chosen event', async () => {
    /*
     * **This is how a review session learns it may post**, and it is the last link in the
     * chain the criterion names: an agent reads the developer's decision back out of the
     * command line it already uses.
     */
    const printed = cli(seededDatabase, ['draft', 'show', '--review', String(reviewId)])

    expect(printed).toContain(`draft of review ${reviewId}: approved`)
    expect(printed).toContain('post as: request_changes')
    /* And the body it will post is the one the developer edited, not the one before. */
    expect(printed).toContain('A retry of the same event cannot be told from a new one')
  })

  test('the command line still cannot sign off', async () => {
    /*
     * The other half of the boundary. The dashboard can do these two things; the command
     * line an agent session calls cannot, and `brief confirm` says so rather than failing
     * with clap's "unrecognized subcommand".
     */
    let refused = ''
    try {
      cli(seededDatabase, ['brief', 'confirm', '--review', String(reviewId)])
    }
    catch (thrown) {
      refused = String((thrown as { stderr?: Buffer }).stderr ?? thrown)
    }
    expect(refused).toContain('brief confirm has been removed')
    expect(refused).toContain('dashboard')
  })

  test('a refused write says why and keeps what the developer typed', async ({ page }) => {
    /*
     * The draft is approved now, and an approved draft is what the review session reads and
     * posts — so editing it underneath is refused. The refusal must not take the text with
     * it: DEV-862 asks for this by name, and it is the one kind of refusal where losing the
     * input would cost real work.
     */
    await page.goto(`${seeded}reviews/${reviewId}/draft`)
    await page.getByRole('tab', { name: 'Edit' }).click()

    const body = page.locator('textarea#draft-body')
    const typed = 'An edit the record will refuse, which must still be here afterwards.'
    await body.fill(typed)
    await page.getByRole('button', { name: 'Save the draft' }).click()

    await expect(page.getByText('cannot be edited')).toBeVisible()
    await expect(page.getByText('what the review session reads and posts')).toBeVisible()
    await expect(body).toHaveValue(typed)
  })

  test('the code review page shows the findings and what was not checked', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/code-review`)

    /* The tally at the foot, which is the three lists in one line. */
    await expect(page.getByText('2 to post, 1 for a ticket, 0 set aside')).toBeVisible()
    await expect(
      page.locator('.title').filter({ hasText: 'A retried delivery is sent without an idempotency key' }),
    ).toBeVisible()
    await expect(page.getByText('jobs/DeliverWebhook.php:58')).toBeVisible()
    await expect(
      page.getByText('The admin screens, which this change does not touch.'),
    ).toBeVisible()

    /* The seven steps of the trail, as the command line set them. */
    await expect(page.locator('.steps li')).toHaveCount(7)
  })

  /*
   * **The address the one-page review had still works, and arrives at the review.** It is
   * in links people have and in anything written down; a route that 404s is worse than one
   * that lands somewhere useful, and every queue row already lands on Understand.
   */
  test('the retired one-page review redirects to Understand', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}`)
    await expect(page).toHaveURL(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.locator('.trail')).toBeVisible()
  })

  test('a brief that needed no confirmation is in nobody’s queue, and says so', async ({
    page,
  }) => {
    /*
     * **DEV-864's second part, end to end.** `confirmation_required: false` is the one
     * place an agent decides a developer step was unnecessary, and this review's brief was
     * written that way by the same command line an agent calls.
     *
     * The Inbox is the developer's queue, so the review is not in it. The One review
     * screen is where the developer reads a review, so it is there — with a third
     * sentence that is neither "confirm this" nor "you confirmed this", and no button.
     */
    await page.goto(seeded)
    await expect(page.getByText('Confirm the problem')).toHaveCount(0)
    await expect(
      page.getByText('Say "not measured" rather than null in the delivery log'),
    ).toHaveCount(0)

    await page.goto(`${seeded}reviews/${waivedReviewId}/understand`)
    await expect(page.getByText('Confirmation not required.')).toBeVisible()
    /*
     * `.first()`, because the reason is legitimately in two places on this screen: the
     * card the developer reads, and layer 2's detail in the trail. Both are asserted —
     * the trail further down — and a bare locator matches both, which is a strict-mode
     * failure rather than a missing string.
     */
    await expect(
      page.getByText('61 changed lines, under this caller\'s 200-line threshold').first(),
    ).toBeVisible()
    await expect(page.getByRole('button', { name: 'Confirm the problem' })).toHaveCount(0)
    await expect(page.getByText('Problem confirmed.')).toHaveCount(0)

    /* The brief itself is on screen: the developer can still read it. */
    /*
     * **The headline, with its code name drawn as code.** The seeded brief writes
     * "prints `null` for a latency" with the backticks a session writes, and since DEV-917
     * the page draws that pair as a chip — so the sentence on screen has no backticks in
     * it, and the word is a `<code>` element.
     */
    await expect(
      page.getByText('The delivery log prints null for a latency it never measured'),
    ).toBeVisible()
    await expect(
      page.locator('code').filter({ hasText: /^null$/ }).first(),
      'the backtick pair is a code chip, not two characters',
    ).toBeVisible()

    /* Layer 2 is finished with on write, and the trail carries why. */
    const layers = await trail(page)
    expect(layers[1]).toContain('confirmation not required')
    expect(layers[1]).toContain('200-line threshold')

    /*
     * **And nothing in the record claims the developer confirmed it.** An agent may carry
     * on past layer 2, and the item says the sign-off was waived rather than given.
     */
    const waiting = workList().find(item => item.review_id === waivedReviewId)
    expect(waiting?.kind).toBe('brief_settled')
    expect(waiting?.sign_off).toBe('waived')
  })

  test('a diagram that is not a diagram is shown as written, and leaves nothing behind', async ({
    page,
  }) => {
    /*
     * **DEV-880, end to end.** The waived review's brief carries the text sketch a review
     * session really wrote into a `diagram` field. mermaid rejects it, and before the fix
     * it drew its own error graphic — a bomb, "Syntax error in text", "mermaid version
     * 12.1.0" — into a `<div>` it appends to `document.body` and then threw before
     * removing it. The dashboard is one page, so it stayed there for the rest of the
     * session.
     *
     * Three things are asserted and they fail separately: the sketch is readable where the
     * diagram would be, nothing of mermaid's is outside the component, and the graphic is
     * still absent two navigations later.
     */
    await page.goto(`${seeded}reviews/${waivedReviewId}/understand`)

    const figure = page.locator('figure')
    await expect(figure.locator('pre')).toContainText(
      'FortifyServiceProvider reads url.intended',
    )
    await expect(figure.getByText('Shown as written.')).toBeVisible()
    await expect(figure.getByText('could not be drawn')).toHaveCount(0)

    /** Everything mermaid leaves behind on a failed render, looked for at the document. */
    const leavings = async () => page.evaluate(() => ({
      orphans: [...document.body.children]
        .filter(element => /^d?i?mermaid-/.test(element.id))
        .map(element => element.id),
      bombs: document.querySelectorAll('path.error-icon').length,
      saysSyntaxError: document.body.textContent?.includes('Syntax error in text') ?? false,
    }))

    expect(await leavings()).toEqual({ orphans: [], bombs: 0, saysSyntaxError: false })

    /* Away from the review, and back past it: the page it was on is not the only one. */
    await page.getByRole('link', { name: 'Rules' }).click()
    await expect(page).toHaveURL(/\/rules$/)
    expect(await leavings()).toEqual({ orphans: [], bombs: 0, saysSyntaxError: false })

    await page.getByRole('link', { name: 'Queue' }).click()
    await expect(page).toHaveURL(new RegExp(`^${seeded}$`))
    expect(await leavings()).toEqual({ orphans: [], bombs: 0, saysSyntaxError: false })
  })

  test('a diagram that is a diagram is still drawn', async ({ page }) => {
    /*
     * The other half of DEV-880, and the reason the sketch above went on the waived review
     * rather than this one. The seed's brief carries a `flowchart`, and a browser is the
     * only thing in this repository that can say it was drawn — jsdom has no `getBBox`, so
     * the component tests cannot.
     */
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    const drawing = page.locator('figure svg')
    await expect(drawing).toBeVisible()
    await expect(drawing).toContainText('Deliver')
    await expect(page.locator('figure').getByText('could not be drawn')).toHaveCount(0)
    await expect(page.locator('figure').getByText('Shown as written.')).toHaveCount(0)
  })

  test('the command line refuses to withdraw a confirmation already asked for', async () => {
    /*
     * The other half of the waiver, and the whole reason it is safe to have. The main
     * review's brief was written as requiring confirmation — and was confirmed in a
     * browser earlier in this run — so an agent cannot rewrite it as needing none.
     */
    let refused = ''
    const brief = resolve('e2e/.waiver-attempt.json')
    writeFileSync(
      brief,
      JSON.stringify({
        problem: 'A webhook delivery that fails once is never tried again.',
        confirmation_required: false,
        confirmation_not_required_because: 'on reflection it is small',
      }),
    )
    try {
      cli(seededDatabase, ['brief', 'put', '--review', String(reviewId), '--file', brief])
    }
    catch (thrown) {
      const failure = thrown as { stderr?: Buffer, status?: number }
      refused = String(failure.stderr ?? thrown)
      expect(failure.status).toBe(1)
    }
    finally {
      rmSync(brief, { force: true })
    }

    expect(refused).toContain('confirmation_required')
    expect(refused).toContain('cannot be withdrawn')
  })

  test('the rules screen reads from the database too', async ({ page }) => {
    await page.goto(`${seeded}rules`)

    /*
     * The repository came from the review, through `GET /repos` — a repository appears once
     * it has either a review or a rule. There are no rules, and the screen says so rather
     * than drawing an empty list: the command line has no way to write one, which is
     * recorded in the pull request as a gap rather than worked around here.
     */
    await expect(page.getByRole('radio', { name: /acme\/portal/ })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await expect(page.getByText('Sample data')).toHaveCount(0)
  })
})

/*
 * **The three step pages, walked in the order a reader walks them.**
 *
 * DEV-885's acceptance criterion: "the end-to-end test extended to walk Understand to
 * Alternatives to Solution". It is one test rather than three, because the thing being
 * checked is the walk — that the trail on each page carries you to the next, that the same
 * numbered problems appear on all three, and that nothing on the way is a dead end.
 *
 * The review it walks is the one the seed built from
 * `contract/examples/confirmBrief.response.real-length.json`, through the command line and
 * the binary like everything else here: twenty-six files, eight options, three numbered
 * problems and thirteen findings.
 */
test.describe('the three step pages of one review', () => {
  test('Understand to Alternatives to Solution, by the trail', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)

    /* ---- 2 · Understand ---- */

    await expect(
      page.getByRole('heading', {
        name: /Scheduled exports get their own context/,
        level: 1,
      }),
    ).toBeVisible()

    /* Both flows, drawn as boxes. Five rows before and six after, from the record. */
    await expect(page.locator('.flow')).toHaveCount(2)
    await expect(page.locator('.flow').first().locator('.row')).toHaveCount(5)
    await expect(page.locator('.flow').nth(1).locator('.row')).toHaveCount(6)
    /* And no mermaid figure: a flow is HTML, which is what DEV-880 bought. */
    await expect(page.locator('figure')).toHaveCount(0)

    /* The captions come off the record, not out of the page. */
    await expect(
      page.getByText('The tenant arrives through a slot the web also writes'),
    ).toBeVisible()

    /* A problem opens its detail in place. The URL does not change. */
    const before = page.url()
    await page.getByRole('button', { name: /A scheduled export read its tenant/ }).click()
    await expect(page.locator('#problem-1')).toBeVisible()
    await expect(page.locator('#problem-1')).toContainText('The request-scoped singleton')
    expect(page.url()).toBe(before)
    /* And its boxes are picked out of both flows at once. */
    await expect(page.locator('.box--focus')).toHaveCount(4)

    /* The supporting table, and how it works today. */
    /*
     * By role: the section's heading and the table's own `<caption>` carry the same words,
     * which is deliberate — the heading names the section and the caption names the table
     * for a reader navigating by table.
     */
    await expect(
      page.getByRole('heading', { name: 'The idea: three questions, each with one owner' }),
    ).toBeVisible()
    await expect(page.getByText('Every report is built by one class.')).toBeVisible()

    /*
     * The open choices are **not** here. One list, drawn once: this page carries the line
     * and the link, and Solution carries the table.
     */
    await expect(
      page.getByRole('heading', { name: 'Where the ticket left a choice' }),
    ).toHaveCount(0)
    await expect(page.getByText('3 choices the ticket left open')).toBeVisible()

    /* ---- 3 · Alternatives, reached by the trail ---- */

    await page.locator('.trail').getByRole('link', { name: /3 Alternatives/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/alternatives`)

    /* Eight options, three problem columns. */
    await expect(page.locator('.option')).toHaveCount(8)
    await expect(page.locator('thead th')).toHaveCount(6)
    /* `not_assessed` is drawn as the verdict it is — option D, which nothing scored. */
    await expect(page.locator('.word--not_assessed')).toHaveCount(3)
    await expect(page.getByText('Not assessed').first()).toBeVisible()
    /* The built option, and the four the ticket never had. */
    await expect(page.locator('.option--built')).toHaveCount(1)
    await expect(page.locator('.option--blind')).toHaveCount(4)
    /* The blind pass ran, and this page says which of its three states that is. */
    await expect(page.getByText('A blind pass ran', { exact: false }).first()).toBeVisible()
    await expect(page.locator('.question')).toHaveCount(4)

    /* An option opens its reasoning in place. */
    await page.getByRole('button', { name: /An export context passed explicitly/ }).click()
    await expect(page.locator('#option-C')).toContainText('Fixes all three')
    await expect(page.locator('#option-C')).toContainText('Two entry points to keep in step')

    /* ---- 4 · Solution, reached by the trail ---- */

    await page.locator('.trail').getByRole('link', { name: /4 Solution/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/solution`)

    /* The verdict, and the size of the change counted off the parts. */
    await expect(
      page.getByRole('heading', { name: /C is the right shape/, level: 1 }),
    ).toBeVisible()
    /* Every file the parts name, and how many the pull request did not touch. */
    await expect(
      page.getByText('27 files (3 the pull request did not change)'),
    ).toBeVisible()
    await expect(page.getByText('959 lines added and 423 removed')).toBeVisible()
    await expect(page.locator('.part')).toHaveCount(7)

    /* The three groups, each from the rule and each with a row under it. */
    for (const heading of [
      'Fixes a problem',
      'Supporting',
      'Not changed, but the change relies on it',
    ]) {
      await expect(page.getByText(heading, { exact: true })).toBeVisible()
    }
    /* A part the review found nothing in says so, and one outside the change says that. */
    await expect(page.getByText('Nothing').first()).toBeVisible()
    await expect(page.getByText('unchanged').first()).toBeVisible()

    /*
     * A row opens in place to the part's full summary and the findings joined to it **by
     * file** — the one place a finding is shown beside the part of the change it is about.
     * The files are behind a fold, closed, because they are reference.
     */
    await page.getByRole('button', { name: /Allowed columns on the report definition/ }).click()
    const part = page.locator('#part-2')
    await expect(part).toBeVisible()
    /*
     * **The summary, which says more than the row's one line did.** That is the whole
     * point of the fold: the row carries "A new column says which fields each report may
     * export", and this carries the backfill and what reads the column.
     *
     * **One assertion, on a phrase only the summary has.** A second one here matched
     * `allowed_columns`, which proved nothing: `toContainText` reads `textContent`,
     * including the closed fold two lines below — and that fold lists
     * `…_add_allowed_columns_to_report_definitions.php`. It would have passed with the
     * summary deleted.
     */
    await expect(part).toContainText('with a backfill for the rows recorded before it')
    await expect(part).toContainText('F6')
    await expect(part).toContainText('F9')
    /* Not the file list, which is one click further in. */
    await expect(part.locator('details')).toHaveCount(1)
    await expect(part.locator('details[open]')).toHaveCount(0)
    /*
     * **Not visible, rather than not present.** A closed `<details>` keeps its content in
     * the document — which is what makes it findable and is why the fold is the right
     * element — so "behind a fold" is a claim about what is drawn, and that is what is
     * measured.
     */
    const path = part.getByText('app/Reporting/ColumnPolicy.php')
    await expect(path).not.toBeVisible()
    await part.locator('summary').click()
    await expect(path).toBeVisible()

    /*
     * **A finding line reaches that finding on the code review page**, which is the whole
     * reason the row carries counts rather than the findings themselves.
     */
    await part.locator('.finding', { hasText: 'F9' }).click()
    await expect(page).toHaveURL(
      `${seeded}reviews/${structuredReviewId}/code-review#F9`,
    )
    await expect(page.locator('#finding-9')).toBeVisible()
    /* `.title .key` — the row's own reference. The decision buttons carry `.key` too, for
     * their keyboard shortcuts. */
    await expect(page.locator('li.row--open').locator('.title .key')).toHaveText('F9')

    await page.goBack()
    await expect(page.locator('.part').first()).toBeVisible()

    /* A file the pull request never changed says so, rather than showing two noughts. */
    await page.getByRole('button', { name: /One filter chain for both paths/ }).click()
    await page.locator('#part-3').locator('summary').click()
    await expect(page.locator('#part-3')).toContainText('not in the change')

    /* Findings no part accounts for are listed rather than lost. */
    await expect(
      page.getByRole('heading', { name: '2 findings are not about any one part' }),
    ).toBeVisible()

    /* The open choices and the disagreements, each naming its finding. */
    await expect(
      page.getByRole('heading', { name: 'Where the ticket left a choice' }),
    ).toBeVisible()
    await expect(page.getByText('Departs from the ticket')).toBeVisible()
    await expect(page.getByText('F3 raises it')).toBeVisible()
    await expect(
      page.getByRole('heading', { name: 'Where the description and the code disagree' }),
    ).toBeVisible()

    /* ---- 5 and 6, which DEV-878 brought into the same shell ---- */

    /*
     * The trail is the only way to either, now that every queue row opens Understand — so
     * these two links are what stop the queue leading somewhere a reader cannot leave.
     * Both pages carry the trail themselves, so there is no way out of it at all.
     */
    await page.locator('.trail').getByRole('link', { name: /5 Code review/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/code-review`)
    /*
     * **And back by the trail, not by a link of the page's own.** Both screens carried one
     * — "Back to Understand" here, "Back to the review" on the draft — because neither was
     * inside the shell. Both are now, so the way back is the trail, and a link beside a
     * trail whose first step is the review would be the same door twice.
     */
    await expect(page.locator('.trail')).toBeVisible()
    await expect(
      page.getByRole('link', { name: /^Back to / }),
      'the back links went with the dead ends they were for',
    ).toHaveCount(0)
    await page.locator('.trail').getByRole('link', { name: /2 Understand/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/understand`)

    /* ---- and back to 2 · Understand, which is what "move back freely" means ---- */

    /*
     * **With the URL asserted in between.** Without it this proved nothing: step 2 is a
     * self-link on the Understand page, so Playwright could resolve and click it before
     * the navigation to `/alternatives` finished and the closing assertion would pass
     * either way.
     */
    await page.locator('.trail').getByRole('link', { name: /3 Alternatives/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/alternatives`)
    await page.locator('.trail').getByRole('link', { name: /2 Understand/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/understand`)

    /*
     * **And step 6 is a link, which it was not before DEV-878.** It was guarded on the
     * draft status, because the draft screen was outside the shell and a review with no
     * draft landed on a blocking refusal carrying no trail and no way back. The draft
     * screen is step 6 now and carries the trail either way, so the guard went with the
     * dead end it was for.
     */
    await page.locator('.trail').getByRole('link', { name: /6 Draft/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/draft`)
    await page.locator('.trail').getByRole('link', { name: /2 Understand/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/understand`)

    /*
     * **And step 7 is a link since DEV-958**, which is the issue that gave it a page:
     * "What happened" is the whole of what came back on a posted review, and
     * `e2e/what-happened.spec.ts` measures it at both widths against a seed of its own.
     */
    await page.locator('.trail').getByRole('link', { name: /7 What happened/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/what-happened`)
    await page.locator('.trail').getByRole('link', { name: /2 Understand/ }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${structuredReviewId}/understand`)

    /* Lint alone is a label, not a link: it is the one step with no page of its own. */
    await expect(page.locator('.trail').getByRole('link', { name: /1 Lint/ })).toHaveCount(0)
  })

  /*
   * **Step 2's note, written and put back.** Outside the test and restored in a hook for
   * the reason the harvest step's writer below gives: a `finally` does not run when
   * Playwright abandons a test at its timeout, and this test measures two viewports. One
   * timeout would leave step 2 reading "Three short words" on the database every later
   * test and the screenshots project share.
   */
  const setStepTwoDetail = (detail: string) =>
    cli(seededDatabase, [
      'layer',
      'set',
      '--review',
      String(structuredReviewId),
      '--layer',
      '2',
      '--state',
      'done',
      '--detail',
      detail,
    ])

  test.describe('a long step detail', () => {
    test.afterEach(() => {
      setStepTwoDetail(STRUCTURED_LONG_NOTE)
    })

    /**
     * **A step's detail is one line, whatever its length, and the page's headline does not
     * move.**
     *
     * DEV-917's first two acceptance criteria, measured where they can be: both are layout,
     * and the unit tests run in jsdom, which computes none. So this reads the rendered height
     * of step 2's line — the 34-word note the invented example carries — at a laptop's width
     * and at a phone's, and then rewrites that step with a three-word detail through the
     * command line and checks the headline is in exactly the same place.
     */
    test('is one line at both widths, and moves the headline nowhere', async ({ page }) => {
    /** The step's line and the page's headline, as the browser lays them out. */
      const measure = async () => {
        await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
        await expect(page.locator('h1')).toBeVisible()
        await page.waitForLoadState('networkidle')
        const step = page.locator('.steps li').nth(1)
        await expect(step).toContainText('2 Understand')
        return {
          line: await step.locator('.outcome').evaluate((element) => {
            const style = getComputedStyle(element)
            return {
              height: element.getBoundingClientRect().height,
              lineHeight: Number.parseFloat(style.lineHeight)
                || Number.parseFloat(style.fontSize) * 1.2,
              scrollWidth: element.scrollWidth,
              clientWidth: element.clientWidth,
              whiteSpace: style.whiteSpace,
              textOverflow: style.textOverflow,
            }
          }),
          headline: await page.locator('h1').evaluate(
            element => Math.round(element.getBoundingClientRect().top + window.scrollY),
          ),
        }
      }

      /* The 34-word detail, at a laptop's width and at a phone's. */
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: 900 })
        const { line } = await measure()

        /*
       * **One line of text, overflowing rather than wrapping.** The element is 44px tall
       * because it is also the control that opens the card, so what is measured is the
       * text: `nowrap`, an ellipsis, and more text than there is room for — without that
       * last part this test would pass against a short detail and prove nothing.
       */
        expect(line.whiteSpace, `at ${width}px the line is ${line.whiteSpace}`).toBe('nowrap')
        expect(line.textOverflow).toBe('ellipsis')
        expect(
          line.scrollWidth,
          `at ${width}px the detail is ${line.scrollWidth}px of text in ${line.clientWidth}px`,
        ).toBeGreaterThan(line.clientWidth)
        expect(
          line.height,
          `at ${width}px the line is drawn ${line.height}px tall at a ${line.lineHeight}px line`,
        ).toBeLessThan(line.lineHeight * 2 + CONTROL_MIN)
      }

      /*
     * **And the headline sits where it would with a three-word detail.** Written through
     * the command line, which is the only way a step's detail is ever written, and put back
     * afterwards because every other test here reads the same database.
     */
      await page.setViewportSize({ width: 1440, height: 900 })
      const withThirtyFour = (await measure()).headline

      setStepTwoDetail('Three short words')
      const withThree = (await measure()).headline
      expect(
        withThree,
        `the headline is at ${withThirtyFour}px with 34 words and ${withThree}px with three`,
      ).toBe(withThirtyFour)
    })
  })

  /**
   * **Opening a step's card moves no other element on the page.**
   *
   * DEV-917's third acceptance criterion, in its own words: "a test compares positions
   * before and after". It is the reason the card floats rather than opening in place —
   * DEV-892's control did open in place, and everything below it moved every time a reader
   * asked for the rest of a note.
   */
  test('opening a step’s card moves nothing else', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()
    await page.waitForLoadState('networkidle')

    /* Every element with a box on this page, by where it is. */
    const positions = () => page.evaluate(() =>
      [...document.querySelectorAll('h1, h2, .steps li, section, .totals, .pair, .key')]
        .map((element) => {
          const box = element.getBoundingClientRect()
          return `${element.tagName}.${element.className}@${Math.round(box.top + window.scrollY)}x${Math.round(box.left)}`
        }))

    const before = await positions()
    const tall = await page.evaluate(() => document.documentElement.scrollHeight)

    const control = page.locator('.steps li').nth(1).locator('.outcome')
    await control.click()
    await expect(page.locator('.floating')).toBeVisible()

    expect(await positions(), 'nothing on the page moved').toEqual(before)
    expect(
      await page.evaluate(() => document.documentElement.scrollHeight),
      'and the page did not grow',
    ).toBe(tall)

    /*
     * The card really is over the page rather than beside it: it overlaps whatever was
     * under the step, which is what "nothing below it moves" means in the end.
     */
    const overlaps = await page.evaluate(() => {
      const card = document.querySelector('.floating')!.getBoundingClientRect()
      return [...document.querySelectorAll('h1, h2')].some((element) => {
        const box = element.getBoundingClientRect()
        return card.bottom > box.top && card.top < box.bottom
          && card.right > box.left && card.left < box.right
      })
    })
    expect(overlaps, 'the card is drawn over the page, not between its rows').toBe(true)
  })

  /**
   * **The card opens and closes by keyboard alone, and the focus comes back.**
   *
   * The acceptance criterion's own list. Hover is welcome and is not enough: a reader on a
   * keyboard has no hover, and a reader on a phone has no pointer at all.
   */
  test('a step’s card opens and closes by keyboard, and returns the focus', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()

    const control = page.locator('.steps li').nth(1).locator('.outcome')
    await control.focus()
    await expect(control).toBeFocused()

    await page.keyboard.press('Enter')
    const card = page.locator('.floating')
    await expect(card).toBeVisible()
    await expect(card).toContainText('Problems')
    await expect(control).toHaveAttribute('aria-expanded', 'true')

    await page.keyboard.press('Escape')
    await expect(card).toHaveCount(0)
    await expect(control, 'the focus is back on the step that opened it').toBeFocused()

    /* And Space opens it too, since it is a button rather than a link. */
    await page.keyboard.press('Space')
    await expect(card).toBeVisible()

    /*
     * **A press outside closes it**, and "outside" has to mean somewhere the card is not
     * drawn over: the line naming the review, above the trail. Pressing the headline would
     * press the card itself, which overlays it — that overlap is the whole point of the
     * card and is measured in the test above.
     *
     * **Beside that line rather than on it.** Since DEV-936 `.where` holds two
     * `target="_blank"` links and begins with one, and `click()` aims at the box centre —
     * so whether this gesture pressed inert text or opened a GitHub tab depended on the
     * title's length and the column's width. It closed the card either way, which is what
     * makes it the bad kind of fault: a stray popup and a latent flake behind a passing
     * assertion.
     *
     * The margin to the left of the content column, at that line's own height, is outside
     * the card and outside everything clickable. Checked rather than assumed: a press
     * whose target turns out to be a link is the failure this is avoiding, so the test
     * fails on it instead of opening a tab.
     */
    const at = await page.locator('.where').boundingBox()
    expect(at, 'the line naming the review is drawn').not.toBeNull()
    const beside = { x: 4, y: at!.y + at!.height / 2 }

    const pressed = await page.evaluate(({ x, y }) => {
      const found = document.elementFromPoint(x, y)
      return found
        ? `${found.tagName.toLowerCase()} ${found.closest('a') ? 'in a link' : 'inert'}`
        : null
    }, beside)
    expect(pressed, 'the point pressed is outside every link').toMatch(/ inert$/)

    await page.mouse.click(beside.x, beside.y)
    await expect(card).toHaveCount(0)
  })

  /**
   * **A step with nothing behind it has no card and no control for one.**
   *
   * Steps 4 and 7 of the seeded review have none, which is most steps of most reviews and
   * every step of every review recorded before DEV-917. Step 7 counts here because the
   * seed leaves it `pending`: a read time is written only while it is `running`, and it is
   * the one step that can draw a card without facts.
   */
  test('a step with nothing behind it draws no control', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()

    for (const index of [3, 6]) {
      const step = page.locator('.steps li').nth(index)
      await expect(step.locator('.outcome')).toBeVisible()
      await expect(step.locator('button')).toHaveCount(0)
    }
    await expect(page.locator('.trail button')).toHaveCount(5)
  })

  /*
   * **The write and the putting back, outside the test.**
   *
   * `--clear-facts` is a flag and `--fact` takes a pair, so the caller passes whichever
   * whole arguments it means rather than a list this helper has to guess at.
   *
   * **Put back in an `afterEach`, not in a `finally`.** Found by the review of the fix
   * commit: a `finally` does not run when Playwright abandons a test at its 60-second
   * timeout, and this one does two viewport passes with two reloads and three full-page
   * measurements. One timeout would then leave step 7 harvested on the database every
   * later test and the screenshots project share — the test above would count six trail
   * controls instead of five, and nine committed screenshots would regenerate with a
   * harvested step 7. A hook runs either way.
   */
  const setStepSeven = (state: string, detail: string, aboutTheFacts: string[]) =>
    cli(seededDatabase, [
      'layer',
      'set',
      '--review',
      String(structuredReviewId),
      '--layer',
      '7',
      '--state',
      state,
      '--detail',
      detail,
      ...aboutTheFacts,
    ])

  /**
   * **A harvested step 7 is drawn on the trail, and nothing on the page moves.**
   *
   * DEV-933's sixth acceptance criterion, in a browser because that is the only thing that
   * computes a layout. Until DEV-933 step 7 only ever said "posted, waiting for the
   * harvest" — nothing in `crates/harvest` wrote a layer — so the state measured here is
   * one the trail had never been drawn with: a step at `running`, carrying facts, in the
   * group that repeats each round.
   *
   * **Written through the command line**, which is how every step of this seed is written,
   * with the words and the pairs `Store::record_harvest_step` produces — pinned in
   * `crates/store/tests/harvest.rs` and in the harvester's own tests against the recording.
   */
  test.describe('a harvested step 7', () => {
    /* Exactly what `seedStructured` wrote, word for word, whatever the test did or how it
       ended. */
    test.afterEach(() => {
      setStepSeven('pending', STRUCTURED_STEP_SEVEN_DETAIL, ['--clear-facts'])
    })

    test('draws on the trail and moves nothing', async ({ page }) => {
      const understand = `${seeded}reviews/${structuredReviewId}/understand`

      /* Every element with a box on this page, by where it is. */
      const positions = () => page.evaluate(() =>
        [...document.querySelectorAll('h1, h2, .steps li, section, .totals, .pair, .key')]
          .map((element) => {
            const box = element.getBoundingClientRect()
            return `${element.tagName}.${element.className}@${Math.round(box.top + window.scrollY)}x${Math.round(box.left)}`
          }))

      for (const [label, width, height] of [
        ['a laptop', 1440, 900],
        ['a phone', 390, 844],
      ] as const) {
        await page.setViewportSize({ width, height })

        /* What the trail looks like for a review no harvest has read. */
        setStepSeven('pending', 'posted, waiting for the harvest', ['--clear-facts'])
        await page.goto(understand)
        await expect(page.locator('h1')).toBeVisible()
        await page.waitForLoadState('networkidle')
        const step = page.locator('.steps li').nth(6)
        await expect(step).toContainText('7 What happened')
        await expect(step.locator('.outcome')).toHaveText('posted, waiting for the harvest')
        await expect(step.locator('button')).toHaveCount(0)
        const before = await positions()
        const tall = await page.evaluate(() => document.documentElement.scrollHeight)

        /* And what a harvest of an open pull request leaves there. */
        setStepSeven('running', '3 replies back', [
          '--fact',
          'Pull request=open',
          '--fact',
          'Replies=3',
          '--fact',
          'Threads resolved=3',
          '--fact',
          'Code changed since=3 findings',
        ])
        await page.reload()
        await expect(page.locator('h1')).toBeVisible()
        await page.waitForLoadState('networkidle')
        /*
           **The line is the store's four words and nothing else, and it fits.** That is
           what the four-word rule buys and what measuring this column settled: the two
           facts together run 186 to 239 pixels in 154, so the read time is on the card
           and in the title instead. The word "running" reaches the page nowhere.
        */
        await expect(step.locator('.outcome')).toHaveText(/^3 replies back/)
        await expect(step.locator('.outcome')).not.toHaveText(/running/)
        await expect(step.locator('.outcome')).toHaveAttribute(
          'title',
          '3 replies back — read just now',
        )
        const line = await step.locator('.outcome').evaluate(element => ({
          scroll: element.scrollWidth,
          client: element.clientWidth,
        }))
        expect(
          line.scroll,
          `at ${label} the step's own words are ${line.scroll}px in ${line.client}px, so `
          + 'the reader does not see what the harvest found',
        ).toBeLessThanOrEqual(line.client)

        expect(await positions(), `nothing moved at ${label}`).toEqual(before)
        expect(
          await page.evaluate(() => document.documentElement.scrollHeight),
          `and the page did not grow at ${label}`,
        ).toBe(tall)

        /*
         * The counts are on the card, which is the only new thing on the trail: a step
         * that recorded facts draws its line as the control that opens one.
         */
        await step.locator('.outcome').click()
        const card = page.locator('.floating')
        await expect(card).toBeVisible()
        await expect(card).toContainText('Replies')
        await expect(card).toContainText('Threads resolved')
        await expect(card).toContainText('Code changed since')
        await expect(card).toContainText('3 findings')
        /* And the read time, where a finished step shows how long it took. */
        await expect(card.locator('.card-took')).toHaveText('read just now')
        await page.keyboard.press('Escape')

        /*
           **A step 7 with no facts still says when it was read, and reachably.** A
           harvest records a fact every time, so this is the hand-written case, which
           `layer set --clear-facts` is how to reach — without it the facts written above
           would stay, by the rule that giving none keeps them. Without a card the answer
           would live only in a `title`, which a keyboard cannot reach and a phone does
           not draw.
        */
        setStepSeven('running', 'nothing back yet', ['--clear-facts'])
        await page.reload()
        await expect(page.locator('h1')).toBeVisible()
        await page.waitForLoadState('networkidle')
        await expect(step.locator('.outcome')).toHaveText(/^nothing back yet/)
        /* Under a pointer as well as a click: the card is drawn on `opens`, and so is
           the hover that opens it. The review of the commit that broadened one and not
           the other is why this line is here. */
        await step.hover()
        await expect(page.locator('.floating .card-took')).toHaveText('read just now')
        await page.mouse.move(0, 0)
        await expect(page.locator('.floating')).toHaveCount(0)

        await step.locator('.outcome').click()
        await expect(page.locator('.floating .card-took')).toHaveText('read just now')
        await expect(page.locator('.floating .facts')).toHaveCount(0)
        expect(await positions(), `and that card moved nothing at ${label}`).toEqual(before)
        await page.keyboard.press('Escape')
        expect(await positions(), `the card moved nothing either at ${label}`).toEqual(before)
      }
    })
  })

  /**
   * **The one line on a row is one line, at a laptop's width and at a phone's.**
   *
   * DEV-919's fourth criterion. It cannot be checked from a board — a board is drawn at one
   * width — and it cannot be checked from a rendered tree either, because what holds the
   * line to one line is `white-space: nowrap` and an `overflow` that only a layout engine
   * resolves. So it is measured: the cell's height against one line of its own text.
   *
   * **Measured twice, and the second is the one that matters.** The second pass writes a
   * 52-word summary into the cell — the length the summaries on the review this was
   * measured against actually ran to, and longer than the writing rule now allows — and
   * measures again. That is a brief written *before* the 30-word cap, which is a record
   * this page has to survive and the contract's own example deliberately is not.
   */
  for (const [label, width, height] of [
    ['a laptop', 1440, 900],
    ['a phone', 390, 844],
  ] as const) {
    test(`a part’s one line stays one line at ${label}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      await page.goto(`${seeded}reviews/${structuredReviewId}/solution`)
      await expect(page.locator('h1')).toBeVisible()
      await expect(page.locator('.part').first()).toBeVisible()

      const lines = page.locator('.part .one-line')
      const count = await lines.count()
      expect(count, 'the example describes the change in several parts').toBeGreaterThan(3)

      for (let at = 0; at < count; at += 1) {
        const measured = await lines.nth(at).evaluate((element) => {
          const style = getComputedStyle(element)
          const one = Number.parseFloat(style.lineHeight) || Number.parseFloat(style.fontSize)
          return { height: element.getBoundingClientRect().height, one }
        })
        /*
         * **One line, whatever the text is.** Not "and it fits": a line longer than its
         * column *should* be clipped, and three of the seven are at this width — that the
         * clipping happens rather than the wrapping is the whole property. What it must
         * never do is become two lines, or widen the page to avoid it, and the page's own
         * width is measured by the no-horizontal-scroll test below.
         */
        expect(
          measured.height,
          `part ${at + 1}'s one line is ${measured.height}px against one line of `
          + `${measured.one}px`,
        ).toBeLessThanOrEqual(measured.one * 1.4)
      }

      /*
       * And with a summary at the length a session actually wrote one. Written into the
       * cell rather than seeded, because the seed comes from the contract's example and
       * that example is held to the 30-word rule — see the note above.
       */
      const first = lines.first()
      const spilled = await first.evaluate((element) => {
        element.textContent
          = 'Replaces the request-scoped slot with a context object constructed by whatever '
            + 'starts an export, so the scheduled path and the web path each carry the '
            + 'tenant they are acting for explicitly rather than reading it out of a shared '
            + 'surface that only one of the two of them ever writes to today.'
        const style = getComputedStyle(element)
        const one = Number.parseFloat(style.lineHeight) || Number.parseFloat(style.fontSize)
        return {
          height: element.getBoundingClientRect().height,
          one,
          clipped: element.scrollWidth > element.clientWidth,
          wide: document.documentElement.scrollWidth,
          client: document.documentElement.clientWidth,
        }
      })
      expect(
        spilled.height,
        `a 52-word summary draws ${spilled.height}px against one line of ${spilled.one}px`,
      ).toBeLessThanOrEqual(spilled.one * 1.4)
      expect(spilled.clipped, 'and it is clipped rather than drawn in full').toBe(true)
      /* And it did not widen the page to fit, which is the other way this fails. */
      expect(spilled.wide).toBeLessThanOrEqual(spilled.client)
    })
  }

  /**
   * **A row opens and closes by keyboard alone, and the files fold is reachable.**
   *
   * DEV-919's own criterion. Hover is welcome and is not enough: a reader on a keyboard has
   * no hover, and a reader on a phone has no pointer at all. `<summary>` is focusable and
   * opens on Enter without any script, which is why the fold is a `<details>`.
   */
  test('a part’s row opens and closes by keyboard, and so does its files fold', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(`${seeded}reviews/${structuredReviewId}/solution`)
    await expect(page.locator('h1')).toBeVisible()

    const row = page.locator('.part').first()
    await row.focus()
    await expect(row).toBeFocused()
    await expect(row).toHaveAttribute('aria-expanded', 'false')

    await page.keyboard.press('Enter')
    const opened = page.locator('#part-0')
    await expect(opened).toBeVisible()
    await expect(row).toHaveAttribute('aria-expanded', 'true')

    /* The fold inside it, reached and opened without a pointer. */
    const fold = opened.locator('summary')
    await fold.focus()
    await expect(opened.locator('details[open]')).toHaveCount(0)
    await page.keyboard.press('Enter')
    await expect(opened.locator('details[open]')).toHaveCount(1)

    /* And the row folds again from the keyboard. Space too, since it is a button. */
    await row.focus()
    await page.keyboard.press('Enter')
    await expect(page.locator('#part-0')).toHaveCount(0)
    await page.keyboard.press('Space')
    await expect(page.locator('#part-0')).toBeVisible()
  })

  /**
   * **One row is open at a time**, which is what keeps the table a table rather than a page
   * of unfolded detail.
   */
  test('opening a second row folds the first', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(`${seeded}reviews/${structuredReviewId}/solution`)
    await expect(page.locator('h1')).toBeVisible()

    await page.getByRole('button', { name: /The export context/ }).click()
    await expect(page.locator('#part-0')).toBeVisible()

    await page.getByRole('button', { name: /Allowed columns on the report definition/ }).click()
    await expect(page.locator('#part-2')).toBeVisible()
    await expect(page.locator('#part-0')).toHaveCount(0)
    await expect(page.locator('.opened')).toHaveCount(1)
  })

  /**
   * **Nothing scrolls sideways**, at the width of a laptop and at the width of a phone.
   *
   * The first acceptance criterion, and the one a board cannot be checked against: a board
   * is drawn at one width. Measured on the document rather than eyeballed, because a table
   * that overflows by eight pixels looks fine in a screenshot.
   */
  for (const [label, width, height] of [
    ['a laptop', 1440, 900],
    ['a phone', 390, 844],
  ] as const) {
    test(`no page of a review scrolls sideways at ${label}`, async ({ page }) => {
      await page.setViewportSize({ width, height })

      for (const step of ['understand', 'alternatives', 'solution', 'code-review', 'draft']) {
        await page.goto(`${seeded}reviews/${structuredReviewId}/${step}`)
        await expect(page.locator('h1')).toBeVisible()
        await page.waitForLoadState('networkidle')

        const overflow = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
        }))
        expect(
          overflow.scrollWidth,
          `${step} at ${width}px is ${overflow.scrollWidth}px wide`,
        ).toBeLessThanOrEqual(overflow.clientWidth)

        /*
         * And with every fold open, which is where the long file paths and the four-column
         * tables are. A page that fits closed and overflows open fits nothing.
         */
        const folds = page.locator('button[aria-expanded="false"]')
        for (let at = 0; at < await folds.count(); at += 1) {
          await folds.nth(0).click()
        }
        await page.waitForTimeout(100)

        const opened = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
        }))
        expect(
          opened.scrollWidth,
          `${step} at ${width}px with a fold open is ${opened.scrollWidth}px wide`,
        ).toBeLessThanOrEqual(opened.clientWidth)
      }
    })

    /*
     * **And the one thing on the code review page that is allowed to be wider than the
     * page — the code — is inside the one thing that scrolls.**
     *
     * DEV-878: "Only the code block scrolls sideways; the page never does." The test above
     * is the second half. This is the first, and the two together are what stop it being
     * satisfied by a page that simply has no code on it: a line of PHP is as long as it is,
     * nothing re-wraps it — these are the only copy of those lines there is — so there has
     * to be something wider than the viewport, and it has to be in the scroller.
     */
    test(`only the code block scrolls sideways at ${label}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)
      await expect(page.locator('h1')).toBeVisible()

      /* The blocker, which is the finding with the widest hunk. */
      await page.locator('.row').filter({ hasText: 'F9' }).first()
        .getByRole('button').first().click()
      await expect(page.locator('.code')).toBeVisible()

      const measured = await page.evaluate(() => {
        const viewport = document.documentElement.clientWidth
        const scroller = document.querySelector('.scroller')!
        return {
          viewport,
          page: document.documentElement.scrollWidth,
          codeScrolls: scroller.scrollWidth > scroller.clientWidth,
          /* Everything past the right edge that is not inside the one scroller. */
          strays: [...document.querySelectorAll('body *')]
            .filter(element => element.getBoundingClientRect().right > viewport + 1)
            .filter(element => !scroller.contains(element) && element !== scroller)
            .map(element => `${element.tagName.toLowerCase()}.${element.className}`),
        }
      })

      expect(measured.page, 'the page itself').toBeLessThanOrEqual(measured.viewport)
      expect(measured.codeScrolls, 'the code block scrolls').toBe(true)
      expect(measured.strays, 'nothing outside the scroller is past the edge').toEqual([])
    })

    /*
     * **The pinned comment stays put while the code scrolls**, which is the one thing this
     * page does that board A8 does not. On the board the comment is carried along with the
     * code and leaves the screen after about forty characters at this width.
     */
    test(`the inline comment stays pinned while the code scrolls at ${label}`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height })
      await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)
      await page.locator('.row').filter({ hasText: 'F9' }).first()
        .getByRole('button').first().click()
      await expect(page.locator('.note .card')).toBeVisible()

      const moved = await page.evaluate(async () => {
        const scroller = document.querySelector('.scroller') as HTMLElement
        const card = document.querySelector('.note .card')!
        const before = card.getBoundingClientRect().left
        scroller.scrollLeft = scroller.scrollWidth - scroller.clientWidth
        await new Promise(settle => requestAnimationFrame(() => settle(null)))
        return { before, after: card.getBoundingClientRect().left }
      })

      expect(Math.abs(moved.after - moved.before)).toBeLessThanOrEqual(1)
    })
  }

  /*
   * **Step 5 in the browser, deciding on a finding, end to end.**
   *
   * On the structured review, because a decision changes a finding's *block* of the draft
   * and this is the one seeded review whose draft is written in parts. The seeded review's
   * is one block of markdown — which is the other half of this, asserted below.
   *
   * The decision reaches the binary, the binary writes the finding, its block, the body
   * and the outcome in one transaction, and the page redraws from what came back. Nothing
   * on the command line can do any of it, which `crates/cli/tests/signoff.rs` is the other
   * half of.
   */
  test('a finding is decided on in the browser, and the record says the developer did', async ({
    page,
  }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)

    /* F10, a nit: cutting it takes it out of the review and off this list. */
    await page.locator('.row').filter({ hasText: 'F10' }).first()
      .getByRole('button').first().click()
    await expect(page.getByText('Your call')).toBeVisible()

    await page.getByRole('group', { name: /How to decide on F10/ })
      .getByRole('button', { name: 'Cut' })
      .click()

    await expect(page.getByText('will not be posted')).toBeVisible()
    await expect(page.locator('.row').filter({ hasText: 'F10' })).toHaveCount(0)

    /* And the record, read back through the binary rather than off the screen. */
    const after = cliJson(seededDatabase, [
      'review', 'show', '--review', String(structuredReviewId),
    ]) as unknown as {
      findings: Array<{ seq: number, disposition: string }>
      outcomes: Array<{ finding_seq: number, kind: string, actor: string }>
      draft: { body: string, lint: { passes: boolean | null } }
    }

    expect(after.findings.find(finding => finding.seq === 10)!.disposition).toBe('suppressed')
    const outcome = after.outcomes.find(one => one.finding_seq === 10 && one.kind === 'cut')
    expect(outcome, 'the decision left an outcome').toBeDefined()
    expect(outcome!.actor).toBe('developer')

    /* The body was recomposed without it, and the lint reads as not checked. */
    expect(after.draft.body).not.toContain('F10')
    expect(after.draft.lint.passes).toBeNull()
  })

  /*
   * **A signed-off draft refuses every decision, and says why.**
   *
   * On the seeded review, whose draft a test further up signed off in a browser: an
   * approved draft is what the review session posts under the developer's name, so a
   * decision that recomposed it underneath would change what gets posted. The controls are
   * not drawn at all rather than drawn and refused.
   *
   * The other refusal — a draft written as one block of markdown, which holds no block for
   * any finding — is covered in `test/code-review.nuxt.spec.ts` against a one-block
   * fixture. It cannot be covered here as well: this is the suite's only one-block draft
   * and by now it is approved, which is a different refusal and the one that wins.
   */
  test('a signed-off draft refuses every decision, and says why', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/code-review`)
    await page.locator('.row').first().getByRole('button').first().click()

    await expect(page.getByText('You have signed this draft off')).toBeVisible()
    await expect(
      page.locator('.call').getByRole('group', { name: /How to decide/ }),
    ).toHaveCount(0)
  })

  /**
   * A brief written the old way, on the page built for the new one.
   *
   * The seeded review's own brief is `problem`, `diagram` and `provenance` and nothing
   * else, which is what every brief recorded before 10 October 2026 holds. The criterion is
   * that it still shows: never a blank page and never an error.
   */
  test('a brief written before the structured one still shows', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/understand`)

    await expect(page.getByText('Written before the structured brief.')).toBeVisible()
    /* The seed's own wording, written by `brief put` before any of this ran. */
    await expect(
      page.getByText('A webhook delivery that fails once is never tried again'),
    ).toBeVisible()
    await expect(page.getByText('The problem, as recorded')).toBeVisible()
    /* Its diagram is the one thing on these pages that still goes to mermaid. */
    await expect(page.locator('figure')).toBeVisible()
    /* And none of the structured page is drawn, because there is nothing to draw. */
    await expect(page.locator('.flow')).toHaveCount(0)
  })

  /**
   * **A Before line holding six code names is one paragraph beside its dash.**
   *
   * DEV-939, measured. The `li` is a flex row so the en-dash sits beside the words, and
   * `TickedText` is `display: contents` so it adds no box — and with nothing between them
   * every word run and every code chip became a flex item of its own. A line with five or
   * six names drew as eight narrow columns with the names broken mid-word, `url.inte /
   * nded`, and the fixtures never caught it because their lines held at most one name.
   *
   * **It cannot be checked from a rendered tree.** The markup was never wrong: what was
   * wrong was which box laid the pieces out, so the only thing that can answer it is a
   * layout engine. Four things are measured, at a laptop's width and at a phone's:
   *
   * 1. the `li` has exactly one element in it, and every chip and every run of words is
   *    inside that one box;
   * 2. the box starts where the dash ends and reaches the column's far edge — the column
   *    less the dash, which is the criterion's own wording;
   * 3. no chip is narrower than its own text while the box still has room for it, which is
   *    what a flex item sized to its content was;
   * 4. no chip is taller than the lines its own text needs at that width.
   *
   * The sixth name is 46 characters, which is the case the fixtures were missing: long
   * enough that it has to break at a phone's width and must not break at a laptop's.
   */
  test('a detail line of six code names is one paragraph, at both widths', async ({ page }) => {
    /**
     * The widest line in problem 1's Before list, as the browser laid it out.
     *
     * The unbroken widths come from a clone with `white-space: nowrap`, appended inside the
     * `li` itself rather than to the body — the chip's size is `0.86em` of its parent, so a
     * probe anywhere else would be measured in the wrong font.
     */
    const measure = () => page.evaluate(() => {
      const lines = [...document.querySelectorAll('#problem-1 .lines li')]
      const li = lines.find(one => one.querySelectorAll('code').length >= 5)
      if (!li) return { found: false as const, lines: lines.length }

      const unbroken = (element: Element) => {
        const probe = element.cloneNode(true) as HTMLElement
        probe.style.cssText
          = 'position:absolute;visibility:hidden;white-space:nowrap;width:auto;max-width:none'
        li.append(probe)
        const width = probe.getBoundingClientRect().width
        probe.remove()
        return width
      }

      const wrappers = [...li.querySelectorAll('.line-text')]
      const wrapper = wrappers[0] as HTMLElement | undefined
      const box = li.getBoundingClientRect()
      const inner = wrapper?.getBoundingClientRect()

      /* Every run of words in the line, not only the chips. */
      const runs = [...li.childNodes].filter(node =>
        node.nodeType === Node.TEXT_NODE && node.textContent!.trim() !== '')

      /*
       * **The `li`'s flex items, which is not the same as its children.** An element at
       * `display: contents` is a child and is not an item: it generates no box, and what
       * the row lays out is its contents instead. Counting children here would read 1 on
       * the broken page as well as the fixed one — the markup was `<li><span
       * class="prose">` either way — and a message saying "laid out as 1 item" would print
       * over a row of thirteen. So the pieces are counted the way the row sees them.
       */
      const items = (parent: Element): string[] =>
        [...parent.childNodes].flatMap((node) => {
          if (node.nodeType === Node.TEXT_NODE) {
            return node.textContent!.trim() === '' ? [] : ['(text)']
          }
          if (node.nodeType !== Node.ELEMENT_NODE) return []
          const element = node as Element
          const name = `${element.tagName.toLowerCase()}${
            [...element.classList].map(c => `.${c}`).join('')}`
          return getComputedStyle(element).display === 'contents'
            ? items(element)
            : [name]
        })

      const laidOut = items(li)

      return {
        found: true as const,
        children: laidOut.length,
        childTags: laidOut,
        wrappers: wrappers.length,
        /*
         * **Whether the box exists, not whether the element does.** The element was always
         * there; what was missing was a box. A wrapper left at `display: contents` passes
         * every assertion that reads the tree and fails every one that measures it, so the
         * layout fact is asserted as a layout fact.
         */
        wrapperDisplay: wrapper && getComputedStyle(wrapper).display,
        /* Nothing of the line's text is a child of the `li` itself. */
        looseRuns: runs.length,
        proseInsideWrapper: [...li.querySelectorAll('.prose')]
          .every(prose => prose.parentElement === wrapper),
        chipsInsideWrapper: [...li.querySelectorAll('code')]
          .every(code => wrapper?.contains(code) ?? false),
        /* The dash is a `::before`, so it is measured as the gap it leaves in front. */
        dashAndGap: inner && Math.round(inner.left - box.left),
        dashWidth: Math.round(
          Number.parseFloat(getComputedStyle(li, '::before').width) || 0),
        gap: Math.round(Number.parseFloat(getComputedStyle(li).columnGap) || 0),
        liContent: Math.round(li.clientWidth),
        innerWidth: inner && Math.round(inner.width),
        innerRightGap: inner && Math.round(box.right - inner.right),
        /*
         * **The defect and the fix, measured against each other.** Taking the box away
         * again is exactly what the page did before this: the wrapper stops laying the
         * pieces out and every one of them becomes a flex item of the `li`. So the height
         * of the line without the box is the height of the bug, and no constant has to be
         * written down for the test to know what it is comparing against.
         */
        ...(() => {
          const asBuilt = li.getBoundingClientRect().height
          if (!wrapper) return { asBuilt, asColumns: asBuilt }
          wrapper.style.display = 'contents'
          const asColumns = li.getBoundingClientRect().height
          wrapper.style.removeProperty('display')
          return { asBuilt, asColumns }
        })(),
        chips: [...li.querySelectorAll('code')].map((code) => {
          const style = getComputedStyle(code)
          return {
            text: code.textContent!,
            width: code.getBoundingClientRect().width,
            height: code.getBoundingClientRect().height,
            clientWidth: code.clientWidth,
            scrollWidth: code.scrollWidth,
            lineHeight: Number.parseFloat(style.lineHeight)
              || Number.parseFloat(style.fontSize) * 1.2,
            /* The chip is a box with padding, so its height is its lines plus that. */
            padding: Number.parseFloat(style.paddingTop)
              + Number.parseFloat(style.paddingBottom),
            unbroken: Math.round(unbroken(code)),
          }
        }),
      }
    })

    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })
      await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
      await expect(page.locator('h1')).toBeVisible()
      await page.waitForLoadState('networkidle')

      await page.getByRole('button', { name: /A scheduled export read its tenant/ }).click()
      await expect(page.locator('#problem-1')).toBeVisible()

      const seen = await measure()
      expect(
        seen.found,
        `at ${width}px no line in problem 1 holds five code names; `
        + `the fixture is what this measures and it has ${'lines' in seen ? seen.lines : 0} lines`,
      ).toBe(true)
      if (!seen.found) return

      const at = `at ${width}px`

      /* ---- 1 · One box, and everything in it ---- */

      expect(
        seen.children,
        `${at} the line is laid out as ${seen.children} items: ${seen.childTags.join(', ')}`,
      ).toBe(1)
      expect(seen.wrappers, `${at} the line's text has one box`).toBe(1)
      expect(
        seen.wrapperDisplay,
        `${at} the line's box is \`display: ${seen.wrapperDisplay}\`, so it lays nothing out`,
      ).not.toBe('contents')
      expect(seen.looseRuns, `${at} runs of words outside that box`).toBe(0)
      expect(seen.proseInsideWrapper, `${at} every text run shares the one box`).toBe(true)
      expect(seen.chipsInsideWrapper, `${at} every code chip shares the one box`).toBe(true)

      /* ---- 2 · The column less the dash ---- */

      expect(
        seen.dashAndGap,
        `${at} the box starts ${seen.dashAndGap}px in, past a ${seen.dashWidth}px dash `
        + `and a ${seen.gap}px gap`,
      ).toBe(seen.dashWidth + seen.gap)
      expect(
        seen.innerRightGap,
        `${at} the box stops ${seen.innerRightGap}px short of the column's edge`,
      ).toBe(0)
      /*
       * **Within a pixel, because these are three separately rounded numbers.** The column
       * width is fractional — `.detail-pair` is `minmax(..., 1fr)` tracks with a gap — and
       * `clientWidth` is rounded by the DOM while the box's width and offset are rounded
       * here, so an exact comparison can disagree by one on a page that is laid out
       * correctly. The two assertions above pin both edges; this says the width in the
       * criterion's own terms.
       */
      expect(
        Math.abs(seen.innerWidth! - (seen.liContent - seen.dashAndGap!)),
        `${at} the box is ${seen.innerWidth}px of a ${seen.liContent}px column, `
        + `starting ${seen.dashAndGap}px in`,
      ).toBeLessThanOrEqual(1)

      /* ---- 3 · Six chips, one of them over 40 characters ---- */

      expect(seen.chips.length, `${at} the line's code chips`).toBeGreaterThanOrEqual(5)
      const longest = seen.chips.reduce((a, b) => (a.text.length >= b.text.length ? a : b))
      expect(
        longest.text.length,
        `${at} the longest name is ${longest.text.length} characters: ${longest.text}`,
      ).toBeGreaterThan(40)

      /* ---- 4 · No chip narrower than its text, none taller than it needs ---- */

      for (const chip of seen.chips) {
        /* Nothing of the chip's own text sits outside the chip. */
        expect(
          chip.scrollWidth,
          `${at} \`${chip.text}\` holds ${chip.scrollWidth}px of text in ${chip.clientWidth}px`,
        ).toBeLessThanOrEqual(chip.clientWidth + 1)

        /*
         * **The lines its width requires, and no more.** A chip whose unbroken text fits
         * the box does not break at all; one wider than the box breaks as many times as it
         * takes, and one more time than that is allowed because a chip starts wherever the
         * words before it left off rather than at the box's left edge. A chip that fits
         * gets no such allowance — not breaking a name that had room is the criterion.
         *
         * This is the assertion the broken layout failed, and it failed it at both widths:
         * a chip sized to its own flex item was five lines of one or two letters.
         */
        const needed = Math.max(1, Math.ceil(chip.unbroken / seen.innerWidth!))
        const allowed = needed === 1 ? 1 : needed + 1
        expect(
          chip.height,
          `${at} \`${chip.text}\` is ${Math.round(chip.height)}px tall — `
          + `${chip.unbroken}px of text in a ${seen.innerWidth}px box needs ${needed} line(s) `
          + `of ${Math.round(chip.lineHeight)}px, and ${allowed} is the most it may use`,
        ).toBeLessThanOrEqual(chip.lineHeight * allowed + chip.padding + 1)

        /*
         * **And no chip is narrower than its own text while the box has room for it**,
         * which is the second criterion and the thing the eye actually saw: a chip sized to
         * its own flex item was narrower than the name in it.
         */
        if (chip.unbroken <= seen.innerWidth!) {
          expect(
            Math.round(chip.width),
            `${at} \`${chip.text}\` is drawn ${Math.round(chip.width)}px wide `
            + `for ${chip.unbroken}px of text, in a box with ${seen.innerWidth}px`,
          ).toBeGreaterThanOrEqual(chip.unbroken - 1)
        }
      }

      /*
       * **One paragraph, and the proof that it is.** Putting the box back to
       * `display: contents` is the page as it was, so this compares the fix against the
       * defect rather than against a number somebody chose: the line was half again as
       * tall, because each chip was a column sized to its own content.
       */
      expect(
        seen.asColumns,
        `${at} the line is ${Math.round(seen.asBuilt)}px tall as one paragraph and `
        + `${Math.round(seen.asColumns)}px with every piece its own flex item — `
        + `if those are close, this test is no longer measuring the defect`,
      ).toBeGreaterThan(seen.asBuilt * 1.5)
    }

    /* Put the viewport back: every test below this one reads the same page. */
    await page.setViewportSize({ width: 1440, height: 900 })
  })

  /*
   * Where the queue's destinations are checked, and why not here.
   *
   * **This suite is a sequence, not a set.** By the time this block runs, the tests above
   * have confirmed the brief and signed off the draft through the browser, so there is no
   * row left with a step waiting — a test here asserting on "Problem brief ready" would be
   * asserting on a state three tests have already moved past. Both halves of the criterion
   * are pinned where the record is in the right state for them: Understand in "the queue
   * lists the review once", and the draft in "the agent's draft is what is left waiting to
   * be signed off".
   */
})

/*
 * ======================== The draft page's aside, and a diagram ========================
 *
 * **DEV-928's ninth criterion: "each of the above is measured in a real browser".** Every
 * one of these is something the unit suite cannot answer. Three of them are about a drawing
 * that jsdom cannot draw at all — `svg` is only ever set by a successful `mermaid.render`,
 * and jsdom has no `getBBox` — and the rest are about a panel at a width, which is a
 * measurement and not a render.
 */
/**
 * Open the fourth review's draft page, at a width, with the page settled.
 *
 * Nine tests below opened it with the same three lines — raised by review, and the literal
 * `1440, 900` went from three places in this file to fourteen. A change to how the page is
 * reached is one edit now rather than nine.
 */
async function draftPage(page: Page, width = 1440) {
  await page.setViewportSize({ width, height: width === 390 ? 844 : 900 })
  await page.goto(`${seeded}reviews/${heldBackReviewId}/draft`)
  await expect(page.locator('h1')).toBeVisible()
  await page.waitForLoadState('networkidle')
}

/*
 * **What GitHub receives, in a real browser, at both widths.**
 *
 * DEV-929. A real review was 309 lines with no index, no severity glyph and two inline
 * threads, because the posted body *was* the blocks joined. The seeded structured review is
 * the contract's real-length example — twelve posted findings, nine of them with a line
 * inside the diff — so this is the volume the complaint was about rather than a two-finding
 * draft that proves nothing about an index.
 *
 * The page never scrolling sideways is asserted for every step, at both widths, in "no page
 * of a review scrolls sideways"; this is what the two halves of the preview say.
 */
test.describe('the body is an index and the findings live on the code', () => {
  for (const [label, width, height] of [
    ['a laptop', 1440, 900],
    ['a phone', 390, 844],
  ] as const) {
    test(`the index and the threads both read at ${label}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
      await expect(page.locator('h1')).toBeVisible()

      /*
       * The body: the session's prose, then the index. The glyph and the word together, the
       * place in the code as a link, and where the finding stands — and the blocker is a
       * row rather than a section, because the diff can anchor it.
       */
      const body = page.locator('.preview')
      await expect(body.getByText('Open — needs a decision')).toBeVisible()
      await expect(body.getByRole('table')).toBeVisible()
      /*
       * **Counted against the record rather than against a number written here.** The tests
       * in this file share one database and some of them decide on a finding, so what the
       * index holds is "one row per posted finding" and not "twelve rows" — and the first of
       * those is the acceptance criterion anyway.
       */
      await expect(body.locator('tbody tr')).toHaveCount(postedFindings())
      await expect(body.getByRole('link', { name: 'ColumnPolicy.php:52' })).toBeVisible()
      await expect(body.getByText('🔴 Blocker · F9')).toBeVisible()
      await expect(
        body.getByRole('heading', { name: /Blocker · F9 · The column policy/ }),
      ).toHaveCount(0)

      /* And the diagram the session drew under `### Problem fit`, which is a prose block. */
      await expect(body.locator('figure svg')).toBeVisible()

      /*
       * The threads: one per anchored finding, each with the line it sits on, and the
       * comment as the author will read it.
       */
      /*
       * **Counted against the index beside it**, which is the stronger claim and the one
       * that does not go stale: every row saying `on the code` has a thread, and nothing
       * else does. The tests in this file share a database and some of them cut a finding,
       * so a number written here would be a number about whichever ran first.
       */
      const anchored = await body.locator('tbody tr', { hasText: 'on the code' }).count()
      expect(anchored, 'the example has findings the diff can anchor').toBeGreaterThan(7)

      const threads = page.locator('.threads')
      await expect(threads.getByText(`On the code · ${anchored} threads`)).toBeVisible()
      await expect(threads.locator('.thread')).toHaveCount(anchored)
      await expect(
        threads.getByText('app/Reporting/ColumnPolicy.php:52', { exact: true }),
      ).toBeVisible()
      await expect(threads.getByText('Blocker · F9', { exact: false }).first()).toBeVisible()

      /* It describes rather than grades: no severity tag anywhere in the card. */
      await expect(threads.locator('.tag')).toHaveCount(0)

      /*
       * **The table scrolls inside its own card, and the page does not.** GitHub gives a
       * table `width: max-content; max-width: 100%; overflow: auto`, so a four-column index
       * at 390 pixels is scrolled rather than reflowed — and the preview is drawn the same
       * way, which is what keeps it a preview.
       */
      const fits = await page.locator('.preview table').evaluate(element => ({
        own: element.scrollWidth > element.clientWidth,
        page: document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      }))
      expect(fits.page, `the page itself does not scroll at ${width}px`).toBe(true)
      if (width === 390) {
        expect(fits.own, 'at 390px the index is scrolled inside its card').toBe(true)
      }
    })
  }

  /*
   * **A cut takes the row and the thread together**, which is the acceptance criterion the
   * page exists to show: the developer decides on the code review page, and what posts
   * changes here.
   */
  test('cutting a finding takes its row and its thread', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
    /* Counted once the card has drawn: a count taken mid-load is zero and proves nothing. */
    await expect(page.locator('.threads .thread').first()).toBeVisible()
    const rowsBefore = await page.locator('.preview tbody tr').count()
    const threadsBefore = await page.locator('.threads .thread').count()
    expect(threadsBefore).toBeGreaterThan(0)

    await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)
    await page.locator('.row').filter({ hasText: 'F9' }).first()
      .getByRole('button').first().click()
    await page.getByRole('group', { name: /How to decide on F9/ })
      .getByRole('button', { name: 'Cut' })
      .click()
    await expect(page.getByText('will not be posted')).toBeVisible()

    await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
    await expect(page.locator('.threads').locator('.thread')).toHaveCount(threadsBefore - 1)
    await expect(page.locator('.preview').locator('tbody tr')).toHaveCount(rowsBefore - 1)
    await expect(page.locator('.preview').getByText('🔴 Blocker · F9')).toHaveCount(0)

    /*
     * Put it back, because every test in this file reads the same database — and the way
     * back is the page, not the command line: nothing there can decide about a finding.
     */
    await page.goto(`${seeded}reviews/${structuredReviewId}/code-review`)
    await page.getByRole('radio', { name: /Set aside/ }).click()
    await page.locator('.row').filter({ hasText: 'F9' }).first()
      .getByRole('button').first().click()
    await page.getByRole('group', { name: /How to decide on F9/ })
      .getByRole('button', { name: /Keep|Post it/ })
      .click()
    await expect(page.getByText('stays in the review')).toBeVisible()

    await page.goto(`${seeded}reviews/${structuredReviewId}/draft`)
    await expect(page.locator('.threads').locator('.thread')).toHaveCount(threadsBefore)
    await expect(page.locator('.preview').locator('tbody tr')).toHaveCount(rowsBefore)
  })
})

test.describe('the findings held back, as rows', () => {
  /**
   * The state the issue was raised from: eight rows in a column about 290 pixels wide.
   *
   * Closed, the panel carries a key, a severity word and a title, and **not one of the eight
   * claims**. That is the whole complaint — 41 to 63 words each, printed as paragraphs — so
   * it is asserted as an absence rather than as a layout.
   */
  test('eight rows, grouped and counted, with no claim until one is opened', async ({ page }) => {
    await draftPage(page)

    const panel = page.locator('.held')
    await expect(panel.getByText('Held back · 8 findings')).toBeVisible()
    await expect(panel.locator('.held-row')).toHaveCount(8)
    await expect(panel.locator('.held-group')).toHaveCount(2)
    await expect(panel.getByText('6 nits')).toBeVisible()
    await expect(panel.getByText('2 questions')).toBeVisible()

    /* Nothing is open, so there is no claim and no file anywhere in the panel. */
    await expect(panel.locator('.held-opened')).toHaveCount(0)
    await expect(panel.getByText('In the code')).toHaveCount(0)

    /* The severity is a word. No tag, and nothing coloured by how serious it is. */
    const first = panel.locator('.held-row').first()
    await expect(first.locator('.cell--severity')).toHaveText(/question|nit/)
    await expect(panel.locator('.tag')).toHaveCount(0)
  })

  /**
   * **A row opens in place, by click, to the claim and where it is in the code.**
   *
   * The Solution page's pattern, which DEV-928 asks for by name, so the assertion is the
   * same one DEV-919's rows get: the detail is the element the row's `aria-controls` names,
   * and it is under the row rather than somewhere else on the page.
   */
  test('a row opens in place to its claim and its file', async ({ page }) => {
    await draftPage(page)

    const row = page.locator('.held-row').filter({ hasText: 'F4' }).first()
    await expect(row).toHaveAttribute('aria-expanded', 'false')
    const id = await row.getAttribute('aria-controls')
    await row.click()

    await expect(row).toHaveAttribute('aria-expanded', 'true')
    const opened = page.locator(`#${id}`)
    await expect(opened).toBeVisible()
    await expect(opened.getByText('In the code')).toBeVisible()
    await expect(opened.getByText('app/Reporting/ColumnPolicy.php:44')).toBeVisible()
    /* 57 words, which is why they are not on the row. */
    await expect(opened.locator('.claim')).toContainText('is read twice in this class')

    /* And it is under the row that opened it, not elsewhere on the page. */
    const rowBox = (await row.boundingBox())!
    const openedBox = (await opened.boundingBox())!
    expect(openedBox.y).toBeGreaterThanOrEqual(rowBox.y + rowBox.height - 2)
  })

  /** One at a time, by keyboard alone. Enter and Space, because the row is a button. */
  test('a row opens and folds by keyboard, and only one is open', async ({ page }) => {
    await draftPage(page)

    const rows = page.locator('.held-row')
    await rows.first().focus()
    await expect(rows.first()).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.locator('.held-opened')).toHaveCount(1)
    await expect(rows.first()).toHaveAttribute('aria-expanded', 'true')

    await page.keyboard.press('Enter')
    await expect(page.locator('.held-opened')).toHaveCount(0)
    await page.keyboard.press('Space')
    await expect(page.locator('.held-opened')).toHaveCount(1)

    /* A second row, and the first folds rather than both standing open. */
    await rows.nth(4).click()
    await expect(page.locator('.held-opened')).toHaveCount(1)
    await expect(rows.first()).toHaveAttribute('aria-expanded', 'false')
    await expect(rows.nth(4)).toHaveAttribute('aria-expanded', 'true')
  })

  /**
   * **A title the page cut says so, and a backtick pair is drawn as code.**
   *
   * Two of the issue's criteria on one panel: F7 has no title in the record, so its row
   * stands in with the claim's first twelve words; F4's title carries
   * `report_definitions.allowed_columns`, which is drawn as a code chip and never as
   * backticks — backticks printed literally is the defect this is measured against.
   */
  test('a shortened line says so, and a code name is drawn as code', async ({ page }) => {
    await draftPage(page)

    const panel = page.locator('.held')
    await expect(panel.getByText('`')).toHaveCount(0)
    await expect(panel.locator('code').first()).toHaveText('report_definitions.allowed_columns')

    const cut = panel.locator('.held-row').filter({ hasText: 'shortened' })
    await expect(cut).toHaveCount(1)
    await expect(cut.locator('.cell--line')).toContainText('…')
    await cut.click()
    await expect(page.locator('.held-opened')).toContainText('No title was written')
    await expect(page.locator('.held-opened')).toContainText('first 12 words')
  })

  /**
   * **Nothing leaves the card, at a laptop's width and at a phone's.**
   *
   * The criterion names both widths, and it is measured on the elements rather than on the
   * document: the page-level check further up this file would pass on a panel whose own
   * contents overflowed inside a card that clipped them. A code name wider than its column
   * is the defect, so the column is what is measured — closed and with a row open, since a
   * panel that fits closed and overflows open fits nothing.
   */
  /* The height goes with the width, inside `draftPage`: a phone is 844 and a laptop 900. */
  for (const [label, width] of [['a laptop', 1440], ['a phone', 390]] as const) {
    test(`no held-back text leaves the card at ${label}`, async ({ page }) => {
      await draftPage(page, width)

      /*
       * **Measured against the card, not against the panel inside it.** The rows carry a
       * negative margin so the rule above each one reaches the card's own edges — a row
       * inset by 16px on both sides in a 290px column loses an eighth of the width its line
       * has to live in — so a child of the panel legitimately reaches 16px past the panel's
       * content box. The card's border box is the edge the criterion is about, and the first
       * version of this assertion used the wrong one and failed on a correct layout.
       */
      const measure = async (where: string) => {
        const panel = page.locator('.held')
        const sizes = await panel.evaluate((element) => {
          const card = element.closest('.panel') ?? element
          const box = card.getBoundingClientRect()
          /*
           * **The visually-hidden names are left out, and they are the reason this reads
           * the way it does.** `.rd-visually-hidden` is a one-pixel box holding its whole
           * string behind `overflow: hidden`, which is what makes it hidden — so every one
           * of them reports 512 pixels of content in one pixel of box, and the first version
           * of this assertion called each a defect. They are the only elements on the page
           * for which overflowing is the point.
           */
          return [...element.querySelectorAll('*')]
            .filter(child => child.closest('.rd-visually-hidden') == null)
            .map(child => ({
              right: child.getBoundingClientRect().right,
              own: child.scrollWidth > child.clientWidth + 1,
              text: `${child.tagName.toLowerCase()}.${child.className} `
                + `[${child.scrollWidth}/${child.clientWidth}] `
                + (child.textContent ?? '').slice(0, 30),
            }))
            .concat([{ right: box.right, own: false, text: 'the card' }])
        })
        const edge = sizes.at(-1)!.right
        for (const child of sizes) {
          expect(
            child.right,
            `${where} at ${width}px: "${child.text}" reaches ${child.right} past ${edge}`,
          ).toBeLessThanOrEqual(edge + 1)
          expect(child.own, `${where} at ${width}px: "${child.text}" scrolls inside itself`)
            .toBe(false)
        }
      }

      await measure('closed')
      /* The 61-word claim and the longest path, which is the widest the panel ever is. */
      await page.locator('.held-row').filter({ hasText: 'F6' }).first().click()
      await expect(page.locator('.held-opened')).toHaveCount(1)
      await measure('with a row open')
    })
  }

  /**
   * **Five edits on one finding is one line.**
   *
   * The third defect, on the record that produced it: `kept`, `kept`, `kept`, `cut`, `kept`
   * inside twelve seconds, four of them with no detail — so `wire::draft_edit` wrote the
   * kind into `summary` and the panel printed "Kept F1 · kept" four times over.
   */
  test('a finding toggled five times is one line, with the verb once', async ({ page }) => {
    await draftPage(page)

    const lines = page.locator('.edit')
    await expect(lines).toHaveCount(1)
    await expect(lines.first()).toContainText('Kept F1')
    await expect(lines.first()).toContainText('changed 5 times')
    /* The verb once: "Kept F1 · kept" is what this replaced. */
    await expect(lines.first()).not.toContainText('· kept')
  })
})

test.describe('a drawing opens larger', () => {
  /**
   * **The draft preview's diagram**, which is a ```` ```mermaid ```` fence in the body, and
   * the first of the two places the criterion names.
   *
   * Opened by click, zoomed, dragged, and closed on Escape with the focus back where it
   * was. Every one of these is unreachable in jsdom: `svg` is only ever set by a successful
   * render and jsdom cannot draw.
   */
  test('the diagram in the draft preview opens, zooms, drags and closes', async ({ page }) => {
    await draftPage(page)

    const canvas = page.locator('button.canvas').first()
    await expect(canvas).toBeVisible()
    await expect(canvas.locator('svg')).toHaveCount(1)
    await expect(canvas).toHaveAttribute('aria-haspopup', 'dialog')

    await canvas.click()
    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()

    /*
     * **The same element, not a copy.** One drawing in the document is the property the
     * whole design turns on: a copy would need a second `v-html`, and mermaid scopes its own
     * stylesheet by the figure's id so a clone either renders unstyled or duplicates every
     * id on the page.
     */
    await expect(page.locator('svg')).toHaveCount(1)
    await expect(sheet.locator('svg')).toHaveCount(1)

    /* It fills the window, rather than the 560-pixel pane it came out of. */
    const box = (await sheet.boundingBox())!
    expect(box.width).toBeGreaterThan(1440 * 0.8)

    /*
      * Zoomed: the control says so, and the transform changes.
      *
      * Read off `.reading` rather than by text. mermaid writes its own stylesheet into the
      * SVG and that stylesheet contains `100%`, so `getByText('100%')` matches two elements
      * and fails on the ambiguity rather than on the page.
      */
    await expect(sheet.locator('.reading')).toContainText('100%')
    const before = await sheet.locator('.moved').getAttribute('style')
    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await expect(sheet.locator('.reading')).toContainText('130%')
    expect(await sheet.locator('.moved').getAttribute('style')).not.toBe(before)

    /*
     * **Zoomed again before the drag, and that is not padding.** The seeded diagram is 936
     * pixels wide in a stage about 1392 wide, so at 130% it is *still* narrower than the
     * window it was opened into — and `panLimit` is nought for a drawing that fits, which
     * is the rule that stops a reader shoving a small diagram off the side and being left
     * looking at the backdrop. A drag at 130% correctly moves nothing. The first version of
     * this test dragged there and read the nought as a defect.
     */
    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await expect(sheet.locator('.reading')).toContainText('169%')

    /* Dragged: the offset moves, and it is the drag that moved it. */
    const stage = sheet.locator('.stage')
    const area = (await stage.boundingBox())!
    const centre = { x: area.x + area.width / 2, y: area.y + area.height / 2 }
    await page.mouse.move(centre.x, centre.y)
    await page.mouse.down()
    await page.mouse.move(centre.x - 120, centre.y - 60, { steps: 8 })
    await page.mouse.up()
    const dragged = await sheet.locator('.moved').getAttribute('style')
    expect(dragged).toContain('translate(')
    expect(dragged).not.toContain('translate(0px, 0px)')

    /* Escape closes it, and the focus comes back to the drawing it came from. */
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(canvas).toBeFocused()
    await expect(canvas.locator('svg')).toHaveCount(1)
  })

  /** By keyboard alone, which is the half a pointer test cannot show. */
  test('it opens by keyboard, pans by arrow key, and closes on Escape', async ({ page }) => {
    await draftPage(page)

    const canvas = page.locator('button.canvas').first()
    await canvas.focus()
    await expect(canvas).toBeFocused()
    await page.keyboard.press('Enter')

    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()
    /* Focus lands on the stage, so the arrow keys work without a Tab first. */
    await expect(sheet.locator('.stage')).toBeFocused()

    /* Zoomed in first: a drawing that fits has nowhere to pan to, which is the clamp. */
    await page.keyboard.press('+')
    await page.keyboard.press('+')
    await expect(sheet.locator('.reading')).toContainText('169%')
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('ArrowRight')
    const panned = await sheet.locator('.moved').getAttribute('style')
    expect(panned).toContain('translate(-')

    /*
     * **Tab stays inside the dialog.** `aria-modal="true"` tells a screen reader the rest
     * of the page is not there, so a Tab that walked out of it would leave a reader moving
     * around a page they have been told is inert.
     */
    const inside = () => page.evaluate(
      () => document.activeElement?.closest('[role="dialog"]') != null,
    )
    for (let press = 0; press < 8; press += 1) {
      await page.keyboard.press('Tab')
      expect(await inside(), `after ${press + 1} presses of Tab`).toBe(true)
    }
    await page.keyboard.press('Shift+Tab')
    expect(await inside()).toBe(true)

    /*
     * **And the keys still answer with focus on a button**, which they did not when they
     * were bound to the stage: clicking a zoom control moves focus to it, and a reader who
     * zoomed with the mouse and then reached for the keyboard found nothing there.
     */
    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await expect(sheet.getByRole('button', { name: 'Zoom in' })).toBeFocused()
    await page.keyboard.press('-')
    await expect(sheet.locator('.reading')).toContainText('169%')

    /*
     * **One step back, not a fixed place.** The view is already at its pan limit from the
     * two ArrowRights above, so ArrowLeft moves it by one `PAN_STEP` towards the middle —
     * which is what is asserted. The first version of this expected an absolute offset and
     * failed on correct behaviour.
     */
    const offset = async () => Number(
      /translate\((-?[\d.]+)px/.exec(await sheet.locator('.moved').getAttribute('style') ?? '')?.[1],
    )
    const atLimit = await offset()
    await page.keyboard.press('ArrowLeft')
    expect(await offset()).toBeCloseTo(atLimit + 48, 1)

    /* Fit puts it back, and then Escape leaves. */
    await sheet.getByRole('button', { name: 'Fit' }).click()
    await expect(sheet.locator('.reading')).toContainText('100%')
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(canvas).toBeFocused()
  })

  /**
   * **A click on the dialog's own chrome leaves the focus in the dialog.**
   *
   * Review raised this as an escape from the trap, and measuring it says something more
   * exact. Before the fix, a click on the bar's text — which nothing can focus — dropped
   * `document.activeElement` to `<body>`, outside a dialog marked `aria-modal="true"`.
   * What review predicted next, that Tab would then walk into the page behind, **does not
   * happen in Chromium**: a click also sets the sequential-navigation starting point, so
   * Tab resumes from the thing that was clicked and comes back into the dialog. Measured,
   * not assumed — the first version of this test asserted the escape and passed against the
   * unfixed code, which is a test that proves nothing.
   *
   * So what is asserted is the part that is really different: `tabindex="-1"` on the sheet
   * means the click lands on the dialog rather than on the body. The starting point is a
   * Chromium behaviour this component should not be resting on in Firefox or Safari, which
   * is why the trap also acts from outside the sheet — that part has no browser here that
   * can show it, and it is listed as untested rather than claimed.
   */
  test('a click on the dialog’s own chrome keeps the focus in it', async ({ page }) => {
    await draftPage(page)
    await page.locator('button.canvas').first().click()
    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()

    const inside = () => page.evaluate(
      () => document.activeElement?.closest('[role="dialog"]') != null,
    )

    await sheet.locator('.reading').click()
    expect(await inside(), 'after a click on the bar’s own text').toBe(true)

    await page.keyboard.press('Tab')
    expect(await inside(), 'and after Tab from there').toBe(true)

    /*
     * **And a control that disables itself leaves the keyboard working**, which is a
     * different claim from where the focus is — and the right one. Focus cannot stay on a
     * disabled element, and on Linux Chromium it drops to `<body>`; on macOS it does not.
     * CI found that, after a local run of the first version of this passed. So what is
     * asserted is the thing that matters either way: every key the view answers is heard on
     * the window, so Tab, the arrows and `+` all still work from wherever focus landed.
     */
    await sheet.getByRole('button', { name: 'Zoom in' }).click()
    await sheet.getByRole('button', { name: 'Fit' }).click()
    await expect(sheet.getByRole('button', { name: 'Fit' })).toBeDisabled()

    await page.keyboard.press('Tab')
    expect(await inside(), 'Tab, after a control disabled itself').toBe(true)

    await page.keyboard.press('+')
    await expect(sheet.locator('.reading')).toContainText('130%')
    await page.keyboard.press('+')
    await expect(sheet.locator('.reading')).toContainText('169%')
    await page.keyboard.press('ArrowRight')
    expect(
      await sheet.locator('.moved').getAttribute('style'),
      'the arrow keys, after a control disabled itself',
    ).toContain('translate(-')

    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
  })

  /** A click outside closes it, and a click on the drawing does not. */
  test('a click outside closes it, and one inside does not', async ({ page }) => {
    await draftPage(page)

    await page.locator('button.canvas').first().click()
    await expect(page.getByRole('dialog')).toBeVisible()

    /* Inside the sheet: still open. */
    await page.locator('.stage').click({ position: { x: 20, y: 20 } })
    await expect(page.getByRole('dialog')).toBeVisible()

    /* The backdrop, which is the 24 pixels of gutter around the sheet. */
    await page.locator('.over').click({ position: { x: 4, y: 4 } })
    await expect(page.getByRole('dialog')).toHaveCount(0)
  })

  /**
   * **The second of the two places the criterion names: a diagram on the Understand page.**
   *
   * The whole reason this lives in `MermaidFigure` rather than on a page. The seeded
   * review's brief is an old-style one, which is the only kind that carries a `diagram`
   * field, and that field is handed to the same component.
   */
  test('the diagram on the Understand page opens larger too', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()
    await page.waitForLoadState('networkidle')

    const canvas = page.locator('button.canvas').first()
    await expect(canvas).toBeVisible()
    await canvas.click()
    await expect(page.getByRole('dialog')).toBeVisible()
    await expect(page.getByRole('dialog').locator('svg')).toHaveCount(1)
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(canvas).toBeFocused()
  })

  /**
   * **At a phone's width it takes the whole screen**, which is what the gutter is dropped
   * for: 24 pixels of backdrop on each side of a 390-pixel screen is a third of the room the
   * diagram was opened to get. Measured because the panel is measured at both widths and
   * this was only ever measured at one.
   */
  test('it fills a phone’s screen, and still closes', async ({ page }) => {
    await draftPage(page, 390)

    await page.locator('button.canvas').first().click()
    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()

    const box = (await sheet.boundingBox())!
    expect(box.width).toBe(390)
    expect(box.x).toBe(0)
    /* And the drawing is inside it rather than over the page. */
    const drawing = (await sheet.locator('svg').boundingBox())!
    expect(drawing.width).toBeLessThanOrEqual(390)

    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
  })

  /**
   * **A source shown as written offers no larger view.**
   *
   * The waived review's `diagram` field holds a flow a session drew by hand in text — the
   * real one from DEV-880. There is nothing to enlarge, and a control over a block of text
   * promising a larger view of it would be a lie the page tells.
   */
  test('a sketch offers nothing to enlarge', async ({ page }) => {
    await page.goto(`${seeded}reviews/${waivedReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()
    await page.waitForLoadState('networkidle')

    await expect(page.locator('pre.written')).toBeVisible()
    await expect(page.locator('button.canvas')).toHaveCount(0)
    await expect(page.getByText('Opens larger')).toHaveCount(0)
  })

  /**
   * **The page behind it does not move**, which is the cost of moving the one drawing into
   * the overlay rather than copying it: the figure holds the height its drawing had, so a
   * reader comes back to the scroll position they left.
   */
  test('the page behind it neither jumps nor scrolls', async ({ page }) => {
    await draftPage(page)

    const canvas = page.locator('button.canvas').first()
    /*
     * **Scrolled into view first, and measured in the page's own coordinates.** Clicking
     * scrolls the control into view, so a viewport-relative box read before the click and
     * again after it is comparing two different scroll positions — which is what the first
     * version of this test did, and it failed on a page that had not moved at all.
     */
    await canvas.scrollIntoViewIfNeeded()
    const place = () => page.evaluate(() => {
      const element = document.querySelector('button.canvas')!
      return {
        top: Math.round(element.getBoundingClientRect().top + window.scrollY),
        height: Math.round(element.getBoundingClientRect().height),
        scrolled: Math.round(window.scrollY),
        page: document.documentElement.scrollHeight,
      }
    })
    const before = await place()

    await canvas.click()
    await expect(page.getByRole('dialog')).toBeVisible()
    /* The figure is still the size it was, with its drawing elsewhere. */
    const holding = await place()
    expect(Math.abs(holding.height - before.height)).toBeLessThanOrEqual(2)
    expect(holding.page).toBe(before.page)
    expect(holding.top).toBe(before.top)

    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    const after = await place()
    expect(after.top).toBe(before.top)
    expect(after.scrolled).toBe(before.scrolled)
    expect(after.page).toBe(before.page)
  })

  /**
   * **Nothing is left over the page after it closes**, and nothing is left behind when the
   * page it was on goes away. DEV-880's own failure in a new costume: the overlay is
   * teleported to the body, so a leak is a sheet nobody can close sitting on every screen
   * the developer opens afterwards.
   */
  test('it leaves nothing over the page when the page goes away open', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    /* A page to come back to, so leaving is a navigation rather than a fresh load. */
    await page.goto(`${seeded}reviews/${heldBackReviewId}/code-review`)
    await expect(page.locator('h1')).toBeVisible()
    await page.goto(`${seeded}reviews/${heldBackReviewId}/draft`)
    await expect(page.locator('h1')).toBeVisible()
    await page.waitForLoadState('networkidle')

    await page.locator('button.canvas').first().click()
    await expect(page.getByRole('dialog')).toBeVisible()

    /*
     * **It covers the page, which is what `aria-modal` promises.** Measured rather than
     * assumed: the trail's links are still painted behind a translucent backdrop, so
     * "visible" says nothing — what matters is that a press over one of them lands on the
     * backdrop. It is also why leaving is a history navigation below and not a click: the
     * first version of this test tried to click a link under the backdrop and waited a
     * minute for a click that correctly never landed.
     */
    const covered = await page.evaluate(() => {
      const points: Array<[number, number]> = [
        [4, 4],
        [window.innerWidth / 2, window.innerHeight / 2],
        [window.innerWidth - 4, window.innerHeight - 4],
      ]
      return points.map(([x, y]) => document.elementFromPoint(x, y)?.closest('.over') != null)
    })
    expect(covered, 'every corner and the middle of the window belong to the overlay')
      .toEqual([true, true, true])

    /* Away by the browser's own Back, with the view still open. */
    await page.goBack()
    await expect(page).toHaveURL(`${seeded}reviews/${heldBackReviewId}/code-review`)
    await expect(page.locator('.over')).toHaveCount(0)
    await expect(page.getByRole('dialog')).toHaveCount(0)
    /* And the page is usable: the body scroll was given back. */
    expect(await page.evaluate(() => document.body.style.overflow)).not.toBe('hidden')
    await expect(page.locator('h1')).toBeVisible()
  })
})

/*
 * The Reviews list, and the two clicks DEV-930 asks for.
 *
 * Its own group because it is the one screen that is not about a review needing something:
 * the queue is what is waiting and `GET /timings` is what has finished, so a review that
 * was posted and has not been replied to was in neither. "The actual review itself has
 * disappeared… I can't even get to the review page if I wanted to go back" — Patrick, on a
 * review, which is the sentence this page deletes.
 */
test.describe('the list of every review', () => {
  test('is in the header, lists every review, and reaches a posted one in two clicks', async ({
    page,
  }) => {
    await page.goto(seeded)

    /*
     * **Click one.** The header item pointed at whichever review happened to be waiting
     * until DEV-930, so it could not take a reader to a *particular* review — and greyed
     * itself out on a database with none.
     */
    await page.getByRole('navigation', { name: 'Review Desk' })
      .getByRole('link', { name: 'Reviews' })
      .click()
    await expect(page).toHaveURL(/\/reviews$/)
    await expect(page.getByRole('heading', { name: /\d+ reviews?$/ })).toBeVisible()

    /* Every review the seed made, posted ones included. */
    for (const title of [
      'Retry failed webhook deliveries',
      'Give a scheduled export its own context',
      'Say "not measured" rather than null in the delivery log',
      'Record the delivery attempt count',
    ]) {
      await expect(page.getByText(title, { exact: false }).first()).toBeVisible()
    }

    /*
     * **The step, and its state.** Never the name alone: a posted review's furthest
     * started step is the draft, and a cell reading "Draft" would say it still needs
     * signing off while the queue says it is with the author.
     */
    /*
     * **The line, not the anchor inside it.** DEV-957 moved the repository line out of the
     * row's own link so that the pull request and the ticket could be anchors inline in it,
     * and "posted" and "round 1 of 3" are on that line. `.queue-line` is the whole row; the
     * `.row` inside it is where it goes.
     */
    const posted = page.locator('.queue-line').filter({ hasText: 'Record the delivery attempt' })
    /*
     * **"Not started", and "posted" beside it, which is not a contradiction.** The seven
     * steps are the review process; this review's GitHub id was recorded by `review link`
     * and no step of it was ever reported. Without the second word the step column would
     * read as a review nothing had happened to, beside a queue calling it posted.
     */
    await expect(posted).toContainText('Not started')
    await expect(posted).toContainText('posted')
    await expect(posted).not.toContainText('step 0 of 7')

    const webhooks = page.locator('.queue-line').filter({ hasText: 'Retry failed webhook' })
    await expect(webhooks).toContainText('Draft')
    await expect(webhooks).toContainText('step 6 of 7')
    /* A round is one of a total, never a bare count of rework against the author. */
    await expect(webhooks).toContainText('round 1')

    /* And it describes rather than grades: no bar, no findings count, no severities. */
    await expect(page.locator('[role="img"]')).toHaveCount(0)
    await expect(page.getByText('blocker', { exact: false })).toHaveCount(0)

    /* **Click two**, and the review is open at Understand, where a review starts. */
    await posted.locator('.row').click()
    await expect(page).toHaveURL(new RegExp(`/reviews/${postedReviewId}/understand$`))
    await expect(page.getByText('Record the delivery attempt count')).toBeVisible()
  })

  test('filters by repository, and says the order out loud', async ({ page }) => {
    await page.goto(`${seeded}reviews`)
    await expect(page.getByText('Newest first', { exact: false })).toBeVisible()

    const billing = page
      .getByRole('radio')
      .filter({ hasText: 'acme/billing-api' })
    /* Every seeded review is on acme/portal, so there is no second repository to pick. */
    await expect(billing).toHaveCount(0)
    const all = page.getByRole('radio').filter({ hasText: 'All repositories' })
    await expect(all).toHaveCount(1)

    const portal = page.getByRole('radio').filter({ hasText: 'acme/portal' })
    await expect(portal).toHaveCount(1)
    await portal.click()
    await expect(page.locator('.row').first()).toBeVisible()
    await expect(page.getByText('acme/portal', { exact: false }).first()).toBeVisible()
  })

  /*
   * **Measured, at a laptop's width and at a phone's**, which DEV-930's last criterion
   * asks for by name. Nothing scrolls sideways at either, on either page — the failure a
   * screenshot cannot show and a click count cannot see.
   */
  for (const [what, width, height] of [
    ['a laptop', 1440, 900],
    ['a phone', 390, 844],
  ] as Array<[string, number, number]>) {
    test(`the queue and the reviews fit ${what} at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height })

      for (const [name, path] of [['the queue', ''], ['the reviews', 'reviews']]) {
        await page.goto(`${seeded}${path}`)
        await expect(page.locator('.row').first()).toBeVisible()

        const sideways = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
        }))
        expect(
          sideways.scrollWidth,
          `${name} at ${width}px is ${sideways.scrollWidth}px wide in ${sideways.clientWidth}px`,
        ).toBeLessThanOrEqual(sideways.clientWidth + 1)

        /*
         * And every row is a target a finger can hit. 44px is this project's own floor for
         * anything you press, stated in `web/app/assets/css/tokens.css`.
         */
        const rows = await page.locator('.row').all()
        for (const row of rows) {
          const box = await row.boundingBox()
          expect(box, `${name} at ${width}px: a row has no box`).not.toBeNull()
          expect(
            box!.height,
            `${name} at ${width}px: a row is ${box!.height}px tall`,
          ).toBeGreaterThanOrEqual(44)
        }
      }
    })
  }
})

test.describe('a harvested review, on the queue and in the list', () => {
  /**
   * **DEV-933's last two checks, and both are about a state that did not exist before
   * it.** A harvest writes step 7: `running` while the pull request is open, `done` once
   * it has settled. The queue groups on a running step and the list prints the furthest
   * started one, so both screens had to be looked at with a review in that state.
   *
   * The seed's posted review is the one used, written through `layer set` with the words
   * and the pairs `record_harvest_step` produces, and put back in a hook — a `finally`
   * does not run when Playwright abandons a test at its timeout, and every other test
   * here reads the same database.
   */
  const setStepSeven = (state: string, detail: string, aboutTheFacts: string[]) =>
    cli(seededDatabase, [
      'layer',
      'set',
      '--review',
      String(postedReviewId),
      '--layer',
      '7',
      '--state',
      state,
      '--detail',
      detail,
      ...aboutTheFacts,
    ])

  test.afterEach(() => {
    setStepSeven('pending', 'posted, waiting for the harvest', ['--clear-facts'])
  })

  test('is on the queue exactly once, and never with the agent', async ({ page }) => {
    setStepSeven('running', 'nothing back yet', ['--fact', 'Pull request=open'])
    await page.goto(seeded)
    await expect(page.locator('.row').first()).toBeVisible()
    await page.waitForLoadState('networkidle')

    /*
     * **Once across the whole page**, which is the claim: the three groups a posted
     * review can fall into are "Needs you", "With the agent" and "Waiting on the author",
     * and a review drawn in two of them is a developer counting the same work twice.
     */
    const rows = page.locator('.row').filter({ hasText: 'Record the delivery attempt' })
    await expect(rows).toHaveCount(1)

    /*
     * **And no "With the agent" group at all**, which is the load-bearing half: nothing
     * else in this seed has a step running, so that section exists only if step 7 leaked
     * into it — and the row above proves the review is on the page, so its absence here
     * is the exclusion working rather than the review having gone missing. Taking the
     * exclusion out of `reviews_in_progress` fails this line.
     *
     * `getByLabel` for the bar, not `getByText`: "Layer 7 of 7" is a `SegmentBar`'s
     * `aria-label` and never text, so a text matcher there could not fail. **Nor could
     * the first `getByLabel` I wrote**, anchored at the seven — the label carries the
     * step's name after it, "Layer 7 of 7, harvest", so the pattern matched nothing on
     * any page. Found by the review of the commit that was fixing the same mistake.
     *
     * It names step 7 rather than any bar, which is what this test is about. Both lines
     * rest on nothing else in this seed having a step running; a seed that adds one has
     * to revisit them together.
     */
    await expect(page.getByText('With the agent')).toHaveCount(0)
    await expect(page.getByLabel(/^Layer 7 of 7, /)).toHaveCount(0)
    await expect(page.getByText('a session is on step 7 of 7')).toHaveCount(0)
  })

  for (const [what, state, detail, facts] of [
    ['running', 'running', 'nothing back yet', ['--fact', 'Pull request=open']],
    ['done', 'done', 'merged, 2 replies back', ['--fact', 'Pull request=merged', '--fact', 'Replies=2']],
  ] as const) {
    test(`the list reads sensibly for step 7 ${what}`, async ({ page }) => {
      setStepSeven(state, detail, [...facts])
      await page.goto(`${seeded}reviews`)
      await expect(page.getByRole('heading', { name: /\d+ reviews?$/ })).toBeVisible()
      await page.waitForLoadState('networkidle')

      const row = page.locator('.row').filter({ hasText: 'Record the delivery attempt' })
      await expect(row).toHaveCount(1)
      /*
       * The step, its own words, and the number of seven — never the bare state word,
       * which on step 7 means "the pull request is open and still being watched" and
       * would read as a harvest executing for as long as a pull request stays open.
       */
      await expect(row).toContainText(`What happened · ${detail}`)
      await expect(row).toContainText('step 7 of 7')
      await expect(row).not.toContainText('· running')
    })
  }
})

test.describe('a database with nothing in it', () => {
  test('all five pages read as new rather than as broken', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await expect(page.getByText('This database holds no reviews')).toBeVisible()
    /* The one thing that fills it, with the field to do it from. */
    await expect(page.locator('input#start-empty')).toBeVisible()

    /*
     * **And the header's Reviews item is a link here**, which it was not before DEV-930:
     * it pointed at whichever review was waiting and greyed itself out when none was. A
     * list is somewhere worth being on an empty database, because it says so.
     */
    await page.getByRole('navigation', { name: 'Review Desk' })
      .getByRole('link', { name: 'Reviews' })
      .click()
    await expect(page).toHaveURL(`${empty}reviews`)
    /*
     * **"No reviews in play", not "this database holds no reviews"** — which is the
     * stronger claim and one this page cannot make, because the answer it is drawn from
     * is the one that leaves dismissed reviews out. DEV-945. The card offers the look
     * instead, and that control is the only way to the dismissed view when this card is
     * drawn: it replaces the default slot, pickers and all.
     */
    await expect(page.getByRole('heading', { name: 'No reviews in play' })).toBeVisible()
    await expect(page.getByText('No review is waiting, running or finished here'))
      .toBeVisible()
    /*
     * **The card's way out goes to Everything since DEV-957.** There are two ways off the
     * desk now, and the answer this card is drawn from leaves both out — so it cannot say
     * which this database's reviews took, and sending the reader to Dismissed would be a
     * guess that is wrong half the time.
     */
    await page.getByRole('button', { name: 'Look at the reviews that have left the desk' })
      .click()
    await expect(page.getByText('No review yet')).toBeVisible()
    await expect(page.getByRole('radio').filter({ hasText: 'In play' })).toHaveCount(1)

    await page.goto(`${empty}rules`)
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await expect(page.getByText('A rule comes out of a finding you kept')).toBeVisible()

    /*
     * A review and a draft that are not there. `not_found` and an empty review are
     * different answers, and the screen says the first rather than drawing the second.
     */
    await page.goto(`${empty}reviews/1`)
    /* The retired address redirects, and the refusal is on the page it lands on. */
    await expect(page).toHaveURL(`${empty}reviews/1/understand`)
    await expect(page.getByText('Not on the desk')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible()

    /*
     * **The draft of a review that is not there is still a refusal**, and that is not a
     * contradiction of step 6 never being a dead end. A review with no *draft* draws the
     * frame and says so; a review that is not on the desk at all has no frame to draw,
     * because the trail is read off the review.
     */
    await page.goto(`${empty}reviews/1/draft`)
    await expect(page.getByText('Not on the desk')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible()
  })

  test('nothing on an empty desk is a zero pretending to be a measurement', async ({ page }) => {
    await page.goto(empty)
    /* "Absent is silent": an empty desk does not report nought items across nought repos. */
    await expect(page.getByText('0 items across')).toHaveCount(0)
  })
})

test.describe('the server stops while a page is open', () => {
  test('a message, the command that fixes it, and a retry that works', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}/understand`)
    await expect(page.getByText('A webhook delivery that fails once')).toBeVisible()

    /*
     * The server going away, as the page sees it: every request to the API fails at the
     * network. Done by cutting the route rather than by killing the binary, because this
     * binary is also serving the page itself and the rest of this run — and what the page
     * sees is identical either way.
     */
    await page.route('**/api/v1/**', route => route.abort('connectionrefused'))
    await page.reload()

    await expect(page.getByText('Review Desk is not answering')).toBeVisible()
    await expect(page.getByText('review-desk serve')).toBeVisible()
    /* Not "the database is unreadable", which is a different failure with a different fix. */
    await expect(page.getByText('could not read its database')).toHaveCount(0)
    /* And not a blank page: the message is a notice, not the whole screen. */
    await expect(page.locator('.notice--blocked')).toBeVisible()

    const retry = page.getByRole('button', { name: 'Retry' })
    await expect(retry).toBeVisible()

    /* The developer starts the server again, and clicks it. */
    await page.unroute('**/api/v1/**')
    await retry.click()

    await expect(page.getByText('Review Desk is not answering')).toHaveCount(0)
    await expect(page.getByText('A webhook delivery that fails once')).toBeVisible()
  })

  test('the message sits over what the page was showing, not instead of it', async ({ page }) => {
    /*
     * Stopped *after* a page has read successfully, which is the state the criterion is
     * about: the record has not gone anywhere, the connection has, so the developer keeps
     * reading what they were reading with a message over it.
     *
     * Reached through the Rules screen's repository picker, because that is the one control
     * that re-reads without navigating — a reload or a link would mount the page afresh,
     * and a page with nothing read yet has nothing to keep.
     */
    await page.goto(`${seeded}rules`)
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()

    await page.route('**/api/v1/**', route => route.abort('connectionrefused'))
    await page.getByRole('radio', { name: 'All repositories' }).click()

    await expect(page.getByText('Review Desk is not answering')).toBeVisible()
    /* And what it had read is still on screen under it. */
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
  })
})

test.describe('the guard is still on', () => {
  test('the dashboard’s own origin is what reaches the API, and no other', async ({ page }) => {
    /*
     * The dashboard works because its requests are same-origin. This asserts the other
     * half — that the server still refuses a request from somewhere else — from inside a
     * real browser, which is the one place it has never been checked.
     */
    await page.goto(seeded)

    const refusal = await page.evaluate(async (base) => {
      const answer = await fetch(`${base}api/v1/inbox`, {
        headers: { origin: 'https://evil.example' },
      })
      return { status: answer.status, body: await answer.text() }
    }, seeded)

    /*
     * A browser will not let a page set its own `Origin`, so this request carries the
     * dashboard's — which is this server's own, and is accepted. The refusal of a foreign
     * origin is asserted where it can be: `crates/server/tests/cross_origin.rs`, over a
     * raw socket. What is proved here is that the dashboard's requests are the ones the
     * guard accepts, which is what DEV-862 had to be true without touching the guard.
     */
    expect(refusal.status).toBe(200)
    expect(refusal.body).toContain('briefs_to_confirm')
  })

  test('nothing may frame the dashboard', async ({ page }) => {
    await page.goto(seeded)
    const framed = await page.evaluate(async (base) => {
      const answer = await fetch(base)
      return {
        frameOptions: answer.headers.get('x-frame-options'),
        policy: answer.headers.get('content-security-policy'),
      }
    }, seeded)

    expect(framed.frameOptions).toBe('DENY')
    expect(framed.policy).toContain('frame-ancestors \'none\'')
  })
})

/*
 * **The gap between a sign-off and a posting**, which is DEV-932 and is the one part of
 * this screen that reports on somebody else.
 *
 * Review Desk never posts. The developer signs off in the browser, and a review session
 * reads the approved draft and runs `review-desk draft posted`. So both halves of this are
 * real here: the sign-off is a click, the posting is the command an agent session runs, and
 * what is measured in between is whether the page noticed without being reloaded.
 *
 * **Its reviews are its own.** Everything above asserts on the seeded reviews, including
 * how many of them the queue is waiting on, so these open theirs through the command line
 * and this block runs last in the file.
 */
test.describe('the draft page after a sign-off', () => {
  /** The draft as the command line reads it, which is how a review session reads it. */
  function draftRecord(id: number) {
    return cliJson(seededDatabase, ['draft', 'show', '--review', String(id)]) as unknown as {
      draft: { status: string, event: string | null }
    }
  }

  /** Sign off in the browser, which is the only place a draft can be signed off. */
  async function signOff(page: Page, id: number, event: string) {
    await page.goto(`${seeded}reviews/${id}/draft`)
    await page.locator('select#event').selectOption(event)
    await page.getByRole('button', { name: 'Sign off and post' }).click()
    /*
     * **The status line, which is drawn.** This waited on the live region's sentence, and
     * the region is `rd-visually-hidden` — a one-pixel clipped box, which `toBeVisible()`
     * accepts. The whole waiting block could have been deleted and this would have passed.
     */
    await expect(page.getByText('— waiting for the session to post it.')).toBeVisible()
  }

  test('while it waits, the page says what is known and keeps it current', async ({ page }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Carry an idempotency key on a retried delivery',
    )
    await signOff(page, id, 'request_changes')

    const panel = page.locator('.watch')

    /* Who is expected to act, which nothing on this page said before. */
    await expect(panel).toContainText('This review’s own session posts it, not Review Desk')
    /* And what it cannot say: Allele owns whether a session is running. */
    await expect(panel).toContainText('not something Review Desk is told')
    await expect(panel).toContainText('within about 40 seconds')

    /* How long ago the sign-off was, and when anything last wrote to the review. */
    await expect(panel).toContainText('Signed off')
    await expect(panel).toContainText('Last written to')
    await expect(panel).not.toContainText('Not recorded')

    /*
     * **Current without a reload.** The figure is worded from the browser's clock on every
     * read, so after three of the page's own intervals it says seconds rather than "just
     * now" — which is the whole difference from the sentence this screen used to print once.
     */
    await expect(panel.getByText(/\d+ sec ago/).first()).toBeVisible({ timeout: 20_000 })

    /* The record is unchanged by any of it: signing off is the only write that happened. */
    expect(draftRecord(id).draft.status).toBe('approved')
  })

  test('the session posts it, and the page says so without a reload', async ({ page }) => {
    const { reviewId: id, pr } = seedReadyDraft(
      seededDatabase,
      'Record the delivery attempt count inside the transaction',
    )
    await signOff(page, id, 'comment')

    /*
     * A mark the test can check for afterwards. If the page reloaded, this is gone — so it
     * is the acceptance criterion "with no reload" rather than a claim about one.
     */
    await page.evaluate(() => {
      ;(window as unknown as { postingWatchStayed?: boolean }).postingWatchStayed = true
    })

    /*
     * **With the send-back form open under it**, which is the race the guard on that
     * form's submit is about: the developer starts typing a note and the session posts
     * while they type.
     */
    await page.getByRole('button', { name: 'Take back the sign-off and send it back' }).click()
    await page.locator('textarea#send-back-note').fill('Half a note, never sent')

    /*
     * **The review session's own command, with the GitHub review it became.** Review Desk
     * posts nothing: this is the one transition that involves GitHub and the session makes
     * it. A number rather than a node id, because a number is what GitHub builds its own
     * address for a review from.
     */
    cli(seededDatabase, [
      'draft',
      'posted',
      '--review',
      String(id),
      '--github-review',
      '1842003517',
    ])

    const panel = page.locator('.watch')
    /* Noticed on the page's own timer, inside two of its intervals. */
    await expect(panel).toContainText('Posted on GitHub.', { timeout: 20_000 })
    await expect(panel).toContainText('Posted as')
    await expect(panel).toContainText('Comment')
    await expect(panel).toContainText('Taken to post')

    const link = panel.getByRole('link', { name: /See the review on GitHub/ })
    await expect(link).toHaveAttribute(
      'href',
      `https://github.com/acme/portal/pull/${pr}#pullrequestreview-1842003517`,
    )

    /* The heading and the sentence under it followed, which is the rest of the page. */
    await expect(page.getByRole('heading', { name: 'Posted.' })).toBeVisible()

    /*
     * And the form closed itself. A textarea asking what the session is to change, over a
     * draft already on GitHub, with a dead button and only Cancel as a way out, is the
     * page leaving a question open that the record has answered.
     */
    await expect(page.locator('textarea#send-back-note')).toHaveCount(0)

    /*
     * **And the sign-off's own notice is gone.** "Signed off. The review session posts it"
     * is what the write said, in the present tense, and it stood until the next write
     * because nothing on this page used to move underneath it. The panel flips now, so a
     * notice left in place would have the page saying two things about one draft.
     */
    await expect(page.getByText('Signed off. The review session posts it')).toHaveCount(0)

    const stayed = await page.evaluate(
      () => (window as unknown as { postingWatchStayed?: boolean }).postingWatchStayed,
    )
    expect(stayed).toBe(true)
  })

  test('after forty seconds with no post it says the session may not be running', async ({
    page,
  }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Skip a schedule whose tenant row has gone',
    )

    /*
     * **The browser's clock, moved rather than waited out.** Forty seconds of real waiting
     * is forty seconds on every run of this suite to measure one sentence, and a test that
     * sleeps past a threshold is a test that goes red when a machine is busy.
     *
     * **Pinned to the record's own `approved_at`, not moved by a duration.** `install`
     * leaves the clock running — measured: three real seconds move `Date.now()` by 3,004
     * ms — so `fastForward('45')` would land at 45 seconds *plus* however long the sign-off
     * took, which is 45 until the wording crosses a minute and becomes "1 min". The figure
     * is exactly 45 here, on any machine, however slow.
     */
    await page.clock.install()
    await signOff(page, id, 'request_changes')

    const panel = page.locator('.watch')
    await expect(panel).not.toContainText('may not be running')

    await pinClockAt(page, draftStamps(seededDatabase, id).approvedAt, 45)

    await expect(panel).toContainText('may not be running')

    /* Exactly the figure the clock was pinned to, because it was pinned to the stamp. */
    await expect(panel).toContainText('in the 45 sec since you signed off')
    /* And the way out. */
    await expect(panel).toContainText('Check the session that was reviewing this')
    await expect(panel).toContainText('take the sign-off back')

    /*
     * **Plainly, which is measured rather than asserted by the absence of a class.**
     * Nothing has gone wrong with the record — the draft is signed off and still postable
     * — and what is in doubt is a session this app cannot see, so the desk does not raise
     * its voice about something it is guessing at. The warn ink is resolved off the token
     * rather than written out, so changing `--rd-warn-ink` cannot quietly pass this.
     */
    const alarm = await page.evaluate(() => {
      const probe = document.createElement('span')
      probe.style.color = getComputedStyle(document.documentElement)
        .getPropertyValue('--rd-warn-ink')
      document.body.append(probe)
      const resolved = getComputedStyle(probe).color
      probe.remove()
      return resolved
    })
    const said = await panel.locator('.late').evaluate(one => ({
      color: getComputedStyle(one).color,
      background: getComputedStyle(one).backgroundColor,
    }))
    expect(said.color).not.toBe(alarm)
    expect(said.background).toBe('rgba(0, 0, 0, 0)')
    await expect(panel.locator('.notice--problem, .notice--blocked')).toHaveCount(0)

    /* And it is still signed off: nothing here cancels anything. */
    expect(draftRecord(id).draft.status).toBe('approved')
  })

  test('taking the sign-off back stops the waiting, while the page is watching', async ({
    page,
  }) => {
    const { reviewId: id } = seedReadyDraft(
      seededDatabase,
      'Name the helper this change deletes',
    )
    await signOff(page, id, 'request_changes')

    const panel = page.locator('.watch')
    await expect(panel).toContainText('Signed off')

    /*
     * The one transition out of `approved` that is the developer's, added by DEV-929's
     * posting half — and the state this page has to handle while it is polling.
     */
    await page.getByRole('button', { name: 'Take back the sign-off and send it back' }).click()
    await page.locator('textarea#send-back-note').fill('Fold F1 into the prose; it has no line')
    await page.getByRole('button', { name: 'Send back', exact: true }).click()

    await expect(panel).toContainText('The sign-off is taken back')
    await expect(panel).not.toContainText('Waiting for this review')
    await expect(panel).not.toContainText('may not be running')

    /*
     * **And the send-back's own notice survived the poll that was running under it.** The
     * page clears a stale notice when the draft reports posted, and only then: clearing on
     * any status change raced the write, because a tick landing between a write's refresh
     * being issued and answered would wipe the notice that write had just set — leaving
     * the developer who took a sign-off back with no confirmation at all.
     */
    await expect(page.getByText('Sent back. The session has the note.')).toBeVisible()

    expect(draftRecord(id).draft.status).toBe('sent_back')
    expect(draftRecord(id).draft.event).toBeNull()
  })
})

/** Nothing on the page is wider than the page, which is the failure a screenshot hides. */
async function fitsSideways(page: Page, what: string): Promise<void> {
  const sideways = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }))
  expect(
    sideways.scrollWidth,
    `${what} is ${sideways.scrollWidth}px wide in ${sideways.clientWidth}px`,
  ).toBeLessThanOrEqual(sideways.clientWidth + 1)
}

test.describe('taking a review off the desk', () => {
  /**
   * **DEV-945, through the browser.** The record's rules are held by
   * `crates/store/tests/dismissal.rs` and the API's by `crates/server/tests/removal.rs`;
   * what only a browser can show is that the developer can actually do it — the reason box
   * is a real field, the delete asks twice, and a dismissed review then says so on its own
   * page and on the list.
   *
   * **The dismissal is undone in a `finally`**, because this suite is serial over one
   * database and every other test in it reads the queue: a review left dismissed would
   * take a row off screens that nothing else in this file touches. The delete has no undo,
   * which is why it is performed on the one review seeded for it and nothing else uses
   * that review.
   */
  test('a review is dismissed with a reason, says so, and is brought back', async ({ page }) => {
    try {
      await page.goto(`${seeded}reviews/${reviewId}/understand`)

      const card = page.locator('#remove-review')
      await expect(card).toContainText('Take this review off the desk')
      await expect(card).toContainText('Dismissing it hides it and keeps it')

      /* No delete for this one: it holds a brief, findings and a draft. */
      await expect(card).toContainText('It cannot be deleted')
      await expect(card.getByRole('button', { name: 'Delete it' })).toHaveCount(0)

      await card.getByRole('button', { name: 'Dismiss this review' }).click()
      const reason = page.getByLabel('Why are you dismissing this review?')
      await expect(reason).toBeVisible()

      /* A blank reason is refused by the control, before anything is sent. */
      await card.getByRole('button', { name: 'Dismiss it' }).click()
      await expect(card).toContainText('A reason is required')

      await reason.fill('the pull request was closed unmerged')
      await card.getByRole('button', { name: 'Dismiss it' }).click()

      await expect(card).toContainText('acme/portal#412 is dismissed.')
      await expect(card).toContainText('the pull request was closed unmerged')

      /* The page says it on every one of the review's pages, not only on this one. */
      await page.goto(`${seeded}reviews/${reviewId}/code-review`)
      await expect(page.getByText('This review is dismissed')).toBeVisible()

      /* It is out of the queue, and the row it had there is gone. */
      await page.goto(seeded)
      await expect(page.getByText('Retry failed webhook deliveries')).toHaveCount(0)

      /* Out of the Reviews list's default view, and in its dismissed one with why. */
      await page.goto(`${seeded}reviews`)
      await expect(page.getByText('Retry failed webhook deliveries')).toHaveCount(0)
      await page.getByRole('radio').filter({ hasText: 'Dismissed' }).click()
      const row = page.locator('.row').filter({ hasText: 'Retry failed webhook' })
      await expect(row).toHaveCount(1)
      await expect(row).toContainText('Dismissed')
      await expect(row).toContainText('the pull request was closed unmerged')

      /* And no session is offered it, which is what the work list is for. */
      const work = cliJson(seededDatabase, ['work', 'list']) as unknown as {
        work: Array<{ review_id: number }>
      }
      expect(work.work.map(item => item.review_id)).not.toContain(reviewId)
    }
    finally {
      /*
       * **The card is waited for before the button is looked for.** `count()` does not
       * wait: the page's first read is a request, so asking straight after a `goto`
       * answers zero and the restore is skipped silently — which left the review dismissed
       * for every test after this one, and the width test below failed on a page with no
       * dismiss button. Waiting for the card's own heading settles the read, and only then
       * is the undo's absence an answer about the review rather than about the timing.
       */
      await page.goto(`${seeded}reviews/${reviewId}/understand`)
      const card = page.locator('#remove-review')
      await expect(card).toContainText('Take this review off the desk')
      const undo = card.getByRole('button', { name: 'Bring it back' })
      if (await undo.count()) {
        await undo.click()
        await expect(card).toContainText('is back in the queue')
      }
    }
  })

  /*
   * **Measured at a laptop's width and at a phone's**, which is this repository's rule for
   * anything a developer presses. The card holds a text field and three buttons in a row,
   * which is the shape that overflows a 390-pixel screen if it is written as one line.
   */
  for (const [what, width, height] of [
    ['a laptop', 1440, 900],
    ['a phone', 390, 844],
  ] as Array<[string, number, number]>) {
    test(`the removal card fits ${what} at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height })

      /*
       * **Both of the card's open states, on the one review that has both.** The request
       * nobody started can be dismissed *and* deleted, so the reason box with its field
       * and two buttons and the delete's question with its two answers are both reachable
       * without this test depending on what another one left behind.
       */
      await page.goto(`${seeded}reviews/${requestedReviewId}/understand`)
      const card = page.locator('#remove-review')
      await expect(card).toContainText('Take this review off the desk')

      await card.getByRole('button', { name: 'Delete it', exact: true }).click()
      await expect(card).toContainText('cannot be undone')
      await fitsSideways(page, `the delete confirmation at ${width}px`)
      for (const name of ['Delete it for good', 'Keep it']) {
        const box = await card.getByRole('button', { name, exact: true }).boundingBox()
        expect(box, `${name} at ${width}px has no box`).not.toBeNull()
        expect(
          box!.height,
          `${name} at ${width}px is ${box!.height}px tall`,
        ).toBeGreaterThanOrEqual(CONTROL_MIN)
      }
      await card.getByRole('button', { name: 'Keep it' }).click()

      await card.getByRole('button', { name: 'Dismiss this review' }).click()
      await expect(page.getByLabel('Why are you dismissing this review?')).toBeVisible()
      await fitsSideways(page, `the reason box at ${width}px`)
      for (const name of ['Dismiss it', 'Cancel']) {
        const box = await card.getByRole('button', { name, exact: true }).boundingBox()
        expect(box, `${name} at ${width}px has no box`).not.toBeNull()
        expect(
          box!.height,
          `${name} at ${width}px is ${box!.height}px tall`,
        ).toBeGreaterThanOrEqual(CONTROL_MIN)
      }
      /* Nothing is left open behind this test, and nothing was written by it. */
      await card.getByRole('button', { name: 'Cancel' }).click()
    })
  }
  test('a review that never started is deleted, after asking twice', async ({ page }) => {
    await page.goto(`${seeded}reviews/${requestedReviewId}/understand`)

    /* Nothing has been written about it, which is the page this review has. */
    await expect(page.getByText('Nothing has been written about this problem yet')).toBeVisible()

    const card = page.locator('#remove-review')
    await expect(card).toContainText('This review never started')

    await card.getByRole('button', { name: 'Delete it', exact: true }).click()
    await expect(card).toContainText(`Delete review ${requestedReviewId}`)
    await expect(card).toContainText('acme/portal #777')

    /* Keeping it is a real way out. */
    await card.getByRole('button', { name: 'Keep it' }).click()
    await expect(card).not.toContainText(`Delete review ${requestedReviewId}`)
    expect(
      cliJson(seededDatabase, ['review', 'find', '--repo', 'acme/portal', '--pr', '777']),
    ).toHaveProperty('reviews')

    await card.getByRole('button', { name: 'Delete it', exact: true }).click()
    await card.getByRole('button', { name: 'Delete it for good' }).click()

    /* It leaves the page it was deleted from, for the list it came from — carrying the
       one sentence an act with no undo gets, which cannot be said where it was done. */
    await expect(page).toHaveURL(/\/reviews$/)
    await expect(page.getByText('is deleted')).toBeVisible()
    await expect(page.locator('.row').filter({ hasText: '#777' })).toHaveCount(0)
    /* And no refusal was drawn over the page on the way out. */
    await expect(page.getByText('Not on the desk')).toHaveCount(0)
    await page.getByRole('radio').filter({ hasText: 'Everything' }).click()
    await expect(page.locator('.row').filter({ hasText: '#777' })).toHaveCount(0)

    /* And the record holds nothing of it. */
    const found = cliJson(seededDatabase, [
      'review',
      'find',
      '--repo',
      'acme/portal',
      '--pr',
      '777',
    ]) as unknown as { reviews: unknown[] }
    expect(found.reviews).toHaveLength(0)
  })
})

/*
 * **A merged pull request's review, finished automatically, in a browser.**
 *
 * DEV-957, and its acceptance criterion by name: "Tested on a record shaped like
 * Patrick's: four rounds of one merged pull request, one posted, three not." `seedMerged`
 * is that record, and the state is written by a real `review-desk harvest` reading a
 * recorded response — the same code path a live harvest takes, with nothing leaving the
 * machine.
 *
 * **Only round 1 carries a `pr_state`**, because a harvest reads the round a review was
 * posted on. So every assertion below is about the derivation and not about a column: a
 * dashboard reading the round's own state would show three of these four rounds as open on
 * a pull request that merged on 11 October.
 */
test.describe('a merged pull request finishes its review', () => {
  test('is in no group of the queue, and in no work list', async ({ page }) => {
    await page.goto(seeded)
    await expect(page.locator('.row').first()).toBeVisible()

    /* Not under Needs you, not with the agent, not waiting on an author. */
    await expect(page.getByText('#9568')).toHaveCount(0)
    await expect(page.getByText('Retry a failed delivery')).toHaveCount(0)

    /*
     * **And no agent is offered work on it**, which is the half the screen cannot show.
     * `work list` is what the Locus dispatcher polls, so an item here is a session started
     * on a review that can never be posted.
     */
    const work = cliJson(seededDatabase, ['work', 'list']) as unknown as {
      work: Array<{ repo: string, pr: number }>
    }
    expect(work.work.filter(item => item.pr === 9568)).toHaveLength(0)
    expect(work.work.length, 'and the list is not empty, so this is not vacuous')
      .toBeGreaterThan(0)
  })

  test('every round of it is under Finished, posted or not', async ({ page }) => {
    await page.goto(`${seeded}reviews`)
    await expect(page.locator('.row').first()).toBeVisible()

    /* Not in the default view. */
    await expect(page.getByText('#9568')).toHaveCount(0)

    await page.getByRole('radio').filter({ hasText: 'Finished' }).click()
    await expect(page.getByText('whose pull request merged or closed')).toBeVisible()

    /* All four, which is the derivation: three of them hold no state of their own. */
    const rows = page.locator('.queue-line').filter({ hasText: '#9568' })
    await expect(rows).toHaveCount(mergedReviewIds.length)

    /* And each says what happened, in the words of the thing that happened. */
    await expect(rows.first()).toContainText('Finished — the pull request merged')
    await expect(rows.first()).toContainText('No round of it can be posted')
    await expect(rows.first()).not.toContainText('Dismissed')

    /* Dismissed is a different view, and nothing here is in it. */
    await page.getByRole('radio').filter({ hasText: 'Dismissed' }).click()
    await expect(page.getByText('#9568')).toHaveCount(0)
  })

  test('step 7 of the round that posted reads done, and says how it ended', async ({ page }) => {
    await page.goto(`${seeded}reviews/${mergedReviewIds[0]}/understand`)
    const steps = await trail(page)
    expect(steps[6], 'step 7').toContain('merged')
  })

  for (const { name, width, height } of [
    { name: 'a laptop', width: 1440, height: 900 },
    { name: 'a phone', width: 390, height: 844 },
  ]) {
    test(`an unposted draft on it says so and offers no sign-off at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      /* Round 2: a draft the session finished and nothing will ever post. */
      await page.goto(`${seeded}reviews/${mergedReviewIds[1]}/draft`)

      /*
       * **Said twice, deliberately, and both are asserted.** The status line under the
       * heading is where the draft's state goes, and the card in the sign-off's place is
       * where a reader looking for the button finds out why there is none. Scoped, because
       * one page saying one thing in two places is a strict-mode violation for an
       * unscoped locator rather than a fault.
       */
      /*
       * The format rather than the day: `dayLabel` words the developer's own local day, so
       * a machine east of UTC reads the 12th of a merge at 22:41 on the 11th. What this
       * pins is day-month-year, which is the thing `toLocaleDateString` got wrong.
       */
      const said = /Not posted — the pull request merged on \d{1,2} [A-Z][a-z]+ \d{4}/
      await expect(page.locator('.prose').filter({ hasText: said })).toBeVisible()
      const aside = page.getByRole('complementary')
      await expect(aside.getByText('Nothing to sign off')).toBeVisible()
      await expect(aside.getByText(said)).toBeVisible()

      /*
       * **No control, rather than a disabled one.** Each of the three acts the panel offers
       * asks a session to do something afterwards, and on a merged pull request none is
       * coming — a greyed-out "Sign off and post" would still be offering the act, and the
       * API refuses it.
       */
      await expect(page.getByRole('button', { name: 'Sign off and post' })).toHaveCount(0)
      await expect(page.getByRole('button', { name: /Send back/ })).toHaveCount(0)
      await expect(page.getByRole('button', { name: /Ticket what is left/ })).toHaveCount(0)

      /* The draft itself is still there to read, which is what "kept" means. */
      await expect(page.getByText('One finding on the key the retry carries')).toBeVisible()
    })

    test(`the round that posted still reads as posted at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      await page.goto(`${seeded}reviews/${mergedReviewIds[0]}/draft`)
      await expect(page.getByText('Not posted —')).toHaveCount(0)
    })
  }

  /*
   * **And the API refuses it, naming the reason.** The page offers no button, so this is
   * the race the hidden button cannot cover: a pull request that merges between the page's
   * read and the press. The screen and the server answer from one value, which is what
   * makes them agree.
   */
  test('the API refuses a sign-off on it, naming the merge', async ({ page }) => {
    /*
     * **From inside the page**, as the cross-origin test above is and for its reason: the
     * server's guard accepts the dashboard's own origin, and a browser will not let a page
     * forge one. A request from Playwright's own context carries no origin the guard knows.
     */
    await page.goto(seeded)
    const refusal = await page.evaluate(async ({ base, reviewId }) => {
      const answer = await fetch(`${base}api/v1/reviews/${reviewId}/draft/approve`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ event: 'comment' }),
      })
      return { status: answer.status, body: await answer.text() }
    }, { base: seeded, reviewId: mergedReviewIds[1] })

    expect(refusal.status, refusal.body).toBe(409)
    const body = JSON.parse(refusal.body) as { code: string, message: string }
    expect(body.code).toBe('conflict')
    expect(body.message, 'the refusal names the reason').toContain('merged')
  })
})

/*
 * **A round a newer round supersedes, in a browser.**
 *
 * DEV-957's comment: "The older round's draft page says it is superseded, and its draft
 * cannot be signed off." `globex/portal#9541` round 1 is that round — its pull request is open,
 * so this is the supersession on its own rather than the merge.
 */
test.describe('a superseded round', () => {
  test('its draft page says which round superseded it, and offers no sign-off', async ({
    page,
  }) => {
    await page.goto(`${seeded}reviews/${supersededReviewIds[0]}/draft`)

    const said = /Not posted — superseded by round 2/
    await expect(page.locator('.prose').filter({ hasText: said })).toBeVisible()
    const aside = page.getByRole('complementary')
    await expect(aside.getByText('Nothing to sign off')).toBeVisible()
    await expect(aside.getByText(said)).toBeVisible()
    await expect(page.getByRole('button', { name: 'Sign off and post' })).toHaveCount(0)
  })

  /**
   * **A review nobody can sign off still has somewhere to say what a write did.**
   *
   * The page's notice was the last line of the sign-off card, and that card is the `v-else`
   * of the one saying "Nothing to sign off" — so on exactly the reviews this block is about,
   * no element on the page could draw it. Raised by a review of this branch.
   *
   * **Asserted through a save that succeeds**, which is the sharper half of it. The record
   * takes an edit to a superseded round: the draft stays on it to be read, and `save_draft`
   * refuses only an `approved` or a `posted` draft. So this is not a refusal going missing —
   * it is the ordinary "Saved." that every other review on the desk gets, and that this one
   * could not be told.
   */
  test('a draft nobody can sign off still says what a save did', async ({ page }) => {
    const id = supersededReviewIds[0]!
    await page.goto(`${seeded}reviews/${id}/draft`)
    const aside = page.getByRole('complementary')
    await expect(aside.getByText('Nothing to sign off')).toBeVisible()

    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    const before = await editor.inputValue()
    await editor.fill(`${before}\n\nA sentence saved on a round nobody can sign off.`)
    await page.getByRole('button', { name: 'Save the draft' }).click()

    /* The sentence the page could not say before: it is in the aside, outside that card. */
    await expect(aside.getByText(/^Saved\./)).toBeVisible()

    /* And the record took it, which is why a refusal is not what this asserts. */
    const saved = cliJson(seededDatabase, ['draft', 'show', '--review', String(id)]) as unknown as {
      draft: { body: string }
    }
    expect(saved.draft.body).toContain('nobody can sign off')
  })

  test('the newer round’s draft page still offers the sign-off', async ({ page }) => {
    /* The converse: a guard that refused every round would refuse the one that can post. */
    await page.goto(`${seeded}reviews/${supersededReviewIds[1]}/draft`)
    await expect(page.getByRole('button', { name: 'Sign off and post' })).toBeVisible()
    await expect(page.getByText('Nothing to sign off')).toHaveCount(0)
  })
})

/*
 * **The row's two links, in its repository line, measured at both widths.**
 *
 * DEV-957's comment: "The GitHub link sits in the row's repository line on every row, at
 * 1440 and 390" and "No row's text extends past the row's right edge". The first is where
 * the link is and the second is what a long line does to the row it is in — and neither is
 * a thing reasoning about a flex box settles, which is why they are measured.
 */
test.describe('a queue row’s links and its edges', () => {
  for (const { name, width, height } of [
    { name: 'a laptop', width: 1440, height: 900 },
    { name: 'a phone', width: 390, height: 844 },
  ]) {
    test(`the GitHub link is in the repository line at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      /*
       * **`globex/portal#9541`, on the queue.** This block runs after the acceptance path,
       * which signs `#412`'s draft off and takes it out of the queue — so a row named
       * there is a row whose presence depends on the order these tests ran in. Nothing in
       * this suite touches #9541, so its row is on the queue from the first test to the
       * last.
       */
      await page.goto(seeded)

      const row = page.locator('.queue-line').filter({ hasText: '#9541' }).first()
      await expect(row).toBeVisible()

      /*
       * **In the line, and not in the row's own link.** Both halves: DEV-957 asks for the
       * first, and DEV-936's seventh criterion forbids the second — an anchor inside the
       * row's anchor opens the review when you press it.
       */
      const line = row.locator('.row-meta')
      await expect(line).toBeVisible()
      await expect(line).toContainText('globex/portal')
      const github = line.getByRole('link', { name: /pull request #9541/ })
      await expect(github).toHaveAttribute(
        'href',
        'https://github.com/globex/portal/pull/9541',
      )
      await expect(row.locator('.row').locator('.row-out-link')).toHaveCount(0)

      /*
       * **And it reads after the repository rather than above it or beside the action.**
       * The complaint was where it sat: "on most rows it sits alone on a line under the
       * pull request number, and on the #9568 row it floats top right above the action".
       *
       * **At 1440 it shares a line with the repository; at 390 that line wraps**, and the
       * link falls under the repository text rather than beside it — which is the line
       * wrapping and is what it should do, because the repository, the round and two links
       * do not fit in 358 pixels. So the measurement is the thing that is true at both
       * widths: the link never sits *above* the repository, and when the two do share a
       * line it begins after it. The floating column this replaced failed both.
       */
      const repo = await line.locator('.meta-text').boundingBox()
      const link = await github.boundingBox()
      const title = await row.locator('.title').boundingBox()
      expect(repo, 'the repository text has a box').not.toBeNull()
      expect(link, 'the link has a box').not.toBeNull()
      expect(title, 'the title has a box').not.toBeNull()

      /* Under the title, which is where the repository line is. */
      expect(link!.y, 'the link is below the title, not level with the action')
        .toBeGreaterThan(title!.y)
      /* Never above the repository text. */
      expect(link!.y, `repo at y=${repo!.y}, link at y=${link!.y}`)
        .toBeGreaterThanOrEqual(repo!.y)

      const sharesALine = link!.y < repo!.y + repo!.height && repo!.y < link!.y + link!.height
      if (sharesALine) {
        expect(link!.x, 'on one line, the link is after the repository')
          .toBeGreaterThanOrEqual(repo!.x)
      }
      else {
        /*
         * **Wrapped: the links are the next line of the same line.** Measured on the cell
         * that holds them rather than on the anchor, because the cell begins with the
         * separator that carries the continuation — the wrapped line reads "· GitHub ·
         * DEV-941", and the anchor therefore starts one separator's width in. What the
         * criterion is about is that the line continues at the line's own left edge, which
         * is the cell.
         */
        expect(width, 'only a phone-width line wraps').toBeLessThan(560)
        const cell = await line.locator('.row-out').boundingBox()
        expect(cell, 'the links cell has a box').not.toBeNull()
        expect(Math.abs(cell!.x - repo!.x), 'and it begins where the line begins')
          .toBeLessThanOrEqual(1)
        expect(link!.x, 'with the link inside it').toBeGreaterThanOrEqual(cell!.x)
      }
    })

    test(`no row's text runs past the row's edge at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height })

      /*
       * **Both pages, and every view of the list**, because the rows differ: a queue row
       * carries a next action and a tally, a list row carries the step and a finished row
       * carries a whole sentence about the merge. They are one component and the widest
       * line is not always on the same page.
       */
      let measured = 0
      for (const [where, path, view] of [
        ['the queue', '', null],
        ['the reviews', 'reviews', null],
        ['the finished reviews', 'reviews', 'Finished'],
      ] as Array<[string, string, string | null]>) {
        await page.goto(`${seeded}${path}`)
        if (view != null) {
          await page.getByRole('radio').filter({ hasText: view }).click()
        }
        await expect(page.locator('.queue-line').first()).toBeVisible()

        /*
         * **Every row, measured against its own right edge rather than the viewport's.**
         * That is the criterion's wording — "no row's text extends past the row's right
         * edge" — and it is the stricter of the two: a row inside a section that is itself
         * too wide passes a viewport check and still has text hanging out of it.
         */
        const over = await page.evaluate(() =>
          [...document.querySelectorAll('.queue-line')].flatMap((row) => {
            const edge = row.getBoundingClientRect().right
            return [...row.querySelectorAll('*')]
              .filter(one => !one.closest('.rd-visually-hidden'))
              .map((one) => {
                const box = one.getBoundingClientRect()
                return {
                  what: `${one.tagName.toLowerCase()}.${one.className || '-'}`,
                  over: Math.max(box.right - edge, one.scrollWidth - one.clientWidth),
                }
              })
              .filter(({ over }) => over > 0.5)
          }))
        expect(over, `nothing in a row runs past it on ${where} at ${name}`).toEqual([])
        measured += await page.locator('.queue-line').count()
      }

      expect(measured, 'some rows were measured, so this is not vacuous').toBeGreaterThan(0)
    })
  }
})

/*
 * **Unsaved edits, and an open row, belong to the review they were made on.** DEV-960.
 *
 * ## What is actually wrong, which is not what the issue says
 *
 * The issue's mechanism is that Nuxt reuses a page component when only the review in the
 * address changes, so five step pages draw one review's state over another's — and on the
 * draft page a save then writes one review's prose into another review's draft.
 *
 * **It does not happen, and that was measured rather than argued.** `<NuxtPage />` without a
 * `pageKey` keys a page by its own path with the parameters interpolated, so changing the
 * review changes the key and Vue mounts a fresh component. Against these pages as they stood
 * before this change: unsaved text typed on one review's draft, a history jump to another
 * review's draft, and the arriving page had the Edit tab unselected and the text nowhere on
 * it. Four of the tests below pass against those pages for that reason, and say so.
 *
 * **What was wrong is that the typing was destroyed without a word.** An unsaved draft is on
 * no record, the component goes either way, and nothing asked. That is what the two route
 * guards fix, and what the tests with teeth are about.
 *
 * ## How a test reaches a change of the parameter at all
 *
 * **A typed address is not one.** `ssr: false` and history-mode routing make it a new
 * document: the page is built again and the editor starts empty. The issue lists it beside
 * browser history, and a test below says what actually happens.
 *
 * **Nor is any link in the app**, today: none goes from one review's step page to another's.
 *
 * What is left is the browser's own history, and not one step of it — Back from a draft page
 * is the step before it, which is another page of the same review. It is a jump of *more
 * than one entry*, which is what the back button's own menu does: pick the entry two or
 * three back and the router changes only the parameter. So each test here builds a real
 * history — the queue, the list, a review, its page, the list again, another review, its
 * page — and jumps. The router sees that as an **update**, which is the route
 * `onBeforeRouteUpdate` is on and the one a guard has to answer.
 *
 * ## Its reviews are its own
 *
 * As the sign-off block above, and for its reason: everything before this asserts how many
 * reviews the queue and the list hold, so a review opened here would move those counts. This
 * block runs last in the file.
 */
test.describe('state that belongs to one review', () => {
  /** The two reviews, of the same shape so that one review's row numbers exist on the other. */
  let first = 0
  let second = 0
  /**
   * Their pull request numbers, which the frame draws and a test waits for.
   *
   * **A jump lands before the arriving review has been read.** `resource()` keeps the
   * previous answer while the next read is in flight, so for a moment the address is the
   * first review's and every row on screen is still the second's — and `toHaveCount(0)` is
   * satisfied by a *transient* absence, so an assertion made in that window would pass over
   * a page that had carried the state and was about to draw it again. Waiting for `#412` to
   * become `#413` is waiting for the read, which is the only thing that closes the window.
   */
  let firstPr = 0
  let secondPr = 0

  /** A sentence typed into the editor and never saved. */
  const TYPED = 'A sentence the developer typed and did not save.'

  /** Each review's draft, named once so the seed and the restore cannot come to differ. */
  const FIRST_DRAFT = 'The first of the two drafts, written against the first review.'
  const SECOND_DRAFT = 'The second of the two drafts, written against the second review.'

  /**
   * The draft body the record holds, which is what the editor is seeded from.
   *
   * **Read at every use rather than cached in two `let`s, which is what it was.** One test
   * here saves through the editor, so the body it cached went stale the moment that test
   * ran — and the suite is serial with no retries, so a failure anywhere in that test left
   * every assertion after it comparing the editor against a body the record no longer held.
   * Three red tests for one defect, with the cause two tests back. A read is one spawn and
   * the record is the only thing that cannot be stale. Raised by a review of this branch.
   */
  function bodyOf(id: number): string {
    return (
      cliJson(seededDatabase, ['draft', 'show', '--review', String(id)]) as unknown as {
        draft: { body: string }
      }
    ).draft.body
  }

  /**
   * The review's draft carries its own sentence and nothing that was typed and not saved.
   *
   * **Not "as this block seeded it"**, which is what this said and is false of the first
   * review once the saving test has run: its body then carries a saved paragraph too. The
   * assertion is a substring and an absence, and the sentence now says so.
   *
   * **Both halves, because `toContain` on its own cannot fail on an append.** These sites
   * read "nothing has been written to either review", and `toContain(SECOND_DRAFT)` passes
   * just as happily over `SECOND_DRAFT` with the unsaved sentence appended to it — which is
   * the one thing they exist to catch. They were `toBe` against a cached body until that
   * cache was removed for going stale; this is the assertion the cache was carrying. Raised
   * by a review of the commit that removed it.
   */
  function untouched(id: number, sentence: string): void {
    const body = bodyOf(id)
    expect(body).toContain(sentence)
    expect(body).not.toContain(TYPED)
  }

  /**
   * Give a review a draft whose body says which review it is.
   *
   * **The two bodies have to differ**, or "shows that review's own body" is an assertion
   * about nothing: two drafts composed from the same blocks are the same string, and the
   * editor would pass it holding either one.
   */
  function draftSaying(id: number, sentence: string): void {
    const file = resolve(scratch(), `dev-960-draft-${id}.json`)
    writeFileSync(file, JSON.stringify([{ kind: 'prose', text: sentence }]))
    cli(seededDatabase, ['draft', 'put', '--review', String(id), '--sections', file])
  }

  test.beforeAll(() => {
    const one = seedStructuredReview(seededDatabase, 'Give the webhook retry its own context')
    const two = seedStructuredReview(seededDatabase, 'Carry the delivery window on the job')
    first = one.reviewId
    second = two.reviewId
    firstPr = one.pr
    secondPr = two.pr
    expect(firstPr).not.toBe(secondPr)
    draftSaying(first, FIRST_DRAFT)
    draftSaying(second, SECOND_DRAFT)
    /* A seed that produced two equal bodies would make four assertions below vacuous. */
    expect(bodyOf(first)).not.toBe(bodyOf(second))
  })

  /**
   * Reach a review's step page the way a developer does, and say how many entries that took.
   *
   * The count is what the jump is measured in: from the second review's page, that many
   * entries back is the *same page of the first review*, which is the parameter-only change
   * the whole block is about.
   */
  async function byTheTrail(page: Page, id: number, step: string): Promise<number> {
    await page.getByRole('navigation', { name: 'Review Desk' })
      .getByRole('link', { name: 'Reviews' })
      .click()
    await expect(page).toHaveURL(/\/reviews$/)
    await page.locator(`a[href="/reviews/${id}/understand"]`).click()
    await expect(page).toHaveURL(`${seeded}reviews/${id}/understand`)
    if (step === 'understand') return 2

    await page.locator(`.trail a[href="/reviews/${id}/${step}"]`).click()
    await expect(page).toHaveURL(`${seeded}reviews/${id}/${step}`)
    return 3
  }

  /** The back button's own menu: this many entries up, in one navigation. */
  async function jump(page: Page, entries: number): Promise<void> {
    await page.evaluate(delta => history.go(delta), entries)
  }

  /**
   * Open the first row on the page and hand back the id of what it opened.
   *
   * By `aria-controls` rather than by the row's words, so that one test covers four pages
   * whose rows are a problem, a part, an option and a finding.
   */
  async function openTheFirstRow(page: Page, prefix: string): Promise<string> {
    const row = page.locator(`button[aria-controls^="${prefix}"]`).first()
    await expect(row).toHaveAttribute('aria-expanded', 'false')
    const controls = (await row.getAttribute('aria-controls'))!
    await row.click()
    await expect(page.locator(`#${controls}`)).toBeVisible()
    await expect(row).toHaveAttribute('aria-expanded', 'true')
    return controls
  }

  for (const [step, prefix, what] of [
    ['code-review', 'finding-', 'a finding'],
    ['understand', 'problem-', 'a problem'],
    ['solution', 'part-', 'a part'],
    ['alternatives', 'option-', 'an option'],
  ] as const) {
    test(`${what} left open is closed on the next review’s ${step} page`, async ({ page }) => {
      await page.goto(seeded)
      await byTheTrail(page, first, step)
      const entries = await byTheTrail(page, second, step)

      const controls = await openTheFirstRow(page, prefix)

      await jump(page, -entries)
      await expect(page).toHaveURL(`${seeded}reviews/${first}/${step}`)

      /* The arriving review has been read: its own pull request number is in the frame. */
      await expect(page.getByText(`· #${firstPr}`).first()).toBeVisible()
      await expect(page.getByText(`· #${secondPr}`)).toHaveCount(0)

      /*
       * **And this row exists here**, so the assertion is about something: both reviews carry
       * the same brief and the same findings, and the row the second review had open is a row
       * of this one too.
       *
       * **This passes against the pages before the change**, and is kept for what it holds
       * rather than for what it caught. The criterion is the issue's own — a row left open on
       * one review is closed on the next — and it holds today because `NuxtPage` mounts a
       * fresh component per review, not because anything resets the row. Both halves of that
       * are worth locking down: a `pageKey` added later would make this the test that fails,
       * and the `watch(reviewId, …)` on each page is what would keep it passing.
       */
      await expect(page.locator(`button[aria-controls="${controls}"]`)).toHaveCount(1)
      await expect(page.locator(`button[aria-controls="${controls}"]`))
        .toHaveAttribute('aria-expanded', 'false')
      await expect(page.locator(`#${controls}`)).toHaveCount(0)
    })
  }

  /**
   * **The draft page, by history**, which is the defect the issue was raised for.
   *
   * One test for the whole choice, because the three things it asserts are three states of
   * one navigation and splitting them would mean building the history three times: the page
   * asks before it moves, Keep editing leaves everything as it was, and Discard moves and
   * arrives at the other review's own body with nothing unsaved.
   */
  test('moving to another review’s draft with unsaved edits asks, in the page', async ({
    page,
  }) => {
    /*
     * **No browser dialog, ever.** The issue's own words, and the reason this is a card and
     * not `beforeunload`. Collected rather than thrown from the listener, so the failure is
     * an assertion at the end of the test rather than an error inside Playwright's event.
     */
    const dialogs: string[] = []
    page.on('dialog', async (dialog) => {
      dialogs.push(dialog.message())
      await dialog.dismiss()
    })

    await page.goto(seeded)
    await byTheTrail(page, first, 'draft')
    const entries = await byTheTrail(page, second, 'draft')

    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await expect(editor).toHaveValue(bodyOf(second))
    await editor.fill(`${bodyOf(second)}\n\n${TYPED}`)

    /* The back menu, three entries up: the same page, the first review. */
    await jump(page, -entries)

    /*
     * **The page asks, and it has not moved.** The address bar has — a history navigation
     * moves it before any guard runs, and the router puts it back when the guard refuses —
     * so what is asserted here is the page, which is still the second review's draft with
     * the typed sentence in it.
     *
     * Before the change there was no card and the typing was gone without a word, which is
     * this test's teeth and the defect that was really there.
     */
    await expect(
      page.getByText('You have unsaved edits to this review’s draft'),
    ).toBeVisible()
    await expect(page.getByRole('button', { name: 'Keep editing' })).toBeFocused()
    await expect(editor).toHaveValue(`${bodyOf(second)}\n\n${TYPED}`)

    /*
     * Keep editing: the address comes back, not a character is lost, and the editor is the
     * tab showing — the card can come up over the Rendered preview, and a button named Keep
     * editing that left the developer unable to see the editing would not be doing its job.
     */
    await page.getByRole('button', { name: 'Keep editing' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${second}/draft`)
    await expect(page.getByRole('button', { name: 'Keep editing' })).toHaveCount(0)
    await expect(page.getByRole('tab', { name: 'Edit' })).toHaveAttribute('aria-selected', 'true')
    await expect(editor).toHaveValue(`${bodyOf(second)}\n\n${TYPED}`)

    /* Nothing has been written to either review. The sentence was never saved. */
    untouched(first, FIRST_DRAFT)
    untouched(second, SECOND_DRAFT)

    /* Discard: the navigation goes through. */
    await jump(page, -entries)
    await page.getByRole('button', { name: 'Discard the edits' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)

    /*
     * **The first review's own body, and nothing unsaved**, which is the acceptance
     * criterion. True before the change as well, for the reason the block comment gives:
     * this page is mounted again per review. What is new is that it took a choice to get
     * here rather than silence.
     */
    await page.getByRole('tab', { name: 'Edit' }).click()
    await expect(editor).toHaveValue(bodyOf(first))
    await expect(editor).not.toHaveValue(new RegExp(TYPED))
    await expect(page.getByText('Save or discard your changes before signing off.'))
      .toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Save the draft' })).toBeDisabled()

    expect(dialogs).toEqual([])
  })

  /** The same, forward: the entry a developer came back from is reached the same way. */
  test('going forward to another review’s draft with unsaved edits asks too', async ({
    page,
  }) => {
    await page.goto(seeded)
    await byTheTrail(page, first, 'draft')
    const entries = await byTheTrail(page, second, 'draft')

    /* Back to the first review's draft with nothing unsaved, so there is a forward entry. */
    await jump(page, -entries)
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)

    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await expect(editor).toHaveValue(bodyOf(first))
    await editor.fill(`${bodyOf(first)}\n\n${TYPED}`)

    await jump(page, entries)
    await expect(
      page.getByText('You have unsaved edits to this review’s draft'),
    ).toBeVisible()

    await page.getByRole('button', { name: 'Keep editing' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)
    await expect(editor).toHaveValue(`${bodyOf(first)}\n\n${TYPED}`)

    await jump(page, entries)
    await page.getByRole('button', { name: 'Discard the edits' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${second}/draft`)
    await page.getByRole('tab', { name: 'Edit' }).click()
    await expect(editor).toHaveValue(bodyOf(second))
    await expect(page.getByRole('button', { name: 'Save the draft' })).toBeDisabled()

    untouched(first, FIRST_DRAFT)
    untouched(second, SECOND_DRAFT)
  })

  /**
   * **A save writes to the review in the address and to no other**, which is the harm.
   *
   * The sentence typed on the second review is discarded on the way out, a different one is
   * typed on the first review and saved, and then both records are read: the first holds
   * what was typed on it, and neither holds what was typed on the other.
   */
  test('a save after moving reaches the review in the address, and no other', async ({
    page,
  }) => {
    await page.goto(seeded)
    await byTheTrail(page, first, 'draft')
    const entries = await byTheTrail(page, second, 'draft')

    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await editor.fill(`${bodyOf(second)}\n\n${TYPED}`)

    await jump(page, -entries)
    await page.getByRole('button', { name: 'Discard the edits' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)

    const saved = `${bodyOf(first)}\n\nTyped on the first review, and saved there.`
    await page.getByRole('tab', { name: 'Edit' }).click()
    await expect(editor).toHaveValue(bodyOf(first))
    await editor.fill(saved)
    await page.getByRole('button', { name: 'Save the draft' }).click()

    /*
     * **The preview, back on Rendered, is what a save that succeeded leaves behind** — the
     * page flips the view itself and only on success, so the editor going is the signal and
     * an assertion on the Save button's state is an assertion about a button that is no
     * longer drawn. Said in the notice too, which is what the developer reads.
     */
    await expect(editor).toHaveCount(0)
    await expect(page.getByText(/^Saved\./)).toBeVisible()

    /*
     * The record, read through the command line, which is how a review session reads it.
     * `trimEnd`, because the store ends a saved body with a newline and the editor's value
     * does not carry one — which is a fact about the store and not about this change.
     */
    expect(bodyOf(first).trimEnd()).toBe(saved)
    expect(bodyOf(first)).not.toContain(TYPED)
    untouched(second, SECOND_DRAFT)
  })

  /**
   * **Typing an address is a new document, and that is the whole answer for it.**
   *
   * The issue lists a typed address beside browser history as a way to reach the defect. It
   * is not one: `ssr: false` and history-mode routing mean the browser fetches the page
   * again, so the component is built from nothing, the editor is seeded from the review in
   * the address, and the unsaved sentence went with the document that held it.
   *
   * **So nothing is asked, and nothing should be.** A guard cannot run — the page it is on
   * is gone — and the only tool that can speak before a document is replaced is
   * `beforeunload`, which is the browser dialog this change exists to avoid. What matters is
   * what the issue actually asks for, and it holds: no text typed on one review reaches
   * another review's draft.
   */
  test('a typed address is a new document, so nothing is carried and nothing is asked', async ({
    page,
  }) => {
    await page.goto(`${seeded}reviews/${first}/draft`)
    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await expect(editor).toHaveValue(bodyOf(first))
    await editor.fill(`${bodyOf(first)}\n\n${TYPED}`)

    /* Typed into the address bar, which is a request for the page rather than a route. */
    await page.goto(`${seeded}reviews/${second}/draft`)

    /*
     * **The editor holding the second review's own body first.** A `toHaveCount(0)` straight
     * after a `goto` is answered by a document that has not booted, so it would pass over a
     * card that appeared on hydration. Seeding the editor from this review's record is the
     * page having run. Raised by a review of this branch.
     */
    await page.getByRole('tab', { name: 'Edit' }).click()
    await expect(editor).toHaveValue(bodyOf(second))
    await expect(page.getByRole('button', { name: 'Keep editing' })).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Save the draft' })).toBeDisabled()

    untouched(first, FIRST_DRAFT)
    untouched(second, SECOND_DRAFT)
  })

  /**
   * **The card comes up over the Rendered preview too, and Keep editing opens the editor.**
   *
   * `dirty` is about what is in the textarea, not about which tab is showing: a developer
   * who typed, switched back to read the preview and then pressed a link has unsaved edits
   * and cannot see them. The card has to appear, and the way back has to be to the editor.
   */
  test('the card comes up from the preview, and Keep editing opens the editor', async ({
    page,
  }) => {
    await page.goto(seeded)
    await byTheTrail(page, first, 'draft')

    await page.getByRole('tab', { name: 'Edit' }).click()
    await page.locator('textarea#draft-body').fill(`${bodyOf(first)}\n\n${TYPED}`)

    /* Back to reading it, which is where the card now has to find the developer. */
    await page.getByRole('tab', { name: 'Rendered' }).click()
    await expect(page.locator('textarea#draft-body')).toHaveCount(0)

    await page.locator(`.trail a[href="/reviews/${first}/code-review"]`).click()
    await expect(page.getByRole('button', { name: 'Keep editing' })).toBeVisible()

    await page.getByRole('button', { name: 'Keep editing' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)
    await expect(page.locator('textarea#draft-body'))
      .toHaveValue(`${bodyOf(first)}\n\n${TYPED}`)

    untouched(first, FIRST_DRAFT)
  })

  /**
   * **"Skip to the page" is not leaving, and must not be asked about.**
   *
   * The layout's skip link is `<a href="#main">`, and a fragment is the one navigation that
   * changes the address without changing the page. If the router saw it as an update, a
   * keyboard user with unsaved edits would be stopped by a card every time they skipped the
   * header — the guard turning an accessibility control into an interruption.
   *
   * **It did, until f64fec0.** The reasoning that said it could not — a plain anchor's
   * fragment jump is the browser's, and the router listens for `popstate` — is wrong:
   * vue-router 5 sees it, `onBeforeRouteUpdate` fired, and this test failed on its first
   * run. The guards compare `to.path` to `from.path` now, and this is what holds them to
   * it. Do not read the first paragraph as a reason the check is redundant; it is the
   * reason the check exists.
   *
   * The Back press is asserted too, because that one *is* a `popstate`.
   */
  test('skipping to the page with unsaved edits asks nothing', async ({ page }) => {
    await page.goto(`${seeded}reviews/${first}/draft`)
    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await editor.fill(`${bodyOf(first)}\n\n${TYPED}`)

    const skip = page.getByRole('link', { name: 'Skip to the page' })
    await skip.focus()
    await page.keyboard.press('Enter')

    /*
     * **The exact address, and it is not a synchronisation barrier — read on.**
     *
     * The first version of this comment claimed the fragment reaching the address was the
     * navigation having been handled. It is not: vue-router 5 attaches only `popstate` and
     * `pagehide`, and its one `preventDefault` is `RouterLink`'s, so a plain anchor's
     * fragment jump is performed by the browser and the router hears about it afterwards.
     * The address is therefore answered before any guard runs. Measured, after a review of
     * this branch disputed the claim: deleting this line and reverting the guard fails the
     * test in exactly the same place, which is what shows the line was never carrying it.
     *
     * So the negative below is bounded by the expect timeout and nothing more — it holds
     * because Vue renders the card well inside ten seconds, not because anything waits for
     * it. What this line is still worth keeping for is the `goBack` leg: asserting the
     * exact address rather than a regular expression is what makes "the fragment is gone"
     * an assertion at all, and the pattern it replaced matched `…/draft#main` too.
     */
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft#main`)
    await expect(page.getByRole('button', { name: 'Keep editing' })).toHaveCount(0)
    await expect(editor).toHaveValue(`${bodyOf(first)}\n\n${TYPED}`)

    /* And back out of the fragment, which the router does see. */
    await page.goBack()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)
    await expect(page.getByRole('button', { name: 'Keep editing' })).toHaveCount(0)
    await expect(editor).toHaveValue(`${bodyOf(first)}\n\n${TYPED}`)

    untouched(first, FIRST_DRAFT)
  })

  /**
   * **Leaving the draft for another step of the same review asks as well.**
   *
   * The other guard: another page is a *leave*, not an update, and the edits are as lost
   * going there as they are going to another review — the component is taken down either
   * way. One review is involved, so nothing here can be written to the wrong draft; what is
   * being asserted is that the developer is asked before their typing is thrown away.
   */
  test('leaving the draft for another step of the review asks before it goes', async ({
    page,
  }) => {
    await page.goto(seeded)
    await byTheTrail(page, first, 'draft')

    await page.getByRole('tab', { name: 'Edit' }).click()
    const editor = page.locator('textarea#draft-body')
    await editor.fill(`${bodyOf(first)}\n\n${TYPED}`)

    const codeReview = page.locator(`.trail a[href="/reviews/${first}/code-review"]`)
    await codeReview.click()
    await expect(page.getByRole('button', { name: 'Keep editing' })).toBeVisible()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)

    await page.getByRole('button', { name: 'Keep editing' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/draft`)
    await expect(editor).toHaveValue(`${bodyOf(first)}\n\n${TYPED}`)

    await codeReview.click()
    await page.getByRole('button', { name: 'Discard the edits' }).click()
    await expect(page).toHaveURL(`${seeded}reviews/${first}/code-review`)

    /* And nothing the developer typed reached the record. */
    untouched(first, FIRST_DRAFT)
  })
})
