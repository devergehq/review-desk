<script setup lang="ts">
import type { ReviewDetail } from '~/types/desk'

/**
 * **Taking a review off the desk**, at the foot of the review's own page.
 *
 * Two acts, and the issue they come from is specific about which is for what. Patrick,
 * DEV-945, 12 October 2026:
 *
 * | | Dismiss | Delete |
 * |---|---|---|
 * | What happens | it leaves the queue and the Reviews list's default view, with a reason | the row and everything under it are gone |
 * | Keeps | its findings, your edits, its rules evidence, the posted review's harvest | nothing |
 * | Undo | yes | no |
 * | Offered for | every review | a review that never got past "requested" |
 *
 * So dismissal is the one with a button on every review, and delete is offered only where
 * there is nothing to lose. **Both are the developer's act**: there is no command for
 * either, which is the same rule that leaves confirming a brief and approving a draft to
 * this dashboard alone.
 *
 * ## Why delete is hidden rather than refused
 *
 * The API refuses it for a review holding a brief, a finding or a draft, and that refusal
 * is what makes the rule real. A button that produced it on every review would be a button
 * whose answer is almost always no — so where the review holds something, the card says
 * what it holds and offers the dismissal instead. **The page decides what to draw; it does
 * not decide the refusal.** The two cannot drift, because what is drawn is read off the
 * same three things the store asks about: the brief, the findings and the draft's status,
 * all of which this page already has from `GET /reviews/{id}` — so the card needs no
 * second read to know whether to offer the button.
 *
 * ## Why delete asks twice and dismissal does not
 *
 * A dismissal is undone by the button beside it. A delete cannot be undone by anything, so
 * it is the one act on this desk that asks for a second press — and the second press names
 * the review, because "Delete" on its own is a word, and `acme/portal #777` is a thing.
 */
const props = defineProps<{ detail: ReviewDetail }>()

const desk = useReviewDesk()

/** Which control is open: the reason box, the delete confirmation, or neither. */
const asking = ref<'dismiss' | 'delete' | null>(null)

/** A write is in flight, so neither button may be pressed again. */
const working = ref(false)

/** What the desk said back, in the tone its code earns. */
const said = ref<DeskNotice | null>(null)

const review = computed(() => props.detail.review)

/**
 * What a session has written to this review: the three things a delete would lose.
 *
 * **The same three the store asks about, in the same words**, because the sentence under
 * the card is the developer's only explanation of why no delete is offered. The findings
 * are counted rather than named: this is a reason, not an index.
 */
const holds = computed(() => {
  const found: string[] = []
  if (props.detail.brief) found.push('a brief')
  if (props.detail.findings.length > 0) {
    found.push(count(props.detail.findings.length, 'finding'))
  }
  if (props.detail.draft_status) found.push('a draft')
  return found
})

/** "a brief and 4 findings" — the list the refusal and this card both read as one. */
const holdsInWords = computed(() => {
  const found = holds.value
  if (found.length <= 1) return found[0] ?? ''
  return `${found.slice(0, -1).join(', ')} and ${found.at(-1)}`
})

/** Whether this review never got past being requested, which is the delete's one case. */
const neverStarted = computed(() => holds.value.length === 0)

async function dismiss(reason: string) {
  working.value = true
  said.value = noticeFor(await desk.dismissReview(review.value.id, reason))
  working.value = false
  /* The box closes only on success, so a refused reason is still there to correct. */
  if (said.value.tone === 'plain') asking.value = null
}

async function restore() {
  working.value = true
  said.value = noticeFor(await desk.restoreReview(review.value.id))
  working.value = false
  asking.value = null
}

async function remove() {
  working.value = true
  const result = await desk.deleteReview(review.value.id)
  said.value = noticeFor(result)
  working.value = false
  asking.value = null
  /*
   * **Away from the page, because the page is about a review that is no longer there.**
   * Staying would leave the review's own screens reading a 404, which is a worse answer
   * than the list the developer came from — and the Reviews list is where the sentence
   * about what went is worth reading. On a refusal nothing moves: the card says what the
   * review holds and the developer is still looking at it.
   */
  if (result.ok) await navigateTo('/reviews')
}
</script>

<template>
  <DeskCard
    id="remove-review"
    as="section"
    tone="well"
    pad="section"
    class="remove"
  >
    <h2 class="rd-rubric">
      Take this review off the desk
    </h2>

    <!--
      Already dismissed. The card says when and why in the developer's own words — the same
      two facts the Reviews list's dismissed view draws — and offers the undo.
    -->
    <template v-if="review.dismissed_at">
      <p class="rd-note rd-measure">
        <strong>Dismissed {{ sinceLabel(review.dismissed_at) }}.</strong>
        It is out of the queue, no session will be started for it, and it is not in the
        Reviews list's default view. Everything on it has been kept.
      </p>
      <p
        v-if="review.dismissed_reason"
        class="rd-note rd-measure reason"
      >
        Why: {{ review.dismissed_reason }}
      </p>

      <div class="controls">
        <DeskButton
          kind="secondary"
          :working="working"
          working-label="Bringing it back"
          @click="restore()"
        >
          Bring it back
        </DeskButton>
      </div>
    </template>

    <!-- In play: the dismissal, with the reason it is refused without. -->
    <template v-else>
      <p class="rd-note rd-measure">
        <strong>Dismissing it hides it and keeps it.</strong>
        The review leaves the queue and the Reviews list's default view, and no session is
        started for it. Its findings, your decisions on them, your sign-offs and the
        evidence its rules were built from all stay, and you can bring it back from this
        page.
      </p>

      <div
        v-if="asking !== 'dismiss'"
        class="controls"
      >
        <DeskButton
          kind="secondary"
          @click="asking = 'dismiss'; said = null"
        >
          Dismiss this review
        </DeskButton>
      </div>
      <ReasonPrompt
        v-else
        id="dismiss-reason"
        label="Why are you dismissing this review?"
        action="Dismiss it"
        @confirm="dismiss"
        @cancel="asking = null"
      />
    </template>

    <!--
      -------------------------------- The delete --------------------------------
      Offered only for a review that never got past being requested. Where the review holds
      something, the card says what, which is the same sentence the API's refusal carries.
    -->
    <div class="delete">
      <template v-if="neverStarted">
        <p class="rd-note rd-measure">
          <strong>This review never started.</strong>
          No brief, no findings, no draft — nothing on it but the request. Deleting it
          removes the row and everything under it, and that cannot be undone.
        </p>

        <div
          v-if="asking !== 'delete'"
          class="controls"
        >
          <DeskButton
            kind="secondary"
            @click="asking = 'delete'; said = null"
          >
            Delete it
          </DeskButton>
        </div>
        <div
          v-else
          class="controls confirm"
        >
          <p class="rd-note">
            Delete review {{ review.id }} — {{ review.repo }} #{{ review.pr }}? This cannot
            be undone.
          </p>
          <DeskButton
            kind="primary"
            :working="working"
            working-label="Deleting"
            @click="remove()"
          >
            Delete it for good
          </DeskButton>
          <DeskButton
            kind="secondary"
            @click="asking = null"
          >
            Keep it
          </DeskButton>
        </div>
      </template>

      <p
        v-else
        class="rd-note rd-measure"
      >
        <strong>It cannot be deleted.</strong>
        This review holds {{ holdsInWords }}, and deleting it would lose
        {{ holds.length === 1 ? 'that' : 'them' }}. Dismissing it takes it out of the queue
        and keeps the record.
      </p>
    </div>

    <DeskNotice
      v-if="said"
      :tone="said.tone"
    >
      {{ said.text }}
    </DeskNotice>
  </DeskCard>
</template>

<style scoped>
.remove {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.remove h2 {
  margin: 0;
}

.remove p {
  margin: 0;
}

.reason {
  color: var(--rd-ink-dim);
}

.controls {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 12px;
  align-items: center;
}

/*
 * The confirmation reads as one sentence with its two answers, so the question takes the
 * whole width above them rather than sitting in the row and pushing the buttons off the
 * card at 390px.
 */
.confirm p {
  flex: 1 1 100%;
}

/* A rule between the two acts: they are different decisions, not two buttons in a row. */
.delete {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--rd-line);
}
</style>
