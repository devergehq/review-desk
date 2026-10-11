<script setup lang="ts">
/**
 * One numbered problem as a tag: the number in a filled circle, then its name.
 *
 * **A bare number down a flow reads as a step count, and that is the defect.** The marks on
 * the before and after flows read 1, 1, 2, 3, 1 on the review this was read against. Every
 * one of them was correct — problem 1 touches four boxes — and the column still said
 * nothing a reader could use: "the numbers don't make any sense at all". A name stops it
 * reading as a count, because a count does not repeat a name.
 *
 * ```
 *  ╭───┬──────────────────╮
 *  │ 1 │ The shared slot  │   the circle and the label, one thing
 *  ╰───┴──────────────────╯
 * ```
 *
 * **It never wraps inside itself, and that is a rule rather than a preference.** Anything
 * pill- or tag-shaped on these screens moves whole to a new line when it does not fit; a
 * tag broken across two lines is two halves of a shape, and the shape is what makes it read
 * as one label. `white-space: nowrap` holds the inside together and `flex: none` stops a
 * flex parent squeezing it; the moving-to-its-own-line is the parent's `flex-wrap`, which is
 * the only place that can know there is no room.
 *
 * **The number is still the mechanism; the name is what makes it legible.** "2 is the same
 * 2" is how the two flows are read against each other, so the number is drawn in both and
 * the colour says which side you are on — and the colour is never the only thing carrying
 * the meaning, which is [`ProblemMark`]'s criterion and holds here for the same reason.
 *
 * **Separate from [`ProblemMark`], which keeps its three sizes and gains nothing.** A mark
 * is the number alone, and it is right where the name is already beside it: a problem row,
 * a table heading, a line of prose. This is the shape for a place with no room for a
 * heading, which is a flow box's corner.
 */
const props = withDefaults(
  defineProps<{
    number: number
    /**
     * Its name, in two or three words. **Null is expected**, not a fault: every problem
     * recorded before 10 October 2026 has none, and the label is the session's to write.
     */
    label?: string | null
    /** `wrong` is the before flow, `fixed` is the after flow. */
    tone: 'wrong' | 'fixed'
  }>(),
  { label: null },
)

/**
 * What the tag says when the record holds no name for the problem.
 *
 * **"Problem 1", never the bare number.** The whole point of the tag is that a number on
 * its own reads as a count; falling back to one would reintroduce the defect on exactly the
 * records that cannot answer for themselves.
 */
const words = computed(() => props.label ?? `Problem ${props.number}`)
</script>

<template>
  <span
    class="tag"
    :class="`tag--${tone}`"
  >
    <span
      class="circle"
      aria-hidden="true"
    >{{ number }}</span>
    <span class="words">{{ words }}</span>
    <!--
      What side of the picture this is, for a reader who is not seeing the colour. The name
      and the number are drawn; this is the third channel, as on `ProblemMark`.
    -->
    <span class="rd-visually-hidden">, {{ tone === 'wrong' ? 'what is wrong' : 'what fixes it' }}</span>
  </span>
</template>

<style scoped>
.tag {
  /*
   * **The four declarations the rule turns on.** `flex: none` so a flex parent cannot
   * squeeze it, `nowrap` so it cannot break inside itself, `inline-flex` so the circle sits
   * on the label's own line, and the pill radius so the shape is the shape.
   */
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 2px 10px 2px 3px;
  border-radius: var(--rd-radius-pill);
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.circle {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--circle);
  color: var(--rd-surface);
  font-size: 12px;
  font-variant-numeric: proportional-nums;
}

/*
 * **Clipped rather than wrapped, in the one case a name is too long for the box.** The tag
 * moves to its own line first — that is the parent's job — and a label of twelve words in a
 * 150px box has nowhere left to go. Cut with an ellipsis, with the whole of it still in the
 * document for a screen reader, which is the same bargain the trail's one line strikes.
 */
.words {
  /*
   * `min-width: 0` is the half that is easy to leave out. A flex child's minimum width is
   * its content by default, so without this the ellipsis never fires and a label of twelve
   * words widens the box it is in — which at 390px is the horizontal scroll this change is
   * against, caused by the fix for it.
   */
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/*
 * The two flows' colours. `--rd-wrong` is 6.54:1 on white and the numeral reversed out of
 * it is the same 6.54:1; the accent is 6.17:1 both ways. `test/contrast.spec.ts` computes
 * both rather than taking the boards' word for it.
 */
.tag--wrong {
  background: var(--rd-wrong-wash);
  color: var(--rd-wrong);
  --circle: var(--rd-wrong);
}

.tag--fixed {
  background: var(--rd-fixed-wash);
  color: var(--rd-accent);
  --circle: var(--rd-accent);
}
</style>
