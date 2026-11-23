<script setup lang="ts">
/**
 * The shell every screen sits in: three columns, filling the window at every width.
 *
 * `docs/design.md`, *The application shell* (21 November 2026), and the S1 and S2 screens:
 *
 * | Column | Width | Holds |
 * | -- | -- | -- |
 * | Sidebar | 232px by default | `AppSidebar`: the wordmark, the navigation, every pull request, the status lines |
 * | Main | the rest, never under 480px | The page |
 * | Conversation | 380px by default | **Ask this review**, on a review's pages only. Absent on the lists and Rules |
 *
 * **The two side columns are as wide as the developer drags them** (DEV-1375). Each has a
 * `ColumnHandle` on its inner edge, and the widths are `useColumnWidths`'s, set on the shell
 * as `--rd-sidebar-now` and `--rd-conversation-now` (and `--rd-preview-now`, which the Pull
 * requests preview reads). The limits, and how the side columns give way so the main column
 * keeps 480px, are in `utils/columns.ts`. Under 1,100px the conversation has no handle, and
 * under 700px nothing does: the phone's sidebar keeps the token's 232px.
 *
 * **The conversation column holds *Ask this review*** (DEV-1333). The layout draws the
 * column, `#conversation`, and the review's page teleports its `ConversationPanel` into it,
 * because the page is what holds the review, its step and its tab.
 *
 * **Between 700 and 1,100px the conversation sits under the page**, the main column's full
 * width: beside it, at 1,024, it would leave a review's page 332px.
 *
 * **Under 700px the shell is one column.** The sidebar goes behind a menu button in a bar
 * at the top. The open sidebar behaves as a dialog would: Escape or a press outside it
 * closes it, focus goes into it and comes back to the button, and the page behind is
 * `inert` meanwhile.
 *
 * **Under 700px the conversation is a bottom sheet over the dimmed review** (S14,
 * DEV-1336), opened by the bar's *Ask* on a review's pages. **The sheet is the same
 * column, not a second one**: `#conversation` is what the page teleports its one
 * `ConversationPanel` into, and under 700 that element is drawn fixed to the bottom of the
 * window rather than not at all — so the thread, the ask box's text and the panel's timer
 * are the ones the wider shell has, and nothing here forks the panel. The layout owns only
 * the container: the grabber, the chevron that closes it, the scrim, the safe-area insets.
 * It behaves as the menu does — a dialog while open, Escape or the scrim closes it, focus
 * goes in and comes back, and widening past 700 or going somewhere closes it — so an
 * accepted suggestion that opens a finding shows the finding rather than the sheet over it.
 *
 * **What the old header carried is all still here**: the name is the sidebar's wordmark,
 * the three places are its navigation, and the sample-data marker is at its foot — and in
 * the bar under 700px, so it is on every screen at every width. The skip link stays first.
 *
 * ## A page with a column of its own
 *
 * **A page that draws its own right-hand column asks for it** with
 * `definePageMeta({ split: true })`: the Pull requests list, whose preview is S1's third
 * column (DEV-1325). The main column then has no right-hand padding, so the page's column
 * meets the window's edge and its rule runs the window's height, as the
 * conversation's does on a review.
 *
 * ## The whole window, and prose that does not grow with it
 *
 * **Desktop is the target, and Review Desk uses the whole window** (Patrick, 23 November
 * 2026, DEV-1372). The shell has no outer cap and is not centred: the sidebar is against the
 * window's left edge and the conversation against its right, and the main column takes
 * everything between them, on every page — a document's as much as a workbench's. What stops
 * a paragraph growing with the window is its own measure, `--rd-measure` or
 * `--rd-measure-recorded`, never the column it sits in.
 */
import { isReviewPath } from '../utils/columns'

const route = useRoute()

/** Whether this page draws a column of its own at the right-hand edge. */
const split = computed(() => route.meta.split === true)

/** A review's pages, which are the ones the conversation sits beside. */
const onAReview = computed(() => isReviewPath(route.path))

/** The side columns' widths, as the custom properties the grid reads (DEV-1375). */
const { style: columnWidths } = useColumnWidths()

/** Whether the sidebar is open over the page, under 700px. Above it, it is always there. */
const menuOpen = ref(false)

const menuButton = ref<HTMLButtonElement | null>(null)
const sidebar = ref<HTMLElement | null>(null)

/** Open it and put focus in it; the first link is Pull requests. */
async function openMenu() {
  menuOpen.value = true
  await nextTick()
  sidebar.value?.querySelector<HTMLElement>('a, button, summary')?.focus()
}

/** Close it, and give focus back to the button that opened it, if focus was inside. */
function closeMenu(returnFocus = true) {
  if (!menuOpen.value) return
  menuOpen.value = false
  if (returnFocus) menuButton.value?.focus()
}

/** Whether the conversation is open as a sheet over the review, under 700px. */
const sheetOpen = ref(false)

const askButton = ref<HTMLButtonElement | null>(null)
const sheetClose = ref<HTMLButtonElement | null>(null)

/**
 * Open it and put focus on its chevron — not in the ask box, which on a phone would put the
 * keyboard over the thread the developer opened the sheet to read.
 */
async function openSheet() {
  closeMenu(false)
  sheetOpen.value = true
  await nextTick()
  sheetClose.value?.focus()
}

/**
 * Close it, and give focus back to *Ask* — after the bar has lost its `inert`, which is the
 * next render: an inert button refuses focus.
 */
async function closeSheet(returnFocus = true) {
  if (!sheetOpen.value) return
  sheetOpen.value = false
  if (!returnFocus) return
  await nextTick()
  askButton.value?.focus()
}

/** Anything over the page under 700px: the sheet or the menu, never both. */
const overlaid = computed(() => menuOpen.value || sheetOpen.value)

function onKey(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  closeMenu()
  void closeSheet()
}

/*
 * **Widening past 700px closes it**, without moving focus: the button and the scrim are not
 * drawn above 700, so an open menu there would leave `main` inert with nothing on screen
 * to close it — a phone turned from 390 to 844, or a window dragged wider (#86's
 * self-review, round 2). The sheet the same, for the same reason.
 */
let phone: MediaQueryList | null = null
function onWidth(event: MediaQueryListEvent) {
  if (event.matches) return
  closeMenu(false)
  void closeSheet(false)
}

/*
 * **The page under an open sheet does not scroll.** `main` is `inert`, which stops a press
 * but not a drag, and a drag on the scrim would otherwise scroll the review behind it.
 */
watch(sheetOpen, (open) => {
  document.documentElement.classList.toggle('rd-sheet-open', open)
})

onMounted(() => {
  window.addEventListener('keydown', onKey)
  phone = window.matchMedia?.('(max-width: 699px)') ?? null
  phone?.addEventListener('change', onWidth)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  phone?.removeEventListener('change', onWidth)
  document.documentElement.classList.remove('rd-sheet-open')
})

/*
 * Going somewhere is what the menu is for, so arriving closes it. Focus stays on the page.
 * A cite, *Open F9* or an accepted suggestion goes somewhere from the sheet, and the
 * developer pressed it to see that place, so arriving closes the sheet too.
 */
watch(() => route.fullPath, () => {
  closeMenu(false)
  void closeSheet(false)
})
</script>

<template>
  <div
    class="shell"
    :class="{ 'shell--review': onAReview, 'shell--menu-open': menuOpen, 'shell--sheet-open': sheetOpen }"
    :style="columnWidths"
  >
    <!--
      Inert while anything is over the page: a skip link under the scrim is one Tab out of
      a modal sheet, and following it moves the hash and closes the sheet.
    -->
    <a
      class="skip"
      href="#main"
      :inert="overlaid || undefined"
    >Skip to the page</a>

    <!-- Only drawn under 700px, where the sidebar is behind it. -->
    <div
      class="topbar"
      :inert="sheetOpen || undefined"
    >
      <button
        ref="menuButton"
        type="button"
        class="menu"
        :aria-expanded="menuOpen"
        aria-controls="sidebar"
        @click="menuOpen ? closeMenu() : openMenu()"
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
      <SampleDataBadge />
      <!--
        **Ask opens the conversation**, on a review's pages: the bar is the one thing on a
        phone that is always in reach, as the column is beside every tab above 700px.
      -->
      <button
        v-if="onAReview"
        ref="askButton"
        type="button"
        class="ask"
        aria-haspopup="dialog"
        :aria-expanded="sheetOpen"
        aria-controls="conversation"
        @click="openSheet()"
      >
        Ask
      </button>
    </div>

    <!--
      Under 700px, with the menu or the sheet open: the page behind it, dimmed, which a
      press closes it from.
    -->
    <div
      v-if="overlaid"
      class="scrim"
      :class="{ 'scrim--sheet': sheetOpen }"
      aria-hidden="true"
      @click="sheetOpen ? closeSheet() : closeMenu()"
    />

    <aside
      id="sidebar"
      ref="sidebar"
      class="sidebar"
      aria-label="Pull requests and places"
    >
      <AppSidebar />
    </aside>
    <ColumnHandle
      column="sidebar"
      edge="right"
      label="Resize the sidebar"
      controls="sidebar"
    />

    <main
      id="main"
      class="main"
      :inert="overlaid || undefined"
      :class="{ 'main--split': split }"
    >
      <div class="page">
        <slot />
      </div>
    </main>

    <!--
      The conversation's column, on a review's pages. The page teleports *Ask this review*
      into it (DEV-1333); a complementary landmark, named by the panel's own heading.

      Under 700px it is the bottom sheet (DEV-1336), and a dialog while it is open. Its
      own child is the sheet's head, and the teleported panel lands after it.

      **The head is always in the DOM, hidden by CSS until the sheet opens**, because the
      sheet is a grid of the head over the panel and the order is what places them: a head
      inserted on open lands after the panel the page teleported in, and the panel then
      takes the head's short row. Only the chevron's drawing waits for the sheet, so a page
      that counts its drawings above 700 counts none of the sheet's.
    -->
    <aside
      v-if="onAReview"
      id="conversation"
      class="shell-conversation"
      aria-label="Ask this review"
      :role="sheetOpen ? 'dialog' : undefined"
      :aria-modal="sheetOpen || undefined"
    >
      <div class="sheet-head">
        <span
          class="grabber"
          aria-hidden="true"
        />
        <button
          ref="sheetClose"
          type="button"
          class="sheet-close"
          aria-label="Close the conversation"
          @click="closeSheet()"
        >
          <svg
            v-if="sheetOpen"
            width="18"
            height="18"
            viewBox="0 0 18 18"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M4 7l5 5 5-5" /></svg>
        </button>
      </div>
    </aside>
    <ColumnHandle
      v-if="onAReview"
      column="conversation"
      edge="left"
      label="Resize the conversation"
      controls="conversation"
    />
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--rd-sidebar-now, var(--rd-sidebar-width)) minmax(0, 1fr);
  min-height: 100vh;
}

.shell--review {
  grid-template-columns:
    var(--rd-sidebar-now, var(--rd-sidebar-width)) minmax(0, 1fr)
    var(--rd-conversation-now, var(--rd-conversation-width));
}

/* Under 1,100 the conversation sits under the page rather than beside it. See the note above. */
@media (max-width: 1099px) {
  .shell--review {
    grid-template-columns: var(--rd-sidebar-now, var(--rd-sidebar-width)) minmax(0, 1fr);
  }

  .shell--review .sidebar {
    grid-row: 1 / span 2;
  }

  .shell--review .shell-conversation {
    grid-column: 2;
    position: static;
    height: auto;
    border-top: 1px solid var(--rd-line);
    border-left: 0;
  }
}

.scrim {
  display: none;
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

.main--split {
  padding-right: 0;
  padding-bottom: 0;
}

.shell-conversation {
  position: sticky;
  top: 0;
  height: 100vh;
  border-left: 1px solid var(--rd-line);
}

/* *Ask* and the sheet's head, the grabber and the chevron, are the phone's alone. */
.sheet-head,
.ask {
  display: none;
}

:global(html.rd-sheet-open) {
  overflow: hidden;
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

  .topbar :deep(.badge) {
    margin-left: auto;
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1;
    display: block;
    background: rgb(20 23 26 / 24%);
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

  .main,
  .main--split {
    padding: 24px 16px 48px;
  }

  .shell-conversation {
    display: none;
  }

  .ask {
    display: inline-flex;
    flex: none;
    align-items: center;
    min-height: var(--rd-control-min);
    margin-left: auto;
    padding: 0 14px;
    color: var(--rd-accent-dark);
    font: inherit;
    font-weight: 600;
    background: var(--rd-accent-wash);
    border: 0;
    border-radius: var(--rd-radius-sm);
    cursor: pointer;
  }

  /* The badge takes the push to the right when it is drawn, and Ask sits after it. */
  .topbar :deep(.badge) + .ask {
    margin-left: 0;
  }

  .ask:hover {
    background: var(--rd-rail-pressed);
  }

  /* S14 dims the review further than the menu does: the sheet is read, not glanced at. */
  .scrim--sheet {
    z-index: 3;
    background: rgb(20 23 26 / 40%);
  }

  /*
   * **The sheet.** Fixed to the window's foot, the top quarter of the review showing above
   * it as S14 draws it, and inside every safe-area inset the browser reports — the home
   * indicator under the composer, a notch to either side, the status bar above. The panel
   * is its second row and takes the rest of the height, so the panel's own column puts the
   * thread in the middle to scroll and the composer at the bottom, pinned.
   */
  .shell--sheet-open .shell-conversation {
    position: fixed;
    top: max(calc(env(safe-area-inset-top, 0px) + 48px), 22vh);
    top: max(calc(env(safe-area-inset-top, 0px) + 48px), 22dvh);
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 4;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: auto;
    padding:
      0 env(safe-area-inset-right, 0px) env(safe-area-inset-bottom, 0px)
      env(safe-area-inset-left, 0px);
    background: var(--rd-ground);
    border: 0;
    border-radius: var(--rd-radius-section) var(--rd-radius-section) 0 0;
    box-shadow: var(--rd-lift-raised);
  }

  /*
   * The grabber in the middle of a strip over the panel's own head, and the chevron hung
   * from that strip's right end, level with the panel's title as S14 draws it. The grabber
   * is drawn, not pressed: the chevron, Escape and the scrim close it. A positioned child
   * paints over the panel's head, which is not positioned, so the chevron takes the press.
   */
  .shell--sheet-open .sheet-head {
    display: block;
    position: relative;
    height: 24px;
  }

  .grabber {
    position: absolute;
    top: 10px;
    left: 50%;
    width: 36px;
    height: 4px;
    border-radius: var(--rd-radius-pill);
    background: var(--rd-line-strong);
    transform: translateX(-50%);
  }

  .sheet-close {
    position: absolute;
    top: 10px;
    right: 8px;
    z-index: 1;
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

  .sheet-close:hover {
    background: var(--rd-rail-pressed);
  }

  .ask:focus-visible,
  .sheet-close:focus-visible {
    outline: 3px solid var(--rd-focus-ring);
    outline-offset: 2px;
  }
}
</style>
