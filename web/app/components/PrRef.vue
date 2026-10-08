<script setup lang="ts">
/** A pull request number, in mono, the way it is written everywhere else. */
defineProps<{ pr: number }>()
</script>

<template>
  <span class="ref rd-mono">#{{ pr }}</span>
</template>

<style scoped>
.ref {
  color: var(--rd-ink-muted);
  font-weight: 400;
}
</style>
