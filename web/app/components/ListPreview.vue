<script setup lang="ts">
/**
 * **The preview beside the Pull requests list: the selected row, drawn from the record.**
 * DEV-1325, S1.
 *
 * Four things, as the ticket names them: the bottom line, the state, the next action and
 * the session. Each is drawn only when the record holds it — the parent's rule for every
 * child of DEV-1322, "a fact the record can't derive is left out":
 *
 * | Part | From |
 * | -- | -- |
 * | The bottom line | `bottomLine`, the one helper DEV-1326's Overview also draws it with |
 * | The state | The row's own word where its tab is read off an inbox array, as the sidebar's is; else `stateOfReview`, as the header's is |
 * | Findings | `finding_severities`, through `severityTally`, when any finding is recorded |
 * | Not checked | `not_checked`, when the session wrote it |
 * | Session | The row's own session line, from the inbox |
 * | The next action | `primaryAction`, the header's one action, and none when it is null |
 *
 * **What S1 draws and this does not**: *Checked by running* and *Second reader* have no
 * field to come from, and *Last thing you asked* is the conversation's, which the second
 * stack builds. Each is left out rather than drawn empty.
 *
 * **It reads `GET /reviews/{id}` for the row it is showing**, one review at a time: the
 * list's own read is the inbox, which carries no brief. Keyed by the review in the parent,
 * so moving the selection reads the next one rather than redrawing the last one's answer.
 */
import type { ListRow } from '~/utils/pullRequests'

const props = defineProps<{ row: ListRow }>()

const desk = useReviewDesk()
const detail = props.row.reviewId != null ? desk.review(props.row.reviewId) : null

const state = computed(() => {
  if (props.row.state) return props.row.state
  const data = detail?.data.value
  return data ? stateOfReview(data) : null
})

const line = computed(() => bottomLine(detail?.data.value?.brief))

const action = computed(() => {
  const data = detail?.data.value
  if (!data || state.value == null) return null
  return primaryAction(data.review.id, state.value, repliesToAnswer(data.outcomes))
})

const waitsOnYou = computed(() =>
  state.value === 'brief_ready' || state.value === 'draft_ready' || state.value === 'replies_in')

/** The facts table: each row only when the record holds its figure. */
const facts = computed(() => {
  const rows: Array<{ label: string, says: string }> = []
  const data = detail?.data.value
  if (data && data.findings.length > 0) {
    const tally = severityTally(data.finding_severities)
    rows.push({
      label: 'Findings',
      says: tally ? `${data.findings.length} · ${tally}` : String(data.findings.length),
    })
  }
  if (data?.not_checked) rows.push({ label: 'Not checked', says: data.not_checked })
  if (props.row.session) rows.push({ label: 'Session', says: props.row.session })
  return rows
})
</script>

<template>
  <section
    class="preview"
    :aria-label="`#${row.pr}, previewed`"
    data-testid="list-preview"
  >
    <p class="top">
      <span class="number rd-mono">#{{ row.pr }}</span>
      <DeskPill
        v-if="state"
        :tone="waitsOnYou ? 'go-quiet' : 'quiet'"
        data-testid="preview-state"
      >
        {{ stateWord(state) }}
      </DeskPill>
      <NuxtLink
        v-if="row.to"
        class="open"
        :to="row.to"
      >Open<span class="rd-visually-hidden"> #{{ row.pr }}</span></NuxtLink>
      <a
        v-else-if="row.href"
        class="open"
        :href="row.href"
        target="_blank"
        rel="noreferrer noopener"
      >Open on GitHub<span class="rd-visually-hidden">, opens in a new tab</span></a>
    </p>

    <p class="title">
      {{ row.title }}
    </p>

    <template v-if="detail">
      <p
        v-if="detail.pending.value && !detail.loaded.value"
        class="rd-meta"
      >
        Reading the review…
      </p>
      <template v-else-if="detail.data.value">
        <div
          v-if="line"
          class="bottom-line"
          data-testid="bottom-line"
        >
          <p
            v-if="line.headline"
            class="headline"
          >
            {{ line.headline }}
          </p>
          <p
            v-if="line.facts.length"
            class="rd-meta answer-facts"
          >
            {{ line.facts.map(fact => fact.says).join(' · ') }}
          </p>
        </div>
        <p
          v-else
          class="rd-meta"
        >
          No brief has been written for this round yet.
        </p>
      </template>
    </template>
    <p
      v-else
      class="rd-meta"
    >
      No review has been opened for this pull request.
    </p>

    <dl
      v-if="facts.length"
      class="facts"
    >
      <div
        v-for="fact in facts"
        :key="fact.label"
        class="fact"
      >
        <dt>{{ fact.label }}</dt>
        <dd>{{ fact.says }}</dd>
      </div>
    </dl>

    <NuxtLink
      v-if="action"
      class="next"
      :to="action.to"
      data-testid="preview-action"
    >
      {{ action.label }}
      <span aria-hidden="true">→</span>
    </NuxtLink>
  </section>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 100%;
}

.top {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
}

.number {
  color: var(--rd-ink-muted);
}

.open {
  margin-left: auto;
  font-size: var(--rd-type-ui);
}

.title {
  margin: 0;
  font-size: var(--rd-type-body);
  font-weight: 600;
  overflow-wrap: anywhere;
}

.bottom-line {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* S1's headline: the sans, a step up and heavy — the page's voice is not a serif any more. */
.headline {
  margin: 0;
  font-size: var(--rd-type-heading);
  font-weight: 600;
  line-height: 1.3;
  overflow-wrap: anywhere;
}

.answer-facts {
  margin: 0;
}

.facts {
  margin: 0;
}

.fact {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 0;
  border-bottom: 1px solid var(--rd-line);
}

.fact dt {
  color: var(--rd-ink-muted);
}

.fact dd {
  margin: 0;
  font-weight: 600;
  text-align: right;
  overflow-wrap: anywhere;
}

/* The one next action, as S1 draws it: a full-width button at the foot of the pane. */
.next {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: var(--rd-control-min);
  margin-top: auto;
  padding: 10px 16px;
  color: var(--rd-surface);
  font-weight: 600;
  text-decoration: none;
  background: var(--rd-accent);
  border-radius: var(--rd-radius-sm);
}

.next:hover {
  color: var(--rd-surface);
  background: var(--rd-accent-hover);
}
</style>
