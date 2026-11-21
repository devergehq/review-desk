/*
 * The sign-in link: trade its one-time token for the developer's cookie — DEV-1346.
 *
 * `review-desk serve` prints the dashboard's URL with `#sign-in=<token>` on it, to its own
 * terminal and nowhere else. A fragment is never sent to a server, so the token is in no
 * request log; this reads it, takes it out of the address bar before anything else happens
 * — so it is not left in the browser's history even if the trade fails — and trades it at
 * `POST /developer/sign-in`. The answer sets an `HttpOnly; SameSite=Strict` cookie that no
 * script here can read, and every sign-off sends it from then on.
 *
 * **Awaited**, so the first screen cannot offer an act before the cookie is there.
 *
 * A trade that fails says nothing here: the token was spent or is from another run, and the
 * first act the developer tries is refused with the server's own sentence about where a new
 * link comes from — which is the one place they will be looking.
 */
import { signIn } from '~/data/api'

const FRAGMENT = /(?:^#|&)sign-in=([0-9a-f]{32,})(?:&|$)/

export default defineNuxtPlugin(async () => {
  const found = FRAGMENT.exec(window.location.hash)
  if (!found?.[1]) return

  window.history.replaceState(
    window.history.state,
    '',
    `${window.location.pathname}${window.location.search}`,
  )

  try {
    await signIn(found[1])
  }
  catch {
    /* See above: the first refused act says why, in the server's words. */
  }
})
