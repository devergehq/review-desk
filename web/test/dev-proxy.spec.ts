import { describe, expect, it } from 'vitest'

import { mayForward } from '../dev-proxy'

/*
 * **The filter on the development proxy, which exists because the proxy opened a hole.**
 *
 * `nuxt dev` forwards `/api/v1` to a running `review-desk serve`, and to do so it rewrites
 * `Host` and `Origin` — the server refuses a request whose `Host` is not the address it
 * bound and whose `Origin`, when present, is not its own, and a browser on `localhost:3000`
 * sends both wrong.
 *
 * That substitution is made on behalf of whoever sent the request. Measured in a real
 * Chrome: a page on `https://example.com` posted a cross-origin form at
 * `localhost:3000/api/v1/reviews/1/draft/approve` and got past two of the guard's three
 * checks, leaving only `Content-Type` between it and approving a draft that a review session
 * posts to GitHub under the developer's name.
 *
 * So the proxy refuses first. **These tests fail if that filter is removed**, which is the
 * reason the decision is a function of its own rather than a line inside `nuxt.config.ts`.
 */

const DEV = 'localhost:3000'

describe('what the development proxy will forward', () => {
  it('forwards a request with no origin at all', () => {
    /*
     * `curl`, a script, a review session — none of them a browser, and refusing them would
     * stop no attacker. The Fetch standard appends an `Origin` to every request whose method
     * is not GET or HEAD, so a *write* with none is not something a page can produce.
     */
    expect(mayForward(undefined, DEV)).toEqual({ forward: true })
    expect(mayForward('', DEV)).toEqual({ forward: true })
  })

  it('forwards the dashboard’s own origin', () => {
    expect(mayForward('http://localhost:3000', DEV)).toEqual({ forward: true })
    expect(mayForward('http://127.0.0.1:3000', '127.0.0.1:3000')).toEqual({ forward: true })
    /* Whatever port or name `nuxt dev` was told to use: the host is read off the request. */
    expect(mayForward('http://localhost:4100', 'localhost:4100')).toEqual({ forward: true })
  })

  it('refuses the attack that was actually made', () => {
    const refused = mayForward('https://evil.example', DEV)
    expect(refused.forward).toBe(false)
    expect(refused.reason).toContain('evil.example')
  })

  it('refuses a null origin, which names no origin it could recognise', () => {
    /* What a sandboxed frame and a `file://` page send, and what an https page sends to an
     * http URL under the default referrer policy — the evil.example-to-localhost case. */
    expect(mayForward('null', DEV).forward).toBe(false)
  })

  it('refuses a host that merely contains the dev server’s', () => {
    /* `localhost:3000.evil.example` and `evil.example/localhost:3000` are not this origin. */
    expect(mayForward('http://localhost:3000.evil.example', DEV).forward).toBe(false)
    expect(mayForward('http://evil.example/localhost:3000', DEV).forward).toBe(false)
    expect(mayForward('http://alocalhost:3000', DEV).forward).toBe(false)
  })

  it('refuses the same name on another port or scheme than the one serving the page', () => {
    expect(mayForward('http://localhost:3001', DEV).forward).toBe(false)
    expect(mayForward('http://localhost', DEV).forward).toBe(false)
  })

  it('accepts either scheme on the host that is serving the page', () => {
    /* `nuxt dev --https` serves the same host over TLS, and the page's origin says so. */
    expect(mayForward('https://localhost:3000', DEV)).toEqual({ forward: true })
  })

  it('refuses when there is an origin but no host to check it against', () => {
    expect(mayForward('https://evil.example', undefined).forward).toBe(false)
  })

  it('compares case-insensitively, as host names are', () => {
    expect(mayForward('HTTP://LOCALHOST:3000', DEV)).toEqual({ forward: true })
  })
})
