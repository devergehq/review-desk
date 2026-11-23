import { computed, reactive } from 'vue'

import {
  type Column,
  DEFAULT_WIDTH,
  fit,
  handleRange,
  isReviewPath,
  readStored,
  rightColumnFor,
} from '../utils/columns'
import { clamp } from '../utils/zoom'

/**
 * **The widths the developer set, and the window they are fitted to** (DEV-1375). One copy
 * for the whole app, because the layout draws the sidebar and the conversation and the Pull
 * requests page draws its preview, and a handle on either has to see the other's width to
 * keep the main column its 480px. The arithmetic is `utils/columns.ts`'s.
 *
 * **Kept per browser**, in `localStorage['review-desk:width:<column>']`, read and written
 * inside `try`, as `data/source.ts` keeps the sample switch: a private window or blocked
 * site data draws the defaults and the handles still move, for this page only.
 */

const KEY = (column: Column) => `review-desk:width:${column}`

const set = reactive<Record<Column, number>>({ ...DEFAULT_WIDTH })

/**
 * The window, twice: `viewport` is what the stylesheet's media queries read, scrollbar and
 * all, and `layout` is what the grid divides. They differ by a classic scrollbar's width.
 */
const view = reactive({ viewport: 0, layout: 0 })

let started = false

function measure() {
  view.viewport = window.innerWidth
  view.layout = document.documentElement.clientWidth || window.innerWidth
}

/** Read what was kept, and follow the window. Once, on first use in a browser. */
function start() {
  if (started || typeof window === 'undefined') return
  started = true
  for (const column of Object.keys(DEFAULT_WIDTH) as Column[]) {
    let raw: string | null = null
    try {
      raw = localStorage.getItem(KEY(column))
    }
    catch {
      /* No storage, or it threw: the default stands. */
    }
    set[column] = readStored(column, raw) ?? DEFAULT_WIDTH[column]
  }
  measure()
  window.addEventListener('resize', measure)
  /*
   * **And when a classic scrollbar comes or goes**, which changes what the grid divides by
   * its width (15px on Windows) and fires no `resize`: a review's content arriving after the
   * layout mounted, or a long page followed by a short one. The root element's box is what
   * the scrollbar narrows, so that is what is watched (#114's self-review, F1).
   */
  if (typeof ResizeObserver !== 'undefined') new ResizeObserver(measure).observe(document.documentElement)
}

function keep(column: Column, width: number | null) {
  try {
    if (width == null) localStorage.removeItem(KEY(column))
    else localStorage.setItem(KEY(column), String(width))
  }
  catch {
    /* Not kept, which is worth nothing to report: the width still holds for this page. */
  }
}

export function useColumnWidths() {
  start()
  const route = useRoute()

  /** The right-hand column beside the main one, at this width on this page. */
  const right = computed(() =>
    rightColumnFor({ review: isReviewPath(route.path), split: route.meta.split === true }, view.viewport))

  const fitted = computed(() => fit(view.layout, {
    sidebar: set.sidebar,
    right: right.value ? { column: right.value, width: set[right.value] } : null,
  }))

  /** The width a column is drawn at now. A right-hand column not beside the main one draws at its set width. */
  function widthOf(column: Column): number {
    if (column === 'sidebar') return fitted.value.sidebar
    return column === right.value ? fitted.value.right! : set[column]
  }

  function rangeOf(column: Column) {
    return handleRange(column, view.layout, fitted.value)
  }

  /** Set a column to a width, held to its handle's range. `keepIt` false while a drag is still moving. */
  function setWidth(column: Column, width: number, keepIt = true) {
    const range = rangeOf(column)
    set[column] = Math.round(clamp(width, range.min, range.max))
    if (keepIt) keep(column, set[column])
  }

  /** Keep what a drag left a column at, once the drag is over. */
  function commit(column: Column) {
    keep(column, set[column])
  }

  /** Back to the default, and nothing kept. */
  function reset(column: Column) {
    set[column] = DEFAULT_WIDTH[column]
    keep(column, null)
  }

  /** The drawn widths, as the custom properties the shell's grid and the preview read. */
  const style = computed(() => ({
    '--rd-sidebar-now': `${widthOf('sidebar')}px`,
    '--rd-conversation-now': `${widthOf('conversation')}px`,
    '--rd-preview-now': `${widthOf('preview')}px`,
  }))

  return { widthOf, rangeOf, setWidth, commit, reset, style }
}
