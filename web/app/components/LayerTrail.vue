<script setup lang="ts">
import type { ReviewLayer } from '~/types/desk'

/** The seven layers of a review, and which one is waiting. */
defineProps<{ layers: ReviewLayer[] }>()
</script>

<template>
  <ol
    class="trail"
    aria-label="The seven layers of this review"
  >
    <li
      v-for="item in layers"
      :key="item.number"
      class="layer"
      :class="`layer--${layerTone(item.state)}`"
    >
      <span class="rd-mono number">{{ item.number }}</span>
      <span>{{ layerLabel(item.name) }}</span>
      <span v-if="item.detail">· {{ layerDetail(item.detail) }}</span>
    </li>
  </ol>
</template>

<style scoped>
.trail {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 13px;
}

.layer {
  display: flex;
  gap: 4px;
  padding: 6px 10px;
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
  border: 1px solid var(--rd-line);
  color: var(--rd-ink-dim);
}

.layer--current {
  background: var(--rd-ink);
  border-color: var(--rd-ink);
  color: var(--rd-surface);
  font-weight: 500;
}

.layer--pending {
  background: transparent;
  border-style: dashed;
  border-color: var(--rd-line-strong);
  color: var(--rd-ink-muted);
}

.layer--failed {
  background: var(--rd-warn-wash);
  border-color: var(--rd-warn-ink);
  color: var(--rd-warn-ink);
  font-weight: 500;
}

.number {
  font-weight: 500;
}
</style>
