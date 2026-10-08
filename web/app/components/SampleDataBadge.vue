<script setup lang="ts">
/** Says, on every screen, that none of these records are real. */
const { isSampleData } = useReviewDesk()
</script>

<template>
  <span
    v-if="isSampleData"
    class="badge"
  >Sample data</span>
</template>

<style scoped>
.badge {
  font-size: 12px;
  color: var(--rd-ink-muted);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-xs);
  padding: 2px 8px;
  white-space: nowrap;
}
</style>
