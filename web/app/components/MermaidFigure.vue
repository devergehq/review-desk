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
 * **Unless the source was declared a diagram**, which is what [`declared`] is for. A
 * brief's `diagram` field is free text and a session may reasonably draw in it; a
 * ```` ```mermaid ```` fence in a draft is the session saying "this is a diagram". A
 * mistyped type keyword inside that fence — `sequencediagram` for `sequenceDiagram` — is
 * `UnknownDiagramError` just the same, and calling it "shown as written" would tell the
 * developer that a broken diagram was deliberate on the one screen that exists to show
 * what GitHub will show. GitHub will show a render error for that fence, so the draft
 * preview says the same thing.
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
 *
 * ## A drawing opens larger, and that is here rather than on a page
 *
 * DEV-928: "a diagram in the rendered preview cannot be enlarged". A flow of eight boxes
 * drawn inside a 560-pixel preview pane is a flow nobody can read, and the answer is not a
 * bigger pane — it is the one every image viewer has. Clicking the drawing, or pressing
 * Enter or Space on it, opens it over the page at the size of the window, where it can be
 * zoomed and dragged; Escape or a click outside closes it and puts focus back where it was.
 *
 * **It is in this component, so every page that draws a diagram has it.** There are two
 * today — the draft preview's ```` ```mermaid ```` fences, through `MarkdownView`, and an
 * old brief's `diagram` field on the Understand page — and a page-level answer would have
 * been written twice and then diverged.
 *
 * **Only a drawing opens.** A sketch and a diagram that could not be drawn show their source
 * as text; there is nothing to enlarge, and a control over a block of text promising a larger
 * view of it would be a lie the page tells.
 *
 * ### The drawing is moved, not copied
 *
 * The enlarged view holds **the same `<svg>` element**, moved into the overlay and moved
 * back when it closes. Three reasons, and the first is the one that decided it:
 *
 * 1. **A copy would need a second `v-html`.** `test/controls.nuxt.spec.ts` and
 *    `test/step-pages.nuxt.spec.ts` both count the ones in the dashboard and name the reason
 *    for each; "the same string again, over there" is not a new reason, it is a second
 *    surface for the same trust.
 * 2. **mermaid scopes its own stylesheet by the figure's id.** It writes a `<style>` block
 *    of `#{figureId} .node { … }` into the SVG, so a clone with its ids stripped — the
 *    obvious way to keep the document's ids unique — renders unstyled, and a clone that
 *    keeps them duplicates every id in the document.
 * 3. One copy cannot disagree with the other about what the record says.
 *
 * What that costs is two things this component has to do by hand, both small and both
 * load-bearing. The figure **holds the height** its drawing had, so the page behind the
 * backdrop does not jump and the reader comes back to the scroll position they left. And the
 * drawing is moved back **before** the overlay is torn down, including on unmount — Vue
 * removes a teleport's children with it, and the drawing is one of them.
 */
const props = withDefaults(
  defineProps<{
    source: string
    label?: string
    /**
     * Whoever wrote this source said it was a diagram, so it is never a sketch.
     *
     * True for a ```` ```mermaid ```` fence, false for a field that merely *may* hold a
     * diagram. See the note above: the difference is intent, and it is not something this
     * component can read off the text.
     */
    declared?: boolean
  }>(),
  { declared: false },
)

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
      const unknown = (error as Error | undefined)?.name === 'UnknownDiagramError'
      if (unknown && !props.declared) sketch.value = true
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

/* ------------------------------ The drawing, opened larger ----------------------------- */

/**
 * Whether the enlarged view is open.
 *
 * Held here rather than by the caller, which is the opposite of `OpenRow` and `FloatingCard`
 * and right for this one: "one at a time" is a rule about a list of rows, and a figure is not
 * in a list. A page with two diagrams has two independent drawings, and a reader who opened
 * one has said nothing about the other.
 */
const large = ref(false)

/** The scale and the offset the enlarged view is drawn at. */
const scale = ref(1)
const pan = ref<Pan>(NO_PAN)

/** The inline figure's drawing, which is also the control that opens the larger view. */
const inline = ref<HTMLElement | null>(null)
/** The area the drawing is read in: what the wheel zooms and the arrow keys pan. */
const stage = ref<HTMLElement | null>(null)
/** The box inside the stage that carries the zoom and the offset. The drawing goes in it. */
const moved = ref<HTMLElement | null>(null)
/** The dialog itself, which is what Tab is kept inside. */
const sheet = ref<HTMLElement | null>(null)

/** The height the figure holds while its drawing is elsewhere, so the page does not jump. */
const held = ref<string | null>(null)

/** What `document.body.style.overflow` was, so closing restores it rather than guessing. */
let bodyOverflow: string | null = null

/** The one `<svg>` mermaid drew, wherever it is at the moment. */
function drawing(): SVGElement | null {
  const found = inline.value?.querySelector('svg') ?? moved.value?.querySelector('svg')
  return (found ?? null) as SVGElement | null
}

/**
 * Put the drawing where it belongs for the state we are in.
 *
 * Called after the overlay has rendered, and again before it is taken away. It is the only
 * place either element's children are touched: `svg` is written once, in `onMounted`, and
 * never again, so Vue has no reason to patch the inline element's contents after this.
 */
function place(into: HTMLElement | null) {
  const svgEl = drawing()
  if (svgEl && into && svgEl.parentElement !== into) into.appendChild(svgEl)
}

async function openLarge() {
  if (!svg.value || large.value) return
  held.value = inline.value ? `${inline.value.getBoundingClientRect().height}px` : null
  large.value = true
  bodyOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
  await nextTick()
  place(moved.value)
  /*
   * Focus goes to the stage rather than to the close button: the stage is what the arrow
   * keys pan and the wheel zooms, so a reader who opened this with the keyboard can use it
   * without a Tab first. `role="dialog"` and the label say what they have landed in.
   */
  stage.value?.focus()
}

function closeLarge(restoreFocus = true) {
  if (!large.value) return
  /* Back into the figure first: Vue removes a teleport's children with it. */
  place(inline.value)
  large.value = false
  held.value = null
  scale.value = 1
  pan.value = NO_PAN
  if (bodyOverflow != null) document.body.style.overflow = bodyOverflow
  bodyOverflow = null
  /*
   * Focus returns to the drawing, which is the control that opened the view. Escape with
   * focus moved nowhere leaves a keyboard reader at the top of the document, which on the
   * draft page is the whole preview away from where they were — `FloatingCard` argues the
   * same point about the step trail.
   */
  if (restoreFocus) inline.value?.focus()
}

function zoom(factor: number) {
  scale.value = scaleBy(scale.value, factor)
  pan.value = clampedPan(pan.value)
}

/** Back to the size it opened at, in the middle. The way out of being lost. */
function fit() {
  scale.value = 1
  pan.value = NO_PAN
}

/** A pan held inside what the drawing and the stage allow at this scale. */
function clampedPan(to: Pan): Pan {
  const svgEl = drawing()
  const area = stage.value
  if (!svgEl || !area) return to
  /*
   * **The drawing's own size at scale 1**, not its rendered box: `getBoundingClientRect`
   * reports the box *after* the transform, so feeding it back in would compound the scale
   * and the limit would grow every time the view was zoomed.
   */
  const box = svgEl.getBoundingClientRect()
  const drawn = { width: box.width / scale.value, height: box.height / scale.value }
  const size = area.getBoundingClientRect()
  return clampPan(to, drawn, { width: size.width, height: size.height }, scale.value)
}

/* `@wheel.prevent` in the template stops the page scrolling; this only has to zoom. */
function onWheel(event: WheelEvent) {
  zoom(wheelFactor(event.deltaY))
}

/**
 * Every key the enlarged view answers, **on the dialog rather than on the stage**.
 *
 * It was on the stage, and that was a half-answer: pressing the zoom button moves focus to
 * the button, and from there the arrow keys and `+` did nothing at all — a reader who zoomed
 * with the mouse and then reached for the keyboard found it dead. Keydown bubbles out of the
 * stage to here, so focus on the stage still works exactly as it did.
 *
 * Escape is not here: it is on the window, because it should close this from anywhere.
 */
function onDialogKey(event: KeyboardEvent) {
  const by = panByKey(event.key)
  if (by) {
    event.preventDefault()
    pan.value = clampedPan({ x: pan.value.x + by.x, y: pan.value.y + by.y })
    return
  }

  /*
   * `+` and `-` zoom, which is what every image viewer does and what a reader will try
   * before they look for a button. `=` is the unshifted key `+` lives on.
   */
  if (event.key === '+' || event.key === '=') {
    event.preventDefault()
    zoom(SCALE_STEP)
  }
  if (event.key === '-') {
    event.preventDefault()
    zoom(1 / SCALE_STEP)
  }
}

/**
 * The drag in progress, or `null`.
 *
 * **One value, not three.** It was a boolean, a starting point and a pointer id, set
 * together in three statements and cleared together in three more — so "is a drag running"
 * had three spellings and an invariant held by hand, and the guard in `onPointerMove` tested
 * two of them for the same fact. Raised by review. The id is in it because a second finger
 * must not fight the first.
 */
const drag = ref<{ id: number, x: number, y: number, pan: Pan } | null>(null)

function onPointerDown(event: PointerEvent) {
  /*
   * **The primary button only.** `pointerdown` fires for a right-click too, so without this
   * a reader opening the context menu over the diagram started a drag they never asked for
   * and could not end — the matching `pointerup` goes to the menu. Touch and pen both report
   * button 0, so this costs them nothing.
   */
  if (event.button !== 0 || drag.value) return
  drag.value = { id: event.pointerId, x: event.clientX, y: event.clientY, pan: pan.value }
  ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
}

function onPointerMove(event: PointerEvent) {
  const from = drag.value
  if (!from || event.pointerId !== from.id) return
  pan.value = clampedPan({
    x: from.pan.x + (event.clientX - from.x),
    y: from.pan.y + (event.clientY - from.y),
  })
}

function onPointerUp(event: PointerEvent) {
  if (event.pointerId === drag.value?.id) drag.value = null
}

/**
 * **Tab stays inside the dialog**, because `aria-modal="true"` says the rest of the page is
 * not there — and a reader who tabbed out of it would be moving around a page their screen
 * reader has been told is inert, with no way of knowing where they had got to.
 *
 * A cycle rather than `inert` on the app root: this component is mounted inside the page it
 * would have to reach up and disable, and a figure that reached for `#__nuxt` would break
 * the first time it was rendered anywhere else.
 */
function trapTab(event: KeyboardEvent) {
  if (!sheet.value) return
  const stops = [...sheet.value.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
  )].filter(stop => !stop.hasAttribute('disabled'))
  const first = stops[0]
  const last = stops.at(-1)
  if (!first || !last) return

  /*
   * **Focus outside the dialog is the case that mattered, and the first version of this
   * could not see it.** It only acted when the focus was exactly on the first or the last
   * stop — so from anywhere else Tab went wherever the browser pleased, which from the body
   * is the first tabbable thing on the page *behind* an `aria-modal` dialog. Two ways in,
   * both found by review and both a click: on the sheet's own chrome, which is not
   * focusable, and on a control that then disables itself — `Fit`, or `Zoom in` at eight
   * times — because a disabled element cannot hold focus and Chromium drops it to the body.
   */
  const here = document.activeElement
  if (!(here instanceof HTMLElement) || !sheet.value.contains(here)) {
    event.preventDefault()
    ;(event.shiftKey ? last : first).focus()
    return
  }

  if (event.shiftKey && here === first) {
    event.preventDefault()
    last.focus()
  }
  else if (!event.shiftKey && here === last) {
    event.preventDefault()
    first.focus()
  }
}

/**
 * Escape closes it, from wherever focus is.
 *
 * On the window rather than on the stage, because focus may have moved to one of the
 * overlay's own controls — and a reader who pressed the zoom button three times should not
 * have to find their way back to the stage before Escape works.
 */
function onKey(event: KeyboardEvent) {
  if (!large.value) return
  if (event.key === 'Escape') closeLarge()
  /*
   * **Tab is heard here and not on the dialog**, for the same reason Escape is: a key
   * pressed while the focus has fallen to the body never reaches a listener on the sheet,
   * and the whole job of a trap is to act in exactly that case.
   */
  if (event.key === 'Tab') trapTab(event)
}

onMounted(() => window.addEventListener('keydown', onKey))

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  /*
   * Without `restoreFocus`: the element focus would return to is going away in the same
   * tick, and asking for it logs a warning in the browser's console for nothing.
   */
  closeLarge(false)
  sweep()
})
</script>

<template>
  <figure class="figure">
    <!--
      mermaid's own output, from the diagram source in the record. mermaid runs with
      `securityLevel: 'strict'`, which is what makes inserting its SVG safe.
    -->
    <!--
      **A `<button>`, because it is one.** The drawing opens larger when it is activated, and
      Enter and Space come free with the element rather than being two keydown branches that
      have to be remembered. It is still the figure's canvas: a wide diagram scrolls inside
      it at this size, as it always did.
    -->
    <!-- eslint-disable vue/no-v-html -->
    <button
      v-if="svg"
      ref="inline"
      type="button"
      class="canvas"
      :style="held ? { minHeight: held } : undefined"
      :aria-label="`${label ?? 'This diagram'} — open it larger`"
      aria-haspopup="dialog"
      @click="openLarge"
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
    <!--
      The one caption a drawing has: how to open it. `aria-hidden`, because the button above
      already says it in its own accessible name, and a screen reader reading the control and
      then the same sentence again is the "one sentence, twice" the step pages were fixed for.
    -->
    <figcaption
      v-if="svg"
      class="rd-meta hint"
      aria-hidden="true"
    >
      Opens larger
    </figcaption>

    <!--
      **The enlarged view, over the page.** Teleported to the body so no ancestor's overflow
      or stacking context can clip it — which is the thing `position: fixed` cannot promise
      inside a page it does not own.
    -->
    <Teleport
      v-if="large"
      to="body"
    >
      <div
        class="over"
        @pointerdown.self="closeLarge()"
      >
        <!--
          `role="dialog"` and `aria-modal`, which is what this is: the page behind it does
          not scroll and Escape is how it is left. `FloatingCard` deliberately is *not* one —
          it is a card of facts beside the thing it is about — and the difference is that
          this covers the page.
        -->
        <div
          ref="sheet"
          class="sheet"
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          :aria-label="`${label ?? 'This diagram'}, larger`"
          @keydown="onDialogKey"
        >
          <div class="bar">
            <p class="rd-meta reading">
              {{ scaleLabel(scale) }} · drag to move it, or use the arrow keys
            </p>
            <div class="controls">
              <button
                type="button"
                class="rd-btn rd-btn--quiet"
                aria-label="Zoom out"
                :disabled="scale <= SCALE_MIN"
                @click="zoom(1 / SCALE_STEP)"
              >
                −
              </button>
              <button
                type="button"
                class="rd-btn rd-btn--quiet"
                aria-label="Zoom in"
                :disabled="scale >= SCALE_MAX"
                @click="zoom(SCALE_STEP)"
              >
                +
              </button>
              <button
                type="button"
                class="rd-btn rd-btn--quiet"
                :disabled="scale === 1 && pan.x === 0 && pan.y === 0"
                @click="fit"
              >
                Fit
              </button>
              <button
                type="button"
                class="rd-btn"
                @click="closeLarge()"
              >
                Close
              </button>
            </div>
          </div>

          <!--
            The stage the drawing is moved into. `tabindex="0"` because it is what the arrow
            keys pan and the wheel zooms, and focus lands here on opening so a keyboard
            reader can use it without a Tab first.
          -->
          <div
            ref="stage"
            class="stage"
            :class="{ 'stage--dragging': drag }"
            tabindex="0"
            @wheel.prevent="onWheel"
            @pointerdown="onPointerDown"
            @pointermove="onPointerMove"
            @pointerup="onPointerUp"
            @pointercancel="onPointerUp"
          >
            <div
              ref="moved"
              class="moved"
              :style="{
                transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
              }"
            />
          </div>
        </div>
      </div>
    </Teleport>
  </figure>
</template>

<style scoped>
.figure {
  margin: 0;
}

/*
 * A wide diagram scrolls inside its own frame; the page never scrolls sideways.
 *
 * **It is a `<button>` now, so every one of a button's own looks has to be taken back off
 * it.** A control that painted the drawing on a grey slab with a border and centred type
 * would be the page announcing a widget where there was a picture. What is left is the one
 * thing it should have: a pointer, and a focus ring a keyboard reader can see.
 */
.canvas {
  display: block;
  width: 100%;
  max-width: 100%;
  overflow-x: auto;
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: zoom-in;
}

.canvas:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 3px;
}

.canvas :deep(svg) {
  max-width: 100%;
  height: auto;
}

/*
 * The caption under a drawing. Quiet, and the accent only on hover, because it is a note
 * about what the picture can do rather than a control of its own — the control is the
 * picture.
 */
.hint {
  margin-top: 2px;
}

.canvas:hover + .hint,
.canvas:focus-visible + .hint {
  color: var(--rd-accent-dark);
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

/* ------------------------------ The drawing, opened larger ----------------------------- */

/*
 * **Over everything, and the one layer above the page that is not a floating card.**
 * `FloatingCard` says the page stacks nothing above 2 and takes 5 for a card anchored to its
 * control; this covers the page, so it sits above that as well. Teleported to the body, so
 * nothing it is nested in can clip it or cap its layer.
 */
.over {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: flex;
  padding: 24px;
  background: color-mix(in srgb, var(--rd-ink) 55%, transparent);
}

/* The window's size, less the gutter that shows the page is still behind it. */
.sheet {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
  border-radius: var(--rd-radius-card);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-raised);
  overflow: hidden;
}

.bar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  border-bottom: 1px solid var(--rd-line);
}

.reading {
  margin: 0;
}

.controls {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

/*
 * **The stage is the viewport the drawing is read through**, and `overflow: hidden` is what
 * makes the transform a pan rather than a scrollbar. `touch-action: none` so a drag on a
 * phone moves the diagram instead of scrolling the page behind the backdrop.
 */
.stage {
  flex: 1;
  display: grid;
  place-items: center;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--rd-ground);
  cursor: grab;
  touch-action: none;
}

.stage:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -3px;
}

.stage--dragging {
  cursor: grabbing;
}

/*
 * The box the zoom and the offset are on, which the drawing is moved into.
 *
 * **No transition.** A pan follows a pointer, so an eased transform lags behind the finger
 * by whatever the duration is — and a reader dragging a diagram is doing it to line
 * something up.
 */
.moved {
  /*
   * **It fills the stage, and that is load-bearing.** mermaid writes `width="100%"` on its
   * SVG, so the width resolves against this box — and a box sized to its content makes that
   * percentage indefinite, which sends the SVG to the 300×150 default every replaced element
   * falls back to. The drawing came out 300 pixels wide inside a 1392-pixel stage: opening
   * it larger made it smaller than the pane it came from. Measured in the browser, because
   * nothing else in this repository can draw it.
   */
  width: 100%;
  height: 100%;
  transform-origin: center center;
  will-change: transform;
}

/*
 * **The drawing fits the stage, and the SVG's own `viewBox` is what fits it.**
 *
 * `width: 100%; height: 100%` against mermaid's `viewBox` and its default
 * `preserveAspectRatio` — `xMidYMid meet` — scales the drawing to the largest size that fits
 * inside the stage and centres it. So a flow 936 pixels wide in the 560-pixel preview pane
 * opens at about 1392, which is the point of opening it, and a diagram taller than it is
 * wide is fitted by its height instead with nothing cropped.
 *
 * The inline figure's `max-width: 100%` is taken back because the zoom is allowed to take it
 * past the stage; the stage's own `overflow: hidden` is what keeps it off the page.
 */
.moved :deep(svg) {
  display: block;
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
}

/* At a phone's width the gutter goes: 24px of backdrop on each side of a 390px screen is a
 * third of the room the diagram was opened to get. */
@media (max-width: 560px) {
  .over {
    padding: 0;
  }

  .sheet {
    border-radius: 0;
  }
}
</style>
