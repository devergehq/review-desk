<script setup lang="ts">
/** A label on the left, its value on the right. Checks and rule counts are both these. */
defineProps<{
  label: string
  value: string
  /** Numbers read in a column, so they get tabular figures. */
  numeric?: boolean
  failing?: boolean
}>()
</script>

<template>
  <div class="fact">
    <span class="label">{{ label }}</span>
    <span
      class="value"
      :class="{ 'value--numeric': numeric, 'value--failing': failing }"
    >
      {{ value }}
    </span>
  </div>
</template>

<style scoped>
.fact {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 14px;
}

.label {
  color: var(--rd-ink-muted);
}

.value {
  font-weight: 500;
  text-align: right;
}

.value--numeric {
  font-family: var(--rd-font-mono);
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}

.value--failing {
  color: var(--rd-warn-ink);
  font-weight: 600;
}
</style>
