// @vitest-environment nuxt
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

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

const header = (detail: ReviewDetail, tab = 'overview' as const) =>
  mountSuspended(ReviewHeader, { props: { detail, tab } })

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

  it('draws a time only when the record measured it', async () => {
    const measured = fixture(DRAFT_READY)
    expect(measured.timings.waiting_on_you_seconds, 'the fixture measured it').not.toBeNull()
    expect((await header(measured)).find('.facts').text()).toContain('waiting on you')

    const unmeasured = fixture(DRAFT_READY)
    unmeasured.timings = { ...unmeasured.timings, waiting_on_you_seconds: null }
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
    const page = await mountSuspended(ReviewHeader, { props: { detail, tab: 'findings' } })
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
