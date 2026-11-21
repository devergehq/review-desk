<script setup lang="ts">
import type { FindingRow } from '~/utils/whatHappened'

/**
 * **Everything the record holds on one finding, as the step-7 page drew it.** DEV-1330.
 *
 * The Replies tab groups findings by what came back and quotes the one thing that grouped
 * each. This is the rest, folded under the row: where the finding is, the round it was
 * carried from, every outcome oldest first with the author's words whole, and the two links
 * — or, for a finding nothing came back on, which of three reasons that is. `docs/design.md`:
 * a page may fold what it draws and never remove it, and this is what the step-7 page drew.
 *
 * **The author's words are interpolated, never `v-html`.** `rd-recorded` keeps the paragraph
 * breaks they typed; a reply carrying a backtick, a tag or GitHub's markdown arrives as the
 * characters typed.
 */
defineProps<{
  row: FindingRow
  /** The element id, for the control that opens it. */
  id: string
}>()
</script>

<template>
  <div
    :id="id"
    class="history"
  >
    <!--
      **Mono only where it is a path.** `whereOf` answers "The pull request itself" for a
      finding that names no file, and a sentence set in mono reads as a path with spaces.
    -->
    <p class="rd-meta where">
      <span :class="{ 'rd-mono': row.inCode }">{{ row.where }}</span>
      <template v-if="row.carried">
        · {{ row.carried }}
      </template>
    </p>

    <ol
      v-if="row.cameBack.length > 0"
      class="back"
    >
      <li
        v-for="came in row.cameBack"
        :key="came.id"
        class="came"
      >
        <p class="came-said">
          {{ came.sentence }}
          <span class="rd-meta when">{{ came.when }}</span>
          <span
            v-if="came.file"
            class="rd-mono rd-meta came-file"
          >{{ came.file }}</span>
        </p>
        <blockquote
          v-if="came.quote"
          class="rd-recorded quote"
        >
          {{ came.quote }}
        </blockquote>
      </li>
    </ol>

    <!--
      **Three reasons nothing came back, said apart.** A finding raised in the body alone has
      no thread at all; one whose comment id no address can be built from has one, reachable
      on the pull request; and the repository name can be what failed.
    -->
    <p
      v-else
      class="rd-note silent"
    >
      Nothing has come back on {{ row.key }}.
      <template v-if="row.comment">
        It was posted as a comment, so a reply would be read.
      </template>
      <template v-else-if="row.postedAsComment && row.addressable">
        It was posted as a comment, so a reply would be read — but the id recorded for that
        comment is not one this page can build an address from, so there is no link to it here.
      </template>
      <template v-else-if="row.postedAsComment">
        It was posted as a comment, so a reply would be read. There is no link to it here
        because this review's repository name is not one a GitHub address can be built from.
      </template>
      <template v-else>
        No comment on GitHub is recorded against it, so there is no thread for a reply to
        attach to and none could be read.
      </template>
    </p>

    <p class="out">
      <NuxtLink
        class="to-finding"
        :to="row.finding"
      >The finding<span class="rd-visually-hidden"> {{ row.key }} on the Findings tab</span></NuxtLink>
      <a
        v-if="row.comment"
        class="to-finding"
        :href="row.comment"
        target="_blank"
        rel="noopener noreferrer"
      >On GitHub<span class="rd-visually-hidden">: {{ row.key }}’s comment, opens in a new tab</span></a>
    </p>
  </div>
</template>

<style scoped>
.history {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 16px 16px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ground);
  min-width: 0;
}

.where {
  margin: 0;
  overflow-wrap: anywhere;
}

.back {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/* A quiet rule to the left of each outcome, in the ink line and no severity colour. */
.came {
  padding-left: 14px;
  border-left: 2px solid var(--rd-line-strong);
  min-width: 0;
}

.came-said {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 10px;
  margin: 0;
  font-size: var(--rd-type-body);
  min-width: 0;
}

.when {
  flex: none;
  font-variant-numeric: tabular-nums;
}

.came-file {
  overflow-wrap: anywhere;
  min-width: 0;
}

.quote {
  margin: 6px 0 0;
  padding: 8px 12px;
  border-left: 3px solid var(--rd-line-strong);
  background: var(--rd-surface);
}

.silent {
  margin: 0;
  max-width: var(--rd-measure);
}

.out {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 16px;
  margin: 0;
}

/* Underlined, not accent-coloured alone: at this size a colour change is easy to miss. */
.to-finding {
  color: var(--rd-accent);
  font-size: 13px;
  text-decoration: underline;
  text-underline-offset: 2px;
  white-space: nowrap;
}
</style>
