<script setup lang="ts">
import MarkdownIt from 'markdown-it'
import DOMPurify from 'dompurify'

/**
 * A draft rendered the way GitHub renders it: markdown with raw HTML allowed, so a
 * folded section stays folded, and mermaid fences drawn as diagrams.
 *
 * The HTML is sanitised before it is inserted. The draft is written by a review session,
 * which is not a person typing, and a preview exists so that nothing is posted unread —
 * it should not be the one place a session can run script.
 */
const props = defineProps<{ source: string }>()

const md = new MarkdownIt({ html: true, linkify: true, typographer: false, breaks: false })

const blocks = computed(() => splitMermaid(props.source))

/*
 * DOMPurify's default allow-list already keeps `details` and `summary`, which is what a
 * folded proof needs; `open` is added so a session can ship a fold already open when the
 * evidence is the point rather than the footnote.
 */
function render(markdown: string): string {
  return DOMPurify.sanitize(md.render(markdown), { ADD_ATTR: ['open'] })
}
</script>

<template>
  <div class="rd-md">
    <template
      v-for="(block, index) in blocks"
      :key="index"
    >
      <!--
        `declared`, because this block came out of a ```mermaid fence: the session said it
        was a diagram. One that does not draw is shown as a diagram that did not draw, not
        as a sketch — GitHub will show a render error for the same fence, and this screen
        exists so that nothing is posted unread.
      -->
      <MermaidFigure
        v-if="block.kind === 'mermaid'"
        :source="block.content"
        label="Diagram from the draft"
        declared
      />
      <!--
        Sanitised in `render` above. markdown-it's raw-HTML mode is what keeps a fold
        folded, and DOMPurify is what keeps a session from running script in the one
        screen that exists so nothing is posted unread.
      -->
      <!-- eslint-disable vue/no-v-html -->
      <div
        v-else
        v-html="render(block.content)"
      />
      <!-- eslint-enable vue/no-v-html -->
    </template>
  </div>
</template>

<style scoped>
.rd-md {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: var(--rd-measure);
}

.rd-md :deep(h1),
.rd-md :deep(h2),
.rd-md :deep(h3),
.rd-md :deep(h4) {
  margin: 20px 0 8px;
  font-size: 17px;
  font-weight: 600;
  line-height: 1.3;
}

.rd-md :deep(h3):first-child,
.rd-md :deep(h4):first-child {
  margin-top: 0;
}

.rd-md :deep(p) {
  margin: 0 0 12px;
}

.rd-md :deep(p:last-child) {
  margin-bottom: 0;
}

.rd-md :deep(ul),
.rd-md :deep(ol) {
  margin: 0 0 12px;
  padding-left: 22px;
}

.rd-md :deep(li) {
  margin-bottom: 4px;
}

.rd-md :deep(code) {
  font-family: var(--rd-font-mono);
  font-size: 13px;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-xs);
  padding: 1px 5px;
}

.rd-md :deep(pre) {
  margin: 0 0 12px;
  padding: 12px 14px;
  overflow-x: auto;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
}

.rd-md :deep(pre code) {
  background: none;
  border: 0;
  padding: 0;
  font-size: 13px;
  line-height: 1.5;
}

.rd-md :deep(details) {
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  padding: 8px 12px;
}

.rd-md :deep(summary) {
  cursor: pointer;
  font-size: 14px;
  color: var(--rd-ink-dim);
  /*
   * A fold's summary is content, not a control of the app's, and it is rendered the
   * compact way GitHub renders it. It spans the fold's full width and is 32px tall,
   * which clears WCAG 2.2's 24px target minimum; the app's own controls are 44px.
   */
  min-height: 32px;
  display: flex;
  align-items: center;
}

/*
 * **A fold has to say it is one.** `display: flex` above takes the native disclosure
 * marker off the summary, so a closed fold was a line of dim text with a border round it
 * and nothing to say a click opened it. That cost nothing while every fold here was a
 * session's own proof — a reader who missed it missed a footnote. Since the index is
 * folded it costs the thing the fold is for: the developer has to be able to see that the
 * index is there and closed, not gone.
 *
 * Drawn rather than typed, so it needs no glyph from the body font and turns with the fold.
 */
.rd-md :deep(summary)::before {
  content: '';
  flex: none;
  width: 0;
  height: 0;
  margin-right: 8px;
  border: 5px solid transparent;
  border-left: 7px solid currentColor;
  transition: transform 120ms ease;
}

.rd-md :deep(details[open] summary)::before {
  transform: rotate(90deg);
}

@media (prefers-reduced-motion: reduce) {
  .rd-md :deep(summary)::before {
    transition: none;
  }
}

.rd-md :deep(details[open] summary) {
  margin-bottom: 10px;
  border-bottom: 1px solid var(--rd-rail);
  padding-bottom: 8px;
}

.rd-md :deep(blockquote) {
  margin: 0 0 12px;
  padding: 0 0 0 12px;
  border-left: 3px solid var(--rd-line);
  color: var(--rd-ink-dim);
}

/*
 * **GitHub's own declaration for a table, so the preview is faithful at a phone's width.**
 *
 * `.markdown-body table` is `display: block; width: max-content; max-width: 100%; overflow:
 * auto` — a table GitHub cannot fit is *scrolled*, not reflowed. Copying it does two things
 * at once: the index the posted body carries is drawn here the way the author will see it,
 * and a four-column table at 390 pixels scrolls inside its own card instead of taking the
 * page sideways. DEV-929, where the body gained the index table.
 */
.rd-md :deep(table) {
  display: block;
  width: max-content;
  max-width: 100%;
  overflow: auto;
  border-collapse: collapse;
  font-size: 14px;
}

.rd-md :deep(th),
.rd-md :deep(td) {
  border: 1px solid var(--rd-line);
  padding: 6px 10px;
  text-align: left;
}

.rd-md :deep(strong) {
  font-weight: 600;
}
</style>
