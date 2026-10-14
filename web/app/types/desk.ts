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
export type DismissReviewRequest = Schema['DismissReviewRequest']
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
/** One round of a pull request, as the timeline across the round-two page lists it. */
export type RoundSummary = Schema['RoundSummary']
/** How a round's findings stand. Zero is a measurement here, not an absence. */
export type RoundStatusCounts = Schema['RoundStatusCounts']
/** What happened to a round's head commit: `moved`, `base_only` or `folded`. */
export type HeadUpdateKind = Schema['HeadUpdateKind']
export type HeadUpdate = Schema['HeadUpdate']
/**
 * What has happened to this round's head commit, with the counts the page reads.
 *
 * `commits` is null rather than zero when the round's commits were never recorded, and a
 * screen must then say it was updated without counting them — zero would read as "nothing
 * changed", which is the one thing it is not.
 */
export type HeadUpdates = Schema['HeadUpdates']
export type ReviewDetail = Schema['ReviewDetail']

/* ---- GET /duplicate-rounds, and the fold it proposes ---- */

/** Why a pull request is out of play: `draft` or `withdrawn`. Both come back. */
export type OutOfPlay = Schema['OutOfPlay']
/** What the tidy would do with one extra unposted round: `fold` or `dismiss`. */
export type TidyAct = Schema['TidyAct']
export type RoundRef = Schema['RoundRef']
export type ExtraRound = Schema['ExtraRound']
export type DuplicateRounds = Schema['DuplicateRounds']

/** `GET /duplicate-rounds` serves an object; this is what is inside it. */
export type DuplicateRoundsPage
  = operations['listDuplicateRounds']['responses']['200']['content']['application/json']

/** What `POST /reviews/{id}/fold` answers: the round that went, and the one it folded into. */
export type FoldedRound
  = operations['foldRound']['responses']['200']['content']['application/json']

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
/**
 * One inline thread the draft hands over: a finding with a line inside the diff.
 *
 * **Not in the body**, which is the whole reason the page needs these as well. A finding
 * GitHub can anchor is a row in the body's index and a comment on the code, so a preview
 * drawn from the body alone would not be showing what posts.
 */
export type DraftThread = Schema['DraftThread']
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
/**
 * The round the queue's single row for a pull request is: its number, its review and its
 * ticket.
 *
 * `author_replies` is the one array that can be about an older round, so the item carries
 * this and the row takes all of it. DEV-983.
 */
export type InboxNewestRound = Schema['InboxNewestRound']
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

/**
 * Which reviews a list of reviews is asked for.
 *
 * `in_play` is the Reviews list's default view and what every other read gets: a review
 * the developer dismissed, or one whose pull request merged or closed, is in front of
 * nobody until this says otherwise.
 *
 * **Four positions because there are two ways off the desk**, which DEV-957 asks to be
 * told apart everywhere a review is listed. It was `DismissedReviews` with `excluded`,
 * `only` and `included`; a fourth value called `finished` on a filter called `dismissed`
 * would have been the filter's own name saying the opposite of its value.
 */
export type ReviewsShown = Schema['ReviewsShown']

/**
 * How a review left the desk, on a row of the Reviews list.
 *
 * Read from the answer rather than worked out by the row: a dismissal is a column on the
 * review, but whether the pull request is finished is a fact about the pull request read
 * off whichever round of it a harvest last reached.
 */
export type LeftTheDesk = Schema['LeftTheDesk']

/**
 * Why a draft may not be signed off, or null when it may.
 *
 * The screen draws its sentence from this and the API refuses from the same value, so a
 * page cannot offer a sign-off the API refuses.
 */
export type NotForSigning = Schema['NotForSigning']

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
