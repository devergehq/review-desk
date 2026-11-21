<script setup lang="ts">
/**
 * `/reviews/{id}/replies` — **Replies: what came back, in three groups.** DEV-1330, S09.
 *
 * ```
 *  The pull request is still open.
 *  Last read 4 days ago. 14 findings were posted. 7 of them had something come back…
 *
 *  NEEDS YOUR ANSWER 1
 *  [should]  F3  A deleted tenant's schedule still runs                       Contested
 *            dana-okoye · 2 hours ago
 *            “Tenants are never hard-deleted in production…”
 *            [R Raise it again] [A Accept their reason] [W Withdraw]   — or why not
 *            › What came back on F3
 *  SETTLED 2
 *  [blocker] F9  The column policy is skipped…                                Fixed
 *  NO REPLY YET 9
 *  [should]  F1  An empty allowed-columns list means every column        Open
 *  … the first four
 *  › 5 more with no reply
 *
 *  THE ROUNDS OF THIS PULL REQUEST
 *  Round 1   11 findings · 3 replies · 2 threads resolved   nothing decided about any of them
 *  Round 2   14 findings · 2 replies · 1 thread resolved   2 fixed, 1 still open, 1 their reason accepted
 * ```
 *
 * It was step 7, *What happened* (DEV-958), and `/what-happened` still redirects here.
 * `utils/replies.ts` carries the argument for the groups; what is here is the drawing.
 *
 * ## Nothing step 7 drew is gone — it is folded
 *
 * `docs/design.md`: a page may fold or reorder what it draws, and never remove any of it.
 * Step 7 drew, for every posted finding, every outcome oldest first with the author's words
 * whole, where the finding is, the round it was carried from, and its two links — or, for
 * a finding nothing came back on, which of three reasons that is. Each row here folds
 * exactly that (`ReplyHistory`, from `whatHappened`'s own `rowFor`). The headline and the
 * line under it, the no-harvest notice, the outcomes on findings no group lists, the rounds
 * table and the footnotes are drawn as step 7 drew them.
 *
 * ## The keys are this tab's alone
 *
 * R, A and W are bound by `DecisionTrack` on **one** row at a time — the row being answered,
 * the first that can be on arrival — so a key never answers two findings at once. They are
 * bound only while this page is mounted, so they never meet the list's J/K/Enter or the
 * Findings keys, and `keyIsForThePage` keeps them out of the reason field.
 *
 * ## Arriving at one reply
 *
 * `?finding=F3` — `replyPath`, which the header's *Answer F3* goes to, read by the same
 * `findingAsked` Findings reads it with. A contested reply is selected (its buttons carry the
 * keys); a settled or quiet one has its history opened. Either way it is scrolled into view.
 *
 * ## One read
 *
 * `GET /reviews/{id}` is the whole of it. A decision is `POST …/findings/{seq}/decide`, the
 * same write Findings sends, through the same composable.
 */
import type { DecideFindingRequest } from '~/types/desk'
import type { ReplyRow } from '~/utils/replies'
import type { TrackOption } from '~/components/DecisionTrack.vue'

const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

/**
 * The whole page, worked out once — the clock read once per render, so the counts and the
 * rows cannot disagree about "just now".
 */
const page = computed(() => {
  const data = detail.data.value
  return data == null ? null : { step7: whatHappened(data), tab: repliesTab(data) }
})

const headline = computed(() => page.value?.step7.headline ?? '')
/**
 * **What came back, counted the way the line above it counts** — DEV-1344 (#88), which made
 * step 7's section note one count with the sub-line so a code change nobody could know is
 * said as "1 not known" rather than counted as something coming back. Kept on this tab as the
 * line over the groups, since the section it was the note of is now three groups.
 */
const cameBackNote = computed(() => page.value?.step7.note ?? 'nothing yet')
const sub = computed(() => page.value?.step7.sub ?? '')
const harvested = computed(() => page.value?.step7.harvested ?? false)
const nothingLooked = computed(() => page.value?.step7.nothingLooked ?? false)
const rounds = computed(() => page.value?.step7.rounds ?? [])
const anyObserved = computed(() => page.value?.step7.anyObserved ?? false)
const anyWithoutComment = computed(() => page.value?.step7.anyWithoutComment ?? false)
const unaddressable = computed(() => page.value?.step7.unaddressable ?? 0)
const noPullRequestAddress = computed(() => page.value?.step7.noPullRequestAddress ?? false)
const posted = computed(() => (page.value?.step7.answered.length ?? 0) + (page.value?.step7.silent.length ?? 0))

const needsAnswer = computed(() => page.value?.tab.needsAnswer ?? [])
const settled = computed(() => page.value?.tab.settled ?? [])
const noReply = computed(() => page.value?.tab.noReply ?? [])
const elsewhere = computed(() => page.value?.tab.elsewhere ?? 0)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `Replies · #${review.pr} · Review Desk` : 'Replies · Review Desk'
  }),
})

/* ------------------------------ What is open ------------------------------ */

/** Which row's history is unfolded, by sequence number. One at a time. */
const open = ref<number | null>(null)

/** Whether the no-reply rows past the first four are drawn. Closed on arrival. */
const showMore = ref(false)

function toggle(seq: number) {
  open.value = open.value === seq ? null : seq
}

/** Fold the rest, and let go of a row open inside it, so reopening does not bring it back. */
function toggleMore() {
  showMore.value = !showMore.value
  if (!showMore.value && noReply.value.slice(NO_REPLY_SHOWN).some(row => row.seq === open.value)) {
    open.value = null
  }
}

const shownNoReply = computed(() =>
  showMore.value ? noReply.value : noReply.value.slice(0, NO_REPLY_SHOWN))
const moreNoReply = computed(() => Math.max(0, noReply.value.length - NO_REPLY_SHOWN))

function historyId(seq: number): string {
  return `history-${seq}`
}

/* ------------------------------ The answers ------------------------------- */

/**
 * The row R, A and W answer: the one last pressed or focused, else the first that can be
 * answered. Only that row's buttons carry keys, so a keystroke answers one finding.
 */
const chosen = ref<number | null>(null)
const keyed = computed(() => {
  const offered = needsAnswer.value.filter(row => row.answer?.offered)
  return offered.find(row => row.seq === chosen.value)?.seq ?? offered[0]?.seq ?? null
})

const ANSWERS: Array<{ value: string, label: string, key: string }> = [
  { value: 'raise_again', label: 'Raise it again', key: 'R' },
  { value: 'accept_reason', label: 'Accept their reason', key: 'A' },
  { value: 'withdraw', label: 'Withdraw', key: 'W' },
]

function answersFor(row: ReplyRow): TrackOption[] {
  return ANSWERS.map(one => ({
    value: one.value,
    label: one.label,
    key: row.seq === keyed.value ? one.key : undefined,
  }))
}

/** The row whose reason is being asked for, and the reason as typed. */
const asking = ref<number | null>(null)
const note = ref('')
const problem = ref('')
const working = ref(false)
const notice = ref<DeskNotice | null>(null)

function pick(row: ReplyRow, decision: string) {
  chosen.value = row.seq
  problem.value = ''
  if (decision === 'accept_reason') {
    /*
     * **Opened on the author's own words**, because that is what the record keeps as the
     * reason: "the author's reason, in their words". The developer can cut it down.
     */
    asking.value = row.seq
    note.value = row.said?.quote ?? ''
    return
  }
  void decide(row, { decision: decision as DecideFindingRequest['decision'] })
}

function acceptReason(row: ReplyRow) {
  if (note.value.trim().length === 0) {
    problem.value = 'Say what the author’s reason was. It is what the record keeps.'
    return
  }
  void decide(row, { decision: 'accept_reason', note: note.value.trim() })
}

async function decide(row: ReplyRow, request: DecideFindingRequest) {
  working.value = true
  const result = await desk.decideFinding(reviewId.value, row.seq, request)
  working.value = false
  notice.value = noticeFor(result)
  if (result.ok) {
    asking.value = null
    note.value = ''
  }
}

/* ------------------------------ Arriving at one ------------------------------ */

/** The finding the address names, if this tab draws a row for it. */
const asked = computed(() => findingAsked(route.query, route.hash))

/**
 * Select the reply the address names: a contested one takes the keys, any other has its
 * history opened — the rest of the quiet group unfolded first if it is past the first four —
 * and the row is scrolled into view. Run when the rows arrive and when the query changes.
 */
async function arrive() {
  const seq = asked.value
  if (seq == null || page.value == null) return
  const { needsAnswer: needs, settled: done, noReply: quiet } = page.value.tab
  const row = [...needs, ...done, ...quiet].find(one => one.seq === seq)
  if (!row) return
  if (needs.includes(row)) chosen.value = seq
  else open.value = seq
  if (quiet.slice(NO_REPLY_SHOWN).includes(row)) showMore.value = true
  await nextTick()
  if (import.meta.client) document.getElementById(`reply-${row.key}`)?.scrollIntoView?.({ block: 'center' })
}

watch([asked, () => page.value != null], () => void arrive(), { immediate: true })

/** Another review on the same page starts folded, with nothing open or half-answered. */
watch(reviewId, () => {
  open.value = null
  showMore.value = false
  chosen.value = null
  asking.value = null
  note.value = ''
  problem.value = ''
  notice.value = null
})

/** The row's name for a screen reader, with the recorded prose spoken as `TickedText` draws it. */
function said(row: ReplyRow): string {
  return `${row.key}, ${row.severity}, ${spoken(row.line)}`
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
        step="harvest"
        tab="replies"
        :headline="headline"
        :sub="sub"
      >
        <!--
          **`nothingLooked`, not `!harvested`.** A harvest writes the pull request's state
          first and its read time last, so a run that fails part way leaves a state and no
          time — and this notice then sat under a headline only a harvest could write.
        -->
        <DeskNotice
          v-if="nothingLooked"
          tone="plain"
        >
          <strong>No harvest has read this review back from GitHub.</strong>
          Whatever the author has said or pushed since it was posted is on the pull request
          and not yet on this record, so the rows below are what Review Desk holds rather
          than what is there — and anything in them was recorded by hand.
        </DeskNotice>

        <DeskNotice
          v-if="notice"
          :tone="notice.tone"
        >
          {{ notice.text }}
        </DeskNotice>

        <p class="rd-meta came-back">
          What came back · {{ cameBackNote }}
        </p>

        <!-- --------------------------- Needs your answer --------------------------- -->
        <section
          class="group"
          aria-labelledby="needs-answer"
        >
          <h2
            id="needs-answer"
            class="rd-label group-head"
          >
            Needs your answer <span class="group-count">{{ needsAnswer.length }}</span>
          </h2>
          <p
            v-if="needsAnswer.length === 0"
            class="rd-note none"
          >
            Nothing waits on your answer: no reply from the author is the newest thing on any
            finding.
          </p>
          <ul
            v-else
            class="rows"
          >
            <li
              v-for="row in needsAnswer"
              :id="`reply-${row.key}`"
              :key="row.seq"
              class="reply-row reply-row--contested"
              :class="{ 'reply-row--picked': row.seq === keyed || (keyed == null && row.seq === chosen) }"
              :aria-current="row.seq === chosen ? 'true' : undefined"
              @focusin="chosen = row.seq"
            >
              <div class="head">
                <SeverityTag :severity="row.severity" />
                <span class="rd-mono key">{{ row.key }}</span>
                <span class="line">
                  <TickedText :text="row.line" />{{ ' ' }}
                  <span
                    v-if="row.shortened"
                    class="rd-meta shortened"
                  >shortened</span>
                </span>
                <span class="standing standing--contested">{{ STANDING_WORDS.contested.word }}</span>
              </div>
              <div
                v-if="row.said"
                class="said"
              >
                <!--
                  **Quoted and attributed, never paraphrased.** The login the record holds —
                  `outcomeSaid` — and their words whole, interpolated rather than `v-html`.
                -->
                <p class="rd-meta who">
                  <template v-if="row.said.who">
                    {{ row.said.who }} ·
                  </template>{{ row.said.when }}
                </p>
                <blockquote
                  v-if="row.said.quote"
                  class="rd-recorded quote"
                >
                  {{ row.said.quote }}
                </blockquote>
                <p
                  v-else-if="row.said.sentence"
                  class="told"
                >
                  {{ row.said.sentence }}
                </p>
              </div>

              <div
                v-if="row.answer?.offered"
                class="answers"
              >
                <DecisionTrack
                  mode="act"
                  :options="answersFor(row)"
                  :label="`How to answer the author on ${row.key}`"
                  :disabled="working"
                  @act="value => pick(row, value)"
                />
                <div
                  v-if="asking === row.seq"
                  class="asking"
                >
                  <label
                    class="rd-label"
                    :for="`reason-${row.seq}`"
                  >The author’s reason, in their words</label>
                  <textarea
                    :id="`reason-${row.seq}`"
                    v-model="note"
                    class="rd-field"
                    rows="3"
                  />
                  <div class="asking-actions">
                    <DeskButton
                      kind="primary"
                      :working="working"
                      working-label="Recording"
                      @click="acceptReason(row)"
                    >
                      Accept it
                    </DeskButton>
                    <DeskButton @click="asking = null">
                      Cancel
                    </DeskButton>
                  </div>
                  <p
                    v-if="problem"
                    class="rd-note problem"
                  >
                    {{ problem }}
                  </p>
                </div>
              </div>
              <!--
                **Why not, where the record will not take them** — a sentence rather than
                three buttons that do nothing. The reason is the record's (`answerFor`).
              -->
              <p
                v-else-if="row.answer"
                class="rd-note why"
              >
                {{ row.answer.why }}
              </p>

              <button
                type="button"
                class="rd-fold-line"
                :aria-expanded="open === row.seq"
                :aria-controls="historyId(row.seq)"
                @click="toggle(row.seq)"
              >
                {{ open === row.seq ? 'Fold' : 'Everything' }} that came back on {{ row.key }}
              </button>
              <ReplyHistory
                v-if="open === row.seq"
                :id="historyId(row.seq)"
                :row="row.history"
              />
            </li>
          </ul>
        </section>

        <!-- -------------------------------- Settled -------------------------------- -->
        <section
          class="group"
          aria-labelledby="settled"
        >
          <h2
            id="settled"
            class="rd-label group-head"
          >
            Settled <span class="group-count">{{ settled.length }}</span>
          </h2>
          <p
            v-if="settled.length === 0"
            class="rd-note none"
          >
            Nothing is settled yet: no thread has been resolved and no reply has been followed
            by anything.
          </p>
          <ul
            v-else
            class="rows"
          >
            <li
              v-for="row in settled"
              :id="`reply-${row.key}`"
              :key="row.seq"
              class="reply-row reply-row--settled"
            >
              <div class="head">
                <SeverityTag :severity="row.severity" />
                <span class="rd-mono key">{{ row.key }}</span>
                <span class="line">
                  <TickedText :text="row.line" />{{ ' ' }}
                  <span
                    v-if="row.shortened"
                    class="rd-meta shortened"
                  >shortened</span>
                </span>
                <span
                  class="standing"
                  :class="`standing--${row.standing}`"
                  :title="STANDING_WORDS[row.standing!].basis"
                >{{ STANDING_WORDS[row.standing!].word }}</span>
              </div>
              <div
                v-if="row.said"
                class="said"
              >
                <p class="rd-meta who">
                  <template v-if="row.said.who">
                    {{ row.said.who }} ·
                  </template>{{ row.said.when }} · {{ STANDING_WORDS[row.standing!].basis }}
                </p>
                <blockquote
                  v-if="row.said.quote"
                  class="rd-recorded quote"
                >
                  {{ row.said.quote }}
                </blockquote>
                <p
                  v-else-if="row.said.sentence"
                  class="told"
                >
                  {{ row.said.sentence }}
                </p>
              </div>
              <button
                type="button"
                class="rd-fold-line"
                :aria-expanded="open === row.seq"
                :aria-controls="historyId(row.seq)"
                @click="toggle(row.seq)"
              >
                {{ open === row.seq ? 'Fold' : 'Everything' }} that came back on {{ row.key }}
              </button>
              <ReplyHistory
                v-if="open === row.seq"
                :id="historyId(row.seq)"
                :row="row.history"
              />
            </li>
          </ul>
        </section>

        <!-- ------------------------------ No reply yet ----------------------------- -->
        <section
          v-if="noReply.length > 0"
          class="group"
          aria-labelledby="no-reply"
        >
          <h2
            id="no-reply"
            class="rd-label group-head"
          >
            No reply yet <span class="group-count">{{ noReply.length }}</span>
          </h2>
          <p
            v-if="nothingLooked"
            class="rd-note none"
          >
            Nothing has looked, so this is an absence of a reading rather than a reading.
          </p>
          <div class="quiet-rows">
            <template
              v-for="row in shownNoReply"
              :key="row.seq"
            >
              <OpenRow
                :id="`reply-${row.key}`"
                :open="open === row.seq"
                :controls="historyId(row.seq)"
                opens="Open"
                :label="said(row)"
                class="quiet-row"
                @toggle="toggle(row.seq)"
              >
                <span class="cell"><SeverityTag :severity="row.severity" /></span>{{ ' ' }}
                <span class="cell rd-mono key">{{ row.key }}</span>{{ ' ' }}
                <span class="cell cell--line">
                  <TickedText
                    :text="row.line"
                    in-control
                  />{{ ' ' }}
                  <span
                    v-if="row.shortened"
                    class="rd-meta shortened"
                  >shortened</span>
                </span>
              </OpenRow>
              <ReplyHistory
                v-if="open === row.seq"
                :id="historyId(row.seq)"
                :row="row.history"
              />
            </template>
            <span class="end" />
          </div>
          <button
            v-if="moreNoReply > 0"
            type="button"
            class="rd-fold-line more"
            :aria-expanded="showMore"
            @click="toggleMore()"
          >
            {{ showMore ? `Fold the ${moreNoReply} more` : `${moreNoReply} more with no reply` }}
          </button>
        </section>
        <p
          v-else-if="posted === 0 && needsAnswer.length + settled.length === 0"
          class="rd-note none"
        >
          This review posted no findings, so nothing could come back on one.
        </p>
        <p
          v-else-if="harvested"
          class="rd-note none"
        >
          Every posted finding has had something come back from the author.
        </p>

        <!--
          **What the record holds about findings no group draws.** The harvester writes
          against any finding of the review, so a held-back nit can carry an outcome. Counted
          here, and sent to the tab that owns the findings, so a measurement is not lost.
        -->
        <DeskSection
          v-if="elsewhere > 0"
          heading="Outcomes on findings this tab does not list"
          :note="count(elsewhere, 'outcome')"
        >
          <p class="rd-note none">
            Against findings this review did not post and the author has not answered — a nit
            held back for a ticket, one set aside, or one carried into this round already
            fixed.
            <NuxtLink :to="`/reviews/${reviewId}/findings`">
              Findings
            </NuxtLink>
            lists every finding this round holds.
          </p>
        </DeskSection>

        <!-- ------------------------------- The rounds -------------------------------- -->
        <DeskSection
          v-if="rounds.length > 0"
          heading="The rounds of this pull request"
          :note="`${rounds.length} rounds`"
        >
          <DeskTable
            :columns="['Round', 'What it held', 'How they stood']"
            template="minmax(0, 1fr) minmax(0, 3fr) minmax(0, 2fr)"
            label="Every round of this pull request, what each held, and how its findings stood"
          >
            <tr
              v-for="line in rounds"
              :key="line.round"
              role="row"
              class="row"
            >
              <td
                role="cell"
                class="td td--strong"
                data-label="Round"
              >
                Round {{ line.round }}
                <span
                  v-if="line.here"
                  class="rd-meta"
                >· this one</span>
              </td>
              <td
                role="cell"
                class="td"
                data-label="What it held"
              >
                {{ line.said }}
              </td>
              <td
                role="cell"
                class="td"
                data-label="How they stood"
              >
                <template v-if="line.stood">
                  {{ line.stood }}
                </template>
                <span
                  v-else
                  class="rd-meta"
                >nothing decided about any of them</span>
              </td>
            </tr>
          </DeskTable>
        </DeskSection>

        <!-- ------------------------------- The footnotes ------------------------------ -->
        <footer
          v-if="anyObserved || anyWithoutComment || unaddressable > 0 || noPullRequestAddress"
          class="notes"
        >
          <p
            v-if="anyObserved"
            class="rd-meta"
          >
            <strong>“Seen” rather than a time.</strong>
            GitHub does not record when a review thread was resolved, so the time against a
            resolution above is when the harvest looked. It may have happened well before.
          </p>
          <p
            v-if="anyWithoutComment"
            class="rd-meta"
          >
            <strong>A finding raised in the body alone.</strong>
            A finding this review did not post as its own comment has no thread, so no
            reply could be read on it and nothing here is a link to one.
            <template v-if="!noPullRequestAddress">
              The pull request itself is linked at the top of this page.
            </template>
          </p>
          <p
            v-if="noPullRequestAddress"
            class="rd-meta"
          >
            <strong>No link to GitHub.</strong>
            This review's repository is recorded under a name no GitHub address can be
            built from, and every address on this page that leads to GitHub is built on top
            of the pull request's.
            <template v-if="posted > 0">
              The links to each finding on the Findings tab are unaffected.
            </template>
            Nothing is missing from the record; what is missing is somewhere on GitHub to
            point at.
          </p>
          <p
            v-if="unaddressable > 0"
            class="rd-meta"
          >
            <strong>{{ count(unaddressable, 'comment') }} with no address.</strong>
            A comment is linked where the record holds GitHub's own numeric comment id for
            it. {{ unaddressable === 1 ? 'One of these findings was' : 'These findings were' }}
            posted as a comment under an id of another kind, which names no place on the
            pull request — so the comment exists and this page does not link it rather than
            linking somewhere else.
          </p>
        </footer>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
/* ------------------------------- A group ------------------------------- */

.came-back {
  margin: 0;
}

.group {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/* S09's group heading: the label, its count beside it in the quieter ink. */
.group-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin: 0 0 8px;
}

.group-count {
  color: var(--rd-ink-dim);
  font-variant-numeric: tabular-nums;
}

.rows {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--rd-line);
  min-width: 0;
}

.reply-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--rd-line);
  min-width: 0;
}

/*
 * **The one group that waits on you is drawn as the selected thing on the page** — S09's
 * ruled, washed card. Ink and ground, no colour that grades what the author said.
 */
.reply-row--contested {
  border: 1px solid var(--rd-ink);
  border-left-width: 3px;
  background: var(--rd-ground);
}

/* The reply the keys answer, lifted as the chosen thing is everywhere on the desk. */
.reply-row--picked {
  box-shadow: var(--rd-lift-chosen);
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  min-width: 0;
}

.key {
  flex: none;
  color: var(--rd-ink-dim);
  font-size: var(--rd-type-ui);
}

.line {
  flex: 1 1 22ch;
  min-width: 0;
  font-size: var(--rd-type-lead);
  font-weight: 500;
  overflow-wrap: break-word;
}

.shortened {
  margin-left: 8px;
  font-size: var(--rd-type-meta);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

/* The word for where it stands. S09 washes it; the wash is the state's, not a grade. */
.standing {
  flex: none;
  margin-left: auto;
  padding: 2px 8px;
  border-radius: var(--rd-radius-sm);
  font-size: var(--rd-type-ui);
  font-weight: 500;
}

.standing--contested {
  background: var(--rd-warn-wash);
  color: var(--rd-warn-ink);
}

.standing--fixed {
  background: var(--rd-accent-wash);
  color: var(--rd-accent-dark);
}

.standing--answered {
  background: var(--rd-nit-wash);
  color: var(--rd-nit-ink);
}

.said {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.who {
  margin: 0;
}

/* The author's words: inset so a quotation reads as one, at the body's size. */
.quote {
  margin: 0;
  padding: 0 0 0 12px;
  border-left: 3px solid var(--rd-line-strong);
  font-size: var(--rd-type-body);
}

.told {
  margin: 0;
  font-size: var(--rd-type-body);
}

.answers {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 4px;
  min-width: 0;
}

.asking {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: var(--rd-measure);
}

.asking-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.problem {
  margin: 0;
  color: var(--rd-warn-ink);
}

.why {
  margin: 4px 0 0;
  max-width: var(--rd-measure);
}

/*
 * **The fold's own control**: a line of accent text that clears the 44px floor every
 * pressable thing on this desk clears, with the triangle drawn rather than typed.
 */
.rd-fold-line {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: var(--rd-control-min);
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--rd-accent);
  font: inherit;
  font-size: var(--rd-type-ui);
  cursor: pointer;
}

.rd-fold-line::before {
  content: '';
  width: 0;
  height: 0;
  border-top: 4px solid transparent;
  border-bottom: 4px solid transparent;
  border-left: 6px solid currentColor;
  transition: transform 0.14s ease;
}

.rd-fold-line[aria-expanded='true']::before {
  transform: rotate(90deg);
}

.rd-fold-line:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.more {
  margin-top: 6px;
}

/* ----------------------------- No reply yet ----------------------------- */

.quiet-rows {
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--rd-line);
  min-width: 0;
}

/* `OpenRow` draws the grid; the columns are the caller's. Tag, key, line, and its "Open". */
.quiet-row {
  grid-template-columns: 11ch 4ch minmax(0, 1fr) auto;
}

.cell {
  min-width: 0;
}

.cell--line {
  overflow-wrap: break-word;
}

.end {
  border-top: 1px solid var(--rd-line);
}

.none {
  margin: 0 0 12px;
  max-width: var(--rd-measure);
}

/* ------------------------------ The rounds ------------------------------ */

.td {
  overflow-wrap: break-word;
}

.td--strong {
  font-weight: 500;
}

/* ----------------------------- The footnotes ----------------------------- */

.notes {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 18px;
  border-top: 1px solid var(--rd-line);
  max-width: var(--rd-measure);
}

/* -------------------------------- Narrow -------------------------------- */

@media (max-width: 560px) {
  .reply-row {
    padding: 12px;
  }

  .line {
    flex-basis: 100%;
    order: 3;
  }

  .quiet-row {
    grid-template-columns: 4ch minmax(0, 1fr) auto;
  }

  /* The severity leaves the narrow quiet row; it is in the row's accessible name either way. */
  .quiet-row .cell:first-child {
    display: none;
  }
}
</style>
