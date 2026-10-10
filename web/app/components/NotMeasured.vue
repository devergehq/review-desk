<script setup lang="ts">
/**
 * What the screen says where a figure has no source.
 *
 * **One component, because there is one rule and it is the brief's.** A figure Review Desk
 * cannot measure is shown as unknown and never as a number — a nought is a measurement and
 * the absence of one is not. The contract is written for it: every count the store cannot
 * yet answer is nullable rather than zero, and `RulesSummary` says so field by field.
 *
 * It draws as the words, under a dashed rule, with the reason as the title — so the same
 * absence looks the same everywhere, and a reader who wonders why can find out without
 * leaving the page.
 *
 * The reason is not optional. "Not measured yet" with nothing behind it reads as a bug; the
 * same words with "the replay check is a later slice" behind them read as a plan.
 */
withDefaults(
  defineProps<{
    /** Why there is no figure. Shown as the title, and read out after the words. */
    because: string
    /** The words. Changed only where "yet" would be a lie. */
    says?: string
  }>(),
  { says: 'Not measured yet' },
)
</script>

<template>
  <span
    class="unknown"
    :title="because"
  >
    {{ says }}<span class="rd-visually-hidden"> — {{ because }}</span>
  </span>
</template>

<style scoped>
.unknown {
  border-bottom: 1px dashed var(--rd-line-strong);
  color: var(--rd-ink-muted);
  /* It is not a number, so it does not get the figures' face. */
  font-family: var(--rd-font-sans);
  cursor: help;
}
</style>
