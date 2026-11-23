import { beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'

import { resetDesk, useReviewDesk } from '../app/composables/useReviewDesk'

/*
 * **The list preview's held reads** — DEV-1374. The preview moves from review to review as
 * fast as the pointer, so `heldReview` draws what the page has already read and `readAhead`
 * reads the open tab's rows before they are visited. These count the reads the sample
 * backend is asked for, because "fewer reads" is the whole claim.
 */

const reads = vi.hoisted(() => ({ ids: [] as number[], slow: null as Promise<void> | null, failing: new Set<number>() }))

vi.mock('../app/data/sample', async (original) => {
  const actual = await original<typeof import('../app/data/sample')>()
  return {
    ...actual,
    sampleBackend: () => {
      const backend = actual.sampleBackend()
      return {
        ...backend,
        getReview: async (id: number) => {
          reads.ids.push(id)
          if (reads.slow) await reads.slow
          if (reads.failing.has(id)) throw new Error('the server is not answering')
          return backend.getReview(id)
        },
      }
    },
  }
})

const WEBHOOKS = 17
const INVOICES = 15
const CACHE = 16

const scope = effectScope()
const desk = () => scope.run(() => useReviewDesk())!

/** Let every queued read answer. */
async function drained() {
  for (let round = 0; round < 20; round += 1) await new Promise(resolve => setTimeout(resolve, 0))
}

beforeEach(async () => {
  await resetDesk()
  reads.ids = []
  reads.slow = null
  reads.failing.clear()
})

describe('a held review', () => {
  it('reads nothing until asked, and draws null rather than another review', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    await drained()
    expect(reads.ids).toEqual([])
    expect(held.data.value).toBeNull()

    await held.refresh()
    expect(held.data.value?.review.id).toBe(WEBHOOKS)

    id.value = INVOICES
    expect(held.data.value, 'not the last review’s answer').toBeNull()
  })

  it('draws a review read before at once when it is asked for again', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    await held.refresh()
    id.value = INVOICES
    await held.refresh()

    id.value = WEBHOOKS
    await nextTick()
    expect(held.data.value?.review.id, 'held, with no read between').toBe(WEBHOOKS)
  })

  it('keeps what it holds drawn while it reads again', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    await held.refresh()

    let answer!: () => void
    reads.slow = new Promise(resolve => (answer = resolve))
    const again = held.refresh()
    expect(held.data.value?.review.id, 'quiet: nothing blanks').toBe(WEBHOOKS)
    answer()
    await again
    expect(reads.ids).toEqual([WEBHOOKS, WEBHOOKS])
  })

  it('is read again by a write, like any read on screen', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    await held.refresh()
    reads.ids = []
    await desk().refreshReads()
    expect(reads.ids).toContain(WEBHOOKS)
  })

  it('says a read failed when nothing is held, and clears it once a read answers', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    reads.failing.add(WEBHOOKS)
    desk().readAhead([WEBHOOKS])
    await drained()
    expect(held.data.value).toBeNull()
    expect(held.error.value, 'a failed read ahead is said').not.toBeNull()

    reads.failing.clear()
    await held.refresh()
    expect(held.error.value).toBeNull()
    expect(held.data.value?.review.id).toBe(WEBHOOKS)
  })
})

describe('reading ahead', () => {
  it('reads three at a time, and skips what is already held', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    await held.refresh()
    reads.ids = []

    let answer!: () => void
    reads.slow = new Promise(resolve => (answer = resolve))
    desk().readAhead([WEBHOOKS, INVOICES, CACHE, 12, 18])
    await drained()
    expect(reads.ids, 'the held one skipped, and three in flight').toEqual([INVOICES, CACHE, 12])

    answer()
    reads.slow = null
    await drained()
    expect(reads.ids).toEqual([INVOICES, CACHE, 12, 18])
  })

  it('stops when it is asked to read nothing', async () => {
    let answer!: () => void
    reads.slow = new Promise(resolve => (answer = resolve))
    desk().readAhead([WEBHOOKS, INVOICES, CACHE, 12, 18])
    await drained()
    desk().readAhead([])
    answer()
    reads.slow = null
    await drained()
    expect(reads.ids).toEqual([WEBHOOKS, INVOICES, CACHE])
  })

  it('reads the selected review once when the preview asks while it is read ahead', async () => {
    /* Self-review round 1: on arrival the selected row was read ahead and then read again. */
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    desk().readAhead([WEBHOOKS, INVOICES])
    void held.refresh()
    await drained()
    expect(reads.ids).toEqual([WEBHOOKS, INVOICES])
  })

  it('joins a read ahead already in flight rather than asking twice', async () => {
    const id = ref<number | null>(WEBHOOKS)
    const held = scope.run(() => desk().heldReview(id))!
    let answer!: () => void
    reads.slow = new Promise(resolve => (answer = resolve))
    desk().readAhead([WEBHOOKS])
    await drained()
    expect(held.data.value).toBeNull()
    answer()
    await drained()
    expect(held.data.value?.review.id).toBe(WEBHOOKS)
    expect(reads.ids).toEqual([WEBHOOKS])
  })
})
