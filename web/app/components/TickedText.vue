<script setup lang="ts">
/**
 * One string a session wrote, with each pair of backticks drawn as inline code.
 *
 * **The session already marks code names; the page was printing the marks.** On the review
 * this was read against, a problem's detail read `` `url.intended` held the pending… `` —
 * backticks and all — and "all the back ticks… makes it a little bit harder to see". The
 * words are right and the record is right. What was missing was a page that knew what the
 * marks meant.
 *
 * ```
 *  in   `ExportContext` is built by the schedule
 *  out   ExportContext  is built by the schedule
 *        └── a <code> chip
 * ```
 *
 * ## Why this is not a markdown renderer
 *
 * **Everything is interpolated, there is no `v-html`, and no markdown library is
 * involved.** The string is split into pieces and Vue draws each piece as text inside an
 * element it chose, so `<script>` in a session's sentence is drawn as the five characters
 * `<`, `s`… and cannot be anything else. That is not a precaution about this one field: a
 * brief is written by an agent reading a pull request, which means its prose is attacker-
 * reachable text in a page served to the developer.
 *
 * It also only does the one thing. **A session's prose is not markdown and must not be
 * rendered as it** — a stray `*` or `#` in a sentence about code would otherwise become
 * emphasis or a heading. The draft is the one field that *is* markdown, and `MarkdownView`
 * renders that one, sanitised; this component is never used on it.
 *
 * ## The commit id rule
 *
 * **A 40-character hexadecimal run is drawn as its first seven characters**, with the whole
 * value one focus away. A source's reference carried one twice on the review this was read
 * against, and a line of prose with 40 characters of hex in it has no shape left. Seven is
 * what git itself abbreviates to and what every tool in this chain prints.
 *
 * It applies inside a backtick pair as well as outside one, because a session that marked
 * the commit as code was right to and the length is the problem either way.
 *
 * | In | Out |
 * |---|---|
 * | `` `a` and `b` `` | two code chips |
 * | `` a ` b `` | the backtick, printed as it is |
 * | 40 hex characters | seven of them, as code, the whole on focus |
 * | `<script>` | eight characters of text |
 *
 * **Both rules are `utils/ticked.ts`'s**, and so is the table above. This component is what
 * draws them; `spoken()` in the same module is what says them where there is no markup.
 */
const props = defineProps<{
  /** What the session wrote. Drawn as it is, apart from the two rules above. */
  text: string | null | undefined
}>()

/**
 * **The split lives in `utils/ticked.ts`**, and this component draws what it answers.
 *
 * It was here until DEV-928, which added a second caller: a held-back row's accessible name
 * is a plain string, so it needs the same two rules applied and nothing to draw them with.
 * A hand-written version of that got the backticks right and the commit ids wrong — a title
 * shown as seven characters and announced as forty — so the rules have one home and both
 * forms are derived from it.
 */
const drawn = computed(() => pieces(props.text))
</script>

<!--
  **The pieces are drawn edge to edge, with no whitespace between the elements.** A newline
  between two of these tags would be a whitespace text node in the middle of a sentence:
  Vue condenses it to a single space, and `` `a`, and `b` `` would come out with a space
  before the comma. All the spacing a reader sees is in the strings themselves, which is why
  the tags sit against each other and the attributes carry the line breaks.

  The commit chip holds both forms. `title` alone would be a mouse-only answer, and a reader
  who cannot see 40 characters of hex still has to be able to copy them, so the whole value
  is shown when the chip has the focus.
-->
<template>
  <span class="prose"><template
    v-for="(piece, index) in drawn"
    :key="index"
  ><code
    v-if="piece.kind === 'code'"
    class="code"
  >{{ piece.text }}</code><code
    v-else-if="piece.kind === 'commit'"
    class="code code--commit"
    tabindex="0"
    :title="piece.whole"
    :aria-label="`commit ${piece.whole}`"
  ><span
    class="short"
    aria-hidden="true"
  >{{ piece.text }}</span><span
    class="whole"
    aria-hidden="true"
  >{{ piece.whole }}</span></code><template
    v-else
  >{{ piece.text }}</template></template></span>
</template>

<style scoped>
/*
 * `display: contents`, so this component adds no box: the string is drawn inside whatever
 * the caller put it in — a paragraph, a table cell, a list item — and a span in the middle
 * of a `pre-wrap` paragraph or a flex row would change how that line lays out.
 */
.prose {
  display: contents;
}

.code {
  padding: 1px 5px;
  border-radius: var(--rd-radius-key);
  background: var(--rd-rail-pressed);
  font-family: var(--rd-font-mono);
  font-size: 0.86em;

  /*
   * A code name is one unbroken token and some of them are long. `anywhere` lets it break
   * inside rather than widen the column it is in — which at 390px is the difference between
   * a page that wraps and a page that scrolls sideways.
   */
  overflow-wrap: anywhere;
}

/* The seven characters, and the forty behind them. */
.whole {
  display: none;
}

.code--commit {
  cursor: help;
}

.code--commit:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.code--commit:focus .short,
.code--commit:hover .short {
  display: none;
}

.code--commit:focus .whole,
.code--commit:hover .whole {
  display: inline;
}
</style>
