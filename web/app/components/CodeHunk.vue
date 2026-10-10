<script setup lang="ts">
import type { FindingHunk } from '~/types/desk'

/**
 * The code a finding is about, with the comment sitting on the flagged line.
 *
 * ```
 *  app/Reporting/ColumnPolicy.php:52
 *  ┌──────────────────────────────────────────────────────┐
 *  │  50 +     $allowed = $definition->allowed_columns;   │
 *  │  51 +                                                │
 *  │▌52 +     if ($allowed === []) {                      │  ← flagged
 *  │     ┌────────────────────────────────────────────┐   │
 *  │     │ Your comment on this line                  │   │
 *  │     │ This early return makes an empty list mean │   │
 *  │     └────────────────────────────────────────────┘   │
 *  │  53 +         return new self(allowed: []);          │
 *  └──────────────────────────────────────────────────────┘
 * ```
 *
 * This is the part of board A8 Patrick singled out — "I love the code with the comments, the
 * inline style. I mean, if we could achieve that, that would be incredible" — and it is also
 * where the warning he gave in the same breath lands: "I think you're going to have trouble
 * trying to contain that."
 *
 * ## Containing it
 *
 * **This is the only thing on the page that scrolls sideways**, which is DEV-878's criterion
 * in one sentence. A line of PHP is as long as it is and nothing here re-wraps it — Review
 * Desk does not fetch code, so these lines are the only copy there is and a soft wrap would
 * make the gutter numbers lie. So the hunk scrolls and the page does not.
 *
 * **The comment is pinned, not carried.** The board puts the comment inside the scrolling
 * area, which means scrolling right to read the end of a line takes the comment off the
 * screen with it — on a phone, after about forty characters. It is a grid row spanning the
 * code's three columns with `position: sticky; left: 0`, so the code moves under it and the
 * comment stays where it was. That is one scroller and nothing lost; splitting the hunk in
 * two around the comment was the alternative, and two scrollbars that move independently is
 * a worse answer than one.
 *
 * Its width is bounded in `ch` rather than as a percentage, because a percentage inside a
 * `max-content` grid resolves against the grid — which is as wide as the longest line of
 * code, so `max-width: 100%` would be no bound at all.
 *
 * ## The flagged line is marked by more than colour
 *
 * Three channels, because the criterion asks for more than one and one of them has to work
 * for a reader who cannot see the wash: a 3px bar down the left edge, the warning wash
 * behind it, and `aria-current` with a visually-hidden "the line this comment is about".
 * The marker column already carries `+`, `−` or nothing, so the wash is not doing the work
 * of saying what the change did to the line either.
 *
 * **A removed line carries no number**, which is the contract's own rule — it is not in the
 * file any more — and is drawn with `−`, not the `-` a diff uses, because this is a reading
 * of the change rather than a patch.
 *
 * ## A hunk that draws no change
 *
 * Every line `unchanged` means the session read these lines out of the head checkout rather
 * than out of the diff: the finding is on a line the pull request did not touch. **Said in
 * words, under the file name**, because the three washes are then all absent and a reader
 * would otherwise have no way to tell this from a hunk of context they happen to be looking
 * at the middle of. It is still drawn as code, which is the point — the alternative is the
 * sentence that says there is none, and that would be less than the session recorded.
 */
const props = defineProps<{
  hunk: FindingHunk
  /** The line the comment attaches to. The server refuses one the hunk does not draw. */
  flaggedLine?: number | null
  /** The comment as it will post, or null when the finding records none. */
  comment?: string | null
  /** Where this is, for the caption over the code. */
  where: string
}>()

/** What the page draws in the marker column, per the contract's `HunkLineMarker`. */
const MARKS = { added: '+', removed: '−', unchanged: '' } as const

const lines = computed(() =>
  props.hunk.lines.map((line, at) => ({
    at,
    number: line.line,
    marker: line.marker,
    mark: MARKS[line.marker],
    text: line.text,
    flagged: line.line != null && line.line === props.flaggedLine,
  })),
)

/** Whether any line is flagged, which decides whether the comment has a line to sit on. */
const anchored = computed(() => lines.value.some(line => line.flagged))

/** Whether the pull request changed none of these lines. */
const unchanged = computed(() => drawsNoChange(props.hunk))
</script>

<template>
  <figure class="hunk">
    <figcaption class="caption">
      <span class="rd-mono">{{ where }}</span>
      <!--
        Not a pill: this is a sentence about the whole block, and a pill beside a file path
        would read as a label on the path. `DeskPill`'s own rule is that a pill never breaks
        across two lines, which is exactly what this would have to do at 390px.
      -->
      <span
        v-if="unchanged"
        class="unchanged"
      >The pull request changed none of these lines — the session read them as they stand.</span>
    </figcaption>

    <!--
      The scroller, and the one element on this page with `overflow-x: auto`. `tabindex="0"`
      and a role with a name, because a region that scrolls has to be reachable by keyboard
      — otherwise the end of a long line is readable with a pointer and with nothing else.
    -->
    <div
      class="scroller"
      tabindex="0"
      role="group"
      :aria-label="`The code at ${where}, which scrolls sideways`"
    >
      <div class="code">
        <template
          v-for="line in lines"
          :key="line.at"
        >
          <div
            class="line"
            :class="[`line--${line.marker}`, { 'line--flagged': line.flagged }]"
            :aria-current="line.flagged ? 'true' : undefined"
          >
            <span
              class="number"
              aria-hidden="true"
            >{{ line.number ?? '' }}</span>
            <span
              class="mark"
              aria-hidden="true"
            >{{ line.mark }}</span>
            <span class="text">{{ line.text }}</span>
          </div>

          <!--
            The comment, directly under the line it is about. Rendered inside the scroller
            and pinned to its left edge, so the code scrolls under it rather than taking it
            along.
          -->
          <div
            v-if="line.flagged && comment"
            class="note"
          >
            <div class="card">
              <span class="rd-rubric note-label">
                Your comment on this line<span class="rd-visually-hidden">, which is line {{ line.number }}</span>
              </span>
              {{ comment }}
            </div>
          </div>
        </template>
      </div>
    </div>

    <!--
      A comment the hunk cannot place. The server refuses a write that anchors one to a line
      the hunk does not draw, so this is a record written before that rule existed — and
      dropping the comment would be the one thing worse than drawing it in the wrong place.
    -->
    <p
      v-if="comment && !anchored"
      class="rd-note adrift"
    >
      <strong>Your comment</strong>, which this hunk draws no line for:
      {{ comment }}
    </p>
  </figure>
</template>

<style scoped>
.hunk {
  display: flex;
  flex-direction: column;
  margin: 0;
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-control);
  /*
   * The border radius has to clip the scroller, or the first and last code lines' washes
   * square off the corners the border rounds.
   */
  overflow: hidden;
  min-width: 0;
}

.caption {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--rd-line);
  background: var(--rd-ground);
  font-size: 12.5px;
  /* The file path is the one thing here allowed to wrap: it is not code. */
  overflow-wrap: anywhere;
}

.unchanged {
  color: var(--rd-ink-muted);
  font-size: 13px;
}

.scroller {
  overflow-x: auto;
  padding: 6px 0;
  /*
   * `overscroll-behavior-x: contain` so that reaching the end of the code does not then
   * scroll the page, which on a trackpad turns reading one line into losing your place.
   */
  overscroll-behavior-x: contain;
}

.scroller:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -3px;
}

/*
 * `max-content`, so the grid is as wide as its longest line and the scroller has something
 * to scroll. The three columns are the gutter, the marker, and the line.
 */
.code {
  display: grid;
  grid-template-columns: 44px 18px max-content;
  width: max-content;
  min-width: 100%;
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
  line-height: 1.6;
}

.line {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: subgrid;
  /* The bar is always there and transparent, so a flagged line does not shift by 3px. */
  border-left: 3px solid transparent;
}

.line--added {
  background: var(--rd-fixed-wash);
}

.line--removed {
  background: var(--rd-wrong-wash);
}

/*
 * The flagged line, in three channels: the bar, the wash, and `aria-current` with the
 * hidden sentence in the comment's label. It wins over the added and removed washes
 * because what matters about this line is that it is the one being commented on.
 */
.line--flagged {
  background: var(--rd-warn-wash);
  border-left-color: var(--rd-warn-line);
}

.number {
  padding-right: 10px;
  color: var(--rd-ink-faint);
  text-align: right;
  /* Numbers are drawn, never selected: copying the code should not copy the gutter. */
  user-select: none;
}

.mark {
  user-select: none;
}

.line--added .mark {
  color: var(--rd-accent);
}

.line--removed .mark {
  color: var(--rd-wrong);
}

/*
 * `pre`, never `pre-wrap`. A soft wrap would put two screen lines under one gutter number,
 * which makes every number after it a claim about the wrong line.
 */
.text {
  padding-right: 14px;
  white-space: pre;
  tab-size: 4;
}

/*
 * The comment's row, pinned to the scroller's left edge. `sticky` on the row rather than on
 * the card so the card's own shadow is not clipped by the sticky box.
 */
.note {
  position: sticky;
  left: 0;
  grid-column: 1 / -1;
  /* `max-content` and nothing wider: the row must not stretch the grid for the code. */
  width: max-content;
  max-width: 46ch;
  padding: 6px 14px 8px 62px;
}

.card {
  padding: 10px 14px;
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-card);
  font-family: var(--rd-font-sans);
  font-size: 14px;
  line-height: 1.45;
  white-space: normal;
}

.note-label {
  display: block;
  margin-bottom: 3px;
}

.adrift {
  margin: 0;
  padding: 10px 14px;
  border-top: 1px dashed var(--rd-line-strong);
}

.adrift strong {
  font-weight: 600;
}

/*
 * At a phone's width the comment's indent is most of the screen. It drops to the gutter's
 * edge and takes the width it can get; the code beside it is unchanged, because the code is
 * the one thing here that must not be reflowed.
 */
@media (max-width: 560px) {
  .note {
    max-width: 38ch;
    padding-left: 18px;
  }

  .code {
    grid-template-columns: 34px 14px max-content;
  }
}
</style>
