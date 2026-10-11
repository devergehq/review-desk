/*
 * The arithmetic of a drawing opened larger: how far it may be zoomed, and how far it may
 * be dragged before it is lost.
 *
 * **Out of the component because the component cannot be tested.** jsdom has no `getBBox`,
 * so a valid diagram's render always throws there and `MermaidFigure`'s enlarged view is
 * unreachable from the unit suite — `test/mermaid-figure.nuxt.spec.ts` says so at the top,
 * and that is a fact about jsdom rather than a gap in the tests. What *can* be tested is the
 * two rules that decide whether the view is usable: a scale that cannot run away, and a pan
 * that cannot put the drawing off the screen with no way back. So they are functions, and
 * `test/zoom.spec.ts` holds them.
 *
 * Nothing here reads an element, an event or a component. Numbers in, numbers out.
 */

/**
 * How small and how large, and why these two numbers.
 *
 * **Down to half**, because a diagram already fits the window when the view opens — the
 * stage scales it down to fit — so zooming out is for seeing a very wide flow whole rather
 * than for making a small one smaller. **Up to eight times**, which is where a 14px label in
 * mermaid's own type size becomes comfortably readable on a 1440-pixel screen, and is the
 * point of opening it larger at all.
 */
export const SCALE_MIN = 0.5
export const SCALE_MAX = 8

/**
 * One press of a zoom control, as a multiplier.
 *
 * Multiplicative, not additive: a step of +0.25 is a quarter of the drawing at 1× and a
 * thirty-second of it at 8×, so an additive step makes the control feel broken at one end of
 * its range. 1.3 reaches 8× in eight presses from 1× and 0.5× in three, which is the range
 * crossed in about the number of presses a reader will tolerate.
 */
export const SCALE_STEP = 1.3

/**
 * A number held between two others.
 *
 * **Negative zero is normalised away.** `Math.min(Math.max(-9999, -0), 0)` is `-0`, which
 * compares unequal to `0` under `Object.is` — so a pan clamped back to the middle was a pan
 * no test could say was the middle, and a label built from one would read "-0%". Found by
 * `test/zoom.spec.ts` on the first run.
 */
export function clamp(value: number, low: number, high: number): number {
  if (Number.isNaN(value)) return low
  const held = Math.min(Math.max(value, low), high)
  return held === 0 ? 0 : held
}

/** A scale held inside the range above. */
export function clampScale(scale: number): number {
  return clamp(scale, SCALE_MIN, SCALE_MAX)
}

/**
 * Zoomed by a factor, and clamped.
 *
 * A factor of zero or less is the identity rather than a collapse: a wheel event with no
 * delta, or a caller passing a nought, should leave the view where it is and not multiply it
 * to nothing.
 */
export function scaleBy(scale: number, factor: number): number {
  if (!(factor > 0) || Number.isNaN(scale)) return clampScale(scale)
  return clampScale(scale * factor)
}

/**
 * The factor one wheel notch is worth.
 *
 * Away from the reader — a negative `deltaY`, which every browser reports for a scroll up —
 * zooms in. The magnitude is ignored on purpose: a trackpad reports tens of small deltas
 * where a mouse reports one of 100, and honouring the magnitude makes a trackpad fly to 8×
 * in one flick.
 */
export function wheelFactor(deltaY: number): number {
  if (deltaY === 0 || Number.isNaN(deltaY)) return 1
  return deltaY < 0 ? SCALE_STEP : 1 / SCALE_STEP
}

export interface Size {
  width: number
  height: number
}

export interface Pan {
  x: number
  y: number
}

export const NO_PAN: Pan = { x: 0, y: 0 }

/**
 * How far the drawing may be dragged along one axis.
 *
 * **Half of whatever it has that the stage has not.** Centred, a drawing wider than the
 * stage hangs over by half the difference at each edge, so that difference halved is exactly
 * the distance that brings one edge flush with the stage's. Dragging further would be
 * dragging empty space into view, and the far edge is reachable in the other direction.
 *
 * A drawing that fits has a limit of nought — it is wholly visible, and there is nothing to
 * drag it towards. That is the rule that stops a reader shoving a small diagram off the side
 * of the window and being left looking at the backdrop.
 */
export function panLimit(drawn: number, stage: number, scale: number): number {
  const over = drawn * scale - stage
  return over > 0 ? over / 2 : 0
}

/** A pan held inside those limits, on both axes. */
export function clampPan(pan: Pan, drawn: Size, stage: Size, scale: number): Pan {
  const x = panLimit(drawn.width, stage.width, scale)
  const y = panLimit(drawn.height, stage.height, scale)
  return { x: clamp(pan.x, -x, x), y: clamp(pan.y, -y, y) }
}

/**
 * How far one arrow key moves the drawing.
 *
 * **The keyboard gets the same drag the pointer does**, because a view that can only be
 * panned by dragging is a view a keyboard reader can open, zoom and then not read. 48 pixels
 * is about a line of mermaid's own type at 1×, so a press moves the view by a readable
 * amount rather than a pixel or a page.
 */
export const PAN_STEP = 48

/** Where the arrow keys point. Anything else moves nothing. */
export function panByKey(key: string): Pan | null {
  if (key === 'ArrowLeft') return { x: PAN_STEP, y: 0 }
  if (key === 'ArrowRight') return { x: -PAN_STEP, y: 0 }
  if (key === 'ArrowUp') return { x: 0, y: PAN_STEP }
  if (key === 'ArrowDown') return { x: 0, y: -PAN_STEP }
  return null
}

/**
 * The scale as the control reports it — "240%".
 *
 * Rounded to whole percentages. A reader is being told roughly how far in they are, and
 * "237.3%" is a measurement of the arithmetic rather than of the diagram.
 */
export function scaleLabel(scale: number): string {
  return `${Math.round(clampScale(scale) * 100)}%`
}
