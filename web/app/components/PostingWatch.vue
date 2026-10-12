<script setup lang="ts">
import type { Draft, Instant, Review } from '~/types/desk'

/**
 * What became of a draft the developer signed off — kept current, in words, until it is
 * posted or comes back.
 *
 * **Review Desk never posts to GitHub.** The developer signs off, the status changes, and
 * the review session reads the approved draft and posts it. There is a real gap between
 * the two, and until DEV-932 this page printed one sentence into that gap and then went
 * quiet: on 11 October 2026 a sign-off posted 4 min 28 s later and, in between, nothing on
 * screen said whether anything would pick it up. "The lack of knowing whether it's going,
 * whether something's going to pick it up or not."
 *
 * So this re-reads the review while it waits, and says what the record can support:
 *
 * | While | It says |
 * |---|---|
 * | signed off | how long ago, when anything last wrote to the review, that the review's own session posts it |
 * | signed off, and {@link POSTED_WITHIN_SECONDS} gone | that the session may not be running, and what the developer can do |
 * | posted | when, as which event, how long it took, and where the review is on GitHub |
 * | sent back after a sign-off | that the sign-off is taken back and the draft is with the session |
 *
 * **It cannot say whether the session is alive, and it says so rather than guessing.**
 * Allele owns that fact; `session_alive` is served on the queue's own item and is null in
 * every response this server writes, and `GET /reviews/{id}` does not carry it at all. What
 * the record does hold is `last_activity_at` — every write moves it and no read does — so
 * how long the review has been silent is the nearest thing there is, and that is what this
 * shows.
 *
 * **Nothing here approves, unapproves or posts anything.** It reads, on a timer, and asks
 * its page to read again.
 */
const props = defineProps<{
  draft: Draft
  /** The draft's own review, for the repository, the pull request and the GitHub id. */
  review: Review
  /** `ReviewTimings.last_activity_at` — when anything last wrote to this review. */
  lastActivityAt: Instant | null | undefined
  /** `DraftDetail.body_is_stale`: the one signed-off state no session can post. */
  bodyIsStale: boolean
}>()

/** Read the review and its draft again. The page owns the reads; this only asks. */
const emit = defineEmits<{ refresh: [] }>()

/**
 * The clock the figures are worded from, moved by the timer and by nothing else.
 *
 * **It stops when the timer does**, which is deliberate and is the rest of this desk's
 * rule: a time is as fresh as the page, so a posted draft left open all afternoon still
 * says what it said when it was posted rather than counting upwards forever. While the
 * page is waiting it is as fresh as the last read, which is at most
 * {@link RE_READ_EVERY_SECONDS} seconds old.
 */
const now = ref(new Date())

/**
 * Whether anything outside this page is expected to change the record.
 *
 * `approved` is the only status where something else is coming: a session posts it, or the
 * developer sends it back from the controls above. `ready` and `drafting` wait on people,
 * and `posted` and `sent_back` are where this stops.
 *
 * **A stale body is not waiting on anything.** The hand-over refuses it, so no session will
 * post it however long this watched, and "the session may not be running" would be the
 * wrong diagnosis of a state whose cause is known. The sentence above the sign-off says
 * what it is and offers the only way out, which is an act on this page — and an act on
 * this page re-reads everything anyway.
 */
const waiting = computed(() => props.draft.status === 'approved' && !props.bodyIsStale)

/** Whether this page watched the sign-off it is now reporting the end of. */
const watchedTheSignOff = ref(props.draft.status === 'approved')

watch(
  () => props.draft.status,
  (status) => {
    if (status === 'approved') watchedTheSignOff.value = true
  },
)

let timer: ReturnType<typeof setInterval> | null = null

function stop(): void {
  if (timer == null) return
  clearInterval(timer)
  timer = null
}

function start(): void {
  /*
   * One timer at a time, and none while the page is hidden. A browser throttles a hidden
   * tab's intervals rather than stopping them, so "stops when hidden" has to be said here
   * — and a tab left open overnight on a signed-off draft should not be reading a database
   * nobody is looking at.
   */
  if (timer != null || !waiting.value || document.visibilityState === 'hidden') return
  timer = setInterval(() => {
    now.value = new Date()
    emit('refresh')
  }, RE_READ_EVERY_SECONDS * 1000)
}

/**
 * Hidden: stop. Shown again: read once, now, and start again.
 *
 * The read on becoming visible is the point of handling this at all rather than letting the
 * browser throttle the interval. A developer who comes back to the tab after ten minutes is
 * looking at a ten-minute-old answer, and waiting up to another five seconds to find out it
 * posted nine minutes ago is the complaint this change is about, in miniature.
 */
function onVisibilityChange(): void {
  if (document.visibilityState === 'hidden') {
    stop()
    return
  }
  if (!waiting.value) return
  now.value = new Date()
  emit('refresh')
  start()
}

watch(waiting, (yes) => {
  /*
   * The clock is moved on every change of state, not only by the timer: the draft has just
   * become posted or come back, and the figures about to be drawn are about this moment.
   */
  now.value = new Date()
  if (yes) start()
  else stop()
})

onMounted(() => {
  document.addEventListener('visibilitychange', onVisibilityChange)
  start()
})

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibilityChange)
  stop()
})

const sinceSignOff = computed(() => secondsSince(props.draft.approved_at, now.value))

/**
 * Whether the session has had longer than the watcher's two intervals and posted nothing.
 *
 * Only while waiting. A posted draft that took three minutes is not late; it is posted, and
 * what it took is a fact about it rather than a warning.
 */
const late = computed(
  () => waiting.value && (sinceSignOff.value ?? 0) >= POSTED_WITHIN_SECONDS,
)

/**
 * The one sentence a screen reader is told as this changes.
 *
 * **It carries no figure, and that is the whole reason the figures are outside the live
 * region.** A region holding "25 sec ago" is a region that re-announces itself every time
 * the clock moves, which on a four-minute wait is fifty interruptions of whatever the
 * developer was reading. This changes when the *state* changes — four times at most over
 * the life of one draft — and the figures below it are ordinary text a screen reader reads
 * on request.
 *
 * Empty while nothing has been signed off, so that the region is in the page before the
 * text is: a live region added at the same moment as its content is one a screen reader
 * may not announce at all.
 */
const announcement = computed(() => {
  if (props.draft.status === 'posted') return 'Posted on GitHub.'
  if (props.draft.status === 'sent_back') {
    /*
     * Only for a sign-off this page saw. A draft sent back from `ready` was never signed
     * off, so "the sign-off is taken back" would be an event that did not happen — and the
     * record cannot tell the two apart afterwards, because a send-back clears `approved_at`
     * along with the event.
     */
    return watchedTheSignOff.value
      ? 'The sign-off is taken back. The draft is with the session again.'
      : ''
  }
  if (!waiting.value) return ''
  /*
   * **Neither of these opens with "Signed off", and that is measured rather than taste.**
   * The heading says it, the sentence under the heading says it, and the notice the write
   * itself leaves — which is a `role="status"` of its own — says it. A fourth would be the
   * same two words announced twice in a row to anybody listening. What is new here is the
   * waiting, so that is what this says.
   */
  return late.value
    ? 'Nothing has reported it posted. The session that was reviewing this may not be '
    + 'running.'
    : 'Waiting for this review’s session to post it.'
})

/**
 * "25 sec ago", down to the second, which is the unit a forty-second wait is read in.
 *
 * `sinceLabel` is the desk's usual wording and says "just now" for anything under a minute,
 * which is right where a time is read at a glance and useless here: the whole question
 * while a draft waits is whether twenty seconds have passed or fifty. The one thing it is
 * borrowed from `sinceLabel` for is the nought — "0 sec ago" is a reading of a clock, and
 * "just now" is what a person says.
 */
function ago(seconds: number | null): string | null {
  if (seconds == null) return null
  if (seconds === 0) return 'just now'
  return `${durationLabel(seconds)} ago`
}

const signedOffLabel = computed(() => ago(sinceSignOff.value) ?? 'at an unrecorded time')

/**
 * When anything last wrote to this review, or that the record does not say.
 *
 * Null is every review from before the record started keeping it, and it is not the same
 * answer as a long time ago — so it is words rather than a figure, which is the rule the
 * checks above this panel follow for a count the server cannot answer.
 */
const lastWrittenLabel = computed(
  () => ago(secondsSince(props.lastActivityAt, now.value)) ?? 'Not recorded',
)

const postedLabel = computed(() => ago(secondsSince(props.draft.posted_at, now.value)))

/**
 * How long the session took, from the sign-off to the posting it reported.
 *
 * Both stamps or nothing: a draft marked posted by a session that reported no time, or one
 * carried over from before the draft held `approved_at`, has no span to state.
 */
const tookToPost = computed(() => {
  const posted = props.draft.posted_at
  if (!posted) return null
  return durationLabel(secondsSince(props.draft.approved_at, new Date(posted)))
})

const postedTo = computed(() => postedReview(props.review))

/** The chosen event in the developer's own words, or that the record does not hold one. */
const postedAs = computed(() =>
  props.draft.event ? eventLabel(props.draft.event) : 'not recorded',
)

/*
 * The three paragraphs, built here rather than out of `<template>` fragments.
 *
 * `SignOffCard` learned this and the reason is the renderer's: the linter requires each
 * fragment on its own line, and Vue then puts a space before the comma and before the full
 * stop. A sentence is one string. It is also what keeps the two interval settings and
 * `eventLabel` referenced from the script, where Nuxt's auto-import can see them — an
 * identifier a `<script setup>` never names is not in the template's scope.
 */

const whoPosts = computed(
  () =>
    'This review’s own session posts it, not Review Desk — it reads the desk about every '
    + `${WATCHER_READS_EVERY_SECONDS} seconds while a draft is waiting, so one that is `
    + `running posts within about ${POSTED_WITHIN_SECONDS} seconds. Whether it is running `
    + 'is not something Review Desk is told; the time above is how long the review has been '
    + 'silent, which is the nearest thing the record holds.',
)

/**
 * The sentence the long wait adds, which states nothing the paragraph above it already has.
 *
 * The forty seconds are stated once, up there, as what a running session takes. Repeating
 * them here as what this one has exceeded was two statements of one number a line apart —
 * and the number a developer wants at this point is how long *they* have been waiting.
 */
const lateSentence = computed(
  () =>
    `Nothing has reported it posted in the ${durationLabel(sinceSignOff.value)} since you `
    + 'signed off, so the session may not be running. Check the session that was reviewing '
    + 'this, or take the sign-off back and send the draft back with a note — a session that '
    + 'picks it up later writes the draft again.',
)

const noAnchorSentence = computed(
  () =>
    `The session reported the review as ${props.review.github_review_id}, which is not the `
    + 'number GitHub builds its own address for a review from — so this link goes to the '
    + 'pull request, where the review is, rather than to an anchor that matches nothing.',
)
</script>

<template>
  <div class="watch">
    <!--
      **The live region, holding the state and never a figure.** Always drawn, including
      empty, so that it is in the page before there is anything to announce. `role="status"`
      is polite and moves nothing: the developer who pressed the button keeps the focus they
      had, which is what makes this readable rather than an interruption.
    -->
    <p
      class="said"
      role="status"
    >
      {{ announcement }}
    </p>

    <template v-if="waiting">
      <FactRow
        label="Signed off"
        :value="signedOffLabel"
        numeric
      />
      <FactRow
        label="Last written to"
        :value="lastWrittenLabel"
        :numeric="lastActivityAt != null"
      />
      <p class="rd-meta who">
        {{ whoPosts }}
      </p>
      <!--
        **A plain sentence, not an alarm.** Nothing has gone wrong with the record: the
        draft is signed off, it is still postable, and what is in doubt is a session this
        app cannot see. A boxed warning here would be the desk raising its voice about
        something it is guessing at.
      -->
      <p
        v-if="late"
        class="rd-note late"
      >
        {{ lateSentence }}
      </p>
    </template>

    <template v-else-if="draft.status === 'posted'">
      <FactRow
        v-if="postedLabel"
        label="Posted"
        :value="postedLabel"
        numeric
      />
      <FactRow
        label="Posted as"
        :value="postedAs"
      />
      <FactRow
        v-if="tookToPost"
        label="Taken to post"
        :value="tookToPost"
        numeric
      />
      <a
        v-if="postedTo"
        class="out"
        :href="postedTo.href"
        target="_blank"
        rel="noreferrer noopener"
      >{{ postedTo.exact ? 'See the review on GitHub' : 'See the pull request on GitHub'
      }}<span class="rd-visually-hidden">, opens in a new tab</span></a>
      <p
        v-if="postedTo && !postedTo.exact"
        class="rd-meta who"
      >
        {{ noAnchorSentence }}
      </p>
      <p
        v-else-if="!postedTo"
        class="rd-meta who"
      >
        Nothing reported which review on GitHub this became, so there is no link to it. The
        session records that with the posting, and this one did not.
      </p>
    </template>
  </div>
</template>

<style scoped>
.watch {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/*
 * The live region. No height of its own while it is empty, which is every draft nobody has
 * signed off — and **not** `display: none` there, however tempting: a region out of the
 * accessibility tree until the moment it has something to say is a region a screen reader
 * may never announce.
 */
.said {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
}

.who,
.late {
  margin: 0;
}

.late {
  padding-top: 8px;
  border-top: 1px solid var(--rd-rail);
}

.out {
  align-self: flex-start;
  font-weight: 500;
}
</style>
