<script setup lang="ts">
import type { Brief, ReviewDetail } from '~/types/desk'

/**
 * Step 2 — **Understand**. Board A5, which Patrick approved on 10 October 2026; of its
 * before-and-after picture, "perfect".
 *
 * What it is for, in his words: someone on the project but outside the domain picks a
 * review up cold and understands the problem and the change **before looking at code**. So
 * the page is ordered by how much it costs to read:
 *
 * | Section | What it answers | Cost |
 * |---|---|---|
 * | the headline | what this change does | one sentence |
 * | the two flows | how it worked, how it works now | one picture |
 * | three things were wrong | what was wrong and what fixes each | three lines |
 * | the supporting table | the idea behind the change | one small table |
 * | how it works today | the context a newcomer needs | a paragraph |
 * | already decided | what is settled, and by whom | one line |
 * | as the session recorded it | the raw record | a fold |
 *
 * **Nothing below the picture has to be read to understand the picture.** That is the
 * mechanism behind "it gives you a very clear way… your cognitive load is much lower", and
 * it is why the recorded prose is at the bottom behind a fold rather than at the top: the
 * rabbit holes are there to be gone down when wanted, not to be gone down first.
 *
 * ## Two things this page deliberately does not do
 *
 * **It draws no open-choices table.** DEV-885's comment: the Understand board's "Already
 * decided" fold and the Solution board's "Where the ticket left a choice" table are the
 * same data, `open_choices`, and it is drawn on Solution only. Here it is one line and a
 * link, which is the comment's own instruction and overrides board A5.
 *
 * **It uses no mermaid.** The flows are HTML boxes. A brief written the old way may still
 * carry a `diagram`, and that one is handed to `MermaidFigure` in the fallback below —
 * which is the only place on these three pages a diagram is drawn at all.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `Understand · #${review.pr} · Review Desk` : 'Understand · Review Desk'
  }),
})

/**
 * Which problem a reader has opened, or `null`.
 *
 * One at a time, as on the board, and it drives three places at once: the row's own detail,
 * and the marks on both flows. That is why these rows are `OpenRow` and not `<details>` —
 * a `<summary>` can only open the thing inside it.
 */
const openProblem = ref<number | null>(null)

function toggleProblem(number: number) {
  openProblem.value = openProblem.value === number ? null : number
}

/** Whether the recorded text at the foot of the page is open. */
const recordedOpen = ref(false)

/* ------------------------- What the brief actually has ------------------------- */

/**
 * The structured problem half, read defensively.
 *
 * **Every one of these needs its `?? []`.** The contract serves a brief written the old way
 * with every new array empty and every new string null — but the invented fixtures in
 * sample mode are briefs recorded *before* those fields existed, so at runtime the arrays
 * are `undefined` rather than empty. Typed as present, absent in fact; a page that read
 * `brief.problems.length` would throw on five of the six sample reviews.
 */
function problems(brief: Brief) {
  return brief.problems ?? []
}

function flowBefore(brief: Brief) {
  return brief.flow_before ?? []
}

function flowAfter(brief: Brief) {
  return brief.flow_after ?? []
}

/**
 * Whether this brief was written before the structured one existed.
 *
 * **It carries none of the six things this page draws**, which is a stronger test than
 * "the headline is missing": a brief whose problem half is being written has a headline and
 * no parts, and that is a brief in progress rather than an old one. A brief with nothing at
 * all is the one the fallback is for.
 */
function writtenTheOldWay(brief: Brief): boolean {
  return brief.headline == null
    && brief.how_it_works_today == null
    && problems(brief).length === 0
    && flowBefore(brief).length === 0
    && flowAfter(brief).length === 0
    && brief.support_table == null
}

/**
 * Whether the session has written the problem half and not yet the rest.
 *
 * **A brief arrives in two writes.** DEV-883's session writes the problem half first —
 * `problem`, `how_it_works_today`, the before flow and its caption, and each problem's
 * number and `was_wrong` — hands that to an independent options pass, and writes everything
 * else when the pass reports. For the minutes in between, a brief has a before flow and
 * numbered problems and no headline, no after flow, no `now_fixed`, no options and no
 * parts.
 *
 * That is a **fourth state**, and the page has to name it. It is not an old-way brief:
 * `writtenTheOldWay` is false, because there are problems and a flow. It is not an error
 * and it is not an empty page. Saying so is the difference between a reader thinking the
 * record is broken and knowing it is twenty minutes old.
 */
function stillBeingWritten(brief: Brief): boolean {
  return flowBefore(brief).length > 0 && flowAfter(brief).length === 0
}

/** The headline, or what to say while the sentence that states the answer is unwritten. */
function headline(brief: Brief): string {
  if (brief.headline) return brief.headline
  return stillBeingWritten(brief)
    ? 'The problem is written. What was done about it is not, yet.'
    : 'This brief records no headline sentence.'
}

/* --------------------------------- Already decided ------------------------------ */

/** The option that was built, when the brief names one. */
function chosen(brief: Brief) {
  return (brief.options ?? []).find(option => option.chosen) ?? null
}

/**
 * What is on the Solution page that used to be offered here, as one sentence.
 *
 * Counted rather than asserted, so the line cannot promise a table that is empty.
 */
function whatSolutionHolds(brief: Brief): string | null {
  const choices = (brief.open_choices ?? []).length
  const clashes = (brief.disagreements ?? []).length
  const parts = [
    choices > 0 ? count(choices, 'choice the ticket left open', `choices the ticket left open`) : null,
    clashes > 0 ? `${count(clashes, 'place')} the description and the code disagree` : null,
  ].filter(Boolean)
  return parts.length === 0 ? null : parts.join(', and ')
}

/* ------------------------- The measurement at the foot ------------------------- */

/**
 * How long the recorded text behind the fold is.
 *
 * **Measured on the brief in hand, and only on the record's side.** The board carries a
 * pair of figures — 85 words and a picture above, against 310 words and a 50-line text
 * sketch as the session recorded it — because on the real review it was drawn from the
 * record was four times the page. That is a fact about that review, not about this one:
 * on the contract's invented example the page is the longer of the two, and the pair then
 * reads as a boast that has gone the wrong way.
 *
 * So the page states what is behind the fold and makes no comparison. The comparison was
 * an argument about the design; the figure is a fact about the record, and only the second
 * belongs on a screen that has to be right about every review.
 */
function words(text: string | null | undefined): number {
  if (!text) return 0
  return text.trim().split(/\s+/).filter(Boolean).length
}

function wordsRecorded(brief: Brief): number {
  return words(brief.problem) + words(brief.provenance)
}

/** The lines of the superseded text sketch, when the brief has one. */
function sketchLines(brief: Brief): number | null {
  return brief.diagram == null ? null : brief.diagram.split('\n').length
}

/** The sources the review recorded, which is what "already decided" rests on. */
function sources(data: ReviewDetail) {
  return data.sources ?? []
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <!--
        No brief at all. Step 2 has not run, which is a different thing from a brief
        written the old way and a different thing again from one waiting to be confirmed.
      -->
      <ReviewStepPage
        v-if="!data.brief"
        :detail="data"
        step="understand"
        headline="Nothing has been written about this problem yet."
        sub="Step 2 is where a session writes what the change is solving. It has not written one, so there is nothing to confirm and nothing after it can run."
      />

      <!--
        A brief written before any of this. **It still shows**, which is an acceptance
        criterion: the recorded text, with one line saying what it is. Never a blank page
        and never an error.

        The heading is the pull request's own title rather than the recorded problem.
        There is no `headline` to draw — that is what makes this brief an old one — and
        setting the heading to the problem drew the same paragraph twice on one page, once
        at 40px and once as the record. The record is shown once, below, with the breaks
        the session wrote.
      -->
      <ReviewStepPage
        v-else-if="writtenTheOldWay(data.brief)"
        :detail="data"
        step="understand"
        :headline="data.review.title ?? `Pull request #${data.review.pr}`"
      >
        <BriefConfirmation
          :brief="data.brief"
          :review-id="data.review.id"
        />

        <DeskNotice tone="plain">
          <strong>Written before the structured brief.</strong>
          This brief records the problem as one piece of text, which is what a brief held
          before 10 October 2026. There is no before-and-after picture, no numbered
          problems and no supporting table to draw — so what the session wrote is below, as
          it wrote it.
        </DeskNotice>

        <DeskSection heading="The problem, as recorded">
          <p class="rd-recorded recorded">
            {{ data.brief.problem }}
          </p>
          <!--
            The one `MermaidFigure` on these three pages, and only for a brief old enough
            to have a `diagram`. `MermaidFigure` owns what happens when the field holds
            something that is not a diagram.
          -->
          <MermaidFigure
            v-if="data.brief.diagram"
            :source="data.brief.diagram"
            label="How the problem happens today, as the session drew it"
          />
        </DeskSection>

        <DeskSection
          v-if="data.brief.provenance"
          heading="Where this came from"
        >
          <p class="rd-recorded recorded">
            {{ data.brief.provenance }}
          </p>
        </DeskSection>
      </ReviewStepPage>

      <!-- The structured brief: the page board A5 draws. -->
      <ReviewStepPage
        v-else
        :detail="data"
        step="understand"
        :headline="headline(data.brief)"
      >
        <!--
          **Layer 2's one act, which lives here because the page it lived on is gone.**
          `/reviews/{id}` drew it and DEV-878 retires that screen; without this the
          dashboard would have no way to confirm a brief at all, and confirming is one of
          only two things in the whole system that record the developer's sign-off.
        -->
        <BriefConfirmation
          :brief="data.brief"
          :review-id="data.review.id"
        />

        <!-- ------------------------------ The two flows ------------------------------ -->
        <section
          v-if="flowBefore(data.brief).length || flowAfter(data.brief).length"
          class="flows"
        >
          <div class="pair">
            <DeskCard
              v-if="flowBefore(data.brief).length"
              tone="well"
              pad="section"
              class="side"
            >
              <div class="side-head">
                <h2 class="rd-display-card">
                  Before
                </h2>
                <p
                  v-if="data.brief.flow_before_caption"
                  class="caption"
                >
                  {{ data.brief.flow_before_caption }}
                </p>
              </div>
              <FlowDiagram
                :rows="flowBefore(data.brief)"
                tone="wrong"
                :focus="openProblem"
              />
            </DeskCard>

            <DeskCard
              v-if="flowAfter(data.brief).length"
              tone="well"
              pad="section"
              class="side"
            >
              <div class="side-head">
                <h2 class="rd-display-card">
                  After
                </h2>
                <p
                  v-if="data.brief.flow_after_caption"
                  class="caption"
                >
                  {{ data.brief.flow_after_caption }}
                </p>
              </div>
              <FlowDiagram
                :rows="flowAfter(data.brief)"
                tone="fixed"
                :focus="openProblem"
              />
            </DeskCard>

            <!--
              The after side, during the minutes between the session's two writes. **A
              column that says it is not written yet, not a column that is missing**: the
              picture is a before and an after, and an absent right-hand side reads as a
              page that failed to load rather than as a record that is twenty minutes old.
            -->
            <DeskCard
              v-else-if="flowBefore(data.brief).length"
              tone="pending"
              pad="section"
              class="side"
            >
              <div class="side-head">
                <h2 class="rd-display-card">
                  After
                </h2>
              </div>
              <p class="rd-note pending-note">
                Not written yet. A session writes the problem first and hands it to an
                independent pass before it writes what was done about it, so this side
                arrives second. Nothing is wrong.
              </p>
            </DeskCard>
          </div>

          <!--
            The legend describes two flows, so it is drawn when there are two. Printed over
            a brief with no picture at all it explained marks that were not there.
          -->
          <p
            v-if="flowBefore(data.brief).length && flowAfter(data.brief).length"
            class="rd-note legend"
          >
            Each number is one of the problems below. Red on the left marks what was wrong;
            green on the right, with the same number, marks what fixes it. Open a problem
            to pick its boxes out of both flows.
          </p>
          <p
            v-else-if="flowBefore(data.brief).length"
            class="rd-note legend"
          >
            Each red number is one of the problems below. The green marks that say what
            fixes each one arrive with the after flow.
          </p>
        </section>

        <!-- -------------------------- What was wrong, numbered ------------------------ -->
        <section
          v-if="problems(data.brief).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            {{ count(problems(data.brief).length, 'thing') }}
            {{ problems(data.brief).length === 1 ? 'was' : 'were' }} wrong
          </h2>

          <div class="rows">
            <div class="rows-head">
              <span />
              <span class="rd-rubric">Was wrong</span>
              <span class="rd-rubric">Now</span>
              <span />
            </div>

            <template
              v-for="problem in problems(data.brief)"
              :key="problem.number"
            >
              <OpenRow
                class="problem"
                :open="openProblem === problem.number"
                :controls="`problem-${problem.number}`"
                opens="By name"
                :label="`problem ${problem.number}`"
                @toggle="toggleProblem(problem.number)"
              >
                <ProblemMark
                  :number="problem.number"
                  tone="wrong"
                  size="row"
                  says="the problem"
                />
                <span
                  class="cell"
                  data-label="Was wrong"
                >{{ problem.was_wrong }}</span>
                <!--
                  `now_fixed` is nullable by design: the problem half can be written before
                  the change is described, which is what lets a blind pass be handed a
                  problem without the answer. So the absence is a state with words.
                -->
                <span
                  class="cell"
                  data-label="Now"
                >
                  <template v-if="problem.now_fixed">{{ problem.now_fixed }}</template>
                  <span
                    v-else
                    class="rd-meta"
                  >not written yet</span>
                </span>
              </OpenRow>

              <div
                v-if="openProblem === problem.number"
                :id="`problem-${problem.number}`"
                class="detail"
              >
                <p class="rd-rubric detail-head">
                  {{ problem.number }}
                  <template v-if="problem.detail_title">
                    · {{ problem.detail_title }}
                  </template>
                </p>
                <div class="detail-pair">
                  <div class="detail-side">
                    <h3 class="detail-label detail-label--wrong">
                      Before
                    </h3>
                    <ul
                      v-if="problem.detail_before?.length"
                      class="lines"
                    >
                      <li
                        v-for="(line, index) in problem.detail_before"
                        :key="index"
                      >
                        {{ line }}
                      </li>
                    </ul>
                    <p
                      v-else
                      class="rd-meta"
                    >
                      The brief records no detail for this side.
                    </p>
                  </div>
                  <div class="detail-side">
                    <h3 class="detail-label detail-label--fixed">
                      After
                    </h3>
                    <ul
                      v-if="problem.detail_after?.length"
                      class="lines"
                    >
                      <li
                        v-for="(line, index) in problem.detail_after"
                        :key="index"
                      >
                        {{ line }}
                      </li>
                    </ul>
                    <p
                      v-else
                      class="rd-meta"
                    >
                      The brief records no detail for this side.
                    </p>
                  </div>
                </div>
              </div>
            </template>
            <span class="end" />
          </div>
        </section>

        <!-- ---------------------------- The supporting table -------------------------- -->
        <section
          v-if="data.brief.support_table"
          class="block"
        >
          <h2 class="rd-display-lead">
            {{ data.brief.support_table.title ?? 'The idea behind the change' }}
          </h2>
          <DeskTable
            :columns="data.brief.support_table.columns"
            :template="`repeat(${data.brief.support_table.columns.length}, minmax(0, 1fr))`"
            :label="data.brief.support_table.title ?? 'The idea behind the change'"
          >
            <tr
              v-for="(row, index) in data.brief.support_table.rows"
              :key="index"
              role="row"
              class="row"
            >
              <td
                v-for="(cell, column) in row"
                :key="column"
                role="cell"
                class="td"
                :class="{ 'td--strong': column === 0 }"
                :data-label="data.brief.support_table.columns[column]"
              >
                {{ cell }}
              </td>
            </tr>
          </DeskTable>
          <span class="end" />
        </section>

        <!-- ----------------------------- How it works today --------------------------- -->
        <section
          v-if="data.brief.how_it_works_today"
          class="block"
        >
          <h2 class="rd-display-lead">
            How it works today
          </h2>
          <p class="rd-recorded recorded">
            {{ data.brief.how_it_works_today }}
          </p>
        </section>

        <!-- ------------------------------ Already decided ----------------------------- -->
        <section class="block">
          <h2 class="rd-display-lead">
            Already decided
          </h2>
          <div class="decided">
            <p
              v-if="chosen(data.brief)"
              class="decided-line"
            >
              Option {{ chosen(data.brief)!.key }} of
              {{ (data.brief.options ?? []).length }} —
              {{ chosen(data.brief)!.title }}
            </p>
            <p
              v-else
              class="rd-meta decided-line"
            >
              The brief names no option as the one that was built.
            </p>
            <p
              v-if="data.brief.provenance"
              class="rd-note"
            >
              {{ data.brief.provenance }}
            </p>
            <!--
              The sources, with their labels. A link is an extra way to read one, never the
              only way: DEV-885 forbids a page whose only route to something is GitHub.
            -->
            <ul
              v-if="sources(data).length"
              class="sources"
            >
              <li
                v-for="source in sources(data)"
                :key="source.position"
                class="source"
              >
                <span class="rd-rubric">{{ source.kind }}</span>
                <span>{{ source.label }}</span>
                <a
                  v-if="source.link"
                  :href="source.link"
                  target="_blank"
                  rel="noreferrer noopener"
                >open<span class="rd-visually-hidden">
                  {{ source.label }}, in a new tab</span></a>
              </li>
            </ul>
            <!--
              The one line that replaces board A5's open-choices fold, per DEV-885's
              comment: one list, drawn once, and the table is Solution's.
            -->
            <p
              v-if="whatSolutionHolds(data.brief)"
              class="rd-note"
            >
              {{ sentence(whatSolutionHolds(data.brief)!) }} They are on <NuxtLink
                :to="`/reviews/${data.review.id}/solution`"
              >the Solution page</NuxtLink>, drawn once.
            </p>
          </div>
        </section>

        <!-- -------------------------- As the session recorded it ----------------------- -->
        <DeskCard
          as="section"
          tone="well"
          pad="section"
          class="measured"
        >
          <div class="figures">
            <div class="figure">
              <span class="rd-display-figure faint">
                {{ count(wordsRecorded(data.brief), 'word') }}
              </span>
              <span class="rd-meta">
                <template v-if="sketchLines(data.brief) != null">
                  and a {{ sketchLines(data.brief) }}-line text sketch, as the session
                  recorded it
                </template>
                <template v-else>as the session recorded it</template>
              </span>
            </div>
            <button
              type="button"
              class="rd-btn read-it"
              :aria-expanded="recordedOpen"
              aria-controls="as-recorded"
              @click="recordedOpen = !recordedOpen"
            >
              {{ recordedOpen ? 'Fold' : 'Read it as recorded' }}
            </button>
          </div>

          <div
            v-if="recordedOpen"
            id="as-recorded"
            class="as-recorded"
          >
            <p class="rd-rubric">
              The problem, as recorded
            </p>
            <p class="rd-recorded recorded">
              {{ data.brief.problem }}
            </p>
            <template v-if="data.brief.provenance">
              <p class="rd-rubric">
                Where this came from, as recorded
              </p>
              <p class="rd-recorded recorded">
                {{ data.brief.provenance }}
              </p>
            </template>
          </div>
        </DeskCard>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.block {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

/* ------------------------------------ The flows ------------------------------------ */

.flows {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

/*
 * The two flows side by side, and **one above the other the moment they do not both fit**.
 * `minmax(min(460px, 100%), 1fr)` is the boards' own expression of that: 460px is the
 * narrowest a flow box reads at, and the `min(…, 100%)` is what stops the track being
 * wider than the page at 390px — which is the difference between wrapping and scrolling.
 */
.pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(460px, 100%), 1fr));
  gap: 28px;
  align-items: start;
  min-width: 0;
}

.side {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.side-head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
}

.caption {
  margin: 0;
  color: var(--rd-ink-muted);
}

.legend {
  margin: 0;
  max-width: var(--rd-measure);
}

.pending-note {
  margin: 0;
  max-width: var(--rd-measure);
}

/* -------------------------------- The problem rows --------------------------------- */

.rows {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.rows-head,
.problem {
  grid-template-columns: 44px minmax(0, 1fr) minmax(0, 1fr) 92px;
}

.rows-head {
  display: grid;
  gap: 8px 20px;
  padding: 0 16px 10px;
  align-items: baseline;
}

.end {
  border-top: 1px solid var(--rd-line);
}

.detail {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 22px 24px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
}

.detail-head {
  margin: 0;
}

.detail-pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 28px;
  min-width: 0;
}

.detail-side {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.detail-label {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.detail-label--wrong {
  color: var(--rd-wrong);
}

.detail-label--fixed {
  color: var(--rd-accent-dark);
}

.lines {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/* The board's en-dash bullet, drawn rather than read out as a list marker. */
.lines li {
  display: flex;
  gap: 10px;
  overflow-wrap: break-word;
}

.lines li::before {
  content: '–';
  flex: none;
  color: var(--rd-ink-faint);
}

/* ------------------------------- Already decided ----------------------------------- */

.decided {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.decided-line {
  margin: 0;
  font-size: 17px;
}

.decided p {
  margin: 0;
  max-width: var(--rd-measure);
}

.sources {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.source {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
}

/* -------------------------- As the session recorded it ----------------------------- */

.measured {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.figures {
  display: flex;
  flex-wrap: wrap;
  gap: 14px 40px;
  align-items: flex-end;
}

.figure {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.faint {
  color: var(--rd-ink-faint);
}

.read-it {
  margin-left: auto;
}

.as-recorded {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.recorded {
  margin: 0;
  color: var(--rd-ink-dim);
}

.cell {
  min-width: 0;
  overflow-wrap: break-word;
}

@media (max-width: 760px) {
  /*
   * The four-column problem row becomes a card, the way every table on these pages does.
   * The column names move out of sight rather than away, so a screen reader still has
   * them, and each cell says which column it is.
   */
  .rows-head {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .problem {
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 16px;
    align-items: start;
  }

  .cell::before {
    content: attr(data-label);
    display: block;
    margin-bottom: 2px;
    font-size: 13px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--rd-ink-muted);
  }

  .read-it {
    margin-left: 0;
  }
}
</style>
