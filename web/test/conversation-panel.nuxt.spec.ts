// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import ConversationPanel from '../app/components/ConversationPanel.vue'
import { useAskBox } from '../app/composables/useAskBox'
import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'
import reviews from '../app/data/reviews.json'
import type { Answer, ReviewDetail, ReviewMessage, SessionReading } from '../app/types/desk'

/*
 * *Ask this review* in each of S6's four states, and the answer in S7's shape. DEV-1333.
 *
 * Review 41 is the sample desk's `acme/portal #514`, the review the S screens draw.
 */

const REVIEW = 41
const DETAIL = (reviews as unknown as ReviewDetail[]).find(one => one.review.id === REVIEW)!

const IDLE: SessionReading = {
  status: 'finished',
  allele_state: 'idle',
  state_age_secs: 30,
  detail: null,
  read_at: '2026-11-21T06:00:00Z',
}

async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
    await page.vm.$nextTick()
  }
}

async function panel(tab: 'overview' | 'findings' = 'findings', route?: string) {
  const page = await mountSuspended(ConversationPanel, {
    props: { detail: DETAIL, step: tab === 'findings' ? 'code_review' : 'understand', tab },
    attachTo: document.body,
    ...(route ? { route } : {}),
  })
  await settle(page)
  mounted.push(page)
  return page
}

/* Each test's panel goes with it, so a shared reading it held cannot answer the next one. */
const mounted: VueWrapper[] = []
afterEach(() => {
  while (mounted.length) mounted.pop()!.unmount()
})

function said(seq: number, over: Partial<ReviewMessage>): ReviewMessage {
  return {
    review_id: REVIEW,
    seq,
    step: 'code_review',
    author: 'session',
    kind: 'reply',
    body: 'x',
    in_reply_to: null,
    created_at: '2026-11-21T06:00:00Z',
    read_at: null,
    read_by_session: null,
    about: { tab: 'findings', finding: null },
    parsed: false,
    answer: null,
    superseded_by: null,
    ...over,
  }
}

function answered(seq: number, answer: Answer, over: Partial<ReviewMessage> = {}): ReviewMessage {
  return said(seq, { parsed: true, answer, body: JSON.stringify(answer), ...over })
}

const ASK: ReviewMessage = said(1, {
  author: 'developer',
  kind: 'ask',
  body: 'Does it also happen on the web path?',
  about: { tab: 'findings', finding: 1 },
})

beforeEach(async () => {
  await resetDesk()
  useAskBox(REVIEW).fill('')
})

describe('no agent can answer', () => {
  it('says so in the header and takes no question', async () => {
    const page = await panel()
    expect(page.find('[data-testid="answering"]').text()).toBe('No agent connected · you can read, not ask')
    expect(page.find('textarea').attributes('disabled')).toBeDefined()
    expect(page.find('textarea').attributes('placeholder')).toContain('Without Allele')
    expect(page.find('[data-testid="about"]').exists()).toBe(false)
    expect(page.findAll('button').map(one => one.text())).not.toContain('Ask')
    expect(page.text()).not.toContain('Start with one of these')
    expect(page.text()).toContain('Everything asked so far is kept here and still reads.')
  })

  it('still draws everything already said', async () => {
    await resetDesk({ messages: [ASK, answered(2, { kind: 'answer', headline: 'Yes, the same hole.', cites: ['F1'] })] })
    const page = await panel()
    expect(page.text()).toContain('Does it also happen on the web path?')
    expect(page.text()).toContain('Yes, the same hole.')
  })
})

describe('nothing asked yet', () => {
  it('offers only the questions this review can support, and fills the box with one', async () => {
    await resetDesk({ session_readings: { [REVIEW]: IDLE } })
    const page = await panel('overview')
    expect(page.find('[data-testid="answering"]').text()).toBe('The session that wrote this review · idle')
    const offered = page.findAll('.suggested').map(one => one.text().replace('↗', '').trim())
    expect(offered).toContain('Why option C over the other seven?')
    expect(offered).not.toContain('Where did the second reader disagree?')
    expect(page.find('[data-testid="asked-so-far"]').exists()).toBe(false)
    expect(page.find('[data-testid="about"]').text()).toContain('About: the whole review')

    await page.findAll('.suggested')[0]!.trigger('click')
    expect((page.find('textarea').element as HTMLTextAreaElement).value)
      .toBe('What would you expect to go wrong after this merges?')
  })
})

describe('asking', () => {
  it('sends the question about the selected finding, and counts it on its tab', async () => {
    await resetDesk({ session_readings: { [REVIEW]: IDLE } })
    const page = await panel('findings', `/reviews/${REVIEW}/findings?finding=F1`)
    expect(page.find('[data-testid="about"]').text()).toContain('About: F1 · app/Reporting/ExportContext.php:44')
    await page.find('textarea').setValue('Could the author argue an empty list means all?')
    await page.find('form').trigger('submit')
    await settle(page)

    const desk = useReviewDesk()
    const kept = desk.messages(REVIEW)
    await kept.refresh()
    expect(kept.data.value!.messages.at(-1)!.about).toEqual({ tab: 'findings', finding: 1 })
    expect(page.text()).toContain('You · on F1')
    expect(page.find('[data-testid="asked-so-far"]').text()).toBe('Asked so far: Findings 1')
    expect((page.find('textarea').element as HTMLTextAreaElement).value).toBe('')
  })

  it('asks about the tab alone once the finding is taken off', async () => {
    await resetDesk({ session_readings: { [REVIEW]: IDLE } })
    const page = await panel('findings', `/reviews/${REVIEW}/findings?finding=F1`)
    await page.find('.unpin').trigger('click')
    expect(page.find('[data-testid="about"]').text()).toBe('About: Findings')
  })

  it('takes text something else filled the box with', async () => {
    await resetDesk({ session_readings: { [REVIEW]: IDLE } })
    const page = await panel()
    useAskBox(REVIEW).fill('And the CSV download?')
    await settle(page)
    expect((page.find('textarea').element as HTMLTextAreaElement).value).toBe('And the CSV download?')
  })
})

describe('an answer in the contract’s shape', () => {
  const answer: Answer = {
    kind: 'answer',
    headline: 'Yes, the web path has the same hole.',
    points: [
      { basis: 'ran', text: 'A web export of a definition with no columns returned all 14.' },
      { basis: 'read', text: 'ExportController.php:41 passes the definition straight through.' },
    ],
    cites: ['F1', 'F99', 'criterion 3'],
    not_sure: 'Didn’t check the CSV download button.',
    suggests: [{ type: 'regrade', finding: 'F1', from: 'should', to: 'blocker', why: 'Both paths leak.' }],
  }

  it('draws the headline, each point after its basis, the cites and what it is not sure of', async () => {
    await resetDesk({ messages: [ASK, answered(2, answer)] })
    const page = await panel()
    const reply = page.find('[data-kind="answer"]')
    expect(reply.find('.headline').text()).toBe('Yes, the web path has the same hole.')
    expect(reply.findAll('.points li').map(one => one.find('.basis').text())).toEqual(['Ran', 'Read'])
    expect(reply.find('.not-sure').text()).toContain('Didn’t check the CSV download button.')
  })

  it('links a cite the record holds and draws the rest as text', async () => {
    await resetDesk({ messages: [ASK, answered(2, answer)] })
    const page = await panel()
    const cites = page.findAll('.cites .cite')
    expect(cites.map(one => one.element.tagName)).toEqual(['A', 'SPAN', 'SPAN'])
    expect(cites[0]!.attributes('href')).toBe(`/reviews/${REVIEW}/findings?finding=F1`)
  })

  it('draws a suggestion with nothing to press', async () => {
    await resetDesk({ messages: [ASK, answered(2, answer)] })
    const page = await panel()
    const card = page.find('.suggestion')
    expect(card.text()).toContain('Regrade F1 from should to blocker')
    expect(card.findAll('button')).toHaveLength(0)
    expect(card.text()).not.toMatch(/Accept|Dismiss/)
  })

  it('draws what the session says it was asked as the session’s', async () => {
    await resetDesk({ messages: [answered(1, { ...answer, asked: 'Is the web path open too?' })] })
    const page = await panel()
    expect(page.text()).toContain('Your question, as your session recorded it')
    expect(page.find('.asked').text()).toBe('Is the web path open too?')
    expect(page.find('[data-author="developer"]').exists()).toBe(false)
  })
})

describe('working on an answer', () => {
  it('draws the newest word on the question, and its activity', async () => {
    await resetDesk({
      messages: [
        ASK,
        answered(2, { kind: 'working', headline: 'Starting', activity: ['Read ExportController.php'] }, { superseded_by: 3 }),
        answered(3, {
          kind: 'working',
          headline: 'Checking the web path',
          activity: ['Read ExportController.php', 'Found the web path calls the same policy', 'Running a web export'],
        }),
      ],
    })
    const page = await panel()
    expect(page.findAll('[data-kind="working"]')).toHaveLength(1)
    expect(page.text()).not.toContain('Starting')
    const steps = page.findAll('.activity li')
    expect(steps).toHaveLength(3)
    expect(steps.map(one => one.attributes('data-done') != null)).toEqual([true, true, false])
    expect(page.text()).toContain('You can keep reading; the answer lands here.')
    expect(page.text()).not.toMatch(/Working for/)
  })
})

describe('a session that can’t answer', () => {
  it('says so, with why and what would let it', async () => {
    await resetDesk({
      messages: [ASK, answered(2, {
        kind: 'cant',
        headline: 'I can’t run the web export here: no database.',
        points: [{ basis: 'told', text: 'A seeded database would let me run it.' }],
      })],
    })
    const page = await panel()
    const reply = page.find('[data-kind="cant"]')
    expect(reply.text()).toContain('can’t answer yet')
    expect(reply.text()).toContain('I can’t run the web export here: no database.')
    expect(reply.text()).toContain('A seeded database would let me run it.')
  })
})

describe('a reply that did not follow the format', () => {
  const long = `So looking at this more broadly, ${'the export pipeline has several layers '.repeat(30)}end.`

  it('is folded and inert, with one button', async () => {
    await resetDesk({ messages: [ASK, said(2, { body: long })] })
    const page = await panel()
    const reply = page.find('[data-kind="not_parsed"]')
    expect(reply.text()).toContain('This answer didn’t follow the format')
    expect(reply.find('summary').text()).toMatch(/^Show the rest \(\d+ words\)$/)
    expect(reply.text()).toContain('No cites and no basis, so no actions are offered and it isn’t kept with the review.')
    expect(reply.findAll('button').map(one => one.text())).toEqual(['Ask it again, in the format'])
  })

  it('asks again through the same send as the box', async () => {
    await resetDesk({ messages: [ASK, said(2, { body: long })], session_readings: { [REVIEW]: IDLE } })
    const page = await panel()
    await page.find('[data-kind="not_parsed"] button').trigger('click')
    await settle(page)
    const kept = useReviewDesk().messages(REVIEW)
    await kept.refresh()
    const last = kept.data.value!.messages.at(-1)!
    expect(last.author).toBe('developer')
    expect(last.body).toContain('reply 2')
  })

  it('cannot ask again where nobody can be asked', async () => {
    await resetDesk({ messages: [ASK, said(2, { body: long })] })
    const page = await panel()
    expect(page.find('[data-kind="not_parsed"] button').attributes('disabled')).toBeDefined()
  })
})
