/*
 * The shapes the dashboard reads, **named rather than written**.
 *
 * Every type here is an alias over `app/types/contract.ts`, which is generated from
 * `contract/openapi.yaml` and is **not** committed — it is rebuilt on install and ahead of
 * every build, type check, lint and test run. Nothing in this file
 * describes a field; it only gives the contract's schemas the short names the screens use,
 * so `import type { Inbox } from '~/types/desk'` keeps reading the way it did while the
 * field list has exactly one source.
 *
 * **That is the point, and it is the fix for a drift that had already happened.** This file
 * used to spell the shapes out by hand, and `OutcomeKind` fell two kinds behind the
 * contract — the harvester wrote `replied` and `thread_resolved` and the dashboard could
 * not name either, so the Inbox's "Author replied" group was built from values the type
 * said were impossible. Hand-written types cannot be made not to drift by being careful;
 * generated ones cannot drift at all, because there is no second copy to disagree with the
 * contract. What `test/contract-types.spec.ts` guards now is the generating: that it fails
 * loudly and leaves nothing stale behind when the contract cannot be read.
 *
 * If the generated module is missing, `nuxt.config.ts` says so in one sentence before the
 * type checker gets a chance to report fifty-one unrelated errors and the real one last.
 *
 * Six reads, one per screen and two lists — `GET /inbox`, `GET /reviews`,
 * `GET /reviews/{id}`, `GET /reviews/{id}/draft`, `GET /rules?repo=`, `GET /repos` — so
 * no screen joins anything. (It read "four reads" until DEV-930, and `GET /repos` had
 * already made that five.) Times are
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
/** What a pull request was last seen to be **at the last harvest**. Null is nobody looked. */
export type PrState = Schema['PrState']
/** What became of a finding in one round. Not the same question as `Disposition`. */
export type RoundStatus = Schema['RoundStatus']
/** Who decided a round status. Deliberately not `Actor`: an agent decides most of them. */
export type DecidedBy = Schema['DecidedBy']
/** Eight decisions behind eleven buttons. `FindingDecision` in the contract has the table. */
export type FindingDecision = Schema['FindingDecision']

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
export type DecideFindingRequest = Schema['DecideFindingRequest']
export type TicketRestAndApproveRequest = Schema['TicketRestAndApproveRequest']
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
export type ReviewLayerRun = Schema['ReviewLayerRun']
/** One fact a step recorded: what the card beside it draws, a label and a value. */
export type LayerFact = Schema['LayerFact']
export type ReviewTimings = Schema['ReviewTimings']
export type BriefOption = Schema['BriefOption']
export type Brief = Schema['Brief']

/*
 * The twelve rows the three approved step pages draw, named the same way everything else
 * here is: an alias over the generated contract, never a second description of a field.
 * `contract/README.md`'s "The brief, by page" is the table that says which row feeds which
 * page.
 */

/** Who put an option forward: the ticket, a blind pass, or both independently. */
export type OptionProposer = Schema['OptionProposer']
/** What one option does about one numbered problem. `not_assessed` is a verdict. */
export type BriefOptionVerdict = Schema['BriefOptionVerdict']
/** A flow is rows; a row is one to three boxes. Nothing carries a layout number. */
export type BriefFlow = Schema['BriefFlow']
export type BriefFlowRow = BriefFlow[number]
export type BriefFlowBox = Schema['BriefFlowBox']
export type BriefProblem = Schema['BriefProblem']
export type BriefSupportTable = Schema['BriefSupportTable']
export type BriefBlindPass = Schema['BriefBlindPass']
/** Three states, three wordings: it never ran, it ran and found nothing, it ran. */
export type BlindPassState = Schema['BlindPassState']
export type BriefPart = Schema['BriefPart']
export type BriefPartFile = Schema['BriefPartFile']
export type BriefOpenChoice = Schema['BriefOpenChoice']
export type BriefDisagreement = Schema['BriefDisagreement']
/** The problem half on its own, which is what a blind pass may be handed. */
export type ProblemStatement = Schema['ProblemStatement']
export type Finding = Schema['Finding']
/** The lines around a finding, as the pull request changed them. */
export type FindingHunk = Schema['FindingHunk']
export type FindingHunkLine = Schema['FindingHunkLine']
/** What the pull request did to one line: words, not `+`, `-` and a space. */
export type HunkLineMarker = Schema['HunkLineMarker']
export type FindingVerification = Schema['FindingVerification']
export type Outcome = Schema['Outcome']
export type NitTicket = Schema['NitTicket']
export type ReviewDetail = Schema['ReviewDetail']

/* ---- GET /reviews/{id}/draft ---- */

export type Draft = Schema['Draft']
/**
 * One block of the draft body — the unit a decision changes.
 *
 * `kind: 'finding'` is the part a decision touches and `prose` is everything else, which is
 * what lets an assembled body reproduce what an agent wrote character for character. A
 * draft with **no** sections is one written as one block of markdown, and the per-finding
 * decisions refuse against it; that is not an empty list to be ignored but the one state
 * the code review page has to say out loud.
 */
export type DraftSection = Schema['DraftSection']
export type DraftSectionKind = Schema['DraftSectionKind']
export type DraftLint = Schema['DraftLint']
export type DraftEdit = Schema['DraftEdit']
export type HeldBackFinding = Schema['HeldBackFinding']
export type DraftDetail = Schema['DraftDetail']
/** The decision, and everything it changed, read in one instant of the record. */
export type FindingDecisionMade = Schema['FindingDecisionMade']
/** What "Ticket what is left, and approve" came to. */
export type TicketedAndApproved = Schema['TicketedAndApproved']

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
/**
 * A posted review whose pull request has not been merged or closed.
 *
 * Its three counts are nullable and null is not zero: null is nothing having measured
 * one, and `harvested_at` and `findings_linked` are what say which absence it is.
 */
export type InboxPostedItem = Schema['InboxPostedItem']
export type Inbox = Schema['Inbox']

/* ---- GET /reviews ---- */

/**
 * One row of the Reviews list.
 *
 * `step_number` and the two fields with it are absent together, for a review nothing has
 * reported on. The page draws that in words — "not started" — rather than as a 1.
 */
export type ReviewListItem = Schema['ReviewListItem']

/** `GET /reviews` serves an object, not an array; this is what is inside it. */
export type ReviewList
  = operations['listReviews']['responses']['200']['content']['application/json']

/* ---- What sample mode holds. The server holds the same facts in SQLite. ---- */

/**
 * The whole desk, as one object.
 *
 * Only sample mode has such a thing. The API serves each read as its own document and
 * joins nothing, which is the contract's "each screen is one GET"; this is what the fixtures have
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
