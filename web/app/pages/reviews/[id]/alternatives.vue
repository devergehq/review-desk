<script setup lang="ts">
import type {
  BlindPassState,
  Brief,
  BriefOption,
  BriefProblem,
  ReviewLayer,
} from '~/types/desk'
import type { Absence } from '~/utils/steps'

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

/**
 * Another review on this page starts with nothing open.
 *
 * `openOption` is about *this* review's options, and it is an option's key: on a page that
 * was reused across a review change, an option left open on review 50 would come back open
 * on review 51 over whichever of its options shared the letter. DEV-960, and the watch is
 * DEV-958's, whose own page was written with it.
 *
 * **It cannot fire as pages are keyed today, and is kept deliberately.** The measurement
 * and the argument are in `draft.vue`, once.
 */
watch(reviewId, () => {
  openOption.value = null
})

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

/**
 * Whether one side put this option forward. `both` counts for each, because it was.
 *
 * **Asked positively, which it was not.** The two pills were drawn on
 * `proposed_by !== 'blind_pass'` and `proposed_by !== 'ticket'`, and both of those are true
 * when the field is absent — so every option on a brief recorded before `proposed_by`
 * existed wore *both* badges and claimed two independent sources had arrived at it. A
 * negated test answers "yes" for a value it has never heard of; a positive one does not.
 */
function raised(option: BriefOption, who: 'ticket' | 'blind_pass'): boolean {
  return option.proposed_by === who || option.proposed_by === 'both'
}

/** How many options each side put forward. */
function raisedBy(brief: Brief, who: 'ticket' | 'blind_pass'): number {
  return options(brief).filter(option => raised(option, who)).length
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
/**
 * The blind pass's state, read once.
 *
 * **Absent reads as `not_run`,** which is what the server derives for a brief recorded
 * before the field existed. Every test of the state goes through here: the three places
 * that each asked `blind_pass_state !== 'not_run'` were each separately true of
 * `undefined`, so a brief that recorded nothing drew the blind column, its count and its
 * ring as though a pass had run.
 */
function state(brief: Brief): BlindPassState {
  return brief.blind_pass_state ?? 'not_run'
}

function blindState(brief: Brief): { heading: string, says: string, tone: 'plain' | 'quiet' } {
  /*
   * **`?? 'not_run'`, and `ran` is a case rather than the default.**
   *
   * The switch fell through to "a blind pass ran" for anything it did not name — including
   * `undefined`, which is what four of the sample briefs carry, because they were recorded
   * before this field existed. The page then asserted that a session had handed the problem
   * to an independent pass when nothing of the sort had happened: the page's own doc says
   * these three states must not collapse, and the absent state was collapsing into the
   * strongest of the three. The server derives `not_run` for such a brief; absent is the
   * same statement and now reads the same way.
   */
  switch (state(brief)) {
    case 'not_run':
      return {
        /*
         * **The fourth of DEV-892's four wordings, read from where they are all declared.**
         * "No blind pass was run" and "none was recorded" are the two easiest sentences on
         * these pages to collapse into one, and they are opposite statements about how much
         * the option list has been tested. `NOTHING_HERE` is where all four live and
         * `test/steps.spec.ts` is what keeps them apart.
         */
        heading: NOTHING_HERE.blind_pass_not_run,
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
    case 'ran':
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

/**
 * The matrix's column ratios, the board's own — with no `repeat(0, …)` in them.
 *
 * **`repeat()` requires a positive integer**, so a brief that numbers no problems produced
 * `repeat(0, minmax(0, 1fr))`, which is invalid, and the browser dropped the whole
 * `grid-template-columns` declaration — taking the board's layout off the header row and
 * every option row with it. A brief written the old way has options and no numbered
 * problems, so this is the common case on the real server rather than an edge.
 */
function matrixTemplate(brief: Brief): string {
  const columns = problems(brief).length
  const middle = columns > 0 ? ` repeat(${columns}, minmax(0, 1fr))` : ''
  return `2.4fr 150px${middle} 110px`
}

/**
 * The second half of the empty page's sentence: what the step's state means for a reader.
 *
 * One sentence, and never the headline again. The headline is the state in three words;
 * this says what follows from it — whether to come back, or that there is nothing coming.
 *
 * Keyed off `absenceOf` rather than off the state, so the pair cannot disagree: one place
 * decides which absence this is, and this only words the consequence.
 */
const WHY: Record<Absence, string> = {
  being_worked_on: 'A session is writing them now.',
  not_written_yet: 'Nothing has written any.',
  none_recorded: 'The step finished without recording any.',
}

function whyNothingYet(layers: ReviewLayer[]): string {
  return WHY[absenceOf(layers, 'alternatives')]
}

/**
 * What heads a problem's column, in the narrow layout's own words.
 *
 * **The number and its label, or the number alone — never the sentence.** DEV-892's defect
 * 2: this matrix headed each problem column with the whole `was_wrong` line, which on a
 * real review wrapped ten lines deep over a column of three verdicts. `problems[].label` is
 * the two or three words a heading can be; it is optional, and a brief that recorded none
 * leaves the number doing the work with the sentence one click away in the fold below.
 *
 * Used for `columns` and for each cell's `data-label`, which is what the narrow layout
 * prints above a verdict — so the sentence is gone from there too, where it was the same
 * defect in a card instead of a column.
 */
function problemHeading(problem: BriefProblem): string {
  return problem.label ? `${problem.number} \u00b7 ${problem.label}` : String(problem.number)
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
      <!--
        **No options, and the page says which nothing this is — from step 3's own state.**
        DEV-892's defect 6: this read "No alternatives are recorded… Nothing has written any"
        while the trail two inches above it drew step 3 as running. A page that contradicts
        the record it is drawn from is worse than a page with nothing on it.
      -->
      <!--
          **And where an earlier round wrote it, this page's heading says so instead.**
          Nothing carries a brief between rounds, so a later round whose session wrote none
          draws nothing here — and "nothing has been written about this problem yet", over a
          pull request that has a brief one round back, is the ambiguity DEV-976 was raised
          to remove rather than a smaller version of it. `briefWrittenEarlier` answers null
          on a first round and wherever no earlier round wrote one, which is when the
          sentences below are the true ones.
        -->
      <ReviewStepPage
        v-if="!data.brief || !options(data.brief).length"
        :detail="data"
        step="alternatives"
        :headline="briefWrittenEarlier(data, 'alternatives')?.headline
          ?? sentence(nothingYet(data.layers, 'alternatives'))"
        :sub="briefWrittenEarlier(data, 'alternatives')?.sub
          ?? `Step 3 is where a session writes down the ways it considered solving the `
            + `problem, before it reads the diff. ${whyNothingYet(data.layers)}`"
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
              ...problems(data.brief).map(problem => problemHeading(problem)),
              '',
            ]"
            :template="matrixTemplate(data.brief)"
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
                role="columnheader"
                class="th"
              >
                Option
              </th>
              <th
                scope="col"
                role="columnheader"
                class="th"
              >
                Seen by
              </th>
              <!--
                The number, and the label when the brief wrote one. **Never `was_wrong`**:
                that is a line of prose, and as a heading over a column of three verdicts it
                wrapped ten lines deep on the review this was measured against. A column
                with no label is headed by its number, and every sentence is in the fold
                under the matrix.
              -->
              <th
                v-for="problem in problems(data.brief)"
                :key="problem.number"
                scope="col"
                role="columnheader"
                class="th th--marked"
              >
                <ProblemMark
                  :number="problem.number"
                  tone="wrong"
                  size="inline"
                  says="the problem this column is about"
                />
                <span v-if="problem.label">{{ problem.label }}</span>
              </th>
              <th
                scope="col"
                role="columnheader"
                class="th"
              >
                <span class="rd-visually-hidden">Open the reasoning</span>
              </th>
            </template>

            <template
              v-for="option in options(data.brief)"
              :key="option.key"
            >
              <tr
                class="shell"
                role="row"
              >
                <td
                  role="cell"
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
                        v-if="raised(option, 'ticket')"
                        tone="quiet"
                      >Ticket {{ option.key }}</DeskPill>
                      <DeskPill
                        v-if="raised(option, 'blind_pass')"
                        tone="info"
                      >Blind</DeskPill>
                      <span
                        v-if="!option.proposed_by"
                        class="rd-meta"
                      >not recorded</span>
                    </span>

                    <span
                      v-for="problem in problems(data.brief)"
                      :key="problem.number"
                      class="cell"
                      :data-label="problemHeading(problem)"
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
                        <TickedText :text="option.argument_for" />
                      </p>
                    </div>
                    <div class="reasoning-side">
                      <p class="rd-rubric">
                        Against
                      </p>
                      <p class="rd-recorded argument">
                        <TickedText :text="option.argument_against" />
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

          <!--
            **Every number said in full, one click away.** The headings carry two or three
            words each, so this is where the sentence lives — and it is on this page rather
            than only behind the link above, because a reader working down a column should
            not have to leave the matrix to find out what column 2 is. A brief that recorded
            no label needs it most: the heading is then the bare number.
          -->
          <details
            v-if="problems(data.brief).length"
            class="numbers"
          >
            <summary>What each number is, in full</summary>
            <ul class="said">
              <li
                v-for="problem in problems(data.brief)"
                :key="problem.number"
                class="says"
              >
                <ProblemMark
                  :number="problem.number"
                  tone="wrong"
                  size="inline"
                  says="the problem"
                />
                <span
                  v-if="problem.label"
                  class="says-label"
                >{{ problem.label }}</span>
                <span class="says-line"><TickedText :text="problem.was_wrong" /></span>
              </li>
            </ul>
          </details>
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
                <TickedText :text="built(data.brief)!.argument_for" />
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
                <TickedText :text="data.brief.blind_pass?.pick_why" />
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
            {{ count(data.brief.blind_pass.questions.length, 'thing') }} the problem
            statement did not say. Each one is cheaper to answer now than after the change
            is built.
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
              <span><TickedText :text="question" /></span>
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
              <!--
                **No count rather than a count of nought.** A brief recorded before
                `proposed_by` existed attributes nothing to either side, and "0 options"
                over a matrix of eight is a measurement that contradicts the page above it.
                Absent is not zero, which is the rule this app applies to every other
                figure it draws.
              -->
              <p
                v-if="raisedBy(data.brief, 'ticket')"
                class="card-title source-count"
              >
                {{ count(raisedBy(data.brief, 'ticket'), 'option') }}
              </p>
              <p
                v-else
                class="rd-note"
              >
                This brief does not record which options came from the ticket.
              </p>
              <p class="rd-note">
                Written before the work started. The option that was built was agreed there.
              </p>
            </DeskCard>

            <DeskCard
              :tone="state(data.brief) === 'not_run' ? 'pending' : 'plain'"
              class="card"
              :class="{ 'card--blind': state(data.brief) !== 'not_run' }"
            >
              <p
                class="rd-rubric"
                :class="{ 'rd-rubric--blind': state(data.brief) !== 'not_run' }"
              >
                {{ blindState(data.brief).heading }}
              </p>
              <p
                v-if="state(data.brief) !== 'not_run' && raisedBy(data.brief, 'blind_pass')"
                class="card-title source-count"
              >
                {{ count(raisedBy(data.brief, 'blind_pass'), 'option') }}
              </p>
              <p class="rd-note">
                {{ blindState(data.brief).says }}
              </p>
            </DeskCard>
          </div>

          <!--
            What the pass was given and what it looked up, verbatim, behind the same control.
            The first is long — a page of prose — and it is the only way to judge whether a
            blind pass was really blind, so it is on the page rather than linked to.

            **The second used to print open**, as a line beginning "It looked up:", and on a
            real review that line was a 91-word paragraph in a card beside another card. It
            is the same kind of thing as the first — the session's own account of its own
            pass — so it is behind the same kind of control.
          -->
          <details
            v-if="data.brief.blind_pass"
            class="given"
          >
            <summary>What the blind pass was given, word for word</summary>
            <p class="rd-recorded given-text">
              <TickedText :text="data.brief.blind_pass.given" />
            </p>
          </details>

          <details
            v-if="data.brief.blind_pass"
            class="given"
          >
            <summary>What it looked up, in its own words</summary>
            <p class="rd-recorded given-text">
              <TickedText :text="data.brief.blind_pass.looked_up" />
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

/* ------------------------------ What each number is -------------------------------- */

/*
 * The same control as the fold at the foot of the page, and deliberately: this page has one
 * way of saying "there is more of this, here". `.numbers` and `.given` share every rule
 * below.
 */
.said {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
  max-width: var(--rd-measure);
  min-width: 0;
}

.says {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 10px;
  align-items: baseline;
  min-width: 0;
}

.says-label {
  font-weight: 600;
}

.says-line {
  flex: 1 1 240px;
  min-width: 0;
  color: var(--rd-ink-dim);
  overflow-wrap: break-word;
}

/* ------------------------------- What it was given --------------------------------- */

.numbers summary,
.given summary {
  display: flex;
  align-items: center;
  min-height: var(--rd-control-min);
  color: var(--rd-accent);
  font-weight: 500;
  cursor: pointer;
}

.numbers summary:focus-visible,
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
