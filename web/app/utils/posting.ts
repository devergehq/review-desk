/*
 * The posting the draft page waits for: the intervals it is timed against, and where the
 * posted review is on GitHub.
 *
 * Nothing here reads or writes a record, as nothing in `utils/` does. Auto-imported, so a
 * screen calls these by name.
 */

import type { Review } from '~/types/desk'

/**
 * How often the review session's watcher reads Review Desk while a draft is waiting.
 *
 * **Locus owns this number and Review Desk is never told it.** Locus 0.5.12 made the
 * session's watcher read the desk every 20 seconds while a draft waits on the developer,
 * and nothing on the wire carries that: `GET /reviews/{id}` has no field for the schedule
 * of a system this server does not talk to, and inventing one would be this app claiming
 * to know something only Allele knows.
 *
 * So it is a setting of this page, written down beside where it came from. If Locus
 * changes its watcher, this line is wrong and is the only line that is — which is the
 * whole reason it is a named constant rather than "about half a minute" in a sentence.
 */
export const WATCHER_READS_EVERY_SECONDS = 20

/**
 * How long a signed-off draft may sit before the page says the session may not be running.
 *
 * **Two of the watcher's intervals, which is DEV-932's own rule.** One would accuse a
 * healthy session every time a sign-off landed just after a read: the worst honest case is
 * a whole interval of waiting and then the posting itself. Two leaves that case room and
 * still answers inside a minute — which is what the developer who waited 4 min 28 s with
 * nothing on screen was asking for.
 *
 * It is a *statement about the watcher*, not a timeout: nothing here cancels anything, and
 * the draft stays signed off and postable for as long as it takes.
 */
export const POSTED_WITHIN_SECONDS = WATCHER_READS_EVERY_SECONDS * 2

/**
 * How often the draft page re-reads a review it is waiting on.
 *
 * Shorter than the watcher's own interval, because this page reports on something it does
 * not control: re-reading every 20 seconds would add up to 20 seconds of its own to
 * whatever the session takes, and the complaint this answers was about not knowing.
 *
 * The cost is two reads of loopback SQLite every five seconds, and only while a draft is
 * signed off, unposted and on screen.
 */
export const RE_READ_EVERY_SECONDS = 5

/** Where a posted review is, and whether that address is the review or its pull request. */
export interface PostedReview {
  href: string
  /** Whether it lands on the review itself rather than on the top of the pull request. */
  exact: boolean
}

/**
 * The posted review's address on GitHub, built from what the record holds.
 *
 * **`github_review_id` is a `ForeignId`: opaque, stored and returned and never parsed.**
 * GitHub's own address for a review is the pull request plus `#pullrequestreview-<id>`,
 * where the id is the review's database number. A session that reported a GraphQL node id
 * — `PRR_kwDOA1…`, which is what the fixtures hold — has given this page an id that
 * fragment cannot be built from, and a browser handed an anchor that matches nothing lands
 * silently at the top of the pull request. That is a link claiming to go somewhere it does
 * not.
 *
 * So a run of digits becomes the anchor and anything else addresses the pull request,
 * where the review is, with the screen saying which it got. **Deciding by shape is not
 * parsing the id**: nothing is decoded, nothing is assumed about what is inside it, and an
 * id this rule cannot place is still shown to the developer rather than dropped.
 *
 * Null when the record holds no id at all, which is every review until a session has run
 * `review-desk draft posted`.
 */
export function postedReview(
  review: Pick<Review, 'repo' | 'pr' | 'github_review_id'>,
): PostedReview | null {
  const id = review.github_review_id?.trim()
  if (!id) return null

  const pull = `https://github.com/${review.repo}/pull/${review.pr}`
  return /^[0-9]+$/.test(id)
    ? { href: `${pull}#pullrequestreview-${id}`, exact: true }
    : { href: pull, exact: false }
}
