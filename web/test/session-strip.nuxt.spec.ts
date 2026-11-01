// @vitest-environment nuxt
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import SessionStrip from '../app/components/SessionStrip.vue'
import { resetDesk, useDeskSource } from '../app/composables/useReviewDesk'

/** A review id no other test uses, so the shared session read is made fresh here. */
const REVIEW = 5511

const GONE = {
  status: 'gone',
  allele_state: null,
  state_age_secs: null,
  detail: 'Allele has no session with this id',
  read_at: '2026-10-30T11:20:00Z',
}

let fresh: Record<string, unknown> | null = null
let posted = 0

function answer(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
    await page.vm.$nextTick()
  }
}

beforeEach(async () => {
  await resetDesk()
  useDeskSource('api')
  fresh = null
  posted = 0
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith('/fresh-session') && init?.method === 'POST') {
      posted += 1
      fresh = { requested_at: new Date().toISOString(), declined_at: null, declined_reason: null }
    }
    if (url.includes('/session') || url.endsWith('/fresh-session')) {
      return answer({ review_id: REVIEW, session_id: 'allele-old', fresh_session: fresh, reading: GONE })
    }
    return answer({}, 404)
  }))
})

afterEach(() => {
  useDeskSource(null)
  vi.unstubAllGlobals()
})

describe('a session that is gone', () => {
  it('offers a fresh one, asks before starting it, and then says it is waiting', async () => {
    const page = await mountSuspended(SessionStrip, {
      props: { reviewId: REVIEW },
      attachTo: document.body,
    })
    await settle(page)
    expect(page.text()).toContain('Session gone')

    const offer = page.findAll('button').find(b => b.text() === 'Start a fresh session')
    await offer!.trigger('click')
    await settle(page)
    expect(page.text()).toContain('Start a fresh session on this review?')
    expect(page.text()).toContain('Nothing is posted. You still sign off.')
    expect(posted).toBe(0)

    const start = page.find('.confirm').findAll('button').find(b => b.text() === 'Start a fresh session')
    await start!.trigger('click')
    await settle(page)

    expect(posted).toBe(1)
    expect(page.find('[data-testid=fresh-session]').text()).toContain('Asked for a fresh session')
    expect(page.findAll('button').some(b => b.text() === 'Start a fresh session')).toBe(false)
  })

  it('says why when the dispatcher declined, and offers again', async () => {
    fresh = {
      requested_at: '2026-10-30T11:20:00Z',
      declined_at: '2026-10-30T11:20:40Z',
      declined_reason: 'Allele says allele-old is running',
    }
    const page = await mountSuspended(SessionStrip, {
      props: { reviewId: REVIEW + 1 },
      attachTo: document.body,
    })
    await settle(page)
    expect(page.text()).toContain('Not started · Allele says allele-old is running')
    expect(page.findAll('button').some(b => b.text() === 'Start a fresh session')).toBe(true)
  })
})
