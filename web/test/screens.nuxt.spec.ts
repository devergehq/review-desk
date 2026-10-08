// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk, useDeskSource, useReviewDesk } from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'

import Inbox from '../app/pages/index.vue'
import OneReview from '../app/pages/reviews/[id]/index.vue'
import DraftPreview from '../app/pages/reviews/[id]/draft.vue'
import Rules from '../app/pages/rules.vue'
import Layout from '../app/layouts/default.vue'

/*
 * The four screens, mounted. These check that each one renders from the data layer and
 * that the controls are real, labelled elements — not that it looks right, which is what
 * the screenshots in the pull request are for.
 *
 * **Every read is a request now**, so a mount is not finished when `mountSuspended`
 * resolves: the resource's first read is in flight. `shown` is a mount plus a settle, and
 * `settle` is what every interaction is followed by. A test that forgot it would assert on
 * the loading state and say so clearly, which is why that state has a test of its own.
 */

/** Let every pending read and re-read finish. */
async function settle(page: VueWrapper) {
  for (let round = 0; round < 8; round += 1) {
    await flushPromises()
    await page.vm.$nextTick()
  }
}

/** Mount a screen and wait for its first read. */
async function shown(
  component: Parameters<typeof mountSuspended>[0],
  options?: Parameters<typeof mountSuspended>[1],
) {
  const page = await mountSuspended(component, options)
  await settle(page)
  return page
}

/** The button with this exact label. */
function button(page: VueWrapper, label: string) {
  return page.findAll('button').find(item => item.text() === label)
}

/** Click the button with this label, and let what it started finish. */
async function click(page: VueWrapper, label: string) {
  const found = button(page, label)
  if (!found) throw new Error(`there is no button labelled ${label!}`)
  await found.trigger('click')
  await settle(page)
}

describe('the shell', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('marks the records as invented on every screen', async () => {
    const page = await shown(Layout)
    expect(page.text()).toContain('Sample data')
  })

  it('offers a way past the navigation for a keyboard', async () => {
    const page = await shown(Layout)
    expect(page.find('a[href="#main"]').exists()).toBe(true)
    expect(page.find('main#main').exists()).toBe(true)
  })

  it('names the three places you can be', async () => {
    const page = await shown(Layout)
    const places = page.findAll('nav a, nav span').map(link => link.text())
    expect(places).toEqual(['Inbox', 'Reviews', 'Rules'])
  })

  it('does not link to a review when the desk holds none', async () => {
    await resetDesk({ reviews: [], drafts: [], rules: [] })
    const page = await shown(Layout)
    /*
     * The item stays, so the navigation keeps its three places; it is not a link, because
     * on a fresh database there is no review to link to. It used to point at `/reviews/1`,
     * which was a link to a review that does not exist.
     */
    const reviews = page.findAll('nav a').map(link => link.text())
    expect(reviews).not.toContain('Reviews')
    expect(page.find('nav span[aria-disabled="true"]').text()).toBe('Reviews')
  })
})

describe('the inbox', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('renders the five waiting groups', async () => {
    const page = await shown(Inbox)
    const text = page.text()
    for (const heading of [
      'Confirm the problem · 1',
      'Approve a draft · 2',
      'Author replied · 1',
      'Your PRs with no reviewer · 2',
      'Proposed rule · 1',
    ]) {
      expect(text).toContain(heading)
    }
    expect(text).toContain('7 items across 2 repositories')
  })

  it('renders the sessions in progress with their layer and a meter', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('In progress · 2')
    expect(page.text()).toContain('Layer 5 of 7 · code review')
    const meters = page.findAll('[role="progressbar"]')
    expect(meters).toHaveLength(2)
    expect(meters[0]!.attributes('aria-valuenow')).toBe('5')
    expect(meters[0]!.attributes('aria-valuemax')).toBe('7')
  })

  it('says when nothing has reported a session as alive', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('session alive')
    expect(page.text()).toContain('session state unknown')
  })

  it('says when GitHub has told it nothing about a pull request', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('description lint passes')
    expect(page.text()).toContain('description not checked')
    expect(page.text()).toContain('open for an unrecorded time')
  })

  it('has a labelled field for starting a review', async () => {
    const page = await shown(Inbox)
    const field = page.find('input#pr-link')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="pr-link"]').text()).toBe('Start a review')
    expect(field.attributes('aria-describedby')).toBe('pr-link-help')
  })

  it('refuses a link that is not a pull request, and says what it wanted', async () => {
    const page = await shown(Inbox)
    await page.find('input#pr-link').setValue('nonsense')
    await click(page, 'Understand it first')

    expect(page.text()).toContain('Paste a link like github.com/acme/portal/pull/412.')
    expect(page.text()).toContain('Accepted: a pull request link')
    /* A refused link is the developer's typing, not their machine. */
    expect(page.find('.notice--blocked').exists()).toBe(false)
  })

  it('says differently when GitHub could not be read, and names the fix', async () => {
    await resetDesk({ github_available: false })
    const page = await shown(Inbox)
    await page.find('input#pr-link').setValue('https://github.com/acme/portal/pull/501')
    await click(page, 'Understand it first')

    const blocked = page.find('.notice--blocked')
    expect(blocked.exists()).toBe(true)
    expect(blocked.text()).toContain('gh auth login')
    expect(blocked.attributes('role')).toBe('status')
    /* The link stays in the field: nothing is wrong with it, so retyping it is waste. */
    expect((page.find('input#pr-link').element as HTMLInputElement).value).toBe(
      'https://github.com/acme/portal/pull/501',
    )
  })

  it('links a reviewer request out to GitHub rather than writing to it', async () => {
    const page = await shown(Inbox)
    const out = page
      .findAll('a')
      .find(link => link.attributes('href')?.startsWith('https://github.com/'))
    expect(out).toBeDefined()
    expect(out!.attributes('href')).toBe('https://github.com/acme/portal/pull/415')
    expect(out!.attributes('rel')).toContain('noopener')
  })

  it('takes the brief out of the inbox once it is confirmed', async () => {
    const page = await shown(Inbox)
    expect(page.text()).toContain('Confirm the problem · 1')

    /*
     * Confirmed from the data layer rather than from this screen, which has no control for
     * it — and then the screen re-reads itself, because a write refreshes every open read.
     * That is the one behaviour this test is about: the Inbox is not patched in place.
     */
    await useReviewDesk().confirmBrief(17)
    await settle(page)

    expect(page.text()).not.toContain('Confirm the problem · 1')
    expect(page.text()).toContain('Approve a draft · 3')
  })

  it('reads as new, not as broken, on a database with nothing in it', async () => {
    await resetDesk({
      reviews: [],
      drafts: [],
      rules: [],
      repos: [],
      own_prs: [],
      session_liveness: {},
    })
    const page = await shown(Inbox)
    const text = page.text()

    expect(text).toContain('Nothing here yet')
    expect(text).toContain('This database holds no reviews')
    /* And it says the one thing that fills it, with the field to do it from. */
    expect(text).toContain('review-desk')
    expect(page.find('input#pr-link-empty').exists()).toBe(true)
    /* Not an error, and not a count of nought items. */
    expect(page.find('.notice--blocked').exists()).toBe(false)
    expect(text).not.toContain('0 items across')
  })
})

describe('one review', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  async function open(id: number) {
    return shown(OneReview, { route: `/reviews/${id}` })
  }

  it('renders the seven layers', async () => {
    const page = await open(17)
    const layers = page.findAll('ol li').map(item => item.text())
    expect(layers).toHaveLength(7)
    expect(layers[0]).toContain('Lint')
    expect(layers[3]).toContain('Solution map')
    expect(layers[6]).toContain('Harvest')
  })

  it('renders the problem and hands its diagram to mermaid', async () => {
    const page = await open(17)
    expect(page.text()).toContain('a webhook delivery fails once and is never tried again')
    expect(page.find('figure').exists()).toBe(true)
  })

  it('marks the option that was built', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('A · Retry inside the request')
    expect(text).toContain('B · Queue with backoff')
    expect(text).toContain('C · Dead-letter and manual replay')
    expect(text).toContain('What they built')
    expect(text).toContain('Verdict:')
  })

  it('lists every finding with its severity and disposition', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('3 in the draft · 3 held back · 1 suppressed')
    expect(text).toContain('F1 · A retried delivery is sent without an idempotency key')
    expect(text).toContain('Blocker')
    expect(text).toContain('Question')
    expect(text).toContain('Suppressed')
    expect(text).toContain('jobs/DeliverWebhook.php:58')
    expect(text).toContain('cites R-09')
  })

  it('says what was not checked', async () => {
    const page = await open(17)
    expect(page.text()).toContain('Not checked')
    expect(page.text()).toContain('under a deploy restart was not run')
  })

  it('offers the brief confirmation while the brief is unconfirmed', async () => {
    const page = await open(17)
    expect(button(page, 'Confirm the problem')).toBeDefined()

    await click(page, 'Confirm the problem')

    expect(page.text()).toContain('Problem confirmed')
    expect(button(page, 'Confirm the problem')).toBeUndefined()
  })

  it('reads as done on a review whose brief was already confirmed', async () => {
    const page = await open(15)
    expect(page.text()).toContain('Problem confirmed')
    expect(button(page, 'Confirm the problem')).toBeUndefined()
  })

  it('moves a finding between the draft and held back, with a reason', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await settle(page)

    const select = page.find('select#disposition-1')
    expect(page.find('label[for="disposition-1"]').exists()).toBe(true)
    await select.setValue('held_back')
    await page.find('input#reason-1').setValue('Covered by the ticket')
    await click(page, 'Save')

    expect(page.text()).toContain('F1 is now held back for a ticket')
    expect(page.text()).toContain('Covered by the ticket')
  })

  it('will not move a finding out of the draft with no reason', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await settle(page)
    await page.find('select#disposition-1').setValue('suppressed')
    await click(page, 'Save')

    expect(page.text()).toContain('Say why it is not going in the draft.')
    expect(page.text()).toContain('Accepted: a sentence saying why.')
  })

  it('shows the author’s replies on a posted review', async () => {
    const page = await open(12)
    const text = page.text()
    expect(text).toContain('The author replied')
    expect(text).toContain('Rejected')
    expect(text).toContain('by the author')
    expect(text).toContain('Code changed after')
  })

  it('says the desk holds no such review, rather than drawing an empty one', async () => {
    const page = await shown(OneReview, { route: '/reviews/999' })
    expect(page.text()).toContain('The desk holds no this review')
    expect(button(page, 'Retry')).toBeDefined()
  })
})

describe('the draft preview', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  async function open(id: number) {
    return shown(DraftPreview, { route: `/reviews/${id}/draft` })
  }

  it('says that nothing has been posted', async () => {
    const page = await open(17)
    expect(page.text()).toContain('Nothing has been posted. This is what GitHub will show.')
  })

  it('renders the body, keeping the folded section folded', async () => {
    const page = await open(17)
    expect(page.text()).toContain('The queue-with-backoff approach fits the ticket')
    const fold = page.find('details')
    expect(fold.exists()).toBe(true)
    expect(fold.attributes('open')).toBeUndefined()
    expect(page.find('summary').text()).toContain('Proof')
  })

  it('hands the body’s mermaid fence to a diagram rather than a code block', async () => {
    const page = await open(17)
    expect(page.find('figure').exists()).toBe(true)
    expect(page.html()).not.toContain('class="language-mermaid"')
  })

  it('offers the three views as tabs', async () => {
    const page = await open(17)
    const tabs = page.findAll('[role="tab"]')
    expect(tabs.map(tab => tab.text())).toEqual(['Rendered', 'Markdown', 'Edit'])
    expect(tabs[0]!.attributes('aria-selected')).toBe('true')

    await tabs[1]!.trigger('click')
    expect(page.find('pre').text()).toContain('```mermaid')

    await tabs[2]!.trigger('click')
    expect(page.find('textarea#draft-body').exists()).toBe(true)
  })

  it('shows the checks, counted from the body as it stands', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Visible words')
    expect(text).toContain('of 150')
    expect(text).toContain('Findings in the body')
    expect(text).toContain('3 of 3')
    expect(text).toContain('Review lint')
    expect(text).toContain('Passes')
  })

  it('lists the developer’s edits and the findings held back', async () => {
    const page = await open(17)
    const text = page.text()
    expect(text).toContain('Your edits to this draft')
    expect(text).toContain('Cut')
    expect(text).toContain('F6 · rename suggestion')
    expect(text).toContain('Regraded')
    expect(text).toContain('Held back · 3 findings')
    expect(text).toContain('F4 · Backoff schedule is repeated in two files')
  })

  it('records an edit and updates the checks without a reload', async () => {
    const page = await open(17)
    expect(page.text()).not.toContain('F3 · wording')

    await page.findAll('[role="tab"]')[2]!.trigger('click')
    const field = page.find('textarea#draft-body')
    await field.setValue(
      (field.element as HTMLTextAreaElement).value.replace(
        'I reverted the retry and',
        'Reverting the retry, and',
      ),
    )
    await click(page, 'Save the draft')

    expect(page.text()).toContain('1 edit recorded for the harvester')
    expect(page.text()).toContain('F3 · wording')
  })

  it('approves with the chosen event, and will not approve twice', async () => {
    const page = await open(17)
    await page.find('select#event').setValue('comment')
    await click(page, 'Approve and post')

    expect(page.text()).toContain('Approved as "Comment". Waiting for the session to post it.')
    expect(page.text()).toContain('The review session posts it under your name')
    expect(button(page, 'Approve and post')!.attributes('disabled')).toBeDefined()
  })

  it('will not approve a draft with unsaved changes', async () => {
    const page = await open(17)
    await page.findAll('[role="tab"]')[2]!.trigger('click')
    await page.find('textarea#draft-body').setValue('Replaced entirely.')
    await page.vm.$nextTick()

    expect(page.text()).toContain('Save or discard your changes before approving.')
    expect(button(page, 'Approve and post')!.attributes('disabled')).toBeDefined()
  })

  it('asks for a note before sending a draft back, and keeps it when refused', async () => {
    const page = await open(17)
    await click(page, 'Send back with a note')

    const field = page.find('textarea#send-back-note')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="send-back-note"]').exists()).toBe(true)

    await click(page, 'Send back')
    expect(page.text()).toContain('A draft sent back with no note teaches it nothing.')

    await field.setValue('Fold the proof and shorten F2.')
    await click(page, 'Send back')
    expect(page.text()).toContain('Sent back. The session has the note.')
    expect(page.text()).toContain('Fold the proof and shorten F2.')
  })

  it('keeps what the developer typed when a save is refused', async () => {
    /*
     * An approved draft cannot be edited — it is what the review session posts — and a
     * refusal must not take the text away with it. DEV-862 asks for this by name: a write
     * that is refused shows why and keeps what was typed.
     */
    const page = await open(15)
    await useReviewDesk().approveDraft(15, 'comment')
    await settle(page)

    await page.findAll('[role="tab"]')[2]!.trigger('click')
    const field = page.find('textarea#draft-body')
    await field.setValue('An edit the record will refuse.')
    await click(page, 'Save the draft')

    expect(page.text()).toContain('cannot be edited')
    expect((page.find('textarea#draft-body').element as HTMLTextAreaElement).value)
      .toBe('An edit the record will refuse.')
  })

  it('files the held-back findings as one ticket', async () => {
    const page = await open(17)
    await click(page, 'File as one ticket for an agent')
    expect(page.text()).toContain('3 findings queued as one ticket')
    expect(page.text()).toContain('Queued as one ticket')
  })

  it('says the desk holds no such draft when the session has written none', async () => {
    const page = await shown(DraftPreview, { route: '/reviews/18/draft' })
    expect(page.text()).toContain('The desk holds no this draft')
    expect(button(page, 'Retry')).toBeDefined()
  })

  it('says the lint has not run, rather than drawing it as failing', async () => {
    /*
     * `lint` is null until the review-craft lint has run over the body. `DraftLint`
     * requires `passes` and `checked_at`, so there is no shape of it that means "not
     * measured" — which is why the object itself is nullable, and why a screen that
     * assumed otherwise would have shown an unmeasured draft as one that fails.
     *
     * Produced by replacing the fixture's lint rather than by finding a draft that happens
     * to have none: every fixture draft was written while the field was required.
     */
    const { drafts } = loadSource()
    drafts.find(item => item.draft.review_id === 15)!.lint = null
    await resetDesk({ drafts })

    const page = await open(15)
    expect(page.text()).toContain('The review lint has not run over this body')
    expect(page.text()).not.toContain('Review lint')
  })
})

describe('the rules', () => {
  beforeEach(async () => {
    await resetDesk()
  })

  it('renders the four figures, and leaves the unmeasured one absent', async () => {
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('Active rules')
    expect(text).toContain('cap of 20 per repository')
    expect(text).toContain('Same answer on re-run')
    expect(text).toContain('Not measured yet')
    expect(text).toContain('the replay check is a later slice')
    expect(text).toContain('31 of 40')
  })

  it('renders the proposed, active and stale groups', async () => {
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('Proposed · 1')
    expect(text).toContain('Do not raise rename-for-clarity suggestions')
    expect(text).toContain('Why proposed:')
    expect(text).toContain('Active · 4')
    expect(text).toContain('A new test must fail when the fix is reverted')
    expect(text).toContain('Prevents:')
    expect(text).toContain('Going stale · 1')
    expect(text).toContain('Legacy importer calls need the tenant guard')
  })

  it('shows a rule’s counts and leaves out the ones nothing measured', async () => {
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('Caught by the check')
    expect(text).toContain('Repository rule · 1 exception')
    expect(text).toContain('CI check')
    expect(text).toContain('Last said by a human')
  })

  it('has a labelled repository picker that changes the list', async () => {
    const page = await shown(Rules)
    expect(page.find('label[for="repo"]').text()).toBe('Repository')
    expect(page.text()).toContain('Active · 4')

    await page.find('select#repo').setValue('acme/billing-api')
    await settle(page)

    expect(page.text()).toContain('Active · 2')
    expect(page.text()).toContain('A cache bust names every key the permission reaches')
    expect(page.text()).not.toContain('Proposed · 1')
  })

  it('promotes a proposal into the active list', async () => {
    const page = await shown(Rules)
    await click(page, 'Promote')
    expect(page.text()).toContain('R-22 promoted')
    expect(page.text()).toContain('Active · 5')
    expect(page.text()).not.toContain('Proposed · 1')
  })

  it('asks why before dismissing a proposal', async () => {
    const page = await shown(Rules)
    await click(page, 'Dismiss')

    const field = page.find('input#dismiss-22')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="dismiss-22"]').text()).toBe('Why is this not a rule?')

    await field.setValue('A formatter should do this.')
    await click(page, 'Dismiss the proposal')
    expect(page.text()).toContain('R-22 dismissed')
  })

  it('asks why before retiring a stale rule', async () => {
    const page = await shown(Rules)
    await click(page, 'Retire')
    expect(page.find('input#retire-5').exists()).toBe(true)

    await page.find('input#retire-5').setValue('The importer is gone in #415.')
    await click(page, 'Retire the rule')
    expect(page.text()).toContain('R-05 retired')
    expect(page.text()).not.toContain('Going stale')
  })

  it('keeps a stale rule for another sixty days', async () => {
    const page = await shown(Rules)
    await click(page, 'Keep 60 more days')
    expect(page.text()).toContain('R-05 kept for 60 days')
    expect(page.text()).not.toContain('Going stale')
    expect(page.text()).toContain('Active · 5')
  })

  it('reads as nothing learned yet when there are no rules at all', async () => {
    await resetDesk({ rules: [], repos: [] })
    const page = await shown(Rules)
    const text = page.text()
    expect(text).toContain('No rules yet')
    expect(text).toContain('A rule comes out of a finding you kept')
    expect(page.find('.notice--blocked').exists()).toBe(false)
  })
})

describe('a read that is still in flight', () => {
  /*
   * A read that never answers, which is what a slow one looks like for as long as it
   * lasts. Sample mode cannot be caught in this state — it resolves in a microtask, inside
   * the mount — so this reads the real API with a `fetch` that is still waiting.
   */
  let answer: (body: string) => void = () => {}

  beforeEach(() => {
    useDeskSource('api')
    vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>((resolve) => {
      answer = (body: string) =>
        resolve({ ok: true, status: 200, text: () => Promise.resolve(body) } as Response)
    })))
  })

  afterEach(async () => {
    vi.unstubAllGlobals()
    await resetDesk()
  })

  it('reserves the space and says what it is reading', async () => {
    const page = await shown(Inbox)

    expect(page.text()).toContain('Reading the inbox…')
    expect(page.find('[role="status"][aria-live="polite"]').exists()).toBe(true)
    /*
     * The reserved height is the point: without it the content lands and everything below
     * it jumps, which DEV-862 calls out by name.
     */
    expect(page.find('.loading').exists()).toBe(true)

    answer(JSON.stringify({
      totals: { items: 0, repos: 0 },
      briefs_to_confirm: [],
      drafts_to_approve: [],
      author_replies: [],
      own_prs_without_reviewer: [],
      proposed_rules: [],
      in_progress: [],
      handled_without_you: { window_days: 7 },
    }))
    await settle(page)

    expect(page.text()).not.toContain('Reading the inbox…')
    expect(page.text()).toContain('Nothing here yet')
  })
})

describe('the server stops while a page is open', () => {
  /*
   * The one state sample mode cannot produce: it is always answering. So this reads the
   * real API with a `fetch` that fails, which is exactly what a stopped `review-desk serve`
   * looks like from the page.
   */
  let answering = false

  beforeEach(() => {
    useDeskSource('api')
    vi.stubGlobal('fetch', vi.fn(() => {
      if (!answering) return Promise.reject(new TypeError('Failed to fetch'))
      return Promise.resolve({
        ok: true,
        status: 200,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              totals: { items: 0, repos: 0 },
              briefs_to_confirm: [],
              drafts_to_approve: [],
              author_replies: [],
              own_prs_without_reviewer: [],
              proposed_rules: [],
              in_progress: [],
              handled_without_you: { window_days: 7 },
            }),
          ),
      } as Response)
    }))
  })

  afterEach(async () => {
    vi.unstubAllGlobals()
    answering = false
    await resetDesk()
  })

  it('says Review Desk is not answering, and names the command that starts it', async () => {
    const page = await shown(Inbox)
    const text = page.text()

    expect(text).toContain('Review Desk is not answering')
    expect(text).toContain('review-desk serve')
    /*
     * Not "the database is unreadable". That is `store_unavailable`, and it means Review
     * Desk is broken — sending the developer to debug a database when they have simply not
     * started the server.
     */
    expect(text).not.toContain('could not read its database')
    /* And not a blank page. */
    expect(page.find('.notice--blocked').exists()).toBe(true)
  })

  it('offers a retry that works once the server is back', async () => {
    const page = await shown(Inbox)
    expect(button(page, 'Retry')).toBeDefined()

    /* The developer starts the server, then clicks the retry. */
    answering = true
    await click(page, 'Retry')

    expect(page.text()).not.toContain('Review Desk is not answering')
    expect(page.text()).toContain('Nothing here yet')
  })
})
