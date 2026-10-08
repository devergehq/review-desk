<script setup lang="ts">
/**
 * How far along something is. The track is a light step of the accent rather than a
 * neutral grey, so the whole bar reads as one measure, and the number is always written
 * out beside it — the fill is never the only thing saying where the work has got to.
 */
const props = defineProps<{ value: number, max: number, label: string }>()

const percent = computed(() => Math.round((props.value / props.max) * 100))
</script>

<template>
  <div
    class="track"
    role="progressbar"
    :aria-valuenow="value"
    :aria-valuemin="0"
    :aria-valuemax="max"
    :aria-label="label"
  >
    <div
      class="fill"
      :style="{ width: `${percent}%` }"
    />
  </div>
</template>

<style scoped>
.track {
  height: 6px;
  border-radius: 3px;
  background: var(--rd-accent-wash);
  overflow: hidden;
}

.fill {
  height: 6px;
  border-radius: 3px;
  background: var(--rd-accent);
}
</style>
