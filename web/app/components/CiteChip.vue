<script setup lang="ts">
import type { CiteSource } from '~/utils/cites'

/**
 * **One cite under an answer**, as S7 draws them: `F9`, `ExportController.php:41`,
 * *Criterion 3*. DEV-1333.
 *
 * Where it goes is `citeTarget`'s, the one place a cite meets the record: a link when the
 * record holds what it names, and the session's words otherwise. Never a link to nothing.
 */
const props = defineProps<{ cite: string, source: CiteSource }>()

const target = computed(() => citeTarget(props.cite, props.source))
const code = computed(() => /[/.:]/.test(props.cite) || /^F\d+$/.test(props.cite))
</script>

<template>
  <NuxtLink
    v-if="target.kind === 'page'"
    class="cite"
    :class="{ 'cite--code': code }"
    :to="target.to"
  >{{ target.text }}</NuxtLink>
  <a
    v-else-if="target.kind === 'away'"
    class="cite cite--code"
    :href="target.href"
    target="_blank"
    rel="noopener noreferrer"
  >{{ target.text }}</a>
  <span
    v-else
    class="cite cite--text"
    :class="{ 'cite--code': code }"
  >{{ target.text }}</span>
</template>

<style scoped>
.cite {
  display: inline-flex;
  align-items: center;
  min-height: 22px;
  padding: 1px 8px;
  border: 1px solid var(--rd-line-strong);
  border-radius: var(--rd-radius-xs);
  color: var(--rd-ink);
  background: var(--rd-surface);
  font-size: var(--rd-type-meta);
  text-decoration: none;
  overflow-wrap: anywhere;
}

a.cite:hover {
  border-color: var(--rd-accent);
  color: var(--rd-accent-dark);
}

a.cite:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.cite--code {
  font-family: var(--rd-font-mono);
}

.cite--text {
  color: var(--rd-ink-dim);
}
</style>
