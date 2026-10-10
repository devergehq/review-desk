<script setup lang="ts">
import type { Rule } from '~/types/desk'

/**
 * A rule the app is proposing, and the two things the developer can do about it.
 *
 * **The app proposes and only the developer promotes**, which is the brief's rule and the
 * reason this card exists at all: it is the one place on the Rules screen where something
 * is waiting on a person. The board gives it an accent ring and a green shadow for exactly
 * that, and nothing else on the page has them.
 *
 * Dismissing asks why. That is not politeness — the API refuses a dismissal with no
 * reason, because a rule turned down silently is proposed again next week by a harvester
 * with no memory of having been told no.
 */
defineProps<{
  rule: Rule
  /** The findings behind the proposal, as the chips the board draws. */
  citations?: Array<{ label: string, to: string | null }>
  working?: boolean
}>()

const emit = defineEmits<{ promote: [], dismiss: [] }>()
</script>

<template>
  <DeskCard
    tone="chosen"
    pad="section"
    class="card"
  >
    <div class="body">
      <div class="rd-display-lead">
        {{ rule.statement }}
      </div>
      <div v-if="rule.why_proposed">
        <strong>Why it is proposed.</strong> {{ rule.why_proposed }}
      </div>
      <div>
        <strong>Prevents.</strong> {{ rule.prevents }}
      </div>
      <!--
        The findings it came out of. Chips rather than a sentence, because the developer's
        question is "which ones", and each is a place to go and look.
      -->
      <div
        v-if="citations && citations.length"
        class="citations"
      >
        <template
          v-for="cite in citations"
          :key="cite.label"
        >
          <NuxtLink
            v-if="cite.to"
            :to="cite.to"
            class="chip rd-mono"
          >
            {{ cite.label }}
          </NuxtLink>
          <span
            v-else
            class="chip rd-mono"
          >{{ cite.label }}</span>
        </template>
      </div>
      <p class="rd-meta said">
        If promoted, these findings are still recorded with their reason. They are just not
        put in a draft.
      </p>
    </div>

    <div class="actions">
      <DeskButton
        kind="primary"
        arrow
        class="promote"
        :working="working"
        working-label="Promoting…"
        @click="emit('promote')"
      >
        Make it a rule
      </DeskButton>
      <DeskButton
        class="dismiss"
        :disabled="working"
        @click="emit('dismiss')"
      >
        Dismiss, and say why
      </DeskButton>
      <p class="rd-meta said">
        A dismissal with a reason stops it being proposed again.
      </p>
    </div>

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
  gap: 24px 40px;
  align-items: flex-start;
}

.body {
  display: flex;
  flex: 3 1 420px;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

strong {
  font-weight: 600;
}

.citations {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.chip {
  padding: 6px 10px;
  border-radius: var(--rd-radius-xs);
  background: var(--rd-ground);
  color: var(--rd-ink);
  text-decoration: none;
}

.chip:hover {
  background: var(--rd-rail);
  color: var(--rd-accent-dark);
}

.actions {
  display: flex;
  flex: 1 1 240px;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

/* Both controls fill the column, so the one action is plainly the one action. */
.promote,
.dismiss {
  justify-content: space-between;
  width: 100%;
}

.promote {
  min-height: 52px;
  font-size: 16px;
}

.dismiss {
  justify-content: flex-start;
}

.said {
  margin: 0;
}

.reason {
  flex: 1 1 100%;
}
</style>
