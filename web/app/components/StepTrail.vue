<script setup lang="ts">
import type { ReviewLayer, ReviewLayerName } from '~/types/desk'

/**
 * The seven steps of a review, in the two groups they actually fall into.
 *
 * ```
 *  THE PULL REQUEST · SETTLED ONCE            ROUND 1 · REPEATS EACH ROUND
 *  ━━━━━━━━  ━━━━━━━━━━  ━━━━━━━━━━━  ━━━━━━  ──────────  ──────  ──────
 *  1 Lint    2 Understand 3 Alternat…  4 Solu… 5 Code rev… 6 Draft 7 Harvest
 *  passed    Brief settl… 4 options…   Right…  9 findings… Waiti…  Not yet
 * ```
 *
 * **The split is the point, and it is a fact about the review rather than a decoration.**
 * Steps 1 to 4 are settled once for the pull request; 5 to 7 run again every round. A
 * trail that drew seven equal steps would say a second round repeats the brief, which it
 * does not.
 *
 * Each step shows **its outcome in a few words**, from `layer.detail` — the store's own
 * words — falling back to its state. `layerOutcome` owns that.
 *
 * ## How long it took
 *
 * Under the outcome, for a step that has finished: "17 min", summed over every pass the
 * step made, so a draft written twice reports both. A step the record holds no duration
 * for says "time not recorded" rather than drawing a nought — which is every finished step
 * of a review from before DEV-890, and the distinction the whole change rests on.
 * `stepDurationLabel` owns the three answers; a step still going shows none, because the
 * figure would grow while you read it.
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
 * reason for the guard is gone with the dead end. A review whose session has not written a
 * draft says so on step 6's own page, which is a better answer than a step that cannot be
 * pressed.
 *
 * The board draws only three links, and drew them against a queue that landed elsewhere.
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
  /*
   * **There is no `draftStatus` prop any more.** It existed to decide whether step 6 was a
   * link, and nothing decides that now: the draft screen is inside this shell, so it is
   * never the dead end the guard was for. A prop every caller passes and nothing reads is
   * a claim about what this component needs that is not true.
   */
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

interface Step {
  number: number
  name: ReviewLayerName
  label: string
  outcome: string
  /** How long it took, or that nothing recorded it. `null` for a step still going. */
  duration: string | null
  to: string | null
  current: boolean
  tone: 'done' | 'current' | 'pending' | 'failed'
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
    .map(layer => ({
      number: layer.number,
      name: layer.name,
      label: layerLabel(layer.name),
      outcome: layerOutcome(layer),
      duration: stepDurationLabel(layer),
      to: PAGES[layer.name] == null
        ? null
        : `/reviews/${props.reviewId}/${PAGES[layer.name]}`,
      current: layer.name === props.active,
      tone: layerTone(layer.state),
    })),
)

/**
 * The two groups, and the boundary between them.
 *
 * Four and three, which is where the pull request stops being settled once and the round
 * starts repeating. Named here rather than inline so the split has one statement.
 */
const SETTLED_THROUGH = 4

const groups = computed(() => [
  {
    heading: 'The pull request \u00b7 settled once',
    steps: steps.value.filter(step => step.number <= SETTLED_THROUGH),
  },
  {
    heading: `Round ${props.round} \u00b7 repeats each round`,
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
        >
          <component
            :is="step.to ? NuxtLink : 'span'"
            :to="step.to ?? undefined"
            class="face"
            :class="[`face--${step.tone}`, { 'face--here': step.current }]"
            :aria-current="step.current ? 'page' : undefined"
          >
            <span class="name">{{ step.number }} {{ step.label }}</span>
            <span class="outcome">{{ step.outcome }}</span>
            <span
              v-if="step.duration"
              class="took"
            >{{ step.duration }}</span>
          </component>
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

.step {
  min-width: 0;
}

/*
 * The 3px rule over each step is what says where you are. Ink for the step you are on,
 * mid-grey for one that has run, a lighter grey for one that has not.
 */
.face {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-height: 52px;
  padding: 8px;
  border-top: 3px solid var(--rd-line-strong);
  border-radius: 0 0 var(--rd-radius-sm) var(--rd-radius-sm);
  color: var(--rd-ink);
  text-decoration: none;
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
  background: var(--rd-ground);
}

a.face:hover {
  background: var(--rd-ground);
  color: var(--rd-ink);
}

a.face:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.name {
  font-weight: 500;
  font-size: 14px;
}

.face--here .name {
  font-weight: 600;
}

/* A link's name is underlined on hover, so the step reads as somewhere to go. */
a.face:hover .name {
  text-decoration: underline;
}

.outcome {
  font-size: 13px;
  color: var(--rd-ink-muted);
  overflow-wrap: break-word;
}

/*
 * The duration, under the outcome. It is the second thing you want off a
 * step — what it did, then how long it took — and at 12px seven of them across the trail
 * read as a column of figures rather than as more words.
 *
 * The same ink as the outcome, and smaller: `ink-faint` is for display sizes only — it is
 * 3.19:1 on white and clears nothing under 24px — and `ink-dim` is *darker* than the
 * outcome above it, which would put the louder colour on the quieter fact. Size does the
 * separating, and `test/contrast.spec.ts` holds this ink to 4.5:1 on all three
 * backgrounds.
 */
.took {
  font-size: 12px;
  color: var(--rd-ink-muted);
  font-variant-numeric: tabular-nums;
}

/*
 * Seven columns at a phone's width is 55px a step, which fits no word. The two groups
 * stack, and each becomes two columns and then one.
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
 * "13 findings, 1 blocker".
 */
@media (max-width: 430px) {
  .steps {
    gap: 8px;
  }
}
</style>
