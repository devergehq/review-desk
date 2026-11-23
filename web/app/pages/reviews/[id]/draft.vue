<script setup lang="ts">
import type { DraftDetail, NotForSigning, ReviewDetail, ReviewEvent } from '~/types/desk'
import type { WriteResult } from '~/composables/useReviewDesk'
import type { DeskNotice } from '~/utils/format'
import type { ActOnTab } from '~/utils/reviewTabs'

import { baseOnlyLine, commitLabel, nothingMoved, updatedLine, withdrawnLine } from '~/utils/rounds'

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

type View = 'rendered' | 'markdown' | 'changes' | 'edit'
const view = ref<View>('rendered')

/**
 * **S05's three views, and the editor this page already had.** *Changes since…* is S05's
 * and S16's third view, named for the round: a later round's changes are the commits since
 * the round before, a first round's are what its head did after it was read. *Edit* is not
 * on either picture and stays, because a page may never remove what it draws.
 */
const views = computed<Array<{ key: View, label: string }>>(() => {
  const round = review.data.value?.review.round ?? 1
  return [
    { key: 'rendered', label: 'Rendered' },
    { key: 'markdown', label: 'Markdown' },
    { key: 'changes', label: round > 1 ? `Changes since round ${round - 1}` : 'Changes since you read it' },
    { key: 'edit', label: 'Edit' },
  ]
})

/** The three verdicts, in S05's order: the segments under *Post as*. */
const EVENT_SEGMENTS: ReadonlyArray<{ value: ReviewEvent, label: string }> = [
  { value: 'request_changes', label: 'Request changes' },
  { value: 'comment', label: 'Comment' },
  { value: 'approve', label: 'Approve' },
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

/*
 * **A draft re-drawn under the developer says so** — DEV-1346, found by its self-review.
 *
 * The page re-reads every open screen after any write, including one made from another card
 * on this page — asking the session something, say. If the session rewrote the draft in the
 * meantime, the new text and its `shown` replace the old, and a sign-off then binds to the
 * new text. That is correct, and the developer has to know it happened: so the `shown` the
 * page last drew is kept, and a new one arriving *outside this page's own writes* puts up the
 * same sentence a refused sign-off does — except after this page's own writes that move the
 * draft, whose re-read is the developer's doing and says what it did in its own notice.
 */
const lastShown = ref<string | null>(null)
const CHANGED_UNDERNEATH = 'The draft changed while you were reading it · review the change.'
/*
 * Whether the re-read inside the write now finishing brought a rewritten draft. A write's own
 * notice is put up after its re-read, so without this the change would be announced and at
 * once replaced by "Queued as one ticket" — `said` keeps it, ahead of what the write did.
 */
let changedUnderneath = false
/*
 * **Set by the writes that move the draft themselves**, and by nothing else: saving, and the
 * three sign-offs, each of which says what it did in its own notice. Not `working`, which
 * asking for the nit ticket sets too — its re-read would otherwise carry a session's
 * rewrite in with the notice muted. Found by the second round of DEV-1346's self-review.
 */
const movingTheDraft = ref(false)

/** Run one of this page's own writes that moves the draft. */
async function moving<T>(write: () => Promise<T>): Promise<T> {
  movingTheDraft.value = true
  try {
    return await write()
  }
  finally {
    movingTheDraft.value = false
  }
}
watch(
  () => detail.data.value,
  (data) => {
    if (!data) return
    const drawn = [
      data.draft.review_id,
      data.shown.draft_rev,
      data.shown.body_sha256,
      data.shown.threads_sha256,
    ].join(':')
    const before = lastShown.value
    lastShown.value = drawn
    if (before === null || before === drawn || movingTheDraft.value) return
    /* Another review in the same page instance is a navigation, not a change under anyone. */
    if (before.split(':')[0] !== String(data.draft.review_id)) return
    /*
     * Remembered only while one of this page's own writes is in flight, because only then is
     * a `said()` coming to replace this notice. A re-draw from another card or from
     * `PostingWatch`'s timer has no `said()` after it, and a flag left set would put this
     * sentence in front of the developer's next, unrelated success. Round 3 of DEV-1346's
     * self-review.
     */
    changedUnderneath = working.value
    notice.value = { tone: 'problem', text: CHANGED_UNDERNEATH }
  },
)
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
 * **A notice is brought into view when it is set.** The sign-off is pressed from the pinned
 * header anywhere in the scroll, and the answer — a refusal above all — is in the side
 * column; `nearest`, so a notice already on screen does not move the page. jsdom has no
 * scrolling, hence the check.
 */
const noticeBox = useTemplateRef<HTMLElement>('noticeBox')
watch(notice, async (said) => {
  if (!said) return
  await nextTick()
  if (typeof noticeBox.value?.scrollIntoView === 'function') {
    noticeBox.value.scrollIntoView({ block: 'nearest' })
  }
})

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

/**
 * The body as the rendered preview draws it, with its open index folded.
 *
 * **The one thing this page printed twice was the open index**, and every row of it is a
 * finding drawn in full further down the same page — as its own section when it posts in
 * the body, as a thread when it posts on a line. `foldOpenIndex` folds the heading and
 * that table into one line and touches nothing else.
 *
 * **Folded, not taken out.** The whole index is still in this preview, one click from
 * open, because the page a developer signs off on has to be what posts — Patrick's ruling
 * of 16 October 2026, argued on the function. The Markdown tab shows `draft.body`
 * verbatim, the editor edits the stored characters, and the session posts the body as
 * stored, so nothing here changes what GitHub receives either.
 */
const preview = computed(() => foldOpenIndex(detail.data.value?.draft.body ?? ''))

/**
 * The body in pieces, each with an anchor the outline reaches.
 *
 * **Cut from the folded body, not from the stored one**, so that the index's fold is inside
 * the first block rather than a block of its own — it is one line of the opening, which is
 * what folding it made it. `bodyBlocks` carries the argument for cutting at all, and a test
 * asserts that the pieces joined are the body.
 */
const blocks = computed(() => bodyBlocks(preview.value.body))

/**
 * The outline: what this draft posts, in the order it posts it.
 *
 * **This is the index, as navigation.** Patrick's words on where the index goes in the
 * three-pane draft: "it is navigation there rather than a duplicate". It carries what the
 * index never did — the body's own prose blocks, because `### Problem fit` is a place a
 * reader wants to get back to — and it is built from the body and the threads rather than
 * from the index's table, so a draft written before the house style grew one still has it.
 */
const outline = computed(() =>
  outlineRows(blocks.value, detail.data.value?.threads ?? []),
)

/** Which outline row is showing as the one reached, or null until one is pressed. */
const reached = ref<string | null>(null)

/**
 * Go to a part of the post.
 *
 * **It switches to the rendered tab first.** The anchors are the rendered tab's; a row
 * pressed while the Markdown tab or the editor is showing would otherwise scroll to
 * nothing, which reads as a broken control rather than as a tab being in the way. Nothing
 * is discarded by it — the editor's text is held in `editing`, not in the DOM.
 *
 * jsdom implements no scrolling, hence the check rather than an optional call; the test for
 * this asserts the anchors exist and that the tab changed, which is the part a layout
 * cannot swallow.
 */
async function goTo(anchor: string) {
  reached.value = anchor
  view.value = 'rendered'
  await nextTick()
  const found = document.getElementById(anchor)
  /*
   * A thread's comment is folded under its row: reaching it opens it. **Only a thread's own
   * fold** — a body block's markdown carries folds of its own (the open index, a proof), and
   * those stay as the body draws them.
   */
  const fold = anchor.startsWith('thread-')
    ? found?.querySelector<HTMLDetailsElement>(':scope > details')
    : null
  if (fold) fold.open = true
  if (typeof found?.scrollIntoView === 'function') {
    found.scrollIntoView({ block: 'start', behavior: 'smooth' })
  }
}

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
  const done = noticeFor(result)
  notice.value = changedUnderneath
    ? { tone: 'problem', text: `${CHANGED_UNDERNEATH} ${done?.text ?? ''}`.trim() }
    : done
  changedUnderneath = false
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
  const result = await moving(() =>
    desk.saveDraft(data.draft.review_id, loadedBody.value, editing.value))
  working.value = false
  if (said(result)) view.value = 'rendered'
}

/*
 * **Every sign-off on this page sends `data.shown`: the hashes of the document on screen** —
 * DEV-1346. A session that rewrites the draft while it is open is refused at the click, and
 * the refusal reads the page again and says "the draft changed while you were reading it ·
 * review the change". A re-read for any other reason that brings a rewritten draft says the
 * same — see `lastShown` above — so the text never changes under the developer unannounced.
 */
async function approve() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  said(await moving(() => desk.approveDraft(data.draft.review_id, event.value, data.shown)))
  working.value = false
}

async function sendBack() {
  const data = detail.data.value
  if (!data) return
  if (!ofThisReview(data)) return stillReading()
  working.value = true
  const result = await moving(() =>
    desk.sendBackDraft(data.draft.review_id, note.value, data.shown))
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
  const result = await moving(() =>
    desk.ticketRestAndApprove(data.draft.review_id, ticketNote.value, data.shown))
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

/* ------------------------- The sign-off, as the header's action ------------------------- */

/**
 * **The sign-off is the header's primary action** (DEV-1329, contract 1 of DEV-1322's plan).
 *
 * The binding is `approve` above, unchanged, and every gate the page's own button had moves
 * with it: not offered at all when nothing can post this draft, and disabled while the draft
 * is decided, while there are unsaved edits, or while a write is in flight. The header
 * draws the label `primaryAction` gives, so the page has no second sign-off button.
 *
 * **Offered only for a draft that is ready and can post.** While edits are unsaved, the
 * disabled button is described by the sentence saying so, which sits beside *Post as*. Once it is signed off, sent back
 * or posted, the review no longer waits on the developer for it and the header draws its
 * state's own action instead; a draft nothing can post draws none. While the draft has not
 * read yet, the review's own `draft_status` decides, and the button waits disabled.
 */
const signOffAct = computed<ActOnTab>(() => {
  const data = detail.data.value
  /*
   * A draft read that failed outright offers nothing: the page says the read failed, and a
   * button disabled until a read that is not coming would be a control that never works.
   */
  const offered = data
    ? data.draft.status === 'ready' && !notForSigning(data)
    : review.data.value?.draft_status === 'ready' && detail.error.value == null
  return {
    press: approve,
    disabled: !data || decided(data) || dirty.value || working.value,
    working: working.value,
    offered,
    describedBy: dirty.value ? 'sign-off-why' : null,
  }
})

/**
 * The sign-off's words: its note, from the one function that words the action. A later
 * round names its round and the commit it read; `primaryAction` holds both.
 */
const signOffNote = computed(() => {
  const owner = review.data.value?.review
  if (!owner) return null
  return primaryAction(owner.id, 'draft_ready', [], { round: owner.round, head: owner.head_sha })?.note ?? null
})

/* ------------------------------- A later round ------------------------------- */

/** S16's *Since round 1*: what became of the carried findings. Null on a first round. */
const since = computed(() => {
  const owner = review.data.value
  if (!owner || owner.review.round < 2) return null
  return sinceRound(owner.findings, owner.outcomes)
})

const sinceLines = computed(() => (since.value ? sinceRoundLines(since.value) : []))

/** The round each carried finding came from, by key, for the thread rows. */
const carriedFrom = computed<Record<string, number>>(() => {
  const held: Record<string, number> = {}
  for (const finding of review.data.value?.findings ?? []) {
    if (finding.continues_round != null) held[finding.key] = finding.continues_round
  }
  return held
})

/**
 * *Raised before on this PR*, named. S16 names the carried findings rather than counting
 * them; the lint's own count stays the measure, and the names are drawn only when they are
 * exactly the findings it counted — the carried ones this draft posts. Otherwise the count,
 * as before.
 */
function raisedBefore(data: DraftDetail): string {
  const counted = data.lint?.raised_before_on_pr
  if (counted == null) return 'Not measured'
  if (counted === 0) return 'None'
  const carried = (review.data.value?.findings ?? []).filter(one => isCarried(one) && willPost(one))
  return carried.length === counted ? carried.map(one => one.key).join(', ') : String(counted)
}

/**
 * S05's *Lines moved since read*, said as what the record measures: **which threads had
 * their line counted on a commit other than the head**. A thread whose commit was not
 * recorded makes the figure unmeasured rather than "none".
 *
 * **Failing only for a thread raised in this round.** A carried finding's line is counted on
 * the earlier round's head by construction — a carry copies it and re-measures nothing — so
 * naming it is a fact and not a fault; its row says *carried from round N*.
 */
function readElsewhere(data: DraftDetail): { value: string, failing: boolean } {
  if (data.threads.length === 0) return { value: 'None', failing: false }
  if (data.threads.some(one => one.line_sha == null || one.line_sha.length === 0)) {
    return { value: 'Not measured', failing: false }
  }
  const head = data.review.head_sha
  const elsewhere = data.threads.filter(one => !sameCommit(one.line_sha!, head))
  if (elsewhere.length === 0) return { value: 'None', failing: false }
  return {
    value: elsewhere.map(one => one.finding_key).join(', '),
    failing: elsewhere.some(one => carriedFrom.value[one.finding_key] == null),
  }
}

/**
 * The *Changes since…* view's sentences: a later round's commits since the round before,
 * or a first round's head since it was read. Each sentence is the one `rounds.ts` already
 * words for the review page, so this view and that notice cannot say two things.
 */
function changesSaid(owner: ReviewDetail): { lead: string, commits: ReviewDetail['commits_since_previous_round'] } {
  const updates = owner.head_updates ?? nothingMoved()
  const moved = updatedLine(updates) ?? baseOnlyLine(updates)
  if (owner.review.round > 1) {
    const commits = owner.commits_since_previous_round ?? []
    const lead = commits.length > 0
      ? `${count(commits.length, 'commit')} between round ${owner.review.round - 1}’s head and this round’s.`
      : `No commits between round ${owner.review.round - 1}’s head and this round’s are recorded.`
    return { lead: moved ? `${lead} ${moved}.` : lead, commits }
  }
  const head = owner.review.head_sha ? commitLabel(owner.review.head_sha) : null
  const lead = moved
    ? `${moved}.`
    : head ? `The head has not moved since this round read it at ${head}.` : 'The head has not moved since this round read it.'
  return { lead, commits: [] }
}

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
      <!--
        **The sign-off is the header's action** (DEV-1329): `signOffAct` hands the header the
        binding and its gates, and the header draws the one button. Nothing below draws a
        second.
      -->
      <ReviewStepPage
        :detail="owner"
        step="draft"
        tab="draft"
        :headline="heading"
        :sub="detail.data.value ? statusLine(detail.data.value) : null"
        :act="signOffAct"
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

              <!--
                **S05: what GitHub will show, beside what you decide about it.**

                The preview and the threads under it are the main column; the side column is
                the verdict, the checks, what came of the round before, what is held back,
                your edits, the outline, and the acts that are not the sign-off. Side by side
                from 760px of the tab's own width, a container query as Findings' bench is,
                because beside the conversation the main column is about 920px at 1,600.
              -->
              <div class="bench">
                <div class="main">
                  <div class="view-bar">
                    <p class="rd-label view-label">
                      {{ data.draft.status === 'posted' ? 'What GitHub shows' : 'What GitHub will show' }}
                    </p>
                    <div
                      role="tablist"
                      aria-label="How to view the draft"
                      class="segments"
                    >
                      <button
                        v-for="tab in views"
                        :key="tab.key"
                        type="button"
                        role="tab"
                        class="segment"
                        :aria-selected="view === tab.key"
                        @click="view = tab.key"
                      >
                        {{ tab.label }}
                      </button>
                    </div>
                  </div>

                  <DeskCard
                    as="section"
                    pad="tight"
                    class="preview"
                  >
                    <div class="pane">
                      <!--
                        **One document, and each part of it says where it lands on GitHub.**
                        The body, labelled as the body, with the open index folded to one
                        line because it was the duplicate — folded and not taken out, see
                        `foldOpenIndex`. The comments on the code are under this card.
                      -->
                      <div
                        v-if="view === 'rendered' && data.draft.body.length > 0"
                        class="rendered"
                      >
                        <p class="rd-label lands">
                          {{
                            data.draft.status === 'posted'
                              ? 'Posted as the review body'
                              : 'Posts as the review body'
                          }}
                        </p>

                        <!--
                          **One block a heading, each with the anchor its outline row
                          names.** The pieces joined are the body, character for character.
                          `bodyBlocks` argues it; `test/draft.spec.ts` asserts it.
                        -->
                        <div class="blocks">
                          <section
                            v-for="block in blocks"
                            :id="block.anchor"
                            :key="block.anchor"
                            class="block"
                          >
                            <MarkdownView :source="block.markdown" />
                          </section>
                        </div>
                      </div>

                      <!--
                        A draft the session has opened and not written into. The body may be
                        empty while the status is `drafting`, which the contract says in as
                        many words, so the screen says what that is.
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

                      <!--
                        **What changed under the draft, from the record's own sentences.** A
                        later round lists the commits since the round before
                        (`commits_since_previous_round`); a first round says what its head
                        did after it was read. No diff: Review Desk reads no git.
                      -->
                      <div
                        v-else-if="view === 'changes'"
                        class="changes"
                      >
                        <p class="rd-note changes-lead">
                          {{ changesSaid(owner).lead }}
                        </p>
                        <ol
                          v-if="changesSaid(owner).commits?.length"
                          class="commits"
                        >
                          <li
                            v-for="commit in changesSaid(owner).commits"
                            :key="commit.sha"
                            class="commit"
                          >
                            <code class="commit-sha">{{ commitLabel(commit.sha) }}</code>
                            <span class="commit-summary">{{ commit.summary }}</span>
                            <span
                              v-if="commit.files.length"
                              class="rd-meta commit-files"
                            >{{ count(commit.files.length, 'file') }}</span>
                          </li>
                        </ol>
                      </div>

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

                  <!--
                    **The comments on the code, as S05's table under the preview.** Each row
                    is a thread and its comment is folded under it: what posts on the line is
                    one press away, never gone. The head goes with them (DEV-978), and a later
                    round's carried findings say which round they came from.
                  -->
                  <DraftThreads
                    :threads="data.threads"
                    :head-sha="data.review.head_sha"
                    :posted="data.draft.status === 'posted'"
                    :carried-from="carriedFrom"
                  />
                </div>

                <aside class="aside">
                  <!--
                    **No verdict and no acts when nothing can post this draft**, DEV-957. Not
                    disabled: on a merged pull request there is no session coming for any of
                    them, and the header draws no sign-off either. The status line above says
                    what happened.
                  -->
                  <section
                    v-if="notForSigning(data)"
                    class="side-part"
                  >
                    <p class="rd-rubric">
                      Nothing to sign off
                    </p>
                    <p class="rd-note">
                      {{ notForSigningLine(notForSigning(data)!) }}
                    </p>
                    <p class="rd-meta foot">
                      The draft, the findings and your decisions are all still on the
                      record. Nothing has been removed.
                    </p>
                  </section>

                  <!--
                    **Post as, as S05's three segments.** Radios, so the choice is one of
                    three to a screen reader and the arrow keys move it; `event` is the same
                    value the sign-off has always sent.
                  -->
                  <fieldset
                    v-else
                    class="side-part verdict"
                    :disabled="decided(data)"
                  >
                    <legend class="rd-rubric">
                      Post as
                    </legend>
                    <div class="verdicts">
                      <label
                        v-for="one in EVENT_SEGMENTS"
                        :key="one.value"
                        class="verdict-segment"
                        :class="{ 'verdict-segment--on': event === one.value }"
                      >
                        <input
                          v-model="event"
                          type="radio"
                          name="event"
                          class="rd-visually-hidden"
                          :value="one.value"
                        >
                        {{ one.label }}
                      </label>
                    </div>
                  </fieldset>

                  <section class="side-part">
                    <p class="rd-rubric">
                      Checks
                    </p>
                    <FactRow
                      label="Lines read on another commit"
                      :value="readElsewhere(data).value"
                      numeric
                      :failing="readElsewhere(data).failing"
                    />
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
                        :value="raisedBefore(data)"
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
                  </section>

                  <!--
                    **S16's Since round 1**, on a later round only, from the carried findings'
                    own statuses (`sinceRound`). A sentence per kind of outcome, and a round
                    that carried nothing draws nothing.
                  -->
                  <section
                    v-if="since && sinceLines.length"
                    class="side-part"
                    data-testid="since-round"
                  >
                    <p class="rd-rubric">
                      Since round {{ since.from }}
                    </p>
                    <p class="rd-note since">
                      {{ sinceLines.join(' ') }}
                    </p>
                  </section>

                  <!--
                    **Held back, with the ticket ask gated like the sign-off**: `POST
                    …/nit-ticket` is refused on a review that has left the desk, so the rows
                    stay and only the button goes.
                  -->
                  <!--
                    A card, unlike the other parts of this column: `HeldBackRows` draws each
                    row's rule to its card's edges with a negative margin, and without the
                    card's padding the rows would reach past the column.
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

                  <section class="side-part">
                    <p class="rd-rubric">
                      Your edits to this draft
                    </p>
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
                  </section>

                  <!--
                    **The outline: what this draft posts, in the order it posts it** — the
                    index, as navigation. Not on S05, and kept, because a page may never
                    remove what it draws; it is the side column's last part so it sits under
                    what S05 puts first.
                  -->
                  <!--
                    **The outline and the foot stay in view together** (DEV-1329): the
                    outline is the place you are in the post, which the three-pane draft kept
                    on screen, and the foot is the note and the other two ways out. One group,
                    so neither slides over the other; it rides under the pinned header.
                  -->
                  <div
                    class="side-pinned"
                    :class="{ 'side-pinned--held': sendingBack || ticketingRest }"
                  >
                    <!--
                      **What the last write did, and why the sign-off is disabled, at the head
                      of the group that stays in view.** The sign-off is pressed from the pinned
                      header anywhere in the scroll, so its answer has to be where the eye can
                      see it from anywhere too; the notice is also scrolled to when it is set.
                      It reports on the sign-off, the save, the send-back and both ticket asks,
                      and draws on a review nobody can sign off, where a refused save would
                      otherwise have nowhere to be said.
                    -->
                    <div
                      v-if="notice"
                      ref="noticeBox"
                      class="side-part"
                    >
                      <DeskNotice :tone="notice.tone">
                        {{ notice.text }}
                      </DeskNotice>
                    </div>
                    <DeskNotice
                      v-if="dirty && !notForSigning(data)"
                      id="sign-off-why"
                      tone="problem"
                    >
                      Save or discard your changes before signing off.
                    </DeskNotice>
                    <nav
                      v-if="outline.length > 0 && data.draft.body.length > 0"
                      class="side-part outline"
                      aria-label="What this draft posts"
                    >
                      <p class="rd-rubric outline-label">
                        {{ data.draft.status === 'posted' ? 'What was posted' : 'What posts' }}
                      </p>
                      <ol class="outline-rows">
                        <li
                          v-for="row in outline"
                          :key="row.anchor"
                          class="outline-row"
                        >
                          <!--
                        `aria-controls` is the part of the post this row reaches, which is
                        true of it and is also the only thing in the markup that says where
                        a row goes — the browser test reads it to check that pressing a row
                        brings that part into view, which is the half jsdom cannot hold.
                      -->
                          <button
                            type="button"
                            class="outline-go"
                            :class="{ 'outline-go--reached': reached === row.anchor }"
                            :aria-controls="row.anchor"
                            :aria-current="reached === row.anchor ? 'true' : undefined"
                            @click="goTo(row.anchor)"
                          >
                            <span class="outline-top">
                              <SeverityTag
                                v-if="row.severity"
                                :severity="row.severity"
                              />
                              <span
                                v-if="row.key"
                                class="outline-key rd-mono"
                              >{{ row.key }}</span>
                              <!--
                            Which half of the post this is, on the row rather than as two
                            headed groups: the order is the order it posts in, and grouping
                            would have to break that to say the same thing.
                          -->
                              <span
                                v-if="row.lands"
                                class="outline-lands"
                              >{{ row.lands }}</span>
                            </span>
                            <span class="outline-label-text">{{ row.label }}</span>
                          </button>
                        </li>
                      </ol>
                    </nav>

                    <!--
                    **The foot: what the sign-off does, and the two other ways out of the
                    draft** — S05's note and its two links. The sign-off itself is the
                    header's action.
                  -->
                    <div
                      v-if="!notForSigning(data)"
                      class="side-part foot-acts"
                    >
                      <!--
                      **What happens after the press, kept current.** DEV-932: it asks this
                      page to read again; it writes nothing.
                    -->
                      <PostingWatch
                        :draft="data.draft"
                        :review="data.review"
                        :last-activity-at="review.data.value?.timings.last_activity_at"
                        :body-is-stale="data.body_is_stale"
                        :record-is-current="recordIsCurrent"
                        :reread="reread"
                      />
                      <p
                        v-if="signOffNote"
                        class="rd-meta sign-off-note"
                      >
                        {{ signOffNote }}
                      </p>
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
                          class="link-act"
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
                          class="link-act"
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
                    </div>
                  </div>
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

/*
 * **S05's two columns: what posts, and what you decide about it.**
 *
 * A container query on the tab's own width, as Findings' bench is (DEV-1328): beside the
 * conversation the main column is about 920px at a 1,600px window, so a rule keyed on the
 * viewport would put two columns into room for one. From 760px of the tab the side column
 * takes 320px, the width *Post as*'s three segments need on one line; under it the side
 * column follows the preview.
 */
.draft {
  container: draft / inline-size;
}

.bench {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 28px;
  align-items: start;
  min-width: 0;
}

@container draft (min-width: 760px) {
  .bench {
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: 0;
    align-items: stretch;
  }

  .main {
    padding-right: 28px;
  }

  .aside {
    padding-left: 28px;
    border-left: 1px solid var(--rd-line);
  }
}

.main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* -------------------------- the preview and its views -------------------------- */

.view-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  align-items: center;
  justify-content: space-between;
}

/* "What GitHub will show": S05 sets it as the column's heading, in the body ink. */
.view-label {
  margin: 0;
  color: var(--rd-ink);
  font-size: var(--rd-type-lead);
  font-weight: 600;
}

/*
 * **Segments, one quiet ground under the chosen one** — S05's view switch, and the same
 * shape as Post as below it, so the page has one segmented control and not two.
 */
.segments,
.verdicts {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 2px;
  padding: 3px;
  border-radius: var(--rd-radius-sm);
  background: var(--rd-ground);
}

.segment,
.verdict-segment {
  padding: 5px 10px;
  border: 0;
  border-radius: var(--rd-radius-sm);
  background: transparent;
  color: var(--rd-ink-muted);
  font: inherit;
  font-size: 13px;
  line-height: 20px;
  cursor: pointer;
}

.segment[aria-selected='true'],
.verdict-segment--on {
  background: var(--rd-surface);
  color: var(--rd-ink);
  font-weight: 600;
  box-shadow: 0 0 0 1px var(--rd-line);
}

.segment:focus-visible,
.verdict-segment:focus-within {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 1px;
}

.preview {
  min-width: 0;
  padding: 0;
}

.pane {
  padding: 24px 24px;
}

.rendered {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* Where this part lands on GitHub: a caption on the preview, not a heading in the review. */
.lands {
  margin: 0;
  color: var(--rd-ink-muted);
}

.blocks {
  display: flex;
  flex-direction: column;
  gap: 20px;
  min-width: 0;
}

.block {
  min-width: 0;
  /* So a row pressed in the outline does not land with its heading under the top edge. */
  scroll-margin-top: 16px;
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

.changes {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.changes-lead {
  margin: 0;
  max-width: var(--rd-measure);
}

.commits {
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--rd-line);
}

.commit {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
  padding: 8px 0;
  border-bottom: 1px solid var(--rd-line);
}

.commit-sha {
  font-family: var(--rd-font-mono);
  font-size: 13px;
}

.commit-summary {
  flex: 1 1 200px;
  min-width: 0;
  overflow-wrap: anywhere;
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

/* ------------------------------- the side column ------------------------------- */

.aside {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

/* A part of the side column: S05 draws no cards here, only a label over what it holds. */
.side-part {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.side-part > .rd-rubric,
.verdict > legend {
  margin: 0;
  padding: 0;
}

.verdict > legend {
  margin-bottom: 10px;
}

.verdict:disabled .verdict-segment {
  cursor: not-allowed;
}

.since {
  margin: 0;
}

/* The held-back card: its own padding, which `HeldBackRows`' row rules reach to. */
.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

/*
 * **The outline and the foot, pinned under the header while the preview scrolls.** 132px
 * is the pinned header's 113 and a gap. Only beside the preview: under it the column is
 * the foot of the page, and a pinned group there would float over what was just reached.
 */
.side-pinned {
  display: flex;
  flex-direction: column;
  gap: 24px;
  min-width: 0;
}

@container draft (min-width: 760px) {
  .side-pinned {
    position: sticky;
    top: 132px;
  }

  /*
   * **Not pinned while a form is open in the foot.** Send back adds 144px and Ticket what
   * is left 217px, which can run the stuck group past a short window with its own submit
   * below the edge; open, the group scrolls with the page so every part of it is reachable.
   */
  .side-pinned--held {
    position: static;
  }
}

.foot-acts {
  padding-top: 16px;
  border-top: 1px solid var(--rd-line);
}

.sign-off-note {
  margin: 0;
}

/* S05's two quiet links: the other ways out of a draft, not a second primary. */
.link-act {
  align-self: flex-start;
  padding: 2px 0;
  border: 0;
  background: transparent;
  color: var(--rd-accent-dark);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.link-act:hover:not(:disabled) {
  text-decoration: underline;
}

.link-act:disabled {
  color: var(--rd-ink-faint);
  cursor: not-allowed;
}

.link-act:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

/* ------------------------------- the outline ---------------------------------- */

.outline-rows {
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--rd-ink);
  min-width: 0;
  /* With the foot under it, the pinned group fits a 900px window: 132 + 360 + ~260. */
  max-height: max(120px, min(360px, calc(100vh - 460px)));
  overflow-y: auto;
  overscroll-behavior: contain;
}

.outline-row {
  border-bottom: 1px solid var(--rd-line);
  min-width: 0;
}

.outline-go {
  display: flex;
  flex-direction: column;
  gap: 3px;
  width: 100%;
  padding: 8px 10px;
  border: 0;
  border-left: 3px solid transparent;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  min-width: 0;
}

.outline-go:hover {
  background: var(--rd-ground);
}

.outline-go:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -3px;
}

/* The one last gone to, marked the way the bench marks the finding it is drawing. */
.outline-go--reached {
  border-left-color: var(--rd-ink);
  background: var(--rd-ground);
}

.outline-top {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.outline-key {
  color: var(--rd-ink-faint);
  font-size: 12px;
}

.outline-lands {
  font-size: 12px;
  color: var(--rd-ink-muted);
}

.outline-label-text {
  font-size: 14px;
  line-height: 1.3;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* ------------------------------- edits and the rest ------------------------------ */

.edit {
  margin: 0;
  font-size: 14px;
}

.edit-verb {
  font-weight: 500;
}

/* How many times a finding moved: a phrase, not a sentence, and not a fault. */
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
</style>
