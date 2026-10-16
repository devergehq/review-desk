<script setup lang="ts">
import type { BriefOptionVerdict } from '~/types/desk'

/**
 * What one option does about one numbered problem.
 *
 * Four values, and **`not_assessed` is one of them.** DEV-885's comment is explicit: it is
 * "a stored verdict, not a missing one. Draw it from the value. A missing verdict is a
 * different state and should not look the same." The board this comes from has an option
 * the blind pass never came up with, so nothing scored it — and "nobody judged this" is a
 * different statement from "the brief forgot to judge this".
 *
 * | Value | Word | Shape | Colour |
 * |---|---|---|---|
 * | `fixed` | Fixed | a filled disc | the accent |
 * | `partly` | Partly | a half-filled disc | the accent |
 * | `stays` | Stays | an outlined ring | grey |
 * | `not_assessed` | Not assessed | a dash | grey, lighter |
 * | *absent* | Not recorded | a dashed outline | grey |
 *
 * Each has a shape as well as a colour, for the same reason `SeverityTag` does: a wash is a
 * hue, and two of these would be a problem for the ~8% of men with a colour vision
 * deficiency. The word is written out in every case, so neither channel carries it alone.
 */
const props = defineProps<{
  /** The verdict, or `null` when the brief recorded none against this problem. */
  verdict: BriefOptionVerdict | null
  /**
   * The shape alone, with the word read out rather than drawn.
   *
   * **What the case page's matrix needs, and why the word is still in the document.** Nine
   * options against four problems is thirty-six cells on one page; the words made each
   * column as wide as "Not assessed" and the matrix then took the width the page has.
   * Drawn as a shape the row is a line a reader scans, and the legend under the matrix says
   * what each shape is. The word stays in the markup, visually hidden, so a screen reader
   * reads "Fixed" and nothing depends on a reader distinguishing four shapes.
   *
   * The `why` is dropped from the cell in this mode and drawn inside the row's own detail,
   * where there is a column for it — a line of prose in a cell 90 pixels wide is the defect
   * this mode exists to undo.
   */
  compact?: boolean
}>()

const WORDS = {
  fixed: 'Fixed',
  partly: 'Partly',
  stays: 'Stays',
  not_assessed: 'Not assessed',
} as const

const kind = computed(() => props.verdict?.verdict ?? 'absent')
const says = computed(() =>
  props.verdict == null ? 'Not recorded' : WORDS[props.verdict.verdict])

/**
 * What a pointer gets where the word is not drawn: the verdict, and its one line of why.
 *
 * A hover costs no layout, so what the compact cell has no room for is here as well as in
 * the row's detail — the same bargain the step trail's one line strikes with its `title`.
 */
const title = computed(() =>
  props.verdict?.why ? `${says.value} — ${props.verdict.why}` : says.value)
</script>

<template>
  <span
    class="cell"
    :class="{ 'cell--compact': compact }"
  >
    <span
      class="word"
      :class="`word--${kind}`"
      :title="compact ? title : undefined"
    >
      <span
        class="shape"
        aria-hidden="true"
      />
      <span :class="{ 'rd-visually-hidden': compact }">{{ says }}</span>
    </span>
    <!--
      The one line of why, which is non-null for every verdict but `not_assessed` — the one
      that is the absence of a judgement rather than one.
    -->
    <span
      v-if="verdict?.why && !compact"
      class="why"
    >{{ verdict.why }}</span>
  </span>
</template>

<style scoped>
.cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

/*
 * The shape alone, centred in its column. Centred because a row of four of these is read
 * across rather than down, and a disc sitting against the left edge of a 90-pixel column
 * reads as four columns of different widths.
 */
.cell--compact {
  align-items: center;
}

.cell--compact .word {
  gap: 0;
}

.word {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

.shape {
  flex: none;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  box-sizing: border-box;
}

.word--fixed {
  color: var(--rd-accent-dark);
  font-weight: 600;
}

.word--fixed .shape {
  background: var(--rd-accent);
}

.word--partly {
  color: var(--rd-ink);
}

/* Half filled: the disc that is half way to being answered. */
.word--partly .shape {
  border: 1px solid var(--rd-accent);
  background: linear-gradient(90deg, var(--rd-accent) 50%, var(--rd-surface) 50%);
}

.word--stays {
  color: var(--rd-ink-muted);
}

.word--stays .shape {
  width: 10px;
  height: 10px;
  border: 2px solid var(--rd-control-line);
}

.word--not_assessed {
  color: var(--rd-ink-muted);
}

/* A dash, because nobody judged it: not an empty ring, which would read as "stays". */
.word--not_assessed .shape {
  height: 2px;
  border-radius: 0;
  background: var(--rd-control-line);
}

/* A brief that recorded nothing here at all. Dashed, as every absent state is. */
.word--absent {
  color: var(--rd-ink-muted);
  font-style: italic;
}

.word--absent .shape {
  border: 1px dashed var(--rd-control-line);
  background: transparent;
}

.why {
  font-size: 14px;
  color: var(--rd-ink-muted);
  overflow-wrap: break-word;
}
</style>
