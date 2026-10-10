<script setup lang="ts">
/**
 * One numbered problem, as the mark that appears wherever it is referred to.
 *
 * **The number is the signal; the colour reinforces it.** The Understand board puts the
 * same number in red on the before flow and in green on the after flow, and the acceptance
 * criterion is that the colour is never the only thing carrying the meaning. Three channels
 * do it here:
 *
 * 1. **The number itself**, which is `brief.problems[].number` and is identical on both
 *    sides. "2 is the same 2" is the whole mechanism of the picture.
 * 2. **The words**, in the accessible name: a mark reads as "2, what is wrong" or
 *    "2, what fixes it". Visually hidden, so the drawing is the board's and the
 *    announcement is a sentence.
 * 3. **The side it is on**, which the flow's own "Before" and "After" heading states.
 *
 * `--rd-wrong` is 6.54:1 on white and the numeral reversed out of it is the same 6.54:1;
 * `test/contrast.spec.ts` computes both rather than taking the boards' word for it.
 *
 * Three sizes, which is what the boards draw: 26px in a flow box, 30px in a problem row,
 * 22px inline in a table heading or beside a line of prose.
 */
const props = withDefaults(
  defineProps<{
    number: number
    /** `wrong` is the before flow, `fixed` is the after flow and a solution part. */
    tone: 'wrong' | 'fixed'
    size?: 'row' | 'box' | 'inline'
    /**
     * What this mark is doing here, when it is neither of the two usual things. A solution
     * part's green mark says "fixes problem 2", not "what fixes it".
     */
    says?: string | null
  }>(),
  { size: 'box', says: null },
)

const WORDS: Record<'wrong' | 'fixed', string> = {
  wrong: 'what is wrong',
  fixed: 'what fixes it',
}

const words = computed(() => props.says ?? WORDS[props.tone])
</script>

<template>
  <span
    class="mark"
    :class="[`mark--${tone}`, `mark--${size}`]"
  >
    <span aria-hidden="true">{{ number }}</span>
    <span class="rd-visually-hidden">{{ number }}, {{ words }}</span>
  </span>
</template>

<style scoped>
.mark {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  color: var(--rd-surface);
  font-weight: 600;
  font-variant-numeric: proportional-nums;
}

.mark--box {
  width: 26px;
  height: 26px;
  font-size: 13px;
}

.mark--row {
  width: 30px;
  height: 30px;
  font-size: 15px;
}

.mark--inline {
  width: 22px;
  height: 22px;
  font-size: 12px;
}

.mark--wrong {
  background: var(--rd-wrong);
}

.mark--fixed {
  background: var(--rd-accent);
}
</style>
