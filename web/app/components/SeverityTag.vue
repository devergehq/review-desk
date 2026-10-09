<script setup lang="ts">
import type { Severity } from '~/types/desk'

/**
 * How bad a finding is: **told apart by shape as well as colour**, which is what the
 * Controls board's own rubric says.
 *
 * | Severity | Colour | Shape |
 * |---|---|---|
 * | Blocker | ink, reversed out | a filled square |
 * | Should | the warning wash | a square with one heavy side |
 * | Nit | neutral | an outlined square |
 * | Question | blue wash | an outlined circle |
 *
 * The shape is the point. Four washes are four hues and two of them are a problem for the
 * ~8% of men with a colour vision deficiency; a filled square, a half-weighted square, an
 * outlined square and a circle are four shapes for everybody. The label is written out
 * next to it in every case, so neither channel is carrying this alone.
 */
const props = defineProps<{ severity: Severity }>()

const LABELS: Record<Severity, string> = {
  blocker: 'Blocker',
  should: 'Should',
  nit: 'Nit',
  question: 'Question',
}

const label = computed(() => LABELS[props.severity])
</script>

<template>
  <span
    class="tag"
    :class="`tag--${severity}`"
  >
    <span
      class="mark"
      aria-hidden="true"
    />
    {{ label }}
  </span>
</template>

<style scoped>
.tag {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 5px 11px 5px 9px;
  border-radius: var(--rd-radius-xs);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  white-space: nowrap;
}

.mark {
  width: 8px;
  height: 8px;
  border-radius: var(--rd-radius-dot);
  box-sizing: border-box;
}

.tag--blocker {
  background: var(--rd-ink);
  color: var(--rd-surface);
}

.tag--blocker .mark {
  background: var(--rd-surface);
}

.tag--should {
  background: var(--rd-warn-wash);
  color: var(--rd-warn-ink);
}

/* One heavy side: the square that is half way to being filled in. */
.tag--should .mark {
  border: 2px solid var(--rd-warn-ink);
  border-right-width: 5px;
}

.tag--nit {
  background: var(--rd-nit-wash);
  color: var(--rd-nit-ink);
}

.tag--nit .mark {
  border: 2px solid var(--rd-nit-ink);
}

.tag--question {
  background: var(--rd-question-wash);
  color: var(--rd-question-ink);
}

.tag--question .mark {
  border: 2px solid var(--rd-question-ink);
  border-radius: 50%;
}
</style>
