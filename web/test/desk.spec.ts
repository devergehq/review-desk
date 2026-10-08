import { beforeEach, describe, expect, it } from 'vitest'
import { ALL_REPOSITORIES, resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'
import { computed, ref } from 'vue'

/*
 * The data layer, which is where every write on the four screens ends up. DEV-855 asks
 * that confirming a brief removes it from the inbox and that approving a draft changes
 * its status; this is where those are checked.
 *
 * Each write mirrors one API operation, so what is asserted here is also the behaviour
 * expected of the Rust server once the fixtures are gone.
 */

const PORTAL = 'acme/portal'
const WEBHOOKS = 17 /* #412, the brief that is waiting */
const INVOICES = 15 /* #407, a draft that is waiting */
const CACHE = 16 /* #88, round 2 */
const EXPORT = 12 /* #398, posted, with author replies */

describe('the inbox', () => {
  beforeEach(() => resetDesk())

  it('groups seven waiting items across two repositories', () => {
    const { inbox } = useReviewDesk()
    expect(inbox.value.totals).toEqual({ items: 7, repos: 2 })
  })

  it('has all five groups filled', () => {
    const { inbox } = useReviewDesk()
    expect(inbox.value.briefs_to_confirm).toHaveLength(1)
    expect(inbox.value.drafts_to_approve).toHaveLength(2)
    expect(inbox.value.author_replies).toHaveLength(1)
    expect(inbox.value.own_prs_without_reviewer).toHaveLength(2)
    expect(inbox.value.proposed_rules).toHaveLength(1)
  })

  it('shows two sessions running, one of them with unknown liveness', () => {
    const { inbox } = useReviewDesk()
    expect(inbox.value.in_progress).toHaveLength(2)
    expect(inbox.value.in_progress.map(item => item.session_alive)).toEqual([true, null])
    expect(inbox.value.in_progress[0]).toMatchObject({ layer_number: 5, layers_total: 7 })
  })

  it('says a draft already written is not holding its session at layer 2', () => {
    const { inbox } = useReviewDesk()
    expect(inbox.value.briefs_to_confirm[0]).toMatchObject({
      review_id: WEBHOOKS,
      blocks_later_layers: false,
    })
  })

  it('carries the author’s two kinds of reply', () => {
    const { inbox } = useReviewDesk()
    const kinds = inbox.value.author_replies[0]!.events.map(event => event.kind)
    expect(kinds).toEqual(['rejected', 'code_changed_after'])
  })
})

describe('confirming a brief', () => {
  beforeEach(() => resetDesk())

  it('takes the item out of the inbox and puts the draft in', () => {
    const { inbox, confirmBrief } = useReviewDesk()
    expect(inbox.value.briefs_to_confirm.map(item => item.review_id)).toContain(WEBHOOKS)
    expect(inbox.value.drafts_to_approve.map(item => item.review_id)).not.toContain(WEBHOOKS)

    confirmBrief(WEBHOOKS)

    expect(inbox.value.briefs_to_confirm).toHaveLength(0)
    expect(inbox.value.drafts_to_approve.map(item => item.review_id)).toContain(WEBHOOKS)
    expect(inbox.value.totals.items).toBe(7)
  })

  it('moves layer 2 to done and layer 6 to waiting', () => {
    const { review, confirmBrief } = useReviewDesk()
    const detail = review(WEBHOOKS)
    expect(detail.value!.layers[1]).toMatchObject({ state: 'waiting_on_you' })
    expect(detail.value!.layers[5]).toMatchObject({ state: 'pending' })

    confirmBrief(WEBHOOKS)

    expect(detail.value!.brief!.confirmed).toBe(true)
    expect(detail.value!.brief!.confirmed_at).not.toBeNull()
    expect(detail.value!.layers[1]).toMatchObject({ state: 'done', detail: 'confirmed by you' })
    expect(detail.value!.layers[5]).toMatchObject({ state: 'waiting_on_you' })
  })

  it('is idempotent', () => {
    const { review, confirmBrief } = useReviewDesk()
    confirmBrief(WEBHOOKS)
    const at = review(WEBHOOKS).value!.brief!.confirmed_at
    confirmBrief(WEBHOOKS)
    expect(review(WEBHOOKS).value!.brief!.confirmed_at).toBe(at)
  })
})

describe('moving a finding', () => {
  beforeEach(() => resetDesk())

  it('refuses to hold one back without a reason', () => {
    const { setFindingDisposition, review } = useReviewDesk()
    const result = setFindingDisposition(WEBHOOKS, 1, 'held_back', '   ')
    expect(result.ok).toBe(false)
    expect(review(WEBHOOKS).value!.findings[0]!.disposition).toBe('in_draft')
  })

  it('records the reason and recounts the dispositions', () => {
    const { setFindingDisposition, review } = useReviewDesk()
    expect(review(WEBHOOKS).value!.finding_dispositions).toEqual({
      in_draft: 3,
      held_back: 3,
      suppressed: 1,
    })

    const result = setFindingDisposition(WEBHOOKS, 2, 'suppressed', 'Covered by F1 already')
    expect(result.ok).toBe(true)

    const detail = review(WEBHOOKS).value!
    expect(detail.findings[1]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Covered by F1 already',
    })
    expect(detail.finding_dispositions).toEqual({ in_draft: 2, held_back: 3, suppressed: 2 })
  })

  it('clears the reason when a finding goes back in the draft', () => {
    const { setFindingDisposition, review } = useReviewDesk()
    setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')
    expect(review(WEBHOOKS).value!.findings[3]).toMatchObject({
      disposition: 'in_draft',
      disposition_reason: null,
    })
  })

  it('updates the draft’s held-back list and its lint', () => {
    const { setFindingDisposition, draft } = useReviewDesk()
    expect(draft(WEBHOOKS).value!.held_back.map(item => item.finding_key)).toEqual([
      'F4',
      'F5',
      'F7',
    ])
    expect(draft(WEBHOOKS).value!.lint.findings_expected).toBe(3)

    setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')

    const detail = draft(WEBHOOKS).value!
    expect(detail.held_back.map(item => item.finding_key)).toEqual(['F5', 'F7'])
    /* F4 is now expected in the body and is not there, so the lint fails loudly. */
    expect(detail.lint.findings_expected).toBe(4)
    expect(detail.lint.findings_in_body).toBe(3)
    expect(detail.lint.passes).toBe(false)
  })

  it('says so when there is no such finding', () => {
    const { setFindingDisposition } = useReviewDesk()
    expect(setFindingDisposition(WEBHOOKS, 99, 'in_draft', '')).toEqual({
      ok: false,
      message: 'There is no such finding.',
    })
  })
})

describe('the draft', () => {
  beforeEach(() => resetDesk())

  it('serves a body with a mermaid fence and a folded section', () => {
    const { draft } = useReviewDesk()
    const body = draft(WEBHOOKS).value!.draft.body
    expect(body).toContain('```mermaid')
    expect(body).toContain('<details>')
    expect(body).toContain('</details>')
  })

  it('counts the words actually read, against a budget', () => {
    const { draft } = useReviewDesk()
    const lint = draft(WEBHOOKS).value!.lint
    expect(lint.word_budget).toBe(150)
    expect(lint.visible_words).toBeLessThanOrEqual(150)
    expect(lint.passes).toBe(true)
  })

  it('flags a finding raised before on the same pull request', () => {
    const { draft } = useReviewDesk()
    expect(draft(CACHE).value!.lint.raised_before_on_pr).toBe(1)
  })

  it('records an edit against the finding it changed, and recounts', () => {
    const { draft, saveDraft } = useReviewDesk()
    const before = draft(WEBHOOKS).value!.draft.body
    expect(draft(WEBHOOKS).value!.edits).toHaveLength(3)

    const edits = saveDraft(
      WEBHOOKS,
      before.replace('I reverted the retry and', 'Reverting the retry and running it,'),
    )

    expect(edits.some(edit => edit.finding_key === 'F3' && edit.kind === 'rewritten')).toBe(true)
    expect(draft(WEBHOOKS).value!.edits).toHaveLength(4)
    expect(draft(WEBHOOKS).value!.lint.checked_at).not.toBe('')
  })

  it('suppresses a finding whose section the developer cut', () => {
    const { draft, review, saveDraft } = useReviewDesk()
    const before = draft(WEBHOOKS).value!.draft.body
    const after = before.slice(0, before.indexOf('### Should · F3'))

    saveDraft(WEBHOOKS, after)

    expect(review(WEBHOOKS).value!.findings[2]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Cut from the draft before posting',
    })
    expect(draft(WEBHOOKS).value!.edits.some(edit => edit.kind === 'cut' && edit.finding_key === 'F3'))
      .toBe(true)
  })

  it('records nothing when the body is unchanged', () => {
    const { draft, saveDraft } = useReviewDesk()
    const body = draft(WEBHOOKS).value!.draft.body
    expect(saveDraft(WEBHOOKS, body)).toEqual([])
    expect(draft(WEBHOOKS).value!.edits).toHaveLength(3)
  })

  it('approving changes the status, the event, and layer 6', () => {
    const { approveDraft, draft, review, inbox } = useReviewDesk()
    expect(draft(INVOICES).value!.draft.status).toBe('ready')

    approveDraft(INVOICES, 'request_changes')

    expect(draft(INVOICES).value!.draft.status).toBe('approved')
    expect(draft(INVOICES).value!.draft.event).toBe('request_changes')
    expect(draft(INVOICES).value!.draft.approved_at).not.toBeNull()
    expect(review(INVOICES).value!.layers[5]).toMatchObject({
      state: 'done',
      detail: 'approved by you',
    })
    expect(inbox.value.drafts_to_approve.map(item => item.review_id)).not.toContain(INVOICES)
  })

  it('refuses to send a draft back with no note', () => {
    const { sendBackDraft, draft } = useReviewDesk()
    expect(sendBackDraft(INVOICES, '  ').ok).toBe(false)
    expect(draft(INVOICES).value!.draft.status).toBe('ready')
  })

  it('sends a draft back with the note, and leaves the inbox', () => {
    const { sendBackDraft, draft, inbox } = useReviewDesk()
    const result = sendBackDraft(INVOICES, 'Check the warehouse column first.')
    expect(result.ok).toBe(true)
    expect(draft(INVOICES).value!.draft).toMatchObject({
      status: 'sent_back',
      sent_back_note: 'Check the warehouse column first.',
      event: null,
    })
    expect(inbox.value.drafts_to_approve.map(item => item.review_id)).not.toContain(INVOICES)
  })

  it('asks for one ticket covering the findings held back now', () => {
    const { requestNitTicket, review, draft } = useReviewDesk()
    expect(review(WEBHOOKS).value!.nit_ticket.status).toBe('none')

    const message = requestNitTicket(WEBHOOKS)

    expect(message).toContain('3 findings')
    expect(review(WEBHOOKS).value!.nit_ticket).toMatchObject({
      status: 'requested',
      finding_seqs: [4, 5, 7],
    })
    expect(draft(WEBHOOKS).value!.nit_ticket.status).toBe('requested')
    expect(requestNitTicket(WEBHOOKS)).toContain('already')
  })

  it('has no draft for a review the session is still writing', () => {
    const { draft } = useReviewDesk()
    expect(draft(18).value).toBeNull()
  })

  it('shows a posted draft as posted', () => {
    const { draft } = useReviewDesk()
    expect(draft(EXPORT).value!.draft.status).toBe('posted')
  })
})

describe('the rules', () => {
  beforeEach(() => resetDesk())

  it('filters by repository, and keeps global rules in every repository', () => {
    const { rules } = useReviewDesk()
    const repo = ref(PORTAL)
    const page = rules(repo)
    expect(page.value.repo).toBe(PORTAL)
    expect(page.value.active.map(rule => rule.key)).toEqual(['R-09', 'R-03', 'R-14', 'R-02'])

    repo.value = 'acme/billing-api'
    expect(page.value.active.map(rule => rule.key)).toEqual(['R-11', 'R-02'])
    /* Nothing is proposed there, which the screen shows by leaving the group out. */
    expect(page.value.proposed).toEqual([])

    repo.value = ALL_REPOSITORIES
    expect(page.value.repo).toBeNull()
    expect(page.value.active).toHaveLength(5)
  })

  it('counts the active rules on screen, and leaves the replay figure absent', () => {
    const { rules } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    expect(page.value.summary.active_rules).toBe(4)
    expect(page.value.summary.active_rules_cap).toBe(20)
    expect(page.value.summary.replay_matched).toBeNull()
    expect(page.value.summary.replay_total).toBeNull()
  })

  it('separates a rule going stale from the ones in force', () => {
    const { rules } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    expect(page.value.stale.map(rule => rule.key)).toEqual(['R-05'])
    expect(page.value.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('promoting moves a proposal into the active list and raises the count', () => {
    const { rules, promoteRule, inbox } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    expect(page.value.summary.active_rules).toBe(4)

    promoteRule(22)

    expect(page.value.proposed).toHaveLength(0)
    expect(page.value.active.map(rule => rule.key)).toContain('R-22')
    expect(page.value.summary.active_rules).toBe(5)
    expect(inbox.value.proposed_rules.map(rule => rule.rule_key)).not.toContain('R-22')
  })

  it('refuses to dismiss a rule without a reason', () => {
    const { rules, dismissRule } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    expect(dismissRule(22, '').ok).toBe(false)
    expect(page.value.proposed).toHaveLength(1)
  })

  it('dismissing with a reason removes the proposal', () => {
    const { rules, dismissRule, inbox } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    const result = dismissRule(22, 'Renames are a formatter’s job.')
    expect(result.ok).toBe(true)
    expect(result.message).toContain('R-22')
    expect(page.value.proposed).toHaveLength(0)
    expect(inbox.value.proposed_rules).toHaveLength(0)
  })

  it('refuses to retire a rule without a reason, then retires it', () => {
    const { rules, retireRule } = useReviewDesk()
    const page = rules(computed(() => PORTAL))
    expect(retireRule(5, ' ').ok).toBe(false)
    expect(page.value.stale).toHaveLength(1)

    expect(retireRule(5, 'The importer is gone in #415.').ok).toBe(true)
    expect(page.value.stale).toHaveLength(0)
    expect(page.value.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('keeping a stale rule puts it back in force with a date to ask again', () => {
    const { rules, deferRule } = useReviewDesk()
    const page = rules(computed(() => PORTAL))

    expect(deferRule(5, 60)).toContain('60 days')

    expect(page.value.stale).toHaveLength(0)
    const kept = page.value.active.find(rule => rule.key === 'R-05')
    expect(kept).toBeDefined()
    expect(kept!.review_again_at).toBeTruthy()
  })
})

describe('starting a review', () => {
  beforeEach(() => resetDesk())

  it('sends only the link, which is all the screen has', () => {
    const { startReview } = useReviewDesk()
    const result = startReview('https://github.com/acme/portal/pull/501')
    expect(result.ok).toBe(true)
    if (!result.ok) return
    /* `head_sha` is the server's to resolve; sending a guess would be worse than none. */
    expect(result.request).toEqual({ pr_url: 'https://github.com/acme/portal/pull/501' })
    expect(result.message).toContain('acme/portal#501')
  })

  it('normalises the short form a person types into a link', () => {
    const { startReview } = useReviewDesk()
    const result = startReview('acme/portal#501')
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.request.pr_url).toBe('https://github.com/acme/portal/pull/501')
  })

  it('refuses anything that is not a pull request link', () => {
    const { startReview } = useReviewDesk()
    for (const input of ['', 'not a link', 'https://github.com/acme/portal']) {
      const result = startReview(input)
      expect(result.ok).toBe(false)
      if (result.ok) continue
      expect(result.error.code).toBe('bad_request')
      expect(result.error.field).toBe('pr_url')
    }
  })

  it('refuses a pull request already on the desk, as a conflict', () => {
    const { startReview } = useReviewDesk()
    const result = startReview('https://github.com/acme/portal/pull/412')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('already on the desk')
  })

  /*
   * The head commit is read from GitHub when the caller has none, so a machine with no
   * `gh` fails the request outright and creates nothing. That is the developer's
   * environment rather than their request, and the screen has to say so differently.
   */
  it('reports a GitHub it could not read, and creates nothing', () => {
    resetDesk({ github_available: false })
    const { startReview, inbox } = useReviewDesk()
    const before = inbox.value.totals.items

    const result = startReview('https://github.com/acme/portal/pull/501')

    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('github_unavailable')
    expect(result.error.field).toBe('head_sha')
    expect(result.error.message).toContain('gh auth login')
    expect(inbox.value.totals.items).toBe(before)
  })

  it('refuses a bad link before it ever needs GitHub', () => {
    resetDesk({ github_available: false })
    const { startReview } = useReviewDesk()
    const result = startReview('not a link')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('bad_request')
  })

  it('reports a conflict before it needs GitHub, since the review is already here', () => {
    resetDesk({ github_available: false })
    const { startReview } = useReviewDesk()
    const result = startReview('acme/portal#412')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
  })
})

describe('the sample-data marker', () => {
  it('is on while the records are invented', () => {
    expect(useReviewDesk().isSampleData).toBe(true)
  })
})

describe('the fixtures stay inside the contract’s closed sets', () => {
  beforeEach(() => resetDesk())

  const SEVERITIES = ['blocker', 'should', 'nit', 'question']
  const DISPOSITIONS = ['in_draft', 'held_back', 'suppressed']
  const LAYER_NAMES = [
    'lint',
    'understand',
    'alternatives',
    'solution_map',
    'code_review',
    'draft',
    'harvest',
  ]
  const LAYER_STATES = ['pending', 'running', 'done', 'waiting_on_you', 'failed', 'skipped']
  const STAGES = ['proposed', 'active', 'lint', 'ci', 'retired']
  const STATUSES = ['drafting', 'ready', 'approved', 'sent_back', 'posted']

  it('every finding, layer and rule uses a value the API defines', () => {
    const { reviews } = { reviews: [17, 15, 16, 12, 18, 19] }
    const desk = useReviewDesk()
    for (const id of reviews) {
      const detail = desk.review(id).value!
      expect(detail.layers).toHaveLength(7)
      for (const layer of detail.layers) {
        expect(LAYER_NAMES).toContain(layer.name)
        expect(LAYER_STATES).toContain(layer.state)
      }
      for (const finding of detail.findings) {
        expect(SEVERITIES).toContain(finding.severity)
        expect(DISPOSITIONS).toContain(finding.disposition)
        expect(finding.key).toBe(`F${finding.seq}`)
      }
      if (detail.draft_status) expect(STATUSES).toContain(detail.draft_status)
    }
    const page = desk.rules(computed(() => ALL_REPOSITORIES)).value
    for (const rule of [...page.proposed, ...page.active, ...page.stale]) {
      expect(STAGES).toContain(rule.stage)
      expect(rule.key).toMatch(/^R-\d{2,}$/)
      expect(rule.prevents.length).toBeGreaterThan(0)
    }
  })

  it('every timestamp is an instant the API would send', () => {
    const desk = useReviewDesk()
    const stamps = [
      ...desk.review(17).value!.findings.map(finding => finding.created_at),
      ...desk.review(17).value!.layers.map(layer => layer.updated_at).filter(Boolean),
      desk.draft(17).value!.draft.updated_at,
      desk.draft(17).value!.lint.checked_at,
    ] as string[]
    for (const stamp of stamps) {
      expect(stamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/)
      expect(Number.isNaN(Date.parse(stamp))).toBe(false)
    }
  })

  it('covers all four severities and all three dispositions', () => {
    const desk = useReviewDesk()
    const all = [17, 15, 16, 12].flatMap(id => desk.review(id).value!.findings)
    expect(new Set(all.map(finding => finding.severity))).toEqual(
      new Set(['blocker', 'should', 'nit', 'question']),
    )
    expect(new Set(all.map(finding => finding.disposition))).toEqual(
      new Set(['in_draft', 'held_back', 'suppressed']),
    )
  })
})
