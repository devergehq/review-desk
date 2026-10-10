<script setup lang="ts">
import type { Disposition, ReviewDetail } from '~/types/desk'
import type { DeskNotice } from '~/utils/format'

/** One review: what the seven layers found, in the order they found it. */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `#${review.pr} ${review.title ?? 'no title'} · Review Desk` : 'Review Desk'
  }),
})

const notice = ref<DeskNotice | null>(null)
const confirming = ref(false)

function dispositionTally(data: ReviewDetail): string {
  const d = data.finding_dispositions
  return [
    `${d.in_draft} in the draft`,
    `${d.held_back} held back`,
    `${d.suppressed} suppressed`,
  ].join(' · ')
}

function authorReplies(data: ReviewDetail) {
  return data.outcomes.filter(outcome => outcome.actor !== 'developer')
}

/** Layer 2 is the developer's. Confirming it is what lets anything be posted. */
async function confirm() {
  const review = detail.data.value?.review
  if (!review) return
  confirming.value = true
  const result = await desk.confirmBrief(review.id)
  confirming.value = false
  notice.value = noticeFor(result)
}

async function changeDisposition(payload: {
  seq: number
  disposition: Disposition
  reason: string
}) {
  const review = detail.data.value?.review
  if (!review) return
  const result = await desk.setFindingDisposition(
    review.id,
    payload.seq,
    payload.disposition,
    payload.reason,
  )
  notice.value = noticeFor(result)
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <div class="review">
        <div class="head">
          <div class="heading">
            <h1>
              <PrRef :pr="data.review.pr" />
              {{ data.review.title ?? 'no title recorded' }}
            </h1>
            <p class="rd-meta meta">
              {{ data.review.repo }}
              <template v-if="data.review.ticket_id">
                · ticket {{ data.review.ticket_id }}
              </template>
              · round {{ data.review.round }} · head
              <span class="rd-mono">{{ data.review.head_sha }}</span>
              <template v-if="data.review.changed_lines != null">
                · {{ count(data.review.changed_lines, 'changed line') }}
              </template>
              <template v-else>
                · changed lines not recorded
              </template>
            </p>
          </div>
          <NuxtLink
            v-if="data.draft_status"
            :to="`/reviews/${data.review.id}/draft`"
            class="rd-btn rd-btn--primary"
          >
            Open draft
          </NuxtLink>
        </div>

        <LayerTrail :layers="data.layers" />

        <!--
          Layer 2 has three states, not two, and the screen has to be able to say which.
          A brief waiting to be confirmed asks for the developer's act; a confirmed one
          records that they gave it; and one recorded as needing no confirmation records
          that an agent decided they did not have to — which is a different thing from
          either, and the one state with nothing for the developer to do.
        -->
        <DeskCard
          v-if="data.brief && !data.brief.confirmed && data.brief.confirmation_required"
          tone="chosen"
          class="confirm"
        >
          <div class="confirm-body">
            <div class="rd-label">
              Confirm the problem before anything is posted
            </div>
            <p class="rd-note confirm-note">
              Layer 2 is yours. Read the problem below; if that is what the change is really
              solving, confirm it and the draft becomes yours to sign off.
            </p>
          </div>
          <button
            type="button"
            class="rd-btn rd-btn--primary"
            :disabled="confirming"
            @click="confirm"
          >
            {{ confirming ? 'Confirming…' : 'Confirm the problem' }}
          </button>
        </DeskCard>

        <DeskCard
          v-else-if="data.brief && data.brief.confirmed"
          as="section"
          pad="tight"
          class="confirmed"
        >
          <span class="rd-note">
            <strong>Problem confirmed.</strong>
            You confirmed it {{ sinceLabel(data.brief.confirmed_at) }}, which is what lets
            the draft be posted.
          </span>
        </DeskCard>

        <DeskCard
          v-else-if="data.brief"
          as="section"
          pad="tight"
          class="confirmed waived"
        >
          <span class="rd-note">
            <strong>Confirmation not required.</strong>
            The session recorded this brief as needing none:
            {{ data.brief.confirmation_not_required_because ?? 'no reason was given' }}.
            Nothing is waiting on you here — you can still read the problem below, and you
            can still send the draft back.
          </span>
        </DeskCard>

        <DeskNotice
          v-if="notice"
          :tone="notice.tone"
        >
          {{ notice.text }}
        </DeskNotice>

        <DeskCard
          v-if="authorReplies(data).length"
          as="section"
          class="panel"
        >
          <div class="panel-head">
            <h2>The author replied</h2>
            <span class="rd-meta">harvested after posting</span>
          </div>
          <ul class="replies">
            <li
              v-for="outcome in authorReplies(data)"
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
          v-if="data.brief"
          as="section"
          class="panel"
        >
          <div class="panel-head">
            <h2>The problem</h2>
            <span
              v-if="data.brief.provenance"
              class="rd-meta rd-recorded"
            >
              {{ data.brief.provenance }}
            </span>
          </div>
          <p class="rd-recorded problem">
            {{ data.brief.problem }}
          </p>
          <MermaidFigure
            v-if="data.brief.diagram"
            :source="data.brief.diagram"
            label="How the problem happens today"
          />
        </DeskCard>

        <!--
          A review with no brief at all. Layer 2 has not run, which is a different thing
          from a brief waiting to be confirmed, and the screen says which.
        -->
        <DeskCard
          v-else
          as="section"
          class="panel"
        >
          <h2>The problem</h2>
          <p class="rd-empty">
            No brief yet. Layer 2 is where a session writes what the change is solving, and
            it has not written one — so there is nothing to confirm and nothing later can
            run.
          </p>
        </DeskCard>

        <DeskCard
          v-if="data.brief?.options?.length"
          as="section"
          class="panel"
        >
          <div class="panel-head">
            <h2>Ways to solve it</h2>
            <span class="rd-meta">written before the diff was read</span>
          </div>
          <div class="options">
            <OptionCard
              v-for="option in data.brief.options"
              :key="option.key"
              :option="option"
            />
          </div>
          <p
            v-if="data.brief.approach_verdict"
            class="rd-callout rd-measure verdict"
          >
            <strong>Verdict:</strong> {{ data.brief.approach_verdict }}
          </p>
        </DeskCard>

        <DeskCard
          as="section"
          class="panel"
        >
          <div class="panel-head">
            <h2>Findings</h2>
            <span class="rd-meta">{{ dispositionTally(data) }}</span>
          </div>
          <ul
            v-if="data.findings.length"
            class="findings"
          >
            <FindingRow
              v-for="finding in data.findings"
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
            v-if="data.nit_ticket.status !== 'none'"
            class="rd-meta ticket"
          >
            Held-back findings:
            {{ data.nit_ticket.status === 'filed' ? 'filed as' : 'queued as one ticket' }}
            <template v-if="data.nit_ticket.ticket_id">
              {{ data.nit_ticket.ticket_id }}
            </template>
          </p>
        </DeskCard>

        <DeskCard
          as="section"
          class="panel"
        >
          <h2>Not checked</h2>
          <p
            v-if="data.not_checked"
            class="rd-measure not-checked"
          >
            {{ data.not_checked }}
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
    </template>
  </DeskState>
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

/*
 * Dashed, like a `pending` layer in the trail, because this is the one brief state that
 * is settled without anybody having signed it. A solid card would read as an assurance.
 */
.waived {
  border-style: dashed;
  border-color: var(--rd-line-strong);
}

.missing {
  padding: 20px;
}
</style>
