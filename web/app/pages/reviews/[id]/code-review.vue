<script setup lang="ts">
import type {
  DecideFindingRequest,
  Finding,
  ReviewDetail,
} from '~/types/desk'
import type { DeskNotice } from '~/utils/format'
import type { FindingList } from '~/utils/findings'
import type { TrackOption } from '~/components/DecisionTrack.vue'

/**
 * Step 5 — **Code review**. Board A8, which Patrick approved on 10 October 2026.
 *
 * ```
 *  1 blocker. 6 should change, 3 questions, 4 small points.
 *  Step 5 has finished. 6 of 14 findings were reproduced by running something.
 *
 *  WHERE THEY SIT IN WHAT WAS BUILT · click a part to see only its findings
 *  [ Everything ] [ The export context ] [ The scheduled… ] [ Outside the parts ]
 *     14 findings     1 to post · 1 not      4 to post
 *
 *  ( To post · 12 )  For a ticket · 1   Set aside · 1       Most serious first.
 *  ──────────────────────────────────────────────────────────────────────────────
 *  BLOCKER   F9 The column policy is skipped entirely…   Reproduced   Allowed…  Open
 *            app/Reporting/ColumnPolicy.php:52
 *  SHOULD    F1 An empty allowed-columns list means…     Reproduced   The exp…  Open
 *  ──────────────────────────────────────────────────────────────────────────────
 *  11 to post, 2 for a ticket, 1 set aside        [ Go to the draft → ]
 * ```
 *
 * This is the page the whole issue is about. The one it replaces printed nineteen findings
 * and their reasoning as paragraphs, up to 326 words each, and Patrick called it "a bit of a
 * fucking dumpster fire". Of this board: "I really like each finding. I like the way it's
 * formatted… I love the code with the comments, the inline style."
 *
 * ## Containing it
 *
 * He named the risk in the same breath — **"I think you're going to have trouble trying to
 * contain that"** — and containing it is half the issue. Four rules, and each is one of
 * DEV-878's acceptance criteria rather than a preference:
 *
 * 1. **A closed row is one line.** Its severity, its number and title, where it is, how it
 *    was checked, which part of the change it is in. Nothing else. A finding with no title
 *    shows its claim clamped to two lines — never the whole of it, which on this review runs
 *    to 56 words.
 * 2. **One finding is open at a time.** `open` is a number, not a set.
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

/** Which finding is open, by its number. `0` for none — one at a time, as on the board. */
const open = ref(0)

/** Whether the open finding's reasoning is unfolded. Reset whenever another row opens. */
const reasoning = ref(false)

const working = ref(false)
const notice = ref<DeskNotice | null>(null)

function show(seq: number) {
  open.value = open.value === seq ? 0 : seq
  reasoning.value = false
}

/*
 * Changing the list or the part closes whatever was open. The row is gone from the screen
 * either way, and leaving `open` pointing at it means the next list draws a row expanded for
 * no reason a reader could see.
 */
watch([list, part], () => {
  open.value = 0
  reasoning.value = false
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

const shown = computed<Finding | null>(
  () => rows.value.find(finding => finding.seq === open.value) ?? null,
)

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
  return data.outcomes.filter(
    outcome => outcome.finding_seq === finding.seq && outcome.actor !== 'developer',
  )
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
                part === 0
                  ? `Nothing in this list. The review's findings are all in the other two.`
                  : 'Nothing here for this part.'
              }}
            </div>

            <ul
              v-else
              class="rows"
            >
              <li
                v-for="finding in rows"
                :key="finding.seq"
                class="row"
                :class="{ 'row--open': open === finding.seq }"
              >
                <!--
                  **`label` is the key alone, because everything else in the row is
                  already read out.** `OpenRow`'s label exists for a row whose visible
                  content is not a name on its own — "By name" on the Understand page.
                  This row's content is the severity, the number, the title and the place,
                  all of it visible, so a label repeating the title made a screen reader
                  say it twice. Found by an end-to-end assertion matching the same string
                  in two places on one row.
                -->
                <OpenRow
                  :open="open === finding.seq"
                  :controls="`finding-${finding.seq}`"
                  opens="Open"
                  :label="finding.key"
                  class="face"
                  @toggle="show(finding.seq)"
                >
                  <SeverityTag :severity="finding.severity" />

                  <span class="what">
                    <!--
                      The row's one line. A finding with no title shows its claim clamped to
                      two lines — the criterion's own wording — because the claim is a
                      paragraph and a row is a line. `title` attribute so the whole of it is
                      still reachable with a pointer, and the open row shows all of it.
                    -->
                    <span
                      class="title"
                      :class="{ 'title--clamped': finding.title == null }"
                      :title="finding.title == null ? finding.claim : undefined"
                    >
                      <span class="key rd-mono">{{ finding.key }}</span>
                      {{ finding.title ?? finding.claim }}
                    </span>
                    <span class="where rd-mono">{{ whereOf(finding) }}</span>
                  </span>

                  <span class="checked">
                    <span
                      v-if="verification(finding)"
                      class="method"
                      :class="{ 'method--ran': finding.verification === 'reproduced' }"
                    >
                      <span
                        class="dot"
                        aria-hidden="true"
                      />
                      {{ verification(finding) }}
                    </span>
                  </span>

                  <span class="in-part">
                    {{ partName(finding) }}
                    <!--
                      A carried finding's round status, beside the part. DEV-878: "A carried
                      finding from an earlier round shows its round status."
                    -->
                    <DeskPill
                      v-if="roundStatusLabel(finding)"
                      :tone="isStillOpen(finding.round_status) ? 'info' : 'quiet'"
                    >
                      {{ roundStatusLabel(finding) }}
                    </DeskPill>
                  </span>
                </OpenRow>

                <div
                  v-if="open === finding.seq"
                  :id="`finding-${finding.seq}`"
                  class="detail"
                >
                  <h3 class="rd-display-card claim">
                    {{ finding.title ?? finding.claim }}
                  </h3>

                  <p
                    v-if="isCarried(finding) && finding.continues_round"
                    class="rd-note carried"
                  >
                    Raised in round {{ finding.continues_round }} and carried into this one.
                    <template v-if="finding.rounds_open">
                      Open for {{ count(finding.rounds_open, 'round') }}.
                    </template>
                    <template v-if="finding.round_status_checked">
                      <strong>Checked:</strong> {{ finding.round_status_checked }}
                    </template>
                  </p>

                  <div class="columns">
                    <div class="said">
                      <div class="field">
                        <span class="rd-rubric">What</span>
                        <p class="rd-recorded said-text">
                          {{ finding.claim }}
                        </p>
                      </div>

                      <div
                        v-if="finding.why_it_matters"
                        class="matters"
                      >
                        <span class="rd-rubric">Why it matters</span>
                        <p class="rd-recorded matters-text">
                          {{ finding.why_it_matters }}
                        </p>
                      </div>

                      <div
                        v-if="finding.suggested_fix"
                        class="field"
                      >
                        <span class="rd-rubric">Suggested fix</span>
                        <p class="rd-recorded">
                          {{ finding.suggested_fix }}
                        </p>
                        <ul
                          v-if="(finding.fix_references ?? []).length"
                          class="refs"
                        >
                          <li
                            v-for="reference in finding.fix_references ?? []"
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
                        v-if="finding.disposition_reason && list !== 'post'"
                        class="field"
                      >
                        <span class="rd-rubric">
                          Why the session {{ list === 'ticket' ? 'held it back' : 'set it aside' }}
                        </span>
                        <p class="rd-recorded">
                          {{ finding.disposition_reason }}
                        </p>
                      </div>

                      <!--
                        What the author said about this finding, harvested after posting.
                        See `saidByOthers`: the screen this page retires was the only thing
                        drawing these, and step 7 has no page of its own.
                      -->
                      <div
                        v-if="saidByOthers(data, finding).length"
                        class="field"
                      >
                        <span class="rd-rubric">What came back</span>
                        <p
                          v-for="outcome in saidByOthers(data, finding)"
                          :key="outcome.id"
                          class="rd-recorded reply"
                        >
                          <span class="reply-kind">{{ editVerb(outcome.kind) }}</span>
                          <span class="rd-note">
                            by the {{ outcome.actor }} · {{ sinceLabel(outcome.occurred_at) }}
                          </span>
                          <template v-if="outcome.detail">
                            <br>{{ outcome.detail }}
                          </template>
                        </p>
                      </div>

                      <p
                        v-if="finding.rule_key"
                        class="rd-note under"
                      >
                        Raised under rule {{ finding.rule_key }}.
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
                            finding.hunk
                              ? 'The code, and the comment as it will post'
                              : 'The code'
                          }}
                        </span>
                        <CodeHunk
                          v-if="finding.hunk"
                          :hunk="finding.hunk"
                          :flagged-line="finding.flagged_line"
                          :comment="finding.inline_comment"
                          :where="`${finding.hunk.file}${finding.flagged_line == null ? '' : `:${finding.flagged_line}`}`"
                        />
                        <p
                          v-else
                          class="rd-recorded no-code"
                        >
                          {{ whyNoCode(finding, data.brief) }}
                        </p>
                      </div>

                      <!--
                        The comment on a finding with no hunk at all. There is no line to put
                        it under, and dropping it would lose the one thing the author will
                        actually read.
                      -->
                      <div
                        v-if="finding.inline_comment && !finding.hunk"
                        class="field"
                      >
                        <span class="rd-rubric">The comment as it will post</span>
                        <p class="rd-recorded">
                          {{ finding.inline_comment }}
                        </p>
                      </div>

                      <div
                        v-if="finding.proof"
                        class="field"
                      >
                        <span class="rd-rubric">
                          Proof<template v-if="verification(finding)"> · {{ verification(finding) }}</template>
                        </span>
                        <pre class="proof">{{ finding.proof }}</pre>
                        <p
                          v-if="finding.proof_note"
                          class="rd-note proof-note"
                        >
                          {{ finding.proof_note }}
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
                      :aria-controls="`reasoning-${finding.seq}`"
                      @click="reasoning = !reasoning"
                    >
                      {{ foldLabel }}
                    </DeskButton>
                    <div
                      v-if="reasoning"
                      :id="`reasoning-${finding.seq}`"
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
                          {{ block.text }}
                        </p>
                      </div>
                    </div>
                  </div>

                  <FindingCall
                    :finding="finding"
                    :sections="draft.data.value?.sections ?? null"
                    :draft-status="data.draft_status ?? null"
                    :working="working"
                    @decide="request => decide(finding.seq, request)"
                  />
                </div>
              </li>
            </ul>
          </section>

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

        <!--
          What step 5 did not look at. Not on board A8, and the only place it was drawn was
          the one-page review this replaces — it is step 5's own statement about its own
          coverage, and the sentence the posted review carries.

          **Outside the findings branch, deliberately.** It sat inside it and so vanished
          from the one review that needs it most: a step 5 that finished and found nothing
          says "we looked at X and not at Y", and the page was dropping the second half
          exactly when it was the whole of the answer. Found by the self-review.
        -->
        <section
          v-if="coverage(data)"
          class="not-checked"
        >
          <h2 class="rd-rubric">
            What the review did not look at
          </h2>
          <p class="rd-recorded">
            {{ coverage(data) }}
          </p>
        </section>
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

/* -------------------------------- the rows ------------------------------------- */

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

.rows {
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--rd-ink);
  min-width: 0;
}

.row {
  border-bottom: 1px solid var(--rd-line);
  min-width: 0;
}

.row--open {
  background: var(--rd-ground);
  border-radius: var(--rd-radius);
}

/*
 * The board's five columns: severity, the finding, how it was checked, its part, and the
 * control `OpenRow` draws itself. `minmax(0, 1fr)` on the second so a long title shrinks it
 * rather than widening the row — which is the one declaration standing between this page and
 * a horizontal scrollbar.
 */
.face {
  grid-template-columns: 104px minmax(0, 1fr) 128px 170px auto;
  border-top: 0;
  background: transparent;
  padding: 12px 16px;
}

.row--open .face {
  background: transparent;
}

.what {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.title {
  font-size: 16px;
  line-height: 1.3;
}

/*
 * **A claim standing in for a title is clamped to exactly two lines.** The criterion says
 * "never the whole of it": a claim on this review runs to 56 words, which is five lines at
 * this width and a row that is no longer a row.
 */
.title--clamped {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.key {
  color: var(--rd-ink-faint);
}

.where {
  font-size: 12.5px;
  color: var(--rd-ink-muted);
  overflow-wrap: anywhere;
}

.checked {
  font-size: 13px;
  color: var(--rd-ink-muted);
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
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 13px;
  color: var(--rd-ink-muted);
  min-width: 0;
  overflow-wrap: anywhere;
}

/* ------------------------------ the open finding ------------------------------- */

.detail {
  display: flex;
  flex-direction: column;
  gap: 22px;
  padding: 6px 16px 24px;
  min-width: 0;
}

.claim {
  margin: 0;
  max-width: 820px;
}

.carried {
  margin: 0;
  max-width: var(--rd-measure-recorded);
}

.carried strong {
  font-weight: 600;
}

/*
 * Two columns at a laptop and one below 900px of available width. `min(460px, 100%)` rather
 * than a breakpoint, because what decides this is how wide the row is, not how wide the
 * window is — and a code block is the thing that suffers most from being squeezed.
 */
.columns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(460px, 100%), 1fr));
  gap: 28px 36px;
  align-items: start;
  min-width: 0;
}

.said,
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

.ref {
  font-size: 12.5px;
  color: var(--rd-ink-muted);
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
 * At a phone's width the five columns become a stack, which is the three step pages' own
 * answer to the same problem: a board has one width and an app has two. The severity tag
 * and the verification sit on one line, because they are both one short phrase.
 */
@media (max-width: 760px) {
  .face {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px 12px;
  }

  .what,
  .checked,
  .in-part {
    grid-column: 1 / -1;
  }

  .detail {
    padding: 6px 12px 20px;
  }

  .columns {
    gap: 22px;
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
