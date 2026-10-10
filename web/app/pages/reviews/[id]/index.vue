<script setup lang="ts">
/**
 * `/reviews/{id}` — **the one-page review, retired.**
 *
 * It printed the whole of a review as paragraphs: the problem, the options, and nineteen
 * findings with their reasoning, up to 326 words each. Patrick, reading a real review on it
 * on 10 October 2026: "a mess… I can't even read it", and "a bit of a fucking dumpster
 * fire". DEV-885 replaced steps 2, 3 and 4 with a page each, DEV-878 replaced step 5, and
 * what was left on this page was the four screens' worth of content those five now draw
 * better.
 *
 * **Its address redirects to Understand**, which is where a review starts. The address is
 * in links people have, in the header nav's history and in anything Patrick has bookmarked,
 * so it keeps working — a route that 404s is worse than one that arrives somewhere useful,
 * and Understand is the page every queue row already opens.
 *
 * A redirect rather than a copy of the page under a new name: nothing on it is lost.
 *
 * | What it drew | Where it is now |
 * |---|---|
 * | the problem, the diagram, the brief | Understand |
 * | the options and the approach verdict | Alternatives |
 * | the change in parts | Solution |
 * | the findings, with their dispositions | Code review |
 * | "Not checked" | Code review, under "What the review did not look at" |
 * | the author's replies | still only here — and that is a gap, recorded below |
 * | the confirm-the-brief card | Understand, which DEV-885 built it on |
 *
 * **One thing goes with the page and is not replaced: the harvested author replies.** They
 * belong to step 7, which has no page — DEV-878 puts the harvest page out of scope by name.
 * The Inbox still groups them under "The author replied" and still links to the review, so
 * they are reachable; nothing on a review's own pages draws them until step 7 is built.
 *
 * **And one thing goes with it on purpose.** This page printed
 * `confirmation_not_required_because` verbatim in a "Confirmation not required." card.
 * Since DEV-883 the review session writes a brief twice, and between the two writes that
 * field begins "problem half only" — so the card showed that string as though it were the
 * reason. The comment on DEV-878 says: "When this issue retires the old page the problem
 * goes with it." It does.
 */
definePageMeta({
  /*
   * A middleware rather than a template that redirects on mount: the redirect then happens
   * before the page component is resolved, so nothing renders and nothing is read. A
   * component that called `navigateTo` in `onMounted` would flash a blank page and, worse,
   * would have to be a page that could read a review in order to decide not to.
   */
  middleware: [
    (to) => {
      return navigateTo(`/reviews/${to.params.id}/understand`, { redirectCode: 302 })
    },
  ],
})
</script>

<template>
  <p class="rd-empty">
    <!--
      Never seen: the middleware redirects before this page is resolved. Here because a page
      needs a template, and this is what it would say if a redirect were somehow skipped.
    -->
    Taking you to the review.
  </p>
</template>
