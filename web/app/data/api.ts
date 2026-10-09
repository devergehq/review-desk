/*
 * The contract's sixteen operations, as sixteen functions.
 *
 * **This file and `sample.ts` are the only two places in `web/` that produce records**, and
 * this is the only one that makes an HTTP request. `test/architecture.spec.ts` holds that.
 *
 * Four reads and twelve writes, named after the `operationId` in `contract/openapi.yaml`,
 * at the paths its `servers` entry gives. Nothing here joins anything: each screen is one
 * GET, which is the contract's own rule and the reason the dashboard needs no client-side
 * model of the record.
 *
 * ## The base URL is relative, and that is the whole deployment story
 *
 * `review-desk serve` serves the dashboard and the API from one binary on one origin, so
 * `/api/v1/inbox` is correct with no configuration — and it keeps the request same-origin,
 * which is what the server's cross-origin guard requires. In development `nuxt dev` serves
 * the dashboard on another port and forwards `/api/v1` to the binary; the proxy is
 * configured in `nuxt.config.ts` and states the `Host` and `Origin` the guard expects. So
 * there is no API host in this file, in an environment variable read at runtime, or in the
 * bundle.
 *
 * ## Nothing here posts to GitHub
 *
 * Approving a draft moves a status. The review session reads the approved draft and posts
 * it under the developer's name — that is the whole point of the preview.
 */

import type { DraftEditToRecord } from './backend'

import { DeskRequestError, NOT_LISTENING, fault } from './fault'

import type {
  ApproveDraftRequest,
  Brief,
  DeferRuleRequest,
  DeskError,
  Disposition,
  DraftDetail,
  Finding,
  Inbox,
  NitTicket,
  PromoteRuleRequest,
  RepoList,
  RepoSummary,
  RequestNitTicketRequest,
  Review,
  ReviewDetail,
  ReviewEvent,
  Rule,
  RuleStage,
  RulesPage,
  SaveDraftRequest,
  SendBackDraftRequest,
  SetFindingDispositionRequest,
  StartReviewRequest,
} from '~/types/desk'

/** Where the API is, from the dashboard. The contract's `servers` path, no host. */
export const API_BASE = '/api/v1'

/** The codes the contract declares, so an answer claiming another one is not trusted. */
const CONTRACT_CODES = new Set([
  'bad_request',
  'invalid_value',
  'not_found',
  'conflict',
  'github_unavailable',
  'store_unavailable',
])

/**
 * How long a request may take before the screen gives up on it.
 *
 * **Not a precaution — a measured failure.** Every operation but one is a handful of SQLite
 * reads over loopback, so ten seconds is enormous for them. But a request can fail to
 * *answer* rather than fail: `nuxt dev` forwards the API to the binary, and when the binary
 * is not running that forward hangs instead of refusing — measured at thirty seconds and
 * still waiting. Without this, the development loop's answer to "the server is not running"
 * is a loading state that never ends, which is the worst of the three states a read has.
 *
 * It is not only the proxy. A server wedged on a lock answers nothing either, and a page
 * that waits forever for it tells the developer nothing at all.
 */
const WAIT = 10_000

/**
 * How long `POST /reviews` may take.
 *
 * Longer, because it is the one operation that leaves the machine: it runs `gh` to read the
 * pull request, and a cold network makes that seconds rather than milliseconds. Giving up on
 * it at ten would report a server that is not answering when it is reading GitHub.
 */
const WAIT_FOR_GITHUB = 30_000

/**
 * A signal that aborts after `wait`.
 *
 * `AbortController` and `setTimeout` rather than `AbortSignal.timeout`, which does the same
 * thing in one line. The one line is not testable: `AbortSignal.timeout` uses a platform
 * timer that a test's clock cannot move, so the deadline could only be checked by waiting
 * out ten real seconds — which is not a test anybody runs.
 */
function deadline(wait: number): { signal: AbortSignal, done: () => void } {
  const controller = new AbortController()
  const timer = setTimeout(
    () => controller.abort(new DOMException('timed out', 'TimeoutError')),
    wait,
  )
  return { signal: controller.signal, done: () => clearTimeout(timer) }
}

/** One request, with the contract's refusal shape read back out of the answer. */
async function send<T>(
  method: string,
  path: string,
  body?: unknown,
  wait: number = WAIT,
): Promise<T> {
  let response: Response
  const limit = deadline(wait)

  try {
    response = await fetch(`${API_BASE}${path}`, {
      method,
      /*
       * **The write's content type is load-bearing, not decoration.** The server refuses a
       * write that is not `application/json`, which is what stops a plain HTML form on
       * another page making one — a form can send only three media types and none of them
       * is JSON. Sent on every request so that the header cannot be forgotten on the one
       * write that matters.
       */
      headers: { 'content-type': 'application/json', 'accept': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      /*
       * No credentials and no cross-origin anything: this is the same origin as the page,
       * and the server sends no CORS headers because there is no origin it wants to be
       * readable from.
       */
      credentials: 'omit',
      signal: limit.signal,
    })
  }
  catch {
    /*
     * Told apart, because they send the developer to different places. Nothing answering is
     * a server that is not running; nothing answering *in time* is one that is running and
     * stuck, or a proxy in front of one that is not.
     */
    const timedOut = limit.signal.aborted
    limit.done()
    throw new DeskRequestError(fault(
      'unreachable',
      /*
       * The fact, and nothing about what to do with it. Whatever shows this adds that —
       * `DeskState` puts the command beside the message as something to copy, and a notice
       * on a refused write says it in words. Saying it here as well read as a stammer:
       * "Review Desk is not answering. Review Desk is not answering on this address."
       */
      timedOut
        ? `Nothing answered within ${Math.round(wait / 1000)} seconds, so Review Desk may `
        + 'be stuck — or something in front of it may be waiting on a server that is not '
        + 'running.'
        : NOT_LISTENING,
    ))
  }
  /*
   * **The deadline stays armed until the body is read, not until the headers arrive.** A
   * server that sends a status line and then stalls is exactly as unhelpful as one that
   * sends nothing, and clearing the timer here would have let that one hang forever —
   * which is the failure the deadline was added for, surviving in the half of the request
   * nobody was looking at.
   *
   * Cleared on **both** ways out of the fetch above, not only on the one that reaches here:
   * a failed request that left its timer armed would leak one per read, and a page opened
   * against a server that is not running makes several at once.
   */
  let text: string
  try {
    text = await response.text()
  }
  catch {
    throw new DeskRequestError(fault(
      'unreachable',
      `Review Desk answered ${response.status} and then stopped sending within `
      + `${Math.round(wait / 1000)} seconds.`,
      response.status,
    ))
  }
  finally {
    limit.done()
  }

  if (response.ok) {
    if (text.length === 0) return undefined as T
    try {
      return JSON.parse(text) as T
    }
    catch {
      throw new DeskRequestError(fault(
        'unreadable',
        'The answer was not the JSON this dashboard expects. Something other than '
        + 'Review Desk may be answering on this address.',
        response.status,
      ))
    }
  }

  /*
   * A refusal. Every error response in the contract is the `Error` object itself rather
   * than a document wrapping one, so this reads `code` and `message` straight off the
   * body — and refuses to trust a `code` the contract does not declare, because a proxy's
   * own error page would otherwise be rendered as if Review Desk had said it.
   */
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  }
  catch {
    parsed = null
  }

  const document = (parsed ?? {}) as Partial<DeskError>
  const code = typeof document.code === 'string' && CONTRACT_CODES.has(document.code)
    ? document.code as DeskError['code']
    : 'unreadable'

  throw new DeskRequestError({
    code,
    message:
      code === 'unreadable'
        ? `Review Desk answered ${response.status} in a shape this dashboard does not `
        + 'know. Something other than Review Desk may be answering on this address.'
        : (document.message ?? `Review Desk answered ${response.status}.`),
    field: document.field ?? null,
    expected: document.expected ?? null,
    status: response.status,
  })
}

/* ------------------------------- The four reads ------------------------------- */

/** `GET /inbox` — the whole Inbox screen in one call. */
export function getInbox(): Promise<Inbox> {
  return send<Inbox>('GET', '/inbox')
}

/** `GET /reviews/{reviewId}` — one review whole. */
export function getReview(reviewId: number): Promise<ReviewDetail> {
  return send<ReviewDetail>('GET', `/reviews/${reviewId}`)
}

/** `GET /reviews/{reviewId}/draft` — the draft preview in one call. */
export function getDraft(reviewId: number): Promise<DraftDetail> {
  return send<DraftDetail>('GET', `/reviews/${reviewId}/draft`)
}

/**
 * `GET /rules?repo=` — the Rules screen for one repository, or for every one.
 *
 * The repository is left off rather than sent empty: `?repo=` is the picker's "All
 * repositories" and the server reads an empty value as absent, but saying it twice is one
 * place for the two to disagree.
 */
export function listRules(repo: string | null): Promise<RulesPage> {
  const query = repo == null ? '' : `?repo=${encodeURIComponent(repo)}`
  return send<RulesPage>('GET', `/rules${query}`)
}

/** `GET /repos` — the repository picker's options. */
export async function listRepos(): Promise<RepoSummary[]> {
  const answer = await send<RepoList>('GET', '/repos')
  return answer.repos
}

/* ------------------------------ The twelve writes ----------------------------- */

/**
 * `POST /reviews` — start a review from a pull request link.
 *
 * Only `pr_url` is sent. The screen has no head commit to give, and the server resolves
 * the head, the title and the changed-line count by reading the pull request before it
 * creates anything. That read is a read; nothing here writes to GitHub. It can fail on a
 * machine where `gh` is missing or logged out, which the server reports as
 * `github_unavailable` — the developer's environment rather than their typing, and a
 * different thing for the screen to say.
 */
export function startReview(prUrl: string): Promise<Review> {
  const request: StartReviewRequest = { pr_url: prUrl }
  /* The one operation that leaves the machine, so the one with a longer wait. */
  return send<Review>('POST', '/reviews', request, WAIT_FOR_GITHUB)
}

/** `POST /reviews/{id}/brief/confirm` — the developer's act at layer 2. */
export function confirmBrief(reviewId: number): Promise<Brief> {
  return send<Brief>('POST', `/reviews/${reviewId}/brief/confirm`)
}

/** `PATCH /reviews/{id}/findings/{seq}` — move a finding, with the reason it moved. */
export function setFindingDisposition(
  reviewId: number,
  seq: number,
  disposition: Disposition,
  reason: string | null,
): Promise<Finding> {
  const request: SetFindingDispositionRequest = { disposition, reason }
  return send<Finding>('PATCH', `/reviews/${reviewId}/findings/${seq}`, request)
}

/**
 * `PUT /reviews/{id}/draft` — the body, and the edits it represents.
 *
 * The edits are stated rather than inferred from a text diff: "regraded F3 from Blocker to
 * Should" is not recoverable from the characters that changed, and the developer's own
 * edits are the strongest signal in the feedback loop.
 */
export function saveDraft(
  reviewId: number,
  body: string,
  edits: DraftEditToRecord[],
): Promise<DraftDetail> {
  const request: SaveDraftRequest = { body, edits }
  return send<DraftDetail>('PUT', `/reviews/${reviewId}/draft`, request)
}

/** `POST /reviews/{id}/draft/approve` — approved here, posted by the review session. */
export function approveDraft(reviewId: number, event: ReviewEvent): Promise<DraftDetail> {
  const request: ApproveDraftRequest = { event }
  return send<DraftDetail>('POST', `/reviews/${reviewId}/draft/approve`, request)
}

/** `POST /reviews/{id}/draft/send-back` — refused without a note, as the API refuses it. */
export function sendBackDraft(reviewId: number, note: string): Promise<DraftDetail> {
  const request: SendBackDraftRequest = { note }
  return send<DraftDetail>('POST', `/reviews/${reviewId}/draft/send-back`, request)
}

/**
 * `POST /reviews/{id}/held-back/ticket` — ask for one ticket, or record the one filed.
 *
 * Review Desk records it; an agent files it. With no ticket id this is the developer
 * asking; with one it is the ticket that answered.
 */
export function requestNitTicket(
  reviewId: number,
  ticketId: string | null = null,
): Promise<NitTicket> {
  const request: RequestNitTicketRequest = { ticket_id: ticketId }
  return send<NitTicket>('POST', `/reviews/${reviewId}/held-back/ticket`, request)
}

/** `POST /rules/{id}/promote` — the developer promotes; the app never promotes itself. */
export function promoteRule(ruleId: number, toStage: RuleStage): Promise<Rule> {
  const request: PromoteRuleRequest = { to_stage: toStage }
  return send<Rule>('POST', `/rules/${ruleId}/promote`, request)
}

/** `POST /rules/{id}/dismiss` — the reason is required, so it is not proposed again. */
export function dismissRule(ruleId: number, reason: string): Promise<Rule> {
  return send<Rule>('POST', `/rules/${ruleId}/dismiss`, { reason })
}

/** `POST /rules/{id}/retire` — a rule that has stopped earning its place. */
export function retireRule(ruleId: number, reason: string): Promise<Rule> {
  return send<Rule>('POST', `/rules/${ruleId}/retire`, { reason })
}

/** `POST /rules/{id}/defer` — keep it, and ask again later. */
export function deferRule(ruleId: number, days: number): Promise<Rule> {
  const request: DeferRuleRequest = { days }
  return send<Rule>('POST', `/rules/${ruleId}/defer`, request)
}

/**
 * `GET /api/v1/health` — not in the contract.
 *
 * It is the server saying it is up and its database is open and migrated. No screen needs
 * it; the retry on a stopped-server message uses it, because asking "are you back?" should
 * not mean re-reading a whole screen's worth of record to find out.
 */
export async function health(): Promise<{ status: string, schema_version: number }> {
  return send('GET', '/health')
}
