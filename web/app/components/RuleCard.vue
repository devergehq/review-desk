<script setup lang="ts">
import type { Rule } from '~/types/desk'

/** An active rule: what it says, the failure it prevents, where it was decided, counts. */
defineProps<{ rule: Rule }>()
</script>

<template>
  <DeskCard class="rule">
    <div class="statement">
      <div class="head">
        <span class="rd-mono rd-meta">{{ rule.key }}</span>
        <h3 class="title">
          {{ rule.statement }}
        </h3>
      </div>
      <p class="rd-note prevents">
        <strong>Prevents:</strong> {{ rule.prevents }}
      </p>
      <p class="rd-meta provenance">
        <template v-if="rule.origin_pr">
          Decided on
          <NuxtLink
            v-if="rule.origin_review_id"
            :to="`/reviews/${rule.origin_review_id}`"
          >
            #{{ rule.origin_pr }}
          </NuxtLink>
          <template v-else>
            #{{ rule.origin_pr }}
          </template>
        </template>
        <template v-if="rule.origin_pr && rule.note">
          ·
        </template>
        <template v-if="rule.note">
          {{ rule.note }}
        </template>
      </p>
    </div>
    <RuleCounts
      :counts="rule.counts"
      :stage-label="rule.stage_label"
    />
  </DeskCard>
</template>

<style scoped>
.rule {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}

.statement {
  flex: 999 1 420px;
  min-width: 0;
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

.prevents {
  margin: 6px 0 0;
}

.prevents strong {
  font-weight: 600;
}

.provenance {
  margin: 4px 0 0;
}
</style>
