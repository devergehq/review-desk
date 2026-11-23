import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'
import ListPreview from '../app/components/ListPreview.vue'
import type { ListRow } from '../app/utils/pullRequests'

/*
 * **What the preview's body draws in each state of its held read** — DEV-1374. The
 * composable's states are tested in `held-reviews.spec.ts`; this renders them, so a branch
 * of the template landing on the wrong sibling fails here (self-review round 2).
 */

const reads = vi.hoisted(() => ({ failing: new Set<number>() }))

vi.mock('../app/data/sample', async (original) => {
  const actual = await original<typeof import('../app/data/sample')>()
  return {
    ...actual,
    sampleBackend: () => {
      const backend = actual.sampleBackend()
      return {
        ...backend,
        getReview: async (id: number) => {
          if (reads.failing.has(id)) throw new Error('the server is not answering')
          return backend.getReview(id)
        },
      }
    },
  }
})

const WEBHOOKS = 17 /* #412, whose brief is waiting */

function row(over: Partial<ListRow> = {}): ListRow {
  return {
    key: 'acme/portal#412',
    reviewId: WEBHOOKS,
    pr: 412,
    repo: 'acme/portal',
    title: 'Retry failed webhook deliveries',
    state: null,
    to: `/reviews/${WEBHOOKS}`,
    href: null,
    action: null,
    session: null,
    ...over,
  }
}

async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
  }
  await page.vm.$nextTick()
}

const shown = (page: VueWrapper, id: string) => page.find(`[data-testid="${id}"]`).exists()

beforeEach(async () => {
  await resetDesk()
  reads.failing.clear()
})

describe('the list preview’s body', () => {
  it('says Reading the review… while nothing is held and the row has not rested', async () => {
    const page = await mountSuspended(ListPreview, { props: { row: row(), rested: false } })
    await settle(page)
    expect(shown(page, 'preview-reading')).toBe(true)
    expect(shown(page, 'preview-unread')).toBe(false)
    expect(page.text(), 'the header is the row’s own').toContain('#412')
  })

  it('draws the bottom line once the rested row is read', async () => {
    const page = await mountSuspended(ListPreview, { props: { row: row(), rested: true } })
    await settle(page)
    expect(shown(page, 'bottom-line')).toBe(true)
    expect(shown(page, 'preview-reading')).toBe(false)
    expect(shown(page, 'preview-unread')).toBe(false)
  })

  it('says the review could not be read when the read failed with nothing held', async () => {
    reads.failing.add(WEBHOOKS)
    const page = await mountSuspended(ListPreview, { props: { row: row(), rested: true } })
    await settle(page)
    expect(shown(page, 'preview-unread')).toBe(true)
    expect(page.find('[data-testid="preview-unread"]').text()).toContain('The review could not be read.')
    expect(shown(page, 'preview-reading')).toBe(false)
    expect(shown(page, 'bottom-line')).toBe(false)
  })

  it('says no review has been opened for a row with none, and reads nothing', async () => {
    const page = await mountSuspended(ListPreview, {
      props: { row: row({ reviewId: null, to: null, href: 'https://github.com/acme/portal/pull/412' }), rested: true },
    })
    await settle(page)
    expect(page.text()).toContain('No review has been opened for this pull request.')
    expect(shown(page, 'preview-reading')).toBe(false)
    expect(shown(page, 'preview-unread')).toBe(false)
  })
})
