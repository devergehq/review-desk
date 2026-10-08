<script setup lang="ts">
import type { BriefOption } from '~/types/desk'

/** One alternative from the option map, written before the diff was read. */
defineProps<{ option: BriefOption }>()
</script>

<template>
  <DeskCard
    :tone="option.chosen ? 'chosen' : 'plain'"
    class="option"
  >
    <div class="head">
      <h3 class="title">
        {{ option.key }} · {{ option.title }}
      </h3>
      <span
        v-if="option.chosen"
        class="chosen"
      >What they built</span>
    </div>
    <p class="rd-note line">
      <strong>For:</strong> {{ option.argument_for }}
    </p>
    <p class="rd-note line">
      <strong>Against:</strong> {{ option.argument_against }}
    </p>
  </DeskCard>
</template>

<style scoped>
.option {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.chosen {
  font-size: 12px;
  font-weight: 600;
  color: var(--rd-accent-dark);
  background: var(--rd-accent-wash);
  border-radius: var(--rd-radius-xs);
  padding: 2px 8px;
}

.line {
  margin: 0;
}

.line strong {
  font-weight: 600;
}
</style>
