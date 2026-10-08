<script setup lang="ts">
/** One figure, what it counts, and the period it counts over. */
defineProps<{ label: string, value: string, note: string }>()
</script>

<template>
  <DeskCard pad="tight">
    <div class="rd-meta">
      {{ label }}
    </div>
    <div class="value">
      {{ value }}
    </div>
    <div class="rd-meta">
      {{ note }}
    </div>
  </DeskCard>
</template>

<style scoped>
.value {
  font-size: 26px;
  font-weight: 600;
  line-height: 1.2;
}
</style>
