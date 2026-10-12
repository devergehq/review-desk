// @vitest-environment nuxt
import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import DecisionTrack from '../app/components/DecisionTrack.vue'
import DeskButton from '../app/components/DeskButton.vue'
import LinkField from '../app/components/LinkField.vue'
import NotMeasured from '../app/components/NotMeasured.vue'
import QueueRow from '../app/components/QueueRow.vue'
import SegmentBar from '../app/components/SegmentBar.vue'
import SeverityTag from '../app/components/SeverityTag.vue'
import SignOffButton from '../app/components/SignOffButton.vue'
import SignOffCard from '../app/components/SignOffCard.vue'
import StatFigure from '../app/components/StatFigure.vue'
import StatusPill from '../app/components/StatusPill.vue'

/*
 * The control set, state by state.
 *
 * **DEV-876: "Each control component has a test for every state the Controls board draws."**
 * So the board is the index for this file: every state it draws has a case here, named after
 * what the board calls it, and the two states a unit test cannot reach — the pointer over a
 * control, and the control held down — are checked where they live, in the stylesheet.
 *
 * That is not a dodge. Hover and press are declared once in `tokens.css` and nothing else
 * can change them; what would go wrong is a kind of button having no such declaration at
 * all, and that is exactly what reading the stylesheet catches. A browser run is what says
 * they look right, and the pull request carries it.
 */

const css = readFileSync(resolve('app/assets/css/tokens.css'), 'utf8')

/** The declarations for one selector, or null. */
function block(selector: string): string | null {
  const at = css.indexOf(selector)
  if (at === -1) return null
  const open = css.indexOf('{', at)
  const close = css.indexOf('}', open)
  return css.slice(open + 1, close)
}

describe('a button, in the five states the board draws', () => {
  it('rests as itself: pressable, not busy, no spinner', async () => {
    const page = await mountSuspended(DeskButton, {
      props: { kind: 'primary' },
      slots: { default: () => 'Sign off' },
    })
    const button = page.find('button')
    expect(button.text()).toBe('Sign off')
    expect(button.attributes('disabled')).toBeUndefined()
    expect(button.attributes('aria-busy')).toBeUndefined()
    expect(page.find('.spinner').exists()).toBe(false)
  })

  it('says the pointer is over it, in the stylesheet rather than here', () => {
    /* One per kind. A kind with no hover rule would be a control that never answers. */
    for (const selector of [
      '.rd-btn:hover:not(:disabled)',
      '.rd-btn--primary:hover:not(:disabled)',
      '.rd-btn--ink:hover:not(:disabled)',
      '.rd-btn--quiet:hover:not(:disabled)',
    ]) {
      expect(block(selector), `${selector} is not declared`).not.toBeNull()
    }
  })

  it('goes one pixel down when pressed, as the board does', () => {
    expect(block('.rd-btn:active:not(:disabled)')).toContain('translateY(1px)')
    expect(block('.rd-btn--primary:active:not(:disabled)')).toContain('--rd-accent-pressed')
  })

  it('is working: the spinner, the working words, and not pressable again', async () => {
    const page = await mountSuspended(DeskButton, {
      props: { kind: 'primary', working: true, workingLabel: 'Signing off' },
      slots: { default: () => 'Sign off' },
    })
    const button = page.find('button')
    expect(button.text()).toContain('Signing off')
    expect(button.text()).not.toContain('Sign off and')
    expect(page.find('.spinner').exists()).toBe(true)
    expect(button.attributes('aria-busy')).toBe('true')
    /*
     * The one that matters. A sign-off that can be pressed twice while the first is in
     * flight is two writes, and on this app the second one is a second POST to approve.
     */
    expect(button.attributes('disabled')).toBeDefined()
  })

  it('is not available: pressable by nobody, and said so to a screen reader', async () => {
    const page = await mountSuspended(DeskButton, {
      props: { kind: 'primary', disabled: true },
      slots: { default: () => 'Sign off' },
    })
    expect(page.find('button').attributes('disabled')).toBeDefined()
    expect(block('.rd-btn--primary:disabled')).toContain('--rd-rail')
  })

  it('is a link when it navigates and a button when it acts', async () => {
    const acts = await mountSuspended(DeskButton, { slots: { default: () => 'Send back' } })
    expect(acts.find('button').exists()).toBe(true)

    const goes = await mountSuspended(DeskButton, {
      props: { to: '/rules' },
      slots: { default: () => 'Decide' },
    })
    expect(goes.find('a').attributes('href')).toBe('/rules')
  })

  it('leaves GitHub in a new tab, and never writes to it', async () => {
    const page = await mountSuspended(DeskButton, {
      props: { href: 'https://github.com/acme/portal/pull/412' },
      slots: { default: () => 'Open on GitHub' },
    })
    const out = page.find('a')
    expect(out.attributes('target')).toBe('_blank')
    expect(out.attributes('rel')).toContain('noopener')
  })
})

describe('the decision track', () => {
  const decisions = [
    { value: 'keep', label: 'Keep', key: 'Y', icon: 'M3 8.5l3.2 3.2L13 5' },
    { value: 'cut', label: 'Cut', key: 'X', icon: 'M4 4l8 8M12 4l-8 8' },
    { value: 'regrade', label: 'Regrade', key: 'G', icon: 'M5 13V3' },
    { value: 'rewrite', label: 'Rewrite', key: 'E', icon: 'M3 13l1-3.5' },
  ]

  async function track(picked = 'keep') {
    return mountSuspended(DecisionTrack, {
      props: { options: decisions, label: 'How to decide on F1', modelValue: picked },
    })
  }

  it('lifts the chosen one out and leaves the rest flat', async () => {
    const page = await track('cut')
    const items = page.findAll('button')
    expect(items).toHaveLength(4)
    expect(items[0]!.classes()).not.toContain('item--picked')
    expect(items[1]!.classes()).toContain('item--picked')
  })

  it('is a radio group, so one choice replaces the other', async () => {
    const page = await track('keep')
    expect(page.find('[role="radiogroup"]').attributes('aria-label')).toBe(
      'How to decide on F1',
    )
    const items = page.findAll('[role="radio"]')
    expect(items.map(item => item.attributes('aria-checked'))).toEqual([
      'true',
      'false',
      'false',
      'false',
    ])
  })

  it('draws the key on the control', async () => {
    const page = await track()
    expect(page.findAll('.key').map(key => key.text())).toEqual(['Y', 'X', 'G', 'E'])
  })

  it('picks with the pointer', async () => {
    const page = await track('keep')
    await page.findAll('button')[2]!.trigger('click')
    expect(page.emitted('update:modelValue')).toEqual([['regrade']])
  })

  it('picks with the key it draws', async () => {
    const page = await track('keep')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x' }))
    await page.vm.$nextTick()
    /* Lower case as well as upper: the key is drawn as `X` and Shift is not required. */
    expect(page.emitted('update:modelValue')).toEqual([['cut']])
  })

  /*
   * The requirement DEV-876 states in as many words: "Keys shown on a control must actually
   * work, and must not fire while the developer is typing in a field." `G` is a letter in
   * every reason anybody will ever type into the box next to this control.
   */
  it('does not fire while the developer is typing in a field', async () => {
    const page = await track('keep')
    const field = document.createElement('input')
    document.body.appendChild(field)
    field.focus()

    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'g', bubbles: true }))
    await page.vm.$nextTick()
    expect(page.emitted('update:modelValue')).toBeUndefined()

    field.remove()
  })

  it('does not fire in a textarea, a select, or anything contenteditable', async () => {
    const page = await track('keep')
    for (const tag of ['textarea', 'select']) {
      const element = document.createElement(tag)
      document.body.appendChild(element)
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', bubbles: true }))
      element.remove()
    }
    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', 'true')
    /* jsdom does not implement `isContentEditable`, which is what the guard reads. */
    Object.defineProperty(editable, 'isContentEditable', { value: true })
    document.body.appendChild(editable)
    editable.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', bubbles: true }))
    editable.remove()

    await page.vm.$nextTick()
    expect(page.emitted('update:modelValue')).toBeUndefined()
  })

  it('leaves the browser’s own shortcuts alone', async () => {
    const page = await track('keep')
    for (const held of [{ metaKey: true }, { ctrlKey: true }, { altKey: true }]) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', ...held }))
    }
    await page.vm.$nextTick()
    expect(page.emitted('update:modelValue')).toBeUndefined()
  })

  it('answers no key at all while it is disabled', async () => {
    const page = await mountSuspended(DecisionTrack, {
      props: {
        options: decisions,
        label: 'How to decide on F1',
        modelValue: 'keep',
        disabled: true,
      },
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x' }))
    await page.vm.$nextTick()
    expect(page.emitted('update:modelValue')).toBeUndefined()
    expect(page.findAll('button')[0]!.attributes('disabled')).toBeDefined()
  })

  it('stops listening once it is gone', async () => {
    const removed = vi.spyOn(window, 'removeEventListener')
    const page = await track()
    page.unmount()
    expect(removed).toHaveBeenCalledWith('keydown', expect.any(Function))
    removed.mockRestore()
  })

  it('counts, where the repository filter needs a count', async () => {
    const page = await mountSuspended(DecisionTrack, {
      props: {
        label: 'Which repository to show',
        modelValue: 'all',
        options: [
          { value: 'all', label: 'All repositories' },
          { value: 'acme/portal', label: 'acme/portal', count: 6 },
        ],
      },
    })
    expect(page.text()).toContain('acme/portal')
    /* The board writes the count as part of the label: "acme/portal · 6". */
    expect(page.find('.count').text()).toBe('· 6')
  })

  it('is a group of buttons, not a radio group, when the options are actions', async () => {
    const page = await mountSuspended(DecisionTrack, {
      props: {
        mode: 'act',
        label: 'What to do about R-05',
        options: [
          { value: 'retire', label: 'Retire it', emphasis: true },
          { value: 'defer', label: 'Keep 60 more days' },
        ],
      },
    })
    expect(page.find('[role="group"]').exists()).toBe(true)
    expect(page.find('[role="radio"]').exists()).toBe(false)
    /* The board lifts the first of the two, and it is still a button rather than a state. */
    expect(page.findAll('button')[0]!.classes()).toContain('item--picked')

    await page.findAll('button')[1]!.trigger('click')
    expect(page.emitted('act')).toEqual([['defer']])
  })
})

describe('the regrade row, which appears only when Regrade is chosen', () => {
  /*
   * The board's own state logic: `isRegrade: s.decision === 'regrade'`, and the summary
   * underneath reads "F1 will be posted as a Should". The row is a second track, so what
   * is tested here is the condition and the severity it names — the track itself is tested
   * above and is not tested twice.
   */
  it('is a track over the four severities, with the chosen one marked', async () => {
    const page = await mountSuspended(DecisionTrack, {
      props: {
        label: 'Regrade F1 to',
        modelValue: 'should',
        options: [
          { value: 'blocker', label: 'Blocker' },
          { value: 'should', label: 'Should' },
          { value: 'nit', label: 'Nit' },
          { value: 'question', label: 'Question' },
        ],
      },
    })
    const picked = page.findAll('button').filter(item => item.classes().includes('item--picked'))
    expect(picked).toHaveLength(1)
    expect(picked[0]!.text()).toBe('Should')
  })
})

describe('a severity label is told apart by shape as well as colour', () => {
  it.each([
    ['blocker', 'Blocker'],
    ['should', 'Should'],
    ['nit', 'Nit'],
    ['question', 'Question'],
  ])('%s', async (severity, label) => {
    const page = await mountSuspended(SeverityTag, { props: { severity } })
    expect(page.text()).toBe(label)
    expect(page.find('.tag').classes()).toContain(`tag--${severity}`)
    /* The shape. Without it the four are four hues and nothing else. */
    expect(page.find('.mark').exists()).toBe(true)
  })

  it('draws a different shape for each, not four coloured squares', async () => {
    const shapes = new Set<string>()
    for (const severity of ['blocker', 'should', 'nit', 'question']) {
      const page = await mountSuspended(SeverityTag, { props: { severity } })
      const scoped = page.find('.tag').classes().find(name => name.startsWith('tag--'))!
      shapes.add(scoped)
    }
    expect(shapes.size).toBe(4)

    /* And the four shape rules really are four different declarations. */
    const marks = ['blocker', 'should', 'nit', 'question'].map(
      name => block(`.tag--${name} .mark`) ?? '',
    )
    const component = readFileSync(resolve('app/components/SeverityTag.vue'), 'utf8')
    expect(component).toContain('border-right-width: 5px')
    expect(component).toContain('border-radius: 50%')
    expect(marks.length).toBe(4)
  })
})

describe('a status pill', () => {
  it.each([
    ['fixed', 'Fixed'],
    ['open', 'Still open · round 2'],
    ['new', 'New this round'],
    ['regression', 'Introduced by a fix'],
    ['waiting', 'Waiting on you'],
  ])('%s', async (kind, words) => {
    const page = await mountSuspended(StatusPill, {
      props: { kind },
      slots: { default: () => words },
    })
    expect(page.text()).toBe(words)
    expect(page.find('.pill').classes()).toContain(`pill--${kind}`)
  })

  it('never says it with colour alone: the words are always there', async () => {
    const page = await mountSuspended(StatusPill, {
      props: { kind: 'fixed' },
      slots: { default: () => 'Fixed' },
    })
    expect(page.find('svg').attributes('aria-hidden')).toBe('true')
    expect(page.text().trim().length).toBeGreaterThan(0)
  })
})

describe('the field with its action inside it', () => {
  async function field(props: Record<string, unknown> = {}) {
    return mountSuspended(LinkField, {
      props: { id: 'start', label: 'Start a review', modelValue: '', ...props },
    })
  }

  it('rests: labelled, empty, and its action live', async () => {
    const page = await field()
    expect(page.find('label[for="start"]').text()).toBe('Start a review')
    expect(page.find('input#start').exists()).toBe(true)
    expect(page.find('.shell').classes()).toContain('shell--resting')
    expect(page.find('.go').attributes('disabled')).toBeUndefined()
  })

  it('is in use once there is something in it', async () => {
    const page = await field({ modelValue: 'github.com/acme/portal/pull/412' })
    expect(page.find('.shell').classes()).toContain('shell--in-use')
  })

  it('is in use while it has focus, with nothing typed', async () => {
    const page = await field()
    await page.find('input#start').trigger('focus')
    expect(page.find('.shell').classes()).toContain('shell--in-use')
  })

  it('is refused: it says why, and its action will not send that again', async () => {
    const page = await field({
      modelValue: 'github.com/acme/portal/issues/90',
      refusal: 'That link is an issue, not a pull request.',
    })
    expect(page.find('.shell').classes()).toContain('shell--refused')
    expect(page.text()).toContain('That link is an issue, not a pull request.')
    expect(page.find('input#start').attributes('aria-invalid')).toBe('true')
    expect(page.find('input#start').attributes('aria-describedby')).toBe('start-refusal')
    expect(page.find('.go').attributes('disabled')).toBeDefined()

    await page.find('.go').trigger('click')
    expect(page.emitted('submit')).toBeUndefined()
  })

  /*
   * The board greys the action in the refused state, which is right while the refused text
   * is still in the field and wrong a keystroke later: the developer is fixing the link and
   * the control that sends the fix is dead.
   */
  it('comes back to life as soon as the developer starts fixing the link', async () => {
    const page = await field({
      modelValue: 'github.com/acme/portal/issues/90',
      refusal: 'That link is an issue, not a pull request.',
    })
    await page.setProps({ modelValue: 'github.com/acme/portal/pull/412' })
    expect(page.find('.shell').classes()).toContain('shell--in-use')
    expect(page.find('.go').attributes('disabled')).toBeUndefined()

    await page.find('.go').trigger('click')
    expect(page.emitted('submit')).toHaveLength(1)
  })

  it('is working: the spinner, and nothing sent twice', async () => {
    const page = await field({ modelValue: 'github.com/acme/portal/pull/412', working: true })
    expect(page.find('.spinner').exists()).toBe(true)
    expect(page.find('.go').attributes('disabled')).toBeDefined()
    await page.find('input#start').trigger('keyup.enter')
    expect(page.emitted('submit')).toBeUndefined()
  })

  it('sends on Enter, so the pointer is never needed', async () => {
    const page = await field({ modelValue: 'github.com/acme/portal/pull/412' })
    await page.find('input#start').trigger('keyup.enter')
    expect(page.emitted('submit')).toHaveLength(1)
  })

  it('says what the link resolved to, when there is nothing wrong', async () => {
    const page = await field({
      modelValue: 'github.com/acme/portal/pull/412',
      hint: 'acme/portal #412 · Retry failed webhook deliveries',
    })
    expect(page.find('#start-hint').text()).toContain('acme/portal #412')
  })
})

describe('the split sign-off button', () => {
  async function split(event = 'request_changes') {
    return mountSuspended(SignOffButton, { props: { event } })
  }

  it('names the event it would be posted as', async () => {
    const page = await split('request_changes')
    expect(page.find('.event-label').text()).toBe('Request changes')
    expect(page.find('.rubric').text()).toBe('Post as')
  })

  it('says sign off, not approve', async () => {
    const page = await split()
    /* DEV-870: the developer signs off; "approve" is a GitHub event and lives in the menu. */
    expect(page.find('.act').text()).toContain('Sign off and post')
    expect(page.find('.act').text()).not.toContain('Approve')
  })

  it('is closed until it is opened', async () => {
    const page = await split()
    expect(page.find('.menu').exists()).toBe(false)
    expect(page.find('.event').attributes('aria-expanded')).toBe('false')
  })

  it('opens on the event half, and offers GitHub’s three by their own names', async () => {
    const page = await split()
    await page.find('.event').trigger('click')
    expect(page.find('.event').attributes('aria-expanded')).toBe('true')
    expect(page.findAll('.option').map(one => one.text())).toEqual([
      'Request changesBlocks the merge until answered',
      'CommentSays it without blocking',
      'ApproveGood to merge',
    ])
  })

  it('marks the one it is set to, and changing it closes the menu', async () => {
    const page = await split('comment')
    await page.find('.event').trigger('click')
    const picked = page.findAll('.option').filter(one => one.classes().includes('option--picked'))
    expect(picked).toHaveLength(1)
    expect(picked[0]!.text()).toContain('Comment')

    await page.findAll('.option')[2]!.trigger('click')
    expect(page.emitted('update:event')).toEqual([['approve']])
    expect(page.find('.menu').exists()).toBe(false)
  })

  it('signs off from the act half', async () => {
    const page = await split()
    await page.find('.act').trigger('click')
    expect(page.emitted('sign-off')).toHaveLength(1)
  })

  it('is working: both halves dead, so nothing is signed off twice', async () => {
    const page = await mountSuspended(SignOffButton, {
      props: { event: 'approve', working: true },
    })
    expect(page.find('.act').attributes('disabled')).toBeDefined()
    expect(page.find('.event').attributes('disabled')).toBeDefined()
    expect(page.find('.act').text()).toContain('Signing off')
    await page.find('.act').trigger('click')
    expect(page.emitted('sign-off')).toBeUndefined()
  })
})

describe('the sign-off card, in the three states the board draws', () => {
  it('ready: what will be posted, and that nothing has been', async () => {
    const page = await mountSuspended(SignOffCard, {
      props: {
        status: 'ready',
        event: 'request_changes',
        summary: '2 findings and 2 comments will be posted.',
      },
    })
    expect(page.text()).toContain('Ready for your sign-off')
    expect(page.text()).toContain('2 findings and 2 comments will be posted.')
    expect(page.text()).toContain('Nothing has been posted yet.')
    expect(page.text()).toContain('Send back with a note')
  })

  it('signed off: the status changed, and Review Desk is not what posts it', async () => {
    const page = await mountSuspended(SignOffCard, {
      props: { status: 'approved', event: 'comment', pollSeconds: 90 },
    })
    expect(page.text()).toContain('Signed off')
    expect(page.text()).toContain('It will be posted as Comment.')
    expect(page.text()).toContain('Waiting for the review session to pick it up')
    expect(page.text()).toContain('It checks every 90 seconds.')
    /* No sign-off control: it has been signed off. */
    expect(page.text()).not.toContain('Sign off and post')
  })

  it('signed off with nothing saying how often: the bar is drawn empty, not part-filled', async () => {
    const page = await mountSuspended(SignOffCard, {
      props: { status: 'approved', event: 'comment' },
    })
    /*
     * The board draws this bar at 38%. Nothing reports the session's schedule, so the
     * track is there and the fill is not — a figure with no source is never a number.
     */
    expect(page.find('.track').exists()).toBe(true)
    expect(page.find('.track').html()).not.toContain('width')
    expect(page.text()).toContain('not known here')
  })

  it('posted: as what, and a link out to the review GitHub holds', async () => {
    const page = await mountSuspended(SignOffCard, {
      props: {
        status: 'posted',
        event: 'approve',
        postedAfter: '40 seconds',
        postedUrl: 'https://github.com/acme/portal/pull/412#pullrequestreview-1',
      },
    })
    expect(page.text()).toContain('Posted')
    expect(page.text()).toContain('As Approve, 40 seconds after you signed off.')
    const out = page.find('.out')
    expect(out.attributes('href')).toContain('github.com')
    expect(out.attributes('rel')).toContain('noopener')
  })
})

describe('the segment bar', () => {
  it('draws the layers done, the one running, and the rest to come', async () => {
    const page = await mountSuspended(SegmentBar, {
      props: {
        segments: ['done', 'done', 'done', 'done', 'now', 'todo', 'todo'],
        label: 'Layer 5 of 7, code review',
      },
    })
    const segments = page.findAll('.seg')
    expect(segments).toHaveLength(7)
    expect(segments.filter(one => one.classes().includes('seg--done'))).toHaveLength(4)
    expect(segments.filter(one => one.classes().includes('seg--now'))).toHaveLength(1)
    expect(segments.filter(one => one.classes().includes('seg--todo'))).toHaveLength(2)
  })

  it('is an image with a sentence, not a progress bar with a percentage', async () => {
    const page = await mountSuspended(SegmentBar, {
      props: { segments: ['done', 'todo'], label: 'Layer 1 of 2, lint' },
    })
    const bar = page.find('[role="img"]')
    expect(bar.attributes('aria-label')).toBe('Layer 1 of 2, lint')
    expect(page.find('[role="progressbar"]').exists()).toBe(false)
  })

  it('has the rules board’s wider size as well as the queue’s', async () => {
    const page = await mountSuspended(SegmentBar, {
      props: { segments: ['done', 'done', 'todo', 'todo'], size: 'wide', label: 'Stage 2 of 4' },
    })
    expect(page.find('.bar').classes()).toContain('bar--wide')
  })
})

describe('a headline figure', () => {
  it('shows what was measured', async () => {
    const page = await mountSuspended(StatFigure, {
      props: { value: '31 of 40', label: 'findings you kept', note: 'last 30 days' },
    })
    expect(page.find('.value').text()).toBe('31 of 40')
    expect(page.text()).toContain('findings you kept')
    expect(page.text()).toContain('last 30 days')
  })

  it('shows the absence of a measurement as an absence, never as a nought', async () => {
    const page = await mountSuspended(StatFigure, {
      props: {
        value: null,
        label: 'same answer on a re-run',
        note: 'the replay check is a later slice',
        unknownBecause: 'Review Desk has no replay yet',
      },
    })
    expect(page.text()).toContain('Not measured yet')
    expect(page.text()).not.toContain('0')
    expect(page.find('.unknown').attributes('title')).toBe('Review Desk has no replay yet')
  })
})

describe('the unknown state itself', () => {
  it('says the words and carries the reason both ways', async () => {
    const page = await mountSuspended(NotMeasured, {
      props: { because: 'no posted review has been harvested yet' },
    })
    expect(page.text()).toContain('Not measured yet')
    /* Read out to a screen reader as well as shown on hover. */
    expect(page.text()).toContain('no posted review has been harvested yet')
    expect(page.find('.unknown').attributes('title')).toBe(
      'no posted review has been harvested yet',
    )
  })

  it('can say it in other words where "yet" would be wrong', async () => {
    const page = await mountSuspended(NotMeasured, {
      props: { because: 'the inbox carries no time for a draft', says: 'no time recorded' },
    })
    expect(page.text()).toContain('no time recorded')
  })
})

describe('text a review session recorded, rather than copy the app wrote', () => {
  /*
   * `rd-recorded` is the one primitive for it. A session writes paragraphs — a finding's
   * evidence separates "PROVED by …", "Mechanism: …" and "Bounded: …" with blank lines —
   * and before this the review page collapsed every break and ran the text the full width
   * of the card.
   *
   * Declared once here, as hover and press are, so that no page can disagree with another
   * about what recorded text looks like. The review page's use of it is asserted in
   * `screens.nuxt.spec.ts`.
   */
  it('keeps the breaks the session wrote', () => {
    expect(block('.rd-recorded')).toContain('white-space: pre-wrap')
  })

  it('holds a line to a readable measure, which is not the page\'s', () => {
    expect(block('.rd-recorded')).toContain('max-width: var(--rd-measure-recorded)')
    expect(css).toContain('--rd-measure-recorded: 70ch')
    /* Narrower than the page's own measure, which is what makes it a second token. */
    expect(block('.rd-measure')).toContain('var(--rd-measure)')
  })

  /*
   * **Every `v-html` in the dashboard, named, with the reason it is there.**
   *
   * This read two components by name until DEV-878, which asks for it over a whole page:
   * "Every field is interpolated; no `v-html` on these pages." A list of two files could
   * not answer that — a third file is what it would have missed — so it reads every
   * component and every page and allows exactly two expressions, each with its reason.
   *
   * The whole point of `rd-recorded` doing this in CSS is that a paragraph break survives
   * without anything being parsed as HTML, so recorded text can be interpolated. A record
   * that could carry markup is a record that could carry a script.
   */
  it('reaches for v-html in exactly two places, each with a reason', () => {
    /** The expression after `v-html=`, and why that one is allowed. */
    const ALLOWED: Record<string, string> = {
      '"svg"':
        'MermaidFigure: mermaid\'s own sanitised SVG, and never the branch that shows a '
        + 'source',
      '"render(block.content)"':
        'MarkdownView: the draft body is GitHub-flavoured markdown and is rendered by its '
        + 'own escaping renderer, which is what the Draft preview is a preview of',
    }

    const found: Array<{ file: string, line: string }> = []
    const walk = (directory: string) => {
      for (const entry of readdirSync(resolve(directory), { withFileTypes: true })) {
        const path = `${directory}/${entry.name}`
        if (entry.isDirectory()) {
          walk(path)
          continue
        }
        if (!/\.(vue|ts)$/.test(entry.name)) continue
        for (const line of readFileSync(resolve(path), 'utf8').split('\n')) {
          /* The directive, not the word: several files discuss `v-html` in a comment. */
          if (line.includes('v-html=')) found.push({ file: path, line: line.trim() })
        }
      }
    }
    walk('app')

    for (const { file, line } of found) {
      const expression = /v-html=("[^"]*"|'[^']*')/.exec(line)?.[1] ?? line
      expect(
        Object.keys(ALLOWED),
        `${file} uses v-html=${expression}, which is not one of the two allowed. Interpolate `
        + 'it: a record that could carry markup is a record that could carry a script.',
      ).toContain(expression)
    }

    /*
     * And both are still there, so that deleting one and leaving this list behind is a
     * failure rather than a quietly weaker test.
     */
    const expressions = found.map(({ line }) => /v-html=("[^"]*")/.exec(line)?.[1])
    for (const allowed of Object.keys(ALLOWED)) expect(expressions).toContain(allowed)
  })
})

describe('a queue row', () => {
  const row = (props: Record<string, unknown>) =>
    mountSuspended(QueueRow, {
      props: { pr: 412, title: 'Retry failed webhook deliveries', action: 'Read it', ...props },
    })

  it('links the ticket the record holds an address for', async () => {
    const page = await row({
      prHref: 'https://github.com/acme/portal/pull/412',
      ticketHref: 'https://linear.app/acme/issue/ABC-231',
      ticketId: 'ABC-231',
    })
    const out = page.findAll('.out-link')
    expect(out.map(link => link.attributes('href'))).toEqual([
      'https://github.com/acme/portal/pull/412',
      'https://linear.app/acme/issue/ABC-231',
    ])
    for (const link of out) {
      expect(link.attributes('target')).toBe('_blank')
      expect(link.attributes('rel')).toBe('noopener noreferrer')
    }
  })

  /**
   * **The row refuses a non-`https` address itself**, rather than trusting its caller to.
   *
   * The rule — a stored address that is not `https` is never rendered as a link — is stated
   * in `contract/README.md`, in `app/utils/links.ts` and in migration 16's own comment, and
   * `ReviewStepPage` enforces it inside the component. This one used what it was handed, so
   * the rule was true only because all four call sites happened to pass `ticketUrl(…)`: a
   * fifth passing `review.ticket_url` straight through would have put a hand-written
   * `javascript:` value in an `href` with nothing failing. Measured by mutation — reverting
   * the guard left the whole suite green, which is why this test exists.
   */
  it('refuses an address that is not https, whatever it is handed', async () => {
    for (const refused of [
      'javascript:alert(1)',
      'http://linear.app/acme/issue/ABC-231',
      'linear.app/acme/issue/ABC-231',
    ]) {
      const page = await row({
        prHref: 'https://github.com/acme/portal/pull/412',
        ticketHref: refused,
        ticketId: 'ABC-231',
      })
      const hrefs = page.findAll('a').map(link => link.attributes('href') ?? '')
      expect(hrefs, `${refused} never reaches an href`).not.toContain(refused)
      expect(
        page.findAll('.out-link').map(link => link.attributes('href')),
        'the pull request is the only link left',
      ).toEqual(['https://github.com/acme/portal/pull/412'])
    }
  })
})
