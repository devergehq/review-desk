import { describe, expect, it } from 'vitest'

import {
  NO_PAN,
  PAN_STEP,
  SCALE_MAX,
  SCALE_MIN,
  SCALE_STEP,
  clamp,
  clampPan,
  clampScale,
  panByKey,
  panLimit,
  scaleBy,
  scaleLabel,
  wheelFactor,
} from '../app/utils/zoom'

/*
 * The arithmetic of a drawing opened larger.
 *
 * **This file exists because `test/mermaid-figure.nuxt.spec.ts` cannot do this.** jsdom has
 * no `getBBox`, so a valid diagram's render always throws there and the enlarged view is
 * never reachable — which means the two rules that decide whether the view is usable at all
 * would otherwise be measured only in the browser, once, at the end. A scale that runs away
 * and a pan that puts the drawing off the screen with no way back are both silent failures
 * on a screenshot.
 */

describe('a number held between two others', () => {
  it('is itself inside the range', () => {
    expect(clamp(3, 1, 5)).toBe(3)
  })

  it('is the nearer edge outside it', () => {
    expect(clamp(0, 1, 5)).toBe(1)
    expect(clamp(9, 1, 5)).toBe(5)
  })

  /* A measurement that is not a number is a measurement that has not been taken. */
  it('is the low edge for something that is not a number', () => {
    expect(clamp(Number.NaN, 1, 5)).toBe(1)
  })
})

describe('how far the view may be zoomed', () => {
  it('starts at the size the drawing already is', () => {
    expect(clampScale(1)).toBe(1)
  })

  it('will not go below half or above eight times', () => {
    expect(clampScale(0.01)).toBe(SCALE_MIN)
    expect(clampScale(99)).toBe(SCALE_MAX)
    expect(SCALE_MIN).toBe(0.5)
    expect(SCALE_MAX).toBe(8)
  })

  it('multiplies rather than adds, so a step feels the same at both ends', () => {
    expect(scaleBy(1, SCALE_STEP)).toBeCloseTo(1.3)
    expect(scaleBy(4, SCALE_STEP)).toBeCloseTo(5.2)
  })

  it('reaches both ends in a handful of presses', () => {
    let scale = 1
    let presses = 0
    while (scale < SCALE_MAX && presses < 50) {
      scale = scaleBy(scale, SCALE_STEP)
      presses += 1
    }
    expect(presses).toBeLessThanOrEqual(10)
    expect(scale).toBe(SCALE_MAX)
  })

  it('clamps rather than running away', () => {
    expect(scaleBy(SCALE_MAX, SCALE_STEP)).toBe(SCALE_MAX)
    expect(scaleBy(SCALE_MIN, 1 / SCALE_STEP)).toBe(SCALE_MIN)
  })

  /* A wheel event with no delta, or a caller passing a nought, leaves the view alone. */
  it('leaves the scale where it is for a factor of nought or less', () => {
    expect(scaleBy(2, 0)).toBe(2)
    expect(scaleBy(2, -1)).toBe(2)
  })
})

describe('what one wheel notch is worth', () => {
  it('zooms in away from the reader and out towards them', () => {
    expect(wheelFactor(-100)).toBe(SCALE_STEP)
    expect(wheelFactor(100)).toBeCloseTo(1 / SCALE_STEP)
  })

  /*
   * The magnitude is ignored: a trackpad reports tens of small deltas where a mouse reports
   * one of 100, and honouring it makes a trackpad fly to eight times in a flick.
   */
  it('is one notch whatever the delta’s size', () => {
    expect(wheelFactor(-4)).toBe(wheelFactor(-400))
  })

  it('is the identity for no movement at all', () => {
    expect(wheelFactor(0)).toBe(1)
  })
})

describe('how far the drawing may be dragged', () => {
  /* Half of whatever it has that the stage has not: one edge comes flush, no further. */
  it('is half of what it overhangs the stage by', () => {
    expect(panLimit(1000, 600, 1)).toBe(200)
    expect(panLimit(600, 600, 2)).toBe(300)
  })

  /*
   * The rule that stops a reader shoving a small diagram off the side of the window and
   * being left looking at the backdrop.
   */
  it('is nothing at all for a drawing that already fits', () => {
    expect(panLimit(400, 600, 1)).toBe(0)
    expect(panLimit(400, 600, 0.5)).toBe(0)
  })

  it('holds a drag inside those limits on both axes', () => {
    const drawn = { width: 1000, height: 800 }
    const stage = { width: 600, height: 400 }
    expect(clampPan({ x: -9999, y: 9999 }, drawn, stage, 1)).toEqual({ x: -200, y: 200 })
    expect(clampPan({ x: -50, y: 20 }, drawn, stage, 1)).toEqual({ x: -50, y: 20 })
  })

  it('pulls a drawing that fits back to the middle', () => {
    const fits = { width: 200, height: 200 }
    const stage = { width: 600, height: 400 }
    expect(clampPan({ x: 120, y: -80 }, fits, stage, 1)).toEqual(NO_PAN)
  })

  it('allows more drag the further in the view is zoomed', () => {
    const drawn = { width: 600, height: 400 }
    const stage = { width: 600, height: 400 }
    expect(clampPan({ x: -9999, y: 0 }, drawn, stage, 1).x).toBe(0)
    expect(clampPan({ x: -9999, y: 0 }, drawn, stage, 3).x).toBe(-600)
  })
})

describe('the keyboard’s own drag', () => {
  /*
   * A view that can only be panned by dragging is a view a keyboard reader can open, zoom
   * and then not read.
   */
  it('moves the drawing the other way to the arrow, so the view goes that way', () => {
    expect(panByKey('ArrowRight')).toEqual({ x: -PAN_STEP, y: 0 })
    expect(panByKey('ArrowLeft')).toEqual({ x: PAN_STEP, y: 0 })
    expect(panByKey('ArrowDown')).toEqual({ x: 0, y: -PAN_STEP })
    expect(panByKey('ArrowUp')).toEqual({ x: 0, y: PAN_STEP })
  })

  it('moves nothing for any other key', () => {
    expect(panByKey('Escape')).toBeNull()
    expect(panByKey('a')).toBeNull()
    expect(panByKey('Enter')).toBeNull()
  })
})

describe('the scale as the control reports it', () => {
  it('is whole percentages', () => {
    expect(scaleLabel(1)).toBe('100%')
    expect(scaleLabel(2.373)).toBe('237%')
    expect(scaleLabel(0.5)).toBe('50%')
  })

  it('reports a scale it would not allow as the one it would', () => {
    expect(scaleLabel(99)).toBe('800%')
  })
})
