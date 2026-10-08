<script setup lang="ts">
import type { RuleCounts } from '~/types/desk'

/**
 * A rule's counts. Each stage carries a different set, and a count the server has no
 * measurement for is left out rather than printed as a nought — a nought is a
 * measurement and its absence is not.
 */
const props = defineProps<{ counts: RuleCounts, stageLabel?: string | null }>()

const LABELS: Array<[keyof RuleCounts, string]> = [
  ['raised', 'Raised'],
  ['kept', 'Kept by you'],
  ['rejected_with_reason', 'Rejected with a reason'],
  ['cited_by_findings', 'Cited by findings'],
  ['caught_by_check', 'Caught by the check'],
  ['raised_in_a_review_since', 'Raised in a review since'],
]

const rows = computed(() =>
  LABELS.filter(([key]) => props.counts[key] != null).map(([key, label]) => ({
    label,
    value: String(props.counts[key]),
    /* A check that graduated and is still being raised by a model is not doing its job. */
    flag: key === 'raised_in_a_review_since' && Number(props.counts[key]) > 0,
  })),
)

const lastSaid = computed(() => props.counts.last_human_mention_at)
</script>

<template>
  <div class="counts">
    <FactRow
      v-for="row in rows"
      :key="row.label"
      :label="row.label"
      :value="row.value"
      numeric
      :failing="row.flag"
    />
    <FactRow
      v-if="stageLabel"
      label="Stage"
      :value="stageLabel"
    />
    <FactRow
      v-if="lastSaid"
      label="Last said by a human"
      :value="sinceLabel(lastSaid)"
    />
  </div>
</template>

<style scoped>
.counts {
  flex: 1 1 300px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
