<script setup lang="ts">
import type {
  DecideFindingRequest,
  Finding,
  ReviewDetail,
} from '~/types/desk'
import type { DeskNotice } from '~/utils/format'
import type { FindingList } from '~/utils/findings'
import type { TrackOption } from '~/components/DecisionTrack.vue'

import { commitLabel, movesSince, nothingMoved } from '~/utils/rounds'

/**
 * Step 5 — **Code review**. Board A8, which Patrick approved on 10 October 2026.
 *
 * ```
 *  1 blocker. 6 should change, 3 questions, 4 small points.
 *  Step 5 has finished. 6 of 14 findings were reproduced by running something.
 *
 *  WHERE THEY SIT IN WHAT WAS BUILT · click a part to see only its findings
 *  [ Everything ] [ The export context ] [ The scheduled… ] [ Outside the parts ]
 *
 *  ( To post · 12 )  For a ticket · 1   Set aside · 1      J K move
 *  ┌──────────────────────┬────────────────────────────────────────────────────┐
 *  │ BLOCKER  F9      ◂   │ The column policy is skipped entirely               │
 *  │ The column policy is │ Reproduced · Allowed columns on the report…         │
 *  │ skipped entirely     │ ┌ YOUR CALL ──────────────────────────────────────┐ │
 *  │ ColumnPolicy.php:52  │ │ Keep Y │ Cut X │ Regrade G │ Rewrite E │ Tkt T │ │
 *  │──────────────────────│ └─────────────────────────────────────────────────┘ │
 *  │ SHOULD   F1          │ WHAT              │ THE CODE, AND THE COMMENT       │
 *  │ An empty allowed-    │ …                 │ 50 │ public function allowed(   │
 *  │ columns list means…  │ WHY IT MATTERS    │ 52 │   return $this->columns    │
 *  │ ExportContext.php:18 │ …                 │    │ ▸ Should · F9 — the policy │
 *  │ …thirteen more…      │ WHAT I'D DO       │                                 │
 *  └──────────────────────┴────────────────────────────────────────────────────┘
 *  11 to post, 2 for a ticket, 1 set aside        [ Go to the draft → ]
 * ```
 *
 * This is the page the whole issue is about. The one it replaces printed nineteen findings
 * and their reasoning as paragraphs, up to 326 words each, and Patrick called it "a bit of a
 * fucking dumpster fire". Of this board: "I really like each finding. I like the way it's
 * formatted… I love the code with the comments, the inline style."
 *
 * ## It is a workbench now — step 1.2 of the design loop
 *
 * The board drew one column of rows, each opening in place. Patrick, 15 October 2026, on
 * what that costs him: he accepts and posts **every** finding, because "there's too much
 * going on". Opening a row pushed the rest of the review down the page, the decision was at
 * the foot of whatever had just unfolded, and the only decision that was cheap was the one
 * that needed no decision.
 *
 * So: **a dense list on the left and the finding in full on the right**, which is direction
 * B of the design pass, approved emphatically. Four things follow from it.
 *
 * 1. **One finding is always showing.** `picked` falls back to the first row rather than to
 *    nothing, because a workbench with an empty bench is a list with an empty half.
 * 2. **The list does not move while you read.** It is sticky and scrolls in its own pane,
 *    so the next finding is where it was when you decided on this one.
 * 3. **The call leads the pane.** "Your call" is the first thing under the finding's name
 *    rather than the last thing after its proof — the decision is what this screen is for.
 * 4. **J and K move, and every decision has a key.** Ticket and Set aside cost exactly what
 *    Post costs, which is the whole of what he asked for.
 *
 * **The row loses two of its five cells** — how it was checked, and which part it is in.
 * Neither is lost from the screen: both are on the line under the finding's name in the
 * pane beside it, where they are read about the finding being decided rather than scanned
 * down a column. A 300px list cannot carry five columns and stay a list.
 *
 * ## Containing it
 *
 * He named the risk in the same breath — **"I think you're going to have trouble trying to
 * contain that"** — and containing it is half the issue. Four rules, and each is one of
 * DEV-878's acceptance criteria rather than a preference:
 *
 * 1. **A row in the list is one line.** Its severity, its number and title, and where it
 *    is. Nothing else. A finding with no title shows its claim clamped to two lines — never
 *    the whole of it, which on this review runs to 56 words.
 * 2. **One finding is showing at a time.** `picked` is a number, not a set.
 * 3. **The reasoning is never shown unfolded.** It is behind a button labelled with its own
 *    word count, which is the measurement a reader needs before deciding to read it.
 * 4. **Only the code block scrolls sideways.** The page does not, at 1440 or at 390.
 *    `CodeHunk` owns that and is the only `overflow-x` on the page.
 *
 * ## Two reads, and the second may fail
 *
 * `GET /reviews/{id}` is the findings. `GET /reviews/{id}/draft` is what the decisions need:
 * the draft's status, and the blocks — because `decideFinding` changes a finding's block and
 * a draft written as one block of markdown holds none, which the page says in words.
 *
 * **The draft read is allowed to fail without blocking the page.** It answers 404 when no
 * session has written one, and a review with no draft is a perfectly good code review page
 * with nothing to decide yet. So it is read outside `DeskState` and its absence is a state
 * the controls describe, not an error over the findings.
 *
 * ## What is here that the board does not draw
 *
 * **"What the review did not look at."** The one-page review this retires was the only place
 * `not_checked` was drawn, and it is step 5's own statement about its own coverage — the
 * sentence the posted review carries. Losing it with the page it was on would have been the
 * quiet kind of regression.
 */
/*
 * **The page is wider than the rest of the app**, which the layout reads off this. Patrick,
 * 16 October 2026: the bench leaves the 1,240px shell for the window, capped near 1,600px,
 * with the extra going to the list and the hunk — the prose fields keep their own measure,
 * so what grows is the two columns that were being squeezed and not the reading.
 */
definePageMeta({ wide: true })

const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

/*
 * The draft, read beside the review rather than inside it. See the note above: its refusal
 * is a state this page can draw, so it must not reach `DeskState`.
 */
const draft = desk.draft(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `Code review · #${review.pr} · Review Desk` : 'Code review · Review Desk'
  }),
})

/* ------------------------------- What is on screen ------------------------------- */

/** Which of the three lists is showing. */
const list = ref<FindingList>('post')

/** Which part is filtering: 0 for everything, -1 for outside the parts. */
const part = ref(0)

/**
 * Which finding the bench is showing, by its number.
 *
 * **`0` means "whichever is first", not "none".** `shown` below falls back to the first row
 * of whatever is listed, so a page that has just opened, a list that has just been switched
 * and a part that has just been filtered all draw a finding rather than an empty half.
 * Nothing here has to reset it to a valid row; the fallback is what makes it always valid.
 */
const picked = ref(0)

/** Whether the shown finding's reasoning is unfolded. Reset whenever another is picked. */
const reasoning = ref(false)

const working = ref(false)
const notice = ref<DeskNotice | null>(null)

/**
 * The head moves since the draft was last written.
 *
 * **The draft's own time, not the round's.** A draft written before a push describes a
 * commit the pull request has moved past, so what the page owes a reader is what moved
 * *after* this body was written — which is `movesSince`, and is empty for a review with no
 * draft at all, because there is nothing for the moves to be out of date against.
 */
const sinceTheDraft = computed(() => {
  const updates = detail.data.value?.head_updates
  if (!updates) return []
  return movesSince(updates, draft.data.value?.draft.updated_at ?? null)
})

/**
 * Show a finding.
 *
 * **Not a toggle.** The row used to open and close, so pressing the open one closed it;
 * here the pane always holds something, and a row that unshowed itself would leave the
 * bench empty and the reader looking at what they had just chosen to read.
 */
function choose(seq: number) {
  picked.value = seq
  reasoning.value = false
}

/*
 * Changing the list or the part takes the bench back to the top of what is now listed. The
 * row that was showing is gone from the list either way, and `picked` is left at 0 — which
 * `shown` reads as the first row rather than as none.
 */
watch([list, part], () => {
  picked.value = 0
  reasoning.value = false
})

/**
 * Arriving at one finding, named in the hash — `…/code-review#F9`.
 *
 * **The Solution page's finding lines reach a finding, not the page it is on.** DEV-919's
 * criterion, and the reason it has to be a criterion: a row of that table says "2 to post"
 * about a part, and a link that dropped the reader at the top of fifteen rows across three
 * lists would hand back the work the line had just done for them.
 *
 * Three things have to happen in order, and the order is the whole of it.
 *
 * 1. **The list has to change first.** A held-back finding is not in the list this page
 *    opens on, so opening the row without choosing its list opens a row that is not drawn.
 * 2. **The part filter has to be cleared**, for the same reason: a filter left on another
 *    part hides the row that was asked for.
 * 3. **`picked` is set after a tick.** The watcher above takes the bench back to the first
 *    row whenever the list or the part changes — so setting all three in one go has the
 *    watcher undo the third, and the link silently shows the wrong finding. That is the
 *    failure this comment exists to stop someone reintroducing by tidying the three lines
 *    into one.
 *
 * Keyed on the findings arriving rather than on mount, because the hash is readable before
 * the read finishes and a `seq` cannot be checked against a review that is not there yet.
 * `once` is deliberate: this is an arrival, not a mode. Once the reader is here, their own
 * clicks own what is open, and a later re-read must not drag the page back.
 */
const arrived = ref(false)

watch(
  () => detail.data.value,
  async (data) => {
    if (arrived.value || !data) return
    const seq = seqInHash(route.hash)
    if (seq == null) {
      /* No finding named: nothing to arrive at, and nothing to keep watching for. */
      arrived.value = true
      return
    }
    const finding = data.findings.find(one => one.seq === seq)
    /*
     * A hash naming a finding this review does not have is left alone rather than reported.
     * It is a stale link — a finding withdrawn, or another review's number — and the page
     * behind it is correct and complete; an error over it would be the page blaming the
     * reader for the link they followed.
     */
    arrived.value = true
    if (!finding) return

    list.value = listOf(finding)
    part.value = 0
    await nextTick()
    picked.value = finding.seq
    reasoning.value = false

    /*
     * And scrolled to, because the browser's own hash scrolling has already happened: the
     * row did not exist when the page loaded, since the list it is in was chosen three
     * lines ago. `block: 'center'` rather than `'start'` so the row and the detail it just
     * opened are both on screen.
     */
    await nextTick()
    document.getElementById(finding.key)?.scrollIntoView({ block: 'center' })
  },
  { immediate: true },
)

/**
 * Another review on this page starts as the page opens it.
 *
 * Every one of these is about *this* review's findings, and `open` is a sequence number: on
 * a page that was reused across a review change, a finding left open on review 50 would come
 * back open on review 51 over whichever finding shared the number. DEV-960, and the watch is
 * DEV-958's, whose own page was written with it.
 *
 * **It cannot fire as pages are keyed today, and is kept deliberately.** The measurement
 * and the argument are in `draft.vue`, once.
 *
 * Five things, and three of them are not the open row:
 *
 * 1. **The two filters.** `part` is an index into *this* brief's parts and `list` is which
 *    of three lists is showing — review 50's answer to "which of these do I want to see",
 *    over review 51's findings.
 * 2. **`picked` and `reasoning` by hand, not by way of the watcher above.** It fires on a
 *    *change* to the list or the part, and arriving with both already at their defaults
 *    changes neither — which is the common case and the one that would be missed.
 * 3. **`notice`** — what a decision on review 50's finding came to, over review 51.
 * 4. **`arrived`**, which is once-only *per review* rather than per page: a `#F9` on the
 *    second review is an arrival of its own, and a page that was reused with `arrived` still
 *    set would ignore it. Re-arming is safe — the watch it gates is keyed on the findings
 *    arriving, so it next runs against this review's own.
 */
watch(reviewId, () => {
  list.value = 'post'
  part.value = 0
  picked.value = 0
  reasoning.value = false
  notice.value = null
  arrived.value = false
})

/* --------------------------------- The findings ---------------------------------- */

const parts = computed(() => detail.data.value?.brief?.parts ?? [])

/** Everything the review found, most serious first. */
const findings = computed(() => mostSeriousFirst(detail.data.value?.findings ?? []))

/** The rows showing: this list, and this part. */
const rows = computed(() =>
  findings.value.filter(
    finding => listOf(finding) === list.value && inPart(finding, part.value, parts.value),
  ),
)

/**
 * The findings this review would post, which is what the headline counts.
 *
 * `willPost`, not `listOf(...) === 'post'`: a finding carried into this round as fixed keeps
 * the disposition it was carried with, so it can sit in the "To post" list and still not be
 * posted. The list is what the developer decided; the headline is what the author receives.
 */
const posting = computed(() => findings.value.filter(willPost))

/**
 * Whether the list showing is empty over the whole review, part filter or no part filter.
 *
 * **Which is what the empty sentence turns on, and `part === 0` was not it.** A list that
 * holds nothing anywhere holds nothing under a part either, and "Nothing here for this
 * part" there blames a filter that is not the reason — the same misattribution the other
 * way round, reachable by choosing "For a ticket" on a review with nothing held back and
 * then choosing a part. The segments carry this count already; this reads the same fact
 * where the sentence needs it.
 */
const listIsEmpty = computed(
  () => !findings.value.some(finding => listOf(finding) === list.value),
)

const tiles = computed(() => partTiles(findings.value, parts.value))

/**
 * The three segments, each with how many are in it.
 *
 * Counted over the whole review rather than over the part showing, which is the board's own
 * arrangement: the parts strip filters the rows, and the segments say how the review as a
 * whole came out. Two filters each narrowing the other's counts would leave a reader unable
 * to say what either number meant.
 */
const segments = computed<TrackOption[]>(() =>
  (['post', 'ticket', 'aside'] as FindingList[]).map(one => ({
    value: one,
    label: LIST_LABELS[one],
    count: findings.value.filter(finding => listOf(finding) === one).length,
  })),
)

const showing = computed(() => {
  if (part.value === 0) return 'Most serious first.'
  const tile = tiles.value.find(one => one.number === part.value)
  return `Only: ${tile?.name ?? 'that part'}.`
})

/* ------------------------------ What step 5 is doing ----------------------------- */

/** Layer 5's own row, which says whether the list can still grow. */
const layer = computed(() =>
  detail.data.value?.layers.find(one => one.name === 'code_review') ?? null,
)

/**
 * What the page says when the review has no findings at all.
 *
 * **"Not written yet" and "none found" are different answers**, which is DEV-878's
 * criterion, and the layer's own state is what tells them apart. A step that has not run has
 * said nothing about the code; a step that finished and found nothing has said something
 * worth hearing. Drawing the same words for both would turn a clean review into a page that
 * looks broken — and, the other way round, would have a review that has not started claim
 * the code is fine.
 *
 * **It is the headline, not a panel under one.** The headline is the page's one sentence, so
 * a review with nothing in it has to say so *there*: `headline()` counts findings and would
 * otherwise print "Nothing to post" over a body saying the review had not run, which is the
 * page contradicting itself in its largest type.
 */
const nothingYet = computed(() => {
  const state = layer.value?.state
  if (state === 'done') {
    return {
      heading: 'The code review found nothing to say.',
      said:
        'That is a result rather than an absence: whatever the review looked at, it had '
        + 'nothing to raise about it.',
    }
  }
  if (state === 'running') {
    return {
      heading: 'The code review has not written anything yet.',
      said: 'Step 5 is running. Findings appear here as it writes them.',
    }
  }
  if (state === 'failed') {
    return {
      heading: 'The code review did not finish.',
      said:
        'Step 5 failed, so there are no findings — and their absence says nothing at all '
        + 'about the code.',
    }
  }
  return {
    heading: 'The code review has not run yet.',
    said:
      'There are no findings because nothing has looked, which is not the same as '
      + 'nothing being found.',
  }
})

/** The page's one sentence: the counts, or what kind of nothing this is. */
const heading = computed(() =>
  findings.value.length === 0 ? nothingYet.value.heading : headline(posting.value),
)

/**
 * The line under it.
 *
 * Two sentences at most, and the first only while step 5 is running. DEV-878: "While step 5
 * is running the page says the list can still grow."
 */
const sub = computed(() => {
  if (findings.value.length === 0) return nothingYet.value.said

  const said: string[] = []
  if (layer.value?.state === 'running') {
    said.push('Step 5 is still running, so this list can still grow.')
  }
  const method = reproducedLine(findings.value)
  if (method) said.push(method)
  return said.length === 0 ? null : said.join(' ')
})

/* ------------------------------- One open finding -------------------------------- */

/**
 * The finding on the bench: the one picked, or the first of whatever is listed.
 *
 * **The fallback is the whole selection model.** Every other way of keeping `picked` valid
 * is a watcher that has to fire before the render that would otherwise draw an empty pane —
 * and there are four things that can invalidate it: the list, the part, a decision that
 * moves a finding to another list, and arriving at another review. One `??` answers all
 * four, and cannot be got wrong by adding a fifth.
 */
const shown = computed<Finding | null>(
  () => rows.value.find(finding => finding.seq === picked.value) ?? rows.value[0] ?? null,
)

/** Whether this row is the one on the bench. Read by the row and by the keys. */
function isShown(finding: Finding): boolean {
  return shown.value?.seq === finding.seq
}

/**
 * J and K, which move down and up the list.
 *
 * **vi's keys, not the arrow keys.** The arrows belong to the control the focus is in — the
 * three lists are a radiogroup and the parts strip is another, and both answer their arrows
 * already. A reader on this page has one hand on the keyboard and is deciding; J and K are
 * the keys that do not fight anything else on it.
 *
 * `keyIsForThePage` is the one rule about when a key is not a key, shared with the decision
 * track's own Y, X, G and E. The listener is added on mount and removed on unmount.
 *
 * **Moving stops at the ends rather than wrapping.** A list that wraps takes a reader from
 * the last finding to the first on a keystroke meant to do nothing, and the finding they
 * were deciding on is then somewhere above with no way to tell it moved.
 */
function stepBy(by: number) {
  const at = rows.value.findIndex(finding => finding.seq === shown.value?.seq)
  const next = rows.value[Math.min(Math.max(at + by, 0), rows.value.length - 1)]
  if (!next || next.seq === shown.value?.seq) return
  choose(next.seq)
  /*
   * And scrolled to inside the list's own pane. `nearest` moves it the least it can, so a
   * row already on screen does not jump; jsdom implements no scrolling at all, hence the
   * check rather than an optional call.
   */
  void nextTick(() => {
    const row = document.getElementById(`row-${next.seq}`)
    if (typeof row?.scrollIntoView === 'function') row.scrollIntoView({ block: 'nearest' })
  })
}

function onKey(event: KeyboardEvent) {
  if (!keyIsForThePage(event)) return
  const by = event.key.toLowerCase() === 'j' ? 1 : event.key.toLowerCase() === 'k' ? -1 : 0
  if (by === 0) return
  event.preventDefault()
  stepBy(by)
}

onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))

const blocks = computed(() => reasoningBlocks(shown.value?.evidence))

const words = computed(() => reasoningWords(shown.value?.evidence))

/** The label the fold carries: its own word count, so a reader knows before they open it. */
const foldLabel = computed(() =>
  reasoning.value
    ? 'Fold the reasoning'
    : `The full reasoning · ${count(words.value, 'word')}`,
)

/**
 * What has already happened to one finding, by somebody other than the developer.
 *
 * **The author's replies, which the retired one-page review was the only thing drawing.**
 * They belong to step 7 and step 7 has no page, so retiring that screen would have dropped
 * them — and a reply from the author is the one piece of context that changes what the
 * developer should decide here. Shown on the finding they are about rather than in a list
 * of their own, which is better than the page they came off: that one grouped them at the
 * top, away from the finding each was answering.
 *
 * `actor !== 'developer'`, so the developer's own decisions are not replayed at them. Those
 * are already on the page as the record the controls read.
 */
function saidByOthers(data: ReviewDetail, finding: Finding) {
  return data.outcomes
    .filter(outcome => outcome.finding_seq === finding.seq && outcome.actor !== 'developer')
    .map((outcome) => {
      /*
       * **Worded here, once per outcome, and not in the template.** The block below needs
       * the sentence, the quote and the time; three calls in the markup would be three
       * evaluations a render and three chances for them to drift — the measurement is in
       * `StepTrail`, where a title and a card built from two calls to the same clock could
       * say "just now" and "1 min ago" about one step.
       *
       * **And the detail is never printed.** This block drew `outcome.detail` raw until
       * DEV-958, which is the same defect as the queue's row and in the same words: a
       * harvested detail is JSON, so a reply on a finding came out as
       * `{"body":"…","comment_id":…}` inside the quotation that was meant to carry it.
       */
      const said = outcomeSaid(outcome)
      return {
        id: outcome.id,
        sentence: sentence(said.sentence),
        quote: said.quote,
        /*
         * **Who acted, where the sentence does not already say and where somebody did.**
         * Replacing `by the {{ outcome.actor }}` with the time alone left an author's own
         * reason with nobody in it; the first way back credited the recording actor for
         * every kind, and the harvester is the actor on every code change, so it read "the
         * code at line 40 changed in 9f3ac21 · by the harvester". `outcomeBy` carries both
         * halves and the argument for them; it is the only caller, and step 7 agrees by
         * drawing no actor at all, so this is where the rule lives rather than a thing two
         * screens share.
         */
        by: outcomeBy(said, outcome.actor),
        when: outcomeWhen(said, outcome.occurred_at),
      }
    })
}

/** The name of the part a finding sits in, for its row. */
function partName(finding: Finding): string {
  const at = partOf(finding, parts.value)
  return at === 0 ? 'Outside the parts' : (parts.value[at - 1]?.title ?? '')
}

/** How a finding's verification reads. `unknown` draws nothing: it said nothing. */
function verification(finding: Finding): string | null {
  if (finding.verification === 'reproduced') return 'Reproduced'
  if (finding.verification === 'read_not_run') return 'Read, not run'
  return null
}

/* ---------------------------------- The writes ----------------------------------- */

async function decide(seq: number, request: DecideFindingRequest) {
  const id = detail.data.value?.review.id
  if (id == null) return
  working.value = true
  const result = await desk.decideFinding(id, seq, request)
  working.value = false
  notice.value = noticeFor(result)
}

/* --------------------------------- The foot -------------------------------------- */

/**
 * Whether there is a draft to go on to, and what the link says.
 *
 * Step 6 is a page either way — it carries the trail, so it is never a dead end — but what
 * it holds is different, and the foot of this page is the one place to say so before a reader
 * follows it.
 */
const draftReady = computed(() => (detail.data.value?.draft_status ?? null) != null)

function coverage(data: ReviewDetail): string | null {
  return data.not_checked ?? null
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <ReviewStepPage
        :detail="data"
        step="code_review"
        :headline="heading"
        :sub="sub"
      >
        <DeskNotice
          v-if="notice"
          :tone="notice.tone"
        >
          {{ notice.text }}
        </DeskNotice>

        <!--
          **The rounds, and what has happened to this one's head commit.** DEV-962: a new
          commit on an unposted round moves that round rather than opening the next one, so
          the findings below may have been written against a commit the pull request has
          moved past — and this is where a reader is told. DEV-954's dismissed round is drawn
          as dismissed by the same component.
        -->
        <RoundTimeline
          :rounds="data.rounds ?? []"
          :current="data.review.id"
          :updates="data.head_updates ?? nothingMoved()"
        />

        <!--
          **What changed since the draft was written.** The issue asks for it by name, and it
          is a different question from "what has the head ever done": a draft written before
          a push describes a commit the pull request has moved past, and the moves *after*
          the draft's last write are the ones it is out of date about. Drawn only when there
          is a draft and something moved after it — otherwise it is a heading over nothing.
        -->
        <section
          v-if="sinceTheDraft.length > 0"
          class="since-draft"
          aria-labelledby="since-draft-heading"
        >
          <h2
            id="since-draft-heading"
            class="rd-rubric"
          >
            Changed since the draft was written
          </h2>
          <ul class="rd-stack">
            <!--
              **Keyed on the three facts together, because none of them is unique alone.**
              `at` is seconds and the record refuses nothing for sharing one — migration 18
              says two updates inside one second are ordered rather than refused — so two
              pushes in the same second would give Vue one key twice. Raised by a review of
              this branch.
            -->
            <li
              v-for="move in sinceTheDraft"
              :key="`${move.at}-${move.from_sha}-${move.to_sha}`"
              class="rd-note"
            >
              <span class="rd-mono">{{ commitLabel(move.from_sha) }}</span>
              →
              <span class="rd-mono">{{ commitLabel(move.to_sha) }}</span>
              <span class="rd-meta">{{ sinceLabel(move.at) }}</span>
            </li>
          </ul>
          <p class="rd-note rd-measure">
            The review session re-reads the change and updates its findings and the draft.
            Anything below was written before this.
          </p>
        </section>

        <!--
          A review with no findings has said everything it has to say in the headline and
          the line under it, which the shell draws. There is no list, no strip and no tally,
          because every one of them would be a count of nothing.
        -->
        <template v-if="findings.length > 0">
          <!--
            The parts strip. Drawn only when the brief describes the change in parts, which
            is DEV-878's criterion: "A brief with no parts shows no strip."
          -->
          <section
            v-if="parts.length > 0"
            class="strip"
          >
            <h2
              id="parts-strip"
              class="rd-rubric"
            >
              Where they sit in what was built
              <span class="rd-visually-hidden">— choose a part to see only its findings</span>
              <span
                class="hint"
                aria-hidden="true"
              >· click a part to see only its findings</span>
            </h2>
            <div
              class="tiles"
              role="radiogroup"
              aria-labelledby="parts-strip"
            >
              <button
                v-for="tile in tiles"
                :key="tile.number"
                type="button"
                role="radio"
                class="tile"
                :class="{ 'tile--picked': part === tile.number }"
                :aria-checked="part === tile.number"
                @click="part = tile.number"
              >
                <span class="tile-name">{{ tile.name }}</span>
                <span class="tile-tally">{{ tile.tally }}</span>
              </button>
            </div>
          </section>

          <section class="findings">
            <div class="controls">
              <DecisionTrack
                v-model="list"
                :options="segments"
                label="Which findings to show"
              />
              <p class="rd-note showing">
                {{ showing }}
              </p>
              <!--
                **Shown because they work.** DEV-876's rule about a key drawn on a control
                is the reason this line exists rather than a tooltip: the keys that decide
                are drawn on the buttons that decide, and the keys that move have nowhere
                else to be drawn — so this says what they do and points at the other set
                rather than listing it twice. `aria-hidden`, as the decision track hides
                its own: a screen reader reaches the rows by their own list.
              -->
              <p
                class="rd-note moves"
                aria-hidden="true"
              >
                <span class="key rd-mono">J</span>
                <span class="key rd-mono">K</span>
                move · the key on each button decides
              </p>
            </div>

            <!--
              **Two sentences, because a part filter is not always what emptied the list.**
              "Nothing here for this part" on the default "Everything" tile blames a filter
              that is not applied — on a review whose findings are all held back, which is
              the state it is most likely to be read in.
            -->
            <div
              v-if="rows.length === 0"
              class="rd-empty none"
            >
              {{
                listIsEmpty
                  ? `Nothing in this list. The review's findings are all in the other two.`
                  : 'Nothing here for this part.'
              }}
            </div>

            <!--
              **The bench: the list on the left, the finding on the right.** Direction B of
              the design pass. The list is sticky and scrolls in its own pane, so deciding
              on one finding does not move the next one — which is what made the page it
              replaces a page of scrolling rather than of deciding.
            -->
            <div
              v-else
              class="bench"
            >
              <ul class="rows">
                <!--
                  **`id` is the finding's key**, which is what a link from the Solution page
                  names in its hash and what a person quotes. The button inside carries
                  `row-{seq}`, which is what J and K scroll to — two names for two jobs, as
                  the detail's own two ids are.
                -->
                <li
                  v-for="finding in rows"
                  :id="finding.key"
                  :key="finding.seq"
                  class="line"
                >
                  <!--
                    **A row chooses; it no longer opens.** `aria-current` rather than
                    `aria-expanded`: nothing unfolds here, and one of these is the one being
                    read. The hidden label is the key alone — everything else in the row is
                    visible text a screen reader already reads.
                  -->
                  <button
                    :id="`row-${finding.seq}`"
                    type="button"
                    class="row"
                    :class="{ 'row--picked': isShown(finding) }"
                    :aria-current="isShown(finding) ? 'true' : undefined"
                    aria-controls="the-finding"
                    @click="choose(finding.seq)"
                  >
                    <span class="rd-visually-hidden">Show {{ finding.key }}:</span>{{ ' ' }}
                    <span class="grade">
                      <SeverityTag :severity="finding.severity" />
                      <span class="key rd-mono">{{ finding.key }}</span>
                      <!--
                        A carried finding's round status. DEV-878: "A carried finding from
                        an earlier round shows its round status."
                      -->
                      <DeskPill
                        v-if="roundStatusLabel(finding)"
                        :tone="isStillOpen(finding.round_status) ? 'info' : 'quiet'"
                      >
                        {{ roundStatusLabel(finding) }}
                      </DeskPill>
                    </span>{{ ' ' }}
                    <!--
                      The row's one line. A finding with no title shows its claim clamped to
                      two lines — the criterion's own wording — because the claim is a
                      paragraph and a row is a line. `title` attribute so the whole of it is
                      still reachable with a pointer, and the pane beside it shows all of it.
                    -->
                    <span
                      class="title"
                      :class="{ 'title--clamped': finding.title == null }"
                      :title="finding.title == null ? finding.claim : undefined"
                    >
                      <TickedText :text="finding.title ?? finding.claim" />
                    </span>{{ ' ' }}
                    <span class="where rd-mono">{{ whereOf(finding) }}</span>
                  </button>
                </li>
              </ul>

              <div
                v-if="shown"
                id="the-finding"
                class="detail"
                role="region"
                :aria-labelledby="`finding-${shown.seq}`"
              >
                <h3
                  :id="`finding-${shown.seq}`"
                  class="rd-display-card claim"
                >
                  <TickedText :text="shown.title ?? shown.claim" />
                </h3>

                <!--
                  **The two cells the row gave up.** How it was checked and which part of
                  the change it is in were columns of a five-column row, and a 300px list
                  cannot carry five columns. They are read here instead — about the one
                  finding being decided, rather than scanned down a column.
                -->
                <p class="rd-note about">
                  <span
                    v-if="verification(shown)"
                    class="method"
                    :class="{ 'method--ran': shown.verification === 'reproduced' }"
                  >
                    <span
                      class="dot"
                      aria-hidden="true"
                    />
                    {{ verification(shown) }}
                  </span>
                  <span class="in-part">{{ partName(shown) }}</span>
                </p>

                <!--
                  **Which commit this line was measured on, where that is not the head.**
                  DEV-978: the link under this finding goes to the commit the line was
                  counted on, and the pull request may have moved past it — a carried
                  finding keeps the earlier round's numbering, and since DEV-962 a head
                  moves without a new round opening. The sentence names both commits,
                  which is `withdrawnLine`'s rule: naming one of two leaves the reader to
                  find the other. Interpolated as text, never as markup.
                -->
                <p
                  v-if="measuredAt(shown, data.review)"
                  class="rd-note measured"
                >
                  {{ measuredAt(shown, data.review) }}
                </p>

                <p
                  v-if="isCarried(shown) && shown.continues_round"
                  class="rd-note carried"
                >
                  Raised in round {{ shown.continues_round }} and carried into this one.
                  <template v-if="shown.rounds_open">
                    Open for {{ count(shown.rounds_open, 'round') }}.
                  </template>
                  <template v-if="shown.round_status_checked">
                    <strong>Checked:</strong> {{ shown.round_status_checked }}
                  </template>
                </p>

                <!--
                  **The call leads the pane.** It was the last thing on an opened row, under
                  the proof and the folded reasoning — so the decision this screen exists for
                  was the one thing on it that had to be scrolled to. Patrick accepts and
                  posts almost everything, and "there's too much going on" is what he gave as
                  the reason.
                -->
                <FindingCall
                  :finding="shown"
                  :sections="draft.data.value?.sections ?? null"
                  :draft-status="data.draft_status ?? null"
                  :working="working"
                  @decide="request => decide(shown!.seq, request)"
                />

                <div class="columns">
                  <div class="fields">
                    <div class="field">
                      <span class="rd-rubric">What</span>
                      <p class="rd-recorded said-text">
                        <TickedText :text="shown.claim" />
                      </p>
                    </div>

                    <div
                      v-if="shown.why_it_matters"
                      class="matters"
                    >
                      <span class="rd-rubric">Why it matters</span>
                      <p class="rd-recorded matters-text">
                        <TickedText :text="shown.why_it_matters" />
                      </p>
                    </div>

                    <!--
                      **"What I'd do", which is what it is.** The record calls the field a
                      suggested fix and the rubric said so; the three labelled fields a
                      reader reads down are a claim, its cost and what to do about it, and
                      Patrick named them in exactly those words. Nothing about the record
                      changed — this is the word over it.
                    -->
                    <div
                      v-if="shown.suggested_fix"
                      class="field"
                    >
                      <span class="rd-rubric">What I’d do</span>
                      <p class="rd-recorded">
                        {{ shown.suggested_fix }}
                      </p>
                      <ul
                        v-if="(shown.fix_references ?? []).length"
                        class="refs"
                      >
                        <li
                          v-for="reference in shown.fix_references ?? []"
                          :key="reference"
                          class="rd-mono ref"
                        >
                          {{ reference }}
                        </li>
                      </ul>
                    </div>

                    <!--
                      Why the session held it back or set it aside. Its own field rather
                      than part of the claim: it is the session's reasoning about what to
                      do, and the developer is about to decide whether they agree.
                    -->
                    <div
                      v-if="shown.disposition_reason && list !== 'post'"
                      class="field"
                    >
                      <span class="rd-rubric">
                        Why the session {{ list === 'ticket' ? 'held it back' : 'set it aside' }}
                      </span>
                      <p class="rd-recorded">
                        {{ shown.disposition_reason }}
                      </p>
                    </div>

                    <!--
                      What the author said about this finding, harvested after posting.
                      See `saidByOthers`: this is the one finding's worth of it, and
                      the whole of it is on step 7's own page, "What happened".
                    -->
                    <div
                      v-if="saidByOthers(data, shown).length"
                      class="field"
                    >
                      <span class="rd-rubric">What came back</span>
                      <p
                        v-for="outcome in saidByOthers(data, shown)"
                        :key="outcome.id"
                        class="rd-recorded reply"
                      >
                        <!--
                          **A text node between them, not a newline.** Both spans are
                          inline and the newline between two elements is a whitespace-only
                          node that `whitespace: 'condense'` deletes — so this read
                          "The code at line 40 changed in 9f3ac21.5 days ago", a commit id
                          with a digit welded to it, on the one value of the row a reader
                          copies. Step 7 draws the same pair with the sentence as bare
                          text in a flex row, which keeps its own trailing space; here the
                          sentence is wrapped, so the space has to be written.
                        -->
                        <span class="reply-kind">{{ outcome.sentence }}</span>{{ ' ' }}
                        <span class="rd-note">
                          <template v-if="outcome.by">{{ outcome.by }} · </template>{{ outcome.when }}
                        </span>
                        <!--
                          The author's words, where the outcome carries them, and
                          interpolated rather than handed to the browser as markup —
                          which is what lets a reply holding a backtick, a tag or
                          GitHub's own markdown arrive as the characters that were
                          typed. The whole of step 7 is a page now; this is the one
                          finding's worth of it, beside the finding it is about.
                        -->
                        <template v-if="outcome.quote">
                          <br>{{ outcome.quote }}
                        </template>
                      </p>
                    </div>

                    <p
                      v-if="shown.rule_key"
                      class="rd-note under"
                    >
                      Raised under rule {{ shown.rule_key }}.
                    </p>
                  </div>

                  <div class="evidence">
                    <div class="field">
                      <!--
                        The board's own caption, **but only when there is a comment under
                        a line to caption.** A finding with no hunk draws the sentence
                        saying why and its comment separately below, and a heading reading
                        "the comment as it will post" over a dashed box saying there is no
                        code promises something two lines further down.
                      -->
                      <span class="rd-rubric">
                        {{
                          shown.hunk
                            ? 'The code, and the comment as it will post'
                            : 'The code'
                        }}
                      </span>
                      <CodeHunk
                        v-if="shown.hunk"
                        :hunk="shown.hunk"
                        :flagged-line="shown.flagged_line"
                        :comment="shown.inline_comment"
                        :where="`${shown.hunk.file}${shown.flagged_line == null ? '' : `:${shown.flagged_line}`}`"
                      />
                      <p
                        v-else
                        class="rd-recorded no-code"
                      >
                        {{ whyNoCode(shown, data.brief) }}
                      </p>
                    </div>

                    <!--
                      The comment on a finding with no hunk at all. There is no line to put
                      it under, and dropping it would lose the one thing the author will
                      actually read.
                    -->
                    <div
                      v-if="shown.inline_comment && !shown.hunk"
                      class="field"
                    >
                      <span class="rd-rubric">The comment as it will post</span>
                      <p class="rd-recorded">
                        {{ shown.inline_comment }}
                      </p>
                    </div>

                    <div
                      v-if="shown.proof"
                      class="field"
                    >
                      <span class="rd-rubric">
                        Proof<template v-if="verification(shown)"> · {{ verification(shown) }}</template>
                      </span>
                      <pre class="proof">{{ shown.proof }}</pre>
                      <p
                        v-if="shown.proof_note"
                        class="rd-note proof-note"
                      >
                        {{ shown.proof_note }}
                      </p>
                    </div>
                  </div>
                </div>

                <!--
                  The reasoning, folded, labelled with its word count. **Never shown
                  unfolded**, which is DEV-878's criterion: on this review it runs to 221
                  words and on the one the board was drawn from, 326.
                -->
                <div
                  v-if="blocks.length > 0"
                  class="reasoning"
                >
                  <DeskButton
                    :aria-expanded="reasoning"
                    :aria-controls="`reasoning-${shown.seq}`"
                    @click="reasoning = !reasoning"
                  >
                    {{ foldLabel }}
                  </DeskButton>
                  <div
                    v-if="reasoning"
                    :id="`reasoning-${shown.seq}`"
                    class="blocks"
                  >
                    <div
                      v-for="(block, at) in blocks"
                      :key="at"
                      class="block"
                    >
                      <h4 class="block-label">
                        {{ block.label }}
                      </h4>
                      <p class="rd-recorded block-text">
                        <TickedText :text="block.text" />
                      </p>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </template>

        <!--
          What step 5 did not look at. Not on board A8, and the only place it was drawn was
          the one-page review this replaces — it is step 5's own statement about its own
          coverage, and the sentence the posted review carries.

          **Outside the findings branch, deliberately.** It sat inside it and so vanished
          from the one review that needs it most: a step 5 that finished and found nothing
          says "we looked at X and not at Y", and the page was dropping the second half
          exactly when it was the whole of the answer. Found by the self-review.
        -->
        <section class="not-checked">
          <h2 class="rd-rubric">
            What the review did not look at
          </h2>
          <p
            v-if="coverage(data)"
            class="rd-recorded"
          >
            {{ coverage(data) }}
          </p>
          <!--
            **And it says so when the field is empty, which is most reviews.** The screen
            this replaced drew the sentence; moving the section here carried the half a
            record happens to hold and left the half that says what an empty field means.
            An absent section reads as "nothing was missed", which is the one thing this
            field can never be read to say — a review names what it skipped only where it
            noticed skipping it.
          -->
          <p
            v-else
            class="rd-empty"
          >
            Nothing is recorded as unchecked. That is not the same as everything having been
            looked at: a review names what it skipped only where it noticed skipping it.
          </p>
        </section>

        <!--
          **The foot is last, after the coverage, and that is why it is its own branch.**
          It closes the page: the tally, and the way on to step 6. A section drawn under it
          reads as stranded past the end — which is what the coverage line did when it moved
          out of the branch above, in the picture this pull request commits.
        -->
        <template v-if="findings.length > 0">
          <section class="foot">
            <div class="foot-said">
              <p class="rd-display-card tally">
                {{ tally(findings) }}
              </p>
              <p class="rd-note">
                The draft is assembled from what you keep. Nothing posts until you have read
                it and signed it off.
              </p>
            </div>
            <DeskButton
              kind="primary"
              arrow
              :to="`/reviews/${data.review.id}/draft`"
            >
              {{ draftReady ? 'Go to the draft' : 'Step 6, the draft' }}
            </DeskButton>
          </section>
        </template>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
/* ------------------------------- the parts strip ------------------------------- */

.strip {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.strip h2 {
  margin: 0;
}

.hint {
  color: var(--rd-ink-faint);
  font-weight: 400;
}

/*
 * `auto-fit` with `minmax(min(…), 1fr)`, which is the three step pages' own pattern: four
 * tiles across at a laptop, two at a tablet, one at a phone. The `min(…, 100%)` is what
 * keeps a tile from being wider than the page at 390px, which is the whole of the criterion
 * that the page never scrolls sideways.
 */
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(150px, 100%), 1fr));
  gap: 8px;
  min-width: 0;
}

.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-start;
  min-height: 64px;
  padding: 10px 12px;
  border: 1px solid var(--rd-key-line);
  border-radius: var(--rd-radius-control);
  background: var(--rd-surface);
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  min-width: 0;
}

.tile--picked {
  border-color: var(--rd-ink);
  background: var(--rd-ground);
}

.tile:hover {
  background: var(--rd-ground);
}

.tile:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.tile-name {
  font-weight: 600;
  font-size: 14px;
  /* A part's title runs long; it wraps rather than widening the grid. */
  overflow-wrap: anywhere;
}

.tile-tally {
  font-size: 13px;
  color: var(--rd-ink-muted);
}

/* -------------------------------- the bench ------------------------------------ */

.findings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.controls {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 24px;
  align-items: center;
}

.showing {
  margin: 0;
}

/* The two keys that move, drawn as keys — the same chip the decision track draws. */
.moves {
  display: flex;
  gap: 6px;
  align-items: center;
  margin: 0 0 0 auto;
}

.moves .key {
  padding: 1px 6px;
  border: 1px solid var(--rd-key-line);
  border-radius: var(--rd-radius-key);
  background: var(--rd-surface);
  font-size: 11px;
}

/*
 * **The bench: a list that stays put, and the finding beside it.**
 *
 * A fixed width and not a fraction, because the list's job is to be scannable at one width
 * while the finding takes whatever is left — a fraction would hand the extra width of a
 * wide window to the list as well, and a 400px list is not more scannable than a 340px one,
 * it is just wider. Below 1,000px there is not enough for both and the two stack, the list
 * first: a phone reads the list, taps, and reads the finding under it.
 */
.bench {
  display: grid;
  grid-template-columns: 296px minmax(0, 1fr);
  gap: 28px;
  align-items: start;
  min-width: 0;
}

/* Past the old shell's width there is room for the list to breathe as well. */
@media (min-width: 1280px) {
  .bench {
    grid-template-columns: 340px minmax(0, 1fr);
    gap: 36px;
  }
}

/*
 * **The list is the thing that does not move.** Deciding on one finding used to push the
 * next one down the page by however tall the one above had unfolded; here the rows keep
 * their places, and the pane scrolls itself when the review is long. `100vh - 96px` leaves
 * the sticky offset and a little air at the foot.
 */
.rows {
  position: sticky;
  top: 16px;
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--rd-ink);
  max-height: calc(100vh - 96px);
  overflow-y: auto;
  overscroll-behavior: contain;
  min-width: 0;
}

.line {
  border-bottom: 1px solid var(--rd-line);
  min-width: 0;
}

/*
 * A row is three short lines: what grade and which finding, its title, and where it is.
 * The five-column row this replaces needed the width of the page; this one needs 320px,
 * which is what buys the pane beside it.
 */
.row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  padding: 10px 12px;
  border: 0;
  border-left: 3px solid transparent;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  min-width: 0;
}

.row:hover {
  background: var(--rd-ground);
}

.row:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -3px;
}

/*
 * **The one being read, marked by a bar and a ground.** Two cues rather than one: the bar
 * survives being told apart without colour, and the ground is what the eye finds when it
 * comes back to the list after reading.
 */
.row--picked {
  border-left-color: var(--rd-ink);
  background: var(--rd-ground);
}

.grade {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.title {
  font-size: 14.5px;
  line-height: 1.32;
  /*
   * Two lines in the list, whatever the finding. A title is twelve words by the house
   * style and most fit one line at this width; the few that do not are cut here rather
   * than widening every row to the longest one.
   */
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/*
 * **A claim standing in for a title is clamped to exactly two lines**, which is the
 * criterion's own wording: "never the whole of it". Every title in this list is clamped to
 * two now, so this class carries no declaration of its own — it is kept because the
 * criterion is about the claim in particular, and a test reads it to say which rows are
 * showing a claim rather than a title.
 */
.title--clamped {
  font-style: normal;
}

.key {
  color: var(--rd-ink-faint);
  font-size: 12px;
}

/*
 * The path, on one line and cut at the end rather than wrapped: a row that grows a line
 * for a deep path is a row whose height depends on where the code lives. The whole of it
 * is in the pane beside the list, over the code itself.
 */
.where {
  font-size: 12px;
  color: var(--rd-ink-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

/*
 * How it was checked: a filled dot for reproduced, a hollow one for read. Two shapes, so the
 * two are told apart without colour — and nothing at all for `unknown`, because a finding
 * that said nothing about its method has not said it was read.
 */
.method {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  white-space: nowrap;
}

.method--ran {
  color: var(--rd-accent-dark);
}

.dot {
  width: 9px;
  height: 9px;
  border: 2px solid currentcolor;
  border-radius: 50%;
  box-sizing: border-box;
}

.method--ran .dot {
  background: currentcolor;
}

.in-part {
  min-width: 0;
  overflow-wrap: anywhere;
}

/* ---------------------------- the finding on the bench ------------------------- */

.detail {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-width: 0;
}

.claim {
  margin: 0;
  max-width: 820px;
}

/*
 * **The two cells the row gave up**, on one line under the finding's name: how it was
 * checked, and which part of the change it sits in.
 */
.about {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  align-items: center;
  margin: -8px 0 0;
}

/*
 * **The call leads the pane now**, so the rule that divided it from the fields above it
 * has nothing above it to divide from. Reached through `:deep` rather than by giving the
 * component a prop: where a thing sits is the page's business, and a border is not a
 * decision `FindingCall` should be asked to take a flag for.
 */
.detail :deep(.call) {
  padding-top: 0;
  border-top: 0;
}

/*
 * The same quiet weight as the carried note above it, and no colour: it is a fact about
 * where the link points, not a problem the developer has to deal with.
 */
.measured {
  margin: 0 0 12px;
  max-width: var(--rd-measure);
}

.carried {
  margin: 0;
  max-width: var(--rd-measure-recorded);
}

.carried strong {
  font-weight: 600;
}

/*
 * **The hunk beside the fields**, which is what direction B asks for: What, Why it matters
 * and What I'd do read down the left, and the code they are about is level with them.
 *
 * `minmax(0, …)` on both, and the second a little wider, because the thing in it is a code
 * block with a gutter. Below 1,180px the pane is too narrow to put a hunk beside a
 * paragraph without making both unreadable, and the two stack — the fields first, since
 * they are what the decision is taken on.
 *
 * **Fractions, and the prose is capped by its own rule rather than by its track.** The
 * first version of this sized the fields column at `--rd-measure-recorded` so that the
 * hunk could take everything left — which is backwards at the width where it matters:
 * 70ch resolves to about 700px, so at a 1,440px window the fields took 699 of the 948
 * going, the code was left 247, and the page scrolled sideways by 18. The cap belongs on
 * the paragraphs, where `.rd-recorded` already puts it. The columns divide what there is,
 * and the extra width of a wide window reaches the code because the code's share is the
 * larger one.
 */
.columns {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 24px 28px;
  align-items: start;
  min-width: 0;
}

@media (min-width: 1180px) {
  .columns {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.25fr);
    gap: 24px 32px;
  }
}

.fields,
.evidence {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-width: 0;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field p {
  margin: 0;
}

.said-text {
  font-size: 16px;
}

/* The board's left bar on "why it matters": the one thing in the column with weight. */
.matters {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px 16px;
  border-left: 3px solid var(--rd-ink);
  border-radius: 0 var(--rd-radius-control) var(--rd-radius-control) 0;
  background: var(--rd-ground);
}

.matters-text {
  margin: 0;
  font-weight: 500;
}

.refs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  margin: 2px 0 0;
  padding: 0;
  list-style: none;
}

/*
 * **A reference breaks rather than taking the page sideways.** Found by the design loop's
 * own shooter, not by a test: review #9614's "What I'd do" names
 * `domain/Funding/Actions/UpdateFundingTotalInternalUsedAmount.php`, and at 390px that one
 * path made the whole page 511px wide. It predates the bench — the same row measured 523px
 * before it — and it is the one rule this page is held to that no test covers for this
 * review, because the end-to-end measurement is taken on a seeded review whose paths are
 * shorter.
 */
.ref {
  font-size: 12.5px;
  color: var(--rd-ink-muted);
  min-width: 0;
  overflow-wrap: anywhere;
}

.under {
  margin: 0;
}

.reply {
  margin: 0;
  /*
   * Not `pre-wrap` through `rd-recorded`'s own rule by accident: an outcome's detail is a
   * sentence the harvester stored verbatim, and its paragraph breaks are the author's.
   */
  padding: 10px 14px;
  border-left: 3px solid var(--rd-question-ink);
  border-radius: 0 var(--rd-radius-control) var(--rd-radius-control) 0;
  background: var(--rd-question-wash);
}

.reply + .reply {
  margin-top: 8px;
}

.reply-kind {
  font-weight: 600;
}

.no-code {
  margin: 0;
  padding: 14px 16px;
  border: 1px dashed var(--rd-line-strong);
  border-radius: var(--rd-radius-control);
  color: var(--rd-ink-muted);
  font-size: 14px;
}

/*
 * The proof, reversed out of ink as the board draws it. `pre-wrap`, not `pre`: a proof is
 * text the agent wrote, not code with a gutter beside it, so wrapping it loses nothing and
 * a second horizontal scroller on the page would.
 */
.proof {
  margin: 0;
  padding: 12px 14px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ink);
  color: #f3f5f4;
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.proof-note {
  margin: 0;
}

/* ------------------------------- the reasoning --------------------------------- */

/*
 * `align-items: flex-start` so the fold is a button and not a bar. A column flex container
 * stretches its children, and a full-width "The full reasoning · 214 words" reads as a
 * section heading rather than as something to press.
 */
.reasoning {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 16px;
  min-width: 0;
}

.blocks {
  align-self: stretch;
}

.blocks {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(320px, 100%), 1fr));
  gap: 20px 36px;
  min-width: 0;
}

.block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.block-label {
  margin: 0;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--rd-line);
  font-size: 15px;
  font-weight: 600;
}

.block-text {
  margin: 0;
  color: var(--rd-ink-dim);
  font-size: 14.5px;
  /* The record's paragraph breaks are kept. It is recorded text, never markup. */
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

/* --------------------------------- the foot ------------------------------------ */

.not-checked {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.not-checked h2,
.not-checked p {
  margin: 0;
}

.foot {
  display: flex;
  flex-wrap: wrap;
  gap: 14px 32px;
  align-items: center;
  padding: 22px 24px;
  border-radius: var(--rd-radius-section);
  background: var(--rd-ground);
}

.foot-said {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1 1 420px;
  min-width: 0;
}

.foot-said p {
  margin: 0;
}

.tally {
  font-size: 24px;
}

/*
 * **Under 1,000px the bench stacks**, which is the three step pages' own answer to the same
 * problem: a board has one width and an app has two. The list comes first and keeps its
 * rows; the finding is read under it. Nothing sticks — a pinned list at a phone's width is
 * a list sitting on top of the thing it was used to choose.
 */
@media (max-width: 999px) {
  .bench {
    grid-template-columns: minmax(0, 1fr);
    gap: 22px;
  }

  .rows {
    position: static;
    max-height: none;
    overflow-y: visible;
  }
}

@media (max-width: 760px) {
  .columns {
    gap: 22px;
  }

  .moves {
    margin-left: 0;
  }
}

@media (max-width: 560px) {
  .tiles {
    gap: 6px;
  }

  .foot {
    padding: 18px 16px;
  }

  .tally {
    font-size: 20px;
  }
}
</style>
