<script setup lang="ts">
/**
 * One of the figures across the top of the Rules board: the number, what it counts, and
 * the period it counts over.
 *
 * ```
 * ─────────────────────────
 * 31 of 40
 * findings you kept
 * last 30 days, draft to posted
 * ```
 *
 * A 1px ink rule over it, the figure in Newsreader at 40px, the label in medium weight,
 * the note in grey. No card and no border: the boards put these four straight on the page,
 * told apart by the rule above each one.
 *
 * **`value` is `null` when nothing has measured it**, and then the figure is the words
 * "Not measured yet" rather than a nought — which is the whole reason this is a component
 * and not four copies of a `<div>`. Three of the four figures on this board have no source
 * in the API today, so the unmeasured state is the common case rather than the edge.
 *
 * `tone: 'warn'` is the board's one coloured figure: reviews that went in a circle, in the
 * brown it uses for something that wants looking at.
 */
withDefaults(
  defineProps<{
    /** Already worded — "31 of 40". `null` means nothing measured it. */
    value: string | null
    label: string
    /** The period, or the caveat. Shown under the label in either state. */
    note: string
    /** Why there is no figure. Required whenever `value` is null. */
    unknownBecause?: string
    tone?: 'plain' | 'warn'
  }>(),
  { unknownBecause: 'nothing has measured it yet', tone: 'plain' },
)
</script>

<template>
  <div class="figure">
    <div
      class="value"
      :class="{ 'value--warn': tone === 'warn' && value != null }"
    >
      <template v-if="value != null">
        {{ value }}
      </template>
      <NotMeasured
        v-else
        :because="unknownBecause"
      />
    </div>
    <div class="label">
      {{ label }}
    </div>
    <div class="rd-meta">
      {{ note }}
    </div>
  </div>
</template>

<style scoped>
.figure {
  padding: 14px 0 6px;
  border-top: 1px solid var(--rd-ink);
  min-width: 0;
}

.value {
  font-family: var(--rd-font-display);
  font-weight: 400;
  font-size: 40px;
  line-height: 1.05;
  font-variant-numeric: proportional-nums;
}

.value--warn {
  color: var(--rd-warn-ink);
}

/*
 * The unmeasured words are not a 40px number. They are a sentence where a number would
 * have been, so they are set at the label's size and the figure's height is kept by the
 * line box above.
 */
.value :deep(.unknown) {
  font-size: 20px;
}

.label {
  font-weight: 600;
}
</style>
