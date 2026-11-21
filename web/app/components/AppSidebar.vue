<script setup lang="ts">
/**
 * The shell's first column: the name, where you can go, every review on the desk,
 * and what is connected.
 *
 * `docs/design.md`, *The application shell*, and the S1 and S2 screens. Four parts, top to
 * bottom:
 *
 * | Part | Holds |
 * | -- | -- |
 * | The wordmark | "Review Desk", **the one serif left in the app** — Patrick, 21 November 2026 |
 * | Navigation | Pull requests, Reviews, Rules |
 * | The groups | Every review on the desk, one row per pull request, under four headings |
 * | The status lines | When GitHub was last read, and whether Allele answered |
 *
 * **Reviews stays in the navigation until the list that replaces it lands** (DEV-1325, S1).
 * The S screens draw two items because by then Queue and Reviews are one page; until they
 * are, taking the link out would leave `/reviews` with no way in, and a page may fold what
 * it draws but never removes any of it.
 *
 * **It reads `GET /inbox` through the shared resource**, so on the queue — which reads the
 * same document — there is still one read, not two. That was the reason DEV-930 took the
 * layout's read away; `useReviewDesk`'s `sharing` is what makes it safe to have one again.
 * And it is quiet when the read fails: one muted line, never a notice, because the page
 * beside it says what went wrong in full and the sidebar saying it too would be two
 * alarms for one fault.
 */
import type { SidebarGroup, SidebarRow } from '~/utils/sidebar'

const route = useRoute()
const desk = useReviewDesk()
const inbox = desk.inbox()
const finished = desk.reviews(computed(() => ALL_REPOSITORIES), computed(() => 'finished' as const))

const groups = computed<SidebarGroup[] | null>(() =>
  inbox.data.value ? sidebarGroups(inbox.data.value, finished.data.value) : null)

/** The count beside **Pull requests**: everything on the desk that has not left it. */
const onTheDesk = computed(() =>
  groups.value?.filter(group => group.id !== 'done')
    .reduce((sum, group) => sum + group.rows.length, 0) ?? null)

const proposedRules = computed(() => inbox.data.value?.proposed_rules.length ?? null)

const githubRead = computed(() => githubReadAt(inbox.data.value, finished.data.value))

/** The review this page is about, or null off a review's pages. */
const reviewId = computed(() => {
  const match = /^\/reviews\/(\d+)(?:\/|$)/.exec(route.path)
  return match ? Number(match[1]) : null
})

const NAV = [
  { label: 'Pull requests', to: '/', icon: 'pulls' },
  { label: 'Reviews', to: '/reviews', icon: 'reviews' },
  { label: 'Rules', to: '/rules', icon: 'rules' },
] as const

/**
 * Which item the page belongs to, which is what the item's highlight marks.
 *
 * **A review's own pages belong to Pull requests**, which is where S2 highlights them: the
 * review is one of the pull requests listed under it. Reviews owns its own list and nothing
 * under it.
 */
function isSection(to: string, path: string): boolean {
  if (to === '/') return path === '/' || reviewId.value != null
  return path === to
}

const navCount = (to: string) =>
  to === '/' ? onTheDesk.value : to === '/rules' ? proposedRules.value : null

const isOpenRow = (row: SidebarRow) => row.reviewId != null && row.reviewId === reviewId.value

/** A folded group opens itself when it holds the review being read. */
const holdsOpenRow = (group: SidebarGroup) => group.rows.some(isOpenRow)
</script>

<template>
  <div class="sidebar">
    <div class="wordmark">
      Review Desk
    </div>

    <nav
      class="nav"
      aria-label="Review Desk"
    >
      <NuxtLink
        v-for="item in NAV"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ 'nav-item--current': isSection(item.to, route.path) }"
        :aria-current="route.path === item.to ? 'page' : undefined"
      >
        <svg
          class="icon"
          width="18"
          height="18"
          viewBox="0 0 18 18"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <template v-if="item.icon === 'pulls'">
            <circle
              cx="4.5"
              cy="4"
              r="1.75"
            />
            <circle
              cx="4.5"
              cy="14"
              r="1.75"
            />
            <circle
              cx="13.5"
              cy="14"
              r="1.75"
            />
            <path d="M4.5 5.75v6.5M13.5 12.25V7.5a2 2 0 00-2-2H9M10.5 3.5L8.5 5.5l2 2" />
          </template>
          <template v-else-if="item.icon === 'reviews'">
            <path d="M3 4.5h12M3 9h12M3 13.5h8" />
          </template>
          <template v-else>
            <rect
              x="3.5"
              y="2.5"
              width="11"
              height="13"
              rx="1.5"
            />
            <path d="M6.5 9l1.75 1.75L11.5 7.5" />
          </template>
        </svg>
        <span class="nav-label">{{ item.label }}</span>
        <span
          v-if="navCount(item.to) != null"
          class="nav-count"
        >{{ navCount(item.to) }}</span>
      </NuxtLink>
    </nav>

    <div
      v-if="groups"
      class="groups"
    >
      <div
        v-for="group in groups"
        :key="group.id"
        class="group"
        :class="`group--${group.id}`"
      >
        <!--
          **Waiting on you is open; the other three fold**, which is how S1 and S2 draw them.
          An empty group does not fold — there is nothing to open — and says "None" under its
          heading, so an empty group is told from a missing one. **Every group's name is an
          `h2`**, the folded ones inside their `summary`, so heading navigation finds all four.
        -->
        <template v-if="group.id === 'waiting_on_you' || group.rows.length === 0">
          <h2
            class="group-heading"
            :class="{ 'group-heading--first': group.id === 'waiting_on_you' }"
          >
            <span>{{ group.label }}</span>
            <span
              v-if="group.id !== 'waiting_on_you'"
              class="group-count"
            >0</span>
          </h2>
          <p
            v-if="group.rows.length === 0"
            class="none"
          >
            None
          </p>
          <SidebarRows
            v-else
            :rows="group.rows"
            :open-review-id="reviewId"
          />
        </template>
        <details
          v-else
          class="fold"
          :open="holdsOpenRow(group) || undefined"
        >
          <summary>
            <h2 class="group-heading">
              <span>{{ group.label }}</span>
              <span class="group-count">{{ group.rows.length }}</span>
            </h2>
          </summary>
          <SidebarRows
            :rows="group.rows"
            :open-review-id="reviewId"
          />
        </details>
      </div>
    </div>
    <p
      v-else-if="inbox.error.value"
      class="none unread"
    >
      The pull requests could not be read.
    </p>

    <footer class="status">
      <p
        v-if="githubRead"
        class="status-line"
      >
        <span
          class="dot dot--on"
          aria-hidden="true"
        />
        GitHub read {{ sinceLabel(githubRead) }}
      </p>
      <SidebarAgentLine
        v-if="reviewId != null"
        :key="reviewId"
        :review-id="reviewId"
      />
      <SampleDataBadge />
    </footer>
  </div>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 100%;
  padding: 22px 14px 18px;
  font-size: var(--rd-type-body);
}

/* The one serif in the app. `--rd-font-wordmark` has this rule as its only caller. */
.wordmark {
  padding: 0 8px;
  font-family: var(--rd-font-wordmark);
  font-size: var(--rd-type-title);
  font-weight: 400;
  letter-spacing: -0.01em;
  line-height: 1.2;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: var(--rd-control-min);
  padding: 0 10px;
  border: 1px solid transparent;
  border-radius: var(--rd-radius-sm);
  color: var(--rd-ink-dim);
  text-decoration: none;
  font-size: var(--rd-type-lead);
}

.nav-item:hover {
  color: var(--rd-ink);
  background: var(--rd-rail-pressed);
}

.nav-item--current {
  color: var(--rd-ink);
  font-weight: 600;
  background: var(--rd-surface);
  border-color: var(--rd-line);
}

.icon {
  flex: none;
}

.nav-label {
  flex: 1;
}

.nav-count {
  color: var(--rd-ink-muted);
  font-size: var(--rd-type-ui);
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}

.groups {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.group-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 36px;
  padding: 0 10px;
  margin: 0;
  color: var(--rd-ink-dim);
  font-size: var(--rd-type-body);
  font-weight: 500;
}

/* The open group's heading is a label, as S1 draws it, not a fold. */
.group-heading--first {
  min-height: 28px;
  color: var(--rd-ink-muted);
  font-size: var(--rd-type-label);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.group-count {
  color: var(--rd-ink-muted);
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}

.fold > summary {
  min-height: var(--rd-control-min);
  list-style: none;
  cursor: pointer;
  border-radius: var(--rd-radius-xs);
}

.fold > summary::-webkit-details-marker {
  display: none;
}

/* A chevron, drawn, as `.rd-fold` draws its triangle: it needs no glyph and turns with the fold. */
.fold > summary .group-heading > span:first-child::before {
  content: '';
  display: inline-block;
  width: 6px;
  height: 6px;
  margin: 0 12px 2px 2px;
  border-right: 1.5px solid currentColor;
  border-bottom: 1.5px solid currentColor;
  transform: rotate(-45deg);
  transition: transform 120ms ease;
}

.fold[open] > summary .group-heading > span:first-child::before {
  transform: rotate(45deg);
}

@media (prefers-reduced-motion: reduce) {
  .fold > summary .group-heading > span:first-child::before {
    transition: none;
  }
}

.fold > summary:hover {
  background: var(--rd-rail-pressed);
}

.none {
  margin: 0;
  padding: 2px 10px 6px;
  color: var(--rd-ink-faint);
  font-size: var(--rd-type-ui);
}

.unread {
  padding: 0 10px;
}

.status {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: auto;
  padding: 14px 10px 0;
  border-top: 1px solid var(--rd-line);
}

.status-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  color: var(--rd-ink-dim);
  font-size: var(--rd-type-ui);
}

.status :deep(.badge) {
  align-self: flex-start;
  margin-top: 6px;
}

.dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
}

.dot--on {
  background: var(--rd-accent);
}
</style>
