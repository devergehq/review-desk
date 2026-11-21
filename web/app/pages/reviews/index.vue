<script setup lang="ts">
/**
 * **`/reviews` is the Pull requests list's *Every review* view now.** DEV-1325.
 *
 * Queue and Reviews became one list (S1), and the Reviews list is drawn whole inside it.
 * An old link, a bookmark and the page a deleted review hands its receipt to all still land
 * on it: the query is kept, and the view is set.
 */
definePageMeta({
  middleware: [
    to => navigateTo(
      { path: '/', query: { ...to.query, view: 'every' }, hash: to.hash },
      { replace: true, redirectCode: 301 },
    ),
  ],
})
</script>

<template>
  <p class="rd-empty">
    <!-- Never seen: the middleware redirects before this page is resolved. -->
    Taking you to every review.
  </p>
</template>
