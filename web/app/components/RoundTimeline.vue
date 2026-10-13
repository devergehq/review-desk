<script setup lang="ts">
/**
 * The rounds of one pull request, across the top of the page, and what has happened to the
 * one being read.
 *
 * ```
 *  ROUNDS
 *  [ Round 1 · posted 9 Oct · 6 findings ]  [ Round 2 · dismissed ]  [ Round 3 · this one ]
 *  Updated for 3 commits since 4d9464f · 1 push brought the base branch in and changed nothing
 * ```
 *
 * **Two things the record could say and no screen did.** DEV-954: a dismissed round was drawn
 * exactly like a live one, because every other field on a round summary is a count and a
 * dismissed round's counts look the same — so the timeline said a round was in play over a
 * round the developer had taken out of it. And DEV-962: a round's head moves now, instead of
 * a second round opening, so the page has to be able to say the round is no longer of the
 * commit the developer was last shown.
 *
 * **A dismissed round is drawn as dismissed and kept in the list.** A hole where round 2 was
 * would read as a gap in the record — `list_reviews` keeps dismissed rounds for that reason —
 * so the word is on the chip, the chip is struck through, and the meaning is in the text
 * rather than in the styling: a reader with no colour, and a screen reader, get the same
 * sentence.
 *
 * The sentences under the chips come from `utils/rounds`, which is the one copy of them —
 * Understand and the draft page say the same things, and three spellings is how two screens
 * come to disagree about the record in front of them.
 */
import type { HeadUpdates, RoundSummary } from '~/types/desk'

import { dayLabel } from '~/utils/format'
import { baseOnlyLine, commitLabel, foldedLine, updatedLine } from '~/utils/rounds'

const props = defineProps<{
  /** Every round of this pull request, oldest first. Always at least this one. */
  rounds: RoundSummary[]
  /** Which of them is being read, by review id. */
  current: number
  /** What has happened to the current round's head. */
  updates: HeadUpdates
}>()

/** What a chip says after the round number: posted, dismissed, or whose turn it is. */
function state(round: RoundSummary): string {
  // **Dismissed first, whatever else is true of it.** A dismissed round with a posted
  // draft is still a round the developer took out of play, and the order here is the one
  // `LeftTheDesk` states for the same reason.
  if (round.dismissed) return 'dismissed'
  if (round.posted_at) {
    const day = dayLabel(round.posted_at)
    return day ? `posted ${day}` : 'posted'
  }
  if (round.review_id === props.current) return 'this one'
  return 'not posted'
}

/** "6 findings", or nothing at all for a round that raised none. */
function findings(round: RoundSummary): string | null {
  if (round.findings < 1) return null
  return round.findings === 1 ? '1 finding' : `${round.findings} findings`
}

/** The three sentences about the current round's head, in the order they happened. */
const lines = computed(() =>
  [updatedLine(props.updates), baseOnlyLine(props.updates), foldedLine(props.updates)]
    .filter((line): line is string => line !== null),
)
</script>

<template>
  <!--
    Drawn only for a pull request with more than one round, or one whose head has moved.
    A first round nobody has pushed to has nothing to say, and a timeline of one chip
    saying "this one" is furniture.
  -->
  <section
    v-if="rounds.length > 1 || lines.length > 0"
    class="timeline"
    aria-labelledby="rounds-heading"
  >
    <h2
      id="rounds-heading"
      class="rd-rubric"
    >
      Rounds
    </h2>

    <ol class="chips">
      <li
        v-for="round in rounds"
        :key="round.review_id"
        class="chip"
        :class="{
          'chip--current': round.review_id === current,
          'chip--dismissed': round.dismissed,
        }"
        :data-round="round.round"
        :data-dismissed="round.dismissed ? 'yes' : 'no'"
      >
        <span class="round">Round {{ round.round }}</span>
        <span class="rd-meta">{{ state(round) }}</span>
        <span
          v-if="findings(round)"
          class="rd-meta"
        >{{ findings(round) }}</span>
        <span class="rd-meta rd-mono">{{ commitLabel(round.head_sha) }}</span>
      </li>
    </ol>

    <p
      v-for="line in lines"
      :key="line"
      class="rd-note moved"
    >
      {{ line }}
    </p>
  </section>
</template>

<style scoped>
.timeline {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.chip {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--rd-rule);
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
}

/* The round being read: the same chip, with the weight a current step has. */
.chip--current {
  border-color: var(--rd-accent);
}

/*
 * **Struck through, and the word says it too.** The line is what a reader sees at a glance;
 * `state()` puts "dismissed" in the text so that nothing about this depends on seeing it.
 */
.chip--dismissed .round {
  text-decoration: line-through;
}

.chip--dismissed {
  background: var(--rd-ground);
  border-style: dashed;
}

.round {
  font-weight: 500;
}

.moved {
  margin: 0;
}
</style>
