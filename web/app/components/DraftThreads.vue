<script setup lang="ts">
import type { DraftThread } from '~/types/desk'

/**
 * The inline threads the draft hands over: what posts **on the line it concerns**.
 *
 * ```
 *  ON THE CODE · 9 THREADS
 *  Each is a row of the index above, and posts as a comment on that line.
 *   F9  blocker  app/Reporting/ColumnPolicy.php:52
 *   ┃ Blocker · F9 — The column policy is skipped entirely when…
 *   ┃ What · the array cast answers [] for a null column…
 * ```
 *
 * ## Why the page needs this at all
 *
 * The preview beside it is the body, and since DEV-929 the body is an index: a finding with
 * a line inside the diff is a row in it and a comment on the code, **not** a section. So a
 * screen that drew only the body would be showing the developer less than they are signing
 * off. These are the rest of it, with the line each one sits on, which is the one fact the
 * body cannot carry.
 *
 * ## Three rules, the first two borrowed from `HeldBackRows`
 *
 * 1. **It describes; it does not grade.** The severity is the record's own word in the body
 *    ink — no tag, no colour that differs by severity. The glyphs the house style uses are
 *    in the comment itself, where they are read on GitHub, and this page does not add a
 *    second opinion about how bad a finding is.
 * 2. **Nothing is printed twice.** The caption is the finding's number, its severity and
 *    the place; the title, the tag and the disposition chip are in the comment below it,
 *    because that is where they post. The same rule is why the marker's *reason* is in the
 *    card's one sentence and only the marker itself sits under each comment: the first
 *    version of this card printed two lines of explanation under all nine of them.
 * 3. **The comment is drawn through `MarkdownView`**, the same renderer the body preview
 *    uses, so a fold stays folded and a backticked path is code. The alert's `[!CAUTION]`
 *    marker shows as text in a quote, which is honest: it is what is in the characters
 *    being posted, and the colour it turns into is GitHub's.
 *
 *    **A mermaid fence inside the alert is drawn as its source**, because `splitMermaid`
 *    lifts a fence that opens a line and every line of a comment is inside the blockquote.
 *    Whether GitHub draws one there as a diagram is not something this page can know. What
 *    the page is for is that nothing is posted unread, and the source satisfies that; a
 *    diagram the developer must *see* belongs under `### Problem fit` or under a finding
 *    with no thread, both of which the body preview draws.
 */
const props = defineProps<{
  threads: DraftThread[]
  /** Where the pull request is, to say which anchors were not measured against it. */
  headSha: string
  posted?: boolean
}>()

const heading = computed(() => count(props.threads.length, 'thread'))

/**
 * Whether **every** thread here is missing its commit, which is said once rather than
 * nine times. DEV-978.
 *
 * **Rule 2, applied to the sentence this change adds.** Nothing is printed twice, and
 * until sessions start recording the commit they measured a line on, every finding on a
 * record carries none — so the per-thread version of this would print the identical
 * sentence under all nine threads of a real-length review, which is the repetition
 * `HeldBackRows` exists to undo and which the card's own first draft was corrected for.
 * A fact true of the whole list belongs to the list.
 *
 * A *mixed* draft is the other case and keeps the per-thread sentence: there the absence
 * is about that one anchor and not about the review, and saying it on the list would be
 * saying it of threads it is not true of.
 */
const noneRecorded = computed(() =>
  props.threads.length > 0
  && props.threads.every(thread => thread.line_sha == null || thread.line_sha.length === 0),
)

/**
 * Which commit this thread's line was counted on, where that is not where the pull request
 * is now — and nothing when it is. DEV-978.
 *
 * **The one fact about an anchor the card could not previously show.** GitHub positions a
 * review comment against the pull request's head, and the line beside it may have been
 * counted somewhere else: a carried finding keeps the earlier round's numbering, and since
 * DEV-962 a head moves under findings nobody has re-read.
 *
 * `measuredAt` is the same states in the code review page's words; this is the thread's
 * shorter version, because what the developer is checking here is the place.
 */
function measuredElsewhere(thread: DraftThread): string | null {
  const measured = thread.line_sha
  if (measured == null || measured.length === 0) {
    // Said once above the list when it is true of all of them — see `noneRecorded`.
    return noneRecorded.value ? null : 'the commit it was measured on was not recorded'
  }
  return sameCommit(measured, props.headSha)
    ? null
    : `measured at ${commitLabel(measured)}, not at ${commitLabel(props.headSha)}`
}
</script>

<template>
  <DeskCard
    as="section"
    class="threads"
  >
    <div class="rd-label">
      On the code · {{ heading }}
    </div>

    <!--
      **Past tense once it is posted, and a caveat with it.** The body is stored and is what
      went to GitHub; these are composed from the record every time it is read, and after
      the post the record is allowed to move on — the harvest reads the author's replies
      back and a finding it settles drops out of this list. Saying "posts" there would be a
      page claiming to show something it cannot.
    -->
    <p
      v-if="threads.length > 0 && posted"
      class="rd-meta help"
    >
      The comments this review put on the code, composed from the record as it stands. A
      finding the harvest has settled since is no longer among them; the body above is the
      text that was posted, and it does not move.
    </p>
    <p
      v-else-if="threads.length > 0"
      class="rd-meta help"
    >
      Each of these is a row of the index, and posts as a comment on the line it names.
      Nothing here is in the body. Each ends with a line GitHub does not draw — it is how
      the session matches the comment it gets back to the finding, which the line alone
      cannot do when two findings sit on one.
    </p>

    <!--
      **Said once, because it is true of the whole list.** Every finding written before the
      record held the commit carries none, so the per-thread version of this would print
      the same sentence under all nine threads of a real-length review.
    -->
    <p
      v-if="noneRecorded"
      class="rd-meta help"
    >
      None of these records which commit its line was measured on. The pull request is at
      <code>{{ commitLabel(headSha) }}</code>, and whether each line still means what it
      did there is not something the record can say.
    </p>

    <ol
      v-if="threads.length > 0"
      class="list"
    >
      <li
        v-for="thread in threads"
        :key="thread.finding_key"
        class="thread"
      >
        <p class="where">
          <span class="key">{{ thread.finding_key }}</span>
          <span class="severity rd-meta">
            <!--
              `{{ ' ' }}`, for the reason `what-happened.vue` measures: without it the text
              reads `Severity:blocker`, and neither a newline nor `&#32;` at the end of one
              survives Vue's `whitespace: 'condense'`.
            -->
            <span class="rd-visually-hidden">Severity:</span>{{ ' ' }}{{ thread.severity }}
          </span>
          <code class="place">{{ thread.path }}:{{ thread.line }}</code>
          <!--
            Interpolated as text, as every other value on this card is. The commit is a
            value a reader copies, so it takes the monospace face the place does.
          -->
          <span
            v-if="measuredElsewhere(thread)"
            class="measured rd-meta"
          >{{ measuredElsewhere(thread) }}</span>
        </p>
        <div class="comment">
          <MarkdownView :source="thread.body" />
        </div>
        <!--
          **The line GitHub does not show, shown — and the reason said once.** It renders as
          nothing in the preview above, exactly as it will on GitHub, so what the developer
          signs off would otherwise be missing a line of itself. What it is *for* is in the
          card's own sentence at the top: nine threads is nine of these, and the same two
          lines of prose under each of them is the repetition `HeldBackRows` exists to undo.
        -->
        <p class="rd-meta marker">
          <span class="rd-visually-hidden">Ends with the marker</span>
          <code>{{ threadMarker(thread.finding_key) }}</code>
        </p>
      </li>
    </ol>

    <!--
      **No thread is a state, not an absence.** A review whose findings are all corrections,
      all outside the change, or all without a hunk has nothing GitHub can anchor — and the
      sentence says which of those it is rather than leaving an empty card.
    -->
    <p
      v-else
      class="rd-empty"
    >
      No finding in this draft has a line inside the diff to hang a comment on, so the whole
      review posts as the body. A finding gets a thread when it names a file and a line, and
      the code it carries is a line the change touched.
    </p>
  </DeskCard>
</template>

<style scoped>
.threads {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.help,
.rd-empty {
  margin: 0;
  max-width: var(--rd-measure);
}

.list {
  display: flex;
  flex-direction: column;
  gap: 20px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/*
 * The caption wraps rather than scrolls: a path is long and the column at 390 is narrow, so
 * the place goes to its own line there instead of taking the card off the page.
 */
.where {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 10px;
  align-items: baseline;
  margin: 0 0 8px;
}

.key {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.place {
  min-width: 0;
  font-family: var(--rd-font-mono);
  font-size: 13px;
  overflow-wrap: anywhere;
}

/*
 * Quiet, and on the caption line with the place it is about. It is not a grade and not a
 * problem: it says which commit the line beside it came from, which is rule 1's
 * "describes, does not grade" applied to the one fact an anchor could not carry.
 */
.measured {
  font-family: var(--rd-font-mono);
  overflow-wrap: anywhere;
}

/*
 * The comment, set in from the rule that stands for GitHub's own rail. The rail is the one
 * piece of colour here and it is the same for every severity: which colour GitHub draws is
 * GitHub's business, and four colours here would be the grading rule 1 rules out.
 */
.comment {
  padding-left: 16px;
  border-left: 2px solid var(--rd-rail);
}

/*
 * Quiet, and outside the rail: it is not part of the finding, which is the same reason the
 * house style puts the `agent:` marker outside the alert.
 */
.marker {
  margin: 6px 0 0;
  max-width: var(--rd-measure);
}

.marker code {
  font-family: var(--rd-font-mono);
  overflow-wrap: anywhere;
}
</style>
