/**
 * **How wide the shell's side columns are** (DEV-1375): the sidebar, the conversation on a
 * review's pages, and the Pull requests preview. Each has a drag handle on its inner edge;
 * this file is the arithmetic the handles and the layout share, and holds nothing that
 * touches the DOM or storage, so every rule in it is asserted in `web/test/columns.spec.ts`.
 *
 * Patrick, 23 November 2026: *"there should be a handle there where I can move it. Likewise,
 * on the other side, too."*
 *
 * | Column | Default | Minimum | Maximum |
 * | -- | -- | -- | -- |
 * | Sidebar | 232px | 180px | 480px |
 * | Conversation | 380px | 300px | half the window, or 720px if that is smaller |
 * | Preview | 380px | 300px | the conversation's |
 *
 * **The defaults are also `tokens.css`'s `--rd-sidebar-width` and `--rd-conversation-width`**,
 * which is what draws before a stored width is read and what the phone's sidebar keeps; a
 * test reads the stylesheet and holds the two copies equal.
 *
 * ## The main column keeps 480px
 *
 * **What a column is set to and what it is drawn at are two numbers.** The set width is the
 * developer's and is kept; the drawn width is the set width fitted to the window. When the
 * window is too narrow for both side columns and 480px between them, **the right-hand column
 * gives way first**, down to its minimum, and then the sidebar: the sidebar is on every page
 * and the right-hand column on some, so the one that moves is the one that is not always
 * there. Widen the window again and both come back to what was set.
 *
 * **A drag is held to the room the other column takes now**, so dragging one column never
 * pushes the other narrower: the handle stops where the main column would go under 480.
 */

import { clamp } from './zoom'

/** The three columns a handle can move. */
export type Column = 'sidebar' | 'conversation' | 'preview'

/** The two that sit at the right-hand edge; a page draws at most one of them. */
export type RightColumn = Exclude<Column, 'sidebar'>

/** The least the main column is ever drawn at, between the side columns. */
export const MAIN_MIN = 480

/** What each column draws at until it is dragged, and again after a double-click. */
export const DEFAULT_WIDTH: Record<Column, number> = {
  sidebar: 232,
  conversation: 380,
  preview: 380,
}

/** One handle step: an arrow key moves it this far, and with Shift held, `BIG_STEP`. */
export const STEP = 16
export const BIG_STEP = 64

/** The widths at which the shell draws each column beside the main one, as its CSS does. */
export const BREAKPOINT = {
  /** Under this the shell is one column, and no handle is drawn. */
  sidebar: 700,
  /** Under this the conversation sits under the page (`default.vue`). */
  conversation: 1100,
  /** Under this the preview is not drawn (`pages/index.vue`). */
  preview: 1200,
} as const satisfies Record<Column, number>

/** A column's own limits at a window this wide, before the main column's 480 is counted. */
export function limits(column: Column, windowWidth: number): { min: number, max: number } {
  if (column === 'sidebar') return { min: 180, max: 480 }
  const min = 300
  return { min, max: Math.max(min, Math.min(720, Math.floor(windowWidth / 2))) }
}

/**
 * A stored width, or `null` when there is nothing usable: absent, not a number, or outside
 * anything the column could ever be set to. `null` means the default.
 */
export function readStored(column: Column, raw: string | null | undefined): number | null {
  if (raw == null || raw.trim() === '') return null
  const value = Number(raw)
  if (!Number.isFinite(value)) return null
  const ever = limits(column, Number.POSITIVE_INFINITY)
  if (value < ever.min || value > ever.max) return null
  return Math.round(value)
}

/** Which right-hand column the page draws beside the main one at this width, if any. */
export function rightColumnFor(page: { review: boolean, split: boolean }, windowWidth: number): RightColumn | null {
  if (page.review) return windowWidth >= BREAKPOINT.conversation ? 'conversation' : null
  if (page.split) return windowWidth >= BREAKPOINT.preview ? 'preview' : null
  return null
}

/** The widths a page is drawn at. `right` is `null` when no right-hand column is beside the main one. */
export interface Fitted {
  sidebar: number
  right: number | null
}

/**
 * The set widths fitted to the window: each within its own limits, and the two together
 * leaving the main column `MAIN_MIN`, the right-hand one giving way first.
 */
export function fit(
  windowWidth: number,
  set: { sidebar: number, right: { column: RightColumn, width: number } | null },
): Fitted {
  const room = windowWidth - MAIN_MIN
  const own = limits('sidebar', windowWidth)
  let sidebar = clamp(set.sidebar, own.min, own.max)
  let right: number | null = null
  if (set.right) {
    const its = limits(set.right.column, windowWidth)
    right = clamp(set.right.width, its.min, its.max)
    right = Math.max(its.min, Math.min(right, room - sidebar))
  }
  sidebar = Math.max(own.min, Math.min(sidebar, room - (right ?? 0)))
  return { sidebar, right }
}

/**
 * How far a handle may go, for its `aria-valuemin` and `aria-valuemax` and for a drag:
 * its column's own limits, with the maximum held to what the other column leaves.
 */
export function handleRange(column: Column, windowWidth: number, fitted: Fitted): { min: number, max: number } {
  const own = limits(column, windowWidth)
  const other = column === 'sidebar' ? (fitted.right ?? 0) : fitted.sidebar
  return { min: own.min, max: Math.max(own.min, Math.min(own.max, windowWidth - MAIN_MIN - other)) }
}

/** Where a key moves a handle to, or `null` for a key the handle does not take. */
export function keyedWidth(
  key: string,
  shift: boolean,
  edge: 'left' | 'right',
  now: number,
  range: { min: number, max: number },
): number | null {
  const step = shift ? BIG_STEP : STEP
  /* The handle moves the way the arrow points: right widens a column whose edge is on its right. */
  const wider = edge === 'right' ? 'ArrowRight' : 'ArrowLeft'
  const narrower = edge === 'right' ? 'ArrowLeft' : 'ArrowRight'
  if (key === wider) return clamp(now + step, range.min, range.max)
  if (key === narrower) return clamp(now - step, range.min, range.max)
  if (key === 'Home') return range.min
  if (key === 'End') return range.max
  return null
}

/** A review's pages, which are the ones the conversation sits beside. */
export function isReviewPath(path: string): boolean {
  return /^\/reviews\/\d+(?:\/|$)/.test(path)
}
