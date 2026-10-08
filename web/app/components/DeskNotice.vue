<script setup lang="ts">
/**
 * What the desk says back after an action. Announced, because the thing it reports on
 * often happens elsewhere on the page.
 *
 * Three tones, because "absent is silent, broken is loud" and the two are different
 * sentences. `problem` is a request the desk understood and refused — the developer
 * changes what they typed. `blocked` is something the desk needed and could not get,
 * which is the machine rather than the request, so it is boxed and names the remedy.
 */
defineProps<{ tone?: 'plain' | 'problem' | 'blocked' }>()
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

.notice--blocked {
  color: var(--rd-warn-ink);
  background: var(--rd-warn-wash);
  border-radius: var(--rd-radius-sm);
  padding: 10px 12px;
  font-weight: 500;
}
</style>
