// @vitest-environment nuxt
import { beforeEach, describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { flushPromises, type VueWrapper } from '@vue/test-utils'

import { resetDesk } from '../app/composables/useReviewDesk'

import { loadSource } from '../app/data/sample'
import ReviewHeader from '../app/components/ReviewHeader.vue'
import { stateOfReview, stateWord } from '../app/utils/stateWord'

import type { ReviewDetail } from '../app/types/desk'

/*
 * DEV-1324: the header every tab of a review draws. The rules it keeps are asserted as
 * functions in `review-tabs.spec.ts`; this asserts that the header draws what they answer,
 * and the browser measures its height in `e2e/shell.spec.ts`.
 */

/** #514, whose draft is waiting on the developer. */
const DRAFT_READY = 41

function fixture(id: number): ReviewDetail {
  const found = loadSource().reviews.find(review => review.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return found
}

/**
 * Mounted, and settled: the header holds its word and its action until the inbox it shares
 * with the sidebar has answered, so a test reads it after that read lands.
 */
async function settle(page: VueWrapper) {
  for (let round = 0; round < 20; round += 1) {
    await new Promise(resolve => setTimeout(resolve, 0))
    await flushPromises()
  }
  return page
}

const header = async (
  detail: ReviewDetail,
  tab: 'overview' | 'findings' = 'overview',
) => settle(await mountSuspended(ReviewHeader, { props: { detail, tab } }))

beforeEach(async () => {
  await resetDesk()
})

describe('the review header', () => {
  it('draws the crumb, the number and the title', async () => {
    const page = await header(fixture(DRAFT_READY))
    expect(page.find('.crumb').text()).toMatch(/^Pull requests\s*›\s*acme\/portal$/)
    expect(page.find('.crumb a').attributes('href')).toBe('/')
    expect(page.find('h1').text()).toContain('#514')
    expect(page.find('h1').text()).toContain('Give a scheduled export its own context')
    expect(page.find('h1 a').attributes('href')).toBe('https://github.com/acme/portal/pull/514')
  })

  it('draws the state word the sidebar draws, from the same function', async () => {
    for (const review of loadSource().reviews) {
      const page = await header(review)
      expect(page.find('[data-testid="state-word"]').text(), `review ${review.review.id}`)
        .toBe(stateWord(stateOfReview(review)))
    }
  })

  /*
   * **The row's word over its own facts** (DEV-1323 round 2). On the sample desk the two
   * agree by construction, so this hands the header a detail whose own facts say *Posted*
   * while the inbox the desk serves still lists #514 under drafts to approve: the header
   * must draw the sidebar's *Draft ready*, and its action with it.
   */
  it('takes the sidebar’s word over the review’s own facts when the sidebar lists it', async () => {
    const detail = fixture(DRAFT_READY)
    detail.draft_status = 'posted'
    detail.review = { ...detail.review, github_review_id: 'PRR_test' }
    expect(stateWord(stateOfReview(detail)), 'its own facts say Posted').toBe('Posted')

    const page = await header(detail)
    expect(page.find('[data-testid="state-word"]').text()).toBe('Draft ready')
    expect(page.find('[data-testid="primary-action"]').text()).toContain('Sign off and post')
  })

  it('draws one action naming its target on a review waiting on the developer', async () => {
    const page = await header(fixture(DRAFT_READY))
    const action = page.findAll('[data-testid="primary-action"]')
    expect(action).toHaveLength(1)
    expect(action[0]!.text()).toContain('Sign off and post')
    expect(action[0]!.attributes('href')).toBe('/reviews/41/draft')
  })

  it('draws no action on any review with nothing for the developer to do', async () => {
    const idle = loadSource().reviews.filter(review =>
      !['brief_ready', 'draft_ready', 'replies_in'].includes(stateOfReview(review)))
    expect(idle.length, 'the fixtures hold such reviews').toBeGreaterThan(0)
    for (const review of idle) {
      const page = await header(review)
      expect(page.find('[data-testid="primary-action"]').exists(), `review ${review.review.id}`)
        .toBe(false)
    }
  })

  it('draws how long the open wait has run, and only when the record holds its start', async () => {
    const measured = fixture(DRAFT_READY)
    /* Ready three hours ago, and an earlier wait of 2 min that ended: the open one is drawn. */
    measured.timings = {
      ...measured.timings,
      draft_ready_at: new Date(Date.now() - 3 * 3600 * 1000).toISOString(),
      waiting_on_you_seconds: 120,
    }
    const drawn = (await header(measured)).find('.facts').text()
    expect(drawn).toContain('3 hr waiting on you')
    expect(drawn).not.toContain('2 min')

    const unmeasured = fixture(DRAFT_READY)
    unmeasured.timings = { ...unmeasured.timings, draft_ready_at: null }
    const facts = (await header(unmeasured)).find('.facts').text()
    expect(facts).not.toContain('waiting on you')
    expect(facts).not.toMatch(/\d+ (sec|min|h|hr|day)/)
  })

  it('keeps the ticket as a way out', async () => {
    const detail = fixture(DRAFT_READY)
    detail.review = { ...detail.review, ticket_id: 'ABC-402', ticket_url: 'https://linear.app/acme/issue/ABC-402' }
    const page = await header(detail)
    const out = page.findAll('a.header-out').map(one => one.attributes('href'))
    expect(out).toContain('https://linear.app/acme/issue/ABC-402')
  })

  it('draws five tabs, marks the one you are on, and counts the findings', async () => {
    const detail = fixture(DRAFT_READY)
    const page = await header(detail, 'findings')
    const tabs = page.findAll('.tab')
    expect(tabs.map(one => one.attributes('data-tab')))
      .toEqual(['overview', 'understanding', 'findings', 'draft', 'replies'])
    expect(page.findAll('[aria-current="page"]').map(one => one.attributes('data-tab')))
      .toEqual(['findings'])
    expect(tabs[2]!.find('.count').text()).toBe(String(detail.findings.length))
  })

  it('counts the replies to answer, and says none with a dash', async () => {
    const none = await header(fixture(DRAFT_READY))
    expect(none.find('[data-tab="replies"] .count').text()).toContain('—')

    const detail = fixture(DRAFT_READY)
    const [first] = detail.findings
    detail.outcomes = [{
      id: 1,
      review_id: detail.review.id,
      finding_seq: first!.seq,
      finding_key: first!.key,
      kind: 'replied',
      actor: 'author',
      occurred_at: '2026-11-01T10:00:00Z',
      detail: null,
    }]
    const one = await header(detail)
    expect(one.find('[data-tab="replies"] .count').text()).toBe('1')
  })
})
