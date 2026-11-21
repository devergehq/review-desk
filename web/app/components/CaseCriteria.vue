<script setup lang="ts">
import type { AcceptanceCriterion } from '~/types/desk'

/**
 * **Ticket against what was built** — S3's second band. DEV-1327.
 *
 * What the ticket asked for, a row each in the order the session wrote them, and what the
 * review observed about each: *yes*, *no, see F9* with the finding a link, or *not in this
 * PR*. The last column is the one pointer the contract allows a criterion — the file and
 * line that answer for it, or the session's note.
 *
 * **Two of S3's columns are not drawn, because the record does not hold them.** *Claimed*
 * would be what the pull request said it did, and *How* whether the review ran a check or
 * read the code; a criterion carries neither. `criterionObserved` says so where it chooses
 * the words, and the pull request says so under *Departs from the design*.
 *
 * **A review that recorded no criteria says so**, rather than dropping the band: the rail
 * names it, and a band that vanished on most reviews would read as a page that failed to
 * draw.
 */
const { reviewId, criteria } = defineProps<{
  reviewId: number
  criteria: readonly AcceptanceCriterion[]
}>()

const rows = computed(() => [...criteria]
  .sort((one, other) => one.position - other.position)
  .map(criterion => ({
    criterion,
    observed: criterionObserved(criterion),
    where: criterionWhere(criterion),
  })))

const findings = computed(() => tabPath(reviewId, 'findings'))

const meta = computed(() => {
  if (!rows.value.length) return null
  const met = rows.value.filter(row => row.observed.mark === 'met').length
  return `${count(rows.value.length, 'criterion', 'criteria')} · ${met} observed met`
})

const COLUMNS = ['Criterion', 'Observed', 'Where']
</script>

<template>
  <section
    id="ticket-against-what-was-built"
    class="band"
  >
    <CaseBandHead
      :heading="bandLabel('ticket-against-what-was-built')"
      :meta="meta"
    />

    <p
      v-if="!rows.length"
      class="rd-note none"
    >
      The review recorded no criteria from the ticket.
    </p>

    <DeskTable
      v-else
      class="criteria"
      :columns="COLUMNS"
      template="minmax(0, 1.6fr) minmax(0, 0.8fr) minmax(0, 1fr)"
      label="What the ticket asks for, and what the review observed"
    >
      <tr
        v-for="row in rows"
        :key="row.criterion.position"
        role="row"
        class="row"
        :class="{ 'row--not-met': row.observed.mark === 'not_met' }"
      >
        <td
          role="cell"
          class="td td--strong"
          data-label="Criterion"
        >
          <TickedText :text="row.criterion.text" />
        </td>
        <td
          role="cell"
          class="td"
          data-label="Observed"
        >
          <!--
            **The flex is on a span inside the cell, never on the cell.** Under 760px
            DeskTable draws each cell's column name as a block `::before`, and a flex cell
            would set that name beside the value instead of above it.
          -->
          <span
            class="observed"
            :class="`observed--${row.observed.mark}`"
          >
            <span
              class="mark"
              aria-hidden="true"
            />
            <NuxtLink
              v-if="row.observed.finding"
              :to="`${findings}#${row.observed.finding}`"
            >{{ row.observed.says }}</NuxtLink>
            <template v-else>
              {{ row.observed.says }}
            </template>
          </span>
        </td>
        <td
          role="cell"
          class="td where"
          data-label="Where"
        >
          <span
            v-if="row.where"
            :class="{ 'rd-mono': row.criterion.file }"
          >{{ row.where }}</span>
          <span
            v-else
            class="rd-note"
          >—</span>
        </td>
      </tr>
    </DeskTable>
  </section>
</template>

<style scoped>
.band {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.none {
  margin: 0;
}

.criteria :deep(.td) {
  min-width: 0;
  overflow-wrap: break-word;
}

.td--strong {
  font-weight: 500;
}

/*
 * **The not-met row is S3's tinted row**: the one a reader is looking for, in the accent's
 * wash rather than in red, because the red is the mark's and the row is only where it is.
 */
.row--not-met {
  background: var(--rd-accent-wash);
}

.observed {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
}

/* A ring drawn, not typed: no glyph from the body font, one shape in three inks. */
.mark {
  flex: none;
  width: 9px;
  height: 9px;
  border: 1.5px solid currentcolor;
  border-radius: var(--rd-radius-pill);
  transform: translateY(1px);
}

.observed--met {
  color: var(--rd-accent);
}

.observed--met .mark {
  background: currentcolor;
}

.observed--not_met {
  color: var(--rd-wrong);
}

.observed--not_met a {
  color: inherit;
}

.observed--not_in_this_pr {
  color: var(--rd-ink-muted);
}
</style>
