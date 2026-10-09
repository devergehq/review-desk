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

const { seeded, empty, seededDatabase, reviewId, waivedReviewId } = addresses

/**
 * The layer trail on the One review screen, one string per layer.
 *
 * Waited for rather than read straight away. The screen's first read is a request, so a
 * `goto` followed immediately by `allTextContents` returns an empty list — which fails as
 * `undefined` on an index and points at the trail rather than at the timing.
 */
async function trail(page: import('@playwright/test').Page): Promise<string[]> {
  const layers = page.locator('ol[aria-label="The seven layers of this review"] li')
  await expect(layers).toHaveCount(7)
  return layers.allTextContents()
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
  test('the inbox lists the review, with its title and nothing from a fixture', async ({ page }) => {
    await page.goto(seeded)

    await expect(page.getByRole('heading', { name: 'Waiting on you' })).toBeVisible()

    /*
     * The seeded review, under the title the command line gave it. "That database's data",
     * which is the first half of the criterion.
     */
    await expect(page.getByText('Confirm the problem · 1')).toBeVisible()
    await expect(page.getByText('ABC-231')).toBeVisible()
    /*
     * The session wrote the draft without waiting, so the review is two items: a brief to
     * confirm and a draft to approve. Both are waiting on the developer, which is why the
     * title is on screen twice and the brief item says "nothing posts" rather than
     * "nothing else runs".
     */
    await expect(page.getByText('Retry failed webhook deliveries')).toHaveCount(2)
    await expect(page.getByText('nothing posts until you confirm')).toBeVisible()
    await expect(page.getByText('2 items across 1 repository')).toBeVisible()

    /*
     * And the second half: no marker, because these are real records. The marker is the one
     * thing on screen that says otherwise, and a production build has no fixtures for it to
     * be about.
     */
    await expect(page.getByText('Sample data')).toHaveCount(0)
    await expect(page.getByText('acme/portal')).toHaveCount(2)
  })

  test('the brief is confirmed in the browser, and the record says the developer did it', async ({
    page,
  }) => {
    /* Nobody has confirmed it: the command line has no way to. One read, two fields. */
    const before = reviewRecord()
    expect(before.brief.confirmed_at).toBeNull()
    expect(before.brief.confirmed_by).toBeNull()

    await page.goto(`${seeded}reviews/${reviewId}`)
    await expect(page.getByText('Confirm the problem before anything is posted')).toBeVisible()
    await expect(
      page.getByText('A webhook delivery that fails once is never tried again'),
    ).toBeVisible()

    /*
     * Layer 3's one thing: the alternatives, written before the diff was read, with the one
     * the author built marked. `brief put` writes them — `NewBrief` carries
     * `options: Vec<BriefOption>` — and this is the only screen that shows them.
     */
    await expect(page.getByRole('heading', { name: 'Ways to solve it' })).toBeVisible()
    await expect(page.getByText('written before the diff was read')).toBeVisible()
    await expect(page.getByText('A · Retry inside the request')).toBeVisible()
    await expect(page.getByText('B · Queue with backoff')).toBeVisible()
    await expect(page.getByText('C · Dead-letter and replay by hand')).toBeVisible()
    /* One of the three is what they built, and exactly one. */
    await expect(page.getByText('What they built')).toHaveCount(1)
    await expect(page.getByText('B is the right shape for this ticket.')).toBeVisible()

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
    await expect(page.getByText('Sign off a draft · 1')).toBeVisible()
    await expect(page.getByText('Confirm the problem · 1')).toHaveCount(0)
    await expect(page.getByText('1 item across 1 repository')).toBeVisible()
    await expect(page.getByText('1 blocker, 1 should')).toBeVisible()
    await expect(page.getByText('1 finding held back for a ticket')).toBeVisible()

    await page.getByRole('link', { name: 'Open draft' }).first().click()
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

    await expect(page.getByText(note)).toBeVisible()

    await page.goto(`${seeded}reviews/${reviewId}`)
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
    await page.goto(`${seeded}reviews/${reviewId}`)
    const layers = await trail(page)
    expect(layers[5]).toContain('approved by the developer')

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

  test('the review screen shows the findings and what was not checked', async ({ page }) => {
    await page.goto(`${seeded}reviews/${reviewId}`)

    await expect(page.getByText('2 in the draft · 1 held back · 0 suppressed')).toBeVisible()
    await expect(
      page.getByText('A retried delivery is sent without an idempotency key'),
    ).toBeVisible()
    await expect(page.getByText('jobs/DeliverWebhook.php:58')).toBeVisible()
    await expect(
      page.getByText('The admin screens, which this change does not touch.'),
    ).toBeVisible()
    await expect(page.getByText('214 changed lines')).toBeVisible()

    /* The seven layers, as the command line set them. */
    await expect(page.locator('ol li')).toHaveCount(7)
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

    await page.goto(`${seeded}reviews/${waivedReviewId}`)
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
    await expect(page.locator('select#repo')).toContainText('acme/portal')
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await expect(page.getByText('Sample data')).toHaveCount(0)
  })
})

test.describe('a database with nothing in it', () => {
  test('all four pages read as new rather than as broken', async ({ page }) => {
    await page.goto(empty)
    await expect(page.getByRole('heading', { name: 'Nothing here yet' })).toBeVisible()
    await expect(page.getByText('This database holds no reviews')).toBeVisible()
    /* The one thing that fills it, with the field to do it from. */
    await expect(page.locator('input#pr-link-empty')).toBeVisible()

    await page.goto(`${empty}rules`)
    await expect(page.getByRole('heading', { name: 'No rules yet' })).toBeVisible()
    await expect(page.getByText('A rule comes out of a finding you kept')).toBeVisible()

    /*
     * A review and a draft that are not there. `not_found` and an empty review are
     * different answers, and the screen says the first rather than drawing the second.
     */
    await page.goto(`${empty}reviews/1`)
    await expect(page.getByText('Not on the desk')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible()

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
    await page.goto(`${seeded}reviews/${reviewId}`)
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
    await page.locator('select#repo').selectOption('all')

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
