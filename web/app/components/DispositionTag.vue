<script setup lang="ts">
import type { Disposition } from '~/types/desk'

const props = defineProps<{ disposition: Disposition }>()

const labels: Record<Disposition, string> = {
  in_draft: 'In draft',
  held_back: 'Held back',
  suppressed: 'Suppressed',
}

const label = computed(() => labels[props.disposition])
</script>

<template>
  <span
    class="tag"
    :class="`tag--${disposition}`"
  >{{ label }}</span>
</template>

<style scoped>
.tag {
  font-size: 13px;
  border-radius: var(--rd-radius-xs);
  padding: 3px 8px;
  white-space: nowrap;
}

.tag--in_draft {
  color: var(--rd-ink-dim);
  background: var(--rd-rail);
}

.tag--held_back {
  color: var(--rd-accent-dark);
  background: var(--rd-accent-wash);
}

.tag--suppressed {
  color: var(--rd-ink-muted);
  border: 1px dashed var(--rd-line-strong);
  padding: 2px 8px;
}
</style>
