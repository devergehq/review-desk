import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import * as api from '../app/data/api'
import { DeskRequestError, asDeskError } from '../app/data/api'
import type { ErrorCode } from '../app/types/desk'

/*
 * `app/data/api.ts`: the sixteen operations as HTTP.
 *
 * **Every error code in the contract is reached here**, which is one of DEV-862's
 * acceptance criteria and could not be met any other way: the server produces one code per
 * situation and most of those situations cannot be set up from a browser. A stubbed `fetch`
 * can produce all six, plus the two the contract cannot describe — nothing answering at
 * all, and an answer that is not this contract's shape.
 *
 * What it does not test is whether the server really sends those codes in those
 * situations. `crates/server/tests/` does that, against a real socket; this is about what
 * the dashboard makes of each one.
 */

/** What the last `fetch` was called with. */
let calls: Array<{ url: string, init: RequestInit }> = []

/** Answer the next request with this status and body. */
function answering(status: number, body: unknown, asText?: string) {
  vi.stubGlobal('fetch', vi.fn((url: string, init: RequestInit) => {
    calls.push({ url: String(url), init })
    return Promise.resolve({
      ok: status >= 200 && status < 300,
      status,
      text: () => Promise.resolve(asText ?? JSON.stringify(body)),
    } as Response)
  }))
}

/** Answer nothing: the server is not running, or the port moved. */
function answeringNothing() {
  vi.stubGlobal('fetch', vi.fn((url: string, init: RequestInit) => {
    calls.push({ url: String(url), init })
    return Promise.reject(new TypeError('Failed to fetch'))
  }))
}

beforeEach(() => {
  calls = []
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('where it sends things', () => {
  it('calls the contract’s own paths, relative to whatever served the page', async () => {
    answering(200, { totals: { items: 0, repos: 0 } })
    await api.getInbox()
    /*
     * Relative, with no host. The binary serves the dashboard and the API on one origin, so
     * this is correct with no configuration — and it is what keeps the request same-origin,
     * which is what the server's guard requires.
     */
    expect(calls[0]!.url).toBe('/api/v1/inbox')
  })

  it.each([
    ['getReview', () => api.getReview(17), 'GET', '/api/v1/reviews/17'],
    ['getDraft', () => api.getDraft(17), 'GET', '/api/v1/reviews/17/draft'],
    ['listRepos', () => api.listRepos(), 'GET', '/api/v1/repos'],
    ['listRules (all)', () => api.listRules(null), 'GET', '/api/v1/rules'],
    ['confirmBrief', () => api.confirmBrief(17), 'POST', '/api/v1/reviews/17/brief/confirm'],
    [
      'setFindingDisposition',
      () => api.setFindingDisposition(17, 4, 'held_back', 'a nit'),
      'PATCH',
      '/api/v1/reviews/17/findings/4',
    ],
    ['saveDraft', () => api.saveDraft(17, 'body', []), 'PUT', '/api/v1/reviews/17/draft'],
    [
      'approveDraft',
      () => api.approveDraft(17, 'comment'),
      'POST',
      '/api/v1/reviews/17/draft/approve',
    ],
    [
      'sendBackDraft',
      () => api.sendBackDraft(17, 'a note'),
      'POST',
      '/api/v1/reviews/17/draft/send-back',
    ],
    [
      'requestNitTicket',
      () => api.requestNitTicket(17),
      'POST',
      '/api/v1/reviews/17/held-back/ticket',
    ],
    ['promoteRule', () => api.promoteRule(9, 'lint'), 'POST', '/api/v1/rules/9/promote'],
    ['dismissRule', () => api.dismissRule(9, 'no'), 'POST', '/api/v1/rules/9/dismiss'],
    ['retireRule', () => api.retireRule(9, 'gone'), 'POST', '/api/v1/rules/9/retire'],
    ['deferRule', () => api.deferRule(9, 60), 'POST', '/api/v1/rules/9/defer'],
    ['startReview', () => api.startReview('https://x/y/pull/1'), 'POST', '/api/v1/reviews'],
  ])('%s is %s %s', async (_name, call, method, path) => {
    answering(200, { repos: [] })
    await call()
    expect(calls[0]!.init.method).toBe(method)
    expect(calls[0]!.url).toBe(path)
  })

  it('leaves the repository off rather than sending it empty', async () => {
    answering(200, {})
    await api.listRules(null)
    /* `?repo=` is the picker's "All repositories", and the server reads empty as absent. */
    expect(calls[0]!.url).toBe('/api/v1/rules')

    calls = []
    await api.listRules('acme/portal')
    expect(calls[0]!.url).toBe('/api/v1/rules?repo=acme%2Fportal')
  })

  it('sends only the link when starting a review', async () => {
    answering(201, { id: 1 })
    await api.startReview('https://github.com/acme/portal/pull/412')
    /*
     * `head_sha` is the server's to resolve, along with the title and the changed-line
     * count. Sending a guess would be worse than sending none.
     */
    expect(JSON.parse(String(calls[0]!.init.body))).toEqual({
      pr_url: 'https://github.com/acme/portal/pull/412',
    })
  })

  it('sends JSON on a write, which is what a plain form could not', async () => {
    answering(200, {})
    await api.approveDraft(17, 'approve')
    const headers = calls[0]!.init.headers as Record<string, string>
    /*
     * The server refuses a write that is not `application/json`, and that check is what
     * stops an HTML form on another page making one: a form can send only three media
     * types and none of them is JSON. If this header goes, every write 400s — which is
     * the failure this assertion exists to make loud rather than mysterious.
     */
    expect(headers['content-type']).toBe('application/json')
  })

  it('sends no credentials, because there is nothing to authenticate with', async () => {
    answering(200, {})
    await api.getInbox()
    expect(calls[0]!.init.credentials).toBe('omit')
  })

  it('unwraps the repository list, which the contract serves as an object', async () => {
    answering(200, { repos: [{ repo: 'acme/portal', open_reviews: 1, active_rules: 4 }] })
    await expect(api.listRepos()).resolves.toEqual([
      { repo: 'acme/portal', open_reviews: 1, active_rules: 4 },
    ])
  })
})

describe('every error code in the contract', () => {
  /*
   * The six the contract declares, each with the HTTP status the server serves it as and
   * the situation it means. A code that stopped being reachable, or gained a seventh
   * sibling, fails this table rather than going unnoticed.
   */
  const CODES: Array<[ErrorCode, number, string]> = [
    ['bad_request', 400, 'this operation needs a JSON body and none was sent'],
    ['invalid_value', 400, '"nonsense" is not a commit hash'],
    ['not_found', 404, 'no review with id 99'],
    ['conflict', 409, 'this draft is approved and cannot be edited'],
    ['github_unavailable', 502, 'gh is not installed'],
    ['store_unavailable', 500, 'the database could not be opened'],
  ]

  it.each(CODES)('%s is read off the answer', async (code, status, message) => {
    answering(status, { code, message })

    const thrown = await api.getReview(99).catch(error => error)
    expect(thrown).toBeInstanceOf(DeskRequestError)

    const desk = asDeskError(thrown)
    expect(desk.code).toBe(code)
    expect(desk.message).toBe(message)
    expect(desk.status).toBe(status)
  })

  it('keeps `field` and `expected`, which is the whole use of invalid_value', async () => {
    answering(400, {
      code: 'invalid_value',
      message: '"nonsense" is not a commit hash',
      field: 'head_sha',
      expected: '7 to 40 lower-case hexadecimal characters',
    })

    const desk = asDeskError(await api.startReview('x').catch(error => error))
    expect(desk.field).toBe('head_sha')
    /* A caller that rejects input has to say what it would have taken, and show it. */
    expect(desk.expected).toBe('7 to 40 lower-case hexadecimal characters')
  })

  it('reads the error object itself, not a document wrapping one', async () => {
    /*
     * Every error response in `openapi.yaml` is `$ref: Error` and the examples under
     * `contract/examples/errors/` are bare objects. The command line wraps its failures in
     * `{"error": …}` because its own contract says so; this is a different contract.
     */
    answering(404, { error: { code: 'not_found', message: 'wrapped' } })
    const desk = asDeskError(await api.getReview(99).catch(error => error))
    expect(desk.code).toBe('unreadable')
  })
})

describe('the two states the contract cannot describe', () => {
  it('nothing answering is unreachable, and names the command that fixes it', async () => {
    answeringNothing()

    const desk = asDeskError(await api.getInbox().catch(error => error))
    /*
     * Not `store_unavailable`. That code means Review Desk is broken; this means it is not
     * running, and telling a developer their database is unreadable when they simply have
     * not started the server would send them to fix the wrong thing.
     */
    expect(desk.code).toBe('unreachable')
    expect(desk.message).toBe('Nothing is listening on this address.')
    expect(desk.status).toBeNull()
  })

  it('an answer that is not this contract’s shape is unreadable', async () => {
    /* A proxy's own error page, or something else listening on the port. */
    answering(502, null, '<html><body>Bad Gateway</body></html>')

    const desk = asDeskError(await api.getInbox().catch(error => error))
    expect(desk.code).toBe('unreadable')
    expect(desk.message).toContain('Something other than Review Desk')
  })

  it('a success that is not JSON is unreadable too', async () => {
    answering(200, null, 'not json at all')
    const desk = asDeskError(await api.getInbox().catch(error => error))
    expect(desk.code).toBe('unreadable')
  })

  it('refuses to trust a code the contract does not declare', async () => {
    /*
     * Something else answering with a plausible-looking error document would otherwise be
     * rendered as if Review Desk had said it — including, in the worst case, as a refusal
     * the developer then tried to work around.
     */
    answering(418, { code: 'teapot', message: 'I am a teapot' })
    const desk = asDeskError(await api.getInbox().catch(error => error))
    expect(desk.code).toBe('unreadable')
    expect(desk.message).not.toContain('teapot')
  })

  it('gives up on a request that never answers, and says so differently', async () => {
    /*
     * Measured, not imagined: `nuxt dev` forwards the API to the binary, and with the
     * binary stopped that forward hangs rather than refusing — thirty seconds and still
     * waiting. Without a deadline the screen's answer to "the server is not running" is a
     * loading state that never ends.
     */
    vi.useFakeTimers()
    vi.stubGlobal('fetch', vi.fn((_url: string, init: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener('abort', () =>
          reject(new DOMException('timed out', 'TimeoutError')))
      })))

    const reading = api.getInbox().catch(error => asDeskError(error))
    await vi.advanceTimersByTimeAsync(11_000)
    const desk = await reading
    vi.useRealTimers()

    expect(desk.code).toBe('unreachable')
    expect(desk.message).toContain('Nothing answered within 10 seconds')
    /* And not "nothing is listening", because something is — it is simply not answering. */
    expect(desk.message).not.toContain('Nothing is listening')
  })

  it('waits longer for the one operation that reads GitHub', async () => {
    vi.useFakeTimers()
    vi.stubGlobal('fetch', vi.fn((_url: string, init: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener('abort', () =>
          reject(new DOMException('timed out', 'TimeoutError')))
      })))

    const starting = api
      .startReview('https://github.com/acme/portal/pull/412')
      .catch(error => asDeskError(error))

    /*
     * Still waiting at eleven seconds, where a read would have given up: `gh` on a cold
     * network takes longer than any of the others, and reporting a server that is not
     * answering while it reads GitHub would be wrong about which thing is slow.
     */
    await vi.advanceTimersByTimeAsync(11_000)
    let settled = false
    void starting.then(() => {
      settled = true
    })
    await vi.advanceTimersByTimeAsync(0)
    expect(settled).toBe(false)

    await vi.advanceTimersByTimeAsync(20_000)
    const desk = await starting
    vi.useRealTimers()

    expect(desk.message).toContain('Nothing answered within 30 seconds')
  })

  it('turns anything else thrown into an error a screen can render', () => {
    const desk = asDeskError(new Error('something nobody planned for'))
    expect(desk.code).toBe('unreachable')
    expect(desk.message).toContain('Nothing is listening')
  })
})

describe('the health endpoint', () => {
  it('is how the retry asks whether the server is back', async () => {
    answering(200, { status: 'ok', schema_version: 4, front_end_embedded: true })
    await expect(api.health()).resolves.toMatchObject({ status: 'ok', schema_version: 4 })
    expect(calls[0]!.url).toBe('/api/v1/health')
  })
})
