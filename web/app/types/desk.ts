/*
 * The shapes the dashboard reads, taken from `contract/openapi.yaml` on the
 * `dev-854-api-contract` branch.
 *
 * Four reads, one per screen — `GET /inbox`, `GET /reviews/{id}`,
 * `GET /reviews/{id}/draft`, `GET /rules?repo=` — so no screen joins anything. Times are
 * ISO 8601 strings. A nullable count means the server has no measurement, which is not
 * the same as zero, and a screen must say so rather than print a nought.
 */

/** An ISO 8601 instant, as the API writes it. */
export type Instant = string

export type RepoName = string

export type Severity = 'blocker' | 'should' | 'nit' | 'question'

export type Disposition = 'in_draft' | 'held_back' | 'suppressed'

export type OutcomeKind
  = | 'cut'
    | 'regraded'
    | 'rewritten'
    | 'kept'
    | 'rejected'
    | 'raised_again'
    | 'code_changed_after'

export type Actor = 'developer' | 'author' | 'harvester'

export type RuleStage = 'proposed' | 'active' | 'lint' | 'ci' | 'retired'

export type DraftStatus = 'drafting' | 'ready' | 'approved' | 'sent_back' | 'posted'

export type ReviewEvent = 'comment' | 'request_changes' | 'approve'

export type ReviewLayerName
  = | 'lint'
    | 'understand'
    | 'alternatives'
    | 'solution_map'
    | 'code_review'
    | 'draft'
    | 'harvest'

export type ReviewLayerState
  = | 'pending'
    | 'running'
    | 'done'
    | 'waiting_on_you'
    | 'failed'
    | 'skipped'

export type NitTicketStatus = 'none' | 'requested' | 'filed'

export interface RuleScope {
  kind: 'global' | 'repo'
  repo?: RepoName | null
}

export interface SeverityCounts {
  blocker: number
  should: number
  nit: number
  question: number
}

export interface DispositionCounts {
  in_draft: number
  held_back: number
  suppressed: number
}

/* ---- GET /reviews/{id} ---- */

export interface Review {
  id: number
  repo: RepoName
  pr: number
  head_sha: string
  round: number
  title?: string | null
  ticket_id?: string | null
  changed_lines?: number | null
  session_id?: string | null
  ledger_id?: string | null
  github_review_id?: string | null
  created_at: Instant
}

export interface ReviewLayer {
  number: number
  name: ReviewLayerName
  state: ReviewLayerState
  detail?: string | null
  updated_at?: Instant | null
}

export interface BriefOption {
  key: string
  title: string
  argument_for: string
  argument_against: string
  chosen: boolean
}

export interface Brief {
  review_id: number
  problem: string
  /** Mermaid source. The record keeps whatever notation the layer used. */
  diagram?: string | null
  options?: BriefOption[]
  approach_verdict?: string | null
  provenance?: string | null
  confirmed: boolean
  confirmed_at?: Instant | null
  updated_at: Instant
}

export interface Finding {
  review_id: number
  seq: number
  key: string
  lens: string
  severity: Severity
  file?: string | null
  line?: number | null
  claim: string
  evidence?: string | null
  disposition: Disposition
  disposition_reason?: string | null
  rule_id?: number | null
  rule_key?: string | null
  created_at: Instant
}

export interface Outcome {
  id: number
  review_id: number
  finding_seq: number
  finding_key: string
  kind: OutcomeKind
  actor: Actor
  occurred_at: Instant
  detail?: string | null
}

export interface NitTicket {
  review_id: number
  status: NitTicketStatus
  ticket_id?: string | null
  finding_seqs: number[]
  requested_at?: Instant | null
  filed_at?: Instant | null
}

export interface ReviewDetail {
  review: Review
  layers: ReviewLayer[]
  brief?: Brief | null
  findings: Finding[]
  outcomes: Outcome[]
  finding_severities: SeverityCounts
  finding_dispositions: DispositionCounts
  not_checked?: string | null
  nit_ticket: NitTicket
  draft_status?: DraftStatus | null
}

/* ---- GET /reviews/{id}/draft ---- */

export interface Draft {
  review_id: number
  body: string
  status: DraftStatus
  event?: ReviewEvent | null
  approved_at?: Instant | null
  sent_back_note?: string | null
  sent_back_at?: Instant | null
  posted_at?: Instant | null
  updated_at: Instant
}

export interface DraftLint {
  passes: boolean
  visible_words?: number | null
  word_budget?: number | null
  findings_in_body?: number | null
  findings_expected?: number | null
  raised_before_on_pr?: number | null
  errors?: string[]
  checked_at: Instant
}

export interface DraftEdit {
  finding_seq: number
  finding_key: string
  kind: OutcomeKind
  summary: string
  occurred_at?: Instant | null
}

export interface HeldBackFinding {
  finding_seq: number
  finding_key: string
  claim: string
  severity?: Severity
}

export interface DraftDetail {
  draft: Draft
  review: Review
  lint: DraftLint
  edits: DraftEdit[]
  held_back: HeldBackFinding[]
  nit_ticket: NitTicket
}

/* ---- GET /rules?repo= ---- */

export interface RuleCounts {
  raised?: number | null
  kept?: number | null
  rejected_with_reason?: number | null
  cited_by_findings?: number | null
  caught_by_check?: number | null
  raised_in_a_review_since?: number | null
  exceptions?: number | null
  last_human_mention_at?: Instant | null
}

export interface Rule {
  id: number
  key: string
  statement: string
  prevents: string
  scope: RuleScope
  stage: RuleStage
  stage_label?: string | null
  why_proposed?: string | null
  note?: string | null
  origin_review_id?: number | null
  origin_pr?: number | null
  origin_findings: Array<{ review_id: number, seq: number }>
  stale: boolean
  last_cited_at?: Instant | null
  review_again_at?: Instant | null
  counts: RuleCounts
  created_at: Instant
}

export interface RulesSummary {
  active_rules: number
  active_rules_cap: number
  replay_matched?: number | null
  replay_total?: number | null
  findings_kept?: number | null
  findings_total?: number | null
  findings_window_days?: number | null
  raised_twice_on_one_pr?: number | null
}

export interface RulesPage {
  repo?: RepoName | null
  summary: RulesSummary
  proposed: Rule[]
  active: Rule[]
  stale: Rule[]
}

/* ---- GET /repos ---- */

export interface RepoSummary {
  repo: RepoName
  open_reviews: number
  active_rules: number
}

/* ---- GET /inbox ---- */

export interface InboxPr {
  repo: RepoName
  pr: number
  title?: string | null
}

export interface InboxTotals {
  items: number
  repos: number
}

export interface InboxBriefItem {
  review_id: number
  pull_request: InboxPr
  ticket_id?: string | null
  brief_ready_at?: Instant | null
  /** True when the session is held at layer 2 and nothing later has run. */
  blocks_later_layers: boolean
}

export interface InboxDraftItem {
  review_id: number
  pull_request: InboxPr
  round: number
  severities: SeverityCounts
  held_back: number
  is_delta: boolean
}

export interface InboxReplyEvent {
  finding_seq: number
  finding_key: string
  kind: OutcomeKind
  detail?: string | null
}

export interface InboxReplyItem {
  review_id: number
  pull_request: InboxPr
  events: InboxReplyEvent[]
}

export interface DescriptionLint {
  passes: boolean
  budget_ratio?: number | null
}

export interface InboxOwnPrItem {
  pull_request: InboxPr
  opened_at?: Instant | null
  description_lint?: DescriptionLint | null
}

export interface InboxProposedRuleItem {
  rule_id: number
  rule_key: string
  statement: string
  why_proposed?: string | null
  scope: RuleScope
}

export interface InboxInProgressItem {
  review_id: number
  pull_request: InboxPr
  layer_number: number
  layer_name: ReviewLayerName
  layers_total: number
  session_id?: string | null
  /** Allele owns this. Null means nothing has said, which the screen must show. */
  session_alive?: boolean | null
}

export interface InboxHandled {
  window_days: number
  description_problems_caught?: number | null
  nits_filed?: number | null
  nit_tickets?: number | null
}

export interface Inbox {
  totals: InboxTotals
  briefs_to_confirm: InboxBriefItem[]
  drafts_to_approve: InboxDraftItem[]
  author_replies: InboxReplyItem[]
  own_prs_without_reviewer: InboxOwnPrItem[]
  proposed_rules: InboxProposedRuleItem[]
  in_progress: InboxInProgressItem[]
  handled_without_you: InboxHandled
}

/* ---- What the sample source holds. The server holds the same facts in SQLite. ---- */

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
}
