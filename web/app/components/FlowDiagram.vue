<script setup lang="ts">
import type { BriefFlow, BriefProblem } from '~/types/desk'

/**
 * How the system works, as rows of boxes with arrows between them.
 *
 * **HTML boxes, not mermaid.** DEV-885 asks for it by name, and the reason is DEV-880: a
 * session wrote a 50-line text sketch into the `diagram` field, mermaid refused it, and its
 * error graphic sat at the bottom of every page the developer opened afterwards. A flow
 * that is `<div>`s cannot fail to parse, wraps at a phone's width, and every word in it is
 * interpolated.
 *
 * **A flow is nested, and nothing here numbers or positions a box.** `flow_before` is a
 * list of rows and a row is a list of one to three boxes, so a split in the flow is a row
 * of two — which is DEV-882's decision, restated in DEV-885's comment: "Do not number or
 * position boxes yourself." This component lays a row out as a grid of as many columns as
 * it has boxes and stops there.
 *
 * ```
 *  ┌───────────────────────────────┐
 *  │ A schedule fires an export    │   one box: the row is the column
 *  └───────────────────────────────┘
 *               ▼
 *  ┌──────────────┐ ┌──────────────┐   two boxes: a split, side by side
 *  │ Web export   │ │ Scheduled    │
 *  └──────────────┘ └──────────────┘
 * ```
 *
 * A box carrying a `problem` is marked: the before flow in red, the after flow in green,
 * with the same number on both. A box carrying none is plain — it is part of the story and
 * not part of what is wrong.
 *
 * ## The mark is a tag, since DEV-917
 *
 * **It carries the problem's name, because a bare number down a flow reads as a step
 * count.** The marks read 1, 1, 2, 3, 1 on the review this was read against: every number
 * correct, since problem 1 touches four boxes, and the column still unreadable. So the mark
 * moved to the box's top right and became a [`ProblemTag`] — the number in a circle and two
 * or three words — and the problems are listed once as a row of the same tags above both
 * flows, which the page draws.
 *
 * The tag sits on the title's own line and **moves whole to the next line when the title
 * leaves no room**, which is this app's rule for anything pill-shaped. `flex-wrap` on the
 * line is what does the moving; the tag is what refuses to break.
 *
 * ## Where this differs from the board, deliberately
 *
 * The board dims the boxes a selected problem does not touch to `opacity: 0.28`. Computed
 * against the tokens, a box's note line is `--rd-ink-muted` and reaches 4.5:1 at no opacity
 * below 1 — 0.8 gives 3.93:1 — so dimming is a contrast failure on the one line that
 * explains the box. **The emphasis is additive instead**: a selected problem's boxes gain a
 * ring, and nothing is suppressed. The question the dimming answered — which boxes does
 * problem 2 touch — is still answered, by highlight rather than by hiding.
 */
const props = defineProps<{
  rows: BriefFlow
  /** `wrong` is the before flow, `fixed` is the after flow. */
  tone: 'wrong' | 'fixed'
  /** The problem number a reader has opened, so its boxes can be picked out. */
  focus?: number | null
  /**
   * The brief's numbered problems, for the names the tags carry.
   *
   * The whole list rather than a map of labels, so this component takes what the record
   * holds and `labelFor` owns the one decision — what a problem with no label is called.
   */
  problems?: BriefProblem[]
}>()

/**
 * The name a box's tag carries, or null when the box marks no problem.
 *
 * A problem the flow marks and the brief does not list is possible — the store numbers
 * them separately — and it reads as "Problem 2" rather than as a blank tag, which is the
 * same answer a problem with no label gets.
 */
function labelFor(number: number): string | null {
  return (props.problems ?? []).find(problem => problem.number === number)?.label ?? null
}
</script>

<template>
  <div
    class="flow"
    :class="`flow--${tone}`"
  >
    <template
      v-for="(row, index) in rows"
      :key="index"
    >
      <!--
        The arrow between two rows, not after every row. Hidden from the accessible tree:
        the reading order already says one row follows another, and "down triangle" read
        five times is noise.
      -->
      <div
        v-if="index > 0"
        class="arrow"
        aria-hidden="true"
      >
        <span class="arrow-line" />
        <span class="arrow-head">▼</span>
      </div>

      <div
        class="row"
        :style="{ '--columns': row.length }"
      >
        <div
          v-for="(box, slot) in row"
          :key="slot"
          class="box"
          :class="{
            'box--marked': box.problem != null,
            'box--focus': box.problem != null && box.problem === focus,
          }"
        >
          <!--
            The title and the tag on one line, and the tag at its right. `flex-wrap` is
            what moves the tag whole to the next line when the title leaves no room for
            it: a tag broken across two lines is two halves of a shape.
          -->
          <div class="head">
            <span class="title">{{ box.title }}</span>
            <ProblemTag
              v-if="box.problem != null"
              :number="box.problem"
              :label="labelFor(box.problem)"
              :tone="tone"
            />
          </div>
          <span
            v-if="box.note"
            class="note"
          >{{ box.note }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.flow {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/*
 * The flow's colour, set once on the flow and read by every marked box inside it. Declared
 * here rather than in each page so that neither page names a hex, and so a box cannot be
 * drawn in the other flow's colour by being copied.
 */
.flow--wrong {
  --border: var(--rd-wrong);
  --wash: var(--rd-wrong-wash);
}

.flow--fixed {
  --border: var(--rd-accent);
  --wash: var(--rd-fixed-wash);
}

/* The join between two rows: a short rule and the head of an arrow under it. */
.arrow {
  display: flex;
  flex-direction: column;
  align-items: center;
  color: var(--rd-ink-faint);
  line-height: 1;
}

.arrow-line {
  width: 2px;
  height: 14px;
  background: var(--rd-key-line);
}

.arrow-head {
  font-size: 12px;
  margin-top: -3px;
}

/*
 * As many columns as the row has boxes, from the record. The `minmax(0, 1fr)` is what stops
 * a long note in one box widening the grid past its container — which at 390px is the
 * difference between a flow that wraps and a page that scrolls sideways.
 */
.row {
  display: grid;
  grid-template-columns: repeat(var(--columns), minmax(0, 1fr));
  gap: 12px;
  min-width: 0;
}

.box {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  padding: 14px 16px;
  border: 1px solid var(--rd-line-strong);
  border-radius: var(--rd-radius);
  background: var(--rd-surface);
}

/*
 * The title and its tag, with the tag pushed to the right of the line. `flex-start` on the
 * cross axis so a title that runs to two lines keeps the tag level with its first line
 * rather than centred against the block.
 */
.head {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: flex-start;
  gap: 6px 12px;
  min-width: 0;
}

.title {
  font-weight: 600;
  overflow-wrap: break-word;
}

.note {
  font-size: 14px;
  color: var(--rd-ink-muted);
  overflow-wrap: break-word;
}

/*
 * A marked box carries its flow's colour, 2px so the edge reads as a statement rather than
 * as a rule between two things. The parent sets which colour, so one block serves both
 * flows and neither page restates a hex.
 */
.box--marked {
  border-width: 2px;
  border-color: var(--border);
  background: var(--wash);
}

.box--focus {
  box-shadow: 0 0 0 3px var(--wash), 0 0 0 4px var(--border);
}

/* At a phone's width a split row is two rows, which is the only way two boxes fit. */
@media (max-width: 560px) {
  .row {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
