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
 * | already decided | what is settled, and by whom | one line, and a fold |
 * | the problem, as recorded | what the session wrote | a fold |
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

/**
 * Whether the provenance behind "Already decided" is open.
 *
 * **Its own state, not the foot's.** They are two different pieces of recorded text with
 * two different questions behind them — where the decision came from, and what the session
 * wrote about the problem — and one flag would open both from either control.
 */
const cameFromOpen = ref(false)

/**
 * Another review on this page starts folded, with no problem open.
 *
 * All three are about *this* review's brief, and `openProblem` is a problem's number: on a
 * page that was reused across a review change, a problem left open on review 50 would come
 * back open on review 51 over whichever of its problems shared the number, with its marks on
 * both of review 51's flows. DEV-960, and the watch is DEV-958's.
 *
 * **Both folds go with it**, and the argument is the one that gave them separate flags: two
 * pieces of recorded text with two different questions behind them. Either one left unfolded
 * would show the next review's prose under a control the reader never pressed.
 *
 * **It cannot fire today, and that is deliberate rather than overlooked.** `<NuxtPage />`
 * keys a page by its own path with the parameters interpolated, so changing the review
 * unmounts this component and mounts a fresh one — DEV-960 says the component is reused and
 * it is not. `draft.vue` carries the measurement and the whole argument for keeping the
 * watch: it is the page's statement of what belongs to one review, and a `pageKey` added
 * later makes it the only thing standing between two reviews.
 */
watch(reviewId, () => {
  openProblem.value = null
  cameFromOpen.value = false
  recordedOpen.value = false
})

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
 * What is still open, as the two figures the sentence used to be.
 *
 * **Counted rather than asserted, so neither figure can promise a table that is empty**,
 * and dropped entirely when both are nought. The table itself is the Solution page's:
 * DEV-885's comment is that `open_choices` and `disagreements` are drawn once, there, and
 * this is the link to it.
 *
 * It was one sentence — "6 choices the ticket left open, and 2 places the description and
 * the code disagree. They are on the Solution page, drawn once" — which is a figure a
 * reader has to read a clause to find. Two figures are two things to look at.
 */
function stillOpen(brief: Brief): Array<{ count: number, says: string }> {
  const choices = (brief.open_choices ?? []).length
  const clashes = (brief.disagreements ?? []).length
  return [
    { count: choices, says: choices === 1 ? 'choice the ticket left open' : 'choices the ticket left open' },
    { count: clashes, says: `${clashes === 1 ? 'place' : 'places'} the description and the code disagree` },
  ].filter(figure => figure.count > 0)
}

/* ------------------------------- The foot of the page --------------------------- */

/*
 * **There is no word count here any more, and that is DEV-892's defect 4.**
 *
 * The foot used to carry a grey strip reading "105 words as the session recorded it", and
 * a `wordsRecorded` and a `sketchLines` to compute it. It came off the board, where the
 * figure is half of a pair — 85 words and a picture against 310 words and a 50-line text
 * sketch — and the pair is an argument that this page is shorter than the record it is
 * drawn from. That argument belongs to the design review it was made in. On a screen it is
 * a figure about nothing: a reader cannot act on it, the comparison it was half of is not
 * drawn, and on the invented example it ran the wrong way round.
 *
 * What is left is the fold itself, which is the thing the strip was attached to and the
 * only route on this page to the problem as the session wrote it.
 */

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
      >
        <!--
          **The one page a never-started review has**, and so the one place it can be taken
          off the desk. This is the shape of the review DEV-945 was raised from: a request
          recorded and never picked up, with nothing under it to lose.
        -->
        <RemoveReview :detail="data" />
      </ReviewStepPage>

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
          :not-for-signing="data.not_for_signing"
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
            <TickedText :text="data.brief.problem" />
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
            <TickedText :text="data.brief.provenance" />
          </p>
        </DeskSection>

        <RemoveReview :detail="data" />
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
          :not-for-signing="data.not_for_signing"
        />

        <!-- ------------------------------ The two flows ------------------------------ -->
        <section
          v-if="flowBefore(data.brief).length || flowAfter(data.brief).length"
          class="flows"
        >
          <!--
            **The problems, listed once, above both flows.** DEV-917: each mark in a box is
            a tag carrying the problem's name, and the names have to be readable in one
            place before a reader meets them scattered down two columns. In number order,
            which is the record's own order and the only order that makes "the same 2" easy
            to find.
          -->
          <div
            v-if="problems(data.brief).length"
            class="key"
          >
            <span class="rd-note">
              {{ count(problems(data.brief).length, 'problem') }}:
            </span>
            <ProblemTag
              v-for="problem in problems(data.brief)"
              :key="problem.number"
              :number="problem.number"
              :label="problem.label"
              tone="wrong"
            />
          </div>

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
                :problems="problems(data.brief)"
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
                :problems="problems(data.brief)"
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
            Each tag is one of the problems above. Red on the left marks what was wrong;
            green on the right, with the same number, marks what fixes it. Open a problem
            below to pick its boxes out of both flows.
          </p>
          <p
            v-else-if="flowBefore(data.brief).length"
            class="rd-note legend"
          >
            Each red tag is one of the problems above. The green marks that say what fixes
            each one arrive with the after flow.
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
                ><TickedText :text="problem.was_wrong" /></span>
                <!--
                  `now_fixed` is nullable by design: the problem half can be written before
                  the change is described, which is what lets a blind pass be handed a
                  problem without the answer. So the absence is a state with words.
                -->
                <span
                  class="cell"
                  data-label="Now"
                >
                  <template v-if="problem.now_fixed">
                    <TickedText :text="problem.now_fixed" />
                  </template>
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
                      <!--
                        **The line's text is one box.** The `li` is a flex row so the
                        en-dash sits beside the words, and `TickedText` adds no box of its
                        own — so without this span every word run and every code chip in
                        the line would be a flex item of the `li` and the line would draw
                        as a row of narrow columns. `.line-text` is what makes it a
                        paragraph again.
                      -->
                      <li
                        v-for="(line, index) in problem.detail_before"
                        :key="index"
                      >
                        <span class="line-text"><TickedText :text="line" /></span>
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
                      <!-- One box for the line's text, as in the Before list above. -->
                      <li
                        v-for="(line, index) in problem.detail_after"
                        :key="index"
                      >
                        <span class="line-text"><TickedText :text="line" /></span>
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
                <TickedText :text="cell" />
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
            <TickedText :text="data.brief.how_it_works_today" />
          </p>
        </section>

        <!--
          ------------------------------ Already decided -----------------------------
          **Three levels, in order: what was decided, what is still open, then where it
          all came from.** DEV-917's defect 4. This was a heading, a small paragraph and
          seven source lines that all looked alike, one of them carrying a 40-character
          commit id twice — "lacking structure and it's difficult to read". The levels are
          what makes it readable: a reader who only wants the decision reads one line and
          stops.
        -->
        <section class="block">
          <h2 class="rd-display-lead">
            Already decided
          </h2>
          <div class="decided">
            <!-- 1 · the decision -->
            <div
              v-if="chosen(data.brief)"
              class="decision"
            >
              <DeskPill tone="go">
                Option {{ chosen(data.brief)!.key }} of
                {{ (data.brief.options ?? []).length }}
              </DeskPill>
              <span class="decision-title">
                <TickedText :text="chosen(data.brief)!.title" />
              </span>
              <NuxtLink
                class="decision-link"
                :to="`/reviews/${data.review.id}/alternatives`"
              >See all {{ (data.brief.options ?? []).length }} options</NuxtLink>
            </div>
            <p
              v-else
              class="rd-meta decided-line"
            >
              The brief names no option as the one that was built.
            </p>

            <!--
              2 · what is still open. **Two figures rather than the sentence this was**,
              and the table itself is Solution's: DEV-885's comment is that `open_choices`
              and `disagreements` are drawn once, there.
            -->
            <div
              v-if="stillOpen(data.brief).length"
              class="open-counts"
            >
              <span
                v-for="figure in stillOpen(data.brief)"
                :key="figure.says"
                class="open-count"
              ><b>{{ figure.count }}</b> {{ figure.says }}</span>
              <NuxtLink
                class="decision-link"
                :to="`/reviews/${data.review.id}/solution`"
              >On the Solution page</NuxtLink>
            </div>

            <!--
              3 · the sources, as a table. Seven loose lines that all looked alike become
              five columns, and a link is an extra way to read one rather than the only
              way: DEV-885 forbids a page whose only route to something is GitHub.
            -->
            <DeskTable
              v-if="sources(data).length"
              :columns="['Kind', 'Reference', 'State', 'What it tells us', '']"
              template="110px minmax(0, 1.1fr) 130px minmax(0, 1.6fr) 60px"
              label="What this review read the problem from"
            >
              <tr
                v-for="source in sources(data)"
                :key="source.position"
                role="row"
                class="row"
              >
                <td
                  role="cell"
                  class="td td--muted"
                  data-label="Kind"
                >
                  {{ source.kind }}
                </td>
                <td
                  role="cell"
                  class="td td--strong"
                  data-label="Reference"
                >
                  <TickedText :text="source.label" />
                </td>
                <td
                  role="cell"
                  class="td"
                  data-label="State"
                >
                  <!--
                    The ticket's state as the session read it. A source that reported none
                    draws an empty cell rather than a word Review Desk made up — it read
                    no ticket.
                  -->
                  <DeskPill
                    v-if="source.status"
                    tone="quiet"
                  >
                    {{ source.status }}
                  </DeskPill>
                </td>
                <td
                  role="cell"
                  class="td"
                  data-label="What it tells us"
                >
                  <TickedText
                    v-if="source.note"
                    :text="source.note"
                  />
                </td>
                <td
                  role="cell"
                  class="td"
                  data-label=""
                >
                  <a
                    v-if="source.link"
                    :href="source.link"
                    target="_blank"
                    rel="noreferrer noopener"
                  >open<span class="rd-visually-hidden">
                    {{ source.label }}, in a new tab</span></a>
                </td>
              </tr>
            </DeskTable>

            <!--
              **Where the decision came from, behind a fold.** DEV-892's defect 3: this
              printed open, and on a real review it was a 59-word paragraph about the
              ticket, the base commit and the dispatcher's ledger sitting under a heading
              that promises one settled thing. The decision is the first level, its sources
              are the table, and the paragraph is what a reader opens when neither answers
              them.
            -->
            <template v-if="data.brief.provenance">
              <button
                type="button"
                class="rd-btn came-from"
                :aria-expanded="cameFromOpen"
                aria-controls="came-from"
                @click="cameFromOpen = !cameFromOpen"
              >
                {{ cameFromOpen ? 'Fold' : 'Where this came from, as recorded' }}
              </button>
              <p
                v-if="cameFromOpen"
                id="came-from"
                class="rd-recorded recorded"
              >
                <TickedText :text="data.brief.provenance" />
              </p>
            </template>
          </div>
        </section>

        <!--
          -------------------------- As the session recorded it -----------------------
          **The fold, and no figure over it.** The grey strip that used to head this card
          counted the words behind it; DEV-892's defect 4 takes it off. What it was attached
          to stays, because it is the only route on this page to the problem in the words
          the session wrote it in — everything above is the page's reading of that record.

          Where the decision came from is not here any more either: it is behind its own
          control under "Already decided", which is the section it is about. One piece of
          recorded text, one place.
        -->
        <DeskCard
          as="section"
          tone="well"
          pad="section"
          class="measured"
        >
          <button
            type="button"
            class="rd-btn read-it"
            :aria-expanded="recordedOpen"
            aria-controls="as-recorded"
            @click="recordedOpen = !recordedOpen"
          >
            {{ recordedOpen ? 'Fold' : 'Read the problem as recorded' }}
          </button>

          <p
            v-if="recordedOpen"
            id="as-recorded"
            class="rd-recorded recorded"
          >
            <TickedText :text="data.brief.problem" />
          </p>
        </DeskCard>

        <!--
          **Last on the page, and that is the whole of its placement argument.** Dismissing
          a review is not a step of reading one, so it sits below everything that is —
          under the record the page is a reading of, where a developer who has decided
          arrives and a developer who is reading never does.
        -->
        <RemoveReview :detail="data" />
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
 * The row of tags over both flows. It wraps, and each tag moves whole: three tags of two
 * or three words each do not fit a phone's width on one line, and a tag split across two
 * is two halves of a shape.
 */
.key {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 14px;
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

/*
 * The board's en-dash bullet, drawn rather than read out as a list marker.
 *
 * **A flex row here has exactly two items: the dash and `.line-text`.** This row had one
 * item per word run and one per code chip instead, which is DEV-939: `TickedText` is
 * `display: contents` so that it adds no box, and a line holding five code names came out
 * as eight narrow columns with the names broken mid-word — `url.inte / nded`. Each rule is
 * right on its own; what was missing was the box between them.
 */
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

/*
 * The line's words and chips, as one paragraph beside the dash.
 *
 * **`min-width: 0` is a guard here, not the thing holding the line inside the column.** A
 * flex item's automatic minimum is its content, which is the usual way one long token
 * widens a row past its container — but the chip carries `overflow-wrap: anywhere`, and
 * that drives its min-content down to a single character, so the automatic minimum never
 * reaches the column's width. Measured at 1440 and 390 with the 46-character name in the
 * record: the box comes out the same size either way.
 *
 * It is kept because what it guards against is real and lives on the chip rather than
 * here. A piece drawn in this box without `anywhere` — a path, a URL, a name some later
 * rule declines to break — would widen the row, and at 390px that is the difference
 * between a line that wraps and a page that scrolls sideways.
 */
/*
 * **`flex: 1` so the box is the column less the dash for *every* line, not just a long
 * one.** Shrink-to-fit is the default, and a shrink-to-fit box only reaches the column's
 * far edge while its content overflows — so on a short line the box would stop where the
 * words stop. Nothing a reader sees changes either way, because the text is left-aligned
 * in both; what changes is whether "one box, the width of the column" is something the
 * stylesheet guarantees or something this particular line happens to produce. The test
 * below measures the former, so the stylesheet has to mean it. `.says-line` on
 * Alternatives carries the same pair for the same reason.
 */
.line-text {
  flex: 1;
  min-width: 0;
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

/*
 * The decision: the pill, the title beside it, the link to the rest. It wraps, and each
 * part moves whole — the pill is a pill, and this app's rule is that one never breaks
 * across lines.
 */
.decision {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  min-width: 0;
}

.decision-title {
  font-size: 17px;
  min-width: 0;
}

.decision-link {
  white-space: nowrap;
}

/* The two figures, side by side where there is room and stacked where there is not. */
.open-counts {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px 28px;
  min-width: 0;
}

.open-count b {
  font-variant-numeric: tabular-nums;
}

/* -------------------------- As the session recorded it ----------------------------- */

.measured {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/*
 * Both controls sit at the start of their column rather than being pushed to an edge — the
 * figure that used to pin `.read-it` to the right is gone, and a lone button hanging off
 * the far side of a card reads as the end of a row that is not there.
 *
 * `align-self` on the control, not `align-items` on the card: the recorded text under it is
 * a sibling in the same column and should still take the whole width.
 */
.read-it,
.came-from {
  align-self: flex-start;
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

}
</style>
