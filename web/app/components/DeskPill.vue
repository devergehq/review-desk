<script setup lang="ts">
/**
 * **The one pill, and the one rule about it: a pill never breaks across two lines.**
 *
 * Patrick flagged exactly that on the Understand board — a badge wrapping mid-phrase onto
 * a second line — and it is an acceptance criterion rather than a polish item, so the rule
 * is a component instead of a convention. Three declarations hold it:
 *
 * | Declaration | What it stops |
 * |---|---|
 * | `white-space: nowrap` | the words inside breaking |
 * | `flex: none` | the flex parent shrinking it below its content |
 * | `min-width: max-content` | a grid or table cell squeezing it |
 *
 * The consequence is deliberate: when a pill does not fit beside its text it drops to its
 * own line **whole**, which is what the criterion asks for. That needs the row around it to
 * be `display: flex; flex-wrap: wrap`, which is why every caller here is.
 *
 * Four tones, all from the approved boards:
 *
 * | Tone | Drawn | On the boards |
 * |---|---|---|
 * | `quiet` | the neutral wash | "Ticket C" — this option came off the ticket |
 * | `info` | the blue wash | "Blind" — a blind pass also raised it |
 * | `go` | the accent, reversed out | "Built" — this is the option that was built |
 * | `go-quiet` | the accent wash | a count on something settled |
 * | `info-strong` | blue, reversed out | "Not on the ticket" |
 * | `ink` | ink, reversed out | a blocker's count on a solution part |
 *
 * **`ink` and `info-strong` are not interchangeable.** A blocker was drawn in
 * `info-strong` and so wore the colour that means "the blind pass raised this" one page
 * earlier, which is the kind of collision a shared vocabulary exists to stop.
 * `SeverityTag` reverses a blocker out of ink, and so does this.
 */
withDefaults(
  defineProps<{
    tone?: 'quiet' | 'info' | 'go' | 'go-quiet' | 'info-strong' | 'ink'
  }>(),
  { tone: 'quiet' },
)
</script>

<template>
  <span
    class="pill"
    :class="`pill--${tone}`"
  ><slot /></span>
</template>

<style scoped>
.pill {
  /*
   * The three declarations the criterion turns on. `inline-flex` rather than `inline` so
   * that a mark or an icon inside sits on the text's own baseline run.
   */
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: 6px;
  min-width: max-content;
  white-space: nowrap;

  padding: 2px 10px;
  border-radius: var(--rd-radius-pill);
  font-size: 13px;
  font-weight: 600;
}

.pill--quiet {
  background: var(--rd-nit-wash);
  color: var(--rd-nit-ink);
}

.pill--info {
  background: var(--rd-question-wash);
  color: var(--rd-question-ink);
}

.pill--go {
  background: var(--rd-accent);
  color: var(--rd-surface);
}

.pill--go-quiet {
  background: var(--rd-accent-wash);
  color: var(--rd-accent-dark);
}

.pill--info-strong {
  background: var(--rd-question-ink);
  color: var(--rd-surface);
}

.pill--ink {
  background: var(--rd-ink);
  color: var(--rd-surface);
}
</style>
