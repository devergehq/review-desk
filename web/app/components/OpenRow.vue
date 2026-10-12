<script setup lang="ts">
/**
 * A row you open, and whose detail appears **in place**.
 *
 * That is DEV-885's criterion in one component: "Opening a row shows its detail in place.
 * Nothing navigates away, and nothing links out to GitHub as the only way to read
 * something." Patrick's reason is the one worth keeping in view — he wants to "go down the
 * rabbit holes when I need to" without having to go down one to understand the page. So
 * the row is readable closed, and what is behind it arrives under it rather than somewhere
 * else.
 *
 * **A `<button>`, not a `<summary>` and not an `<a>`.** `<details>` would be less code and
 * the wrong element: the detail is a sibling the caller renders, because on the Understand
 * page opening a problem row also marks its boxes on both flows — one row's state changes
 * three places, which `<details>` has no way to say. `aria-expanded` and
 * `aria-controls` carry the relationship instead.
 *
 * The caller owns which row is open — one at a time on all three pages, as on the boards —
 * so this takes `open` and emits `toggle` rather than holding state of its own.
 */
const props = defineProps<{
  open: boolean
  /** The id of the element this row opens, for `aria-controls`. */
  controls: string
  /** What the right-hand label says when the row is closed. Open, it says "Fold". */
  opens: string
  /** Named for a screen reader, so "By name" is not the whole announcement. */
  label: string
}>()

defineEmits<{ toggle: [] }>()

const says = computed(() => (props.open ? 'Fold' : props.opens))
</script>

<template>
  <button
    type="button"
    class="row"
    :class="{ 'row--open': open }"
    :aria-expanded="open"
    :aria-controls="controls"
    @click="$emit('toggle')"
  >
    <!--
      **`&#32;` after each of these, and it is a scan that found them.** Vue's
      `whitespace: 'condense'` deletes a whitespace-only text node between two elements, so
      the newlines here render nothing: the row's accessible name came out as
      `Open F4, nit, What F4 says is wrong:F4nitWhat F4 says is wrongOpen`, with every
      boundary closed up. `test/step-pages.nuxt.spec.ts` holds the rule for a hidden label
      that ends in a colon; the slot's own two boundaries are the same defect and are fixed
      with it. `what-happened.vue` carries the measurement of all three forms.
    -->
    <span class="rd-visually-hidden">{{ open ? 'Fold' : 'Open' }} {{ label }}:</span>&#32;
    <slot />&#32;
    <span class="opens">{{ says }}</span>
  </button>
</template>

<style scoped>
.row {
  /*
   * The grid is the caller's: every one of these rows is a column layout the board sets,
   * and a row that imposed its own would make each page fight it. What is here is what is
   * true of all of them — the control geometry, the rule above, and the open wash.
   */
  display: grid;
  align-items: center;
  gap: 8px 20px;
  width: 100%;
  min-width: 0;
  min-height: var(--rd-control-item);
  padding: 12px 16px;
  border: 0;
  border-top: 1px solid var(--rd-line);
  background: var(--rd-surface);
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.14s ease;
}

@media (prefers-reduced-motion: reduce) {
  .row {
    transition: none;
  }
}

.row:hover {
  background: var(--rd-ground);
}

.row--open {
  background: var(--rd-ground);
}

.row:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -3px;
}

/*
 * The label is the last cell of the caller's grid, right-aligned, and it is the accent
 * because it is the thing to press. `nowrap` because "By name" on two lines is the pill
 * defect in a different costume.
 */
.opens {
  justify-self: end;
  font-size: 14px;
  font-weight: 500;
  color: var(--rd-accent);
  white-space: nowrap;
}

.row:hover .opens {
  color: var(--rd-accent-dark);
  text-decoration: underline;
}
</style>
