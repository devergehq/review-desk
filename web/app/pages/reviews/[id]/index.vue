<script setup lang="ts">
import type { Disposition } from '~/types/desk'

/** One review: what the seven layers found, in the order they found it. */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId.value)
const draft = desk.draft(reviewId.value)

useHead({
  title: computed(() =>
    detail.value
      ? `#${detail.value.review.pr} ${detail.value.review.title} · Review Desk`
      : 'Review Desk',
  ),
})

const confirmed = computed(() => detail.value?.brief?.confirmed === true)

const dispositionTally = computed(() => {
  const d = detail.value?.finding_dispositions
  if (!d) return ''
  return [
    `${d.in_draft} in the draft`,
    `${d.held_back} held back`,
    `${d.suppressed} suppressed`,
  ].join(' · ')
})

const authorReplies = computed(
  () => detail.value?.outcomes.filter(outcome => outcome.actor !== 'developer') ?? [],
)

const notice = ref<{ text: string, ok: boolean } | null>(null)

/** Layer 2 is the developer's. Confirming it is what lets anything be posted. */
function confirm() {
  if (!detail.value) return
  desk.confirmBrief(detail.value.review.id)
  notice.value = {
    text: 'Problem confirmed. The draft is yours to approve.',
    ok: true,
  }
}

function changeDisposition(payload: {
  seq: number
  disposition: Disposition
  reason: string
}) {
  if (!detail.value) return
  const result = desk.setFindingDisposition(
    detail.value.review.id,
    payload.seq,
    payload.disposition,
    payload.reason,
  )
  notice.value = { text: result.message, ok: result.ok }
}
</script>

<template>
  <div
    v-if="detail"
    class="review"
  >
    <div class="head">
      <div class="heading">
        <h1><PrRef :pr="detail.review.pr" /> {{ detail.review.title }}</h1>
        <p class="rd-meta meta">
          {{ detail.review.repo }}
          <template v-if="detail.review.ticket_id">
            · ticket {{ detail.review.ticket_id }}
          </template>
          · round {{ detail.review.round }} · head
          <span class="rd-mono">{{ detail.review.head_sha }}</span>
          <template v-if="detail.review.changed_lines != null">
            · {{ count(detail.review.changed_lines, 'changed line') }}
          </template>
        </p>
      </div>
      <NuxtLink
        v-if="draft"
        :to="`/reviews/${detail.review.id}/draft`"
        class="rd-btn rd-btn--primary"
      >
        Open draft
      </NuxtLink>
    </div>

    <LayerTrail :layers="detail.layers" />

    <DeskCard
      v-if="detail.brief && !confirmed"
      tone="chosen"
      class="confirm"
    >
      <div class="confirm-body">
        <div class="rd-label">
          Confirm the problem before anything is posted
        </div>
        <p class="rd-note confirm-note">
          Layer 2 is yours. Read the problem below; if that is what the change is really
          solving, confirm it and the draft becomes yours to approve.
        </p>
      </div>
      <button
        type="button"
        class="rd-btn rd-btn--primary"
        @click="confirm"
      >
        Confirm the problem
      </button>
    </DeskCard>

    <DeskCard
      v-else-if="detail.brief"
      as="section"
      pad="tight"
      class="confirmed"
    >
      <span class="rd-note">
        <strong>Problem confirmed.</strong>
        You confirmed it {{ sinceLabel(detail.brief.confirmed_at) }}, which is what lets
        the draft be posted.
      </span>
    </DeskCard>

    <DeskNotice
      v-if="notice"
      :tone="notice.ok ? 'plain' : 'problem'"
    >
      {{ notice.text }}
    </DeskNotice>

    <DeskCard
      v-if="authorReplies.length"
      as="section"
      class="panel"
    >
      <div class="panel-head">
        <h2>The author replied</h2>
        <span class="rd-meta">harvested after posting</span>
      </div>
      <ul class="replies">
        <li
          v-for="outcome in authorReplies"
          :key="outcome.id"
          class="reply"
        >
          <span class="rd-mono rd-meta">{{ outcome.finding_key }}</span>
          <div>
            <div class="reply-kind">
              {{ editVerb(outcome.kind) }}
              <span class="rd-meta">· by the {{ outcome.actor }} · {{ sinceLabel(outcome.occurred_at) }}</span>
            </div>
            <p
              v-if="outcome.detail"
              class="rd-note reply-detail"
            >
              {{ outcome.detail }}
            </p>
          </div>
        </li>
      </ul>
    </DeskCard>

    <DeskCard
      v-if="detail.brief"
      as="section"
      class="panel"
    >
      <div class="panel-head">
        <h2>The problem</h2>
        <span
          v-if="detail.brief.provenance"
          class="rd-meta"
        >
          {{ detail.brief.provenance }}
        </span>
      </div>
      <p class="rd-measure problem">
        {{ detail.brief.problem }}
      </p>
      <MermaidFigure
        v-if="detail.brief.diagram"
        :source="detail.brief.diagram"
        label="How the problem happens today"
      />
    </DeskCard>

    <DeskCard
      v-if="detail.brief?.options?.length"
      as="section"
      class="panel"
    >
      <div class="panel-head">
        <h2>Ways to solve it</h2>
        <span class="rd-meta">written before the diff was read</span>
      </div>
      <div class="options">
        <OptionCard
          v-for="option in detail.brief.options"
          :key="option.key"
          :option="option"
        />
      </div>
      <p
        v-if="detail.brief.approach_verdict"
        class="rd-callout rd-measure verdict"
      >
        <strong>Verdict:</strong> {{ detail.brief.approach_verdict }}
      </p>
    </DeskCard>

    <DeskCard
      as="section"
      class="panel"
    >
      <div class="panel-head">
        <h2>Findings</h2>
        <span class="rd-meta">{{ dispositionTally }}</span>
      </div>
      <ul
        v-if="detail.findings.length"
        class="findings"
      >
        <FindingRow
          v-for="finding in detail.findings"
          :key="finding.seq"
          :finding="finding"
          editable
          @change="changeDisposition"
        />
      </ul>
      <p
        v-else
        class="rd-empty"
      >
        No findings yet. The code review is layer 5 and has not finished.
      </p>
      <p
        v-if="detail.nit_ticket.status !== 'none'"
        class="rd-meta ticket"
      >
        Held-back findings:
        {{ detail.nit_ticket.status === 'filed' ? 'filed as' : 'queued as one ticket' }}
        <template v-if="detail.nit_ticket.ticket_id">
          {{ detail.nit_ticket.ticket_id }}
        </template>
      </p>
    </DeskCard>

    <DeskCard
      as="section"
      class="panel"
    >
      <h2>Not checked</h2>
      <p
        v-if="detail.not_checked"
        class="rd-measure not-checked"
      >
        {{ detail.not_checked }}
      </p>
      <p
        v-else
        class="rd-empty"
      >
        Nothing is recorded as unchecked. That is not the same as everything having been
        checked: the code review has not finished.
      </p>
    </DeskCard>
  </div>

  <DeskCard
    v-else
    class="missing"
  >
    <h1>No such review</h1>
    <p class="rd-note">
      The desk holds no review with that id.
      <NuxtLink to="/">Back to the inbox</NuxtLink>.
    </p>
  </DeskCard>
</template>

<style scoped>
.review {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  align-items: flex-start;
}

.heading {
  flex: 1 1 420px;
  min-width: 0;
}

.meta {
  margin: 4px 0 0;
}

.confirm {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  align-items: center;
}

.confirm-body {
  flex: 1 1 380px;
  min-width: 0;
}

.confirm-note {
  margin: 4px 0 0;
}

.panel {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.panel-head {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: baseline;
}

.problem,
.not-checked,
.verdict,
.ticket {
  margin: 0;
}

.not-checked {
  color: var(--rd-ink-dim);
}

.verdict strong {
  font-weight: 600;
}

.options {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 12px;
}

.findings,
.replies {
  margin: 0;
  padding: 0;
  list-style: none;
}

.reply {
  display: flex;
  gap: 10px;
  padding: 10px 0;
  border-top: 1px solid var(--rd-rail);
}

.reply:first-child {
  border-top: 0;
  padding-top: 0;
}

.reply-kind {
  font-weight: 500;
}

.reply-detail {
  margin: 2px 0 0;
  max-width: var(--rd-measure);
}

.confirmed {
  display: flex;
  align-items: center;
  gap: 8px;
}

.confirmed strong {
  font-weight: 600;
}

.missing {
  padding: 20px;
}
</style>
