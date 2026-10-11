<script setup lang="ts">
/**
 * A small table that becomes a labelled list at a phone's width.
 *
 * **A board has one width; an app has two.** The approved boards draw these as CSS grids at
 * 1200px — the supporting table is four columns, the open-choices table is four, the
 * matrix is six — and a four-column grid at 390px either scrolls sideways or squeezes a
 * column to nothing. DEV-885's first acceptance criterion forbids the first, so the table
 * **restructures**: under 760px the header row goes, each row becomes a card, and each cell
 * carries the name of its own column.
 *
 * The narrow labels are the record's own words. `columns[]` comes off
 * `brief.support_table.columns`, and the caller passes the matching one to each cell as
 * `data-label` — so a brief that names its columns differently is labelled differently,
 * with nothing hardcoded.
 *
 * ```
 *   wide                              narrow
 *   ┌─────────┬────────┬────────┐     ┌──────────────────────────┐
 *   │ Question│ Owner  │ Before │     │ The question             │
 *   ├─────────┼────────┼────────┤     │   Which tenant is this…  │
 *   │ Which…  │ The…   │ A requ…│     │ Who should answer it     │
 *   └─────────┴────────┴────────┘     │   The export context     │
 *                                     └──────────────────────────┘
 * ```
 *
 * ## The elements are a table; the roles have to say so out loud
 *
 * These are tables and are written as `<table>`, `<th scope="col">` and `<td>`. But the
 * geometry the boards set — `1.2fr 1fr 1.4fr`, and a card per row under 760px — is a grid,
 * and **`display: grid` on a table row strips the table semantics in every browser**: the
 * table, row and cell roles go, and with them the `<th>`-to-cell association that makes a
 * column name reach a screen reader.
 *
 * So the roles are written explicitly. `role="table"`, `role="rowgroup"`, `role="row"`,
 * `role="columnheader"` and `role="cell"` restore what the display property took away, and
 * the header stays in the accessibility tree at narrow widths where it is moved out of
 * sight. Without them the only thing carrying a column name was the visible `data-label`
 * and the `<caption>` — which is what this comment used to claim was fine, and was not.
 */
defineProps<{
  /** The column names, in order. From the record wherever the record has them. */
  columns: string[]
  /** The wide layout's `grid-template-columns`. The board's own ratios. */
  template: string
  /** Named for a screen reader. A table with no name is a table nobody can skip to. */
  label: string
}>()
</script>

<template>
  <table
    class="table"
    role="table"
    :style="{ '--template': template }"
  >
    <caption class="rd-visually-hidden">
      {{ label }}
    </caption>
    <thead
      class="head"
      role="rowgroup"
    >
      <tr
        class="row row--head"
        role="row"
      >
        <!--
          The column names, or whatever the caller draws instead. The matrix on the
          Alternatives page heads three of its columns with a numbered mark and the problem
          it stands for, which a string cannot carry — so the slot takes the `<th>`s and
          `columns` stays the source of the narrow layout's labels either way.
        -->
        <slot name="head">
          <th
            v-for="(column, index) in columns"
            :key="index"
            scope="col"
            role="columnheader"
            class="th"
          >
            {{ column }}
          </th>
        </slot>
      </tr>
    </thead>
    <tbody role="rowgroup">
      <slot />
    </tbody>
  </table>
</template>

<style scoped>
/*
 * `display: grid` on the table rather than `table-layout`, because the boards' ratios are
 * `1.2fr 1fr 1.4fr` and friends, which a real table layout cannot express. The semantics
 * are the elements'; the geometry is the grid's.
 */
.table {
  display: block;
  width: 100%;
  min-width: 0;
  border-collapse: collapse;
  text-align: left;
}

.table :deep(tbody) {
  display: block;
}

.head {
  display: block;
}

.table :deep(.row) {
  display: grid;
  grid-template-columns: var(--template);
  gap: 8px 20px;
  align-items: baseline;
  min-width: 0;
  padding: 12px 16px;
  border-top: 1px solid var(--rd-line);
}

.row--head {
  padding: 0 16px 10px;
  border-top: 0;
}

.table :deep(.th) {
  min-width: 0;
  font-size: 13px;
  font-weight: 400;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--rd-ink-muted);
}

/*
 * A heading with a mark beside its words — the Alternatives matrix, where each problem
 * column is headed by its number and the two or three words naming it.
 *
 * **It keeps the column-name style, which the sentence version could not.** This used to be
 * `th--sentence`: the heading was the problem's whole `was_wrong` line, so the uppercase and
 * the letter-spacing had to go, and on a real review the result wrapped ten lines deep.
 * A label is a column name, so it is drawn like one; the mark is a 22px circle beside it and
 * `align-items: center` is what sits the two on the same line.
 */
.table :deep(.th--marked) {
  display: flex;
  gap: 8px;
  align-items: center;
  overflow-wrap: break-word;
}

/* Cells the caller renders. Scoped styles do not reach a slot, so these are `:deep`. */
.table :deep(.td) {
  min-width: 0;
  overflow-wrap: break-word;
}

.table :deep(.td--strong) {
  font-weight: 600;
}

.table :deep(.td--muted) {
  color: var(--rd-ink-muted);
}

/* A row of pills, kept on one line each and wrapping as whole pills. */
.table :deep(.td--pills) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

/*
 * The 1px line that closes the last row. The boards draw one under the table as well as
 * between its rows, and a list whose last row has no bottom edge reads as unfinished.
 */
.table :deep(.end) {
  display: block;
  border-top: 1px solid var(--rd-line);
}

@media (max-width: 760px) {
  /*
   * Out of sight, not removed: `display: none` on a header takes the column names out of
   * the accessibility tree too, and then the visible label below is the only one there is.
   */
  .head {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .table :deep(.row) {
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 16px;
  }

  /* Each cell says which column it is, in the record's own words. */
  .table :deep(.td[data-label]::before) {
    content: attr(data-label);
    display: block;
    margin-bottom: 2px;
    font-size: 13px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--rd-ink-muted);
  }

  /* `::before` needs a block to sit above, which a flex row of pills is not. */
  .table :deep(.td--pills) {
    display: block;
  }

  .table :deep(.td--pills > *) {
    margin: 0 6px 2px 0;
  }
}
</style>
