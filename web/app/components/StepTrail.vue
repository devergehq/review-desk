<script setup lang="ts">
import type { LayerFact, ReviewLayer, ReviewLayerName } from '~/types/desk'

/**
 * The seven steps of a review, in the two groups they actually fall into.
 *
 * ```
 *  THE PULL REQUEST · SETTLED ONCE            ROUND 1 · REPEATS EACH ROUND
 *  ━━━━━━━━  ━━━━━━━━━━━  ━━━━━━━━━━  ━━━━━  ──────────  ──────  ───────
 *  1 Lint    2 Understand 3 Alternat… 4 Sol… 5 Code rev… 6 Draft 7 Harvest
 *  passes…   3 problems   9 options,… Right… 17 finding… Waitin… Not yet
 * ```
 *
 * **The split is the point, and it is a fact about the review rather than a decoration.**
 * Steps 1 to 4 are settled once for the pull request; 5 to 7 run again every round. A
 * trail that drew seven equal steps would say a second round repeats the brief, which it
 * does not.
 *
 * ## The trail is a map, and the rest is a card
 *
 * **Two lines a step, and each of them is one line.** The step's number and name, then its
 * `detail`, ellipsised if it does not fit and never wrapped — whatever its length. That is
 * DEV-917, and it replaces DEV-892's two-line clamp and the "Read the rest" control under
 * it. The measurement behind both: on the review this was read against, a session wrote 34
 * words into a step's `detail` and the column is about 150 pixels wide, so the note printed
 * whole ran eleven lines deep and pushed the page's headline half a screen down. The clamp
 * stopped that and bought a new problem — opening the note in place moved everything below
 * it, every time a reader asked.
 *
 * **So the rest floats.** A step with something behind it draws its `detail` line as a
 * button, and activating it opens a [`FloatingCard`] anchored to the step: the step's
 * name, how long it took or when a harvest last read the review, and the facts as two
 * columns. The card is out of flow, so nothing on the
 * page moves when it opens — which is the acceptance criterion, measured in
 * `e2e/review-desk.spec.ts` by comparing positions either side of the click.
 *
 * **A step with nothing behind it draws no card and no control for one.** Its `detail`
 * line is a plain span. That is most steps of most reviews and every step of every review
 * recorded before the facts existed, so a trail of seven buttons promising nothing was
 * never an option.
 *
 * **Nothing behind it means no facts *and* no read time.** DEV-933 gave step 7 a second
 * thing to say — when a harvest last read the review — and the card is where it is said,
 * because the line is 154 pixels wide and its own words fill it. A harvest records a fact
 * every time, so in practice a harvested step has both; a step 7 written by hand through
 * `layer set` has only the read time, and a card with a head and no pairs is how it is
 * reachable. Without that the answer would exist only in a `title`, which a keyboard
 * cannot reach and a phone does not draw.
 *
 * **The step's name is still a link, and the card is a second control.** Five of the seven
 * steps have a page; a card and a link are two things, and a button inside a link is
 * neither valid markup nor clickable — which is also why DEV-892's control was a sibling of
 * the face. The link is the name, the button is the line under it.
 *
 * ## How long it took
 *
 * **On the card, not on the trail.** It was a third line under the name until DEV-917, and
 * a third line is what the one-line rule is against. `stepDurationLabel` owns the three
 * answers and they are unchanged: a figure for a step that has finished, "time not
 * recorded" for one the record holds no duration for — every finished step of a review from
 * before DEV-890 — and nothing at all for a step still going, because the figure would grow
 * while you read it. The review's totals stay under the trail, in `ReviewTimings`.
 *
 * ## Which steps are links
 *
 * **Five of the seven, and the trail is the only way to four of them.**
 *
 * Every row in the queue opens Understand — "when we click a review, we want to go to the
 * review, not to the draft", and in a page-per-step review the review starts at
 * Understand. That makes the trail load-bearing rather than decorative: if it did not
 * reach the draft and the findings, the queue would lead somewhere you could not leave.
 *
 * | Step | Goes to |
 * |---|---|
 * | 2, 3, 4 | the pages DEV-885 builds |
 * | 5 Code review | `/reviews/{id}/code-review`, which DEV-878 builds |
 * | 6 Draft | `/reviews/{id}/draft` |
 * | 1 Lint, 7 Harvest | nowhere — no page yet, and a link to a page that does not exist is worse than a label |
 *
 * **Step 6 is a link whatever the draft status says, and that changed in DEV-878.** It used
 * to be guarded on `draft_status`, because `GET …/draft` refuses when there is no draft row
 * and the draft screen then drew a blocking not-found panel carrying no trail and no way
 * back — the same dead end steps 1 and 7 are labels to avoid. The draft screen is inside
 * this shell now, so it carries the trail even when there is nothing to read, and the
 * reason for the guard is gone with the dead end.
 */
/*
 * **Resolved to the component, not named as a string.** `QueueRow` carries the measurement:
 * `:is="'NuxtLink'"` resolves to nothing in the static build, the element comes out unknown
 * and the link silently stops being a link. A string is fine for `span`, which is a real
 * tag; a component has to be the component.
 */
const NuxtLink = resolveComponent('NuxtLink')

const props = defineProps<{
  layers: ReviewLayer[]
  reviewId: number
  /** Which step's page this is. */
  active: ReviewLayerName
  /** Which round the second group is. */
  round: number
}>()

/**
 * The five steps that have somewhere to go.
 *
 * A suffix each, now that every one of them is a suffix: step 5 was the base route while
 * the one-page review was where the findings were, and `/reviews/{id}` is a redirect to
 * Understand since DEV-878 retired it.
 */
const PAGES: Partial<Record<ReviewLayerName, string>> = {
  understand: 'understand',
  alternatives: 'alternatives',
  solution_map: 'solution',
  code_review: 'code-review',
  draft: 'draft',
}

/**
 * The two groups, and the boundary between them.
 *
 * Four and three, which is where the pull request stops being settled once and the round
 * starts repeating. Named here rather than inline so the split has one statement.
 */
const SETTLED_THROUGH = 4

interface Step {
  number: number
  name: ReviewLayerName
  label: string
  /** The one line under the name: the store's own words, else the state, worded. */
  detail: string
  /** What the step came to, as pairs. Empty for a step that recorded none. */
  facts: LayerFact[]
  /** How long it took, or that nothing recorded it. `null` for a step still going. */
  duration: string | null
  /** When a harvest last read the review. `null` for every step but a running step 7. */
  read: string | null
  /**
   * The line's `title`: what it says, and the read time it has no room for.
   *
   * One field rather than the same ternary in both branches of the line — the line is a
   * button when the step has a card and a span when it does not, and a title written
   * twice is a title that can come to disagree with itself.
   */
  title: string
  /** Whether this step has a card: facts to show, or a read time to say. */
  opens: boolean
  to: string | null
  current: boolean
  tone: 'done' | 'current' | 'pending' | 'failed'
}

/**
 * The line's `title`: the step's own words, and the read time the line has no room for.
 *
 * A hover costs no layout, so what will not fit on the line is here as well as on the
 * card — measured in `harvestReadLabel`, which is why it is not on the line itself.
 */
function titleOf(said: string, read: string | null): string {
  return read ? `${said} — ${read}` : said
}

/**
 * The steps, in number order, whatever order the API served them in.
 *
 * Built from the layers the review has rather than from a list of seven written here: a
 * review whose record is missing a layer should draw six steps and no gap, and a layer the
 * contract adds should appear without this file being edited.
 */
const steps = computed<Step[]>(() =>
  [...props.layers]
    .sort((one, two) => one.number - two.number)
    .map((layer) => {
      const said = layerOutcome(layer)
      /*
       * **Read once.** The line, the title and the card all say this, and
       * `harvestReadLabel` defaults to the clock: called three times it would answer from
       * three readings of it, and the title could say "read just now" beside a card
       * saying "read 1 min ago".
       */
      const read = harvestReadLabel(layer)
      /*
       * `?? []` because the contract added `facts` after this response shape existed: a
       * review served by a build from before DEV-917 is missing the field rather than
       * wrong, and the sample fixtures are briefs recorded before it.
       */
      const facts = layer.facts ?? []

      return {
        number: layer.number,
        name: layer.name,
        label: layerLabel(layer.name),
        detail: said,
        facts,
        duration: stepDurationLabel(layer),
        read,
        title: titleOf(said, read),
        opens: facts.length > 0 || read != null,
        to: PAGES[layer.name] == null
          ? null
          : `/reviews/${props.reviewId}/${PAGES[layer.name]}`,
        current: layer.name === props.active,
        tone: layerTone(layer.state),
      }
    }),
)

/**
 * Which step's card a click has pinned, and which one a mouse is over.
 *
 * **Two states, because they are two acts.** A click pins a card open and it stays open
 * until something closes it; hovering opens one for as long as the pointer is there. One
 * flag for both would make a card opened by a click vanish when the pointer left it, which
 * is exactly the behaviour that makes hover-only menus unusable.
 *
 * One at a time either way, which is the rule every fold on these pages follows.
 */
const pinned = ref<number | null>(null)
const hovered = ref<number | null>(null)

/** Which card is open: what a click pinned, else what the pointer is over. */
const open = computed(() => pinned.value ?? hovered.value)

/**
 * The buttons, by step number, so Escape can put the focus back where it came from.
 *
 * A keyboard reader who pressed Escape and found the focus at the top of the document
 * would be the trail's whole length away from the step they were reading.
 */
const controls = ref<Record<number, HTMLButtonElement | null>>({})

function keep(number: number) {
  return (element: unknown) => {
    controls.value[number] = (element as HTMLButtonElement | null) ?? null
  }
}

function toggle(number: number) {
  pinned.value = pinned.value === number ? null : number
  /*
   * Clicking clears the hover, so a card closed by a second click does not spring back
   * open under the pointer that closed it.
   */
  hovered.value = null
}

function close() {
  pinned.value = null
  hovered.value = null
}

/**
 * The pointer arriving on a step, and leaving it.
 *
 * Only a step with a card opens — `step.opens`, the same test the control is drawn on.
 * A step with nothing behind it has no card, and setting the state to it would close
 * whatever a click had pinned for nothing.
 *
 * **The same test, and that is the whole of it.** This read `facts.length` until the
 * review of the commit that broadened the control to facts *or* a read time: the one step
 * the broadening was for drew a button, answered a click, and did nothing at all under a
 * pointer — the unreachable answer moved rather than fixed.
 */
function over(step: Step) {
  if (step.opens) hovered.value = step.number
}

function out(number: number) {
  if (hovered.value === number) hovered.value = null
}

function restore(number: number) {
  controls.value[number]?.focus()
}

const groups = computed(() => [
  {
    heading: 'The pull request · settled once',
    steps: steps.value.filter(step => step.number <= SETTLED_THROUGH),
  },
  {
    heading: `Round ${props.round} · repeats each round`,
    steps: steps.value.filter(step => step.number > SETTLED_THROUGH),
  },
])
</script>

<template>
  <nav
    class="trail"
    aria-label="The seven steps of this review"
  >
    <div
      v-for="group in groups"
      :key="group.heading"
      class="group"
      :style="{ '--span': group.steps.length }"
    >
      <h2 class="rd-rubric">
        {{ group.heading }}
      </h2>
      <ol
        class="steps"
        :style="{ '--columns': group.steps.length }"
      >
        <li
          v-for="step in group.steps"
          :key="step.number"
          class="step"
          @mouseenter="over(step)"
          @mouseleave="out(step.number)"
        >
          <component
            :is="step.to ? NuxtLink : 'span'"
            :to="step.to ?? undefined"
            class="face"
            :class="[`face--${step.tone}`, { 'face--here': step.current }]"
            :aria-current="step.current ? 'page' : undefined"
          >
            <span class="name">{{ step.number }} {{ step.label }}</span>
          </component>

          <!--
            The one line under the name, and the control that opens the card.

            **A button when the step has a card, a plain span when it has not.** A card is
            facts to show or — on step 7 alone — a read time to say. Both branches draw
            the same one ellipsised line, so the trail keeps its height either way —
            which is what makes the page's headline sit where it sits whatever a session
            wrote. Outside the face above, because that face is a link on five of the seven.
          -->
          <button
            v-if="step.opens"
            :ref="keep(step.number)"
            type="button"
            class="outcome outcome--opens"
            :class="{ 'outcome--open': open === step.number }"
            :title="step.title"
            :aria-expanded="open === step.number"
            :aria-controls="`step-facts-${step.number}`"
            @click="toggle(step.number)"
          >
            {{ step.detail }}
            <!--
              What the control opens, for a reader who cannot see that it is one. The
              facts branch counts them; step 7 with none says when it was last read,
              which is the whole of what its card holds.
            -->
            <span class="rd-visually-hidden">
              — <template v-if="step.facts.length">{{ count(step.facts.length, 'fact') }}
                about</template><template v-else>{{ step.read }}; opens</template>
              step {{ step.number }} {{ step.label }}</span>
          </button>
          <span
            v-else
            class="outcome"
            :title="step.title"
          >{{ step.detail }}</span>

          <FloatingCard
            :id="`step-facts-${step.number}`"
            :open="open === step.number"
            :side="step.number > SETTLED_THROUGH ? 'right' : 'left'"
            :label="`What step ${step.number} ${step.label} came to`"
            @close="close()"
            @escape="restore(step.number)"
          >
            <div class="card-head">
              <span class="card-name">{{ step.label }}</span>
              <!--
                The duration, where it was a third line on the trail until DEV-917.
                Absent for a step still going: `stepDurationLabel` answers null for one,
                and a figure that grew while it was read would not be an answer to how
                long the step took.
              -->
              <span
                v-if="step.duration"
                class="card-took"
              >{{ step.duration }}</span>
              <!--
                **And when a harvest last read the review**, which is step 7's and no
                other step's. It sits where the duration sits because the two never
                appear together: a duration is for a step that has finished, and this is
                for the one step that is `running` for as long as a pull request stays
                open. Why it is here rather than on the step's line is measured in
                `harvestReadLabel` — the line is 154 pixels wide and the detail fills it.
              -->
              <span
                v-else-if="step.read"
                class="card-took"
              >{{ step.read }}</span>
            </div>
            <!--
              Label then value, as a description list: that is what two columns of pairs
              are, and it is what lets a screen reader read "Problems, 3" rather than four
              cells of a table nobody laid out.
            -->
            <dl
              v-if="step.facts.length"
              class="facts"
            >
              <template
                v-for="(fact, index) in step.facts"
                :key="index"
              >
                <dt class="fact-label">
                  {{ fact.label }}
                </dt>
                <dd class="fact-value">
                  {{ fact.value }}
                </dd>
              </template>
            </dl>
          </FloatingCard>
        </li>
      </ol>
    </div>
  </nav>
</template>

<style scoped>
.trail {
  display: flex;
  gap: 24px;
  min-width: 0;
}

/*
 * Each group takes as many share of the width as it has steps, so all seven steps come out
 * the same width — which is the board's single seven-column grid, kept while the two
 * rubrics stay over the steps they are about.
 */
.group {
  display: flex;
  flex: var(--span) 1 0;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.steps {
  display: grid;
  grid-template-columns: repeat(var(--columns), minmax(0, 1fr));
  gap: 12px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/*
 * **`position: relative`, which is what the card hangs from.** `FloatingCard` is
 * `position: absolute` and says nothing about its anchor on purpose; this is the anchor.
 * Without it the card would position itself against the page and arrive nowhere near the
 * step it is about.
 */
.step {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/*
 * The 3px rule over each step is what says where you are. Ink for the step you are on,
 * mid-grey for one that has run, a lighter grey for one that has not.
 */
.face {
  display: flex;
  flex-direction: column;
  padding: 8px 8px 1px;
  border-top: 3px solid var(--rd-line-strong);
  border-radius: 0 0 var(--rd-radius-sm) var(--rd-radius-sm);
  color: var(--rd-ink);
  text-decoration: none;
  min-width: 0;
}

.face--done {
  border-top-color: var(--rd-control-line);
}

.face--failed {
  border-top-color: var(--rd-warn-line);
}

/*
 * A step that is running or waiting on the developer. The accent, which is the colour this
 * app uses for a thing to act on everywhere else.
 *
 * **Named apart from the step you are on, because the two collided.** `layerTone` answers
 * `current` for `waiting_on_you`, and the modifier for the active page was also
 * `face--current` — so on every one of these pages step 6 was drawn exactly like step 2,
 * and the trail said you were on two steps at once. They are different facts: one is about
 * the review, the other about the browser.
 */
.face--current {
  border-top-color: var(--rd-accent);
}

.face--here {
  border-top-color: var(--rd-ink);
}

/*
 * The step you are on is washed whole — the face and the line under it — so the two
 * elements read as one step rather than as a label with a button stuck to it.
 */
.step:has(.face--here) {
  background: var(--rd-ground);
  border-radius: 0 0 var(--rd-radius-sm) var(--rd-radius-sm);
}

a.face:hover {
  color: var(--rd-ink);
}

a.face:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

/*
 * **One line, and never two.** The name is the shorter half of the pair and is clipped by
 * the same rule as the detail: "5 Code review" fits the column at every width the trail is
 * drawn at, and a label the contract adds later should push the page's headline nowhere.
 */
.name {
  font-weight: 500;
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.face--here .name {
  font-weight: 600;
}

/* A link's name is underlined on hover, so the step reads as somewhere to go. */
a.face:hover .name {
  text-decoration: underline;
}

/*
 * **The one line DEV-917 is about: ellipsised, never wrapped, whatever the length.**
 *
 * `text-overflow` needs all three of these declarations and the column needs the fourth:
 * `nowrap` so there is an overflow to clip, `hidden` so it is clipped, `ellipsis` so the
 * clip is marked, and `min-width: 0` so the flex item is allowed to be narrower than its
 * text — without which the step widens to fit 34 words and the grid scrolls sideways.
 *
 * The whole string is in the document either way, so a screen reader reads all of it and
 * only the drawing is cut. `title` is what a mouse reader gets.
 */
.outcome {
  /*
   * **44px of hit area for one 13px line, which is this app's floor for anything you
   * press.** The line is the control that opens the card, so it is a target as well as a
   * label, and `test/contrast.spec.ts` holds every component with a pointer cursor to
   * `--rd-control-min`. Flex and centred rather than padded, so the figure stays on one
   * line and the height is the same on the steps whose line is a plain span — a row of
   * steps whose text sat at two different heights would read as two kinds of step.
   */
  display: flex;
  align-items: center;
  min-height: var(--rd-control-min);
  min-width: 0;
  padding: 0 8px 4px;
  font-size: 13px;
  color: var(--rd-ink-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: left;
}

/* The same line, when it is the control that opens the card. */
.outcome--opens {
  border: 0;
  background: none;
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.outcome--opens:hover,
.outcome--open {
  color: var(--rd-ink);
  text-decoration: underline;
  text-decoration-style: dotted;
  text-underline-offset: 3px;
}

.outcome--opens:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

/* ---------------------------------- on the card ---------------------------------- */

.card-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.card-name {
  font-weight: 600;
}

.card-took {
  color: var(--rd-ink-muted);
  font-variant-numeric: tabular-nums;
}

/*
 * Label then value. `auto 1fr` so the labels take what they need and the values take the
 * rest — a fact whose value is "6: 4 should, 1 question, 1 nit" wraps in its own column
 * rather than widening the card.
 */
.facts {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 4px 14px;
  margin: 0;
}

.fact-label {
  color: var(--rd-ink-muted);
}

.fact-value {
  margin: 0;
  overflow-wrap: anywhere;
}

/*
 * **One column of facts where the card is as narrow as its step.** `FloatingCard` takes the
 * anchor's width under 680px, because a 330px card hanging off the trail's right-hand
 * column is wider than the page there; two columns of pairs inside 190px would be a label
 * and a value both broken across three lines each.
 */
@media (max-width: 680px) {
  .facts {
    grid-template-columns: minmax(0, 1fr);
    gap: 0 0;
  }

  .fact-label {
    margin-top: 6px;
    font-size: 13px;
  }
}

/*
 * Seven columns at a phone's width is 55px a step, which fits no word. The two groups
 * stack, and each becomes two columns.
 */
@media (max-width: 860px) {
  .trail {
    flex-direction: column;
    gap: 16px;
  }

  .steps {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

/*
 * Two columns at a phone's width, not one. Seven steps in a single column is 360px of
 * trail before the headline, which puts the thing the page is about below the fold on the
 * device with the least of it — and two columns at 390px still fits "5 Code review" over
 * one line of what it came to.
 */
@media (max-width: 430px) {
  .steps {
    gap: 8px;
  }
}
</style>
