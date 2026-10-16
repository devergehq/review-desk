<script setup lang="ts">
/**
 * Step 3 — **Alternatives**, which is band 2 of the case page.
 *
 * The matrix of options against numbered problems, who put each one forward, and what an
 * independent pass would have picked. It is drawn by `ReviewCase` with the other two steps,
 * for the reason that component states; this address lands on it, and the trail's link to
 * this step carries `#what-was-chosen` so pressing it arrives at the band.
 *
 * **`not_assessed` is a verdict, not a missing one**, and `CaseChosen` is where that rule
 * lives now.
 */
</script>

<template>
  <ReviewCase step="alternatives" />
</template>
