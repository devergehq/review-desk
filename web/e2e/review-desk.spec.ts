import { rmSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { expect, test } from '@playwright/test'

import { cli, cliJson, readAddresses, serversUnderTest, stillServing } from './harness'

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

const addresses = readAddresses()

const {
  seeded,
  empty,
  seededDatabase,
  reviewId,
  waivedReviewId,
  structuredReviewId,
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
     * **Two, and they are two reviews rather than one review twice.** The structured
     * review gained a draft in DEV-878 — the code review page's decisions change a
     * finding's *block*, so a review with no draft has nothing to decide against — and its
     * own layer trail already claimed one by saying step 6 was waiting on the developer.
     */
    await expect(page.getByRole('heading', { name: '2 reviews need you' })).toBeVisible()

    /*
     * **DEV-876's first acceptance criterion, on a real database.** This session wrote the
     * draft without waiting for the brief to be confirmed, so the review is waiting on the
     * developer for two things at once — and `GET /inbox` really does serve it in two of
     * its five arrays. One row, showing the earlier layer's action, saying the other is
     * behind it. The screen this replaced drew the title twice.
     */
    await expect(page.getByText('Retry failed webhook deliveries')).toHaveCount(1)
    await expect(page.locator('.row')).toHaveCount(2)
    await expect(page.getByText('Problem brief ready')).toBeVisible()
    await expect(page.getByText('nothing posts until you confirm')).toBeVisible()
    await expect(page.getByText('a draft is ready behind it')).toBeVisible()
    await expect(page.getByText('Read the brief')).toBeVisible()
    await expect(page.getByText('ABC-231')).toBeVisible()

    /*
     * **And the row opens the review, which starts at Understand** — Patrick on
     * tc-portal #9584, "when we click a review, we want to go to the review, not to the
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

    /* The one group the board draws that the API cannot fill says so, in those words. */
    await expect(page.getByRole('heading', { name: 'Waiting on the author' })).toBeVisible()
    await expect(
      page.getByText('serves no group for a posted review awaiting a reply'),
    ).toBeVisible()

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
     * **Still two rows, and the same two.** What moved is this one's next action: from the
     * brief to the draft, because layer 2 is settled and layer 6 is not. That is what "one
     * row showing its next action" buys — a review waiting for two things does not become
     * two rows, and does not stop being a row when one of them is answered.
     */
    await expect(page.locator('.row')).toHaveCount(2)
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
     * Patrick, on tc-portal #9584: "when we click a review, we want to go to the review,
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
    await expect(
      page.getByText('The delivery log prints `null` for a latency it never measured'),
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
    await expect(page.getByText('26 files')).toBeVisible()
    await expect(page.getByText('959 lines added and 423 removed')).toBeVisible()
    await expect(page.locator('.part')).toHaveCount(6)
    /* A part the review found nothing in says so. */
    await expect(page.getByText('Nothing found here')).toBeVisible()

    /*
     * A part opens to its files and the findings joined to it **by file** — the one place
     * a finding is shown beside the part of the change it is about.
     */
    await page.getByRole('button', { name: /Allowed columns on the report definition/ }).click()
    const part = page.locator('#part-2')
    await expect(part).toBeVisible()
    await expect(part).toContainText('app/Reporting/ColumnPolicy.php')
    await expect(part).toContainText('F6')
    await expect(part).toContainText('F9')

    /* A file the pull request never changed says so, rather than showing two noughts. */
    await page.getByRole('button', { name: /One filter chain for both paths/ }).click()
    await expect(page.locator('#part-3')).toContainText('not changed by this pull request')

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

    /* Lint and Harvest are labels, not links: neither has a page of its own. */
    await expect(page.locator('.trail').getByRole('link', { name: /1 Lint/ })).toHaveCount(0)
    await expect(page.locator('.trail').getByRole('link', { name: /7 Harvest/ })).toHaveCount(0)
  })

  /**
   * **A long step note is two lines on the trail, and the rest opens in place.**
   *
   * DEV-892's defect 1, measured where it can be: a clamp is layout, and the unit tests
   * run in jsdom, which computes none. So this reads the rendered height of step 2's
   * note — the 45-word one the invented example carries — against its own line height,
   * presses the control, and reads it again.
   *
   * **The duration under it is measured too.** The clamp is on the note alone and a rule
   * that ate the figure beneath it would break what DEV-890 put there, which is the one
   * way this fix could be worse than the defect.
   */
  test('a long step note is clamped to two lines and opens whole', async ({ page }) => {
    await page.goto(`${seeded}reviews/${structuredReviewId}/understand`)
    await expect(page.locator('h1')).toBeVisible()

    const step = page.locator('.steps li').nth(1)
    await expect(step).toContainText('2 Understand')

    const note = step.locator('.outcome')
    const clamped = await note.evaluate((element) => {
      const style = getComputedStyle(element)
      return {
        height: element.getBoundingClientRect().height,
        lineHeight: Number.parseFloat(style.lineHeight) || Number.parseFloat(style.fontSize) * 1.2,
        scrollHeight: element.scrollHeight,
      }
    })
    /*
     * Two lines and a pixel of rounding. `scrollHeight` is what the note *would* be, and
     * it has to be more than what is drawn — otherwise this test would pass against a
     * short note and prove nothing about the clamp.
     */
    expect(
      clamped.height,
      `the note is drawn ${clamped.height}px tall at a ${clamped.lineHeight}px line`,
    ).toBeLessThanOrEqual(clamped.lineHeight * 2 + 1)
    expect(
      clamped.scrollHeight,
      'the note is longer than the two lines drawn, so the clamp is doing something',
    ).toBeGreaterThan(clamped.height + 1)

    /*
     * **The duration keeps its own line under the clamped note**, whole and not clipped.
     * The seeded record writes each step's state once through the command line, so it has a
     * finish and no start and the figure reads "time not recorded" — which is DEV-890's own
     * answer for that and is exactly as long as a figure would be. What is asserted is that
     * the line is there, is not empty, and is drawn below the note rather than inside it.
     */
    const took = step.locator('.took')
    await expect(took).toBeVisible()
    await expect(took).not.toHaveText('')
    const stacked = await step.evaluate((element) => {
      const note = element.querySelector('.outcome')!.getBoundingClientRect()
      const figure = element.querySelector('.took')!.getBoundingClientRect()
      return { below: figure.top >= note.bottom - 1, height: Math.round(figure.height) }
    })
    expect(stacked.below, 'the figure is under the note, not inside it').toBe(true)
    expect(stacked.height, 'the figure has a line of its own').toBeGreaterThan(0)

    /* Opened by keyboard, which is the acceptance criterion's own word. */
    const control = step.getByRole('button', { name: /Read the rest/ })
    await control.focus()
    await page.keyboard.press('Enter')
    await expect(step.getByRole('button', { name: /Fold/ })).toBeVisible()

    const opened = await note.evaluate(element => element.getBoundingClientRect().height)
    expect(opened, 'the whole note is drawn once it is opened').toBeGreaterThan(clamped.height)
    await expect(note).toContainText('the options pass was asked for anything at all')
    /* And the figure is still there, still on its own line, under a note four times taller. */
    await expect(took).toBeVisible()
    expect(
      await step.evaluate((element) => {
        const note = element.querySelector('.outcome')!.getBoundingClientRect()
        const figure = element.querySelector('.took')!.getBoundingClientRect()
        return figure.top >= note.bottom - 1 && figure.height > 0
      }),
      'the figure survives the note being opened',
    ).toBe(true)
  })

  /**
   * **Every part card's head reads the same way, and a row of them lines up.**
   *
   * DEV-892's defect 5, measured where it can be. A title that did not fit beside its
   * marks went under them, so on one grid some cards read "2 · the title" across and others
   * put the number on one line and the title on the next; and every card took its own
   * height, so a row of three stood at three heights with holes between them.
   *
   * Both are layout, so both are read off the rendered boxes: the title's left edge is past
   * the marks' right edge in every card that has marks, and every card sharing a row shares
   * a height.
   */
  test('a part card’s title is beside its marks, and a row of cards lines up', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto(`${seeded}reviews/${structuredReviewId}/solution`)
    await expect(page.locator('h1')).toBeVisible()
    await expect(page.locator('.part').first()).toBeVisible()

    const cards = await page.locator('.part').evaluateAll(elements =>
      elements.map((element) => {
        const box = element.getBoundingClientRect()
        const marks = element.querySelector('.part-marks')?.getBoundingClientRect() ?? null
        const title = element.querySelector('.part-title')!.getBoundingClientRect()
        return {
          top: Math.round(box.top),
          height: Math.round(box.height),
          beside: marks == null ? null : title.left >= marks.right,
          /* The title's first line sits against the marks rather than below them. */
          level: marks == null ? null : title.top < marks.bottom,
        }
      }),
    )

    expect(cards.length, 'the example describes the change in several parts')
      .toBeGreaterThan(3)
    expect(
      cards.filter(card => card.beside !== null).length,
      'some of them fix a numbered problem',
    ).toBeGreaterThan(0)

    for (const [at, card] of cards.entries()) {
      if (card.beside === null) continue
      expect(card.beside, `part ${at + 1}'s title is beside its marks`).toBe(true)
      expect(card.level, `part ${at + 1}'s title is not under its marks`).toBe(true)
    }

    /* Grouped by the row they are drawn on; a row of cards is one height. */
    const rows = new Map<number, number[]>()
    for (const card of cards) {
      rows.set(card.top, [...(rows.get(card.top) ?? []), card.height])
    }
    expect([...rows.values()].some(row => row.length > 1), 'the grid draws a row of cards')
      .toBe(true)
    for (const [top, heights] of rows) {
      expect(new Set(heights).size, `the cards on the row at ${top}px are one height`).toBe(1)
    }
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

test.describe('a database with nothing in it', () => {
  test('all four pages read as new rather than as broken', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await expect(page.getByText('This database holds no reviews')).toBeVisible()
    /* The one thing that fills it, with the field to do it from. */
    await expect(page.locator('input#start-empty')).toBeVisible()

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
