// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import ConversationPanel from '../app/components/ConversationPanel.vue'
import { useAskBox } from '../app/composables/useAskBox'
import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'
import reviews from '../app/data/reviews.json'
import type { Answer, ReviewDetail, ReviewMessage, Suggestion } from '../app/types/desk'

/*
 * Accept and Dismiss on the session's suggestions, through the panel. DEV-1334, S8.
 *
 * Review 41 is the sample desk's `acme/portal #514`. Its draft is written in parts, so F6 has a
 * block to regrade. The sample backend keeps what the store keeps: the decision, the finding,
 * and which reply prompted it.
 */

const REVIEW = 41
const DETAIL = (reviews as unknown as ReviewDetail[]).find(one => one.review.id === REVIEW)!

async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
    await page.vm.$nextTick()
  }
}

const mounted: VueWrapper[] = []
afterEach(() => {
  while (mounted.length) mounted.pop()!.unmount()
})

async function panel() {
  const page = await mountSuspended(ConversationPanel, {
    props: { detail: DETAIL, step: 'code_review', tab: 'findings' },
    attachTo: document.body,
  })
  await settle(page)
  mounted.push(page)
  return page
}

function reply(seq: number, answer: Answer): ReviewMessage {
  return {
    review_id: REVIEW,
    seq,
    step: 'code_review',
    author: 'session',
    kind: 'reply',
    body: JSON.stringify(answer),
    in_reply_to: null,
    created_at: '2026-11-21T06:00:00Z',
    read_at: null,
    read_by_session: null,
    about: { tab: 'findings', finding: null },
    parsed: true,
    answer,
    superseded_by: null,
    suggestion_answers: [],
  }
}

function suggesting(...suggests: Suggestion[]): ReviewMessage {
  return reply(1, {
    kind: 'answer',
    headline: 'Yes. The web path has the same hole, and it isn’t in the findings yet.',
    points: [{ basis: 'ran', text: 'A web export with no columns set returned all 14.' }],
    cites: ['F9', 'ExportController.php:41'],
    suggests,
  })
}

const NEW: Suggestion = {
  type: 'new_finding',
  severity: 'should',
  title: 'The web path exports private columns when none are set',
  where: 'ExportController.php:41',
  basis: 'ran',
}
const REGRADE: Suggestion = { type: 'regrade', finding: 'F6', from: 'should', to: 'blocker', why: 'Both paths leak.' }
const FOLLOW: Suggestion = { type: 'follow_up', question: 'Does the CSV download use a third path?' }

async function findingNow(seq: number) {
  const detail = useReviewDesk().review(REVIEW)
  await detail.refresh()
  return detail.data.value!.findings.find(one => one.seq === seq)
}

beforeEach(async () => {
  await resetDesk()
  useAskBox(REVIEW).fill('')
})

describe('accepting', () => {
  it('regrades through the page’s own decision, and says so in place of the buttons', async () => {
    await resetDesk({ messages: [suggesting(REGRADE)] })
    const page = await panel()
    expect(page.find('[data-testid="dismiss-regrade"]').text()).toBe('Keep as should')

    await page.find('[data-testid="accept-regrade"]').trigger('click')
    await settle(page)

    expect(page.find('[data-testid="suggestion-actions"]').text()).toContain('Regraded F6 to blocker')
    expect(page.find('[data-testid="accept-regrade"]').exists()).toBe(false)
    expect((await findingNow(6))?.severity).toBe('blocker')
    const outcome = useReviewDesk().review(REVIEW)
    await outcome.refresh()
    expect(outcome.data.value!.outcomes.at(-1)).toMatchObject({ finding_key: 'F6', kind: 'regraded', actor: 'developer' })
  })

  it('adds a new finding to To post, marked with the reply that prompted it', async () => {
    await resetDesk({ messages: [suggesting(NEW)] })
    const page = await panel()

    await page.find('[data-testid="accept-new_finding"]').trigger('click')
    await settle(page)

    const card = page.find('[data-testid="suggestion-actions"]').text()
    expect(card).toContain('Added as F16 · should · ran')
    expect(card).toContain('Open F16')
    const added = await findingNow(16)
    expect(added).toMatchObject({ disposition: 'in_draft', prompted_by: 1, file: 'ExportController.php', line: 41 })
    expect(added?.round_status_by).toBeNull()
  })

  it('fills the ask box with a follow-up and writes nothing', async () => {
    await resetDesk({ messages: [suggesting(FOLLOW)] })
    const page = await panel()

    await page.find('[data-testid="accept-follow_up"]').trigger('click')
    await settle(page)

    expect(useAskBox(REVIEW).text.value).toBe('Does the CSV download use a third path?')
    expect(page.find('[data-testid="suggestion-actions"]').text()).toContain('Nothing is sent until you ask.')
    const messages = useReviewDesk().messages(REVIEW)
    await messages.refresh()
    expect(messages.data.value!.messages).toHaveLength(1)
    expect(messages.data.value!.messages[0]!.suggestion_answers).toEqual([])
  })
})

describe('dismissing', () => {
  it('is one click, and the reason comes after it', async () => {
    await resetDesk({ messages: [suggesting(REGRADE)] })
    const page = await panel()

    await page.find('[data-testid="dismiss-regrade"]').trigger('click')
    await settle(page)
    expect(page.find('[data-testid="suggestion-actions"]').text()).toContain('Dismissed')
    expect((await findingNow(6))?.severity).toBe('should')

    await page.find('.say-why').trigger('click')
    await page.find('.reason input').setValue('F6 stays a should until the web path is fixed.')
    await page.find('.reason').trigger('submit')
    await settle(page)
    expect(page.find('[data-testid="suggestion-actions"]').text())
      .toContain('Dismissed · “F6 stays a should until the web path is fixed.”')
  })
})

describe('turn into a finding', () => {
  it('adds the answer as a finding through the same call, once', async () => {
    await resetDesk({ messages: [suggesting()] })
    const page = await panel()
    const turn = page.find('[data-testid="turn-into-a-finding"]')
    expect(turn.text()).toBe('Turn into a finding')

    await turn.find('button').trigger('click')
    await settle(page)

    expect(page.find('[data-testid="turn-into-a-finding"]').text()).toContain('Added as F16')
    expect(await findingNow(16)).toMatchObject({ prompted_by: 1, severity: 'should', file: 'ExportController.php' })
  })
})
