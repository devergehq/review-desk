/*
 * The shapes the dashboard reads, **named rather than written**.
 *
 * Every type here is an alias over `app/types/contract.ts`, which is generated from
 * `contract/openapi.yaml` by `pnpm run types:contract` and committed. Nothing in this file
 * describes a field; it only gives the contract's schemas the short names the screens use,
 * so `import type { Inbox } from '~/types/desk'` keeps reading the way it did while the
 * field list has exactly one source.
 *
 * **That is the point, and it is the fix for a drift that had already happened.** This file
 * used to spell the shapes out by hand, and `OutcomeKind` fell two kinds behind the
 * contract — the harvester wrote `replied` and `thread_resolved` and the dashboard could
 * not name either, so the Inbox's "Author replied" group was built from values the type
 * said were impossible. Hand-written types cannot be made not to drift by being careful;
 * generated ones cannot drift at all, and `test/contract-types.spec.ts` fails when the
 * committed file and the contract disagree.
 *
 * Four reads, one per screen — `GET /inbox`, `GET /reviews/{id}`,
 * `GET /reviews/{id}/draft`, `GET /rules?repo=` — so no screen joins anything. Times are
 * RFC 3339 strings. A nullable count means the server has no measurement, which is not the
 * same as zero, and a screen must say so rather than print a nought.
 */

import type { components, operations } from '~/types/contract'

type Schema = components['schemas']

/** An RFC 3339 instant in UTC, as the API writes it. */
export type Instant = string

export type RepoName = Schema['RepoName']

/* ---- The closed sets, straight off the contract's enums ---- */

export type Severity = Schema['Severity']
export type Disposition = Schema['Disposition']
export type OutcomeKind = Schema['OutcomeKind']
export type Actor = Schema['Actor']
export type RuleStage = Schema['RuleStage']
export type DraftStatus = Schema['DraftStatus']
export type ReviewEvent = Schema['ReviewEvent']
export type ReviewLayerName = Schema['ReviewLayerName']
export type ReviewLayerState = Schema['ReviewLayerState']
export type NitTicketStatus = Schema['NitTicketStatus']

/**
 * Why a request was refused. `store_unavailable` means Review Desk is broken;
 * `github_unavailable` means a read it needed could not be made, which is the developer's
 * environment rather than their request. Everything else means the request was understood
 * and refused. A caller branches on this, never on the message.
 */
export type ErrorCode = Schema['ErrorCode']

/**
 * The contract's one error shape, plus the two codes that never reach the wire.
 *
 * `unreachable` and `unreadable` are the data layer's own, and they are not in
 * `ErrorCode` because the contract cannot describe them: the first is what there is to say
 * when the server answered nothing at all — it was stopped, or never started — and the
 * second is an answer that was not the contract's error shape, which means something other
 * than Review Desk replied. Both are states a screen has to be able to render, and
 * folding them into `store_unavailable` would tell the developer their database is broken
 * when their server is simply not running.
 */
export type DeskErrorCode = ErrorCode | 'unreachable' | 'unreadable'

export interface DeskError {
  code: DeskErrorCode
  /** Written for a person to read. The screen shows it; it never parses it. */
  message: string
  field?: string | null
  expected?: string | null
  /** The HTTP status, when there was one. Absent when nothing answered. */
  status?: number | null
}

/* ---- Request bodies ---- */

export type StartReviewRequest = Schema['StartReviewRequest']
export type SetFindingDispositionRequest = Schema['SetFindingDispositionRequest']
export type SaveDraftRequest = Schema['SaveDraftRequest']
export type SaveDraftEdit = NonNullable<SaveDraftRequest['edits']>[number]
export type ApproveDraftRequest = Schema['ApproveDraftRequest']
export type SendBackDraftRequest = Schema['SendBackDraftRequest']
export type RequestNitTicketRequest = Schema['RequestNitTicketRequest']
export type PromoteRuleRequest = Schema['PromoteRuleRequest']
export type RuleReasonRequest = Schema['RuleReasonRequest']
export type DeferRuleRequest = Schema['DeferRuleRequest']

/* ---- GET /reviews/{id} ---- */

export type RuleScope = Schema['RuleScope']
export type SeverityCounts = Schema['SeverityCounts']
export type DispositionCounts = Schema['DispositionCounts']
export type Review = Schema['Review']
export type ReviewLayer = Schema['ReviewLayer']
export type BriefOption = Schema['BriefOption']
export type Brief = Schema['Brief']
export type Finding = Schema['Finding']
export type Outcome = Schema['Outcome']
export type NitTicket = Schema['NitTicket']
export type ReviewDetail = Schema['ReviewDetail']

/* ---- GET /reviews/{id}/draft ---- */

export type Draft = Schema['Draft']
export type DraftLint = Schema['DraftLint']
export type DraftEdit = Schema['DraftEdit']
export type HeldBackFinding = Schema['HeldBackFinding']
export type DraftDetail = Schema['DraftDetail']

/* ---- GET /rules?repo= ---- */

export type RuleCounts = Schema['RuleCounts']
export type Rule = Schema['Rule']
export type RulesSummary = Schema['RulesSummary']
export type RulesPage = Schema['RulesPage']

/* ---- GET /repos ---- */

export type RepoSummary = Schema['RepoSummary']

/** `GET /repos` serves an object, not an array; this is what is inside it. */
export type RepoList = operations['listRepos']['responses']['200']['content']['application/json']

/* ---- GET /inbox ---- */

export type InboxPr = Schema['InboxPr']
export type InboxTotals = Schema['InboxTotals']
export type InboxBriefItem = Schema['InboxBriefItem']
export type InboxDraftItem = Schema['InboxDraftItem']
export type InboxReplyEvent = Schema['InboxReplyEvent']
export type InboxReplyItem = Schema['InboxReplyItem']
export type DescriptionLint = Schema['DescriptionLint']
export type InboxOwnPrItem = Schema['InboxOwnPrItem']
export type InboxProposedRuleItem = Schema['InboxProposedRuleItem']
export type InboxInProgressItem = Schema['InboxInProgressItem']
export type InboxHandled = Schema['InboxHandled']
export type Inbox = Schema['Inbox']

/* ---- What sample mode holds. The server holds the same facts in SQLite. ---- */

/**
 * The whole desk, as one object.
 *
 * Only sample mode has such a thing. The API serves four separate documents and joins
 * nothing, which is the contract's "each screen is one GET"; this is what the fixtures have
 * to be in order to recompute those four documents in memory as a write patches them.
 */
export interface DeskSource {
  reviews: ReviewDetail[]
  drafts: DraftDetail[]
  rules: Rule[]
  summary: RulesSummary
  repos: RepoSummary[]
  own_prs: InboxOwnPrItem[]
  handled_without_you: InboxHandled
  /** What Allele would tell the server about each running session, by review id. */
  session_liveness: Record<string, boolean | null>
  /**
   * Whether the server could read GitHub. It stands in for the developer's environment —
   * `gh` present and logged in — which is what `POST /reviews` needs to resolve a head
   * commit, and which no fixture of the record itself could express.
   */
  github_available: boolean
}
