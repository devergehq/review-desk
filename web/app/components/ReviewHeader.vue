<script setup lang="ts">
import type { ReviewDetail } from '~/types/desk'
import type { ActOnTab, ReviewTab } from '~/utils/reviewTabs'

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
  /**
   * **The act itself, from the tab that holds it.** DEV-1329.
   *
   * Off its own tab the action is a link to the tab that does it. *On* that tab a link to
   * the page you are on is a control that does nothing, so the page hands its act up and
   * the header draws a button bound to it. Draft is the one tab that does: the sign-off's
   * binding and every one of its checks moved here, and the page draws no second button.
   *
   * - `offered: false` draws no action at all — a draft nothing can post (DEV-957) has
   *   no sign-off, not a disabled one.
   * - `disabled` is every gate the old button had, decided by the page that owns them.
   */
  act?: ActOnTab | null
  /**
   * **Pinned to the top while the tab scrolls** (DEV-1329): on Draft, so the sign-off stays
   * in reach for the whole of a long draft, which the old aside's `.pinned` panel was for —
   * the design pass measured it out of reach for three quarters of a 4,305px page.
   */
  pinned?: boolean
}>()

const review = computed(() => props.detail.review)

const pullRequest = computed(() => prUrl(review.value.repo, review.value.pr))
const ticket = computed(() => ticketUrl(review.value.ticket_url))

/*
 * **The sidebar's word, when the sidebar lists this review.** DEV-1323's round 2: a review
 * the sidebar lists takes the sidebar's state in its own header, so the two can never say
 * different words. Both reads are the ones `AppSidebar` holds, through the two keys
 * `useReviewDesk` shares (`inbox`, `reviews:all:finished`), so the header costs no request of
 * its own and groups the very answer the sidebar groups. A round the sidebar does not open
 * falls to the review's own facts inside `stateOfReview`.
 *
 * **While those reads are in flight the header waits** rather than drawing its own-facts word
 * and flipping when they land: the sidebar draws nothing until its inbox is read, and a pill
 * that changes under the reader's eye is the disagreement this rule exists to stop.
 */
const desk = useReviewDesk()
const inbox = desk.inbox()
const finished = desk.finishedReviews()
const listed = computed(() =>
  rowFor(inbox.data.value ? sidebarGroups(inbox.data.value, finished.data.value) : null, review.value))

const state = computed(() => stateOfReview(props.detail, listed.value))
/** Whether the inbox has answered (or failed), so the word drawn is the word that stays. */
const settled = computed(() => inbox.loaded.value)
const word = computed(() => stateWord(state.value))

const toAnswer = computed(() => repliesToAnswer(props.detail.outcomes))
const action = computed(() =>
  settled.value
    ? primaryAction(
        review.value.id,
        state.value,
        toAnswer.value,
        { round: review.value.round, head: review.value.head_sha },
      )
    : null)

/**
 * **The word is washed in the accent exactly when there is an action.** *Replies in* with
 * nothing to answer — a thread resolved, code changed — is not waiting on the developer, so
 * it draws neither. Asked by the self-review of #89.
 */
const waitsOnYou = computed(() => action.value != null)

/**
 * **What the header draws as its action: the tab's own act, a link, or nothing.**
 *
 * - **The act**, when the tab hands one up and offers it — on Draft, a ready draft that can
 *   post. Its label is `primaryAction`'s draft-ready answer whatever the review's state word
 *   is, so a ready draft on a review whose brief also waits is still signed off from here,
 *   as it was from the page before DEV-1329 moved the button.
 * - **Nothing**, when the tab hands an act up, does not offer it, and the state's action
 *   would only lead back to this tab: a draft nothing can post has no sign-off, not a link
 *   to the page already open.
 * - **The state's link** otherwise.
 */
const actionShape = computed<'act' | 'link' | 'none'>(() => {
  if (props.act?.offered) return 'act'
  if (!action.value) return 'none'
  if (props.act && action.value.to === tabPath(review.value.id, props.tab)) return 'none'
  return 'link'
})

/** The act's label, from the one function that words the action. */
const actLabel = computed(() => primaryAction(
  review.value.id,
  'draft_ready',
  [],
  { round: review.value.round, head: review.value.head_sha },
)?.label ?? '')

/**
 * The facts line, each fact only when the record holds it.
 *
 * The round and the commit read are always held. The size is held when the session wrote
 * it. **The one time is how long the open wait has run**, from `openWait`, which answers
 * nothing when the record holds no start for it — drawn as nothing rather than as a guess.
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

const waiting = computed(() => openWait(state.value, props.detail.timings))

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
    :class="{ 'review-header--pinned': pinned }"
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
            v-if="settled"
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

      <!--
        **On its own tab the action is the act** — DEV-1329's sign-off, with the gates the
        Draft page decides. `type="button"`, and the same class, so it is the same control
        to look at and the one control to press.
      -->
      <button
        v-if="actionShape === 'act' && act"
        type="button"
        class="action"
        data-testid="primary-action"
        :disabled="act.disabled"
        :aria-describedby="act.disabled && act.describedBy ? act.describedBy : undefined"
        @click="act.press"
      >
        {{ act.working ? 'Working…' : actLabel }}
        <span aria-hidden="true">→</span>
      </button>
      <NuxtLink
        v-else-if="actionShape === 'link' && action"
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

/* The number stays muted beside the title, as S2 draws it, and keeps its underline. */
.number.header-out {
  color: var(--rd-ink-muted);
}

.action {
  flex: none;
  display: inline-flex;
  min-height: var(--rd-control-min);
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

/*
 * **Pinned on the page's own ground**, so what scrolls under it is covered rather than
 * read through. Not under 700px, where the shell's own bar is the sticky top and a second
 * one would take a phone's screen.
 */
@media (min-width: 700px) {
  .review-header--pinned {
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--rd-page);
  }
}

/* The act drawn as a button on its own tab (DEV-1329): the same look as the link. */
button.action {
  border: 0;
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

button.action:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

button.action:disabled {
  background: var(--rd-ink-faint);
  cursor: not-allowed;
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
