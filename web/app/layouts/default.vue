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
 * **"Reviews" is a page now, and until DEV-930 it was an action.** It went to whichever
 * review happened to be waiting — the brief to confirm, else the draft to approve, else the
 * first reply — and greyed itself out on a database with none. That was the best available
 * answer while there was no index to point at: it meant the one item in the header could
 * not take a reader to a *particular* review, which is the whole of what Patrick asked for
 * after posting review 2. It points at `/reviews`.
 *
 * Two things went with it, and both are a gain. The layout no longer reads `GET /inbox` on
 * every page — it did so only to work out that link, so the header now renders with the
 * server stopped. And the disabled-label branch is gone, because a list is somewhere worth
 * being on a database with nothing in it: it says so.
 *
 * **`current` and `section` are two questions**, and the item needs both. The underline
 * marks the part of the app you are in, so `/reviews/3/understand` still underlines
 * Reviews. `aria-current="page"` is a narrower claim — *this* is the page — and setting it
 * on a review page would tell a screen reader it was on the list.
 */
const route = useRoute()

const links = computed(() => [
  {
    label: 'Queue',
    to: '/',
    current: (path: string) => path === '/',
    section: (path: string) => path === '/',
  },
  {
    label: 'Reviews',
    to: '/reviews',
    current: (path: string) => path === '/reviews',
    section: (path: string) => path.startsWith('/reviews'),
  },
  {
    label: 'Rules',
    to: '/rules',
    current: (path: string) => path === '/rules',
    section: (path: string) => path.startsWith('/rules'),
  },
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
        <NuxtLink
          v-for="link in links"
          :key="link.label"
          :to="link.to"
          class="link"
          :class="{ 'link--current': link.section(route.path) }"
          :aria-current="link.current(route.path) ? 'page' : undefined"
        >
          {{ link.label }}
        </NuxtLink>
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
