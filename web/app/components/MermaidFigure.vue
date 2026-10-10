<script setup lang="ts">
/**
 * One mermaid diagram. A browser was chosen as the renderer for Review Desk so that a
 * brief's diagram and a draft's diagram are drawn rather than described, so this is the
 * component that earns that choice.
 *
 * mermaid is loaded on mount and never during the build: the app is generated static and
 * the diagram source is a record, not a template.
 *
 * ## Three things a source can be
 *
 * | The source | What is shown |
 * |---|---|
 * | A diagram mermaid draws | the drawing |
 * | A diagram mermaid knows and cannot draw | the source, and a note that it could not be drawn |
 * | Not a diagram at all | the source, as a sketch, and a line saying it is shown as written |
 *
 * The third is the common one and it is **not** a failure. A review session writes the
 * record, and a session that drew a flow by hand in text wrote content — showing it as
 * written is the right answer, not an apology.
 *
 * ## Why nothing may escape this component
 *
 * `mermaid.render(id, source)` with no container element makes mermaid append its own
 * `<div id="d{id}">` to `document.body`, draw into it, and remove it when it is finished.
 * **Both of its failure paths `throw` before that removal** — on a parse failure it draws
 * the bomb-and-"Syntax error in text" graphic into the div and then re-throws the parse
 * error one line above its own cleanup, and on a draw failure it paints the same graphic
 * and throws. The dashboard is one page, so the orphan then sits at the bottom of every
 * screen the developer opens afterwards. That is DEV-880, and it is guarded three ways:
 *
 * 1. **The source is parsed before it is rendered.** `mermaid.parse` touches no DOM at
 *    all, so a source that cannot parse never reaches the call that appends to the body.
 *    It is also what tells the three cases above apart, by mermaid's own answer rather
 *    than by a guess at what mermaid accepts.
 * 2. **`suppressErrorRendering`** makes mermaid clean up after itself on the one failure
 *    that is left — a source that parses and will not draw.
 * 3. **[`sweep`]** removes mermaid's containers by id whatever happened, including when
 *    this component is unmounted while a render is still in flight.
 *
 * Any one of the three would cover today's bug. All three are here because the first two
 * are promises mermaid makes and the third is the only one this repository can keep.
 */
const props = defineProps<{ source: string, label?: string }>()

const svg = ref('')
/** mermaid knows this diagram and could not draw it. */
const failed = ref(false)
/** mermaid does not recognise this as a diagram: it is text, and it is shown as written. */
const sketch = ref(false)

/**
 * The id mermaid is asked to use, and so the id of everything it may leave behind.
 *
 * Unique per figure, which is why [`sweep`] exists at all: mermaid clears a previous render
 * of the *same* id on its way in, so no other figure on the page will ever collect this
 * one's orphan.
 */
const figureId = `mermaid-${useId()}`

/**
 * Remove mermaid's own scratch elements, if it left any.
 *
 * `d{id}` is the div it renders into and `i{id}` the iframe it would use under
 * `securityLevel: 'sandbox'`. **Not `{id}` itself**: mermaid hands back the serialised
 * innerHTML of that div, so the svg this component inserts carries that id too, and
 * sweeping it would delete the drawing.
 */
function sweep(): void {
  document.getElementById(`d${figureId}`)?.remove()
  document.getElementById(`i${figureId}`)?.remove()
}

onMounted(async () => {
  try {
    const mermaid = (await import('mermaid')).default
    mermaid.initialize({
      startOnLoad: false,
      /*
       * The diagram source is untrusted text from a record, so mermaid sanitises its own
       * output and this component's `v-html` is safe. Nothing here relaxes that.
       */
      securityLevel: 'strict',
      /* See the note above: this is what makes mermaid tidy up its own failed render. */
      suppressErrorRendering: true,
      theme: 'base',
      /*
       * `--rd-font-sans`, spelled out. mermaid writes this into a `<style>` inside the svg
       * and the value has to survive being serialised, so it is the stack rather than the
       * custom property. If `tokens.css` changes, change it here too.
       */
      fontFamily: '\'Instrument Sans\', \'Helvetica Neue\', sans-serif',
      themeVariables: {
        primaryColor: '#ffffff',
        primaryBorderColor: '#d9dce2',
        primaryTextColor: '#16181d',
        lineColor: '#565b66',
        fontSize: '14px',
      },
    })

    try {
      await mermaid.parse(props.source)
    }
    catch (error) {
      /*
       * `UnknownDiagramError` is mermaid saying "this is not any diagram I have", which is
       * the sketch. Anything else is a diagram whose grammar it rejected. The class is a
       * type-only export, so the name is what there is to match on.
       */
      if ((error as Error | undefined)?.name === 'UnknownDiagramError') sketch.value = true
      else failed.value = true
      return
    }

    svg.value = (await mermaid.render(figureId, props.source)).svg
  }
  catch {
    /* It parsed and would not draw — or mermaid itself did not load. */
    failed.value = true
  }
  finally {
    sweep()
  }
})

onBeforeUnmount(sweep)
</script>

<template>
  <figure class="figure">
    <!--
      mermaid's own output, from the diagram source in the record. mermaid runs with
      `securityLevel: 'strict'`, which is what makes inserting its SVG safe.
    -->
    <!-- eslint-disable vue/no-v-html -->
    <div
      v-if="svg"
      class="canvas"
      :aria-label="label"
      v-html="svg"
    />
    <!-- eslint-enable vue/no-v-html -->
    <!--
      Interpolated, never `v-html`. The source is text a review session wrote and this is
      the branch that shows it verbatim, so it is the one place where treating it as markup
      would turn a record into a script.
    -->
    <pre
      v-else
      :class="sketch ? 'written' : 'fallback'"
    ><code>{{ source }}</code></pre>
    <figcaption
      v-if="sketch"
      class="rd-meta"
    >
      Shown as written.
    </figcaption>
    <figcaption
      v-else-if="failed"
      class="rd-meta"
    >
      This diagram could not be drawn. Its source is above.
    </figcaption>
  </figure>
</template>

<style scoped>
.figure {
  margin: 0;
}

/* A wide diagram scrolls inside its own frame; the page never scrolls sideways. */
.canvas {
  overflow-x: auto;
  max-width: 100%;
}

.canvas :deep(svg) {
  max-width: 100%;
  height: auto;
}

.fallback,
.written {
  margin: 0;
  padding: 12px;
  /*
   * As the drawing does, and for the same reason: a sketch is drawn with spaces, and
   * wrapping it would take the diagram apart.
   */
  overflow-x: auto;
  background: var(--rd-ground);
  border-radius: var(--rd-radius-sm);
  font-family: var(--rd-font-mono);
  font-size: 13px;
}

/*
 * A diagram that failed is in a frame, because something is missing from the page and the
 * border is what says so. A sketch has no border: it is the content, whole, and the boards
 * draw quiet recorded text as an unbordered wash — `docs/mockups/uplift/BriefRefined.dc.html`
 * sets "How we know" that way.
 */
.fallback {
  border: 1px solid var(--rd-line);
}

.written {
  line-height: 1.55;
}
</style>
