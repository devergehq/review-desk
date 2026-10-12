import { describe, expect, it } from 'vitest'

import {
  POSTED_WITHIN_SECONDS,
  RE_READ_EVERY_SECONDS,
  WATCHER_READS_EVERY_SECONDS,
  postedReview,
} from '../app/utils/posting'
import { secondsSince } from '../app/utils/format'
import type { RepoName } from '../app/types/desk'

/*
 * The draft page's settings, and the one rule it has about an address it did not invent.
 *
 * These are the numbers DEV-932's "after a stated time" is stated in, so the relation
 * between them is the thing worth holding: the stated time is two of the watcher's
 * intervals, and the page reads faster than the watcher does. A test on the literal 40
 * would pass while the relation was broken.
 */

describe('the intervals the draft page is timed against', () => {
  it('states the watcher Locus runs, which nothing on the wire reports', () => {
    expect(WATCHER_READS_EVERY_SECONDS).toBe(20)
  })

  it('waits two of those intervals before doubting the session', () => {
    expect(POSTED_WITHIN_SECONDS).toBe(WATCHER_READS_EVERY_SECONDS * 2)
  })

  it('re-reads faster than the watcher reads, so it adds no wait of its own', () => {
    expect(RE_READ_EVERY_SECONDS).toBeLessThan(WATCHER_READS_EVERY_SECONDS)
  })

  it('answers inside a minute, which is what the complaint was about', () => {
    expect(POSTED_WITHIN_SECONDS).toBeLessThanOrEqual(60)
  })
})

describe('where the posted review is on GitHub', () => {
  const place = { repo: 'acme/portal' as const, pr: 412 }

  it('anchors a numeric review id, which is what GitHub builds its own address from', () => {
    expect(postedReview({ ...place, github_review_id: '1842003517' })).toEqual({
      href: 'https://github.com/acme/portal/pull/412#pullrequestreview-1842003517',
      exact: true,
    })
  })

  it('sends a GraphQL node id to the pull request, and says it is not the review', () => {
    /*
     * The id is opaque and is never parsed, so the node id cannot become an anchor — and an
     * anchor that matches nothing lands silently at the top of the pull request while the
     * link claims to be the review. The pull request is where the review is; `exact: false`
     * is how the screen knows to say so.
     */
    expect(postedReview({ ...place, github_review_id: 'PRR_kwDOA1' })).toEqual({
      href: 'https://github.com/acme/portal/pull/412',
      exact: false,
    })
  })

  it('treats the command line’s own shape the same way', () => {
    /* `review-desk draft posted --github-review gh-review-3312`, which the README shows. */
    expect(postedReview({ ...place, github_review_id: 'gh-review-3312' })?.exact).toBe(false)
  })

  it('has nowhere to send a review nothing has reported posted', () => {
    expect(postedReview({ ...place, github_review_id: null })).toBeNull()
  })

  it('treats an id of nothing but space as no id at all', () => {
    expect(postedReview({ ...place, github_review_id: '   ' })).toBeNull()
  })

  it('draws nothing for a repository no address can be built from', () => {
    /*
     * **`prUrl`'s rule, borrowed rather than re-stated.** `acme/portal#x` is storable on
     * a Review Desk released before DEV-936, and `../..` was storable until its own
     * review — a browser resolves that to `github.com/pull/1`, which is a link whose text
     * and destination name two different things. No link beats a wrong one.
     */
    expect(postedReview({ repo: '../..' as RepoName, pr: 1, github_review_id: '184' }))
      .toBeNull()
    expect(postedReview({ repo: 'acme/portal#x' as RepoName, pr: 1, github_review_id: '184' }))
      .toBeNull()
  })

  it('trims an id before it decides, so a stray newline still anchors', () => {
    expect(postedReview({ ...place, github_review_id: '1842003517\n' })?.exact).toBe(true)
  })
})

describe('the arithmetic the watch is timed by', () => {
  const at = '2026-10-12T10:00:00Z'

  it('counts whole seconds since an instant', () => {
    expect(secondsSince(at, new Date('2026-10-12T10:00:41Z'))).toBe(41)
  })

  it('answers null where there is no instant, which is not nought seconds', () => {
    expect(secondsSince(null, new Date(at))).toBeNull()
    expect(secondsSince(undefined, new Date(at))).toBeNull()
  })

  it('never reads as the future, so a browser clock behind the server’s says nought', () => {
    expect(secondsSince(at, new Date('2026-10-12T09:59:55Z'))).toBe(0)
  })
})
