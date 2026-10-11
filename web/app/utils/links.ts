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
 * GitHub's address for a pull request.
 *
 * **Built from the two fields, never stored.** A stored copy could disagree with the
 * `repo` and `pr` it was built from, and those two are the review's identity — the store
 * normalises them and the contract constrains `repo` to
 * `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`. So there is no free text in here and nothing a
 * caller can put in the address.
 *
 * **Written as a function because `//` cannot go in a component's interpolated
 * attribute.** `vue-tsc` emits a component's props as one line, so the `//` in `https://`
 * starts a JavaScript line comment and everything after it in the template is commented
 * out: on the queue that was `TS2304: Cannot find name` seventeen times over. Measured
 * rather than guessed — the same attribute with one slash reports 0 errors and with two
 * reports 17. It works inline on a *native* element, which is why a header's own `<a>`
 * could write it; it is here so that no caller has to know which case it is in.
 */
export function prUrl(repo: string, pr: number): string {
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
