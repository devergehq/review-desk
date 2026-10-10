<script setup lang="ts">
import type { Brief, BriefOption, BriefProblem } from '~/types/desk'

/**
 * Step 3 — **Alternatives**. Board A6, with a real blind pass overlaid, which Patrick
 * liked on 10 October 2026.
 *
 * The page answers four questions and stops:
 *
 * 1. **What were the options, and what does each do about each numbered problem?** The
 *    matrix: a row per option, a column per problem, a verdict in each cell.
 * 2. **Who saw each one?** `proposed_by` — the ticket, a blind pass that never saw the
 *    ticket, or both independently.
 * 3. **What was built, against what a fresh pair of eyes would pick?** Two cards side by
 *    side. When they agree that is worth knowing; when they disagree it is worth more.
 * 4. **What did the blind pass need to know that nobody told it?** Its questions, which
 *    are the cheapest findings on the page.
 *
 * ## `not_assessed` is a verdict
 *
 * DEV-885's comment is explicit, and it is the one thing about this page that is easy to
 * get wrong: "`not_assessed` is a stored verdict, not a missing one. Draw it from the
 * value. A missing verdict is a different state and should not look the same." The board
 * this comes from has an option the blind pass never came up with, so nothing scored it
 * against the three problems — "nobody judged this" and "the brief forgot to judge this"
 * are different statements, and `VerdictCell` draws them differently.
 *
 * ## The blind pass has three states, and each has its own wording
 *
 * `blind_pass_state` is derived by the server from the pass's presence and the options'
 * `proposed_by` together, so it cannot contradict the option list under it. **Review Desk
 * runs no blind pass**; everything here is what a session reported.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review
      ? `Alternatives · #${review.pr} · Review Desk`
      : 'Alternatives · Review Desk'
  }),
})

/** Which option's reasoning is open, by key. One at a time, as on the board. */
const openOption = ref<string | null>(null)

function toggleOption(key: string) {
  openOption.value = openOption.value === key ? null : key
}

/* ------------------------------- Reading the brief ------------------------------- */

function options(brief: Brief): BriefOption[] {
  return brief.options ?? []
}

function problems(brief: Brief): BriefProblem[] {
  return brief.problems ?? []
}

/**
 * One option's verdict on one problem, or `null`.
 *
 * Looked up by problem number rather than taken positionally. `verdicts` is served in
 * problem order, but an option that recorded verdicts against two of three problems would
 * otherwise have its second verdict drawn in the third column — a wrong answer that looks
 * like a right one.
 */
function verdictOn(option: BriefOption, problem: number) {
  return (option.verdicts ?? []).find(verdict => verdict.problem === problem) ?? null
}

/** The option that was built. */
function built(brief: Brief): BriefOption | null {
  return options(brief).find(option => option.chosen) ?? null
}

/** The option the blind pass would have picked, when it named one of the brief's own. */
function blindPick(brief: Brief): BriefOption | null {
  const key = brief.blind_pass?.pick_key
  if (!key) return null
  return options(brief).find(option => option.key === key) ?? null
}

/** How many options each side put forward. `both` counts on both sides, because it was. */
function raisedBy(brief: Brief, who: 'ticket' | 'blind_pass'): number {
  return options(brief).filter(
    option => option.proposed_by === who || option.proposed_by === 'both',
  ).length
}

/**
 * Whether an option is one the blind pass found and the ticket did not.
 *
 * The interesting column on the board — four of its eight rows — because an option nobody
 * on the project thought of is the thing a blind pass is run to produce.
 */
function onlyBlind(option: BriefOption): boolean {
  return option.proposed_by === 'blind_pass'
}

/* ----------------------------- The three blind states ---------------------------- */

/**
 * What to say about the blind pass, in its own words for each of the three states.
 *
 * **Three wordings, and none of them reads like another** — the acceptance criterion names
 * this twice, because a pass that did not run and a pass that found nothing new are the two
 * that are easiest to collapse into one grey sentence, and they mean opposite things about
 * how much the option list has been tested.
 */
function blindState(brief: Brief): { heading: string, says: string, tone: 'plain' | 'quiet' } {
  switch (brief.blind_pass_state) {
    case 'not_run':
      return {
        heading: 'No blind pass was run',
        says:
          'Nothing has looked at this problem without also seeing the answer, so the '
          + 'options below are the ones the ticket already contained. Review Desk runs no '
          + 'blind pass; a session reports one.',
        tone: 'quiet',
      }
    case 'ran_and_found_nothing':
      return {
        heading: 'A blind pass ran and found nothing the ticket had missed',
        says:
          'A session reported handing this problem to a pass that saw no ticket, no pull '
          + 'request and no code. It came back with nothing the ticket did not already '
          + 'have — which is a result about the option list, not an absence of one.',
        tone: 'plain',
      }
    default:
      return {
        heading: 'A blind pass ran',
        says:
          'A session reported handing this problem to a pass that saw no ticket, no pull '
          + 'request and no code. What it put forward is marked below.',
        tone: 'plain',
      }
  }
}

/** The matrix's label, which names how many options and how many problems it holds. */
function matrixLabel(brief: Brief): string {
  return `${count(options(brief).length, 'option')} against `
    + `${count(problems(brief).length, 'numbered problem')}`
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <ReviewStepPage
        v-if="!data.brief || !options(data.brief).length"
        :detail="data"
        step="alternatives"
        headline="No alternatives are recorded for this change."
        sub="Step 3 is where a session writes down the ways it considered solving the problem, before it reads the diff. Nothing has written any."
      />

      <ReviewStepPage
        v-else
        :detail="data"
        step="alternatives"
        :headline="`${count(options(data.brief).length, 'option')}, and who saw each`"
        :sub="data.brief.approach_verdict ?? null"
      >
        <!-- --------------------------------- The matrix ------------------------------- -->
        <section class="block">
          <DeskTable
            :columns="[
              'Option',
              'Seen by',
              ...problems(data.brief).map(problem => `${problem.number} · ${problem.was_wrong}`),
              '',
            ]"
            :template="`2.4fr 150px repeat(${problems(data.brief).length}, minmax(0, 1fr)) 110px`"
            :label="matrixLabel(data.brief)"
          >
            <!--
              The problem columns are headed by the mark and the problem itself, which is
              how the board reads them and the only way the matrix answers without a trip
              back to the Understand page.
            -->
            <template #head>
              <th
                scope="col"
                class="th"
              >
                Option
              </th>
              <th
                scope="col"
                class="th"
              >
                Seen by
              </th>
              <th
                v-for="problem in problems(data.brief)"
                :key="problem.number"
                scope="col"
                class="th th--sentence"
              >
                <ProblemMark
                  :number="problem.number"
                  tone="wrong"
                  size="inline"
                  says="the problem this column is about"
                />
                <span>{{ problem.was_wrong }}</span>
              </th>
              <th
                scope="col"
                class="th"
              >
                <span class="rd-visually-hidden">Open the reasoning</span>
              </th>
            </template>

            <template
              v-for="option in options(data.brief)"
              :key="option.key"
            >
              <tr class="shell">
                <td
                  class="holder"
                  :colspan="problems(data.brief).length + 3"
                >
                  <OpenRow
                    class="option"
                    :class="{
                      'option--built': option.chosen,
                      'option--blind': onlyBlind(option),
                    }"
                    :open="openOption === option.key"
                    :controls="`option-${option.key}`"
                    opens="Reasoning"
                    :label="`option ${option.key}`"
                    @toggle="toggleOption(option.key)"
                  >
                    <span
                      class="cell title"
                      data-label="Option"
                    >
                      <span class="key">{{ option.key }}</span>
                      <span class="words">{{ option.title }}</span>
                      <DeskPill
                        v-if="option.chosen"
                        tone="go"
                      >Built</DeskPill>
                      <DeskPill
                        v-else-if="onlyBlind(option)"
                        tone="info-strong"
                      >Not on the ticket</DeskPill>
                    </span>

                    <!--
                      Who put it forward. `both` draws both pills, because the point of the
                      column is that two sides arrived at the same option independently.
                    -->
                    <span
                      class="cell seen"
                      data-label="Seen by"
                    >
                      <DeskPill
                        v-if="option.proposed_by !== 'blind_pass'"
                        tone="quiet"
                      >Ticket {{ option.key }}</DeskPill>
                      <DeskPill
                        v-if="option.proposed_by !== 'ticket'"
                        tone="info"
                      >Blind</DeskPill>
                    </span>

                    <span
                      v-for="problem in problems(data.brief)"
                      :key="problem.number"
                      class="cell"
                      :data-label="`${problem.number} · ${problem.was_wrong}`"
                    >
                      <VerdictCell :verdict="verdictOn(option, problem.number)" />
                    </span>
                  </OpenRow>

                  <div
                    v-if="openOption === option.key"
                    :id="`option-${option.key}`"
                    class="reasoning"
                  >
                    <div class="reasoning-side">
                      <p class="rd-rubric">
                        For
                      </p>
                      <p class="rd-recorded argument">
                        {{ option.argument_for }}
                      </p>
                    </div>
                    <div class="reasoning-side">
                      <p class="rd-rubric">
                        Against
                      </p>
                      <p class="rd-recorded argument">
                        {{ option.argument_against }}
                      </p>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
          </DeskTable>
          <span class="end" />

          <p class="rd-note legend">
            The numbered columns are the problems on
            <NuxtLink :to="`/reviews/${data.review.id}/understand`">
              the Understand page
            </NuxtLink>. Open an option for the reasoning recorded for it.
          </p>
        </section>

        <!-- ------------------------- Built against the blind pick --------------------- -->
        <section class="block">
          <h2 class="rd-display-lead">
            Built against what a fresh pair of eyes would pick
          </h2>
          <div class="cards">
            <DeskCard
              v-if="built(data.brief)"
              class="card"
            >
              <p class="rd-rubric">
                Built
              </p>
              <p class="rd-display-card card-title">
                {{ built(data.brief)!.key }} · {{ built(data.brief)!.title }}
              </p>
              <p class="rd-note card-why">
                {{ built(data.brief)!.argument_for }}
              </p>
            </DeskCard>
            <DeskCard
              v-else
              tone="pending"
              class="card"
            >
              <p class="rd-rubric">
                Built
              </p>
              <p class="rd-note">
                The brief names no option as the one that was built.
              </p>
            </DeskCard>

            <!--
              The blind pick, in the three states it has: it picked one of the brief's own
              options; it ran and recommended nothing; or no pass ran at all.
            -->
            <DeskCard
              v-if="blindPick(data.brief)"
              class="card card--blind"
            >
              <p class="rd-rubric rd-rubric--blind">
                The blind pass would pick
              </p>
              <p class="rd-display-card card-title">
                {{ blindPick(data.brief)!.key }} · {{ blindPick(data.brief)!.title }}
              </p>
              <p class="rd-note card-why">
                {{ data.brief.blind_pass?.pick_why }}
              </p>
              <DeskPill
                v-if="blindPick(data.brief)!.chosen"
                tone="go-quiet"
              >
                Same as what was built
              </DeskPill>
              <DeskPill
                v-else
                tone="info"
              >
                Not what was built
              </DeskPill>
            </DeskCard>
            <DeskCard
              v-else
              tone="pending"
              class="card"
            >
              <p class="rd-rubric">
                {{ blindState(data.brief).heading }}
              </p>
              <p class="rd-note">
                <template v-if="data.brief.blind_pass && !data.brief.blind_pass.pick_key">
                  The pass ran and recommended none of these options.
                </template>
                <template v-else>
                  Nothing independent has picked between them.
                </template>
              </p>
            </DeskCard>
          </div>
        </section>

        <!-- ---------------------- What the blind pass needed to know ------------------- -->
        <section
          v-if="data.brief.blind_pass?.questions?.length"
          class="block"
        >
          <h2 class="rd-display-lead">
            What the blind pass needed to know
          </h2>
          <p class="rd-note legend">
            Four things the problem statement did not say. Each one is cheaper to answer now
            than after the change is built.
          </p>
          <ul class="questions">
            <li
              v-for="(question, index) in data.brief.blind_pass.questions"
              :key="index"
              class="question"
            >
              <span
                class="ask"
                aria-hidden="true"
              >?</span>
              <span>{{ question }}</span>
            </li>
          </ul>
        </section>

        <!-- --------------------------- Where these came from -------------------------- -->
        <section class="block">
          <h2 class="rd-display-lead">
            Where these came from
          </h2>
          <div class="cards">
            <DeskCard class="card">
              <p class="rd-rubric">
                The ticket
              </p>
              <p class="card-title source-count">
                {{ count(raisedBy(data.brief, 'ticket'), 'option') }}
              </p>
              <p class="rd-note">
                Written before the work started. The option that was built was agreed there.
              </p>
            </DeskCard>

            <DeskCard
              :tone="data.brief.blind_pass_state === 'not_run' ? 'pending' : 'plain'"
              class="card"
              :class="{ 'card--blind': data.brief.blind_pass_state !== 'not_run' }"
            >
              <p
                class="rd-rubric"
                :class="{ 'rd-rubric--blind': data.brief.blind_pass_state !== 'not_run' }"
              >
                {{ blindState(data.brief).heading }}
              </p>
              <p
                v-if="data.brief.blind_pass_state !== 'not_run'"
                class="card-title source-count"
              >
                {{ count(raisedBy(data.brief, 'blind_pass'), 'option') }}
              </p>
              <p class="rd-note">
                {{ blindState(data.brief).says }}
              </p>
              <p
                v-if="data.brief.blind_pass"
                class="rd-note"
              >
                <strong>It looked up:</strong> {{ data.brief.blind_pass.looked_up }}
              </p>
            </DeskCard>
          </div>

          <!--
            What the pass was given, verbatim, behind a fold. It is long — a page of prose —
            and it is the only way to judge whether a blind pass was really blind, so it is
            on the page rather than linked to.
          -->
          <details
            v-if="data.brief.blind_pass"
            class="given"
          >
            <summary>What the blind pass was given, word for word</summary>
            <p class="rd-recorded given-text">
              {{ data.brief.blind_pass.given }}
            </p>
          </details>
        </section>
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

/*
 * The matrix row is an `OpenRow` inside one full-width cell, because a row that is a
 * `<button>` cannot also be a `<tr>` of `<td>`s — and the row has to be the button, so that
 * what you click is what you read. The table keeps its header association through
 * `data-label` on each cell, which is what the narrow layout shows as well.
 */
.shell {
  display: block;
  border-top: 1px solid var(--rd-line);
}

.holder {
  display: block;
  padding: 0;
}

/*
 * The row takes the table's own column ratios. `--template` is set on the `<table>` as a
 * custom property, so it inherits down to here — which is what lets the row be a button
 * and still line up with the header above it.
 */
.option {
  grid-template-columns: var(--template);
  border-top: 0;
}

/* A 4px edge saying where an option came from, which is the board's own marker. */
.option--built {
  border-left: 4px solid var(--rd-accent);
}

.option--blind {
  border-left: 4px solid var(--rd-question-ink);
}

.cell {
  min-width: 0;
  overflow-wrap: break-word;
}

.title {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  align-items: center;
}

.key {
  flex: none;
  font-family: var(--rd-font-mono);
  font-size: 13px;
  color: var(--rd-ink-muted);
}

.words {
  font-weight: 500;
  min-width: 0;
}

.option--built .words,
.option--blind .words {
  font-weight: 600;
}

.seen {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.reasoning {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 28px;
  padding: 20px 24px 24px;
  background: var(--rd-ground);
  min-width: 0;
}

.reasoning-side {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.reasoning p {
  margin: 0;
}

.argument {
  color: var(--rd-ink-dim);
}

.end {
  border-top: 1px solid var(--rd-line);
}

.legend {
  margin: 0;
  max-width: var(--rd-measure);
}

/* ---------------------------------- The two cards ---------------------------------- */

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 16px;
  min-width: 0;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
  padding: 18px 20px;
  min-width: 0;
}

.card p {
  margin: 0;
}

.card--blind {
  box-shadow: 0 0 0 1px var(--rd-question-ink), var(--rd-lift-card);
}

.rd-rubric--blind {
  color: var(--rd-question-ink);
}

.card-title {
  overflow-wrap: break-word;
}

.source-count {
  font-weight: 600;
  font-size: 17px;
}

.card-why {
  max-width: var(--rd-measure);
}

/* ----------------------------------- Questions ------------------------------------- */

.questions {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
  max-width: 860px;
}

.question {
  display: flex;
  gap: 10px;
  padding: 10px 0;
  border-top: 1px solid var(--rd-line);
  overflow-wrap: break-word;
}

.question:last-child {
  border-bottom: 1px solid var(--rd-line);
}

.ask {
  flex: none;
  color: var(--rd-ink-faint);
}

/* ------------------------------- What it was given --------------------------------- */

.given summary {
  display: flex;
  align-items: center;
  min-height: var(--rd-control-min);
  color: var(--rd-accent);
  font-weight: 500;
  cursor: pointer;
}

.given summary:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.given-text {
  margin: 8px 0 0;
  padding: 16px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
  color: var(--rd-ink-dim);
}

/*
 * Under 760px the matrix becomes one card per option: the row is a single column and every
 * cell says which column it was, in the problem's own words. `DeskTable` does this for the
 * tables it owns the rows of; the matrix's rows are this page's, so it does it here.
 */
@media (max-width: 760px) {
  .option {
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
    padding: 16px;
    align-items: start;
  }

  .cell[data-label]::before {
    content: attr(data-label);
    display: block;
    margin-bottom: 4px;
    font-size: 13px;
    color: var(--rd-ink-muted);
  }

  /* `::before` needs a block above it, which a wrapping flex row of pills is not. */
  .title,
  .seen {
    display: block;
  }

  .title > *,
  .seen > * {
    margin: 0 6px 2px 0;
  }
}
</style>
