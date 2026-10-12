// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import type { VueWrapper } from '@vue/test-utils'

import PostingWatch from '../app/components/PostingWatch.vue'
import { POSTED_WITHIN_SECONDS, RE_READ_EVERY_SECONDS } from '../app/utils/posting'

import type { Draft, DraftStatus, Instant, Review, ReviewEvent } from '../app/types/desk'

/*
 * `PostingWatch`, mounted — the gap between a sign-off and a posting, which is the whole
 * of DEV-932.
 *
 * **Mounted on its own rather than through the draft page.** The thing under test is a
 * timer and the words around it, and reaching it through the page would mean driving two
 * reads, a composable and a `DeskState` to get at a `setInterval`. The page's own wiring is
 * checked where it can only be checked — in a real browser, against the real binary, with
 * the posting driven from the command line: `e2e/review-desk.spec.ts`.
 *
 * Time is faked for all of it. Every assertion here is about a number of seconds, and
 * waiting out forty of them in a unit test would make the suite slower than the thing it
 * measures.
 */

const SIGNED_OFF_AT = '2026-10-12T10:00:00Z'

/** The clock both the browser and these fixtures start from. */
const START = new Date(SIGNED_OFF_AT)

const REVIEW: Review = {
  id: 17,
  repo: 'acme/portal',
  pr: 412,
  head_sha: '3f9c2ab',
  round: 1,
  title: 'Retry failed webhook deliveries',
  ticket_id: 'ABC-231',
  changed_lines: 214,
  session_id: 'ses-9',
  ledger_id: null,
  github_review_id: null,
  created_at: '2026-10-12T09:40:00Z',
  pr_state: 'open',
  harvested_at: null,
}

function draft(over: Partial<Draft> = {}): Draft {
  return {
    review_id: 17,
    body: 'The approach fits the ticket.',
    status: 'approved',
    event: 'request_changes',
    approved_at: SIGNED_OFF_AT,
    sent_back_note: null,
    sent_back_at: null,
    posted_at: null,
    updated_at: SIGNED_OFF_AT,
    ...over,
  }
}

interface Props {
  draft: Draft
  review: Review
  lastActivityAt: Instant | null | undefined
  bodyIsStale: boolean
}

function props(over: Partial<Props> = {}): Props {
  return {
    draft: draft(),
    review: REVIEW,
    lastActivityAt: SIGNED_OFF_AT,
    bodyIsStale: false,
    ...over,
  }
}

/**
 * Every `refresh` the component has asked for.
 *
 * A listener rather than `page.emitted()`, because one of these tests unmounts the
 * component and then asks what it did — and an unmounted wrapper has no record of its own
 * emits left to read, so the assertion passed by being unanswerable.
 */
let heard = vi.fn()

/** Mounted attached to the document, so `visibilityState` and the live region are real. */
async function watching(over: Partial<Props> = {}) {
  return mountSuspended(PostingWatch, {
    props: { ...props(over), onRefresh: heard },
    attachTo: document.body,
  })
}

/** How many times it has asked its page to read again. */
function reads(_page: VueWrapper): number {
  return heard.mock.calls.length
}

/** Move the browser's clock on, letting every due interval fire and Vue re-render. */
async function tick(page: VueWrapper, seconds: number) {
  await vi.advanceTimersByTimeAsync(seconds * 1000)
  await page.vm.$nextTick()
}

/** What the tab did, as the page sees it: the state, and the event that reports it. */
async function visibility(page: VueWrapper, state: 'hidden' | 'visible') {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true })
  document.dispatchEvent(new Event('visibilitychange'))
  await page.vm.$nextTick()
}

/** The one element a screen reader is told about. */
function announced(page: VueWrapper): string {
  return page.find('[role="status"]').text()
}

beforeEach(() => {
  heard = vi.fn()
  vi.useFakeTimers()
  vi.setSystemTime(START)
  Object.defineProperty(document, 'visibilityState', {
    value: 'visible',
    configurable: true,
  })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('while a signed-off draft is waiting', () => {
  it('re-reads the review on a timer, with nothing reloaded', async () => {
    const page = await watching()
    expect(reads(page)).toBe(0)

    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(1)

    await tick(page, RE_READ_EVERY_SECONDS * 3)
    expect(reads(page)).toBe(4)
  })

  it('says how long ago the sign-off was, down to the second', async () => {
    const page = await watching()
    expect(page.text()).toContain('Signed off')
    expect(page.text()).toContain('just now')

    await tick(page, 25)
    /*
     * Seconds, not "just now". The whole question while a draft waits is whether twenty
     * seconds have gone or fifty, and `sinceLabel`'s wording — right everywhere a time is
     * read at a glance — cannot tell those apart.
     */
    expect(page.text()).toContain('25 sec ago')
  })

  it('says when anything last wrote to this review', async () => {
    const page = await watching({ lastActivityAt: '2026-10-12T09:58:00Z' })
    expect(page.text()).toContain('Last written to')
    expect(page.text()).toContain('2 min ago')
  })

  it('says a review nothing has written to in words, never as a figure', async () => {
    const page = await watching({ lastActivityAt: null })
    expect(page.text()).toContain('Last written to')
    expect(page.text()).toContain('Not recorded')
    /* A nought is a measurement; the absence of one is not. */
    expect(page.text()).not.toContain('0 sec ago')
  })

  it('says the review’s own session posts it, and that Review Desk never does', async () => {
    const page = await watching()
    expect(page.text()).toContain('This review’s own session posts it')
    expect(page.text()).toContain('never writes to GitHub')
  })

  it('says it is not told whether that session is running', async () => {
    /*
     * Allele owns session liveness and tells this server nothing, so the page says that
     * rather than inferring it from silence. DEV-932: "Where the record cannot say whether
     * the session is alive, it says that, not a guess."
     */
    const page = await watching()
    expect(page.text()).toContain('not something Review Desk is told')
  })

  it('states the number it is timed against, rather than "about half a minute"', async () => {
    const page = await watching()
    expect(page.text()).toContain(`within about ${POSTED_WITHIN_SECONDS} seconds`)
  })
})

describe('when nothing has posted it', () => {
  it('says nothing is wrong before the watcher’s two intervals are up', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS - RE_READ_EVERY_SECONDS)
    expect(page.text()).not.toContain('may not be running')
  })

  it('says the session may not be running once they are', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('may not be running')
    expect(announced(page)).toContain('may not be running')
  })

  it('names what the developer can do about it', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('Check the session')
    expect(page.text()).toContain('take the sign-off back')
  })

  it('keeps reading, because the draft is still signed off and still postable', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS)
    const sofar = reads(page)
    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(sofar + 1)
  })
})

describe('when the session posts it', () => {
  const posted = (over: Partial<Draft> = {}, review: Partial<Review> = {}) => ({
    draft: draft({
      status: 'posted' as DraftStatus,
      posted_at: '2026-10-12T10:00:54Z',
      ...over,
    }),
    review: { ...REVIEW, github_review_id: '1842003517', ...review },
  })

  it('flips to posted and stops reading, with no reload', async () => {
    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(1)

    vi.setSystemTime(new Date('2026-10-12T10:00:54Z'))
    await page.setProps(posted())
    expect(announced(page)).toBe('Posted on GitHub.')

    await tick(page, RE_READ_EVERY_SECONDS * 4)
    expect(reads(page)).toBe(1)
  })

  it('says when, as which event, and how long the session took', async () => {
    vi.setSystemTime(new Date('2026-10-12T10:02:54Z'))
    const page = await watching(posted())
    expect(page.text()).toContain('Posted')
    expect(page.text()).toContain('2 min ago')
    expect(page.text()).toContain('Request changes')
    expect(page.text()).toContain('Taken to post')
    expect(page.text()).toContain('54 sec')
  })

  it('links the review itself when the record holds the number GitHub uses', async () => {
    const page = await watching(posted())
    const link = page.find('a')
    expect(link.attributes('href')).toBe(
      'https://github.com/acme/portal/pull/412#pullrequestreview-1842003517',
    )
    expect(link.text()).toContain('See the review on GitHub')
  })

  it('links the pull request for a node id, and says why it is not the review', async () => {
    const page = await watching(posted({}, { github_review_id: 'PRR_kwDOA1' }))
    const link = page.find('a')
    expect(link.attributes('href')).toBe('https://github.com/acme/portal/pull/412')
    expect(link.text()).toContain('See the pull request on GitHub')
    expect(page.text()).toContain('not the number GitHub builds its own address')
  })

  it('draws no link at all when nothing reported which review it became', async () => {
    const page = await watching(posted({}, { github_review_id: null }))
    expect(page.find('a').exists()).toBe(false)
    expect(page.text()).toContain('Nothing reported which review on GitHub this became')
  })

  it('says a posting with no sign-off stamp without inventing a span', async () => {
    const page = await watching(posted({ approved_at: null }))
    expect(page.text()).toContain('Posted')
    expect(page.text()).not.toContain('Taken to post')
  })
})

describe('when the developer takes the sign-off back', () => {
  it('stops reading and says the draft is with the session again', async () => {
    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS)

    /* What `sendBackDraft` leaves: the status, the note, and no sign-off. */
    await page.setProps({
      draft: draft({
        status: 'sent_back',
        event: null,
        approved_at: null,
        sent_back_note: 'F2 is the same point as F1',
        sent_back_at: '2026-10-12T10:00:05Z',
      }),
    })

    expect(announced(page)).toContain('The sign-off is taken back')
    expect(page.text()).not.toContain('Waiting for this review')

    await tick(page, RE_READ_EVERY_SECONDS * 4)
    expect(reads(page)).toBe(1)
  })

  it('says nothing about a sign-off for a draft that never had one', async () => {
    /*
     * An ordinary send-back, from `ready`. The record cannot tell it from a take-back
     * afterwards — a send-back clears `approved_at` with the event — so what this page
     * says is what this page watched happen.
     */
    const page = await watching({
      draft: draft({ status: 'ready', event: null, approved_at: null }),
    })
    await page.setProps({
      draft: draft({
        status: 'sent_back',
        event: null,
        approved_at: null,
        sent_back_note: 'Cut F2',
      }),
    })
    expect(announced(page)).toBe('')
  })
})

describe('the states it has nothing to add to', () => {
  it('says nothing and starts no timer on a draft waiting on the developer', async () => {
    const page = await watching({
      draft: draft({ status: 'ready', event: null, approved_at: null }),
    })
    expect(announced(page)).toBe('')
    expect(page.text()).not.toContain('Signed off')

    await tick(page, RE_READ_EVERY_SECONDS * 4)
    expect(reads(page)).toBe(0)
  })

  it('waits on nothing for a signed-off draft the hand-over will refuse', async () => {
    /*
     * `body_is_stale`: the body is no longer what the blocks compose to, so no session can
     * post it however long this watched, and "the session may not be running" would be the
     * wrong diagnosis of a state whose cause is known. The sentence above the sign-off
     * names the way out, and taking it is an act on this page.
     */
    const page = await watching({ bodyIsStale: true })
    expect(announced(page)).toBe('')
    await tick(page, POSTED_WITHIN_SECONDS * 2)
    expect(reads(page)).toBe(0)
    expect(page.text()).not.toContain('may not be running')
  })
})

describe('a hidden tab', () => {
  it('stops reading a review nobody is looking at', async () => {
    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(1)

    await visibility(page, 'hidden')
    await tick(page, RE_READ_EVERY_SECONDS * 5)
    expect(reads(page)).toBe(1)
  })

  it('reads once the moment it is looked at again, rather than waiting an interval', async () => {
    const page = await watching()
    await visibility(page, 'hidden')
    await tick(page, 600)

    await visibility(page, 'visible')
    /*
     * The read on becoming visible is the point of handling this rather than letting the
     * browser throttle the interval: a developer back at the tab after ten minutes is
     * looking at a ten-minute-old answer.
     */
    expect(reads(page)).toBe(1)
    expect(page.text()).toContain('10 min ago')

    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(2)
  })

  it('starts no timer at all when it is mounted hidden', async () => {
    Object.defineProperty(document, 'visibilityState', {
      value: 'hidden',
      configurable: true,
    })
    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS * 4)
    expect(reads(page)).toBe(0)
  })
})

describe('what it leaves behind', () => {
  it('takes its timer with it when the page goes', async () => {
    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(1)

    page.unmount()
    await vi.advanceTimersByTimeAsync(RE_READ_EVERY_SECONDS * 5 * 1000)
    /*
     * An interval left running keeps reading a review nobody has open — and on this app it
     * would keep reading after the developer navigated to another review's draft.
     */
    expect(reads(page)).toBe(1)
  })

  it('stops listening for the tab as well', async () => {
    const page = await watching()
    page.unmount()
    await visibility(page, 'hidden')
    await visibility(page, 'visible')
    expect(reads(page)).toBe(0)
  })
})

describe('what a screen reader is told', () => {
  it('holds one polite region, which moves nothing', async () => {
    const page = await watching()
    const region = page.find('[role="status"]')
    expect(region.exists()).toBe(true)
    /* `role="status"` is polite and takes no focus; the developer keeps what they had. */
    expect(document.activeElement).toBe(document.body)
    expect(page.findAll('[role="status"], [role="alert"], [aria-live]')).toHaveLength(1)
  })

  it('is in the page before there is anything to announce', async () => {
    /*
     * A live region added at the same moment as its content is one a screen reader may not
     * announce at all, so the region is drawn on every status — empty on the ones with
     * nothing to say.
     */
    const page = await watching({
      draft: draft({ status: 'ready', event: null, approved_at: null }),
    })
    expect(page.find('[role="status"]').exists()).toBe(true)
    expect(announced(page)).toBe('')
  })

  it('carries the state and never a figure, so a moving clock is not an interruption', async () => {
    const page = await watching()
    const first = announced(page)
    expect(first).toContain('Waiting for this review')

    await tick(page, 25)
    /*
     * Twenty-five seconds on, the figures have moved and the announcement has not. A
     * region holding "25 sec ago" re-announces itself every time the clock does, which on
     * a four-minute wait is fifty interruptions of whatever was being read.
     */
    expect(page.text()).toContain('25 sec ago')
    expect(announced(page)).toBe(first)
    expect(announced(page)).not.toMatch(/\d/)
  })
})
