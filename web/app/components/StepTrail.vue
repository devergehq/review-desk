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
 * ## Which steps are links
 *
 * Understand, Alternatives and Solution, which are the three pages DEV-885 builds. The
 * board makes exactly these three links and leaves the other four as labels, and that is
 * the right behaviour rather than a limitation of the mockup: steps 1, 5 and 7 have no page
 * until DEV-878 builds them, and a link to a page that does not exist is worse than a
 * label. **Step 6 is deliberately not a link either**, even though `/reviews/{id}/draft`
 * exists: that screen carries no trail, so following the link would be a one-way trip out
 * of a navigation whose whole purpose is moving between steps freely.
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

/** The three steps with a page, and where each one is. */
const PAGES: Partial<Record<ReviewLayerName, string>> = {
  understand: 'understand',
  alternatives: 'alternatives',
  solution_map: 'solution',
}

interface Step {
  number: number
  name: ReviewLayerName
  label: string
  outcome: string
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
