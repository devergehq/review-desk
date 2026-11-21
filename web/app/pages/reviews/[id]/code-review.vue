<script setup lang="ts">
/**
 * `/reviews/{id}/code-review` — **Code review's old address, which redirects to its tab.** DEV-1324.
 *
 * The step pages became five tabs; this address is in links people hold, in the queue's
 * history and in anything bookmarked, so it keeps working. Where it goes, and the band an
 * address with no fragment of its own lands at, is `RETIRED_ROUTES` in
 * `app/utils/reviewTabs.ts`. **A fragment the link carries is kept**, so a link to one
 * finding or one band still arrives at it. `e2e/redirects.spec.ts` follows each one.
 *
 * A middleware, as the old `/reviews/{id}` redirect was: it runs before the page resolves,
 * so nothing renders and nothing is read.
 */
definePageMeta({
  middleware: [
    to => navigateTo(
      retiredRouteTarget(String(to.params.id), 'code-review', to.hash, to.query),
      { replace: true, redirectCode: 301 },
    ),
  ],
})
</script>

<template>
  <p class="rd-empty">
    <!-- Never seen: the middleware redirects before this page is resolved. -->
    Taking you to the review.
  </p>
</template>
