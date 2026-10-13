<script setup lang="ts">
import type { ReviewDetail, ReviewLayerName } from '~/types/desk'

/**
 * **Which round you are reading, and the same step of every other round.**
 *
 * ```
 *  ROUND 2 OF 3
 *  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
 *  │ Round 1      │ │ Round 2      │ │ Round 3      │
 *  │ posted 9 Oct │ │ being worked… │ │ not posted  │
 *  │ 3f9c2ab      │ │ 9d02e71 ·here │ │ a7b8c97     │
 *  └──────────────┘ └──────────────┘ └──────────────┘
 *  └ a link ─────┘  └ not a link ─┘  └ a link ────┘
 * ```
 *
 * A review is one round of a pull request, and until DEV-976 that was a dead end: round 2
 * is a new record and there was no way back to round 1 from inside it. Patrick, 14 October
 * 2026: "there's no way for me to actually go back to the previous round… especially when I
 * go and look at the understand, the alternatives, the solution map, and even the code
 * review. It's really important to be able to see where we've come from, not just this
 * specific point in time."
 *
 * ## Why it is here rather than on a page
 *
 * Drawn once, by `ReviewStepPage`, which is what all six step pages are inside. The three
 * things a reader needs above a step are which pull request, which round of it and which
 * step of that round — so this sits between the header line and the trail, and the three
 * lines descend in scope. Six copies of it would be six chances for one page to go without.
 *
 * ## What it says, and what it refuses to say
 *
 * **Each other round is a link to the same step**, because a reader on Alternatives in round
 * 3 wants Alternatives in round 1, not the top of round 1. The round being read is drawn as
 * text and carries `aria-current="page"`: a link to the page you are on is a link that does
 * nothing, and a keyboard user has no way to tell.
 *
 * **"Being worked on" is the newest unposted round and only that one.** An older unposted
 * round is one the author pushed past — nobody is working on it and nobody can post it,
 * which is DEV-957's whole subject — so it reads "not posted". `standingOf` in
 * `utils/rounds.ts` owns that rule.
 *
 * **A dismissed round is marked only where the wire says so.** `RoundSummary` does not carry
 * the flag on master; DEV-954's arrives with DEV-962, and `roundDismissed` is the one reader
 * waiting for it. Until then no round is marked, and none is marked "not dismissed" either:
 * the answer is three-valued because "I was not told" is not "it is fine".
 *
 * ## The shape
 *
 * A wrapping row of chips rather than a scrolling strip, so 390 pixels is a layout question
 * and not a gesture: four rounds wrap to two rows and nothing goes off the side.
 * `e2e/rounds.spec.ts` measures that against the binary rather than trusting it.
 *
 * Every chip is an anchor or a span — no button, no handler — so keyboard reach is the
 * global `:focus-visible` ring and the browser's own tab order, and the back button works.
 */

const NuxtLink = resolveComponent('NuxtLink')

const props = defineProps<{
  detail: ReviewDetail
  /** Which step this is. Each link goes to the same one. */
  step: ReviewLayerName
}>()

const rounds = computed(() => roundLinks(props.detail, props.step))
const label = computed(() => roundsLabel(props.detail))

/**
 * The step's name, for the part of each link only a screen reader hears.
 *
 * "Round 1" on its own does not say where it goes, and six of these pages differ by nothing
 * else. Said in the link rather than in the heading, because a screen reader reading the
 * links of a page reads them out of their surroundings.
 */
const step = computed(() => layerLabel(props.step))
</script>

<template>
  <nav
    v-if="label && rounds.length > 1"
    class="rounds"
    aria-label="The rounds of this pull request"
  >
    <h2 class="rd-rubric">
      {{ label }}
    </h2>

    <ol class="list">
      <li
        v-for="round in rounds"
        :key="round.round"
        class="item"
      >
        <component
          :is="round.to ? NuxtLink : 'span'"
          :to="round.to ?? undefined"
          class="round"
          :class="{ 'round--here': round.here }"
          :aria-current="round.here ? 'page' : undefined"
        >
          <!--
            **The round's number, and the part of it only a screen reader hears.** "Round 1"
            does not say where it goes and six of these pages differ by nothing else, so the
            step is said inside the link — a screen reader reading a page's links reads them
            out of their surroundings.

            **Written hard against the number with no line break**, because Vue's
            `whitespace: 'condense'` condenses the whitespace around an interpolation into a
            single space rather than deleting it: `Round 2\n  <span>, Understand` reaches
            the browser as "Round 2 , Understand". Measured in `e2e/rounds.spec.ts`, which
            asserts the accessible name through the real build.
          -->
          <span class="which">Round {{ round.round }}<span class="rd-visually-hidden"><template v-if="round.here">, the round you are reading</template><template v-else>, {{ step }}</template></span></span>
          <span class="standing">{{ round.standing }}</span>
          <span class="shas">
            <span class="rd-mono head">{{ round.head }}</span>
            <!--
              Only where the wire said so. See `roundDismissed`: the absence of the flag is
              not the absence of a dismissal, so nothing is drawn for "not told".
            -->
            <span
              v-if="round.dismissed"
              class="tag"
            >dismissed</span>
          </span>
        </component>
      </li>
    </ol>
  </nav>
</template>

<style scoped>
.rounds {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

/*
 * **Wrapping, which is what makes 390 pixels a layout rather than a gesture.** A strip that
 * scrolled sideways would hide the round a reader came looking for behind a swipe, on the
 * one screen whose whole purpose is being able to get back to it.
 */
.list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

.item {
  min-width: 0;
}

.round {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 12px;
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
  color: var(--rd-ink);
  text-decoration: none;
  min-width: 0;
}

/*
 * The round being read, drawn as the chosen one. Its left edge rather than a fill, so the
 * row still reads as one row of the same thing — and the weight on the name, because a
 * colour change alone is the cue a reader can miss.
 */
.round--here {
  border-color: var(--rd-accent);
  box-shadow: inset 2px 0 0 var(--rd-accent);
  background: var(--rd-accent-wash);
}

.which {
  font-size: 14px;
  font-weight: 500;
}

.round--here .which {
  color: var(--rd-accent-dark);
}

/* Underlined on the link, which is the one cue that survives being small and grey. */
a.round .which {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.standing {
  font-size: 13px;
  color: var(--rd-ink-muted);
}

.shas {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.head {
  color: var(--rd-ink-muted);
}

.tag {
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--rd-warn-ink);
  background: var(--rd-warn-wash);
  border-radius: var(--rd-radius-pill);
  padding: 1px 8px;
}

@media (max-width: 560px) {
  /*
   * **One chip per row under 560 pixels, measured rather than assumed.** Two chips of
   * "posted 9 October 2026" do not fit 390 pixels side by side, and a flex item whose
   * content will not fit does not shrink below its min-content width — it pushes the row
   * past the viewport. Stretching them to the full width is what keeps the page's only
   * horizontal measurement the page's own.
   */
  .item {
    width: 100%;
  }

  /*
   * **And the chip reads across rather than down**, which is the whole reason for the
   * breakpoint doing two things. Three stacked lines each in a full-width box put 270
   * pixels of switcher — 360 on a pull request with four rounds — between the header and
   * the trail on a phone, so the step a reader came to read starts below the fold. Across,
   * each round is one line or two.
   *
   * The children stay flex items, so they stay blockified and the accessible name is the
   * same one `e2e/rounds.spec.ts` asserts at both widths.
   */
  .round {
    width: 100%;
    flex-flow: row wrap;
    align-items: baseline;
    column-gap: 10px;
  }
}
</style>
