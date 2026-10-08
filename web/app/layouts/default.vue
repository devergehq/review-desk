<script setup lang="ts">
/**
 * The shell every screen sits in: the name, the marker that says these records are
 * invented, and the three places you can be.
 *
 * "Reviews" has no index of its own in this slice, so it goes to the review that is
 * waiting; the mockups' fourth item pointed at the flow diagram, which is reference
 * material in `docs/mockups/` rather than a screen.
 */
const route = useRoute()
const { inbox } = useReviewDesk()

/*
 * Whichever review is waiting on the developer, so "Reviews" always lands somewhere
 * worth being: the brief to confirm, else the draft to approve, else the first reply.
 */
const firstReview = computed(() => {
  const waiting = inbox.value
  return (
    waiting.briefs_to_confirm[0]?.review_id
    ?? waiting.drafts_to_approve[0]?.review_id
    ?? waiting.author_replies[0]?.review_id
    ?? waiting.in_progress[0]?.review_id
    ?? 1
  )
})

const links = computed(() => [
  { label: 'Inbox', to: '/', match: (path: string) => path === '/' },
  {
    label: 'Reviews',
    to: `/reviews/${firstReview.value}`,
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
      <SampleDataBadge />
      <nav
        class="nav"
        aria-label="Review Desk"
      >
        <NuxtLink
          v-for="link in links"
          :key="link.to"
          :to="link.to"
          class="link"
          :class="{ 'link--current': link.match(route.path) }"
          :aria-current="link.match(route.path) ? 'page' : undefined"
        >
          {{ link.label }}
        </NuxtLink>
      </nav>
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
  gap: 16px;
  padding: 10px 24px;
  background: var(--rd-surface);
  border-bottom: 1px solid var(--rd-line);
}

.name {
  font-weight: 600;
  font-size: 17px;
}

.nav {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-left: auto;
}

.link {
  display: flex;
  align-items: center;
  padding: 12px 14px;
  border-radius: var(--rd-radius-sm);
  text-decoration: none;
  color: var(--rd-ink-dim);
}

.link:hover {
  background: var(--rd-ground);
  color: var(--rd-ink);
}

.link--current {
  font-weight: 600;
  color: var(--rd-ink);
  background: var(--rd-rail);
}

.main {
  max-width: var(--rd-shell-max);
  margin: 0 auto;
  padding: 28px 24px 48px;
}

@media (max-width: 520px) {
  .bar {
    padding: 10px 16px;
  }

  .main {
    padding: 20px 16px 40px;
  }

  .nav {
    margin-left: 0;
    width: 100%;
  }
}
</style>
