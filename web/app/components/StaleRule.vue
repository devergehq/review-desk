<script setup lang="ts">
import type { Rule } from '~/types/desk'

/**
 * A rule nothing has cited lately, and the two answers to that.
 *
 * **Still in force.** A stale rule is not a retired one — the contract is explicit that
 * staleness is derived rather than a stage, precisely so that the rule keeps applying until
 * the developer decides. So this is a grey well rather than a dashed outline: it is a
 * question, not a ruin.
 *
 * "Keep 60 more days" and the sixty days in the sentence are the same number, and it is
 * the contract's, written once there and quoted here.
 *
 * Retiring asks why, for the same reason dismissing a proposal does.
 */
defineProps<{ rule: Rule, working?: boolean }>()

const emit = defineEmits<{ retire: [], defer: [] }>()

/*
 * The track in `act` mode: these are two things to do, not two states to be in. The board
 * draws them in one track and lifts the first, which is what `emphasis` is for.
 */
function act(answer: string) {
  if (answer === 'retire') emit('retire')
  if (answer === 'defer') emit('defer')
}
</script>

<template>
  <DeskCard
    tone="well"
    class="card"
  >
    <div class="body">
      <h3 class="rd-display-statement">
        <span class="key rd-mono rd-meta">{{ rule.key }}</span>{{ rule.statement }}
      </h3>
      <p class="why">
        {{ rule.note ?? staleLabel(rule) }}
      </p>
    </div>
    <DecisionTrack
      mode="act"
      class="answers"
      :label="`What to do about ${rule.key}`"
      :disabled="working"
      :options="[
        { value: 'retire', label: 'Retire it', emphasis: true },
        { value: 'defer', label: 'Keep 60 more days' },
      ]"
      @act="act"
    />
    <div
      v-if="$slots.reason"
      class="reason"
    >
      <slot name="reason" />
    </div>
  </DeskCard>
</template>

<style scoped>
.card {
  display: flex;
  flex-wrap: wrap;
  gap: 18px 40px;
  align-items: center;
  padding: 26px 30px;
}

.body {
  display: flex;
  flex: 3 1 420px;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.key {
  margin-right: 10px;
}

.why {
  margin: 0;
  color: var(--rd-ink-dim);
}

.answers {
  flex: 0 1 auto;
}

.reason {
  flex: 1 1 100%;
}
</style>
