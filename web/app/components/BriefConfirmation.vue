<script setup lang="ts">
import type { Brief } from '~/types/desk'
import type { DeskNotice } from '~/utils/format'

/**
 * Layer 2's one act: the developer confirming that this is what the change is solving.
 *
 * **It is on the Understand page because the page it was on is gone.** `/reviews/{id}`
 * drew it and DEV-878 retires that screen, so without this the dashboard would have no way
 * to confirm a brief at all — and `confirmBrief` is one of only two operations in the whole
 * system that record the developer's sign-off, the other being approving a draft. Losing it
 * would have made `crates/cli/tests/signoff.rs` guard a boundary with nothing on the far
 * side of it.
 *
 * Its own component rather than two copies in the page: Understand draws a structured brief
 * and an old-style one through two different branches, and the one act is the same under
 * both.
 *
 * ## Three states, and the third is not a quieter version of the second
 *
 * | The record says | What this says |
 * |---|---|
 * | not confirmed, confirmation required | asks for it, with the button |
 * | confirmed | that the developer confirmed it, and when |
 * | confirmation not required | that a session recorded it as needing none, and shows what the session wrote |
 *
 * The third is the one state settled without anybody having signed it, so it is drawn
 * dashed rather than as a solid card: a solid one would read as an assurance.
 *
 * **It does not print `confirmation_not_required_because` as the reason**, and that is a
 * defect carried over deliberately rather than a wording choice. The screen this replaces
 * wrote "The session recorded this brief as needing none: {{ field }}." — and since DEV-883
 * a review session writes a brief twice, with that field reading "problem half only"
 * between the two writes. The sentence then said the reason no confirmation was needed was
 * "problem half only", which is not a reason and not what the field meant. The comment on
 * DEV-878 says the problem goes with the page; it would have come straight back if this
 * were a copy. So the field is shown as **what the session wrote**, labelled, as recorded
 * text — where "problem half only" reads as the session's own note rather than as the
 * page's claim.
 */
const props = defineProps<{ brief: Brief, reviewId: number }>()

const desk = useReviewDesk()

const working = ref(false)
const notice = ref<DeskNotice | null>(null)

/** Which of the three states the record is in. */
const state = computed<'asking' | 'confirmed' | 'not required'>(() => {
  if (props.brief.confirmed) return 'confirmed'
  return props.brief.confirmation_required ? 'asking' : 'not required'
})

async function confirm() {
  working.value = true
  notice.value = noticeFor(await desk.confirmBrief(props.reviewId))
  working.value = false
}
</script>

<template>
  <div class="layer-two">
    <DeskCard
      v-if="state === 'asking'"
      tone="chosen"
      class="asking"
    >
      <div class="said">
        <div class="rd-label">
          Confirm the problem before anything is posted
        </div>
        <p class="rd-note">
          Step 2 is yours. Read the problem below; if that is what the change is really
          solving, confirm it and the draft becomes yours to sign off.
        </p>
      </div>
      <DeskButton
        kind="primary"
        :working="working"
        working-label="Confirming"
        @click="confirm"
      >
        Confirm the problem
      </DeskButton>
    </DeskCard>

    <DeskCard
      v-else-if="state === 'confirmed'"
      as="section"
      pad="tight"
      class="settled"
    >
      <span class="rd-note">
        <strong>Problem confirmed.</strong>
        You confirmed it {{ sinceLabel(brief.confirmed_at) }}, which is what lets the draft
        be posted.
      </span>
    </DeskCard>

    <!--
      Dashed, like a pending step in the trail, because this is the one state that is
      settled without anybody having signed it. A solid card would read as an assurance.
    -->
    <DeskCard
      v-else
      as="section"
      pad="tight"
      class="settled waived"
    >
      <span class="rd-note">
        <strong>Confirmation not required.</strong>
        A session recorded this brief as needing none, so nothing here is waiting on you.
        You can still read the problem below, and you can still send the draft back.
      </span>
      <p
        v-if="brief.confirmation_not_required_because"
        class="rd-recorded why"
      >
        <span class="rd-rubric">What the session wrote</span>
        {{ brief.confirmation_not_required_because }}
      </p>
    </DeskCard>

    <DeskNotice
      v-if="notice"
      :tone="notice.tone"
    >
      {{ notice.text }}
    </DeskNotice>
  </div>
</template>

<style scoped>
.layer-two {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.asking {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  align-items: center;
}

.said {
  flex: 1 1 380px;
  min-width: 0;
}

.said p {
  margin: 4px 0 0;
}

.settled {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.settled strong {
  font-weight: 600;
}

.waived {
  border-style: dashed;
  border-color: var(--rd-line-strong);
}

.why {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
}
</style>
