<script setup lang="ts">
/**
 * Step 2 — **Understand**, and the address the case page is read from first.
 *
 * Every row in the queue opens this, which is why it is the address that draws the whole
 * case rather than one third of it: "when we click a review, we want to go to the review,
 * not to the draft", and the review starts here.
 *
 * **The page itself is `ReviewCase`**, which steps 3 and 4 draw too. Patrick picked that
 * shape on 16 October 2026 from three options put to him: one page, a band per question,
 * because three pages at 3,653, 3,132 and 2,768 pixels meant three navigations to read one
 * case and, in his words, "you've got to dig into them". The component carries the argument
 * and the four states a brief has.
 *
 * **A wrapper rather than a redirect.** `/reviews/{id}/understand` is in links people have,
 * in the queue's rows, in the round switcher's suffixes and in anything bookmarked; a
 * redirect would work and would make every one of them a round trip through a page that
 * draws nothing. `step` is what the trail marks with `aria-current` and nothing else.
 */
</script>

<template>
  <ReviewCase step="understand" />
</template>
