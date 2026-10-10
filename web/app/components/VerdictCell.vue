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
</script>

<template>
  <span class="cell">
    <span
      class="word"
      :class="`word--${kind}`"
    >
      <span
        class="shape"
        aria-hidden="true"
      />
      {{ says }}
    </span>
    <!--
      The one line of why, which is non-null for every verdict but `not_assessed` — the one
      that is the absence of a judgement rather than one.
    -->
    <span
      v-if="verdict?.why"
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
