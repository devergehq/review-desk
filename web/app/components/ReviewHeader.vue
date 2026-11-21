<script setup lang="ts">
import type { ReviewDetail } from '~/types/desk'
import type { ReviewTab } from '~/utils/reviewTabs'

/**
 * **The review's header: which review, where it stands, what to do next, and the tabs.**
 * DEV-1324, the S2 header.
 *
 * ```
 *  Pull requests › acme/portal
 *  #514  Give a scheduled export its own context              [ Sign off and post → ]
 *  (Draft ready)  round 1 · at 7f3c2d1 · ABC-402 · 3 h waiting on you
 *  Overview   Understanding   Findings 12   Draft   Replies —
 * ```
 *
 * It replaces the step trail and the top line every step page drew. The rules are
 * `docs/design.md`'s, *The application shell*, and each is kept in one place:
 *
 * - **One state word**, from `stateOfReview` and `stateWord` — the sidebar's function, so a
 *   review cannot be *Draft ready* there and *Posted* here.
 * - **One next action, top right**, from `primaryAction`, which names what it acts on and
 *   answers nothing for a review with nothing to do.
 * - **No taller than 120px** at 1,600, which `e2e/shell.spec.ts` measures.
 * - **Times only when measured.** The facts line draws a time only when the record holds
 *   one, and says nothing in its place otherwise.
 *
 * **The two ways out stay.** The number links to the pull request and the ticket's id to
 * the tracker, as the old top line did since DEV-936, and both say they open elsewhere.
 */
const props = defineProps<{
  detail: ReviewDetail
  /** Which tab the page is. `aria-current` names it. */
  tab: ReviewTab
}>()

const review = computed(() => props.detail.review)

const pullRequest = computed(() => prUrl(review.value.repo, review.value.pr))
const ticket = computed(() => ticketUrl(review.value.ticket_url))

const state = computed(() => stateOfReview(props.detail))
const word = computed(() => stateWord(state.value))

/** The three states that wait on the developer draw their word in the accent's wash. */
const waitsOnYou = computed(() =>
  state.value === 'brief_ready' || state.value === 'draft_ready' || state.value === 'replies_in',
)

const toAnswer = computed(() => repliesToAnswer(props.detail.outcomes))
const action = computed(() => primaryAction(review.value.id, state.value, toAnswer.value))

/**
 * The facts line, each fact only when the record holds it.
 *
 * The round and the commit read are always held. The size is held when the session wrote
 * it. **The one time is the total waiting on the developer**, and only while the review is
 * waiting on them and the record measured it — `timings` says `null` for a span it did not
 * measure, and that is drawn as nothing rather than as a guess.
 */
const facts = computed(() => {
  const parts: string[] = [`round ${review.value.round}`]
  const commit = shortCommit(review.value.head_sha)
  if (commit) parts.push(`at ${commit}`)
  if (review.value.changed_lines != null) {
    parts.push(count(review.value.changed_lines, 'line changed', 'lines changed'))
  }
  return parts
})

const waiting = computed(() =>
  waitsOnYou.value ? durationLabel(props.detail.timings?.waiting_on_you_seconds) : null,
)

const findings = computed(() => findingsCount(props.detail))
const replies = computed(() => toAnswer.value.length)

const tabs = computed(() =>
  REVIEW_TABS.map(one => ({
    ...one,
    to: tabPath(review.value.id, one.tab),
    current: one.tab === props.tab,
  })),
)
</script>

<template>
  <header
    class="review-header"
    data-testid="review-header"
  >
    <div class="head">
      <div class="who">
        <nav
          class="crumb rd-meta"
          aria-label="Breadcrumb"
        >
          <NuxtLink to="/">Pull requests</NuxtLink>
          <span
            class="sep"
            aria-hidden="true"
          >›</span>
          <span class="repo">{{ review.repo }}</span>
        </nav>

        <h1 class="title">
          <a
            v-if="pullRequest"
            class="number header-out"
            :href="pullRequest"
            target="_blank"
            rel="noopener noreferrer"
          >#{{ review.pr }}<span class="rd-visually-hidden">, {{ review.repo }} on GitHub, opens in a new tab</span></a>
          <span
            v-else
            class="number"
          >#{{ review.pr }}</span>
          <span class="name">{{ review.title ?? `${review.repo} #${review.pr}` }}</span>
        </h1>

        <p class="facts rd-meta">
          <DeskPill
            :tone="waitsOnYou ? 'go-quiet' : 'quiet'"
            data-testid="state-word"
          >
            {{ word }}
          </DeskPill>
          <span class="fact-list">
            <template
              v-for="(fact, index) in facts"
              :key="fact"
            >
              <template v-if="index > 0"> · </template>{{ fact }}
            </template>
            <template v-if="review.ticket_id">
              ·
              <a
                v-if="ticket"
                class="header-out"
                :href="ticket"
                target="_blank"
                rel="noopener noreferrer"
              >{{ review.ticket_id }}<span class="rd-visually-hidden">, on the tracker, opens in a new tab</span></a>
              <template v-else>{{ review.ticket_id }}</template>
            </template>
            <template v-if="waiting"> · {{ waiting }} waiting on you</template>
          </span>
        </p>
      </div>

      <NuxtLink
        v-if="action"
        class="action"
        data-testid="primary-action"
        :to="action.to"
      >
        {{ action.label }}
        <span aria-hidden="true">→</span>
      </NuxtLink>
    </div>

    <nav
      class="tabs"
      aria-label="This review"
    >
      <NuxtLink
        v-for="one in tabs"
        :key="one.tab"
        class="tab"
        :class="{ 'tab--current': one.current }"
        :to="one.to"
        :aria-current="one.current ? 'page' : undefined"
        :data-tab="one.tab"
      >
        {{ one.label }}
        <span
          v-if="one.tab === 'findings' && findings != null"
          class="count"
        >{{ findings }}</span>
        <span
          v-if="one.tab === 'replies'"
          class="count"
        ><template v-if="replies > 0">{{ replies }}</template><template v-else><span aria-hidden="true">—</span><span class="rd-visually-hidden">, none to answer</span></template></span>
      </NuxtLink>
    </nav>
  </header>
</template>

<style scoped>
/*
 * **120px at 1,600, and measured.** Crumb 18, title 32, facts 24, a 4px gap, tabs 34 and
 * the rule under them: 113. `e2e/shell.spec.ts` fails a header that grows past 120.
 */
.review-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  border-bottom: 1px solid var(--rd-line);
}

.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 24px;
  min-width: 0;
}

.who {
  display: flex;
  flex-direction: column;
  gap: 0;
  min-width: 0;
}

.crumb {
  display: flex;
  flex-wrap: wrap;
  gap: 0 8px;
  align-items: center;
  min-width: 0;
  margin: 0;
  line-height: 18px;
  color: var(--rd-ink-muted);
}

.crumb a {
  color: inherit;
}

/*
 * **A repository name breaks anywhere**, because a flex item's min-content width is its
 * longest word and a 38-character name is one word: `e2e/header-links.spec.ts` measures it
 * at 390.
 */
.repo {
  min-width: 0;
  overflow-wrap: anywhere;
}

.title {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin: 0;
  font-size: var(--rd-type-title);
  line-height: 32px;
  font-weight: 600;
  min-width: 0;
}

.number {
  flex: none;
  color: var(--rd-ink-muted);
  font-weight: 500;
}

.name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.facts {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  line-height: 24px;
  color: var(--rd-ink-muted);
  min-width: 0;
}

.fact-list {
  overflow-wrap: anywhere;
}

/* Underlined, for the reason `ReviewStepPage` gave the old top line: small text needs it. */
.header-out {
  color: inherit;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.action {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 18px;
  border-radius: var(--rd-radius-action);
  background: var(--rd-accent);
  color: var(--rd-surface);
  font-weight: 600;
  text-decoration: none;
}

.action:hover {
  background: var(--rd-accent-hover);
}

.tabs {
  display: flex;
  gap: 28px;
  overflow-x: auto;
  scrollbar-width: none;
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: none;
  height: 34px;
  border-bottom: 2px solid transparent;
  color: var(--rd-ink-muted);
  text-decoration: none;
  font-size: var(--rd-type-lead);
  line-height: 20px;
}

.tab:hover {
  color: var(--rd-ink);
}

.tab--current {
  color: var(--rd-ink);
  font-weight: 600;
  border-bottom-color: var(--rd-ink);
}

.count {
  color: var(--rd-ink-faint);
  font-weight: 400;
  font-size: var(--rd-type-ui);
}

@media (max-width: 700px) {
  .head {
    flex-direction: column;
    gap: 10px;
  }

  .name {
    white-space: normal;
  }

  .facts {
    flex-wrap: wrap;
    row-gap: 4px;
  }

  /*
   * **Wrapped, not scrolled, under 700px.** Five tabs and two counts are about 450px of
   * text, and a strip that scrolls sideways hides Replies past the edge of a phone. Two
   * rows keep all five in sight.
   */
  .tabs {
    flex-wrap: wrap;
    column-gap: 20px;
    row-gap: 0;
    overflow-x: visible;
  }
}
</style>
