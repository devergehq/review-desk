<script setup lang="ts">
/**
 * The shell every screen sits in: three columns, capped at `--rd-shell-wide`.
 *
 * `docs/design.md`, *The application shell* (21 November 2026), and the S1 and S2 screens:
 *
 * | Column | Width | Holds |
 * | -- | -- | -- |
 * | Sidebar | 232px | `AppSidebar`: the wordmark, the navigation, every pull request, the status lines |
 * | Main | the rest | The page |
 * | Conversation | 380px | **Ask this review**, on a review's pages only. Absent on the lists and Rules |
 *
 * **The conversation column is drawn empty**, as a placeholder: the second stack fills it
 * (DEV-1322's plan). It is drawn now so that every page laid out from here on is laid out
 * at the width it will have, and a page that fits beside an empty column fits beside a full
 * one.
 *
 * **Under 700px the shell is one column.** The sidebar goes behind a menu button in a bar
 * at the top, and the conversation column is not drawn at all: on a phone it is a bottom
 * sheet (S14), which is the second stack's too.
 *
 * **What the old header carried is all still here**: the name is the sidebar's wordmark,
 * the three places are its navigation, and the sample-data marker is at its foot. The skip
 * link stays first in the document.
 *
 * ## Wide pages
 *
 * A page still asks for the wider main column itself, with `definePageMeta({ wide: true })`
 * — read off the route, so a page that is wide is wide from its first frame. A document's
 * main column is capped at `--rd-shell-max`; a workbench's takes all the main column has.
 */
const route = useRoute()

/** Whether this page asked to be a workbench rather than a document. */
const wide = computed(() => route.meta.wide === true)

/** A review's pages, which are the ones the conversation sits beside. */
const onAReview = computed(() => /^\/reviews\/\d+(?:\/|$)/.test(route.path))

/** Whether the sidebar is open over the page, under 700px. Above it, it is always there. */
const menuOpen = ref(false)

/* Going somewhere is what the menu is for, so arriving closes it. */
watch(() => route.fullPath, () => {
  menuOpen.value = false
})
</script>

<template>
  <div
    class="shell"
    :class="{ 'shell--review': onAReview, 'shell--menu-open': menuOpen }"
  >
    <a
      class="skip"
      href="#main"
    >Skip to the page</a>

    <!-- Only drawn under 700px, where the sidebar is behind it. -->
    <div class="topbar">
      <button
        type="button"
        class="menu"
        :aria-expanded="menuOpen"
        aria-controls="sidebar"
        @click="menuOpen = !menuOpen"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 18 18"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          aria-hidden="true"
        ><path d="M3 5h12M3 9h12M3 13h12" /></svg>
        <span class="rd-visually-hidden">Menu</span>
      </button>
      <span class="topbar-name">Review Desk</span>
    </div>

    <aside
      id="sidebar"
      class="sidebar"
      aria-label="Pull requests and places"
    >
      <AppSidebar />
    </aside>

    <main
      id="main"
      class="main"
    >
      <div
        class="page"
        :class="{ 'page--wide': wide }"
      >
        <slot />
      </div>
    </main>

    <!--
      The conversation's column, on a review's pages. Empty until the second stack fills it,
      so it is hidden from assistive technology: an empty landmark is a stop with nothing at
      it.
    -->
    <div
      v-if="onAReview"
      class="shell-conversation"
      aria-hidden="true"
    />
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--rd-sidebar-width) minmax(0, 1fr);
  max-width: var(--rd-shell-wide);
  min-height: 100vh;
  margin: 0 auto;
}

.shell--review {
  grid-template-columns: var(--rd-sidebar-width) minmax(0, 1fr) var(--rd-conversation-width);
}

.skip {
  position: absolute;
  left: -9999px;
}

.skip:focus {
  left: 8px;
  top: 8px;
  z-index: 3;
  padding: 10px 14px;
  background: var(--rd-surface);
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-sm);
}

.topbar {
  display: none;
}

/*
 * The sidebar stays in view as the page scrolls, as S1 and S2 draw it: its foot is the
 * status lines, and a reader halfway down a long review still sees them.
 */
.sidebar {
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
  background: var(--rd-ground);
  border-right: 1px solid var(--rd-line);
}

.main {
  min-width: 0;
  padding: 36px 40px 80px;
}

.page {
  max-width: var(--rd-shell-max);
}

.page--wide {
  max-width: none;
}

.shell-conversation {
  position: sticky;
  top: 0;
  height: 100vh;
  border-left: 1px solid var(--rd-line);
}

@media (max-width: 699px) {
  .shell,
  .shell--review {
    grid-template-columns: minmax(0, 1fr);
  }

  .topbar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    background: var(--rd-page);
    border-bottom: 1px solid var(--rd-line);
  }

  .menu {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--rd-control-min);
    height: var(--rd-control-min);
    padding: 0;
    color: var(--rd-ink);
    background: none;
    border: 0;
    border-radius: var(--rd-radius-sm);
    cursor: pointer;
  }

  .menu:hover {
    background: var(--rd-rail-pressed);
  }

  /*
   * The bar's name is the sans: the wordmark proper is the sidebar's, and the one serif in
   * the app is that one element.
   */
  .topbar-name {
    font-size: var(--rd-type-lead);
    font-weight: 600;
  }

  .sidebar {
    position: fixed;
    top: calc(var(--rd-control-min) + 9px);
    bottom: 0;
    left: 0;
    z-index: 2;
    display: none;
    width: min(var(--rd-sidebar-width), 100vw);
    height: auto;
    box-shadow: var(--rd-lift-raised);
  }

  .shell--menu-open .sidebar {
    display: block;
  }

  .main {
    padding: 24px 16px 48px;
  }

  .shell-conversation {
    display: none;
  }
}
</style>
