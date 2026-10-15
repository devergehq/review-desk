import { describe, expect, it } from 'vitest'

// @ts-expect-error — a plain .mjs script with no types; the three exports are the point.
import { mayShoot, nameFor, routesFor } from '../scripts/shoot.mjs'

/*
 * **The screenshot script's guard, which exists because something already went wrong.**
 *
 * `scripts/shoot.mjs` walks every screen of a running desk. On 12 October 2026 a build
 * session wrote a review into `~/.review-desk/review-desk.db`, the database the developer's
 * own desk on port 7777 serves — so this tool, of all of them, must not be pointable at it.
 *
 * The refusal is a function returning a decision rather than a `process.exit` inline, for
 * the reason `dev-proxy.ts` is a module: so that this file fails if it is removed. The
 * other two exports are here because a before-and-after pair is only comparable if both
 * sides name their files the same way and visit the same routes.
 */

describe('what the shooter refuses', () => {
  it('refuses the desk the developer is using', () => {
    const decision = mayShoot('http://127.0.0.1:7777')

    expect(decision.shoot).toBe(false)
    expect(decision.reason).toContain('7777')
    expect(decision.reason).toContain('~/.review-desk/review-desk.db')
  })

  /*
   * The host is not the fact; the port is. A check written against one spelling of
   * loopback is one an `/etc/hosts` entry or a `localhost` habit walks straight past.
   */
  it.each(['http://localhost:7777', 'http://0.0.0.0:7777/', 'http://[::1]:7777'])(
    'refuses it however the host is spelled: %s',
    (base) => {
      expect(mayShoot(base).shoot).toBe(false)
    },
  )

  it('allows the port the design loop serves its own copy on', () => {
    expect(mayShoot('http://127.0.0.1:7790').shoot).toBe(true)
  })

  /*
   * A URL with no port means its scheme's default, which is 80 and not 7777. The guard
   * compares what the address means rather than the characters in it, so this has to pass
   * — and would not if the comparison were `url.port === '7777'` against an empty string
   * coerced to a number.
   */
  it('allows an address with no port at all', () => {
    expect(mayShoot('http://127.0.0.1').shoot).toBe(true)
  })

  it('refuses something that is not a URL rather than shooting it', () => {
    const decision = mayShoot('127.0.0.1:7790')

    expect(decision.shoot).toBe(false)
    expect(decision.reason).toContain('is not a URL')
  })
})

describe('what the two sides of a pair have in common', () => {
  /*
   * The names are how `before/` and `after/` pair up. The home route has no path to name
   * it, and `_home` is what `baseline-shots-2026-10-15/` already calls that file.
   */
  it('names a picture after its route', () => {
    expect(nameFor('desk', '/reviews/36/draft')).toBe('desk_reviews_36_draft.png')
    expect(nameFor('phone', '/rules')).toBe('phone_rules.png')
    expect(nameFor('desk', '/')).toBe('desk_home.png')
  })

  it('visits the three screens and all seven steps of every review it is given', () => {
    const routes = routesFor(['36', '1'])

    expect(routes.slice(0, 3)).toEqual(['/', '/reviews', '/rules'])
    expect(routes).toHaveLength(3 + 2 * 7)
    expect(routes).toContain('/reviews/36/what-happened')
    expect(routes).toContain('/reviews/1/understand')
  })
})
