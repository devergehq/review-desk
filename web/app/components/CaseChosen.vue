<script setup lang="ts">
import type {
  BlindPassState,
  Brief,
  BriefOption,
  BriefOptionVerdict,
  BriefProblem,
  ReviewLayer,
} from '~/types/desk'
import type { Absence } from '~/utils/steps'

/**
 * Band 2 of the case page — **what was chosen**.
 *
 * ```
 *  WHAT WAS CHOSEN · 8 OPTIONS
 *        OPTION                                       1 2 3 4     WHO PUT IT UP
 *   A    Attach the credentials once in the config    ● ● ○ ○     ticket · blind
 *   B    Route the portal's own requests…    Built    ● ● ● ◐     ticket · blind
 *   D    Trade the credentials for a cookie   Blind   ● ● ● ◐     blind
 *  ● fixes it  ◐ partly  ○ leaves it  – not assessed · the columns are the problems above
 * ```
 *
 * **Eight options are eight rows, and a verdict is a dot.** The matrix drew each verdict as
 * a word — "Fixed", "Not assessed" — which made every problem column as wide as its widest
 * word: on review #9639 that is eight rows of four words each, and the matrix took the
 * width of the page. As dots a row is one line a reader scans across, and the legend under
 * it says what the four shapes are. `VerdictCell`'s `compact` keeps each word in the
 * markup, visually hidden, so nothing depends on telling four shapes apart.
 *
 * **The two cards are gone, and what they said is on the rows.** Alternatives drew "Built"
 * and "The blind pass would pick" as a pair of cards under the matrix, which is the same two
 * facts the matrix already marks — and both are now on the page's opening answer line as
 * well. The one thing the cards carried that the rows cannot is `pick_why`, which is a
 * sentence and is drawn as one, under the legend.
 *
 * ## `not_assessed` is a verdict
 *
 * DEV-885's comment, and the one thing about this band that is easy to get wrong:
 * "`not_assessed` is a stored verdict, not a missing one. Draw it from the value. A missing
 * verdict is a different state and should not look the same." An option nothing scored
 * against the numbered problems and a brief that forgot to score one are different
 * statements; `VerdictCell` draws them differently and this passes the value through.
 */
const props = defineProps<{
  brief: Brief
  /** The review's steps, for the one sentence this band says about step 3's own state. */
  layers: ReviewLayer[]
}>()

/**
 * The four shapes, as the legend draws them.
 *
 * **A legend's verdict is a sample, not a record**, which is why it is declared here and
 * not fabricated in the template: `VerdictCell` draws a shape from the value, so the only
 * honest way to show a reader what each shape means is to hand it each value. `problem: 1`
 * is a number the object needs and the legend never shows.
 */
const LEGEND: Array<{ verdict: BriefOptionVerdict, says: string }> = [
  { verdict: { problem: 1, verdict: 'fixed', why: null }, says: 'fixes it' },
  { verdict: { problem: 1, verdict: 'partly', why: null }, says: 'partly' },
  { verdict: { problem: 1, verdict: 'stays', why: null }, says: 'leaves it' },
  { verdict: { problem: 1, verdict: 'not_assessed', why: null }, says: 'not assessed' },
]

/** Which option's reasoning is open, by key. One at a time, as on the board. */
const openOption = ref<string | null>(null)

function toggleOption(key: string) {
  openOption.value = openOption.value === key ? null : key
}

const options = computed(() => optionsOf(props.brief))
const problems = computed(() => problemsOf(props.brief))
const pick = computed(() => blindPick(props.brief))

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

/**
 * Whether one side put this option forward. `both` counts for each, because it was.
 *
 * **Asked positively, which it was not.** The two pills were drawn on
 * `proposed_by !== 'blind_pass'` and `proposed_by !== 'ticket'`, and both of those are true
 * when the field is absent — so every option on a brief recorded before `proposed_by`
 * existed wore *both* badges and claimed two independent sources had arrived at it.
 */
function raised(option: BriefOption, who: 'ticket' | 'blind_pass'): boolean {
  return option.proposed_by === who || option.proposed_by === 'both'
}

function raisedBy(who: 'ticket' | 'blind_pass'): number {
  return options.value.filter(option => raised(option, who)).length
}

/** An option the blind pass found and the ticket did not — what a blind pass is run for. */
function onlyBlind(option: BriefOption): boolean {
  return option.proposed_by === 'blind_pass'
}

/**
 * The blind pass's state, read once.
 *
 * **Absent reads as `not_run`,** which is what the server derives for a brief recorded
 * before the field existed. Every test of the state goes through here: the three places
 * that each asked `blind_pass_state !== 'not_run'` were each separately true of
 * `undefined`, so a brief that recorded nothing drew the blind column, its count and its
 * ring as though a pass had run.
 */
const state = computed<BlindPassState>(() => props.brief.blind_pass_state ?? 'not_run')

/**
 * What to say about the blind pass, in its own words for each of the three states.
 *
 * **Three wordings, and none of them reads like another** — a pass that did not run and a
 * pass that found nothing new are the two easiest to collapse into one grey sentence, and
 * they mean opposite things about how much the option list has been tested.
 */
const blindState = computed<{ heading: string, says: string }>(() => {
  switch (state.value) {
    case 'not_run':
      return {
        heading: NOTHING_HERE.blind_pass_not_run,
        says:
          'Nothing has looked at this problem without also seeing the answer, so the '
          + 'options above are the ones the ticket already contained. Review Desk runs no '
          + 'blind pass; a session reports one.',
      }
    case 'ran_and_found_nothing':
      return {
        heading: 'A blind pass ran and found nothing the ticket had missed',
        says:
          'A session reported handing this problem to a pass that saw no ticket, no pull '
          + 'request and no code. It came back with nothing the ticket did not already '
          + 'have — which is a result about the option list, not an absence of one.',
      }
    case 'ran':
    default:
      return {
        heading: 'A blind pass ran',
        says:
          'A session reported handing this problem to a pass that saw no ticket, no pull '
          + 'request and no code. What it put forward is marked above.',
      }
  }
})

/**
 * What follows from step 3's state, as the sentence after the phrase.
 *
 * Keyed off `absenceOf` rather than off the state, so the pair cannot disagree: one place
 * decides which absence this is, and this only words the consequence.
 */
const WHY: Record<Absence, string> = {
  being_worked_on: 'A session is writing them now.',
  not_written_yet: 'Nothing has written any.',
  none_recorded: 'The step finished without recording any.',
}

/**
 * How the sentence under the legend opens, as one string.
 *
 * **Composed here rather than in the template**, because a ternary written across two lines
 * inside an interpolation puts the newline and its indentation into the text — and a test
 * matching the sentence then matches nothing while the page looks right.
 */
const pickLead = computed(() => {
  const chosen = pick.value?.chosen
  return chosen
    ? 'The blind pass picked the same option'
    : `The blind pass would pick ${pick.value?.key}`
})

/** What heads a problem's column: the number, and the label as its hidden name. */
function columnName(problem: BriefProblem): string {
  return problem.label ? `${problem.number} · ${problem.label}` : `Problem ${problem.number}`
}

/**
 * What the fold holds, named piece by piece.
 *
 * The pass's own account of itself — what it was given, what it says it read, and what it
 * would have needed to know. **Review Desk ran nothing and can check none of it**, which is
 * the reason the verbatim text is on the page rather than summarised: it is the only way to
 * judge whether a blind pass was really blind.
 */
const fold = computed(() => {
  const said: string[] = []
  if (options.value.length) said.push('the reasoning for and against each option')
  if (props.brief.blind_pass) said.push('what the blind pass was given and what it read')
  const questions = props.brief.blind_pass?.questions ?? []
  if (questions.length) said.push(`the ${count(questions.length, 'thing')} it needed to know`)
  return said
})
</script>

<template>
  <section
    id="what-was-chosen"
    class="band"
  >
    <h2 class="rd-display-lead">
      What was chosen<template v-if="options.length">
        ·
        {{ count(options.length, 'option') }}
      </template>
    </h2>

    <!--
      **No options, and the band says which nothing this is — from step 3's own state.**
      DEV-892's defect 6: the page read "No alternatives are recorded… Nothing has written
      any" while the trail two inches above it drew step 3 as running. A page that
      contradicts the record it is drawn from is worse than a page with nothing on it.
    -->
    <DeskNotice
      v-if="!options.length"
      tone="plain"
    >
      <strong>{{ sentence(nothingYet(layers, 'alternatives')) }}</strong>
      Step 3 is where a session writes down the ways it considered solving the problem,
      before it reads the diff. {{ WHY[absenceOf(layers, 'alternatives')] }}
    </DeskNotice>

    <template v-else>
      <div
        class="matrix"
        :class="{ 'matrix--unnumbered': !problems.length }"
        :style="{ '--problems': problems.length }"
      >
        <!--
          The column captions. `aria-hidden`, because a row is a button and not a table cell
          — so the names a screen reader needs are inside each cell, where they belong to the
          value rather than to a position in a grid.
        -->
        <div
          class="caption-row"
          aria-hidden="true"
        >
          <span />
          <span class="rd-rubric">Option</span>
          <span
            v-if="problems.length"
            class="verdicts"
          >
            <!--
              The numbers, over the dots they are the columns of. The number alone and never
              `was_wrong`: that is a line of prose, and as a heading over a column of
              verdicts it wrapped ten lines deep on the review this was measured against.
              What each number is, in full, is band 1 directly above.
            -->
            <ProblemMark
              v-for="problem in problems"
              :key="problem.number"
              :number="problem.number"
              tone="wrong"
              size="inline"
              says="the problem this column is about"
            />
          </span>
          <span class="rd-rubric">Who put it up</span>
          <span />
        </div>

        <template
          v-for="option in options"
          :key="option.key"
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
            <span class="key">{{ option.key }}</span>

            <span
              class="cell title"
              data-label="Option"
            >
              <span class="words">{{ option.title }}</span>
              <DeskPill
                v-if="option.chosen"
                tone="go"
              >Built</DeskPill>
              <!--
                **The pick, marked on the row it is about.** It was a card under the matrix
                saying the same thing twice — once as the option's letter and title, once as
                "Not what was built". Here the row is the option, so the marker is a pill on
                it; the sentence that made the card worth reading is under the legend below.
              -->
              <DeskPill
                v-if="pick && pick.key === option.key && !option.chosen"
                tone="info-strong"
              >Blind pass would pick</DeskPill>
              <DeskPill
                v-else-if="onlyBlind(option) && !option.chosen"
                tone="info"
              >Not on the ticket</DeskPill>
            </span>

            <span
              v-if="problems.length"
              class="verdicts"
            >
              <span
                v-for="problem in problems"
                :key="problem.number"
                class="verdict"
                :data-label="columnName(problem)"
              >
                <span class="rd-visually-hidden">{{ columnName(problem) }}: </span>
                <VerdictCell
                  :verdict="verdictOn(option, problem.number)"
                  compact
                />
              </span>
            </span>

            <!--
              Who put it forward. `both` draws both pills, because the point of the column is
              that two sides arrived at the same option independently.
            -->
            <span
              class="cell seen"
              data-label="Who put it up"
            >
              <DeskPill
                v-if="raised(option, 'ticket')"
                tone="quiet"
              >Ticket</DeskPill>
              <DeskPill
                v-if="raised(option, 'blind_pass')"
                tone="info"
              >Blind</DeskPill>
              <span
                v-if="!option.proposed_by"
                class="rd-meta"
              >not recorded</span>
            </span>
          </OpenRow>

          <!--
            The row's detail: the reasoning both ways, and what this option does about each
            numbered problem in the brief's own words. **The `why` lines live here**, because
            the cell they were in is 90 pixels wide once a verdict is a dot — and they are
            the half of the matrix a reader is reading for.
          -->
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
            <div
              v-if="problems.length"
              class="reasoning-side reasoning-side--whole"
            >
              <p class="rd-rubric">
                Against each problem
              </p>
              <ul class="whys">
                <li
                  v-for="problem in problems"
                  :key="problem.number"
                  class="why"
                >
                  <ProblemTag
                    :number="problem.number"
                    :label="problem.label"
                    tone="wrong"
                  />
                  <VerdictCell :verdict="verdictOn(option, problem.number)" />
                </li>
              </ul>
            </div>
          </div>
        </template>
        <span class="end" />
      </div>

      <!--
        **The legend, which is what makes a dot readable.** Four shapes and what each one is,
        and a line saying the columns are the problems above. On the exploration artifact the
        dots went up with no legend at all and were meaningless until it was rendered — the
        lesson being that a key you can infer while writing the markup is not a key.
      -->
      <p
        v-if="problems.length"
        class="rd-note legend"
      >
        <span
          v-for="shape in LEGEND"
          :key="shape.verdict.verdict"
          class="legend-item"
        >
          <VerdictCell
            :verdict="shape.verdict"
            compact
          />
          {{ shape.says }}
        </span>
        <span class="legend-words">The {{ count(problems.length, 'column') }} are the
          {{ count(problems.length, 'problem') }} above, in order.</span>
      </p>

      <!--
        Why an independent pass would pick what it picked. One sentence, and the only thing
        the two cards under the old matrix carried that a row cannot.
      -->
      <p
        v-if="pick && brief.blind_pass?.pick_why"
        class="pick-why"
      >
        <strong>{{ pickLead }}:</strong>
        <TickedText :text="brief.blind_pass.pick_why" />
      </p>

      <!--
        Where the options came from, as one line of two counts. **No count rather than a
        count of nought**: a brief recorded before `proposed_by` existed attributes nothing
        to either side, and "0 options" over a matrix of eight is a measurement that
        contradicts the page above it.
      -->
      <p class="rd-note legend">
        <span v-if="raisedBy('ticket')"><b>{{ raisedBy('ticket') }}</b> from the
          ticket</span>
        <span v-if="state !== 'not_run' && raisedBy('blind_pass')"><b>{{
          raisedBy('blind_pass') }}</b> from the blind pass</span>
        <span v-if="!raisedBy('ticket') && !raisedBy('blind_pass')">This brief does not
          record which options came from where.</span>
      </p>

      <!--
        **The blind pass's own state, in one of three wordings, always drawn.** It was two
        cards — one for the ticket, one for the pass — and the pass's card is the only place
        these three sentences were said. A pass that never ran and a pass that found nothing
        new are the two easiest to collapse into one grey line, and they mean opposite things
        about how far the option list has been tested, so none of the three reads like
        another.
      -->
      <p class="rd-note legend blind-state">
        <span class="legend-words"><strong>{{ blindState.heading }}.</strong>
          {{ blindState.says }}</span>
      </p>

      <details
        v-if="fold.length"
        class="fold"
      >
        <summary>{{ fold.join(' · ') }}</summary>

        <div class="reference">
          <!--
            The reasoning for and against, every option at once. It is also one click away on
            each row; this is the way to read all of it, which is what a reader comparing two
            options is doing. **One list, not a second copy**: the row draws the same two
            fields, and nothing here is a rewording of them.
          -->
          <div class="piece">
            <h3 class="rd-rubric">
              The reasoning, option by option
            </h3>
            <div
              v-for="option in options"
              :key="option.key"
              class="argued"
            >
              <p class="argued-head">
                <span class="key">{{ option.key }}</span>
                <span class="words">{{ option.title }}</span>
              </p>
              <p class="rd-recorded argument">
                <strong>For.</strong> <TickedText :text="option.argument_for" />
              </p>
              <p class="rd-recorded argument">
                <strong>Against.</strong> <TickedText :text="option.argument_against" />
              </p>
            </div>
          </div>

          <template v-if="brief.blind_pass">
            <div class="piece">
              <h3 class="rd-rubric">
                What the blind pass was given, word for word
              </h3>
              <p class="rd-recorded given">
                <TickedText :text="brief.blind_pass.given" />
              </p>
            </div>
            <div class="piece">
              <h3 class="rd-rubric">
                What it looked up, in its own words
              </h3>
              <p class="rd-recorded given">
                <TickedText :text="brief.blind_pass.looked_up" />
              </p>
            </div>
          </template>

          <div
            v-if="brief.blind_pass?.questions?.length"
            class="piece"
          >
            <h3 class="rd-rubric">
              What the blind pass needed to know
            </h3>
            <p class="rd-note">
              {{ count(brief.blind_pass.questions.length, 'thing') }} the problem statement
              did not say. Each one is cheaper to answer now than after the change is built.
            </p>
            <ul class="questions">
              <li
                v-for="(question, index) in brief.blind_pass.questions"
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
          </div>
        </div>
      </details>
    </template>
  </section>
</template>

<style scoped>
.band {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.matrix {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/*
 * **Five tracks, and the dots are one of them.** The verdicts were a track each, which made
 * every problem column as wide as the widest word in it; as one track holding a fixed grid
 * of discs, the row's width is the title's and the dots line up with the numbers above them
 * because both inner grids are `repeat(var(--problems), 22px)`.
 */
.caption-row,
.option {
  display: grid;
  grid-template-columns:
    26px minmax(0, 1fr) calc(var(--problems) * 22px) 132px 92px;
  gap: 8px 18px;
  min-width: 0;
}

/*
 * **A brief that numbers no problems has no verdicts column at all**, and a four-track row
 * rather than a five-track one with an empty track in it. A gap applies to an empty track
 * as much as to a full one, so `calc(0 * 22px)` leaves an 18-pixel hole where the dots would
 * be. This is the same defect the old matrix had in a louder form — `repeat(0, …)` is
 * invalid and a browser drops the whole declaration, taking the layout off every row.
 */
.matrix--unnumbered .caption-row,
.matrix--unnumbered .option {
  grid-template-columns: 26px minmax(0, 1fr) 132px 92px;
}

.caption-row {
  padding: 0 16px 8px;
  align-items: end;
}

.option {
  align-items: center;
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

.key {
  flex: none;
  font-family: var(--rd-font-mono);
  font-size: 13px;
  color: var(--rd-ink-muted);
}

.title {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  align-items: center;
}

.words {
  font-weight: 500;
  min-width: 0;
}

.option--built .words,
.option--blind .words {
  font-weight: 600;
}

/*
 * The dots, and the numbers over them: one grid, one declaration, so a column of discs sits
 * under the number it answers for.
 */
.verdicts {
  display: grid;
  grid-template-columns: repeat(var(--problems), 22px);
  align-items: center;
  justify-items: center;
  min-width: 0;
}

.seen {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.end {
  border-top: 1px solid var(--rd-line);
}

/* ---------------------------------- An opened row ---------------------------------- */

.reasoning {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 20px 28px;
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

.reasoning-side--whole {
  grid-column: 1 / -1;
}

.reasoning p {
  margin: 0;
}

.argument {
  color: var(--rd-ink-dim);
}

.whys {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

.why {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  align-items: baseline;
  min-width: 0;
}

/* ----------------------------------- The legend ------------------------------------ */

.legend {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 18px;
  margin: 0;
  max-width: var(--rd-measure);
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}

.legend-words {
  flex: 1 1 240px;
}

.legend b {
  font-variant-numeric: tabular-nums;
}

.pick-why {
  margin: 0;
  max-width: var(--rd-measure);
  color: var(--rd-question-ink);
}

/* ------------------------------------ The fold ------------------------------------- */

.fold > summary {
  display: flex;
  align-items: center;
  min-height: var(--rd-control-min);
  color: var(--rd-accent);
  font-weight: 500;
  cursor: pointer;
}

.fold > summary:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.reference {
  display: flex;
  flex-direction: column;
  gap: 32px;
  padding-top: 14px;
  min-width: 0;
}

.piece {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.argued {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 0;
  border-top: 1px solid var(--rd-line);
  min-width: 0;
}

.argued-head {
  display: flex;
  gap: 10px;
  align-items: baseline;
  margin: 0;
  min-width: 0;
}

.argued p {
  margin: 0;
}

.given {
  margin: 0;
  padding: 16px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
  color: var(--rd-ink-dim);
}

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

/*
 * Under 860px the matrix becomes one card per option: the row is a single column, the dots
 * become a row of named verdicts, and every cell says which column it was.
 */
@media (max-width: 860px) {
  .caption-row {
    display: none;
  }

  .option {
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
    padding: 16px;
    align-items: start;
  }

  .key {
    font-size: 14px;
  }

  /*
   * The dots unwrap into labelled rows. A disc with no number over it says nothing, and the
   * numbers are what the caption row carried — so at this width each verdict carries its
   * own.
   */
  .verdicts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    justify-content: flex-start;
  }

  .verdict::before {
    content: attr(data-label);
    margin-right: 6px;
    font-size: 13px
  }

  .verdict {
    display: flex;
    align-items: center;
    color: var(--rd-ink-muted);
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
