<script setup lang="ts">
import type { ReviewDetail } from '~/types/desk'

/**
 * `/reviews/{id}` — the **Overview** tab, and the address every pull request opens at.
 * DEV-1326; the target is S2, S12 and S15 in `docs/mockups/s-structured`.
 *
 * ```
 *  ┌ THE BOTTOM LINE ─────────────────────────────────────────────────────────────┐
 *  │ the brief's headline                                                         │
 *  │ 3 problems   Option C built   All 3 problems it fixes   12 findings          │
 *  └──────────────────────────────────────────────────────────────────────────────┘
 *  WHERE IT STANDS                                      NEEDS YOUR EYE
 *  ✓ Brief ready ── ✓ Code review ── ● Draft ready ─…   [BLOCKER] F9 · …
 *  ▸ The seven steps                                    [SHOULD]  F6 · …
 *  WHAT YOU'D BE ASKED                    3 of 5 written
 *  ✓ What does it do?   the after-flow caption    Understanding →
 *  …
 * ```
 *
 * **Three Overviews, chosen from the record alone** by `overviewState`: *ready*, *being
 * written* (the bottom line not written yet, findings shown without a severity) and *replies
 * in* (*Since you posted* and *What came back* in place of the bottom line and the
 * questions). `app/utils/overview.ts` holds every rule; this file draws what it answers.
 *
 * **What this page drew before moved rather than vanished.** It drew the case and the step
 * trail. The case is the Understanding tab, which every question here links to. The trail
 * is folded under *Where it stands*: the five stages say where the review is, and the trail
 * still says what each of the seven steps came to and how long it took, which nothing else
 * draws.
 */

const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `Overview · #${review.pr} · Review Desk` : 'Overview · Review Desk'
  }),
})

/** Everything the page draws, worked out once per review. */
function overviewOf(data: ReviewDetail) {
  const state = overviewState(data)
  const groups = state === 'replies_in' ? groupsOf(data) : null
  const questions = fiveQuestions(data)
  return {
    state,
    bottom: groups ? sinceBottomLine(data, groups) : readyBottomLine(data, state),
    stages: whereItStands(data),
    questions,
    written: questions.filter(question => question.mark === 'written').length,
    cameBack: groups ? whatCameBack(data, groups) : [],
    eye: needsYourEye(data, state),
  }
}

const BASIS = BASIS_WORDS
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <ReviewStepPage
        :detail="data"
        step="understand"
        tab="overview"
        :conversation-steps="CASE_STEPS"
        :headline="null"
      >
        <template
          v-for="view in [overviewOf(data)]"
          :key="view.state"
        >
          <!--
            **Said on Overview as it was on the case**, because a moved head is true of the
            review: a new commit on an unposted round means what is below may describe a
            commit the pull request has moved past.
          -->
          <MovedHead :detail="data" />

          <section
            class="bottom"
            :data-state="view.state"
            data-testid="bottom-line"
            aria-labelledby="bottom-line-heading"
          >
            <h2
              id="bottom-line-heading"
              class="rd-rubric"
            >
              {{ view.bottom.heading }}
            </h2>
            <p class="sentence">
              <TickedText :text="view.bottom.sentence" />
            </p>
            <dl
              v-if="view.bottom.cells.length"
              class="cells"
            >
              <div
                v-for="cell in view.bottom.cells"
                :key="cell.label"
                class="cell"
              >
                <dt class="figure">
                  {{ cell.figure }}
                </dt>
                <dd class="label">
                  {{ cell.label }}
                  <span
                    v-if="cell.sub"
                    class="cell-sub"
                  >{{ cell.sub }}</span>
                </dd>
              </div>
            </dl>
          </section>

          <div class="columns">
            <div class="main">
              <section
                class="stands"
                aria-labelledby="stands-heading"
              >
                <h2
                  id="stands-heading"
                  class="rd-rubric"
                >
                  Where it stands
                </h2>
                <ol
                  class="stages"
                  data-testid="where-it-stands"
                >
                  <li
                    v-for="stage in view.stages"
                    :key="stage.label"
                    class="stage"
                    :class="`stage--${stage.mark}`"
                    :aria-current="stage.mark === 'current' || stage.mark === 'writing' ? 'step' : undefined"
                  >
                    <span
                      class="dot"
                      aria-hidden="true"
                    />
                    {{ stage.label }}
                  </li>
                </ol>

                <!--
                  **The seven steps, folded.** The stages above answer *where is it*; the trail
                  still answers what each step came to, its facts and how long it took, and
                  nothing else draws those — so it is folded here rather than dropped.
                -->
                <details
                  class="rd-fold steps-fold"
                  data-testid="seven-steps"
                >
                  <summary>The seven steps, what each came to and how long it took</summary>
                  <StepTrail
                    :layers="data.layers"
                    :review-id="data.review.id"
                    :round="data.review.round"
                    :off-the-desk="data.not_for_signing != null"
                  />
                </details>
              </section>

              <section
                v-if="view.state !== 'replies_in'"
                class="list"
                aria-labelledby="asked-heading"
                data-testid="questions"
              >
                <header class="list-head">
                  <h2
                    id="asked-heading"
                    class="rd-rubric"
                  >
                    What you'd be asked
                  </h2>
                  <span class="count">{{ view.written }} of 5 written</span>
                </header>
                <ul class="rows">
                  <li
                    v-for="question in view.questions"
                    :key="question.asks"
                    class="row"
                    :data-mark="question.mark"
                  >
                    <span
                      class="mark"
                      :class="`mark--${question.mark}`"
                      :aria-label="question.mark === 'written' ? 'Written' : question.mark === 'writing' ? 'Writing now' : 'Not started'"
                      role="img"
                    />
                    <div class="row-body">
                      <h3 class="row-title">
                        {{ question.asks }}
                      </h3>
                      <p class="row-says">
                        <TickedText :text="question.answer" />
                      </p>
                    </div>
                    <NuxtLink
                      v-if="question.link"
                      class="row-link"
                      :to="question.link.to"
                    >
                      {{ question.link.label }} →
                    </NuxtLink>
                  </li>
                </ul>
              </section>

              <section
                v-else
                class="list"
                aria-labelledby="came-back-heading"
                data-testid="came-back"
              >
                <header class="list-head">
                  <h2
                    id="came-back-heading"
                    class="rd-rubric"
                  >
                    What came back
                  </h2>
                  <span class="count">{{ view.cameBack.length }} to look at</span>
                </header>
                <ul class="rows">
                  <li
                    v-for="row in view.cameBack"
                    :key="row.title"
                    class="row"
                    :data-tone="row.tone"
                  >
                    <span
                      class="mark"
                      :class="`mark--${row.tone}`"
                      aria-hidden="true"
                    />
                    <div class="row-body">
                      <h3 class="row-title">
                        {{ row.title }}
                      </h3>
                      <p class="row-says">
                        {{ row.says }}
                      </p>
                      <p
                        class="basis"
                        data-testid="basis"
                      >
                        {{ BASIS[row.basis] }}
                      </p>
                    </div>
                    <NuxtLink
                      class="row-link"
                      :to="row.link.to"
                    >
                      {{ row.link.label }} →
                    </NuxtLink>
                  </li>
                </ul>
              </section>
            </div>

            <aside
              v-if="view.eye.length"
              class="eye"
              aria-labelledby="eye-heading"
              data-testid="needs-your-eye"
            >
              <h2
                id="eye-heading"
                class="rd-rubric"
              >
                {{ view.state === 'being_written' ? 'Early · not graded yet' : 'Needs your eye' }}
              </h2>
              <NuxtLink
                v-for="card in view.eye"
                :key="card.key"
                class="card"
                :to="card.to"
              >
                <SeverityTag
                  v-if="card.severity"
                  :severity="card.severity"
                />
                <span class="card-title">{{ card.key }} · {{ card.claim }}</span>
                <span
                  v-if="card.where"
                  class="card-where"
                >{{ card.where }}</span>
              </NuxtLink>
            </aside>
          </div>
        </template>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.bottom {
  margin-top: 20px;
  padding: 24px 28px;
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-card);
  background: var(--rd-surface);
}

.sentence {
  margin: 12px 0 0;
  font-size: var(--rd-type-title);
  font-weight: 600;
  line-height: 1.35;
  color: var(--rd-ink);
  max-width: var(--rd-measure-recorded);
}

.cells {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 16px;
  margin: 20px 0 0;
  padding-top: 18px;
  border-top: 1px solid var(--rd-line);
}

.cell {
  min-width: 0;
}

.figure {
  font-size: var(--rd-type-title);
  font-weight: 600;
  color: var(--rd-ink);
}

.label {
  margin: 4px 0 0;
  font-size: var(--rd-type-body);
  color: var(--rd-ink-dim);
}

.cell-sub {
  display: block;
  font-size: var(--rd-type-ui);
  color: var(--rd-ink-muted);
}

.columns {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
  gap: 32px;
  margin-top: 28px;
}

.stages {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 18px;
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  font-size: var(--rd-type-body);
  color: var(--rd-ink-muted);
}

.stage {
  display: flex;
  align-items: center;
  gap: 6px;
}

.dot {
  width: 14px;
  height: 14px;
  border: 1.5px solid var(--rd-line-strong);
  border-radius: 50%;
}

.stage--done,
.stage--current,
.stage--writing {
  color: var(--rd-ink-dim);
}

.stage--done .dot,
.stage--current .dot {
  border-color: var(--rd-accent);
  background: radial-gradient(circle, var(--rd-accent) 0 3px, transparent 3.5px);
}

.stage--current,
.stage--writing {
  font-weight: 600;
  color: var(--rd-ink);
}

.stage--writing .dot {
  border-style: dashed;
  border-color: var(--rd-accent);
}

.steps-fold {
  margin-top: 8px;
  font-size: var(--rd-type-ui);
}

.list {
  margin-top: 24px;
}

.list-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--rd-line);
}

.count {
  font-size: var(--rd-type-ui);
  color: var(--rd-ink-muted);
}

.rows {
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: 20px minmax(0, 1fr) auto;
  gap: 14px;
  align-items: start;
  padding: 14px 0;
  border-bottom: 1px solid var(--rd-line);
}

.mark {
  width: 16px;
  height: 16px;
  margin-top: 4px;
  border: 1.5px solid var(--rd-line-strong);
  border-radius: 50%;
}

.mark--written,
.mark--fixed {
  border-color: var(--rd-accent);
  background: radial-gradient(circle, var(--rd-accent) 0 3px, transparent 3.5px);
}

.mark--writing {
  border-style: dashed;
  border-color: var(--rd-accent);
}

.mark--contested {
  border-color: var(--rd-warn-line);
  background: radial-gradient(circle, var(--rd-warn-line) 0 3px, transparent 3.5px);
}

.mark--answered {
  border-color: var(--rd-ink-muted);
}

.mark--moved {
  border-radius: var(--rd-radius-dot);
  border-color: var(--rd-ink-muted);
}

.row-title {
  margin: 0;
  font-size: var(--rd-type-lead);
  font-weight: 600;
  color: var(--rd-ink);
}

.row-says {
  margin: 2px 0 0;
  font-size: var(--rd-type-body);
  color: var(--rd-ink-dim);
}

.basis {
  margin: 2px 0 0;
  font-size: var(--rd-type-meta);
  color: var(--rd-ink-muted);
}

.row-link {
  font-size: var(--rd-type-body);
  color: var(--rd-accent);
  text-decoration: none;
  white-space: nowrap;
  align-self: center;
}

.row-link:hover {
  text-decoration: underline;
}

.eye {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  padding: 14px 16px;
  border-radius: var(--rd-radius);
  background: var(--rd-nit-wash);
  color: var(--rd-ink);
  text-decoration: none;
}

.card:hover {
  outline: 1px solid var(--rd-line-strong);
}

.card-title {
  font-size: var(--rd-type-body);
  font-weight: 600;
}

.card-where {
  font-family: var(--rd-font-mono);
  font-size: var(--rd-type-meta);
  color: var(--rd-ink-muted);
  overflow-wrap: anywhere;
}

@media (width <= 700px) {
  .bottom {
    padding: 18px;
  }

  .columns {
    grid-template-columns: minmax(0, 1fr);
  }

  .row {
    grid-template-columns: 20px minmax(0, 1fr);
  }

  .row-link {
    grid-column: 2;
    align-self: start;
  }
}
</style>
