/*
 * What a source of records can do: the contract's sixteen operations, and nothing else.
 *
 * Two things implement it — `api.ts`, which calls the Rust server, and `sample.ts`, which
 * patches invented records in memory. **The point of writing it down is that the composable
 * above cannot tell them apart.** A screen behaves the same way on fixtures as it does on
 * the real record, including when a write is refused: sample mode raises the same refusals,
 * with the same `code`s, that the server raises, so an empty state or an error state that
 * works in a component test works in the binary.
 *
 * It is a type and nothing else. Importing it pulls in no fixture and makes no request,
 * which is what lets `source.ts` decide between the two without loading both.
 */

import type {
  Brief,
  Disposition,
  DraftDetail,
  Finding,
  Inbox,
  NitTicket,
  OutcomeKind,
  RepoSummary,
  Review,
  ReviewDetail,
  ReviewEvent,
  Rule,
  RuleStage,
  RulesPage,
} from '~/types/desk'

/** One edit a draft save represents, recorded as an outcome by the developer. */
export interface DraftEditToRecord {
  finding_seq: number
  kind: OutcomeKind
  summary: string
}

/** The contract's operations, as one object. */
export interface DeskBackend {
  /** Whether these records are invented. The header's marker reads it. */
  readonly isSampleData: boolean

  /* The four reads — one per screen, so no screen joins anything. */
  getInbox: () => Promise<Inbox>
  getReview: (reviewId: number) => Promise<ReviewDetail>
  getDraft: (reviewId: number) => Promise<DraftDetail>
  listRules: (repo: string | null) => Promise<RulesPage>
  listRepos: () => Promise<RepoSummary[]>

  /* The twelve writes. */
  startReview: (prUrl: string) => Promise<Review>
  confirmBrief: (reviewId: number) => Promise<Brief>
  setFindingDisposition: (
    reviewId: number,
    seq: number,
    disposition: Disposition,
    reason: string | null,
  ) => Promise<Finding>
  saveDraft: (
    reviewId: number,
    body: string,
    edits: DraftEditToRecord[],
  ) => Promise<DraftDetail>
  approveDraft: (reviewId: number, event: ReviewEvent) => Promise<DraftDetail>
  sendBackDraft: (reviewId: number, note: string) => Promise<DraftDetail>
  requestNitTicket: (reviewId: number, ticketId?: string | null) => Promise<NitTicket>
  promoteRule: (ruleId: number, toStage: RuleStage) => Promise<Rule>
  dismissRule: (ruleId: number, reason: string) => Promise<Rule>
  retireRule: (ruleId: number, reason: string) => Promise<Rule>
  deferRule: (ruleId: number, days: number) => Promise<Rule>

  /** Whether the server is answering. Behind the retry on a stopped-server message. */
  health: () => Promise<unknown>
}
