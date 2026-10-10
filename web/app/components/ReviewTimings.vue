<script setup lang="ts">
import type { ReviewTimings } from '~/types/desk'

/**
 * How long the review has taken: one line, three figures.
 *
 * ```
 *  34 min with the agent · 10 min waiting on you · requested 2 days ago
 * ```
 *
 * **"Not recorded" is a figure here, and it is why this is a component rather than three
 * interpolations.** A review recorded before DEV-890 has no start for any step, so both
 * durations are null — and a nought in their place would say the review took no time at
 * all. Each one says which of the two it is, through `NotMeasured`, with the reason the
 * record gives.
 *
 * The totals only. Each step's own duration is on the trail above, beside its outcome,
 * because a second copy of a step's figure here is a second answer the page could show.
 */
const props = defineProps<{ timings: ReviewTimings }>()

const withTheAgent = computed(() => durationLabel(props.timings.with_the_agent_seconds))
const waiting = computed(() => durationLabel(props.timings.waiting_on_you_seconds))

/**
 * Why the agent's total is absent, in the words the record would use.
 *
 * Two different absences, and the difference matters to whoever is reading: a review from
 * before this change can never have the figure, and a review still being worked on does
 * not have it *yet*.
 *
 * **Told apart by whether the draft was ever signed off**, not by `draft_ready_at`. The
 * total is null exactly when `draft_ready_at` is — the server derives one from the other —
 * so keying on it would make one of these branches unreachable and send a review posted
 * last week down the "not ready yet" one.
 */
const everReady = computed(() =>
  Boolean(props.timings.signed_off_at || props.timings.posted_at),
)

const whyNoAgentTotal = computed(() =>
  everReady.value
    ? 'this review was signed off before Review Desk kept when a draft became ready'
    : 'the draft is not ready yet, so the span it measures has no end',
)
</script>

<template>
  <p class="rd-meta totals">
    <span class="figure">
      <template v-if="withTheAgent">{{ withTheAgent }} with the agent</template>
      <template v-else>
        with the agent:
        <NotMeasured
          :because="whyNoAgentTotal"
          :says="everReady ? 'not recorded' : 'not recorded yet'"
        />
      </template>
    </span>
    <span class="figure">
      <template v-if="waiting">{{ waiting }} waiting on you</template>
      <template v-else>
        waiting on you:
        <NotMeasured
          because="no wait on the developer has finished on this review"
          says="not recorded"
        />
      </template>
    </span>
    <span class="figure">requested {{ sinceLabel(timings.requested_at) }}</span>
    <span
      v-if="timings.posted_at"
      class="figure"
    >posted {{ sinceLabel(timings.posted_at) }}</span>
  </p>
</template>

<style scoped>
/*
 * `totals` rather than `took`, which is what the trail above calls one step's own figure.
 * Two different facts, and a test that could not tell them apart would be asserting about
 * whichever one the selector found first.
 *
 * One line that wraps into several on a narrow screen, with the separator carried by the
 * gap rather than by a character — a middle dot typed between interpolations is the thing
 * that produced "Handled without you  this week :" on the queue.
 */
.totals {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 10px;
  margin: 0;
  font-variant-numeric: tabular-nums;
}

.figure:not(:last-child)::after {
  content: ' \00b7';
}
</style>
