/*
 * Sample mode: invented records, in memory, behind the same sixteen operations.
 *
 * **This is the only file in `web/` that names a fixture**, and nothing imports it unless
 * `source.ts` decides to — which a production build has already decided not to, so none of
 * this reaches a built dashboard. `test/architecture.spec.ts` and
 * `test/build-output.spec.ts` hold both halves of that.
 *
 * ## Why it survived the switch to the API
 *
 * Two reasons, and neither is nostalgia. The component tests mount four screens and assert
 * what they render; against a real server they would be a server test wearing a component
 * test's clothes, and they would need one running to say anything at all. And a developer
 * changing a screen can see it with records on it without starting a binary and seeding a
 * database.
 *
 * ## It refuses the way the server refuses
 *
 * A write here raises `DeskRequestError` with the contract's own `code` — `invalid_value`
 * for a reason the server would have required, `conflict` for a draft that cannot be
 * edited, `github_unavailable` for a machine where `gh` is not logged in. That is what
 * makes a refusal state tested here a refusal state that works in the binary: the screens
 * branch on `code`, and sample mode speaks the same six.
 *
 * The one thing it cannot be faithful about is a server that is not running, which is why
 * `test/desk.spec.ts` reaches for `api.ts` with a stubbed `fetch` for that state.
 */

import { reactive } from 'vue'

import type { DeskBackend, DraftEditToRecord } from './backend'

import { DeskRequestError } from './api'
import desk from './desk.json'
import drafts from './drafts.json'
import reviews from './reviews.json'
import rules from './rules.json'
import { visibleWords } from '~/utils/draft'
import type {
  Brief,
  DeskSource,
  Disposition,
  DraftDetail,
  Finding,
  Inbox,
  InboxPr,
  NitTicket,
  RepoName,
  RepoSummary,
  Review,
  ReviewDetail,
  ReviewEvent,
  Rule,
  RuleStage,
  RulesPage,
} from '~/types/desk'

/** The repository filter's value that means every repository. */
export const ALL_REPOSITORIES = 'all'

/*
 * `7`, not `number`. The contract declares `layers_total` as a constant — the screen's
 * "Layer 5 of 7" needs no constant of its own — and the generated type says so, so this one
 * has to be the literal rather than widen.
 */
const LAYERS_TOTAL = 7 as const

/** The whole desk, cloned so a write can never reach the fixture files. */
export function loadSource(): DeskSource {
  return structuredClone({
    reviews: reviews as unknown as ReviewDetail[],
    drafts: drafts as unknown as DraftDetail[],
    rules: rules as unknown as Rule[],
    summary: desk.summary,
    repos: desk.repos,
    own_prs: desk.own_prs,
    handled_without_you: desk.handled_without_you,
    session_liveness: desk.session_liveness as Record<string, boolean | null>,
    github_available: desk.github_available,
  })
}

const source = reactive<DeskSource>(loadSource())

/**
 * Put the desk back as it loaded, optionally with some of it replaced.
 *
 * Tests use this to reach a state no fixture of the record could express — a machine where
 * `gh` cannot be read, for instance, or a database with nothing in it. No screen calls it.
 */
export function resetSource(overrides: Partial<DeskSource> = {}) {
  Object.assign(source, loadSource(), overrides)
}

/** The desk as it stands, for a test that wants to read it without a screen. */
export function currentSource(): DeskSource {
  return source
}

function nowIso(): string {
  return new Date().toISOString()
}

/* ------------------------------- The refusals -------------------------------- */

/** The server's `not_found`, for a review, finding, draft or rule that is not there. */
function notFound(what: string): DeskRequestError {
  return new DeskRequestError({
    code: 'not_found',
    message: `Review Desk holds no ${what}.`,
    field: null,
    expected: null,
    status: 404,
  })
}

/** The server's `invalid_value`, which has to say what it would have accepted. */
function invalidValue(field: string, message: string, expected: string): DeskRequestError {
  return new DeskRequestError({
    code: 'invalid_value',
    message,
    field,
    expected,
    status: 400,
  })
}

/** The server's `conflict`: a statement about the record, not about the request. */
function conflict(message: string): DeskRequestError {
  return new DeskRequestError({
    code: 'conflict',
    message,
    field: null,
    expected: null,
    status: 409,
  })
}

/* ------------------------------- Reading it ---------------------------------- */

function reviewById(id: number): ReviewDetail | undefined {
  return source.reviews.find(item => item.review.id === id)
}

function draftById(id: number): DraftDetail | undefined {
  return source.drafts.find(item => item.draft.review_id === id)
}

function prOf(detail: ReviewDetail): InboxPr {
  return {
    repo: detail.review.repo,
    pr: detail.review.pr,
    title: detail.review.title ?? null,
  }
}

/** Recount the tallies the API serves precomputed, after a disposition has changed. */
function retally(detail: ReviewDetail) {
  const severities = { blocker: 0, should: 0, nit: 0, question: 0 }
  const dispositions = { in_draft: 0, held_back: 0, suppressed: 0 }
  for (const finding of detail.findings) {
    severities[finding.severity] += 1
    dispositions[finding.disposition] += 1
  }
  detail.finding_severities = severities
  detail.finding_dispositions = dispositions
}

/** Re-run the review lint over a draft body, the way the server does on every save. */
function relint(detail: DraftDetail) {
  const owner = reviewById(detail.draft.review_id)
  const expected = (owner?.findings ?? []).filter(f => f.disposition === 'in_draft')
  const present = expected.filter(f => new RegExp(`\\b${f.key}\\b`).test(detail.draft.body))
  const words = visibleWords(detail.draft.body)
  const budget = detail.lint?.word_budget ?? null
  detail.lint = {
    ...detail.lint,
    visible_words: words,
    findings_in_body: present.length,
    findings_expected: expected.length,
    passes: (budget == null || words <= budget) && present.length === expected.length,
    checked_at: nowIso(),
  }
}

function isActive(rule: Rule): boolean {
  return rule.stage === 'active' || rule.stage === 'lint' || rule.stage === 'ci'
}

function inScope(rule: Rule, repo: string | null): boolean {
  if (repo == null) return true
  return rule.scope.kind === 'global' || rule.scope.repo === repo
}

/** The whole Inbox, recomputed from the desk — the shape `GET /inbox` serves. */
function inbox(): Inbox {
  const briefs_to_confirm = source.reviews
    .filter(detail => detail.brief && !detail.brief.confirmed)
    .map(detail => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      ticket_id: detail.review.ticket_id ?? null,
      brief_ready_at: detail.brief!.updated_at,
      /* The session held at layer 2 only if nothing after it has run. */
      blocks_later_layers: detail.layers
        .filter(item => item.number > 2)
        .every(item => item.state === 'pending'),
    }))

  /*
   * **Every ready draft, confirmed brief or not**, which is what the server does and what
   * this filtered out until DEV-862's end-to-end test caught the two disagreeing.
   *
   * The Inbox is what is waiting on the developer, and a draft at `ready` is waiting on
   * them whatever layer 2 says. A session may run ahead of the confirmation — the contract
   * gives the brief item `blocks_later_layers` precisely to say whether it did — so hiding
   * the draft would mean a review whose session ran ahead showed nothing to do while a
   * finished draft sat behind it.
   */
  const drafts_to_approve = source.drafts
    .filter(detail => detail.draft.status === 'ready')
    .map(detail => ({ detail, owner: reviewById(detail.draft.review_id) }))
    .filter(pair => pair.owner != null)
    .map(({ owner }) => ({
      review_id: owner!.review.id,
      pull_request: prOf(owner!),
      round: owner!.review.round,
      severities: owner!.finding_severities,
      held_back: owner!.finding_dispositions.held_back,
      is_delta: owner!.review.round > 1,
    }))

  const author_replies = source.reviews
    .map(detail => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      events: detail.outcomes
        .filter(outcome => outcome.actor !== 'developer')
        .map(outcome => ({
          finding_seq: outcome.finding_seq,
          finding_key: outcome.finding_key,
          kind: outcome.kind,
          detail: outcome.detail ?? null,
        })),
    }))
    .filter(item => item.events.length > 0)

  const proposed_rules = source.rules
    .filter(rule => rule.stage === 'proposed')
    .map(rule => ({
      rule_id: rule.id,
      rule_key: rule.key,
      statement: rule.statement,
      why_proposed: rule.why_proposed ?? null,
      scope: rule.scope,
    }))

  const in_progress = source.reviews
    .map(detail => ({ detail, running: detail.layers.find(item => item.state === 'running') }))
    .filter(pair => pair.running != null && pair.detail.draft_status === 'drafting')
    .map(({ detail, running }) => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      layer_number: running!.number,
      layer_name: running!.name,
      layers_total: LAYERS_TOTAL,
      session_id: detail.review.session_id ?? null,
      /*
       * Allele owns session liveness, not Review Desk. A review nothing has reported on
       * is null, and the screen says so rather than assuming the session is alive.
       */
      session_alive: source.session_liveness[String(detail.review.id)] ?? null,
    }))

  const groups = [
    briefs_to_confirm,
    drafts_to_approve,
    author_replies,
    source.own_prs,
    proposed_rules,
  ]
  const repos = new Set<string>()
  for (const item of [
    ...briefs_to_confirm,
    ...drafts_to_approve,
    ...author_replies,
    ...source.own_prs,
  ]) {
    repos.add(item.pull_request.repo)
  }
  for (const rule of proposed_rules) {
    if (rule.scope.repo) repos.add(rule.scope.repo)
  }

  return {
    totals: {
      items: groups.reduce((total, group) => total + group.length, 0),
      repos: repos.size,
    },
    briefs_to_confirm,
    drafts_to_approve,
    author_replies,
    own_prs_without_reviewer: source.own_prs,
    proposed_rules,
    in_progress,
    handled_without_you: source.handled_without_you,
  }
}

/** The Rules screen for one repository, or for all of them. */
function rulesPage(repo: string | null): RulesPage {
  const scoped = source.rules.filter(rule => inScope(rule, repo))
  const active = scoped.filter(rule => isActive(rule) && !rule.stale)
  return {
    repo: (repo as RepoName | null) ?? null,
    summary: { ...source.summary, active_rules: active.length },
    proposed: scoped.filter(rule => rule.stage === 'proposed'),
    active,
    stale: scoped.filter(rule => rule.stale && rule.stage !== 'retired'),
  }
}

/* ------------------------------- The writes ---------------------------------- */

/**
 * `POST /reviews` — the developer pastes a link and nothing else.
 *
 * The link is checked the way `crates/server/src/wire.rs` checks it, and in the same order:
 * a malformed link is refused before anything reaches for GitHub, so a machine with no
 * `gh` is told its link is wrong rather than told `gh` is missing.
 */
function startReview(prUrl: string): Review {
  const match = prUrl
    .trim()
    .match(/^(?:https?:\/\/github\.com\/)?([\w.-]+\/[\w.-]+)(?:\/pull\/|#)(\d+)\/?$/)
  if (!match) {
    throw invalidValue(
      'pr_url',
      'Paste a link like github.com/acme/portal/pull/412.',
      'a pull request link such as https://github.com/acme/portal/pull/412',
    )
  }

  const repo = match[1]!
  const pr = Number(match[2])

  if (source.reviews.some(item => item.review.repo === repo && item.review.pr === pr)) {
    throw conflict(`${repo}#${pr} is already on the desk.`)
  }

  /* No head commit was sent, so the server reads the pull request to resolve one. */
  if (!source.github_available) {
    throw new DeskRequestError({
      code: 'github_unavailable',
      message:
        `Could not read ${repo}#${pr}: gh is not logged in. Run \`gh auth login\`, then `
        + 'try again.',
      field: null,
      expected: null,
      status: 502,
    })
  }

  /*
   * Sample mode records nothing: there is no store behind it and a review invented here
   * would have no brief, no findings and no draft, which would put a row on the Inbox that
   * every screen then showed as empty. The refusal says so, in the one place the real API
   * would have succeeded.
   */
  throw conflict(
    `${repo}#${pr} is a real pull request and these records are invented. Start `
    + 'Review Desk with `review-desk serve` to open a review on it.',
  )
}

/**
 * `POST /reviews/{id}/brief/confirm` — layer 2, and the gate on everything after it.
 *
 * **It sets the confirmation and touches nothing else**, which is what the server does.
 * This used to move layer 2 to `done` and layer 6 to `waiting_on_you` as well, and the
 * server never has: a review's layer rows are written by the layer that ran, through
 * `review-desk layer set`, and the contract says they are stored rather than derived
 * precisely so that a skipped layer can be told from one that has not run.
 *
 * The visible consequence is that the layer trail still reads "2 Understand · waiting on
 * you" after the developer has confirmed. That is what the record says, and the screen is
 * right to show it — the record is what is wrong. Left as it is and reported rather than
 * fixed here, because fixing it is a change to what `confirmBrief` writes, which is beyond
 * DEV-862.
 */
function confirmBrief(id: number): Brief {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)
  if (!detail.brief) throw conflict(`Review ${id} has no brief to confirm.`)
  if (detail.brief.confirmed) return detail.brief

  detail.brief.confirmed = true
  detail.brief.confirmed_at = nowIso()
  return detail.brief
}

/**
 * `PATCH /reviews/{id}/findings/{seq}` — move a finding between the draft, held back and
 * suppressed. A finding is never deleted; the reason is what a later rule cites.
 */
function setFindingDisposition(
  reviewId: number,
  seq: number,
  disposition: Disposition,
  reason: string | null,
): Finding {
  const detail = reviewById(reviewId)
  const finding = detail?.findings.find(item => item.seq === seq)
  if (!detail || !finding) throw notFound(`finding ${seq} on review ${reviewId}`)

  const trimmed = (reason ?? '').trim()
  if (disposition !== 'in_draft' && trimmed.length === 0) {
    throw invalidValue(
      'reason',
      'Say why it is not going in the draft. A finding removed without a reason teaches '
      + 'the harvester nothing.',
      'a sentence saying why',
    )
  }

  finding.disposition = disposition
  finding.disposition_reason = disposition === 'in_draft' ? null : trimmed
  retally(detail)

  /* The ticket covers whatever is held back now, not whatever was held back before. */
  if (detail.nit_ticket.status === 'none') {
    detail.nit_ticket.finding_seqs = detail.findings
      .filter(item => item.disposition === 'held_back')
      .map(item => item.seq)
  }

  const drafted = draftById(reviewId)
  if (drafted) {
    drafted.held_back = detail.findings
      .filter(item => item.disposition === 'held_back')
      .map(item => ({
        finding_seq: item.seq,
        finding_key: item.key,
        claim: item.claim,
        severity: item.severity,
      }))
    drafted.nit_ticket = detail.nit_ticket
    relint(drafted)
  }

  return finding
}

/** `PUT /reviews/{id}/draft` — the body, plus the edits this save represents. */
function saveDraft(id: number, body: string, edits: DraftEditToRecord[]): DraftDetail {
  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)

  if (detail.draft.status === 'approved' || detail.draft.status === 'posted') {
    throw conflict(
      `This draft is ${detail.draft.status} and cannot be edited: an approved draft is `
      + 'what the review session reads and posts.',
    )
  }

  const owner = reviewById(id)
  const at = nowIso()

  detail.draft.body = body
  detail.draft.updated_at = at

  for (const edit of edits) {
    const finding = owner?.findings.find(item => item.seq === edit.finding_seq)
    if (!finding) throw notFound(`finding ${edit.finding_seq} on review ${id}`)
    if (edit.summary.trim().length === 0) {
      throw invalidValue(
        'edits[].summary',
        `The edit to ${finding.key} has an empty summary.`,
        'what changed, in a few words',
      )
    }

    const existing = detail.edits.find(
      item => item.finding_seq === edit.finding_seq && item.kind === edit.kind,
    )
    if (existing) {
      existing.summary = edit.summary
      existing.occurred_at = at
    }
    else {
      detail.edits.push({ ...edit, finding_key: finding.key, occurred_at: at })
    }

    if (edit.kind === 'cut' && finding.disposition === 'in_draft' && owner) {
      finding.disposition = 'suppressed'
      finding.disposition_reason = 'Cut from the draft before posting'
      retally(owner)
    }
  }

  relint(detail)
  return detail
}

/** `POST /reviews/{id}/draft/approve` — approved here, posted by the review session. */
function approveDraft(id: number, event: ReviewEvent): DraftDetail {
  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)
  if (detail.draft.status !== 'ready') {
    throw conflict(
      `This draft is ${detail.draft.status} and only a draft that is ready can be `
      + 'approved.',
    )
  }

  detail.draft.status = 'approved'
  detail.draft.event = event
  detail.draft.approved_at = nowIso()
  detail.draft.sent_back_note = null
  detail.draft.sent_back_at = null

  /* `draft_status` is derived from the draft, so it follows; the layer rows do not. */
  const owner = reviewById(id)
  if (owner) owner.draft_status = 'approved'

  return detail
}

/** `POST /reviews/{id}/draft/send-back` — refused without a note, as the API refuses it. */
function sendBackDraft(id: number, note: string): DraftDetail {
  const trimmed = note.trim()
  if (trimmed.length === 0) {
    throw invalidValue(
      'note',
      'Say what the session is to change. A draft sent back with no note teaches it '
      + 'nothing.',
      'what the session is to change',
    )
  }

  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)
  if (detail.draft.status !== 'ready') {
    throw conflict(
      `This draft is ${detail.draft.status} and only a draft that is ready can be sent `
      + 'back.',
    )
  }

  detail.draft.status = 'sent_back'
  detail.draft.sent_back_note = trimmed
  detail.draft.sent_back_at = nowIso()
  detail.draft.event = null

  const owner = reviewById(id)
  if (owner) owner.draft_status = 'sent_back'

  return detail
}

/** `POST /reviews/{id}/held-back/ticket` — Review Desk records it; an agent files it. */
function requestNitTicket(id: number, ticketId: string | null = null): NitTicket {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)

  const seqs = detail.findings
    .filter(finding => finding.disposition === 'held_back')
    .map(finding => finding.seq)
  if (seqs.length === 0) {
    throw conflict(`Review ${id} holds back no nits, so there is nothing to file.`)
  }

  detail.nit_ticket = {
    ...detail.nit_ticket,
    status: ticketId == null ? 'requested' : 'filed',
    ticket_id: ticketId ?? detail.nit_ticket.ticket_id ?? null,
    finding_seqs: seqs,
    requested_at: detail.nit_ticket.requested_at ?? nowIso(),
    filed_at: ticketId == null ? detail.nit_ticket.filed_at ?? null : nowIso(),
  }

  const drafted = draftById(id)
  if (drafted) drafted.nit_ticket = detail.nit_ticket
  return detail.nit_ticket
}

/** The stage that follows this one, or `null` at the end of the line. */
function nextStage(stage: RuleStage): RuleStage | null {
  if (stage === 'proposed') return 'active'
  if (stage === 'active') return 'lint'
  if (stage === 'lint') return 'ci'
  return null
}

/** How the Rules screen words a stage. */
function stageLabel(rule: Rule, stage: RuleStage): string {
  const base
    = stage === 'active'
      ? rule.scope.kind === 'global' ? 'Global rule' : 'Repository rule'
      : stage === 'lint'
        ? 'Lint check'
        : stage === 'ci'
          ? 'CI check'
          : stage === 'proposed' ? 'Proposed' : 'Retired'
  const exceptions = rule.counts.exceptions ?? 0
  if (exceptions === 1) return `${base} · 1 exception`
  if (exceptions > 1) return `${base} · ${exceptions} exceptions`
  return base
}

function ruleById(id: number): Rule {
  const rule = source.rules.find(item => item.id === id)
  if (!rule) throw notFound(`rule ${id}`)
  return rule
}

/** `POST /rules/{id}/promote` — the developer promotes; the app never promotes itself. */
function promoteRule(id: number, toStage: RuleStage): Rule {
  const rule = ruleById(id)
  const next = nextStage(rule.stage)
  if (next !== toStage) {
    throw conflict(
      `A rule at ${rule.stage} cannot be promoted to ${toStage}: `
      + (next == null ? 'there is no stage after it.' : `the stage that follows is ${next}.`),
    )
  }

  rule.stage = toStage
  rule.why_proposed = null
  rule.stale = false
  rule.stage_label = stageLabel(rule, toStage)
  rule.note = 'Promoted by you. Nothing cites it yet.'
  rule.counts = { ...rule.counts, raised: rule.counts.raised ?? 0, kept: rule.counts.kept ?? 0 }
  return rule
}

/** The reason is required on both, so a rule is not proposed again next week. */
function retireWithReason(id: number, reason: string, what: string): Rule {
  const trimmed = reason.trim()
  if (trimmed.length === 0) {
    throw invalidValue(
      'reason',
      `A rule ${what} with no reason gets proposed again next week.`,
      'why',
    )
  }

  const rule = ruleById(id)
  rule.stage = 'retired'
  rule.stale = false
  rule.stage_label = 'Retired'
  rule.note = `${what === 'dismissed' ? 'Dismissed' : 'Retired'} by you: ${trimmed}`
  return rule
}

/** `POST /rules/{id}/defer` — keep it, and ask again later. */
function deferRule(id: number, days: number): Rule {
  if (!Number.isInteger(days) || days < 1 || days > 365) {
    throw invalidValue(
      'days',
      `${days} is not a period to keep a rule for.`,
      'a whole number of days from 1 to 365',
    )
  }

  const rule = ruleById(id)
  rule.stale = false
  rule.review_again_at = new Date(Date.now() + days * 86_400_000).toISOString()
  rule.note = `Kept for ${days} ${days === 1 ? 'day' : 'days'}. Asked about again after that.`
  return rule
}

/**
 * A read's answer, as a plain object nothing else holds a reference to.
 *
 * **The API hands back a fresh object every read, because it parses fresh JSON, and this
 * has to as well.** Without the clone a read returned the live reactive record the writes
 * mutate in place, which made sample mode behave differently from the server in a way no
 * test was looking at: a component watching `() => props.finding` to notice that its
 * finding had changed never fired, because the object was the same one. The two backends
 * are meant to be indistinguishable, and identity is part of that.
 *
 * It also closes a second hole. A screen handed the live record could have written to it —
 * reaching past the data layer into the fixtures, which is the one thing
 * `test/architecture.spec.ts` exists to prevent and could not have caught.
 *
 * **A JSON round trip rather than `structuredClone`**, and not only because the latter
 * throws on a Vue proxy: this is precisely what the real answer goes through, so sample
 * mode now differs from the API in no way a caller could detect — including on the fields
 * the wire cannot carry.
 */
function answered<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

/**
 * The sixteen operations over the records in memory.
 *
 * Every one is `async` so that the composable above cannot tell which backend it has. They
 * resolve immediately — there is nothing to wait for — which means a loading state is not
 * exercised here; `test/api.spec.ts` exercises that against `api.ts` with a stubbed
 * `fetch`, where it is real.
 */
export function sampleBackend(): DeskBackend {
  return {
    isSampleData: true,

    getInbox: async () => answered(inbox()),
    getReview: async (id) => {
      const detail = reviewById(id)
      if (!detail) throw notFound(`review ${id}`)
      return answered(detail)
    },
    getDraft: async (id) => {
      const detail = draftById(id)
      if (!detail) throw notFound(`draft on review ${id}`)
      return answered(detail)
    },
    listRules: async repo => answered(rulesPage(repo)),
    listRepos: async (): Promise<RepoSummary[]> =>
      answered(source.repos.map(repo => ({
        ...repo,
        active_rules: source.rules.filter(
          rule => isActive(rule) && (rule.scope.kind === 'global' || rule.scope.repo === repo.repo),
        ).length,
      }))),

    startReview: async link => answered(startReview(link)),
    confirmBrief: async id => answered(confirmBrief(id)),
    setFindingDisposition: async (reviewId, seq, disposition, reason) =>
      answered(setFindingDisposition(reviewId, seq, disposition, reason)),
    saveDraft: async (id, body, edits) => answered(saveDraft(id, body, edits)),
    approveDraft: async (id, event) => answered(approveDraft(id, event)),
    sendBackDraft: async (id, note) => answered(sendBackDraft(id, note)),
    requestNitTicket: async (id, ticketId = null) => answered(requestNitTicket(id, ticketId)),
    promoteRule: async (id, toStage) => answered(promoteRule(id, toStage)),
    dismissRule: async (id, reason) => answered(retireWithReason(id, reason, 'dismissed')),
    retireRule: async (id, reason) => answered(retireWithReason(id, reason, 'retired')),
    deferRule: async (id, days) => answered(deferRule(id, days)),

    /* Nothing to be unreachable. Sample mode is always answering. */
    health: async () => ({ status: 'ok', schema_version: 0 }),
  }
}
