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
 *    because that is where they post.
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
const props = defineProps<{ threads: DraftThread[] }>()

const heading = computed(() => count(props.threads.length, 'thread'))
</script>

<template>
  <DeskCard
    as="section"
    class="threads"
  >
    <div class="rd-label">
      On the code · {{ heading }}
    </div>

    <p
      v-if="threads.length > 0"
      class="rd-meta help"
    >
      Each of these is a row of the index, and posts as a comment on the line it names.
      Nothing here is in the body.
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
            <span class="rd-visually-hidden">Severity:</span>{{ thread.severity }}
          </span>
          <code class="place">{{ thread.path }}:{{ thread.line }}</code>
        </p>
        <div class="comment">
          <MarkdownView :source="thread.body" />
        </div>
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
 * The comment, set in from the rule that stands for GitHub's own rail. The rail is the one
 * piece of colour here and it is the same for every severity: which colour GitHub draws is
 * GitHub's business, and four colours here would be the grading rule 1 rules out.
 */
.comment {
  padding-left: 16px;
  border-left: 2px solid var(--rd-rail);
}
</style>
