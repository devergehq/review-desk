<script setup lang="ts">
/**
 * One waiting thing: what it is, one line about it, and the one action it offers.
 * The inbox and the stale rules are both lists of these.
 */
defineProps<{ tone?: 'plain' | 'chosen' | 'pending' }>()
</script>

<template>
  <DeskCard
    :tone="tone"
    pad="tight"
    class="row"
  >
    <div class="body">
      <div class="title">
        <slot name="title" />
      </div>
      <div
        v-if="$slots.meta"
        class="rd-meta meta"
      >
        <slot name="meta" />
      </div>
      <div
        v-if="$slots.extra"
        class="extra"
      >
        <slot name="extra" />
      </div>
    </div>
    <div class="actions">
      <slot name="actions" />
    </div>
  </DeskCard>
</template>

<style scoped>
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
}

.body {
  flex: 1 1 320px;
  min-width: 0;
}

.title {
  font-weight: 500;
}

.meta {
  margin-top: 2px;
}

.extra {
  margin-top: 4px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
