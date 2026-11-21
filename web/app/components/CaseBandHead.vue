<script setup lang="ts">
/**
 * **A band's heading on the Understanding tab, and the line of facts beside it.** DEV-1327.
 *
 * S3 heads every band the same way: the question in the sans at the heading size, and a
 * muted line of figures on the same baseline — "Approach  option C of 8 · blind pass agrees".
 * **The figures sit outside the `h2`**, so a heading's accessible name is its question and a
 * reader moving by headings hears "Approach", not the arithmetic under it.
 */
defineProps<{
  /** The question the band answers, from `CASE_BANDS` where it is one of them. */
  heading: string
  /** The facts beside it, or nothing when the record has none to say. */
  meta?: string | null
}>()
</script>

<template>
  <div class="band-head">
    <h2 class="rd-display-lead heading">
      {{ heading }}
    </h2>
    <span
      v-if="meta"
      class="rd-meta meta"
    >{{ meta }}</span>
  </div>
</template>

<style scoped>
.band-head {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 12px;
  min-width: 0;
}

.heading {
  margin: 0;
}

.meta {
  overflow-wrap: anywhere;
}
</style>
