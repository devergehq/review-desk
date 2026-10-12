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
 * The API refuses it for a review holding a brief, a finding, a draft or the review it
 * posted on GitHub, and that refusal is what makes the rule real. A button that produced it on every review would be a button
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
 * **The same four the store asks about, in the same words**, through the one helper the
 * sample backend's refusal is also built from — the sentence under this card is the
 * developer's only explanation of why no delete is offered, and it must not read
 * differently from the refusal it is explaining.
 *
 * The fourth is the review posted on GitHub, which a review can hold with none of the
 * other three: `review link --github-review` is a review whose process ran outside
 * Review Desk, and the Reviews list draws it as posted. Without it the card told the
 * developer such a review had "nothing on it but the request".
 */
const held = computed(() => ({
  brief: props.detail.brief != null,
  findings: props.detail.findings.length,
  draft: props.detail.draft_status != null,
  posted: props.detail.review.github_review_id != null,
}))

/** "a brief and 4 findings" — the list the refusal and this card both read as one. */
const holdsInWords = computed(() => reviewWorkInWords(held.value))

/** Whether this review never got past being requested, which is the delete's one case. */
const neverStarted = computed(() => holdsInWords.value === '')

/**
 * Whether a session has picked this review up, whatever it has written yet.
 *
 * **Not part of what a delete is refused for**, and deliberately: the store asks what the
 * review *holds*, and none of these is work the developer would lose — the stray review
 * this was built for was written by a build session and may well carry several of them.
 * What it changes is what the card is allowed to *say*. "Nothing on it but the request"
 * is false about a review somebody has begun, and the delete takes all of these rows with
 * it, so the sentence says so and the confirmation says it again.
 *
 * **The same signs the store reads in `review_is_claimed`, all of them.** It read two
 * while the store read eight, which a review of this branch caught: a review holding a
 * per-file change map, its sources, the ticket's criteria, the description lint's
 * results, a round's commits or a not-checked sentence was still being described as
 * having nothing on it but the request. Every one of them is already on this page —
 * `GET /reviews/{id}` serves the lot — so there is nothing to read twice.
 */
const claimed = computed(() => {
  const detail = props.detail
  return detail.review.session_id != null
    || detail.not_checked != null
    || detail.layers.some(layer => layer.state !== 'pending')
    || (detail.coverage?.files.length ?? 0) > 0
    || (detail.sources?.length ?? 0) > 0
    || (detail.criteria?.length ?? 0) > 0
    || (detail.description_lint?.results.length ?? 0) > 0
    || (detail.commits_since_previous_round?.length ?? 0) > 0
})

/**
 * Whether what it holds is one thing or several, for "lose that" or "lose them".
 *
 * Through the one helper the store's refusal has its own half of, so the card and the
 * message a developer reads when the API refuses cannot say it differently.
 */
const holdsOneThing = computed(() => reviewWorkIsOneThing(held.value))

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
   * **Away from the page, because the page is about a review that is no longer there**,
   * and the sentence goes with the developer. Staying would leave the review's own
   * screens reading a 404; leaving without the sentence would make the one act with no
   * undo the one act with no acknowledgement, which is what `handOver` is for. On a
   * refusal nothing moves: the card says what the review holds and the developer is
   * still looking at it.
   */
  if (result.ok) {
    handOver(result.message)
    await navigateTo('/reviews')
  }
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
      Already dismissed: when and why, in the developer's own words — the same two facts
      the Reviews list's dismissed view draws — and the undo.

      **And not what a dismissal means.** The notice under the trail says that, on this
      page and on the review's four others; three paragraphs saying it on one screen is
      what this card had before.
    -->
    <template v-if="review.dismissed_at">
      <p class="rd-note rd-measure">
        <strong>Dismissed {{ sinceLabel(review.dismissed_at) }}.</strong>
        <template v-if="review.dismissed_reason">
          Why: {{ review.dismissed_reason }}
        </template>
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

      <!--
        **Hidden while anything is open, not only while this one is.** `asking` is a
        single slot, so offering the other opener beside an open box let a press throw
        away what the developer had typed in it: "Delete it" with a reason half written
        unmounted the box and lost the words, silently.
      -->
      <div
        v-if="asking === null"
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
        :working="working"
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
        <p
          v-if="!claimed"
          class="rd-note rd-measure"
        >
          <strong>This review never started.</strong>
          No brief, no findings, no draft — nothing on it but the request. Deleting it
          removes the row and everything under it, and that cannot be undone.
        </p>
        <p
          v-else
          class="rd-note rd-measure"
        >
          <strong>A session has this review.</strong>
          It has written no brief, no findings and no draft, so it can still be deleted —
          but a session is linked to it or has reported a step, and deleting it takes
          those rows with it while that session is still working. Dismissing it instead
          takes it out of the queue and leaves the session's own record alone.
        </p>

        <div
          v-if="asking === null"
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
            be undone.<template v-if="claimed">
              A session is still at work on it.
            </template>
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
        {{ holdsOneThing ? 'that' : 'them' }}. Dismissing it takes it out of the queue
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
