<script setup lang="ts">
/**
 * What became of something, as the pill the Controls board draws.
 *
 * Five kinds, and each one is a sentence the record can support rather than a decoration:
 *
 * | Kind | Says | Drawn |
 * |---|---|---|
 * | `fixed` | the author fixed it | accent wash, a tick |
 * | `open` | still open, and which round | warning wash, a dot |
 * | `new` | raised this round for the first time | neutral, a plus |
 * | `regression` | a fix introduced it | ink reversed out, a turning arrow |
 * | `waiting` | waiting on the developer | an accent outline, a dot |
 *
 * `waiting` is the only outlined one, and that is deliberate: it is the only one that is
 * about the reader rather than about the record, so it reads as a thing to do rather than
 * as a thing that happened.
 *
 * The icon is `aria-hidden` and the text carries the meaning. A pill whose meaning was in
 * its colour would be unreadable to a screen reader and ambiguous to everyone else.
 */
withDefaults(
  defineProps<{
    kind: 'fixed' | 'open' | 'new' | 'regression' | 'waiting'
  }>(),
  {},
)

/** The board's own paths, at 16×16, stroked in `currentColor`. */
const PATHS: Record<string, string | null> = {
  fixed: 'M3 8.5l3.2 3.2L13 5',
  open: null,
  new: 'M8 3v10M3 8h10',
  regression: 'M3 5h7a3 3 0 010 6H6M5.5 2.5L3 5l2.5 2.5',
  waiting: null,
}
</script>

<template>
  <span
    class="pill"
    :class="`pill--${kind}`"
  >
    <svg
      v-if="PATHS[kind]"
      width="13"
      height="13"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    ><path :d="PATHS[kind]!" /></svg>
    <span
      v-else
      class="dot"
      aria-hidden="true"
    />
    <slot />
  </span>
</template>

<style scoped>
.pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 13px 6px 10px;
  border-radius: var(--rd-radius-pill);
  font-size: 13px;
  font-weight: 500;
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentcolor;
}

.pill--fixed {
  background: var(--rd-accent-wash);
  color: var(--rd-accent-dark);
}

.pill--open {
  background: var(--rd-warn-wash);
  color: var(--rd-warn-ink);
}

.pill--new {
  background: var(--rd-nit-wash);
  color: var(--rd-ink-dim);
}

.pill--regression {
  background: var(--rd-ink);
  color: var(--rd-surface);
}

/* The one that is about the reader: outlined, so it does not read as settled. */
.pill--waiting {
  padding: 5px 12px 5px 9px;
  border: 1.5px solid var(--rd-accent);
  background: var(--rd-surface);
  color: var(--rd-accent);
  font-weight: 600;
}
</style>
