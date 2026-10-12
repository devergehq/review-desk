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
  /**
   * Whether the page's last read of this review answered.
   *
   * **Only one sentence here depends on it, and it is the one that sends somebody
   * somewhere.** A failed read leaves the last good answer on screen — `resource()` keeps
   * it and `DeskState` draws the refusal over the top — so the props still say `approved`
   * however long the server has been gone, and the clock keeps moving. Without this the
   * page would say "nothing has reported it posted in the 4 min since you signed off, so
   * the session may not be running" about a session that is posting away happily, while
   * the thing it cannot reach is its own server two lines above. Review Desk accusing
   * Locus of being down because Review Desk is down is the worst sentence on the page.
   */
  recordIsCurrent: boolean
  /**
   * Read the review and its draft again. The page owns the reads; this only asks.
   *
   * **A function rather than an emit, because this has to know when the answer landed.**
   * An emit is fire-and-forget, and moving the clock on the way *out* rather than on the
   * way *back* is what let a hung server look like a dead session: a server that accepts
   * a connection and never answers leaves no error to catch, so `recordIsCurrent` stays
   * true while the clock walks past forty seconds. Awaiting the read makes the clock stop
   * when the answers stop, which is the honest thing for it to do and needs no flag.
   */
  reread: () => Promise<void>
}>()

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
 * **Something is coming after a send-back too, and this deliberately does not wait for
 * it.** The session rewrites the draft and puts it back to `ready`, which is a second wait
 * on a second system and the same not-knowing in the other direction — and DEV-932 scoped
 * itself to the first: "re-reading the review every few seconds until the draft is posted
 * **or sent back**, and stopping then" is the issue's own criterion. Watching the way back
 * is a change to what this waits for, not a bug in what it waits for now.
 *
 * **A stale body is not waiting on anything.** The hand-over refuses it, so no session will
 * post it however long this watched, and "the session may not be running" would be the
 * wrong diagnosis of a state whose cause is known. The sentence above the sign-off says
 * what it is and offers the only way out, which is an act on this page — and an act on
 * this page re-reads everything anyway.
 */
const waiting = computed(() => props.draft.status === 'approved' && !props.bodyIsStale)

/**
 * Whether this page has watched the status change at all, on the draft in front of it.
 *
 * **A live region is for news, and a status this page did not watch arrive is not news.**
 * One draft page serves every review, so going from a review being waited on to one posted
 * days ago rewrites the region's text, and a screen reader reads "Posted on GitHub." as
 * though it had just happened. The region stays empty until this page sees something move,
 * which is the rule a fresh page load already followed and for the same reason.
 */
const sawAChange = ref(false)

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

/**
 * What this page last saw on the draft in front of it. Plain, not reactive: it is the
 * watcher's own memory, read and written in one place.
 */
let seen = { review: props.draft.review_id, status: props.draft.status }

/**
 * **One watcher, keyed on a string, over the two facts that decide what this says.**
 *
 * Keyed rather than taken as a tuple: `watch` over a getter returning a fresh array is
 * never `Object.is`-equal to its last value, and `props.draft` is a new object after every
 * read — so a tuple watcher ran on all twelve ticks of a minute's waiting and a guard
 * inside it was the only thing stopping the clock resetting each time. A string changes
 * when one of the two does and not otherwise.
 *
 * **And one watcher rather than two, because a navigation changes both at once.** Split
 * into a watcher per fact, going from a review being waited on to one posted days ago
 * fired both, and whichever ran second won — which was the status, so the page announced a
 * days-old posting as news. The two cases are mutually exclusive and are decided here
 * together.
 *
 * **A different review is not news about this one.** One draft page serves every review,
 * so the router keeps the page and the resources re-read underneath it. Nothing this
 * remembers was about the review now in front of it, so it starts again — which is also
 * why the clock moves, since the figures below are about the new one.
 */
watch(
  () => `${props.draft.review_id} ${props.draft.status}`,
  () => {
    const { review_id: review, status } = props.draft

    if (review !== seen.review) {
      /*
       * **The clock first, and `inTime()` rather than `late`.** `now` is frozen for
       * every status this panel does not wait on, so a page open two minutes on a draft
       * nobody had signed off and then clicked through to a review signed off sixty
       * seconds ago judged the new draft against the old review's clock, found it in
       * time, and announced the accusation a moment later as though it had just arrived.
       * Moving the clock is half of it; the other half is not asking the held conclusion,
       * which this flush has not updated yet.
       */
      now.value = new Date()
      watchedTheSignOff.value = status === 'approved'
      sawAChange.value = false
      sawItInTime = inTime()
    }
    else {
      sawAChange.value = true
      if (status === 'approved') watchedTheSignOff.value = true
      else if (status === 'ready' || status === 'drafting') watchedTheSignOff.value = false
    }

    seen = { review, status }
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
  timer = setInterval(() => void readNow(), RE_READ_EVERY_SECONDS * 1000)
}

/**
 * Ask the page to read, and move the clock to when it answered.
 *
 * **One read at a time.** The timer fires every five seconds whether or not the last read
 * came back, so against a server that has stopped answering the requests would otherwise
 * stack two deep a tick. A skipped tick costs nothing: the next one is five seconds away
 * and the read it skipped is still in flight.
 *
 * The clock moves *after* the answer, which is the whole reason this is a function and not
 * an emit. Two things fall out of it. A server that hangs stops the clock rather than
 * letting it walk past the threshold over a record that is no longer arriving. And coming
 * back to a tab hidden across the posting no longer renders — or announces — the
 * accusation for the length of a round trip, because the figures are drawn from the answer
 * rather than from the moment the question was asked.
 */
let reading = false

async function readNow(): Promise<void> {
  if (reading) return
  reading = true
  try {
    await props.reread()
    now.value = new Date()
  }
  finally {
    reading = false
  }
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
   * part of the desk that is told when you came back.
   *
   * For a draft still waiting the clock moves inside `readNow`, with the answer, so the
   * figures on return are drawn from what the record says and not from what it said ten
   * minutes ago.
   */
  if (!waiting.value) {
    now.value = new Date()
    return
  }
  void readNow()
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
 *
 * **And it needs the record to be current, not merely old**, which takes two things.
 * `recordIsCurrent` covers a read that was refused: the answer is kept on screen and the
 * refusal drawn over it, so without the flag the clock would walk past the threshold over
 * a record nobody could reach. A read that is accepted and never answered leaves no
 * refusal to catch, and that one is covered by where the clock is moved — inside
 * `readNow`, after the answer — so when the answers stop, so does the clock.
 *
 * **It is a conclusion this page has drawn, not a value recomputed from the last read —
 * and the difference is four reviews' worth of bugs.** As a computed gated on
 * `recordIsCurrent` it went false the moment one poll was refused and true again when the
 * next answered, so the sentence vanished and came back on every hiccup, the live region
 * retracted the diagnosis and read it out again, and three rounds of flags were added to
 * stop the flap being mistaken for news. Those flags were patches at the wrong level.
 *
 * Drawn once and held: nothing concludes lateness without a read that answered, and
 * nothing un-concludes it because a later read did not. It is dropped only when the draft
 * stops waiting, which is the posting or the take-back the whole panel is watching for.
 */
const late = ref(false)

watch(
  [waiting, sinceSignOff, () => props.recordIsCurrent],
  ([isWaiting, elapsed, current]) => {
    if (!isWaiting) {
      late.value = false
      return
    }
    /* Unreachable: hold what is on screen. Retracting it would be a second guess. */
    if (!current) return
    late.value = (elapsed ?? 0) >= POSTED_WITHIN_SECONDS
  },
  { immediate: true },
)

/**
 * Whether this page has seen this draft waiting and *not yet* late.
 *
 * **A page that opened on a draft already late never watched it become late**, so the
 * threshold crossing is not news there and the region stays quiet. This says whether the
 * page was there to see it drawn.
 *
 * Read from the facts rather than from `late`, which is a held conclusion and lags by a
 * flush: the watcher that maintains it runs after the one that moves the clock, so asking
 * `late` the instant a navigation lands gets the answer for the review just left.
 */
function inTime(): boolean {
  return waiting.value
    && props.recordIsCurrent
    && (sinceSignOff.value ?? 0) < POSTED_WITHIN_SECONDS
}

let sawItInTime = inTime()

/**
 * The wait passing the threshold is news too, and keying news off the status alone lost it.
 *
 * **A page loaded straight into a signed-off draft never sees its status change.** The
 * poll returns the same review at the same status, the keyed watcher never fires, and
 * `sawAChange` stays false — so a developer who signed off and then reloaded the tab got
 * the sentence drawn and never announced. The one sentence on this panel that sends
 * somebody somewhere, silent for exactly the reader who cannot see it. Worse, the same
 * page announced the *posting* afterwards, because that does change the status.
 *
 * News is the threshold being *crossed* on a draft this page watched be in time for it,
 * which is why the flag above exists and why this is not `immediate`.
 */
watch(late, (yes) => {
  if (yes) {
    if (sawItInTime) sawAChange.value = true
    return
  }
  sawItInTime = waiting.value && props.recordIsCurrent
})

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
  if (!sawAChange.value) return ''
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

const signedOffLabel = computed(
  () => agoLabel(props.draft.approved_at, now.value) ?? 'at an unrecorded time',
)

/**
 * When anything last wrote to this review, or that the record does not say.
 *
 * Null is every review from before the record started keeping it, and it is not the same
 * answer as a long time ago — so it is words rather than a figure, which is the rule the
 * checks above this panel follow for a count the server cannot answer.
 */
const lastWrittenLabel = computed(
  () => agoLabel(props.lastActivityAt, now.value) ?? 'Not recorded',
)

const postedLabel = computed(() => agoLabel(props.draft.posted_at, now.value))

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
 * Why the two figures above usually read the same, said rather than left to be puzzled at.
 *
 * **The sign-off is itself a write**, so it moves `last_activity_at` to `approved_at` in
 * the same transaction, and the API refuses the writes that could move it again while the
 * draft is approved. So for most of this state the two rows are one number twice, and two
 * identical figures stacked read as two measurements that happen to agree. They are not:
 * the second is the first until something other than the posting writes, and the day they
 * differ is the day that sentence is worth having.
 */
const sameFigureTwice = computed(
  () => props.lastActivityAt != null
    && props.draft.approved_at != null
    && secondsSince(props.lastActivityAt, now.value)
    === secondsSince(props.draft.approved_at, now.value),
)

const bothTheSignOff
  = 'Those are one time twice: signing off is itself a write, so it is the last thing to '
    + 'have touched this review. They part company only if something other than the '
    + 'posting writes to it.'

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
/**
 * What a sighted reader is told when a sign-off is taken back under them.
 *
 * **The state table above said the panel "says" this and the panel did not.** The sentence
 * existed only in the hidden region, so a screen-reader user was told the sign-off had
 * gone and everyone else got an empty panel and the status line's "Sent back to the
 * session with a note." — which cannot tell a withdrawn sign-off from an ordinary
 * send-back, the one distinction `watchedTheSignOff` exists to draw.
 *
 * It adds what the announcement cannot fit rather than repeating it: where the draft is
 * now, and that nothing is waiting on the developer until a session writes it again.
 */
const takenBack
  = 'The sign-off is taken back and the draft is with the session again. Nothing here is '
    + 'waiting on you until a session writes it afresh.'

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

      **Always hidden, too, because announcing and printing are different jobs.** Drawn, it
      said the long wait's diagnosis in bold and the paragraph four lines below said it
      again — "nothing has reported it posted … may not be running", twice in one card, in
      a picture this branch committed. What a sighted reader needs is here already: the
      heading, the figures and the sentence under them. What a screen reader needs is to be
      told when that changed, which is all this is now.
    -->
    <p
      class="rd-visually-hidden"
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
      <p
        v-if="sameFigureTwice"
        class="rd-meta who"
      >
        {{ bothTheSignOff }}
      </p>
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

    <template v-else-if="draft.status === 'sent_back' && watchedTheSignOff">
      <p class="rd-note">
        {{ takenBack }}
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
