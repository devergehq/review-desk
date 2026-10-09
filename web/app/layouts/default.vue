<script setup lang="ts">
/**
 * The shell every screen sits in: the name, the three places you can be, and the marker
 * that says these records are invented.
 *
 * The boards all carry the same header — the name in Newsreader at 22px, the three items
 * underlined when current, a 1px ink rule under the lot — and they name the first item
 * **Queue** rather than Inbox. That is the word this app now uses for the screen at `/`:
 * it is a queue of reviews, one row each, and "inbox" was the name of the thing that
 * listed one review three times.
 *
 * "Reviews" has no index of its own in this slice, so it goes to the review that is
 * waiting; the boards' own Reviews link points at the review page, which is a later issue.
 */
const route = useRoute()
const inbox = useReviewDesk().inbox()

/*
 * Whichever review is waiting on the developer, so "Reviews" always lands somewhere worth
 * being: the brief to confirm, else the draft to approve, else the first reply.
 *
 * `null` while the inbox has not answered, and on a database with nothing in it. There is
 * then no review to link to, so the item is a disabled label rather than a link to
 * `/reviews/1` — which on a fresh database was a link to a review that does not exist.
 */
const firstReview = computed(() => {
  const waiting = inbox.data.value
  if (!waiting) return null
  return (
    waiting.briefs_to_confirm[0]?.review_id
    ?? waiting.drafts_to_approve[0]?.review_id
    ?? waiting.author_replies[0]?.review_id
    ?? waiting.in_progress[0]?.review_id
    ?? null
  )
})

const links = computed(() => [
  { label: 'Queue', to: '/', match: (path: string) => path === '/' },
  {
    label: 'Reviews',
    to: firstReview.value == null ? null : `/reviews/${firstReview.value}`,
    match: (path: string) => path.startsWith('/reviews'),
  },
  { label: 'Rules', to: '/rules', match: (path: string) => path.startsWith('/rules') },
])
</script>

<template>
  <div class="shell">
    <a
      class="skip"
      href="#main"
    >Skip to the page</a>
    <header class="bar">
      <div class="name">
        Review Desk
      </div>
      <nav
        class="nav"
        aria-label="Review Desk"
      >
        <template
          v-for="link in links"
          :key="link.label"
        >
          <NuxtLink
            v-if="link.to"
            :to="link.to"
            class="link"
            :class="{ 'link--current': link.match(route.path) }"
            :aria-current="link.match(route.path) ? 'page' : undefined"
          >
            {{ link.label }}
          </NuxtLink>
          <!--
            Nothing to link to. Shown rather than hidden, so the navigation keeps its three
            places and the one that is empty says it is empty.
          -->
          <span
            v-else
            class="link link--none"
            aria-disabled="true"
          >{{ link.label }}</span>
        </template>
      </nav>
      <SampleDataBadge />
    </header>
    <main
      id="main"
      class="main"
    >
      <slot />
    </main>
  </div>
</template>

<style scoped>
.shell {
  min-height: 100vh;
}

.skip {
  position: absolute;
  left: -9999px;
}

.skip:focus {
  left: 8px;
  top: 8px;
  z-index: 2;
  padding: 10px 14px;
  background: var(--rd-surface);
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-sm);
}

.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 28px;
  padding: 14px 40px;
  /* Ink, not the row grey: this is the line that separates the app from the page. */
  border-bottom: 1px solid var(--rd-ink);
}

.name {
  font-family: var(--rd-font-display);
  font-size: 22px;
  font-weight: 500;
  letter-spacing: -0.01em;
}

.nav {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.link {
  display: flex;
  align-items: center;
  padding: 12px;
  color: var(--rd-ink-muted);
  text-decoration: none;
}

.link:hover {
  color: var(--rd-ink);
}

.link--current {
  color: var(--rd-ink);
  font-weight: 600;
  box-shadow: inset 0 -2px 0 var(--rd-ink);
}

.link--none {
  color: var(--rd-line-strong);
  cursor: default;
}

/* The marker sits at the far end, which is where every board puts it. */
.bar :deep(.badge) {
  margin-left: auto;
}

.main {
  max-width: var(--rd-shell-max);
  margin: 0 auto;
  padding: 44px 40px 80px;
}

@media (max-width: 560px) {
  .bar {
    padding: 12px 16px;
  }

  .main {
    padding: 28px 16px 48px;
  }
}
</style>
