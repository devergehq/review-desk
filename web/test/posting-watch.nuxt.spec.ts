// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import type { VueWrapper } from '@vue/test-utils'

import PostingWatch from '../app/components/PostingWatch.vue'
import { POSTED_WITHIN_SECONDS, RE_READ_EVERY_SECONDS } from '../app/utils/posting'

import type { Draft, DraftStatus, Instant, Review } from '../app/types/desk'

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
  recordIsCurrent: boolean
  reread: () => Promise<void>
}

function props(over: Partial<Props> = {}): Props {
  return {
    draft: draft(),
    review: REVIEW,
    lastActivityAt: SIGNED_OFF_AT,
    bodyIsStale: false,
    recordIsCurrent: true,
    reread: heard,
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
let heard = vi.fn<() => Promise<void>>()

/** Mounted attached to the document, so `visibilityState` and the live region are real. */
async function watching(over: Partial<Props> = {}) {
  return mountSuspended(PostingWatch, {
    props: props(over),
    attachTo: document.body,
  })
}

/**
 * Mounted on a draft waiting for the developer, then signed off — which is how a developer
 * reaches `approved`, and the only way the live region has anything to announce.
 *
 * **A test that mounts straight into `approved` is testing a page load, not a sign-off**,
 * and a page load deliberately announces nothing: a live region is for news, and a status
 * this component did not watch arrive is not news. Four assertions here were written
 * against the shorthand and passed on behaviour the sign-off path does not have.
 */
async function signedOff(over: Partial<Props> = {}) {
  const page = await watching({
    ...over,
    draft: draft({ status: 'ready', event: null, approved_at: null }),
  })
  await page.setProps({ draft: over.draft ?? draft() })
  return page
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
  heard = vi.fn<() => Promise<void>>().mockResolvedValue(undefined)
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

  it('says the review’s own session posts it, and that Review Desk does not', async () => {
    const page = await watching()
    expect(page.text()).toContain('This review’s own session posts it, not Review Desk')
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
    const page = await signedOff()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('may not be running')
    expect(announced(page)).toContain('may not be running')
  })

  it('never opens an announcement with the two words three things above it say', async () => {
    /*
     * The heading, the sentence under it and the notice the write leaves all say "Signed
     * off" — and the notice is a live region of its own, so a fourth would be the same two
     * words announced twice in a row.
     */
    const page = await watching()
    expect(announced(page)).not.toContain('Signed off')
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(announced(page)).not.toContain('Signed off')
  })

  it('names what the developer can do about it', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('Check the session')
    expect(page.text()).toContain('take the sign-off back')
  })

  it('says how long they have waited, and states the forty seconds only once', async () => {
    const page = await watching()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('in the 40 sec since you signed off')
    /*
     * The threshold belongs to the paragraph that says what a running session takes.
     * Repeated a line below as what this one has exceeded, it was one number stated twice.
     */
    expect(page.text().match(/40 seconds/g) ?? []).toHaveLength(1)
  })

  it('stops the clock when the answers stop, rather than walking past the threshold', async () => {
    /*
     * **A server that accepts a connection and never answers leaves no error to catch**,
     * which is the failure `crates/server/tests/concurrency.rs` is written for. The flag
     * cannot see it. What can is where the clock is moved: after the answer, so when the
     * answers stop, so does the figure.
     */
    let hang: (() => void) | null = null
    heard.mockImplementation(() => new Promise<void>((settle) => {
      hang = settle
    }))

    const page = await watching()
    await tick(page, RE_READ_EVERY_SECONDS * 20)

    expect(page.text()).not.toContain('may not be running')
    /* And the asks do not stack up behind the one that never came back. */
    expect(reads(page)).toBe(1)

    /* It answers, and the clock catches up to where it really is. */
    hang?.()
    await tick(page, 0)
    expect(page.text()).toContain('may not be running')
  })

  it('announces it on a page that was loaded already signed off', async () => {
    /*
     * **No status change ever happens here**, which is a reload of a draft signed off a
     * moment ago: the poll keeps answering `approved`, so news keyed off the status alone
     * never arrives and the one sentence that sends somebody somewhere was drawn and never
     * spoken. The wait passing the threshold is the change.
     */
    const page = await watching()
    expect(announced(page)).toBe('')

    await tick(page, POSTED_WITHIN_SECONDS)
    expect(announced(page)).toContain('may not be running')
  })

  it('stays silent about a draft that was already late when the page opened', async () => {
    /* Reading a state rather than watching one arrive. Announcing that would be the noise
     * the gate exists to stop. */
    vi.setSystemTime(new Date('2026-10-12T11:00:00Z'))
    const page = await watching()
    expect(page.text()).toContain('may not be running')
    expect(announced(page)).toBe('')
  })

  it('holds the diagnosis through a refused read rather than taking it back', async () => {
    /*
     * **Lateness is a conclusion, not a reading.** Recomputed from the last read it went
     * false the moment a poll was refused and true when the next answered — so the
     * sentence carrying "Check the session… or take the sign-off back" vanished and came
     * back on every hiccup, which is to say it disappeared exactly when something was
     * wrong. Drawn once and held, nothing retracts it but the draft ceasing to wait.
     */
    const page = await signedOff()
    await tick(page, POSTED_WITHIN_SECONDS)
    expect(page.text()).toContain('may not be running')

    await page.setProps({ recordIsCurrent: false })
    expect(page.text()).toContain('may not be running')

    await page.setProps({ recordIsCurrent: true })
    expect(page.text()).toContain('may not be running')
  })

  it('does not read the diagnosis out again when a read comes back', async () => {
    /* A live region that retracts and repeats itself is one a screen reader reads twice. */
    const page = await signedOff()
    await tick(page, POSTED_WITHIN_SECONDS)
    const spoken = announced(page)
    expect(spoken).toContain('may not be running')

    await page.setProps({ recordIsCurrent: false })
    expect(announced(page)).toBe(spoken)
    await page.setProps({ recordIsCurrent: true })
    expect(announced(page)).toBe(spoken)
  })

  it('draws no conclusion it could not have reached, on a page opened already late', async () => {
    vi.setSystemTime(new Date('2026-10-12T11:00:00Z'))
    const page = await watching()
    expect(page.text()).toContain('may not be running')
    /* Shown, because it is true; not announced, because this page did not watch it arrive. */
    expect(announced(page)).toBe('')

    await page.setProps({ recordIsCurrent: false })
    await page.setProps({ recordIsCurrent: true })
    expect(announced(page)).toBe('')
  })

  it('never concludes lateness from a record it could not reach', async () => {
    /*
     * The server stops ten seconds after the sign-off. The clock walks past forty, but no
     * read has answered in that time, so there is nothing to conclude it from.
     */
    const page = await signedOff()
    await page.setProps({ recordIsCurrent: false })
    await tick(page, POSTED_WITHIN_SECONDS * 3)
    expect(page.text()).not.toContain('may not be running')
  })

  it('says nothing of the sort before the page has read the record', async () => {
    /*
     * **The worst sentence this page could say.** A failed read leaves the last good
     * answer on screen — `DeskState` draws "Review Desk is not answering" over it — and
     * the clock keeps moving, so without this the panel would accuse the review session of
     * being down because *Review Desk* is down, and send the developer to the wrong
     * system.
     */
    const page = await signedOff({ recordIsCurrent: false })
    await tick(page, POSTED_WITHIN_SECONDS * 3)
    expect(page.text()).not.toContain('may not be running')
    expect(announced(page)).toBe('Waiting for this review’s session to post it.')

    /* And it says it the moment the record is current again and still unposted. */
    await page.setProps({ recordIsCurrent: true })
    expect(page.text()).toContain('may not be running')
  })

  it('keeps reading while it cannot read, because that is how it recovers', async () => {
    const page = await watching({ recordIsCurrent: false })
    await tick(page, RE_READ_EVERY_SECONDS * 3)
    expect(reads(page)).toBe(3)
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
    expect(page.text()).toContain('The record holds no GitHub review for this draft')
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

  it('forgets the sign-off once the session has the draft back and ready again', async () => {
    /*
     * A page left open across a take-back, a rewrite and an ordinary send-back. The flag
     * says what this page last saw, not what it ever saw, so the second send-back is not
     * reported as a sign-off being taken back.
     */
    const page = await watching()
    await page.setProps({ draft: draft({ status: 'sent_back', event: null, approved_at: null }) })
    expect(announced(page)).toContain('The sign-off is taken back')

    await page.setProps({ draft: draft({ status: 'ready', event: null, approved_at: null }) })
    await page.setProps({ draft: draft({ status: 'sent_back', event: null, approved_at: null }) })
    expect(announced(page)).toBe('')
  })

  it('announces nothing about a review it clicked through to that was already late', async () => {
    /*
     * **The clock is frozen for a draft nobody has signed off**, so a page left open on
     * one and then clicked through to a review signed off a minute ago has a stale `now`
     * at the moment the new props land. Judged against it the new draft looks in time,
     * and the accusation is announced a moment later as though it had just arrived.
     */
    const page = await watching({
      draft: draft({ status: 'ready', event: null, approved_at: null }),
    })
    vi.setSystemTime(new Date(START.getTime() + 120_000))

    await page.setProps({
      draft: draft({
        review_id: 15,
        status: 'approved',
        approved_at: new Date(START.getTime() + 60_000).toISOString(),
      }),
    })

    expect(page.text()).toContain('may not be running')
    expect(announced(page)).toBe('')
  })

  it('remembers nothing about another review’s draft', async () => {
    /*
     * One page serves every review, so a draft-to-draft navigation changes these props
     * without mounting anything. A sign-off watched on review 17 is not a sign-off on 15.
     */
    const page = await watching()
    await page.setProps({
      draft: draft({
        review_id: 15,
        status: 'sent_back',
        event: null,
        approved_at: null,
        sent_back_note: 'Cut F2',
      }),
    })
    expect(announced(page)).toBe('')
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
    await tick(page, 0)
    expect(page.text()).toContain('10 min ago')

    await tick(page, RE_READ_EVERY_SECONDS)
    expect(reads(page)).toBe(2)
  })

  it('moves the clock on a posted draft too, which no timer is going to', async () => {
    /*
     * A figure that has stopped is stale, which this desk accepts. One that has stopped
     * across an hour the page was *told* about is wrong — and this panel is the only part
     * of the desk that is told when the tab came back.
     */
    vi.setSystemTime(new Date('2026-10-12T10:00:54Z'))
    const page = await watching({
      draft: draft({ status: 'posted', posted_at: '2026-10-12T10:00:54Z' }),
    })
    expect(page.text()).toContain('just now')

    await visibility(page, 'hidden')
    await tick(page, 3600)
    await visibility(page, 'visible')

    expect(page.text()).toContain('1 hr ago')
    /* And still nothing is being read: a posted draft is not waiting for anything. */
    expect(reads(page)).toBe(0)
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

  it('announces nothing about a draft it did not watch arrive', async () => {
    /*
     * A page *loaded* on a draft posted days ago, or navigated to from one being waited
     * on. The region is in the page and empty: news is what a live region is for, and this
     * is not news. The figures beside it say what the record holds, for anyone reading.
     */
    const page = await watching({
      draft: draft({ status: 'posted', posted_at: '2026-10-12T10:00:54Z' }),
    })
    expect(page.find('[role="status"]').exists()).toBe(true)
    expect(announced(page)).toBe('')
    expect(page.text()).toContain('Posted')

    await page.setProps({ draft: draft({ review_id: 15, status: 'approved' }) })
    expect(announced(page)).toBe('')
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

  it('is announced and never printed, so the late state is not said twice', async () => {
    /*
     * The region is the only part of this panel a sighted reader does not see. Drawn, it
     * said the long wait's diagnosis in bold and the paragraph four lines below said it
     * again — twice in one card, in a picture this branch committed.
     */
    const page = await signedOff()
    await tick(page, POSTED_WITHIN_SECONDS)

    const region = page.find('[role="status"]')
    expect(region.classes()).toContain('rd-visually-hidden')
    expect(region.text()).toContain('may not be running')

    /*
     * The phrase is in the page twice and that is right — once for the ear, once for the
     * eye. What must not happen twice is the *seen* one, so the region's own words come
     * out before the count.
     */
    const seen = page.text().replace(region.text(), '')
    expect(seen.split('may not be running')).toHaveLength(2)
  })

  it('carries the state and never a figure, so a moving clock is not an interruption', async () => {
    const page = await signedOff()
    const first = announced(page)
    expect(first).toBe('Waiting for this review’s session to post it.')

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
