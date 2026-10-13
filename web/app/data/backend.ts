/*
 * What a source of records can do: the contract's twenty-two operations, and nothing else.
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
  DecideFindingRequest,
  ReviewsShown,
  Disposition,
  DraftDetail,
  DuplicateRoundsPage,
  Finding,
  FindingDecisionMade,
  FoldedRound,
  Inbox,
  NitTicket,
  OutcomeKind,
  RepoSummary,
  Review,
  ReviewDetail,
  ReviewEvent,
  ReviewList,
  Rule,
  RuleStage,
  RulesPage,
  TicketedAndApproved,
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

  /* The six reads — one per screen, so no screen joins anything. */
  getInbox: () => Promise<Inbox>
  listReviews: (repo: string | null, show?: ReviewsShown) => Promise<ReviewList>
  getReview: (reviewId: number) => Promise<ReviewDetail>
  getDraft: (reviewId: number) => Promise<DraftDetail>
  listRules: (repo: string | null) => Promise<RulesPage>
  listRepos: () => Promise<RepoSummary[]>

  /* The sixteen writes. */
  startReview: (prUrl: string) => Promise<Review>
  confirmBrief: (reviewId: number) => Promise<Brief>
  setFindingDisposition: (
    reviewId: number,
    seq: number,
    disposition: Disposition,
    reason: string | null,
  ) => Promise<Finding>
  /**
   * `POST …/findings/{seq}/decide` — the developer's decision, and the draft recomposed.
   *
   * **The whole request, not a decision and some arguments.** Four of the eight decisions
   * carry words — a severity for a regrade, a title or a text for a rewrite, the author's
   * reason for an acceptance — and the contract refuses the wrong combination by name. A
   * signature with one parameter per field would let a caller send a severity on a cut and
   * find out from the server; passing the request means the generated type refuses it here.
   */
  decideFinding: (
    reviewId: number,
    seq: number,
    request: DecideFindingRequest,
  ) => Promise<FindingDecisionMade>
  saveDraft: (
    reviewId: number,
    body: string,
    edits: DraftEditToRecord[],
  ) => Promise<DraftDetail>
  approveDraft: (reviewId: number, event: ReviewEvent) => Promise<DraftDetail>
  sendBackDraft: (reviewId: number, note: string) => Promise<DraftDetail>
  /** `POST …/draft/ticket-and-approve` — a sign-off. It neither posts nor files anything. */
  ticketRestAndApprove: (
    reviewId: number,
    note: string | null,
  ) => Promise<TicketedAndApproved>
  requestNitTicket: (reviewId: number, ticketId?: string | null) => Promise<NitTicket>
  /**
   * `POST …/dismiss` — the developer takes a review out of play, with a reason.
   *
   * **The developer's act, and there is no other way to make it.** No command dismisses a
   * review, restores one or deletes one — DEV-945, and the same rule that leaves confirming
   * a brief and approving a draft to this interface alone.
   */
  dismissReview: (reviewId: number, reason: string) => Promise<Review>
  /** `POST …/restore` — bring a dismissed review back. The undo a dismissal has. */
  restoreReview: (reviewId: number) => Promise<Review>
  /**
   * `DELETE /reviews/{id}` — remove a review that never got past being requested.
   *
   * Refused for a review holding a brief, a finding, a draft or the review it posted on
   * GitHub, saying which. It answers with the review that went: nothing can read a row it
   * has just removed.
   */
  deleteReview: (reviewId: number) => Promise<Review>
  /**
   * `GET /duplicate-rounds` — the one-off tidy's proposal.
   *
   * A seventh read, and the only one that is not a screen's own: the Reviews page asks for
   * it when the developer opens the tidy. It decides nothing.
   */
  listDuplicateRounds: (repo: string | null) => Promise<DuplicateRoundsPage>
  /**
   * `POST /reviews/{id}/fold` — fold an extra unposted round into the newest one.
   *
   * **The developer's act, on their confirmation, and there is no other way to make it.**
   * The same rule as a dismissal, a restore and a delete: no command folds a round, because
   * a session that could would be able to fold away a round of its own review.
   */
  foldRound: (reviewId: number) => Promise<FoldedRound>
  promoteRule: (ruleId: number, toStage: RuleStage) => Promise<Rule>
  dismissRule: (ruleId: number, reason: string) => Promise<Rule>
  retireRule: (ruleId: number, reason: string) => Promise<Rule>
  deferRule: (ruleId: number, days: number) => Promise<Rule>

  /** Whether the server is answering. Behind the retry on a stopped-server message. */
  health: () => Promise<unknown>
}
