<script setup lang="ts">
import type { Suggestion } from '~/types/desk'

/**
 * **One thing an answer suggests**, from the closed list of seven. DEV-1333 draws it
 * **inert**: what it would do, and nothing to press.
 *
 * DEV-1334 makes it act. The `actions` slot is where its Accept and Dismiss go, so the card
 * gains behaviour there rather than a second card being drawn beside this one.
 */
const props = defineProps<{ suggestion: Suggestion }>()

const said = computed(() => suggestionLine(props.suggestion))
</script>

<template>
  <li
    class="suggestion"
    :data-type="suggestion.type"
  >
    <div class="what">
      <p class="line">
        {{ said.line }}
      </p>
      <p
        v-if="said.why"
        class="why rd-meta"
      >
        {{ said.why }}
      </p>
    </div>
    <slot
      name="actions"
      :suggestion="suggestion"
    />
  </li>
</template>

<style scoped>
.suggestion {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  justify-content: space-between;
  padding: 8px 10px;
  background: var(--rd-ground);
  border-radius: var(--rd-radius-control);
}

.what {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.line,
.why {
  margin: 0;
  overflow-wrap: anywhere;
}

.line {
  font-size: var(--rd-type-ui);
}
</style>
