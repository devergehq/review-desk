/*
 * Who may use the development proxy.
 *
 * **This exists because the proxy opened a hole, and closing it is the proxy's job.**
 *
 * `nuxt dev` forwards `/api/v1` to a running `review-desk serve`, and it has to rewrite two
 * headers to do so: the server refuses a request whose `Host` is not the address it bound
 * and whose `Origin`, when present, is not its own, and a browser on `localhost:3000` sends
 * both wrong. `changeOrigin` and `headers.origin` state what the proxy is really talking to.
 *
 * The cost of that was measured rather than imagined. A page on `https://example.com`, in a
 * real Chrome, posted a cross-origin form at `localhost:3000/api/v1/reviews/1/draft/approve`
 * — and the proxy satisfied the guard's first two checks on its behalf. Only the third, that
 * a write is `application/json`, stood between a foreign page and approving a draft that a
 * review session then posts to GitHub under the developer's name.
 *
 * So the proxy refuses what the server would have refused, before forwarding anything. The
 * substitution the server sees is then true of the request as well as of the proxy: it came
 * from the dashboard's own origin.
 *
 * **`crates/server/src/guard.rs` is untouched and is still the boundary.** This is not a
 * second one; it is the proxy declining to speak for a caller it should not. A test in
 * `test/dev-proxy.spec.ts` fails if this filter is removed.
 */

/** What the proxy decided about one request, and why. */
export interface ProxyDecision {
  /** Whether to forward it to `review-desk serve`. */
  forward: boolean
  /** Why not, for the body of the refusal. Absent when it is forwarded. */
  reason?: string
}

/**
 * Whether the dev proxy may forward a request.
 *
 * `host` is the request's own `Host` — the address the dev server is answering on — so this
 * needs no configuration and holds on whatever port or name `nuxt dev` is told to use. It is
 * the same reasoning `guard.rs` uses: the origin a page was served from is the only one the
 * proxy is entitled to speak for.
 *
 * | `origin` | Decision |
 * |---|---|
 * | absent | forward — not a browser, or a same-origin `GET`. The Fetch standard sends an `Origin` on every request that is not `GET` or `HEAD`, so a *write* with none did not come from a page |
 * | the dev server's own | forward — this is the dashboard |
 * | `null` | refuse — a sandboxed frame or a `file://` page; it names no origin this can recognise |
 * | anything else | refuse |
 */
export function mayForward(
  origin: string | undefined | null,
  host: string | undefined,
): ProxyDecision {
  if (origin == null || origin === '') return { forward: true }

  if (host == null || host === '') {
    return {
      forward: false,
      reason:
        'this request named an origin but no host, so there is nothing to check it against',
    }
  }

  const own = [`http://${host}`, `https://${host}`]
  if (own.some(mine => origin.toLowerCase() === mine.toLowerCase())) {
    return { forward: true }
  }

  return {
    forward: false,
    reason:
      `the development proxy forwards the dashboard's own origin and no other, and this `
      + `request came from ${JSON.stringify(origin)}. Review Desk's API is reachable only `
      + `from the page it serves.`,
  }
}
