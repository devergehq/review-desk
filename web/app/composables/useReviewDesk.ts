/*
 * The data layer. Every screen reads and writes the desk through this composable and
 * through nothing else, which is what makes pointing the dashboard at the Rust server a
 * change in `app/data/snapshot.ts` rather than a change in every screen.
 *
 * The four reads mirror the four the API offers — one GET per screen, so no screen joins
 * anything:
 *
 *   inbox()          GET /inbox
 *   review(id)       GET /reviews/{id}
 *   draft(id)        GET /reviews/{id}/draft
 *   rules(repo)      GET /rules?repo=
 *
 * The writes mirror its operations, one function each. While the records are sample data
 * a write patches the desk held in memory and the reads recompute, so a flow can be
 * clicked through; when the API exists each write becomes its call followed by a re-read,
 * and the patching here is deleted rather than rewritten.
 *
 * Nothing is posted to GitHub from here. The developer approves a draft and the review
 * session posts it — that is the whole point of the preview.
 */

import { computed, reactive, type ComputedRef } from 'vue'
import { isSampleData, loadSource } from '~/data/snapshot'
import type {
  DeskSource,
  Disposition,
  DraftDetail,
  DraftEdit,
  Inbox,
  InboxPr,
  ReviewDetail,
  ReviewEvent,
  RepoName,
  RepoSummary,
  Rule,
  RuleStage,
  RulesPage,
} from '~/types/desk'

/** The repository filter's value that means every repository. */
export const ALL_REPOSITORIES = 'all'

const LAYERS_TOTAL = 7

/** A disposition in a sentence about what just happened to a finding. */
const LABELS: Record<Disposition, string> = {
  in_draft: 'in the draft',
  held_back: 'held back for a ticket',
  suppressed: 'suppressed',
}

const source = reactive<DeskSource>(loadSource())

function nowIso(): string {
  return new Date().toISOString()
}

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

function layer(detail: ReviewDetail, number: number) {
  return detail.layers.find(item => item.number === number)
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
  const budget = detail.lint.word_budget ?? null
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

function inScope(rule: Rule, repo: string): boolean {
  if (repo === ALL_REPOSITORIES) return true
  return rule.scope.kind === 'global' || rule.scope.repo === repo
}

export function useReviewDesk() {
  /* ------------------------------- GET /inbox ------------------------------- */

  const inbox = computed<Inbox>(() => {
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

    const drafts_to_approve = source.drafts
      .filter(detail => detail.draft.status === 'ready')
      .map(detail => ({ detail, owner: reviewById(detail.draft.review_id) }))
      .filter(pair => pair.owner?.brief?.confirmed)
      .map(({ detail, owner }) => ({
        review_id: detail.draft.review_id,
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
         * Allele owns session liveness, not Review Desk. A review nothing has reported
         * on is null, and the screen says so rather than assuming the session is alive.
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
  })

  /* --------------------------- GET /reviews/{id} ---------------------------- */

  function review(id: number): ComputedRef<ReviewDetail | null> {
    return computed(() => reviewById(id) ?? null)
  }

  /* ------------------------ GET /reviews/{id}/draft ------------------------- */

  function draft(id: number): ComputedRef<DraftDetail | null> {
    return computed(() => draftById(id) ?? null)
  }

  /* ----------------------------- GET /repos --------------------------------- */

  const repos = computed<RepoSummary[]>(() =>
    source.repos.map(repo => ({
      ...repo,
      active_rules: source.rules.filter(
        rule => isActive(rule) && (rule.scope.kind === 'global' || rule.scope.repo === repo.repo),
      ).length,
    })),
  )

  /* --------------------------- GET /rules?repo= ----------------------------- */

  function rules(repo: ComputedRef<string> | { value: string }): ComputedRef<RulesPage> {
    return computed(() => {
      const scoped = source.rules.filter(rule => inScope(rule, repo.value))
      const active = scoped.filter(rule => isActive(rule) && !rule.stale)
      return {
        repo: repo.value === ALL_REPOSITORIES ? null : (repo.value as RepoName),
        summary: { ...source.summary, active_rules: active.length },
        proposed: scoped.filter(rule => rule.stage === 'proposed'),
        active,
        stale: scoped.filter(rule => rule.stale && rule.stage !== 'retired'),
      }
    })
  }

  /* ----------------------------- The writes --------------------------------- */

  /** POST /reviews — the developer pastes a link; the session starts at the brief. */
  function startReview(link: string): { ok: boolean, message: string } {
    const match = link
      .trim()
      .match(/^(?:https?:\/\/github\.com\/)?([\w.-]+\/[\w.-]+)(?:\/pull\/|#)(\d+)\/?$/)
    if (!match) {
      return { ok: false, message: 'Paste a link like github.com/acme/portal/pull/412.' }
    }
    const repo = match[1]!
    const pr = Number(match[2])
    if (source.reviews.some(item => item.review.repo === repo && item.review.pr === pr)) {
      return { ok: false, message: `${repo}#${pr} is already on the desk.` }
    }
    return {
      ok: true,
      message: `${repo}#${pr} queued. The session reads the problem first and stops there.`,
    }
  }

  /** POST /reviews/{id}/brief/confirm — layer 2, and the gate on everything after it. */
  function confirmBrief(id: number) {
    const detail = reviewById(id)
    if (!detail?.brief || detail.brief.confirmed) return
    detail.brief.confirmed = true
    detail.brief.confirmed_at = nowIso()

    const understand = layer(detail, 2)
    if (understand) {
      understand.state = 'done'
      understand.detail = 'confirmed by you'
      understand.updated_at = nowIso()
    }

    const drafted = draftById(id)
    const draftLayer = layer(detail, 6)
    if (draftLayer && drafted?.draft.status === 'ready') {
      draftLayer.state = 'waiting_on_you'
      draftLayer.detail = 'waiting on you'
      draftLayer.updated_at = nowIso()
    }
    else if (draftLayer) {
      draftLayer.state = 'running'
      draftLayer.detail = 'running'
      draftLayer.updated_at = nowIso()
    }
  }

  /**
   * PATCH /reviews/{id}/findings/{seq} — move a finding between the draft, held back and
   * suppressed. A finding is never deleted; the reason is what a later rule cites.
   */
  function setFindingDisposition(
    reviewId: number,
    seq: number,
    disposition: Disposition,
    reason: string,
  ): { ok: boolean, message: string } {
    const detail = reviewById(reviewId)
    const finding = detail?.findings.find(item => item.seq === seq)
    if (!detail || !finding) return { ok: false, message: 'There is no such finding.' }
    const trimmed = reason.trim()
    if (disposition !== 'in_draft' && trimmed.length === 0) {
      return { ok: false, message: 'Say why it is not going in the draft.' }
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
    return { ok: true, message: `${finding.key} is now ${LABELS[disposition]}.` }
  }

  /**
   * PUT /reviews/{id}/draft — the body, plus the edits this save represents. Returns the
   * edits this save made, which is what the screen reports back; the whole history stays
   * on the draft.
   */
  function saveDraft(id: number, body: string): DraftEdit[] {
    const detail = draftById(id)
    if (!detail || detail.draft.body === body) return []
    const owner = reviewById(id)
    const detected = detectEdits(detail.draft.body, body, owner?.findings ?? [])
    const at = nowIso()

    detail.draft.body = body
    detail.draft.updated_at = at
    for (const edit of detected) {
      const existing = detail.edits.find(
        item => item.finding_seq === edit.finding_seq && item.kind === edit.kind,
      )
      if (existing) {
        existing.summary = edit.summary
        existing.occurred_at = at
      }
      else {
        detail.edits.push({ ...edit, occurred_at: at })
      }
      if (edit.kind === 'cut' && owner) {
        const finding = owner.findings.find(item => item.seq === edit.finding_seq)
        if (finding && finding.disposition === 'in_draft') {
          finding.disposition = 'suppressed'
          finding.disposition_reason = 'Cut from the draft before posting'
          retally(owner)
        }
      }
    }
    relint(detail)
    const at_this_save = new Set(detected.map(edit => `${edit.finding_seq}:${edit.kind}`))
    return detail.edits.filter(edit => at_this_save.has(`${edit.finding_seq}:${edit.kind}`))
  }

  /** POST /reviews/{id}/draft/approve — approved here, posted by the review session. */
  function approveDraft(id: number, event: ReviewEvent) {
    const detail = draftById(id)
    if (!detail) return
    detail.draft.status = 'approved'
    detail.draft.event = event
    detail.draft.approved_at = nowIso()
    detail.draft.sent_back_note = null
    detail.draft.sent_back_at = null

    const owner = reviewById(id)
    if (!owner) return
    owner.draft_status = 'approved'
    const draftLayer = layer(owner, 6)
    if (draftLayer) {
      draftLayer.state = 'done'
      draftLayer.detail = 'approved by you'
      draftLayer.updated_at = nowIso()
    }
    const harvest = layer(owner, 7)
    if (harvest && harvest.state === 'pending') {
      harvest.detail = 'once the session posts it'
    }
  }

  /** POST /reviews/{id}/draft/send-back — refused without a note, as the API refuses it. */
  function sendBackDraft(id: number, note: string): { ok: boolean, message: string } {
    const trimmed = note.trim()
    if (trimmed.length === 0) {
      return { ok: false, message: 'Say what the session is to change. A bare no teaches nothing.' }
    }
    const detail = draftById(id)
    if (!detail) return { ok: false, message: 'There is no draft to send back.' }
    detail.draft.status = 'sent_back'
    detail.draft.sent_back_note = trimmed
    detail.draft.sent_back_at = nowIso()
    detail.draft.event = null

    const owner = reviewById(id)
    if (owner) {
      owner.draft_status = 'sent_back'
      const draftLayer = layer(owner, 6)
      if (draftLayer) {
        draftLayer.state = 'running'
        draftLayer.detail = 'sent back with a note'
        draftLayer.updated_at = nowIso()
      }
    }
    return { ok: true, message: 'Sent back. The session has the note.' }
  }

  /** POST /reviews/{id}/held-back/ticket — Review Desk records it; an agent files it. */
  function requestNitTicket(id: number): string {
    const detail = reviewById(id)
    const drafted = draftById(id)
    if (!detail || detail.nit_ticket.status !== 'none') {
      return 'A ticket has already been asked for.'
    }
    const seqs = detail.findings
      .filter(finding => finding.disposition === 'held_back')
      .map(finding => finding.seq)
    detail.nit_ticket = {
      ...detail.nit_ticket,
      status: 'requested',
      finding_seqs: seqs,
      requested_at: nowIso(),
    }
    if (drafted) drafted.nit_ticket = detail.nit_ticket
    return `${count(seqs.length, 'finding')} queued as one ticket. An agent files it.`
  }

  /** POST /rules/{id}/promote — the developer promotes; the app never promotes itself. */
  function promoteRule(id: number, toStage: RuleStage = 'active') {
    const rule = source.rules.find(item => item.id === id)
    if (!rule) return
    rule.stage = toStage
    rule.why_proposed = null
    rule.stale = false
    rule.stage_label
      = toStage === 'active'
        ? rule.scope.kind === 'global'
          ? 'Global rule'
          : 'Repository rule'
        : toStage === 'ci'
          ? 'CI check'
          : toStage === 'lint'
            ? 'Lint check'
            : 'Retired'
    rule.note = 'Promoted by you. Nothing cites it yet.'
    rule.counts = { ...rule.counts, raised: rule.counts.raised ?? 0, kept: rule.counts.kept ?? 0 }
  }

  /** POST /rules/{id}/dismiss — the reason is required, so it is not proposed again. */
  function dismissRule(id: number, reason: string): { ok: boolean, message: string } {
    const trimmed = reason.trim()
    if (trimmed.length === 0) {
      return { ok: false, message: 'Say why. Without a reason it is proposed again next week.' }
    }
    const rule = source.rules.find(item => item.id === id)
    if (!rule) return { ok: false, message: 'That rule is no longer listed.' }
    source.rules.splice(source.rules.indexOf(rule), 1)
    return { ok: true, message: `${rule.key} dismissed: ${trimmed}` }
  }

  /** POST /rules/{id}/retire — a rule that has stopped earning its place. */
  function retireRule(id: number, reason: string): { ok: boolean, message: string } {
    const trimmed = reason.trim()
    if (trimmed.length === 0) {
      return { ok: false, message: 'Say why it is being retired.' }
    }
    const rule = source.rules.find(item => item.id === id)
    if (!rule) return { ok: false, message: 'That rule is no longer listed.' }
    rule.stage = 'retired'
    rule.stale = false
    rule.stage_label = 'Retired'
    rule.note = `Retired by you: ${trimmed}`
    return { ok: true, message: `${rule.key} retired.` }
  }

  /** POST /rules/{id}/defer — keep it, and ask again later. */
  function deferRule(id: number, days: number): string {
    const rule = source.rules.find(item => item.id === id)
    if (!rule) return 'That rule is no longer listed.'
    const again = new Date(Date.now() + days * 86_400_000).toISOString()
    rule.stale = false
    rule.review_again_at = again
    rule.note = `Kept for ${count(days, 'day')}. Asked about again after that.`
    return `${rule.key} kept for ${count(days, 'day')}.`
  }

  return {
    isSampleData,
    inbox,
    review,
    draft,
    repos,
    rules,
    startReview,
    confirmBrief,
    setFindingDisposition,
    saveDraft,
    approveDraft,
    sendBackDraft,
    requestNitTicket,
    promoteRule,
    dismissRule,
    retireRule,
    deferRule,
  }
}

/** Put the desk back as it loaded. Tests use this; no screen does. */
export function resetDesk() {
  Object.assign(source, loadSource())
}
