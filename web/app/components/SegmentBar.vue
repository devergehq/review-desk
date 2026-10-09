<script setup lang="ts">
/**
 * How far along something is, as a row of segments rather than a filled track.
 *
 * The boards draw two of these and they are the same control at two sizes:
 *
 * | Where | Segments | Size | Means |
 * |---|---|---|---|
 * | The queue, "With the agent" | 7 | `thin` — 14×5 | which of the seven layers is running |
 * | Rules, a rule in force | 4 | `wide` — 22×6 | which stage the rule has graduated to |
 *
 * A segment is one of three, and the names are the boards' colours rather than an
 * interpretation of them: `done` is ink, `now` is the accent, `todo` is the drawing grey.
 * The queue draws the layers before the running one as `done` and the running one as
 * `now`. Rules draws a repository rule's attained stages as `done` — and a rule that has
 * graduated into a check draws **every** segment as `now`, because the board does: the
 * accent is where the thing has got to, and a check needs no model to get there.
 *
 * A bar is **never the only thing saying where the work has got to**: the caller writes
 * the stage out beside it, which is the boards' own arrangement and the one thing that
 * makes this readable without colour.
 *
 * **It is a `role="img"` with a label, not a progressbar.** Seven layers are named stages,
 * not a percentage, and a screen reader announcing "71%" of a code review would be making
 * a number up. The label is a sentence the caller writes — "Layer 5 of 7, code review".
 */
withDefaults(
  defineProps<{
    /** One per segment, in order. */
    segments: Array<'done' | 'now' | 'todo'>
    /** The sentence that replaces the bar for anyone who cannot see it. */
    label: string
    size?: 'thin' | 'wide'
  }>(),
  { size: 'thin' },
)
</script>

<template>
  <span
    class="bar"
    :class="`bar--${size}`"
    role="img"
    :aria-label="label"
  >
    <span
      v-for="(state, at) in segments"
      :key="at"
      class="seg"
      :class="`seg--${state}`"
    />
  </span>
</template>

<style scoped>
.bar {
  display: inline-flex;
  flex: 0 0 auto;
  gap: 2px;
}

/* The stage bar's segments are rounded and spaced; the layer bar's are square and tight. */
.bar--wide {
  gap: 6px;
}

.seg {
  display: block;
}

.bar--thin .seg {
  width: 14px;
  height: 5px;
}

.bar--wide .seg {
  width: 22px;
  height: 6px;
  border-radius: var(--rd-radius-bar);
}

.seg--done {
  background: var(--rd-ink);
}

.seg--now {
  background: var(--rd-accent);
}

.seg--todo {
  background: var(--rd-line-strong);
}
</style>
