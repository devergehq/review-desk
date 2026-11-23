<script setup lang="ts">
/**
 * One sidebar group's pull requests: the number, the title, the state word.
 *
 * **The title is the recorded one, cut to two lines.** S1 draws a shorter title than the
 * list beside it ("Scheduled export context" for "Give a scheduled export its own
 * context"), and nothing in the record holds a shorter title — so the sidebar draws the
 * real one and lets the line clamp shorten it, rather than inventing an abbreviation.
 *
 * **Every row is a link** (DEV-1373). A row with a round opens its review's Overview. A
 * review GitHub asked for that no session has opened a round of has no review page, so it
 * opens the Pull requests list with that pull request selected — `/?pr=acme/portal%23901`
 * — where the preview says what can be done with it. It was drawn as text until then, and
 * Patrick found the dead end: "the actual PRs I can't select".
 */
import type { RouteLocationRaw } from 'vue-router'

import { selectionQuery } from '~/utils/pullRequests'
import type { SidebarRow } from '~/utils/sidebar'

defineProps<{
  rows: SidebarRow[]
  openReviewId: number | null
}>()

function rowTo(row: SidebarRow): RouteLocationRaw {
  return row.reviewId != null
    ? `/reviews/${row.reviewId}`
    : { path: '/', query: selectionQuery(row.key) }
}
</script>

<template>
  <ul class="rows">
    <li
      v-for="row in rows"
      :key="row.key"
    >
      <NuxtLink
        :to="rowTo(row)"
        class="side-row"
        :class="{ 'side-row--open': row.reviewId != null && row.reviewId === openReviewId }"
        :aria-current="row.reviewId != null && row.reviewId === openReviewId ? 'page' : undefined"
      >
        <span class="line">
          <span class="number">#{{ row.pr }}</span>
          <span class="title">{{ row.title ?? 'no title recorded' }}</span>
        </span>
        <span class="state">{{ stateWord(row.state) }}</span>
      </NuxtLink>
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
