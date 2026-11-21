<script setup lang="ts">
/**
 * `/reviews/{id}` — the **Overview** tab, and the address every pull request opens at.
 *
 * It was a redirect to Understand from DEV-878, which retired the one-page review that
 * stood here. DEV-1324 makes it the first of a review's five tabs again.
 *
 * **What it draws until DEV-1326 builds Overview**: the case, as Understanding draws it,
 * with the step trail under the header. That is what this address arrived at before — the
 * redirect landed on the case, where a review starts — and the trail is the one thing that
 * says where each step stands until Overview's *Where it stands* does. DEV-1326 replaces
 * this page's body with the S2 Overview (the bottom line, where it stands, the five
 * questions); the header and tabs above it are already the S2 ones.
 */
</script>

<template>
  <ReviewCase tab="overview" />
</template>
