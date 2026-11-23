<script setup lang="ts">
import { ref } from 'vue'

import { type Column, keyedWidth } from '../utils/columns'

/**
 * **The handle on a side column's inner edge** (DEV-1375): drag it, or focus it and use the
 * arrow keys, and the column is that wide until it is moved again — in this browser, across
 * reloads. A double-click puts it back to its default.
 *
 * **It is a window splitter as WAI-ARIA draws one**: `role="separator"`, focusable, with its
 * value and its limits. Left and Right move it 16px, 64px with Shift, the way the arrow
 * points; Home and End go to the minimum and the maximum. The maximum is the column's own
 * held to what the other side column leaves the main one, so `aria-valuemax` is the furthest
 * a drag will actually go (`utils/columns.ts`, `handleRange`).
 *
 * **Fixed to the window, not placed in the column**, because the sidebar and the preview are
 * scroll containers: a handle inside one would scroll away with its content. The rule a
 * reader sees is the column's own 1px border; the handle is 8px over it, and tints that line
 * the accent while it is pointed at or dragged.
 */
const props = defineProps<{
  column: Column
  /** Which of the column's edges the handle is on: the sidebar's right, or a right-hand column's left. */
  edge: 'left' | 'right'
  /** What the handle resizes, as a screen reader says it: "Resize the sidebar". */
  label: string
  /** The `id` of the column it resizes. */
  controls: string
}>()

const { widthOf, rangeOf, setWidth, commit, reset } = useColumnWidths()

const dragging = ref(false)
let from: { x: number, width: number, pointer: number, moved: boolean } | null = null

function down(event: PointerEvent) {
  if (event.button !== 0) return
  event.preventDefault()
  ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
  from = { x: event.clientX, width: widthOf(props.column), pointer: event.pointerId, moved: false }
  dragging.value = true
  document.documentElement.classList.add('rd-resizing')
}

function move(event: PointerEvent) {
  if (!from || event.pointerId !== from.pointer) return
  const dx = event.clientX - from.x
  if (dx !== 0) from.moved = true
  setWidth(props.column, from.width + (props.edge === 'right' ? dx : -dx), false)
}

/** The drag is over: keep where it ended, if it went anywhere. */
function up() {
  if (!from) return
  const moved = from.moved
  from = null
  dragging.value = false
  document.documentElement.classList.remove('rd-resizing')
  if (moved) commit(props.column)
}

function key(event: KeyboardEvent) {
  if (event.altKey || event.ctrlKey || event.metaKey) return
  const next = keyedWidth(event.key, event.shiftKey, props.edge, widthOf(props.column), rangeOf(props.column))
  if (next == null) return
  event.preventDefault()
  setWidth(props.column, next)
}
</script>

<template>
  <div
    class="column-handle"
    :class="[`column-handle--${column}`, { 'column-handle--dragging': dragging }]"
    role="separator"
    tabindex="0"
    aria-orientation="vertical"
    :aria-label="label"
    :aria-controls="controls"
    :aria-valuenow="widthOf(column)"
    :aria-valuemin="rangeOf(column).min"
    :aria-valuemax="rangeOf(column).max"
    :aria-valuetext="`${widthOf(column)} pixels`"
    :data-column="column"
    @pointerdown="down"
    @pointermove="move"
    @pointerup="up"
    @pointercancel="up"
    @lostpointercapture="up"
    @dblclick="reset(column)"
    @keydown="key"
  />
</template>

<style scoped>
.column-handle {
  position: fixed;
  top: 0;
  bottom: 0;
  z-index: 2;
  width: 8px;
  cursor: col-resize;
  touch-action: none;
}

/* The rule is the column's border, at the handle's fifth pixel; this tints it. */
.column-handle::before {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 4px;
  width: 1px;
  content: '';
}

.column-handle:hover::before,
.column-handle--dragging::before {
  background: var(--rd-accent);
}

.column-handle:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 0;
}

.column-handle:focus-visible::before {
  background: var(--rd-accent);
}

/* The sidebar's border is its last pixel, so its handle starts one further left. */
.column-handle--sidebar {
  left: calc(var(--rd-sidebar-now, var(--rd-sidebar-width)) - 5px);
}

.column-handle--conversation {
  right: calc(var(--rd-conversation-now, var(--rd-conversation-width)) - 4px);
}

.column-handle--preview {
  right: calc(var(--rd-preview-now, var(--rd-conversation-width)) - 4px);
}

/* While a drag is on, the whole window is the handle: no text is selected, the cursor holds. */
:global(html.rd-resizing),
:global(html.rd-resizing *) {
  cursor: col-resize !important;
  user-select: none !important;
}

/* The breakpoints are `utils/columns.ts`'s `BREAKPOINT`, which the layout's grid also keeps. */
@media (max-width: 699px) {
  .column-handle {
    display: none;
  }
}

@media (max-width: 1099px) {
  .column-handle--conversation {
    display: none;
  }
}

@media (max-width: 1199px) {
  .column-handle--preview {
    display: none;
  }
}
</style>
