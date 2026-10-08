<script setup lang="ts">
/**
 * What the desk says back after an action. Announced, because the thing it is reporting
 * on often happens somewhere else on the page.
 */
defineProps<{ tone?: 'plain' | 'problem' }>()
</script>

<template>
  <p
    class="notice"
    :class="`notice--${tone ?? 'plain'}`"
    role="status"
  >
    <slot />
  </p>
</template>

<style scoped>
.notice {
  margin: 0;
  font-size: 13px;
}

.notice--plain {
  color: var(--rd-ink-dim);
}

.notice--problem {
  color: var(--rd-warn-ink);
  font-weight: 500;
}
</style>
