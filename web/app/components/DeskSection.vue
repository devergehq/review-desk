<script setup lang="ts">
/**
 * A group: its rubric, what is beside the rubric, and the rows under it.
 *
 * The Queue and Rules boards both draw it the same way — small uppercase heading, a grey
 * count next to it, a 1px ink rule under the pair, and the rows below that. The rule under
 * the heading is `#14171A`, not the row grey: it is the line that says a group starts here.
 *
 * `note` is the thing the boards put after the count — "nothing for you to do", "3 of 14
 * shown". It is not a second count; it is a sentence about the group.
 */
withDefaults(
  defineProps<{
    heading: string
    /** Shown beside the heading. Left off when there is nothing to count. */
    count?: number | null
    note?: string | null
    /** `ruled` draws the board's line under the heading; `open` leaves it off. */
    tone?: 'ruled' | 'open'
  }>(),
  { count: null, note: null, tone: 'ruled' },
)
</script>

<template>
  <section class="group">
    <div
      class="head"
      :class="{ 'head--ruled': tone === 'ruled' }"
    >
      <h2 class="rd-group-heading">
        {{ heading }}
      </h2>
      <span
        v-if="count != null || note"
        class="rd-meta"
      >
        <template v-if="count != null">{{ count }}</template>
        <template v-if="count != null && note"> · </template>
        <template v-if="note">{{ note }}</template>
      </span>
    </div>
    <slot />
  </section>
</template>

<style scoped>
.group {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
}

.head--ruled {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--rd-ink);
}

/* A group with no rule under its heading still needs air before its content. */
.head:not(.head--ruled) {
  padding-bottom: 16px;
}
</style>
