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
 * A refusal, thrown so that a read and a write report failure the same way.
 *
 * It carries the contract's `Error` object rather than a message, because the screens
 * branch on `code` and never on wording — a `github_unavailable` is the developer's
 * environment and names a command that fixes it, while a `conflict` is about the record and
 * names what state it is in.
 */
export class DeskRequestError extends Error {
  readonly desk: DeskError

  constructor(desk: DeskError) {
    super(desk.message)
    this.name = 'DeskRequestError'
    this.desk = desk
  }
}

/** Whatever went wrong, as the one error shape the screens render. */
export function asDeskError(thrown: unknown): DeskError {
  if (thrown instanceof DeskRequestError) return thrown.desk

  /*
   * Nothing answered. The server was stopped, or never started, or the port moved — which
   * is a state DEV-862 requires a screen to handle with a message and a retry, and which
   * no contract code describes. Calling it `store_unavailable` would tell the developer
   * their database is broken when their server is simply not running.
   */
  return {
    code: 'unreachable',
    message:
      'Review Desk is not answering on this address. Start it with `review-desk serve`, '
      + 'then retry.',
    field: null,
    expected: null,
    status: null,
  }
}

/** One request, with the contract's refusal shape read back out of the answer. */
async function send<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  let response: Response

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
    })
  }
  catch {
    // A network-level failure. `asDeskError` turns it into `unreachable`.
    throw new TypeError('the Review Desk server did not answer')
  }

  const text = await response.text()

  if (response.ok) {
    if (text.length === 0) return undefined as T
    try {
      return JSON.parse(text) as T
    }
    catch {
      throw new DeskRequestError({
        code: 'unreadable',
        message:
          'The answer was not the JSON this dashboard expects. Something other than '
          + 'Review Desk may be answering on this address.',
        field: null,
        expected: null,
        status: response.status,
      })
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
  return send<Review>('POST', '/reviews', request)
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
