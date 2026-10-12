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
 *
 * The exception is coming back to the tab, where it moves whatever the status — see
 * `onVisibilityChange`. A figure that has stopped is stale; one that has stopped across an
 * hour the page was told about is wrong.
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

/**
 * Whether the last thing this page saw on this draft was a sign-off.
 *
 * **Cleared as well as set**, because it is "the last thing", not "ever". A draft can go
 * back to `ready` — the session rewrites one that was sent back — and an ordinary
 * send-back from there is not a sign-off being taken back. It takes a page left open
 * across both to get there and nothing would crash if it latched, but a flag that only
 * ever goes one way is a sentence that becomes wrong and stays wrong.
 */
const watchedTheSignOff = ref(props.draft.status === 'approved')

watch(
  () => [props.draft.review_id, props.draft.status] as const,
  ([review, status], [wasReview]) => {
    /*
     * **A different review in the same component.** One draft page serves every review, so
     * going from `/reviews/17/draft` to `/reviews/15/draft` changes these props without
     * mounting anything: the router keeps the page and the two resources re-read. Nothing
     * this remembers was about the review now in front of it, so it starts again — which
     * is also why the clock moves, since the figures below are about the new one.
     */
    if (review !== wasReview) {
      watchedTheSignOff.value = status === 'approved'
      now.value = new Date()
      return
    }
    if (status === 'approved') watchedTheSignOff.value = true
    else if (status === 'ready' || status === 'drafting') watchedTheSignOff.value = false
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
  /*
   * **The clock moves whether or not anything is still being waited for**, and that is the
   * one place this panel departs from "a time is as fresh as the page". A posted draft
   * whose figure stops moving is stale, which the desk accepts everywhere; a posted draft
   * that says "12 sec ago" an hour after you left the tab is *wrong*, and this is the only
   * part of the desk that is told when you came back. It costs one recompute: no timer
   * starts for a draft nothing is coming to.
   */
  now.value = new Date()
  if (!waiting.value) return
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
 *
 * **It compares a server stamp to a browser clock, and here that is safe.** Everywhere
 * else that arithmetic can only mis-word a label; this one emits a diagnosis the developer
 * will act on, so it is worth saying why a skew cannot cause a false accusation. Review
 * Desk serves from a binary on loopback to a browser on the same machine — one clock, read
 * twice — and `now()` in the store is `as_secs()`, which truncates down, so the browser's
 * reading is at most a second ahead of the stamp and never behind it. A build that ever
 * served this page from another machine would need a reference instant on the wire before
 * this sentence could be trusted.
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
 *
 * **That covers a change, which is all a live region is for.** A page *loaded* on a draft
 * that is already waiting or already posted draws the region and its sentence in the same
 * patch and announces nothing, and that is right: the sentence is page content then, read
 * when the developer reads the panel, and a page that spoke a line at every load would be
 * announcing something nobody did.
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
 *
 * Null where the record holds no instant, so each caller says in its own words what that
 * absence is: an unrecorded sign-off and a review nothing has written to are both nulls
 * and are not the same sentence.
 */
function agoLabel(at: Instant | null | undefined): string | null {
  const seconds = secondsSince(at, now.value)
  if (seconds == null) return null
  if (seconds === 0) return 'just now'
  return `${durationLabel(seconds)} ago`
}

const signedOffLabel = computed(
  () => agoLabel(props.draft.approved_at) ?? 'at an unrecorded time',
)

/**
 * When anything last wrote to this review, or that the record does not say.
 *
 * Null is every review from before the record started keeping it, and it is not the same
 * answer as a long time ago — so it is words rather than a figure, which is the rule the
 * checks above this panel follow for a count the server cannot answer.
 */
const lastWrittenLabel = computed(() => agoLabel(props.lastActivityAt) ?? 'Not recorded')

const postedLabel = computed(() => agoLabel(props.draft.posted_at))

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

/* Two settings and no props, so a constant: there is nothing here for a computed to track. */
const whoPosts
  = 'This review’s own session posts it, not Review Desk — it reads the desk about every '
    + `${WATCHER_READS_EVERY_SECONDS} seconds while a draft is waiting, so one that is `
    + `running posts within about ${POSTED_WITHIN_SECONDS} seconds. Whether it is running `
    + 'is not something Review Desk is told; the time above is how long the review has been '
    + 'silent, which is the nearest thing the record holds.'

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

/**
 * What there is to say where the record holds no GitHub review for a posted draft.
 *
 * **Not reachable through this API, and drawn anyway.** `post_draft` writes `posted_at`
 * and the review's `github_review_id` in one go — the contract calls them one fact — and
 * `draft posted` requires the id on the command line, so nothing can write one without
 * the other. But the field is nullable in the contract, which makes this a state a client
 * has to be able to render, and the honest rendering says what the record holds rather
 * than guessing at how it got that way or drawing a link to nowhere.
 */
const noReviewSentence
  = 'The record holds no GitHub review for this draft, so there is nothing to link to. '
    + 'Nothing this API writes can leave it that way — a session reports the id when it '
    + 'reports the posting — so this is a record something else has written.'

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
      :class="{ 'said--waiting-to-speak': announcement.length === 0 }"
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
        {{ noReviewSentence }}
      </p>
    </template>
  </div>
</template>

<style scoped>
/*
 * **`contents`, so that a panel with nothing to say is the panel it was.** These rows are
 * laid out by the card around them — `.panel` is a column with the same 10px gap — and a
 * box of its own here was one more flex child, which cost a gap on each side: every draft
 * nobody had signed off gained ten pixels of blank space above the card's last line. Two
 * reviews called that incidental and it was.
 */
.watch {
  display: contents;
}

/*
 * The live region. **Not `display: none` while it is empty, however tempting**: a region
 * out of the accessibility tree until the moment it has something to say is a region a
 * screen reader may never announce. Taken out of the flow instead, which keeps it in the
 * tree and in the page while costing the layout nothing — `contents` above only works
 * because of this, since an empty paragraph is a flex child like any other.
 */
.said {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
}

.said--waiting-to-speak {
  position: absolute;
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
