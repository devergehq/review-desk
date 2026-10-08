// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { resetDesk } from '../app/composables/useReviewDesk'

import Inbox from '../app/pages/index.vue'
import OneReview from '../app/pages/reviews/[id]/index.vue'
import DraftPreview from '../app/pages/reviews/[id]/draft.vue'
import Rules from '../app/pages/rules.vue'
import Layout from '../app/layouts/default.vue'

/*
 * The four screens, mounted. These check that each one renders from the data layer and
 * that the controls DEV-855 names are real, labelled elements — not that it looks right,
 * which is what the screenshots in the pull request are for.
 */

describe('the shell', () => {
  it('marks the records as invented on every screen', async () => {
    const page = await mountSuspended(Layout)
    expect(page.text()).toContain('Sample data')
  })

  it('offers a way past the navigation for a keyboard', async () => {
    const page = await mountSuspended(Layout)
    expect(page.find('a[href="#main"]').exists()).toBe(true)
    expect(page.find('main#main').exists()).toBe(true)
  })

  it('names the three places you can be', async () => {
    const page = await mountSuspended(Layout)
    const links = page.findAll('nav a').map(link => link.text())
    expect(links).toEqual(['Inbox', 'Reviews', 'Rules'])
  })
})

describe('the inbox', () => {
  beforeEach(() => resetDesk())

  it('renders the five waiting groups', async () => {
    const page = await mountSuspended(Inbox)
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
    const page = await mountSuspended(Inbox)
    expect(page.text()).toContain('In progress · 2')
    expect(page.text()).toContain('Layer 5 of 7 · code review')
    const meters = page.findAll('[role="progressbar"]')
    expect(meters).toHaveLength(2)
    expect(meters[0]!.attributes('aria-valuenow')).toBe('5')
    expect(meters[0]!.attributes('aria-valuemax')).toBe('7')
  })

  it('says when nothing has reported a session as alive', async () => {
    const page = await mountSuspended(Inbox)
    expect(page.text()).toContain('session alive')
    expect(page.text()).toContain('session state unknown')
  })

  it('says when GitHub has told it nothing about a pull request', async () => {
    const page = await mountSuspended(Inbox)
    expect(page.text()).toContain('description lint passes')
    expect(page.text()).toContain('description not checked')
    expect(page.text()).toContain('open for an unrecorded time')
  })

  it('has a labelled field for starting a review', async () => {
    const page = await mountSuspended(Inbox)
    const field = page.find('input#pr-link')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="pr-link"]').text()).toBe('Start a review')
    expect(field.attributes('aria-describedby')).toBe('pr-link-help')
  })

  it('refuses a link that is not a pull request, and says why', async () => {
    const page = await mountSuspended(Inbox)
    await page.find('input#pr-link').setValue('nonsense')
    await page.findAll('button').find(button => button.text().includes('Understand it first'))!.trigger('click')
    expect(page.text()).toContain('Paste a link like github.com/acme/portal/pull/412.')
    expect(page.find('.notice--blocked').exists()).toBe(false)
  })

  it('says differently when GitHub could not be read, and names the fix', async () => {
    resetDesk({ github_available: false })
    const page = await mountSuspended(Inbox)
    await page.find('input#pr-link').setValue('https://github.com/acme/portal/pull/501')
    await page.findAll('button').find(button => button.text().includes('Understand it first'))!.trigger('click')

    const blocked = page.find('.notice--blocked')
    expect(blocked.exists()).toBe(true)
    expect(blocked.text()).toContain('gh auth login')
    expect(blocked.attributes('role')).toBe('status')
    /* The link stays in the field: nothing is wrong with it, so retyping it is waste. */
    expect((page.find('input#pr-link').element as HTMLInputElement).value).toBe(
      'https://github.com/acme/portal/pull/501',
    )
  })

  it('clears the field and reports plainly when the link is accepted', async () => {
    const page = await mountSuspended(Inbox)
    await page.find('input#pr-link').setValue('acme/portal#501')
    await page.findAll('button').find(button => button.text().includes('Understand it first'))!.trigger('click')
    expect(page.text()).toContain('acme/portal#501')
    expect(page.find('.notice--blocked').exists()).toBe(false)
    expect((page.find('input#pr-link').element as HTMLInputElement).value).toBe('')
  })

  it('links a reviewer request out to GitHub rather than writing to it', async () => {
    const page = await mountSuspended(Inbox)
    const out = page
      .findAll('a')
      .find(link => link.attributes('href')?.startsWith('https://github.com/'))
    expect(out).toBeDefined()
    expect(out!.attributes('href')).toBe('https://github.com/acme/portal/pull/415')
    expect(out!.attributes('rel')).toContain('noopener')
  })

  it('takes the brief out of the inbox once it is confirmed', async () => {
    const page = await mountSuspended(Inbox)
    expect(page.text()).toContain('Confirm the problem · 1')

    const { confirmBrief } = useReviewDesk()
    confirmBrief(17)
    await page.vm.$nextTick()

    expect(page.text()).not.toContain('Confirm the problem · 1')
    expect(page.text()).toContain('Approve a draft · 3')
  })
})

describe('one review', () => {
  beforeEach(() => resetDesk())

  async function open(id: number) {
    return mountSuspended(OneReview, { route: `/reviews/${id}` })
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
    const button = page.findAll('button').find(item => item.text() === 'Confirm the problem')
    expect(button).toBeDefined()

    await button!.trigger('click')

    expect(page.text()).toContain('Problem confirmed')
    expect(page.findAll('button').some(item => item.text() === 'Confirm the problem')).toBe(false)
  })

  it('reads as done on a review whose brief was already confirmed', async () => {
    const page = await open(15)
    expect(page.text()).toContain('Problem confirmed')
    expect(page.findAll('button').some(item => item.text() === 'Confirm the problem')).toBe(false)
  })

  it('moves a finding between the draft and held back, with a reason', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')

    const select = page.find('select#disposition-1')
    expect(page.find('label[for="disposition-1"]').exists()).toBe(true)
    await select.setValue('held_back')
    await page.find('input#reason-1').setValue('Covered by the ticket')
    await page.findAll('button').find(item => item.text() === 'Save')!.trigger('click')

    expect(page.text()).toContain('F1 is now held back for a ticket')
    expect(page.text()).toContain('Covered by the ticket')
  })

  it('will not move a finding out of the draft with no reason', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text().startsWith('Change'))!.trigger('click')
    await page.find('select#disposition-1').setValue('suppressed')
    await page.findAll('button').find(item => item.text() === 'Save')!.trigger('click')
    expect(page.text()).toContain('Say why it is not going in the draft.')
  })

  it('shows the author’s replies on a posted review', async () => {
    const page = await open(12)
    const text = page.text()
    expect(text).toContain('The author replied')
    expect(text).toContain('Rejected')
    expect(text).toContain('by the author')
    expect(text).toContain('Code changed after')
  })

  it('says so when there is no such review', async () => {
    const page = await mountSuspended(OneReview, { route: '/reviews/999' })
    expect(page.text()).toContain('No such review')
  })
})

describe('the draft preview', () => {
  beforeEach(() => resetDesk())

  async function open(id: number) {
    return mountSuspended(DraftPreview, { route: `/reviews/${id}/draft` })
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
    await page.findAll('button').find(item => item.text() === 'Save the draft')!.trigger('click')

    expect(page.text()).toContain('1 edit recorded for the harvester')
    expect(page.text()).toContain('F3 · wording')
  })

  it('approves with the chosen event, and will not approve twice', async () => {
    const page = await open(17)
    await page.find('select#event').setValue('comment')
    await page.findAll('button').find(item => item.text() === 'Approve and post')!.trigger('click')

    expect(page.text()).toContain('Approved as "Comment". Waiting for the session to post it.')
    expect(page.text()).toContain('The review session posts it under your name')
    const approve = page.findAll('button').find(item => item.text() === 'Approve and post')
    expect(approve!.attributes('disabled')).toBeDefined()
  })

  it('will not approve a draft with unsaved changes', async () => {
    const page = await open(17)
    await page.findAll('[role="tab"]')[2]!.trigger('click')
    await page.find('textarea#draft-body').setValue('Replaced entirely.')
    expect(page.text()).toContain('Save or discard your changes before approving.')
    const approve = page.findAll('button').find(item => item.text() === 'Approve and post')
    expect(approve!.attributes('disabled')).toBeDefined()
  })

  it('asks for a note before sending a draft back', async () => {
    const page = await open(17)
    await page.findAll('button').find(item => item.text() === 'Send back with a note')!.trigger('click')
    const field = page.find('textarea#send-back-note')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="send-back-note"]').exists()).toBe(true)

    await page.findAll('button').find(item => item.text() === 'Send back')!.trigger('click')
    expect(page.text()).toContain('A bare no teaches nothing.')

    await field.setValue('Fold the proof and shorten F2.')
    await page.findAll('button').find(item => item.text() === 'Send back')!.trigger('click')
    expect(page.text()).toContain('Sent back. The session has the note.')
    expect(page.text()).toContain('Fold the proof and shorten F2.')
  })

  it('files the held-back findings as one ticket', async () => {
    const page = await open(17)
    const button = page
      .findAll('button')
      .find(item => item.text() === 'File as one ticket for an agent')
    await button!.trigger('click')
    expect(page.text()).toContain('3 findings queued as one ticket')
    expect(page.text()).toContain('Queued as one ticket')
  })

  it('says so when the session has not written a draft yet', async () => {
    const page = await mountSuspended(DraftPreview, { route: '/reviews/18/draft' })
    expect(page.text()).toContain('No draft yet')
  })
})

describe('the rules', () => {
  beforeEach(() => resetDesk())

  it('renders the four figures, and leaves the unmeasured one absent', async () => {
    const page = await mountSuspended(Rules)
    const text = page.text()
    expect(text).toContain('Active rules')
    expect(text).toContain('cap of 20 per repository')
    expect(text).toContain('Same answer on re-run')
    expect(text).toContain('Not measured yet')
    expect(text).toContain('the replay check is a later slice')
    expect(text).toContain('31 of 40')
  })

  it('renders the proposed, active and stale groups', async () => {
    const page = await mountSuspended(Rules)
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
    const page = await mountSuspended(Rules)
    const text = page.text()
    expect(text).toContain('Caught by the check')
    expect(text).toContain('Repository rule · 1 exception')
    expect(text).toContain('CI check')
    expect(text).toContain('Last said by a human')
  })

  it('has a labelled repository picker that changes the list', async () => {
    const page = await mountSuspended(Rules)
    expect(page.find('label[for="repo"]').text()).toBe('Repository')
    expect(page.text()).toContain('Active · 4')

    await page.find('select#repo').setValue('acme/billing-api')

    expect(page.text()).toContain('Active · 2')
    expect(page.text()).toContain('A cache bust names every key the permission reaches')
    expect(page.text()).not.toContain('Proposed · 1')
  })

  it('promotes a proposal into the active list', async () => {
    const page = await mountSuspended(Rules)
    await page.findAll('button').find(item => item.text() === 'Promote')!.trigger('click')
    expect(page.text()).toContain('R-22 promoted')
    expect(page.text()).toContain('Active · 5')
    expect(page.text()).not.toContain('Proposed · 1')
  })

  it('asks why before dismissing a proposal', async () => {
    const page = await mountSuspended(Rules)
    await page.findAll('button').find(item => item.text() === 'Dismiss')!.trigger('click')
    const field = page.find('input#dismiss-22')
    expect(field.exists()).toBe(true)
    expect(page.find('label[for="dismiss-22"]').text()).toBe('Why is this not a rule?')

    await field.setValue('A formatter should do this.')
    await page.findAll('button').find(item => item.text() === 'Dismiss the proposal')!.trigger('click')
    expect(page.text()).toContain('R-22 dismissed')
  })

  it('asks why before retiring a stale rule', async () => {
    const page = await mountSuspended(Rules)
    await page.findAll('button').find(item => item.text() === 'Retire')!.trigger('click')
    expect(page.find('input#retire-5').exists()).toBe(true)

    await page.find('input#retire-5').setValue('The importer is gone in #415.')
    await page.findAll('button').find(item => item.text() === 'Retire the rule')!.trigger('click')
    expect(page.text()).toContain('R-05 retired')
    expect(page.text()).not.toContain('Going stale')
  })

  it('keeps a stale rule for another sixty days', async () => {
    const page = await mountSuspended(Rules)
    await page.findAll('button').find(item => item.text() === 'Keep 60 more days')!.trigger('click')
    expect(page.text()).toContain('R-05 kept for 60 days')
    expect(page.text()).not.toContain('Going stale')
    expect(page.text()).toContain('Active · 5')
  })
})
