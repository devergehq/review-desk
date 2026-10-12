/*
 * **Where a review's two links point.** Auto-imported, so a screen calls these by name.
 *
 * The pull request's address and the ticket's address arrive by opposite routes, and the
 * difference is the whole content of this module:
 *
 * | Link | Where its address comes from |
 * |---|---|
 * | the pull request | **built** from `repo` and `pr`, which the record has held since its first slice |
 * | the ticket | **stored**, in `ticket_url`, because nothing can build it |
 *
 * `contract/README.md`'s "The two links in a review's header" is the same table in the
 * contract's own words. Neither address is ever read from, and Review Desk asks a tracker
 * nothing: an address is drawn and that is all.
 */

/**
 * GitHub's address for a pull request, or `null` when `repo` is not a repository name.
 *
 * **Built from the two fields, never stored.** A stored copy could disagree with the
 * `repo` and `pr` it was built from, and those two are the review's identity.
 *
 * **And checked here, not assumed from the store.** This used to rest its whole case on
 * the store — "the contract constrains `repo`, so there is nothing a caller can put in the
 * address" — which `ticketUrl` below refuses to do for its own field, in as many words: a
 * column is not a promise about every row in it. Two populations prove the point for this
 * one. Released Review Desk never checked the characters at all, so `acme/portal#x` is
 * storable today and a header built from it links the *repository page*; and until DEV-936's
 * own review, `../..` passed too, which a browser resolves to `github.com/pull/1`. Both are
 * a link whose text and its destination name two different things, which is the defect this
 * feature exists to remove.
 *
 * **A second implementation of one rule, and that is the right number here.** Three copies
 * in the store was the bug — same layer, same language, one of them looser. This is the
 * other side of a boundary the store cannot reach: a row already written, read by a renderer
 * that must not draw it. `normalise::is_repo_name` is the rule; this is the same rule at the
 * only other place it can be enforced.
 *
 * **Written as a function because `//` cannot go in a component's interpolated
 * attribute.** `vue-tsc` emits a component's props as one line, so the `//` in `https://`
 * starts a JavaScript line comment and everything after it in the template is commented
 * out: on the queue that was `TS2304: Cannot find name` seventeen times over. Measured
 * rather than guessed — the same attribute with one slash reports 0 errors and with two
 * reports 17. It works inline on a *native* element, which is why a header's own `<a>`
 * could write it; it is here so that no caller has to know which case it is in.
 */
export function prUrl(repo: string, pr: number): string | null {
  const halves = repo.split('/')
  if (halves.length !== 2) return null
  const usable = (part: string) =>
    part.length > 0 && part !== '.' && part !== '..' && /^[A-Za-z0-9._-]+$/.test(part)
  if (!halves.every(usable)) return null

  return `https://github.com/${repo}/pull/${pr}`
}

/**
 * A review's ticket address, or `null` when there is nothing to draw as a link.
 *
 * **`https` or nothing, checked here as well as at the write.** The store refuses anything
 * else on the way in, which is where it is refused *once* — but a column is not a promise
 * about every row in it. A database can be written to by hand, and the one thing this
 * value is for is reaching an `href`, where a `javascript:` or `data:` address runs. So
 * the renderer refuses it a second time and the header draws the ticket's id as text,
 * which is exactly what it draws for a review that has no address at all.
 *
 * The scheme is matched case-sensitively, as the store stores it. A value the store would
 * have refused is a value this refuses too; nothing here repairs one.
 */
export function ticketUrl(url: string | null | undefined): string | null {
  if (!url) return null
  return url.startsWith('https://') ? url : null
}
