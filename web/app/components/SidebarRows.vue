<script setup lang="ts">
/**
 * One sidebar group's pull requests: the number, the title, the state word.
 *
 * **The title is the recorded one, cut to two lines.** S1 draws a shorter title than the
 * list beside it ("Scheduled export context" for "Give a scheduled export its own
 * context"), and nothing in the record holds a shorter title — so the sidebar draws the
 * real one and lets the line clamp shorten it, rather than inventing an abbreviation.
 *
 * **A row with no round is not a link.** A review GitHub asked for that no session has
 * opened a round of has nowhere to go yet, so it is drawn as text.
 */
import type { SidebarRow } from '~/utils/sidebar'

defineProps<{
  rows: SidebarRow[]
  openReviewId: number | null
}>()
</script>

<template>
  <ul class="rows">
    <li
      v-for="row in rows"
      :key="row.key"
    >
      <component
        :is="row.reviewId != null ? resolveComponent('NuxtLink') : 'div'"
        :to="row.reviewId != null ? `/reviews/${row.reviewId}` : undefined"
        class="side-row"
        :class="{ 'side-row--open': row.reviewId != null && row.reviewId === openReviewId }"
        :aria-current="row.reviewId != null && row.reviewId === openReviewId ? 'page' : undefined"
      >
        <span class="line">
          <span class="number">#{{ row.pr }}</span>
          <span class="title">{{ row.title ?? 'no title recorded' }}</span>
        </span>
        <span class="state">{{ stateWord(row.state) }}</span>
      </component>
    </li>
  </ul>
</template>

<style scoped>
.rows {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.side-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border-radius: var(--rd-radius-sm);
  color: var(--rd-ink);
  text-decoration: none;
}

a.side-row:hover {
  color: var(--rd-ink);
  background: var(--rd-rail-pressed);
}

.side-row--open {
  background: var(--rd-rail);
}

.side-row--open .title {
  font-weight: 600;
}

.line {
  display: flex;
  gap: 8px;
  min-width: 0;
}

.number {
  flex: none;
  color: var(--rd-ink-muted);
  font-size: var(--rd-type-ui);
  line-height: 1.5;
  font-variant-numeric: tabular-nums;
}

.title {
  display: -webkit-box;
  min-width: 0;
  overflow: hidden;
  overflow-wrap: anywhere;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  font-size: var(--rd-type-body);
  line-height: 1.4;
}

.state {
  color: var(--rd-ink-muted);
  font-size: var(--rd-type-ui);
}
</style>
