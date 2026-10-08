<script setup lang="ts">
import type { Severity } from '~/types/desk'

const props = defineProps<{ severity: Severity }>()

const labels: Record<Severity, string> = {
  blocker: 'Blocker',
  should: 'Should',
  nit: 'Nit',
  question: 'Question',
}

const label = computed(() => labels[props.severity])
</script>

<template>
  <span
    class="tag"
    :class="`tag--${severity}`"
  >{{ label }}</span>
</template>

<style scoped>
.tag {
  font-size: 12px;
  font-weight: 600;
  padding: 3px 8px;
  border-radius: var(--rd-radius-xs);
  white-space: nowrap;
}

.tag--blocker {
  background: var(--rd-ink);
  color: var(--rd-surface);
}

.tag--should {
  background: var(--rd-warn-wash);
  color: var(--rd-warn-ink);
}

.tag--nit,
.tag--question {
  background: var(--rd-rail);
  color: var(--rd-ink-dim);
}
</style>
