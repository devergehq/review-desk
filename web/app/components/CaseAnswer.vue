<script setup lang="ts">
import type { Brief } from '~/types/desk'

/**
 * The line of facts the case page opens with, **derived from what the record already
 * holds**.
 *
 * ```
 *  Every request the browser sends to the portal now carries the gate's credentials…
 *  4 problems · option B built · fixes 3 of 4 · blind pass would pick D
 *  Right shape; the fail-fast sits one call site too low, and nothing has tested it…
 * ```
 *
 * Patrick's own example of the line, and his condition on it, 16 October 2026: it is
 * derived from existing review data — **no new session-written summary and no new field** —
 * and a fact that cannot be derived is left out rather than guessed at.
 *
 * **Why a derived line rather than a written one.** The page it opens is the case for a
 * change, and the question it answers is the one he asked of all three step pages: how far
 * do I have to read before I know what this is. A sentence a session wrote would answer it
 * and would also be a fifth thing to check, because nothing on the page could show it was
 * true. Every fact here is drawn again, in full, in the band under it: the count of
 * problems is band 1's row count, the option is band 2's marked row, the figure is that
 * row's four dots, and the blind pick is the row marked beside it. The line is a reading of
 * the page, so the page is its proof.
 *
 * `answerFacts` in `utils/theCase.ts` owns which facts there are and how each is worded, so
 * `test/the-case.spec.ts` tests the rules without rendering anything. This draws them.
 */
const props = defineProps<{ brief: Brief }>()

const facts = computed(() => answerFacts(props.brief))
</script>

<template>
  <!--
    **Nothing at all when nothing derives.** A brief part way through being written has
    earned some of these facts and not others, and a line that drew an empty row would be a
    heading over nothing — which is the one thing every absence on these pages is written to
    avoid.
  -->
  <p
    v-if="facts.length"
    class="answer"
  >
    <!--
      What the line is, for a reader who cannot see that it sits under the headline. "From
      the record" rather than "In short": these are not a summary of the page, they are four
      values read off it, and the difference is the whole reason the line can be trusted.
    -->
    <span class="rd-visually-hidden">From the record: </span>
    <span
      v-for="(fact, index) in facts"
      :key="fact.kind"
      class="fact"
      :class="`fact--${fact.kind}`"
    ><!--
      **The separator a reader hears, because the one they see is a `::before`.** The middot
      between two facts is drawn by the stylesheet, so that a line wrapping to two rows never
      begins with a dot — and a stylesheet draws nothing a screen reader reads, which left
      the line announcing "3 problemsoption C built". A comma in the markup is the same
      punctuation in the channel that was missing it.
    --><span
      v-if="index"
      class="rd-visually-hidden"
      >, </span>{{ fact.says }}</span>
  </p>
</template>

<style scoped>
/*
 * One line that wraps whole facts, never a fact. A flex row with a gap is what makes the
 * separator between two facts a thing the stylesheet draws rather than a middot in the
 * string — which is the difference between a line that wraps to two and a line whose second
 * row starts with a dot.
 */
.answer {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 0;
  margin: 0;
  font-size: 17px;
  color: var(--rd-ink);
  /* A column of digits is not what this is, but "3 of 4" reads evenly with them lined up. */
  font-variant-numeric: tabular-nums;
  max-width: var(--rd-measure);
}

.fact {
  white-space: nowrap;
}

/*
 * The separator, drawn by the fact that follows it. Faint, and with its own space either
 * side, so the four facts read as four and the dots read as punctuation rather than as
 * values.
 */
.fact + .fact::before {
  content: '·';
  margin: 0 10px;
  color: var(--rd-ink-faint);
}

/*
 * **The one fact that is a judgement about the build rather than a count of it.** A blind
 * pass that picked something else is the most useful thing on this line and the easiest to
 * read past, so it carries the question-blue this app already uses for everything the blind
 * pass says — on Alternatives' ring, its rubric and its badge.
 */
.fact--blind {
  color: var(--rd-question-ink);
}

@media (max-width: 560px) {
  .answer {
    font-size: 15px;
  }

  /*
   * At a phone's width four facts are four lines, and a dot at the start of each is the
   * thing a wrapped middot does wrong. One fact a line, no separators.
   */
  .fact + .fact::before {
    content: none;
    margin: 0;
  }

  .fact {
    flex: 0 0 100%;
  }
}
</style>
