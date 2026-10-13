<script setup lang="ts">
import type { DraftDetail, NotForSigning, ReviewEvent } from '~/types/desk'
import type { WriteResult } from '~/composables/useReviewDesk'
import type { DeskNotice } from '~/utils/format'

import { withdrawnLine } from '~/utils/rounds'

/**
 * Step 6 — **the draft**: what GitHub will show, before anything is posted.
 *
 * Nothing on this screen posts. The developer signs off, and the review session posts it
 * under their name with no bot banner — a preview that could post would be a bot.
 *
 * ## It is inside the frame now, and that is what killed a dead end
 *
 * DEV-878: "The draft screen is drawn inside the frame with the trail, and the trail's step
 * 5 and step 6 lead to these pages." So this reads two things: the review, for the frame and
 * the trail, and the draft, for everything below them.
 *
 * **The draft read is allowed to fail.** `GET …/draft` refuses when no session has written
 * one, and that used to make step 6 a label rather than a link — a link would have landed on
 * a blocking not-found panel carrying no trail and no way back. Inside the frame there is no
 * such panel: the trail is drawn either way and this page says, in its own words, that there
 * is nothing to read yet. The guard in `StepTrail` went with the dead end it was for.
 *
 * So the review is the resource `DeskState` renders and the draft is read beside it.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const review = desk.review(reviewId)
const detail = desk.draft(reviewId)

useHead({
  title: computed(() =>
    detail.data.value ? `Draft · #${detail.data.value.review.pr} · Review Desk` : 'Draft · Review Desk',
  ),
})

/**
 * The page's one heading: where the draft stands, in a few words.
 *
 * Short, because the sentence under it says the rest — `statusLine` below is what this page
 * has always said and is unchanged. A heading that repeated it would be the same sentence
 * at two sizes.
 */
const heading = computed(() => {
  if (detail.error.value?.code === 'not_found') return 'No draft yet.'
  const status = detail.data.value?.draft.status
  if (status === 'posted') return 'Posted.'
  /*
   * **Before the status, because it outranks it.** "Read it, then sign it off." is an
   * instruction to do the one thing this page is refusing, and "Signed off." over a draft
   * that merged says it is on its way to GitHub when nothing is going to take it there.
   * DEV-957's screenshots showed the first of those over "Not posted — the pull request
   * merged". A posted draft is exempt and is answered above: it did post.
   */
  if (detail.data.value && notForSigning(detail.data.value)) return 'Nothing left to do.'
  if (status === 'approved') return 'Signed off.'
  if (status === 'sent_back') return 'Sent back.'
  if (status === 'drafting') return 'Still being written.'
  return 'Read it, then sign it off.'
})

type View = 'rendered' | 'markdown' | 'edit'
const view = ref<View>('rendered')
const views: Array<{ key: View, label: string }> = [
  { key: 'rendered', label: 'Rendered' },
  { key: 'markdown', label: 'Markdown' },
  { key: 'edit', label: 'Edit' },
]

const editing = ref('')
const event = ref<ReviewEvent>('request_changes')
const note = ref('')
const sendingBack = ref(false)
const working = ref(false)
const notice = ref<DeskNotice | null>(null)

/*
 * Whether the developer is being asked why the rest is going to a ticket. Up here with the
 * rest of the page's own state rather than beside `ticketRest` below, as `sendingBack` and
 * `note` are: the watch that clears every one of them when the review changes reads them
 * all, and reads nothing declared after it.
 */
const ticketingRest = ref(false)
const ticketNote = ref('')

/**
 * The body the editor last loaded, which is what a save compares against.
 *
 * Kept apart from `editing`, because a refusal must not lose what the developer typed:
 * when a save is refused, `editing` keeps their text and this keeps what the record still
 * holds, so the next attempt is still an edit of the same thing.
 */
const loadedBody = ref('')

watch(
  () => detail.data.value,
  (value) => {
    if (!value) return
    /*
     * The editor is only re-seeded while it has nothing unsaved in it. A write refreshes
     * every open read, so a refused save re-runs this — and overwriting the textarea here
     * would discard the edit the refusal was about.
     *
     * One condition, not two. An `editing.value === ''` arm looked like it covered the
     * first load, which the comparison below already does — both start empty — and what it
     * really covered was a textarea the developer had deliberately cleared before pasting a
     * rewrite. Any write from elsewhere on the screen then refilled it with the old body.
     */
    if (editing.value === loadedBody.value) {
      editing.value = value.draft.body
    }
    loadedBody.value = value.draft.body
    if (value.draft.event) event.value = value.draft.event
  },
  { immediate: true },
)

const dirty = computed(() => editing.value !== loadedBody.value)

/* ---------------- Unsaved edits belong to the review they were typed on ---------------- */

/**
 * Another review on this page is read from its own record, with nothing carried over.
 *
 * ## It cannot fire today, and the reason is worth more than the watch
 *
 * **This page is not reused when the review in the address changes.** DEV-960 says it is,
 * and that is true of a bare `<RouterView>` — it is not true of `<NuxtPage />` without a
 * `pageKey`, which keys a page by its own path with the parameters interpolated. Changing
 * the review changes the key, so Vue unmounts this component and mounts a fresh one.
 *
 * **Measured, not read off the documentation.** Against this file as it stood before the
 * change: unsaved text typed on one review's draft, a history jump to another review's
 * draft, and the arriving page had the Edit tab unselected and the text nowhere on it. The
 * carry-over the issue describes — and with it the save into the wrong draft — does not
 * happen here. What does happen is that the typing is destroyed without a word, which is
 * what the guards below are for and the only part of this change that alters behaviour.
 *
 * ## So why it is here
 *
 * Because it is this page's own statement of what belongs to one review, and the thing that
 * makes the page correct whichever way `NuxtPage` is keyed. A `pageKey`, or a
 * `definePageMeta({ key })`, added later for any reason — the obvious one is stopping the
 * remount flash between two rounds of a pull request — turns every ref below into exactly
 * the defect the issue describes, and this watch is then what stands between review 50's
 * prose and review 51's draft. **Necessary and not sufficient**, which the sentence here
 * used to claim: `ofThisReview` below would be carrying the five writes, and the read that
 * is still in flight would be drawing review 50's threads, held-back rows and sign-off under
 * review 51's address. Raised by a review of this branch. It costs ten lines and it is
 * already the house pattern: DEV-958's page carries the same watch.
 *
 * As written it fires on an instance that is about to be unmounted — the global route
 * updates before Vue re-renders with the new key — so it is a no-op rather than dead code.
 *
 * **Both halves of the comparison are cleared, which is what would make the next read a
 * first load**: they are equal, so the watch above seeds the editor from the arriving
 * review's own body and `dirty` is false.
 *
 * `view` goes back to Rendered with the rest. `resource()` keeps the previous answer while
 * the next read is in flight, so a page that *was* reused would still be drawing review 50
 * under review 51's address; taking the textarea off screen is what would stop something
 * typed in that window being saved against the review the page had left.
 *
 * `working` is deliberately not here. It is a write in flight rather than a view of a
 * review, and clearing it would re-enable the controls a running write has disabled.
 */
watch(reviewId, () => {
  editing.value = ''
  loadedBody.value = ''
  event.value = 'request_changes'
  view.value = 'rendered'
  note.value = ''
  sendingBack.value = false
  ticketNote.value = ''
  ticketingRest.value = false
  notice.value = null
})

/**
 * The navigation waiting on the developer's choice, or null when none is.
 *
 * One ref, holding the pending navigation's own resolve. Its being non-null **is** "a
 * choice is being asked for", so there is no second flag for the card to read that could
 * fall out of step with the navigation it is about.
 */
const leaving = ref<{ decide: (go: boolean) => void } | null>(null)

/**
 * Ask, in the page, before unsaved edits are left behind — and make the navigation wait.
 *
 * **This is the part of DEV-960 that changes what the page does.** The watch above cannot
 * fire, for the reason it states; what was really wrong is that leaving this page threw the
 * developer's typing away and said nothing — the component is unmounted either way, and an
 * unsaved draft is not on the record anywhere.
 *
 * **In the page and not in a browser dialog**, which is DEV-960's own wording and is also
 * the only version that can name the review: `beforeunload` shows the browser's sentence,
 * not ours. The guard returns a promise, so the navigation is held open until a button is
 * pressed rather than being cancelled and re-offered.
 *
 * **The guards run on the live instance**, before the navigation is confirmed and so before
 * any key change can take the component down. That is why they work where the watch does
 * not.
 *
 * **Nothing is done for a full reload, deliberately.** The only tool for one is
 * `beforeunload`, and it is the dialog this is written to avoid. It would also buy nothing:
 * a reload re-mounts this page with an empty editor and re-seeds it from the record, so no
 * typed text survives to be applied to a different review. What a reload loses is the edit
 * itself, which is what a reload has always meant.
 *
 * A second navigation while the card is up resolves the first as a refusal. The router has
 * already abandoned it, and a promise nobody resolves is a navigation that never settles.
 */
function askBeforeLeaving(): boolean | Promise<boolean> {
  if (!dirty.value) return true
  leaving.value?.decide(false)
  return new Promise<boolean>((decide) => {
    leaving.value = { decide }
  })
}

/*
 * **Both guards, because the two ways out are different route changes.** Another review's
 * draft is an *update* — the same page record with a new parameter, which is the route the
 * issue was raised about — and any other page is a *leave*. Neither fires for the other.
 *
 * **And the path has to change, or it is not a way out at all.** The layout's first control
 * is `<a href="#main">Skip to the page</a>`, and vue-router 5 sees a fragment jump from a
 * plain anchor as a navigation on this route: without this test, a keyboard user with
 * unsaved edits met the card every time they skipped the header, and the guard had turned an
 * accessibility control into an interruption. Measured, not reasoned about — the first
 * version of this was `() => askBeforeLeaving()` and the test for it failed.
 *
 * `to.path`, so a query or a fragment on this same page is not asked about and a Back out of
 * one is not either. What the card is for is the page going away, which is the only thing
 * that takes the editor with it.
 */
onBeforeRouteUpdate((to, from) => (to.path === from.path ? true : askBeforeLeaving()))
onBeforeRouteLeave((to, from) => (to.path === from.path ? true : askBeforeLeaving()))

const keep = useTemplateRef<HTMLButtonElement>('keep')

/*
 * **Focused when it appears**, and on Keep editing rather than on Discard. The card is
 * drawn at the top of a page that is often scrolled past it, so without this the developer
 * presses a link, nothing appears to happen, and the answer is above the fold. The safe
 * choice takes the focus because Enter is the key a keyboard reaches for first.
 */
watch(leaving, async (waiting) => {
  if (!waiting) return
  await nextTick()
  keep.value?.scrollIntoView({ block: 'center' })
  keep.value?.focus()
})

/*
 * **And it opens the editor, because that is what the button says.** `dirty` is about the
 * textarea's contents and not about which tab is showing, so the card can come up over the
 * Rendered preview — the developer typed, switched back to read it, and then pressed a
 * link. Keeping the editing while leaving them on a tab that does not show it would be the
 * control not doing the one thing it is named for.
 */
function keepEditing() {
  const waiting = leaving.value
  leaving.value = null
  view.value = 'edit'
  waiting?.decide(false)
}

/*
 * Discard lets the navigation through and clears nothing itself: arriving at another review
 * runs the watch above, and leaving this page altogether takes the component with it. A
 * third copy of the same clearing here would be a third place for it to be got wrong.
 */
function discardEdits() {
  const waiting = leaving.value
  leaving.value = null
  waiting?.decide(true)
}

/*
 * **A save landing while the card is up answers it, and the navigation goes through.**
 *
 * `dirty` can go false underneath the card. Type, press Save the draft, and press a link
 * while the PUT is in flight: the guard sees unsaved edits because `loadedBody` has not been
 * refreshed yet, the card comes up, and then the write's own refresh lands and there is
 * nothing unsaved left to ask about. The card was then asking about edits that are on the
 * record — Discard the edits discarding nothing, Keep editing holding the developer back
 * for no reason either of them could see.
 *
 * **Answered here and deliberately not with `v-if="leaving && dirty"`.** That is the obvious
 * version and it is the one that wedges the page: it takes away the only two controls that
 * can resolve the promise, leaving a navigation nobody can settle and a page nobody can
 * leave. Raised by a review of this branch, which named both the sequence and the trap.
 *
 * **It also covers the developer undoing their own typing while the card is up**, which the
 * card does not prevent — the editor is still on screen behind it. They pressed the link,
 * so the navigation is one they asked for; what the card asked about has stopped existing,
 * and holding them on the page until they answer a question with no subject would be the
 * worse of the two. So the navigation goes through on the keystroke rather than on either
 * button, and that is the intended reading of it.
 */
watch(dirty, (unsaved) => {
  if (unsaved || !leaving.value) return
  discardEdits()
})

/**
 * The aside's two derived lists, computed rather than called from the template.
 *
 * **The editor is in the same slot.** `v-model="editing"` on the textarea is tracked by the
 * slot's render effect, so a call in the template re-ran both derivations on every keystroke
 * — and, because each handed back a fresh array, re-rendered the whole held-back panel with
 * them. Measured by review at 76µs a keystroke against 7µs from a stable identity: sub-
 * millisecond either way and nothing a reader could feel, which is why this is three lines
 * and not a component.
 *
 * They read the two resources rather than the slot's `data`, which is the same object: the
 * slot cannot be closed over, and it does not need to be.
 */
const editRows = computed(() => editLines(detail.data.value?.edits ?? []))

const heldRows = computed(() =>
  heldBackRows(detail.data.value?.held_back ?? [], review.data.value?.findings ?? []),
)

function decided(data: DraftDetail): boolean {
  return data.draft.status !== 'ready'
}

/**
 * Why nothing on this review may be signed off, or null when it may.
 *
 * **Served, not worked out here**, which is the point of the field: whether the pull
 * request has finished is read off whichever round of it a harvest last reached, and this
 * page holds one review. `review.pr_state` is this round's own column and is null for
 * three of the four rounds of a merged pull request.
 *
 * **Null for a posted draft**, whatever the pull request did. A review that posted did
 * post; the sentence DEV-957 asks for is about a draft that never will, and "not posted"
 * over a posted review would be the page contradicting the record. The sign-off is not
 * offered on a posted draft anyway — `decided` is already true of it.
 *
 * **So this answers "what does the page say", and `offTheDesk` below answers "what will
 * the API refuse".** They are not the same question and treating them as one put a live
 * button on a posted review: `request_nit_ticket` has no posted exemption — a posted round
 * can still hold back nits nobody has ticketed, and the store refuses the ask on a merged
 * pull request whatever the draft did. Found by a review of this branch.
 */
function notForSigning(data: DraftDetail): NotForSigning | null {
  if (data.draft.status === 'posted') return null
  return data.not_for_signing ?? null
}

/**
 * Why the sign-off was withdrawn, or null when none was.
 *
 * **One copy of the sentence**, in `utils/rounds`, because the commit the developer
 * approved is on the draft and the commit the pull request is on now is on the review —
 * two records, one sentence, and a page composing it for itself is a second place for it
 * to be worded.
 */
function withdrawn(data: DraftDetail): string | null {
  return withdrawnLine(data.draft, data.review)
}

/**
 * Whether the API will refuse the four acts on this review, posted or not.
 *
 * The record's own answer, read straight. Every control for one of those four is gated on
 * **this**; what the page *says* is `notForSigning` above, which exempts a posted draft
 * because the sentence it draws would be false of one.
 */
function offTheDesk(data: DraftDetail): boolean {
  return data.not_for_signing != null
}

/**
 * The sentence a draft that cannot be signed off carries, in place of the status line.
 *
 * Three reasons, three sentences, each naming the thing that happened and what follows
 * from it. **The merge names the day when the record holds one** and stops after the word
 * when it does not: GitHub does not always give a time, and a date nobody holds must not
 * be invented — `dayLabel` returns null and the clause is left out.
 */
function notForSigningLine(why: NotForSigning): string {
  if (why.reason === 'dismissed') {
    return 'Not posted — you dismissed this review. Bring it back from the Reviews list '
      + 'if it should be posted after all.'
  }
  if (why.reason === 'superseded') {
    return `Not posted — superseded by round ${why.by_round}. The author pushed past the `
      + 'commit this was written against, so this draft can never be the review that '
      + 'posts; round ' + `${why.by_round}` + ' is the one that can.'
  }
  const day = dayLabel(why.at ?? null)
  const when = day ? ` on ${day}` : ''
  return `Not posted — the pull request ${why.how}${when}. No round of it can be posted `
    + 'now, so this draft stays as it is on the record.'
}

/**
 * Whether the draft can be sent back: one that is ready, or one signed off and not posted.
 *
 * **Not `!decided`**, which is what it was. Taking a sign-off back is the one way out of
 * `approved`, and it is the developer's — the session has no route to it, as it has none to
 * approving. A posted review is on GitHub and what follows it is the next round.
 */
function canSendBack(data: DraftDetail): boolean {
  return data.draft.status === 'ready' || data.draft.status === 'approved'
}

function said(result: WriteResult): boolean {
  notice.value = noticeFor(result)
  return result.ok
}

/**
 * Whether the draft on screen is the review in the address.
 *
 * **Every write on this page is gated on it, not only the body.** `resource()` keeps the
 * previous answer while the next read is in flight, so a page that was reused across a
 * review change would draw review 50 under review 51's address for the length of that
 * read — and each of these five reads the review off the draft it is drawing.
 *
 * It began as one line in `save()`, on the argument that writing a body against the wrong
 * review is the one thing this page must never do. That was true of the body and not of
 * the rest: a **sign-off** is the worse write of the two, because the session posts what it
 * finds approved, and `approve`, `sendBack`, `requestNitTicket` and `ticketRestAndApprove`
 * all took the id the same way. Raised by a review of this branch.
 *
 * None of it can fire as the page is keyed — see the watch above — which is why it is four
 * words at each call rather than a mechanism.
 */
function ofThisReview(data: DraftDetail): boolean {
  return data.draft.review_id === reviewId.value
}

/**
 * What a control says when it is pressed between two reviews.
 *
 * **Said rather than ignored.** A guard that returns silently leaves the developer pressing
 * a live button that does nothing, which reads as the page being broken — and the page
 * already has one place for a sentence about a write that did not happen.
 */
function stillReading(): void {
  notice.value = {
    tone: 'problem',
    text: 'This review is still being read. Try that again in a moment.',
  }
}

async function save() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  const result = await desk.saveDraft(data.draft.review_id, loadedBody.value, editing.value)
  working.value = false
  if (said(result)) view.value = 'rendered'
}

async function approve() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  said(await desk.approveDraft(data.draft.review_id, event.value))
  working.value = false
}

async function sendBack() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  const result = await desk.sendBackDraft(data.draft.review_id, note.value)
  working.value = false
  if (said(result)) {
    sendingBack.value = false
    note.value = ''
  }
  /* A refused send-back keeps the note: the developer wrote it, and it is not wrong. */
}

/**
 * Read the review and its draft again, which is what `PostingWatch` asks for on its timer.
 *
 * **Both, because the two facts the waiting state needs are on different documents.** The
 * draft carries the status, the chosen event, the two stamps and — through its own
 * `review` — the GitHub review id; `last_activity_at` is on `ReviewTimings`, which only
 * `GET /reviews/{id}` serves. The contract's shape is one GET per screen, and this screen
 * was already making both of them.
 *
 * A read that fails leaves the last good answer on screen with a notice over it, which is
 * what `resource()` and `DeskState` do everywhere — and the next tick tries again, so a
 * server restarted while a draft waits recovers without the developer pressing anything.
 */
async function reread() {
  await Promise.all([detail.refresh(), review.refresh()])
}

/**
 * Whether the page's last read of this review answered.
 *
 * `PostingWatch` needs it for one sentence — the one that says the session may not be
 * running — because a failed read leaves the last good answer on screen and the clock
 * keeps moving over it. A server that stopped after the sign-off would otherwise have the
 * page accusing the session, under a notice saying Review Desk is not answering.
 */
const recordIsCurrent = computed(
  () => detail.error.value == null && review.error.value == null,
)

/**
 * A notice says what a write did, and stops being true when the record moves on.
 *
 * **Nothing on this page could go stale under a notice until now.** The page read once, so
 * "Signed off. The review session posts it; Review Desk does not." stood until the next
 * write. It re-reads on a timer now, so the panel flips to "Posted" underneath that
 * sentence and the page says two things about one draft — the second of them in the
 * present tense about something that has already happened.
 *
 * **On `posted` and on nothing else, which is what makes it safe.** Every other status
 * this page can arrive at is one the developer's own write produced, and that write is
 * about to set a notice of its own; clearing on *any* change raced it — a poll landing
 * between a write's refresh being issued and answered would clear the notice the write had
 * just set, and the developer who took a sign-off back would be told nothing at all.
 * `posted` is the one transition no act on this page can cause, and the only one that can
 * strand a notice.
 */
watch(
  () => detail.data.value?.draft.status,
  (status) => {
    if (status !== 'posted') return
    notice.value = null
    /*
     * **And the open send-back form goes with it.** Its submit is guarded, so pressing it
     * does nothing — but a textarea asking "What is the session to change?" over a draft
     * that is already on GitHub, with a dead button and only Cancel as a way out, is the
     * page leaving a question open that the record has answered.
     */
    sendingBack.value = false
  },
)

async function fileTicket() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  said(await desk.requestNitTicket(data.draft.review_id))
  working.value = false
}

/**
 * "Ticket what is left, and sign off" — one act, and a sign-off.
 *
 * The contract calls the operation `ticketRestAndApprove` and its summary says "approve",
 * which is the GitHub event. **The button does not**, for the reason the whole screen avoids
 * the word: the developer signs off, and the session posts what they signed off as the event
 * named in the control. A button reading "and approve" beside one reading "Sign off and
 * post" would say two different things about the same act.
 *
 * **It neither posts nor files anything.** Every finding the draft would have posted is held
 * back and recorded as ticketed this round, the one ticket is asked for, the body is
 * recomposed without them, and the draft is approved as `approve` — which is not a choice,
 * because a request for changes that ticketed everything it asked for would contradict
 * itself.
 *
 * The note is asked for rather than required. The API takes none, and the contract says why:
 * it is stored against every finding this moves, as the disposition's reason and each
 * outcome's detail, which is worth a sentence — but a developer who has read the draft and
 * decided none of it is for this round has said enough by pressing the button.
 */
async function ticketRest() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  const result = await desk.ticketRestAndApprove(data.draft.review_id, ticketNote.value)
  working.value = false
  if (said(result)) {
    ticketingRest.value = false
    ticketNote.value = ''
  }
}

/**
 * Whether there is anything left for that button to file.
 *
 * **Counted with `willPost`, not off `finding_dispositions.in_draft`.** The server refuses
 * unless a finding is *posted* — the disposition **and** an open round status — so a review
 * whose carried findings are all `in_draft` and `fixed` has a non-zero `in_draft` count and
 * nothing to file. Drawing the button there is exactly the "a button that is always there
 * is a button that is sometimes a refusal" case the comment beside it says it avoids.
 */
const openFindings = computed(
  () => (review.data.value?.findings ?? []).some(willPost),
)

function statusLine(data: DraftDetail): string {
  const draft = data.draft
  /*
   * **First, because it outranks every other thing this line could say.** A draft on a
   * merged pull request may be `ready`, or signed off and waiting for a session to post
   * it, and neither sentence is true any more: nothing is going to post it. DEV-957 asks
   * the page to say so where the status goes, and the sign-off is taken off the screen in
   * the same breath — see the panel below.
   */
  const why = notForSigning(data)
  if (why) return notForSigningLine(why)
  if (draft.status === 'approved' && data.body_is_stale) {
    /*
     * **The one state the session cannot post**, and the page has to say so: this body was
     * composed in an older shape, so handing it over with the threads would post each
     * anchored finding twice. The way out is below, and it is the only thing offered.
     */
    return 'Signed off, and it cannot be posted: this body was composed before the review '
      + 'took its present shape, so posting it would carry each finding on the code twice. '
      + 'Take the sign-off back and send it back; the session writes it again.'
  }
  if (draft.status === 'approved') {
    /*
     * **The clause came off and went back on, and the round trip is the lesson.** It was
     * cut because `PostingWatch`'s live region said the same words a few inches away — and
     * then the region was made `rd-visually-hidden`, which is right for an announcer and
     * took the only visible statement of the waiting with it. For a few commits a
     * developer reloading a draft signed off four minutes ago was told what it would post
     * as and never that anything was being waited for.
     *
     * Three end-to-end assertions should have caught that and did not: they ask
     * `toBeVisible()` of the region, and Playwright calls a one-pixel clipped element
     * visible. They assert on this line now.
     */
    return `Signed off. Will be posted as "${eventLabel(draft.event)}" — waiting for the `
      + 'session to post it.'
  }
  if (draft.status === 'sent_back') return 'Sent back to the session with a note.'
  if (draft.status === 'posted') return 'Posted. Layer 7 reads the author’s replies.'
  if (draft.status === 'drafting') return 'The session is still writing this.'
  return 'Nothing has been posted. This is what GitHub will show.'
}

/** How the screen words each event. At module scope, as `VERBS` and `LAYER_LABELS` are. */
const EVENTS: Record<ReviewEvent, string> = {
  request_changes: 'Request changes',
  comment: 'Comment',
  approve: 'Approve',
}

function eventLabel(value: ReviewEvent | null | undefined): string {
  return value ? EVENTS[value] : 'not chosen'
}

/**
 * "118 of 150", or what is true when one of the two was not measured.
 *
 * `figure` does the formatting — it is the same rule the Rules screen's tiles use, and a
 * second implementation of "x of y" is a second place for the two to disagree about a null.
 * The contract's rule is that a count the server cannot answer is null and never zero, so a
 * figure built by interpolating two possibly-null numbers would print "118 of null".
 */
function ofFigure(value: number | null | undefined, of: number | null | undefined): string {
  return figure(value, of) ?? 'Not measured'
}
</script>

<template>
  <DeskState
    :resource="review"
    what="this review"
  >
    <template #default="{ data: owner }">
      <ReviewStepPage
        :detail="owner"
        step="draft"
        :headline="heading"
        :sub="detail.data.value ? statusLine(detail.data.value) : null"
      >
        <!--
          **Unsaved edits belong to the review they were typed on**, and the navigation
          that would leave them behind is waiting on this card.

          **Above the draft's own `DeskState` on purpose.** A card drawn inside it would not
          be there when `GET …/draft` refuses or is in flight, and a guard that blocks on a
          control the page is not drawing is a page with no way off it. The review's
          `DeskState` outside is not the same hazard: it keeps the last good answer and draws
          a failure *over* it, so the frame is still here — and a review that never read at
          all has no draft to have edited.

          **One sentence for two guards, so it has to be true of both.** It read "They
          cannot be carried to another review", which is the reason for the *update* guard
          and no reason at all for the *leave* guard — the trail's Code review link involves
          one review, and what is at stake there is that leaving the page discards the
          typing. Both are said now, in that order. Raised by a review of this branch.

          The review is named by its pull request, as every other sentence on these screens
          names one, so that the choice is about a review and not about "this page".
        -->
        <DeskCard
          v-if="leaving"
          class="unsaved"
          @keyup.escape="keepEditing"
        >
          <DeskNotice tone="problem">
            You have unsaved edits to this review’s draft, #{{ owner.review.pr }}. Leaving
            this page discards them, and they are never carried to another review: keep
            editing them here, or discard them.
          </DeskNotice>
          <div class="unsaved-actions">
            <button
              ref="keep"
              type="button"
              class="rd-btn rd-btn--primary"
              @click="keepEditing"
            >
              Keep editing
            </button>
            <button
              type="button"
              class="rd-btn"
              @click="discardEdits"
            >
              Discard the edits
            </button>
          </div>
        </DeskCard>

        <!--
          No draft, said inside the frame. This is the whole reason step 6 can be a link
          unconditionally: the trail is above this sentence, so a reader who came here from
          it has every other step one click away.
        -->
        <DeskNotice
          v-if="detail.error.value?.code === 'not_found'"
          tone="blocked"
        >
          No session has written a draft for this review. Step 6 is where one does; until
          then there is nothing to read and nothing to sign off, and the findings at step 5
          are what the review has so far.
        </DeskNotice>

        <!--
          Any other refusal is a refusal, and `DeskState` is what says so in the server's
          own words. Nested here rather than wrapping the page, because the frame above has
          already drawn and taking it away to report a failed second read would be the dead
          end this change removed.
        -->
        <DeskState
          v-else
          :resource="detail"
          what="this draft"
        >
          <template #default="{ data }">
            <div class="draft">
              <!--
                **Why the sign-off is gone, said before anything else about the draft.**
                DEV-962's own criterion: a signed-off but unposted draft whose head moves is
                no longer signed off, and the page says why. Without it the draft reads as
                one nobody ever approved — the status went back to `drafting` and the
                approval columns were cleared, so there is nothing else on the record a
                reader could tell the difference from.
                `problem`, not `blocked`: the record is right and the session is already
                re-reading, so this is a thing that happened rather than a thing to fix.
              -->
              <DeskNotice
                v-if="withdrawn(data)"
                tone="problem"
              >
                {{ withdrawn(data) }}
              </DeskNotice>

              <p
                v-if="data.draft.sent_back_note"
                class="rd-note sent-back"
              >
                <strong>Your note:</strong> {{ data.draft.sent_back_note }}
              </p>

              <div class="body">
                <!--
                  **The main column is the two halves of what posts.** The body above, and
                  the threads below it — a finding with a line inside the diff is a row in
                  the body's index and a comment on the code, so the preview is only half
                  the review until both are drawn. The aside is untouched: it is where the
                  sign-off lives.
                -->
                <div class="main">
                  <DeskCard
                    as="section"
                    pad="tight"
                    class="preview"
                  >
                    <div class="tabs">
                      <div class="rd-label tabs-label">
                        Preview
                      </div>
                      <div
                        role="tablist"
                        aria-label="How to view the draft"
                        class="tablist"
                      >
                        <button
                          v-for="tab in views"
                          :key="tab.key"
                          type="button"
                          role="tab"
                          class="rd-btn rd-btn--quiet"
                          :aria-selected="view === tab.key"
                          @click="view = tab.key"
                        >
                          {{ tab.label }}
                        </button>
                      </div>
                    </div>

                    <div class="pane">
                      <MarkdownView
                        v-if="view === 'rendered' && data.draft.body.length > 0"
                        :source="data.draft.body"
                      />

                      <!--
                A draft the session has opened and not written into. The body may be empty
                while the status is `drafting`, which the contract says in as many words, so
                the screen says what that is rather than drawing an empty page.
              -->
                      <p
                        v-else-if="view === 'rendered'"
                        class="rd-empty"
                      >
                        This draft has no body yet. Layer 6 is where a session writes the review, and
                        it has written nothing — there is nothing here to sign off.
                      </p>

                      <pre
                        v-else-if="view === 'markdown'"
                        class="source"
                      >{{ data.draft.body }}</pre>

                      <div
                        v-else
                        class="editor"
                      >
                        <label
                          class="rd-visually-hidden"
                          for="draft-body"
                        >The draft body in markdown</label>
                        <textarea
                          id="draft-body"
                          v-model="editing"
                          class="rd-field"
                          rows="22"
                          spellcheck="false"
                        />
                        <div class="editor-actions">
                          <!--
                            **Not gated on the status, and that is deliberate.** A review of
                            DEV-960 proposed disabling this on a draft the server will refuse
                            — `save_draft` refuses an `approved` or a `posted` one — on the
                            argument that a button whose only outcome is a 409 is a refusal
                            pretending to be an act.

                            It is the opposite here, and DEV-862 asked for it by name:
                            attempting the save is how the developer is *told* why, and the
                            refusal keeps what they typed. `test/screens.nuxt.spec.ts`'s
                            "keeps what the developer typed when a save is refused" is that
                            criterion, and it fails against the gate — which is how the
                            proposal was caught. What was actually wrong is that the sentence
                            had nowhere to be drawn on a review nobody can sign off; that is
                            fixed in the aside, not here.
                          -->
                          <button
                            type="button"
                            class="rd-btn rd-btn--primary"
                            :disabled="!dirty || working"
                            @click="save"
                          >
                            {{ working ? 'Saving…' : 'Save the draft' }}
                          </button>
                          <button
                            type="button"
                            class="rd-btn"
                            :disabled="!dirty || working"
                            @click="editing = loadedBody"
                          >
                            Discard changes
                          </button>
                          <p class="rd-meta editor-help">
                            An edit is recorded against the finding it changes. Three of the same kind
                            become a proposed rule. <strong>Saving replaces the blocks with
                              these characters</strong>, so the index stops being composed, the
                            threads below go with it, and a decision about a finding is refused
                            afterwards — cut, regrade and rewrite are on the code review page.
                          </p>
                        </div>
                      </div>
                    </div>
                  </DeskCard>

                  <DraftThreads
                    :threads="data.threads"
                    :posted="data.draft.status === 'posted'"
                  />
                </div>

                <aside class="aside">
                  <DeskCard class="panel">
                    <div class="rd-label">
                      Checks
                    </div>

                    <!--
                `lint` is null until the review-craft lint has run over this body, and a
                body that was just saved has no result yet. Null is not a failing lint, so
                the screen says which it is rather than drawing "Fails".
              -->
                    <template v-if="data.lint">
                      <FactRow
                        label="Visible words"
                        :value="ofFigure(data.lint.visible_words, data.lint.word_budget)"
                        numeric
                        :failing="
                          data.lint.visible_words != null
                            && data.lint.word_budget != null
                            && data.lint.visible_words > data.lint.word_budget
                        "
                      />
                      <FactRow
                        label="Findings in the body"
                        :value="ofFigure(data.lint.findings_in_body, data.lint.findings_expected)"
                        numeric
                        :failing="
                          data.lint.findings_in_body != null
                            && data.lint.findings_expected != null
                            && data.lint.findings_in_body !== data.lint.findings_expected
                        "
                      />
                      <FactRow
                        label="Review lint"
                        :value="data.lint.passes ? 'Passes' : 'Fails'"
                        :failing="!data.lint.passes"
                      />
                      <FactRow
                        label="Raised before on this PR"
                        :value="
                          data.lint.raised_before_on_pr == null
                            ? 'Not measured'
                            : data.lint.raised_before_on_pr === 0
                              ? 'None'
                              : String(data.lint.raised_before_on_pr)
                        "
                        :failing="(data.lint.raised_before_on_pr ?? 0) > 0"
                      />
                      <p
                        v-for="line in data.lint.errors ?? []"
                        :key="line"
                        class="rd-note lint-error"
                      >
                        {{ line }}
                      </p>
                      <p class="rd-meta checked">
                        Checked {{ sinceLabel(data.lint.checked_at) }}
                      </p>
                    </template>
                    <p
                      v-else
                      class="rd-empty"
                    >
                      The review lint has not run over this body. That is not the same as it
                      failing: nothing has measured it yet.
                    </p>
                  </DeskCard>

                  <DeskCard class="panel">
                    <div class="rd-label">
                      Your edits to this draft
                    </div>
                    <p
                      v-if="data.edits.length === 0"
                      class="rd-meta help"
                    >
                      You have not changed this draft.
                    </p>
                    <!--
                      **One line a finding, not one a toggle.** Every edit was its own line
                      and the verb was printed twice — "Kept F1 · kept", four times over on
                      the review DEV-928 was raised from. The record keeps every edit and
                      still does; `editLines` is what changed, and it reports where each
                      finding ended up and how many times it moved to get there.
                    -->
                    <p
                      v-for="line in editRows"
                      :key="line.key"
                      class="edit"
                    >
                      <span class="edit-verb">{{ line.verb }}</span>
                      {{ line.key }}<!--
                        **The separators are in the strings, not in the stylesheet.** A
                        newline between two tags in a Vue template is condensed away, so a
                        margin would move the boxes apart and leave the text nodes touching —
                        which is what a screen reader reads and what a copy gives. The
                        Solution page learned this on a finding's round status.
                      --><span v-if="line.detail"> · {{ line.detail }}</span><span
                        v-if="line.changed"
                        class="rd-meta edit-times"
                      > · {{ line.changed }}</span>
                    </p>
                    <p class="rd-meta foot">
                      Saved as feedback when you sign off. Three edits of the same kind become a
                      proposed rule.
                    </p>
                  </DeskCard>

                  <!--
                    **Gated like the sign-off panel, because it is the fourth of the same
                    four acts.** "File as one ticket for an agent" is `POST …/nit-ticket`,
                    which `request_nit_ticket` refuses on a review that has left the desk
                    for the reason the other three are refused: it asks an agent to file a
                    ticket, and no agent is coming. The contract names it in
                    `NotForSigning`'s own description. It lives in its own card, which is
                    how it was missed when the panel below was gated — found by a review of
                    the commit that gated the panel.

                    **The rows stay; only the button goes.** What the review held back is
                    part of the record, and a reader of a finished review still wants to see
                    it. What they cannot do is ask for a ticket.
                  -->
                  <DeskCard
                    v-if="data.held_back.length"
                    class="panel"
                  >
                    <!--
                      **The rows, and the join that makes them possible.** `HeldBackFinding`
                      carries a key, a claim and a severity; the title, the file and the line
                      a row needs come from the review's own findings, which this page is
                      already reading for the frame and the trail. Nothing about the record
                      or the contract changed to allow it.
                    -->
                    <HeldBackRows :rows="heldRows" />
                    <p
                      v-if="offTheDesk(data)"
                      class="rd-meta foot"
                    >
                      These stay on the record. There is no ticket to ask for: nothing will
                      act on this review.
                    </p>
                    <button
                      v-else
                      type="button"
                      class="rd-btn"
                      :disabled="data.nit_ticket.status !== 'none' || working"
                      @click="fileTicket"
                    >
                      {{
                        data.nit_ticket.status === 'none'
                          ? 'File as one ticket for an agent'
                          : data.nit_ticket.status === 'requested'
                            ? 'Queued as one ticket'
                            : `Filed as ${data.nit_ticket.ticket_id}`
                      }}
                    </button>
                  </DeskCard>

                  <!--
                    **No sign-off at all when nothing can post this draft**, which is
                    DEV-957's "an unposted draft on a finished review says so and cannot be
                    signed off". Not a disabled button: the panel is the three acts that
                    each ask a session to do something afterwards — post it, file a ticket
                    and post it, send it back to be rewritten — and on a merged pull request
                    there is no session coming for any of them. A greyed-out "Sign off and
                    post" would still be offering the act, and the API refuses it; the
                    status line above says what happened instead.

                    The card in its place is not a refusal notice. Nothing was refused and
                    nothing is wrong: the record simply says what became of the pull request,
                    and the draft stays on it to be read.
                  -->
                  <DeskCard
                    v-if="notForSigning(data)"
                    class="panel"
                  >
                    <p class="rd-label">
                      Nothing to sign off
                    </p>
                    <p class="rd-note">
                      {{ notForSigningLine(notForSigning(data)!) }}
                    </p>
                    <p class="rd-meta foot">
                      The draft, the findings and your decisions are all still on the
                      record. Nothing has been removed.
                    </p>
                  </DeskCard>

                  <DeskCard
                    v-else
                    class="panel"
                  >
                    <label
                      class="rd-label"
                      for="event"
                    >Post as</label>
                    <select
                      id="event"
                      v-model="event"
                      class="rd-field"
                      :disabled="decided(data)"
                    >
                      <option value="request_changes">
                        Request changes
                      </option>
                      <option value="comment">
                        Comment
                      </option>
                      <option value="approve">
                        Approve
                      </option>
                    </select>
                    <button
                      type="button"
                      class="rd-btn rd-btn--primary"
                      :disabled="decided(data) || dirty || working"
                      @click="approve"
                    >
                      {{ working ? 'Working…' : 'Sign off and post' }}
                    </button>
                    <!--
                **"Ticket what is left, and sign off" — one act, and a sign-off.**

                Drawn only while there is something left to file. The API refuses a review
                with no open finding, saying to sign off instead and ask for a ticket for
                the nits on their own — so a button that is always there is a button that
                is sometimes a refusal, and the two things it would do are already the two
                buttons above it.
              -->
                    <template v-if="openFindings && !ticketingRest">
                      <button
                        type="button"
                        class="rd-btn"
                        :disabled="decided(data) || dirty || working"
                        @click="ticketingRest = true"
                      >
                        Ticket what is left, and sign off
                      </button>
                    </template>
                    <div
                      v-else-if="ticketingRest"
                      class="send-back"
                    >
                      <label
                        class="rd-meta"
                        for="ticket-rest-note"
                      >Why is the rest going to a ticket? Optional.</label>
                      <textarea
                        id="ticket-rest-note"
                        v-model="ticketNote"
                        class="rd-field"
                        rows="2"
                      />
                      <p class="rd-meta">
                        Every open finding is held back and recorded as ticketed this round,
                        and the draft is signed off. The event is not yours to choose here —
                        the control above is set to Approve, because a request for changes
                        that ticketed everything it asked for would contradict itself.
                        Review Desk files no ticket; an agent does, and the sign-off cannot
                        link to one that does not exist yet.
                      </p>
                      <div class="send-back-actions">
                        <button
                          type="button"
                          class="rd-btn rd-btn--primary"
                          :disabled="working"
                          @click="ticketRest"
                        >
                          Ticket it and sign off
                        </button>
                        <button
                          type="button"
                          class="rd-btn"
                          @click="ticketingRest = false"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>

                    <!--
                      **The one transition out of a sign-off.** A draft that is approved and
                      not yet posted can be sent back, which takes the sign-off with it —
                      and without that a draft the hand-over refuses has nowhere to go at
                      all: it cannot be posted, cannot be rewritten, and could not be sent
                      back. Measured by the session building the posting half of DEV-929.
                      The label says what the button does, because taking back a sign-off is
                      not the same act as sending back a draft nobody has signed.
                    -->
                    <template v-if="!sendingBack">
                      <button
                        type="button"
                        class="rd-btn"
                        :disabled="!canSendBack(data) || working"
                        @click="sendingBack = true"
                      >
                        {{
                          data.draft.status === 'approved'
                            ? 'Take back the sign-off and send it back'
                            : 'Send back with a note'
                        }}
                      </button>
                    </template>
                    <div
                      v-else
                      class="send-back"
                    >
                      <label
                        class="rd-meta"
                        for="send-back-note"
                      >What is the session to change?</label>
                      <textarea
                        id="send-back-note"
                        v-model="note"
                        class="rd-field"
                        rows="3"
                      />
                      <div class="send-back-actions">
                        <!--
                          **Guarded like the button that opened it**, which it did not need
                          to be until this page started re-reading. The form opens on a
                          draft that is `approved`, and the status can now move underneath
                          it: the session posts while the note is being typed, the panel
                          flips to "Posted", and an enabled Send back over a posted draft
                          buys the developer a refusal from the server.
                        -->
                        <button
                          type="button"
                          class="rd-btn rd-btn--primary"
                          :disabled="working || !canSendBack(data)"
                          @click="sendBack"
                        >
                          Send back
                        </button>
                        <button
                          type="button"
                          class="rd-btn"
                          @click="sendingBack = false"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                    <DeskNotice
                      v-if="dirty"
                      tone="problem"
                    >
                      Save or discard your changes before signing off.
                    </DeskNotice>
                    <!--
                      **What happens after the press, kept current.** Everything above this
                      is the developer's to do; this is the only part of the panel that
                      reports on somebody else, so it is the only part that has to re-read
                      the record — DEV-932, where a sign-off went 4 min 28 s with nothing
                      on screen. It asks this page to read again; it writes nothing.
                    -->
                    <PostingWatch
                      :draft="data.draft"
                      :review="data.review"
                      :last-activity-at="review.data.value?.timings.last_activity_at"
                      :body-is-stale="data.body_is_stale"
                      :record-is-current="recordIsCurrent"
                      :reread="reread"
                    />
                    <p class="rd-meta foot">
                      The review session posts it under your name, with no bot banner.
                    </p>
                  </DeskCard>

                  <!--
                    **What the last write did, in the aside itself and not in a panel.**

                    It was the last line of the sign-off card — and that card is the `v-else`
                    of the one that says "Nothing to sign off", so on a review the record
                    will not let anybody sign off, there was no element on this page that
                    could draw a notice at all. The editor is not gated on status, and the
                    server refuses a save on an `approved` or a `posted` draft, so a
                    developer on a merged pull request could press Save the draft, be refused
                    with a 409, and watch nothing happen: `said()` set a sentence that had
                    nowhere to go. Raised by a review of this branch, which also caught that
                    `stillReading()` had been written into the same blind spot while its own
                    comment claimed the page always had somewhere to say this.

                    It belongs out here because it is the page's notice, not the sign-off
                    panel's: four of the five writes it reports on are in that panel, and the
                    fifth — the save — is in the editor in the other column.
                  -->
                  <DeskCard
                    v-if="notice"
                    class="panel"
                  >
                    <DeskNotice :tone="notice.tone">
                      {{ notice.text }}
                    </DeskNotice>
                  </DeskCard>
                </aside>
              </div>
            </div>
          </template>
        </DeskState>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.draft {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.back {
  font-size: 14px;
}

.title {
  margin: 6px 0 0;
}

.status {
  margin: 4px 0 0;
}

.status--decided {
  color: var(--rd-accent-dark);
  font-weight: 500;
}

.unsaved {
  max-width: var(--rd-measure);
}

.unsaved-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 12px 0 0;
}

.sent-back {
  margin: 8px 0 0;
  max-width: var(--rd-measure);
}

.sent-back strong {
  font-weight: 600;
}

.body {
  display: flex;
  flex-wrap: wrap;
  gap: 24px;
  align-items: flex-start;
}

/*
 * The main column, which is what the preview alone used to be. The flex numbers moved out
 * here unchanged: the column takes the room, and the aside keeps its own 320px basis.
 */
.main {
  flex: 999 1 560px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.preview {
  min-width: 0;
  padding: 0;
}

.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 8px 12px 8px 20px;
  border-bottom: 1px solid var(--rd-line);
}

.tabs-label {
  margin-right: auto;
}

.tablist {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.pane {
  padding: 24px 20px;
}

.source {
  margin: 0;
  padding: 16px;
  overflow-x: auto;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  font-family: var(--rd-font-mono);
  font-size: 13px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

.editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.editor-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.editor-help {
  flex: 1 1 240px;
  margin: 0;
}

.aside {
  flex: 1 1 320px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.edit {
  margin: 0;
  font-size: 14px;
}

.edit-verb {
  font-weight: 500;
}

/*
 * How many times a finding moved, beside where it ended up. Quiet and never wrapping inside
 * itself — it is a phrase, not a sentence — and in the body ink, because the number of times
 * somebody changed their mind is not a fault.
 */
.edit-times {
  white-space: nowrap;
}

.help,
.foot,
.checked {
  margin: 0;
}

.foot,
.checked {
  padding-top: 8px;
  border-top: 1px solid var(--rd-rail);
}

.send-back {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.send-back-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.missing {
  padding: 20px;
}
</style>
