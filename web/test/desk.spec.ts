import { beforeEach, describe, expect, it } from 'vitest'
import { computed, effectScope, ref } from 'vue'

import {
  ALL_REPOSITORIES,
  resetDesk,
  useReviewDesk,
  type DeskResource,
} from '../app/composables/useReviewDesk'

/*
 * The data layer, which is where every write on the four screens ends up.
 *
 * It runs here over sample records — DEV-862 keeps them for exactly this, so that the
 * behaviour of a screen can be checked without a binary and a seeded database. Sample mode
 * raises the same refusals, with the same contract `code`s, that the server raises, so what
 * is asserted here is also the behaviour expected of the Rust server.
 *
 * Two things are *not* checked here, and both have their own file. `test/api.spec.ts`
 * covers what the dashboard makes of each error code and of a server that is not answering,
 * because sample mode is always answering. `web/e2e/` covers the whole path through a real
 * binary in a real browser.
 *
 * Reads are asynchronous now, so every one is awaited through `settled`. That is the one
 * shape change from the fixture era: a read has an in-flight state, and a test that read
 * `.data.value` without waiting would be asserting on `null`.
 */

const PORTAL = 'acme/portal'
const WEBHOOKS = 17 /* #412, the brief that is waiting */
const INVOICES = 15 /* #407, a draft that is waiting */
const CACHE = 16 /* #88, round 2 */
const EXPORT = 12 /* #398, posted, with author replies */
const DRAFTING = 18 /* a review a session is still writing */

/** Wait for a resource's first read, and hand back what it answered. */
async function settled<T>(resource: DeskResource<T>): Promise<T> {
  await resource.refresh()
  if (resource.error.value) {
    throw new Error(`the read failed: ${resource.error.value.message}`)
  }
  return resource.data.value as T
}

/**
 * A resource's refusal rather than its answer, for a read that is meant to fail.
 *
 * `settled` throws on a failed read, which is right everywhere else: a test that meant to
 * read something and got an error should say so rather than assert on `null`.
 */
async function refused<T>(resource: DeskResource<T>) {
  await resource.refresh()
  if (!resource.error.value) throw new Error('that read was expected to fail and did not')
  return resource.error.value
}

/*
 * Resources register themselves for refresh-after-write while their scope is alive, and a
 * test has no component scope. One scope per file keeps them from accumulating across
 * hundreds of tests, which would turn every write into hundreds of reads.
 */
const scope = effectScope()
function desk() {
  return scope.run(() => useReviewDesk())!
}

beforeEach(async () => {
  await resetDesk()
})

describe('the inbox', () => {
  it('groups seven waiting items across two repositories', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.totals).toEqual({ items: 7, repos: 2 })
  })

  it('has all five groups filled', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.briefs_to_confirm).toHaveLength(1)
    expect(inbox.drafts_to_approve).toHaveLength(2)
    expect(inbox.author_replies).toHaveLength(1)
    expect(inbox.own_prs_without_reviewer).toHaveLength(2)
    expect(inbox.proposed_rules).toHaveLength(1)
  })

  it('shows two sessions running, one of them with unknown liveness', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.in_progress).toHaveLength(2)
    expect(inbox.in_progress.map(item => item.session_alive)).toEqual([true, null])
    expect(inbox.in_progress[0]).toMatchObject({ layer_number: 5, layers_total: 7 })
  })

  it('says a draft already written is not holding its session at layer 2', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.briefs_to_confirm[0]).toMatchObject({
      review_id: WEBHOOKS,
      blocks_later_layers: false,
    })
  })

  it('carries the author’s two kinds of reply', async () => {
    const inbox = await settled(desk().inbox())
    const kinds = inbox.author_replies[0]!.events.map(event => event.kind)
    expect(kinds).toEqual(['rejected', 'code_changed_after'])
  })
})

describe('confirming a brief', () => {
  it('takes the item out of the inbox and puts the draft in', async () => {
    const api = desk()
    const inbox = api.inbox()
    expect((await settled(inbox)).briefs_to_confirm.map(item => item.review_id))
      .toContain(WEBHOOKS)
    expect((await settled(inbox)).drafts_to_approve.map(item => item.review_id))
      .not.toContain(WEBHOOKS)

    const result = await api.confirmBrief(WEBHOOKS)
    expect(result.ok).toBe(true)

    const after = await settled(inbox)
    expect(after.briefs_to_confirm).toHaveLength(0)
    expect(after.drafts_to_approve.map(item => item.review_id)).toContain(WEBHOOKS)
    expect(after.totals.items).toBe(7)
  })

  it('moves layer 2 to done and layer 6 to waiting', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    const before = await settled(review)
    expect(before.layers[1]).toMatchObject({ state: 'waiting_on_you' })
    expect(before.layers[5]).toMatchObject({ state: 'pending' })

    await api.confirmBrief(WEBHOOKS)

    const after = await settled(review)
    expect(after.brief!.confirmed).toBe(true)
    expect(after.brief!.confirmed_at).not.toBeNull()
    expect(after.layers[1]).toMatchObject({ state: 'done', detail: 'confirmed by you' })
    expect(after.layers[5]).toMatchObject({ state: 'waiting_on_you' })
  })

  it('is idempotent, as the contract requires', async () => {
    const api = desk()
    await api.confirmBrief(WEBHOOKS)
    const at = (await settled(api.review(WEBHOOKS))).brief!.confirmed_at

    /* Confirming an already confirmed brief answers with the brief rather than failing. */
    const again = await api.confirmBrief(WEBHOOKS)
    expect(again.ok).toBe(true)
    expect((await settled(api.review(WEBHOOKS))).brief!.confirmed_at).toBe(at)
  })

  it('says so when there is no such review', async () => {
    const result = await desk().confirmBrief(999)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('moving a finding', () => {
  it('refuses to hold one back without a reason, and says what it wanted', async () => {
    const api = desk()
    const result = await api.setFindingDisposition(WEBHOOKS, 1, 'held_back', '   ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect(result.error.field).toBe('reason')
    expect(result.error.expected).toBe('a sentence saying why')
    expect((await settled(api.review(WEBHOOKS))).findings[0]!.disposition).toBe('in_draft')
  })

  it('records the reason and recounts the dispositions', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    expect((await settled(review)).finding_dispositions).toEqual({
      in_draft: 3,
      held_back: 3,
      suppressed: 1,
    })

    const result = await api.setFindingDisposition(WEBHOOKS, 2, 'suppressed', 'Covered by F1')
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toBe('F2 is now suppressed.')

    const after = await settled(review)
    expect(after.findings[1]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Covered by F1',
    })
    expect(after.finding_dispositions).toEqual({ in_draft: 2, held_back: 3, suppressed: 2 })
  })

  it('clears the reason when a finding goes back in the draft', async () => {
    const api = desk()
    await api.setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')
    expect((await settled(api.review(WEBHOOKS))).findings[3]).toMatchObject({
      disposition: 'in_draft',
      disposition_reason: null,
    })
  })

  it('updates the draft’s held-back list and its lint', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = await settled(draft)
    expect(before.held_back.map(item => item.finding_key)).toEqual(['F4', 'F5', 'F7'])
    expect(before.lint!.findings_expected).toBe(3)

    await api.setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')

    const after = await settled(draft)
    expect(after.held_back.map(item => item.finding_key)).toEqual(['F5', 'F7'])
    /* F4 is now expected in the body and is not there, so the lint fails loudly. */
    expect(after.lint!.findings_expected).toBe(4)
    expect(after.lint!.findings_in_body).toBe(3)
    expect(after.lint!.passes).toBe(false)
  })

  it('says so when there is no such finding', async () => {
    const result = await desk().setFindingDisposition(WEBHOOKS, 99, 'in_draft', '')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('the draft', () => {
  it('serves a body with a mermaid fence and a folded section', async () => {
    const body = (await settled(desk().draft(WEBHOOKS))).draft.body
    expect(body).toContain('```mermaid')
    expect(body).toContain('<details>')
    expect(body).toContain('</details>')
  })

  it('counts the words actually read, against a budget', async () => {
    const lint = (await settled(desk().draft(WEBHOOKS))).lint!
    expect(lint.word_budget).toBe(150)
    expect(lint.visible_words!).toBeLessThanOrEqual(150)
    expect(lint.passes).toBe(true)
  })

  it('flags a finding raised before on the same pull request', async () => {
    expect((await settled(desk().draft(CACHE))).lint!.raised_before_on_pr).toBe(1)
  })

  it('records an edit against the finding it changed, and recounts', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = (await settled(draft)).draft.body
    expect((await settled(draft)).edits).toHaveLength(3)

    const result = await api.saveDraft(
      WEBHOOKS,
      before,
      before.replace('I reverted the retry and', 'Reverting the retry and running it,'),
    )

    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('1 edit recorded')

    const after = await settled(draft)
    expect(after.edits.some(edit => edit.finding_key === 'F3' && edit.kind === 'rewritten'))
      .toBe(true)
    expect(after.edits).toHaveLength(4)
    expect(after.lint!.checked_at).not.toBe('')
  })

  it('suppresses a finding whose section the developer cut', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = (await settled(draft)).draft.body
    const after = before.slice(0, before.indexOf('### Should · F3'))

    await api.saveDraft(WEBHOOKS, before, after)

    expect((await settled(api.review(WEBHOOKS))).findings[2]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Cut from the draft before posting',
    })
    expect(
      (await settled(draft)).edits.some(
        edit => edit.kind === 'cut' && edit.finding_key === 'F3',
      ),
    ).toBe(true)
  })

  it('records nothing when the body is unchanged', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const body = (await settled(draft)).draft.body

    const result = await api.saveDraft(WEBHOOKS, body, body)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('Nothing changed')
    expect((await settled(draft)).edits).toHaveLength(3)
  })

  it('approving changes the status, the event, and layer 6', async () => {
    const api = desk()
    const draft = api.draft(INVOICES)
    const inbox = api.inbox()
    expect((await settled(draft)).draft.status).toBe('ready')

    const result = await api.approveDraft(INVOICES, 'request_changes')
    expect(result.ok).toBe(true)

    const after = await settled(draft)
    expect(after.draft.status).toBe('approved')
    expect(after.draft.event).toBe('request_changes')
    expect(after.draft.approved_at).not.toBeNull()
    expect((await settled(api.review(INVOICES))).layers[5]).toMatchObject({
      state: 'done',
      detail: 'approved by you',
    })
    expect((await settled(inbox)).drafts_to_approve.map(item => item.review_id))
      .not.toContain(INVOICES)
  })

  it('refuses to approve a draft that is not ready, as a conflict', async () => {
    const api = desk()
    await api.approveDraft(INVOICES, 'comment')

    /* Approving twice: the record is not in a state this act can be performed from. */
    const again = await api.approveDraft(INVOICES, 'comment')
    expect(again.ok).toBe(false)
    if (again.ok) return
    expect(again.error.code).toBe('conflict')
  })

  it('refuses to edit an approved draft, because that is what gets posted', async () => {
    const api = desk()
    const draft = api.draft(INVOICES)
    const body = (await settled(draft)).draft.body
    await api.approveDraft(INVOICES, 'comment')

    const result = await api.saveDraft(INVOICES, body, `${body}\n\nAn afterthought.`)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect((await settled(draft)).draft.body).toBe(body)
  })

  it('refuses to send a draft back with no note', async () => {
    const api = desk()
    const result = await api.sendBackDraft(INVOICES, '  ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect((await settled(api.draft(INVOICES))).draft.status).toBe('ready')
  })

  it('sends a draft back with the note, and leaves the inbox', async () => {
    const api = desk()
    const inbox = api.inbox()
    const result = await api.sendBackDraft(INVOICES, 'Check the warehouse column first.')
    expect(result.ok).toBe(true)

    expect((await settled(api.draft(INVOICES))).draft).toMatchObject({
      status: 'sent_back',
      sent_back_note: 'Check the warehouse column first.',
      event: null,
    })
    expect((await settled(inbox)).drafts_to_approve.map(item => item.review_id))
      .not.toContain(INVOICES)
  })

  it('asks for one ticket covering the findings held back now', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    expect((await settled(review)).nit_ticket.status).toBe('none')

    const result = await api.requestNitTicket(WEBHOOKS)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('3 findings')

    expect((await settled(review)).nit_ticket).toMatchObject({
      status: 'requested',
      finding_seqs: [4, 5, 7],
    })
    expect((await settled(api.draft(WEBHOOKS))).nit_ticket.status).toBe('requested')
  })

  it('asking twice keeps the first time asked, as the store does', async () => {
    const api = desk()
    await api.requestNitTicket(WEBHOOKS)
    const first = (await settled(api.review(WEBHOOKS))).nit_ticket.requested_at

    const again = await api.requestNitTicket(WEBHOOKS)
    expect(again.ok).toBe(true)
    expect((await settled(api.review(WEBHOOKS))).nit_ticket.requested_at).toBe(first)
  })

  it('refuses a ticket for a review that holds nothing back', async () => {
    const result = await desk().requestNitTicket(EXPORT)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('holds back no nits')
  })

  it('has no draft for a review the session has not reached layer 6 on', async () => {
    const error = await refused(desk().draft(DRAFTING))
    expect(error.code).toBe('not_found')
  })

  it('shows a posted draft as posted', async () => {
    expect((await settled(desk().draft(EXPORT))).draft.status).toBe('posted')
  })
})

describe('the rules', () => {
  it('filters by repository, and keeps global rules in every repository', async () => {
    const repo = ref(PORTAL)
    const page = desk().rules(repo)
    expect((await settled(page)).repo).toBe(PORTAL)
    expect((await settled(page)).active.map(rule => rule.key))
      .toEqual(['R-09', 'R-03', 'R-14', 'R-02'])

    repo.value = 'acme/billing-api'
    expect((await settled(page)).active.map(rule => rule.key)).toEqual(['R-11', 'R-02'])
    /* Nothing is proposed there, which the screen shows by leaving the group out. */
    expect((await settled(page)).proposed).toEqual([])

    repo.value = ALL_REPOSITORIES
    expect((await settled(page)).repo).toBeNull()
    expect((await settled(page)).active).toHaveLength(5)
  })

  it('counts the active rules on screen, and leaves the replay figure absent', async () => {
    const page = await settled(desk().rules(computed(() => PORTAL)))
    expect(page.summary.active_rules).toBe(4)
    expect(page.summary.active_rules_cap).toBe(20)
    expect(page.summary.replay_matched).toBeNull()
    expect(page.summary.replay_total).toBeNull()
  })

  it('separates a rule going stale from the ones in force', async () => {
    const page = await settled(desk().rules(computed(() => PORTAL)))
    expect(page.stale.map(rule => rule.key)).toEqual(['R-05'])
    expect(page.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('promoting moves a proposal into the active list and raises the count', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const inbox = api.inbox()
    expect((await settled(page)).summary.active_rules).toBe(4)

    const result = await api.promoteRule(22)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('R-22')

    const after = await settled(page)
    expect(after.proposed).toHaveLength(0)
    expect(after.active.map(rule => rule.key)).toContain('R-22')
    expect(after.summary.active_rules).toBe(5)
    expect((await settled(inbox)).proposed_rules.map(rule => rule.rule_key))
      .not.toContain('R-22')
  })

  it('refuses a promotion that would skip a stage', async () => {
    const api = desk()
    /*
     * R-22 is a proposal, and the stage after `proposed` is `active`. Naming `ci` is what
     * an accidental double click would amount to, and the contract has the caller state
     * the stage precisely so that it cannot happen silently.
     */
    const result = await api.promoteRule(22, 'ci')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('the stage that follows is active')
  })

  it('refuses to dismiss a rule without a reason', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const result = await api.dismissRule(22, '')
    expect(result.ok).toBe(false)
    if (!result.ok) expect(result.error.code).toBe('invalid_value')
    expect((await settled(page)).proposed).toHaveLength(1)
  })

  it('dismissing with a reason removes the proposal', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const inbox = api.inbox()

    const result = await api.dismissRule(22, 'Renames are a formatter’s job.')
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('R-22')

    expect((await settled(page)).proposed).toHaveLength(0)
    expect((await settled(inbox)).proposed_rules).toHaveLength(0)
  })

  it('refuses to retire a rule without a reason, then retires it', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    expect((await api.retireRule(5, ' ')).ok).toBe(false)
    expect((await settled(page)).stale).toHaveLength(1)

    expect((await api.retireRule(5, 'The importer is gone in #415.')).ok).toBe(true)
    const after = await settled(page)
    expect(after.stale).toHaveLength(0)
    expect(after.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('keeping a stale rule puts it back in force with a date to ask again', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))

    const result = await api.deferRule(5, 60)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('60 days')

    const after = await settled(page)
    expect(after.stale).toHaveLength(0)
    const kept = after.active.find(rule => rule.key === 'R-05')
    expect(kept).toBeDefined()
    expect(kept!.review_again_at).toBeTruthy()
  })

  it('refuses a deferral outside the contract’s range', async () => {
    const result = await desk().deferRule(5, 400)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect(result.error.expected).toContain('1 to 365')
  })

  it('says so when there is no such rule', async () => {
    const result = await desk().promoteRule(999)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('starting a review', () => {
  it('refuses anything that is not a pull request link, and says what it wanted', async () => {
    const api = desk()
    for (const input of ['not a link', 'https://github.com/acme/portal']) {
      const result = await api.startReview(input)
      expect(result.ok).toBe(false)
      if (result.ok) continue
      expect(result.error.code).toBe('invalid_value')
      expect(result.error.field).toBe('pr_url')
      expect(result.error.expected).toContain('pull request link')
    }
  })

  it('refuses an empty field before it sends anything', async () => {
    const result = await desk().startReview('   ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('bad_request')
    expect(result.error.field).toBe('pr_url')
  })

  it('refuses a pull request already on the desk, as a conflict', async () => {
    const result = await desk().startReview('https://github.com/acme/portal/pull/412')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('already on the desk')
  })

  /*
   * The head commit, the title and the changed-line count are read from GitHub when the
   * caller has none, so a machine with no `gh` fails the request outright and creates
   * nothing. That is the developer's environment rather than their request, and the screen
   * has to say so differently — which is what `refusalTone` decides on.
   */
  it('reports a GitHub it could not read, and creates nothing', async () => {
    await resetDesk({ github_available: false })
    const api = desk()
    const inbox = api.inbox()
    const before = (await settled(inbox)).totals.items

    const result = await api.startReview('https://github.com/acme/portal/pull/501')

    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('github_unavailable')
    expect(result.error.message).toContain('gh auth login')
    expect((await settled(inbox)).totals.items).toBe(before)
  })

  it('refuses a bad link before it ever needs GitHub', async () => {
    await resetDesk({ github_available: false })
    const result = await desk().startReview('not a link')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
  })

  it('says plainly that invented records cannot hold a real review', async () => {
    /*
     * The one place sample mode cannot imitate the server, and it says so rather than
     * reporting a success that recorded nothing. A review invented here would have no
     * brief, no findings and no draft, so it would sit on the Inbox with every screen
     * behind it empty — which reads as a broken app rather than as a sample.
     */
    const result = await desk().startReview('https://github.com/acme/portal/pull/501')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('review-desk serve')
  })
})

describe('a database with nothing in it', () => {
  /*
   * A fresh database, which is what the first run of `review-desk serve` serves. Every
   * screen has to read as "nothing here yet" rather than as broken, and the four reads have
   * to answer rather than fail — an empty desk is an answer.
   */
  beforeEach(async () => {
    await resetDesk({
      reviews: [],
      drafts: [],
      rules: [],
      repos: [],
      own_prs: [],
      handled_without_you: {
        window_days: 7,
        description_problems_caught: null,
        nits_filed: null,
        nit_tickets: null,
      },
      session_liveness: {},
    })
  })

  it('answers the inbox with every group empty', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.totals).toEqual({ items: 0, repos: 0 })
    expect(inbox.briefs_to_confirm).toEqual([])
    expect(inbox.drafts_to_approve).toEqual([])
    expect(inbox.author_replies).toEqual([])
    expect(inbox.proposed_rules).toEqual([])
    expect(inbox.in_progress).toEqual([])
  })

  it('answers the rules page with no rule at any stage', async () => {
    const page = await settled(desk().rules(computed(() => ALL_REPOSITORIES)))
    expect(page.proposed).toEqual([])
    expect(page.active).toEqual([])
    expect(page.stale).toEqual([])
    expect(page.summary.active_rules).toBe(0)
  })

  it('answers the repository list with nothing in it', async () => {
    expect(await settled(desk().repos())).toEqual([])
  })

  it('refuses a review that is not there, rather than serving an empty one', async () => {
    /*
     * A `not_found` and an empty review are different answers, and the screen says
     * different things about them: one is "the desk holds no such review", the other
     * would be a review with every section blank.
     */
    expect((await refused(desk().review(WEBHOOKS))).code).toBe('not_found')
    expect((await refused(desk().draft(WEBHOOKS))).code).toBe('not_found')
  })
})

describe('the sample-data marker', () => {
  it('is on while the records are invented', () => {
    expect(desk().isSampleData.value).toBe(true)
  })
})

describe('the fixtures stay inside the contract’s closed sets', () => {
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

  it('every finding, layer and rule uses a value the API defines', async () => {
    const api = desk()
    for (const id of [17, 15, 16, 12, 18, 19]) {
      const detail = await settled(api.review(id))
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

    const page = await settled(api.rules(computed(() => ALL_REPOSITORIES)))
    for (const rule of [...page.proposed, ...page.active, ...page.stale]) {
      expect(STAGES).toContain(rule.stage)
      expect(rule.key).toMatch(/^R-\d{2,}$/)
      expect(rule.prevents.length).toBeGreaterThan(0)
    }
  })

  it('every timestamp is an instant the API would send', async () => {
    const api = desk()
    const review = await settled(api.review(17))
    const draft = await settled(api.draft(17))
    const stamps = [
      ...review.findings.map(finding => finding.created_at),
      ...review.layers.map(layer => layer.updated_at).filter(Boolean),
      draft.draft.updated_at,
      draft.lint!.checked_at,
    ] as string[]
    for (const stamp of stamps) {
      expect(stamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/)
      expect(Number.isNaN(Date.parse(stamp))).toBe(false)
    }
  })

  it('covers all four severities and all three dispositions', async () => {
    const api = desk()
    const all = []
    for (const id of [17, 15, 16, 12]) all.push(...(await settled(api.review(id))).findings)

    expect(new Set(all.map(finding => finding.severity))).toEqual(
      new Set(['blocker', 'should', 'nit', 'question']),
    )
    expect(new Set(all.map(finding => finding.disposition))).toEqual(
      new Set(['in_draft', 'held_back', 'suppressed']),
    )
  })
})
