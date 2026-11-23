import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { describe, expect, it } from 'vitest'

import {
  DEFAULT_WIDTH,
  MAIN_MIN,
  fit,
  handleRange,
  isReviewPath,
  keyedWidth,
  limits,
  readStored,
  rightColumnFor,
} from '../app/utils/columns'

/*
 * **The side columns' widths** (DEV-1375): each column's limits, the main column's 480px
 * held at every width the handles are drawn, what a stored width may be, and where a key
 * moves a handle. The drag itself, the reload and blocked storage are `e2e/shell.spec.ts`'s,
 * because only a browser has a pointer and a `localStorage` to block.
 */

describe('each column’s own limits', () => {
  it('holds the sidebar between 180 and 480 at any width', () => {
    expect(limits('sidebar', 1100)).toEqual({ min: 180, max: 480 })
    expect(limits('sidebar', 2560)).toEqual({ min: 180, max: 480 })
  })

  it('holds the conversation to half the window, or 720 when that is smaller', () => {
    expect(limits('conversation', 1100)).toEqual({ min: 300, max: 550 })
    expect(limits('conversation', 1440)).toEqual({ min: 300, max: 720 })
    expect(limits('conversation', 2560)).toEqual({ min: 300, max: 720 })
  })

  it('gives the preview the conversation’s limits', () => {
    for (const width of [1200, 1300, 1920]) {
      expect(limits('preview', width)).toEqual(limits('conversation', width))
    }
  })
})

describe('the main column keeps 480px', () => {
  it('at 1,100 with both side columns set to their maximum', () => {
    const drawn = fit(1100, { sidebar: 480, right: { column: 'conversation', width: 720 } })
    expect(1100 - drawn.sidebar - drawn.right!).toBeGreaterThanOrEqual(MAIN_MIN)
    /* The conversation gives way first, to its minimum, and the sidebar takes the rest. */
    expect(drawn).toEqual({ sidebar: 320, right: 300 })
  })

  it('at every width a handle is drawn, whatever was set', () => {
    for (let width = 700; width <= 2600; width += 50) {
      const right = rightColumnFor({ review: true, split: false }, width)
      const drawn = fit(width, { sidebar: 480, right: right ? { column: right, width: 720 } : null })
      expect(width - drawn.sidebar - (drawn.right ?? 0), `at ${width}`).toBeGreaterThanOrEqual(MAIN_MIN)
    }
  })

  it('leaves what was set alone when there is room for it', () => {
    expect(fit(1920, { sidebar: 300, right: { column: 'conversation', width: 500 } }))
      .toEqual({ sidebar: 300, right: 500 })
    expect(fit(1920, { sidebar: 232, right: null })).toEqual({ sidebar: 232, right: null })
  })

  it('draws the defaults unchanged at 1,280', () => {
    expect(fit(1280, { sidebar: 232, right: { column: 'conversation', width: 380 } }))
      .toEqual({ sidebar: 232, right: 380 })
  })

  it('gives a drag only the room the other column leaves, so one never pushes the other', () => {
    const drawn = fit(1400, { sidebar: 232, right: { column: 'conversation', width: 600 } })
    expect(drawn).toEqual({ sidebar: 232, right: 600 })
    /* 1,400 − 480 − 600 = 320: the sidebar's handle stops there, not at 480. */
    expect(handleRange('sidebar', 1400, drawn)).toEqual({ min: 180, max: 320 })
    /* And the conversation's at 1,400 − 480 − 232 = 688, under its own 700. */
    expect(handleRange('conversation', 1400, drawn)).toEqual({ min: 300, max: 688 })
  })

  it('lets the sidebar alone reach 480 with no right-hand column beside it', () => {
    expect(handleRange('sidebar', 1920, fit(1920, { sidebar: 232, right: null })))
      .toEqual({ min: 180, max: 480 })
  })
})

describe('which right-hand column a page draws', () => {
  it('draws the conversation beside a review from 1,100', () => {
    expect(rightColumnFor({ review: true, split: false }, 1100)).toBe('conversation')
    expect(rightColumnFor({ review: true, split: false }, 1099)).toBeNull()
  })

  it('draws the preview beside the list from 1,200', () => {
    expect(rightColumnFor({ review: false, split: true }, 1200)).toBe('preview')
    expect(rightColumnFor({ review: false, split: true }, 1199)).toBeNull()
  })

  it('draws neither on Rules', () => {
    expect(rightColumnFor({ review: false, split: false }, 2560)).toBeNull()
  })

  it('knows a review’s pages by their path', () => {
    expect(isReviewPath('/reviews/12')).toBe(true)
    expect(isReviewPath('/reviews/12/findings')).toBe(true)
    expect(isReviewPath('/reviews')).toBe(false)
    expect(isReviewPath('/rules')).toBe(false)
  })
})

describe('a stored width', () => {
  it('is read back when it is one the column could be set to', () => {
    expect(readStored('sidebar', '300')).toBe(300)
    expect(readStored('conversation', '612.4')).toBe(612)
  })

  it('is the default when it is absent, empty, garbage or out of range', () => {
    for (const raw of [null, undefined, '', '  ', 'wide', 'NaN', 'Infinity', '100', '9000']) {
      expect(readStored('sidebar', raw), String(raw)).toBeNull()
    }
    expect(readStored('conversation', '721')).toBeNull()
  })
})

describe('a handle’s keys', () => {
  const range = { min: 180, max: 480 }

  it('moves the sidebar’s handle 16px the way the arrow points, 64 with Shift', () => {
    expect(keyedWidth('ArrowRight', false, 'right', 232, range)).toBe(248)
    expect(keyedWidth('ArrowLeft', false, 'right', 232, range)).toBe(216)
    expect(keyedWidth('ArrowRight', true, 'right', 232, range)).toBe(296)
  })

  it('widens a right-hand column when its handle moves left', () => {
    const its = { min: 300, max: 720 }
    expect(keyedWidth('ArrowLeft', false, 'left', 380, its)).toBe(396)
    expect(keyedWidth('ArrowRight', true, 'left', 380, its)).toBe(316)
  })

  it('goes to the minimum on Home and the maximum on End', () => {
    expect(keyedWidth('Home', false, 'right', 232, range)).toBe(180)
    expect(keyedWidth('End', false, 'right', 232, range)).toBe(480)
  })

  it('stops at the limits, and leaves every other key to the page', () => {
    expect(keyedWidth('ArrowLeft', true, 'right', 200, range)).toBe(180)
    expect(keyedWidth('ArrowRight', true, 'right', 470, range)).toBe(480)
    expect(keyedWidth('j', false, 'right', 232, range)).toBeNull()
    expect(keyedWidth('ArrowUp', false, 'right', 232, range)).toBeNull()
  })
})

describe('the defaults', () => {
  it('are the stylesheet’s, so what draws before a width is read is what a reset gives', () => {
    const css = readFileSync(resolve(__dirname, '../app/assets/css/tokens.css'), 'utf8')
    const token = (name: string) => Number(new RegExp(`${name}:\\s*(\\d+)px`).exec(css)![1])
    expect(token('--rd-sidebar-width')).toBe(DEFAULT_WIDTH.sidebar)
    expect(token('--rd-conversation-width')).toBe(DEFAULT_WIDTH.conversation)
    expect(DEFAULT_WIDTH.preview).toBe(DEFAULT_WIDTH.conversation)
  })
})
