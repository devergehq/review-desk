// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import StepConversation from '../app/components/StepConversation.vue'
import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'

/** A review the sample desk holds. */
const REVIEW = 41

async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
    await page.vm.$nextTick()
  }
}

async function conversation() {
  const page = await mountSuspended(StepConversation, {
    props: { reviewId: REVIEW, step: 'draft' },
    attachTo: document.body,
  })
  await settle(page)
  return page
}

beforeEach(async () => {
  await resetDesk()
})

describe('the conversation on a step', () => {
  it('offers to ask when nothing has been said', async () => {
    const page = await conversation()
    expect(page.find('summary').text()).toBe('Ask the agent about Draft')
  })

  it('keeps a question with the review and says it has not been read yet', async () => {
    const page = await conversation()
    await page.find('textarea').setValue('Why is F13 a Should?')
    const ask = page.findAll('button').find(button => button.text() === 'Ask')
    await ask!.trigger('click')
    await settle(page)

    expect(page.text()).toContain('Why is F13 a Should?')
    expect(page.text()).toContain('Not read yet')
    expect(page.find('summary').text()).toBe('1 message with the agent on this step · 1 not read yet')
    expect((page.find('textarea').element as HTMLTextAreaElement).value).toBe('')
  })

  it('shows a send-back note as part of the conversation', async () => {
    const page = await conversation()
    const desk = useReviewDesk()
    await desk.sendBackDraft(REVIEW, 'Regrade F13 to Blocker.')
    await settle(page)
    expect(page.text()).toContain('sent the draft back')
    expect(page.text()).toContain('Regrade F13 to Blocker.')
  })

  it('will not ask nothing', async () => {
    const page = await conversation()
    const ask = page.findAll('button').find(button => button.text() === 'Ask')
    expect(ask!.attributes('disabled')).toBeDefined()
  })
})
