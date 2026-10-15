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

const LINKS = [
  { label: 'Queue', to: '/' },
  { label: 'Reviews', to: '/reviews' },
  { label: 'Rules', to: '/rules' },
]

/** Whether this is the page being read. The narrower claim, and `aria-current`'s. */
const isCurrent = (to: string, path: string) => path === to

/**
 * Whether this is the part of the app being read, which is what the underline marks.
 *
 * **The Queue is the exception and it is stated here rather than left to look like an
 * inconsistency**: every path starts with `/`, so the prefix rule would underline Queue on
 * every screen. It owns one path and no children.
 */
const isSection = (to: string, path: string) =>
  (to === '/' ? path === '/' : path.startsWith(to))

/**
 * Whether this page asked for the wider shell.
 *
 * The bar above is full-bleed and always has been, so nothing moves out of line with it —
 * at a 1,440px window the wide page's content starts at the same 40px gutter the app's own
 * name does, which is closer to the bar than the 1,240px shell ever was.
 */
const wide = computed(() => route.meta.wide === true)
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
          v-for="link in LINKS"
          :key="link.label"
          :to="link.to"
          class="link"
          :class="{ 'link--current': isSection(link.to, route.path) }"
          :aria-current="isCurrent(link.to, route.path) ? 'page' : undefined"
        >
          {{ link.label }}
        </NuxtLink>
      </nav>
      <SampleDataBadge />
    </header>
    <!--
      **One page may be wider than the shell, and says so itself.**

      `definePageMeta({ wide: true })` on the code review page and the draft: both are
      workbenches — a list beside a finding, an outline beside a preview — and at 1,240px
      the pane with the code in it shows about 45 characters. Read off the route rather
      than set by the page into a store, so a page that is wide is wide from its first
      frame and a page that is not never has to say so.
    -->
    <main
      id="main"
      class="main"
      :class="{ 'main--wide': wide }"
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

/*
 * The workbench screens. Capped rather than unbounded: a review read on a 27-inch monitor
 * is still read by one pair of eyes, and the cap is what the token's own note argues.
 */
.main--wide {
  max-width: var(--rd-shell-wide);
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
