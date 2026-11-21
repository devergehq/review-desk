<script setup lang="ts">
/**
 * The sidebar's agent line, for the review this page is about.
 *
 * **Its own component so it can be keyed on the review**: `session(id)` is a shared
 * resource taken by id, and the review's own strip holds the same one — so on a review's
 * pages this is no second read, and it moves with the strip's own timer. Moving to another
 * review remounts it against that review's reading. `agentLine` in `utils/sidebar.ts` has
 * the rules.
 */
const props = defineProps<{ reviewId: number }>()

const session = useReviewDesk().session(props.reviewId)
const line = computed(() => agentLine(session.data.value?.reading))
</script>

<template>
  <p
    v-if="line"
    class="status-line"
  >
    <span
      class="dot"
      :class="`dot--${line.tone}`"
      aria-hidden="true"
    />
    {{ line.text }}
  </p>
</template>

<style scoped>
.status-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  color: var(--rd-ink-dim);
  font-size: var(--rd-type-ui);
}

.dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
}

.dot--on {
  background: var(--rd-accent);
}

.dot--off {
  background: var(--rd-ink-faint);
}
</style>
