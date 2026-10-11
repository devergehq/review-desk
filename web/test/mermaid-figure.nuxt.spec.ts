// @vitest-environment nuxt
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { beforeAll, describe, expect, it, vi } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { defineComponent, h } from 'vue'

import MermaidFigure from '../app/components/MermaidFigure.vue'

/*
 * What a diagram leaves behind, and what a reader sees when it is not a diagram at all.
 *
 * **DEV-880.** `mermaid.render(id, source)` with no container element appends mermaid's own
 * `<div id="d{id}">` to `document.body` and removes it when the render finishes. Both of
 * mermaid 12.1.0's failure paths throw before that removal: a parse failure draws the
 * bomb-and-"Syntax error in text" graphic into the div and re-throws the parse error one
 * line above the cleanup, and a draw failure paints the same graphic and throws. The
 * dashboard is one page, so the orphan then sits at the bottom of every screen afterwards.
 *
 * **These tests run against real mermaid, not a stub**, which is the only way they can fail
 * when the fix is removed. Reverting `MermaidFigure.vue` turns the four `nothingEscaped`
 * assertions red.
 *
 * One thing jsdom cannot do is draw: it has no `getBBox`, so a *valid* diagram's render
 * throws here and takes the draw-failure path. That is useful — it is how the draw-failure
 * case below is reached without inventing a diagram that breaks mermaid's renderer — but it
 * means "a valid diagram is drawn" is not a claim this file can make. `e2e/review-desk.spec.ts`
 * makes it, in Chromium, against the binary.
 */

/** The sketch from the review that found this, as the session wrote it. */
const SKETCH = `BEFORE (shared surface, implicit flow record)

consumer app --> GET /oauth/authorize --(guest)--> redirect()->guest() writes url.intended
    |
    v
GET /login  (SHARED staff page)
    |
    FortifyServiceProvider reads url.intended`

/** Mermaid by its first word, and rejected by mermaid's grammar. */
const BROKEN = 'flowchart LR\n  A[[[-->'

/** A diagram mermaid parses. jsdom cannot draw it; a browser can. */
const VALID = 'flowchart LR\n  A[Event] --> B[Deliver]'

/**
 * mermaid, imported once before any test runs.
 *
 * The first `await import('mermaid')` is the slow one — a large ESM bundle with no vite
 * dependency cache on a cold run — and every timing below depends on it being over. Paid
 * here, once, rather than inside the first test's budget.
 */
beforeAll(async () => {
  await import('mermaid')
}, 60_000)

/**
 * Let the component's `onMounted` finish.
 *
 * It imports mermaid, parses and may render, none of which `mountSuspended` awaits.
 *
 * **It throws rather than returning what it found.** An earlier version polled for two
 * seconds and then returned the page as it was, which on a slow import is the page
 * *before* mermaid answered — so `pre.written` was still `pre.fallback` and the test
 * failed pointing at the component instead of at the wait. The budget is now well inside
 * the test timeout and running out is an error with its own words.
 */
async function settled(page: { html: () => string }): Promise<string> {
  await vi.waitFor(
    () => {
      const html = page.html()
      if (!html.includes('figcaption') && !html.includes('<svg')) {
        throw new Error('the figure has not settled: no caption and no svg yet')
      }
    },
    { timeout: 10_000, interval: 10 },
  )
  return page.html()
}

/**
 * Everything mermaid could have left outside the component, in one assertion.
 *
 * Three separate things are checked because they fail separately: the container mermaid
 * appends, the bomb it draws into it, and the words it writes beside the bomb.
 */
function nothingEscaped(): void {
  expect([...document.querySelectorAll('[id]')].filter(element =>
    /^d?i?mermaid-/.test(element.id) && !element.closest('figure'),
  )).toEqual([])
  expect(document.querySelectorAll('path.error-icon')).toHaveLength(0)
  expect([...document.querySelectorAll('text')].map(node => node.textContent)).not.toContain(
    'Syntax error in text',
  )
}

describe('a source that is not a diagram at all', () => {
  it('is shown as a sketch, in the diagram\'s place', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    const html = await settled(page)
    expect(page.find('pre.written').exists()).toBe(true)
    expect(html).toContain('FortifyServiceProvider reads url.intended')
  })

  it('says it is shown as written, and does not call it a failure', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    await settled(page)
    expect(page.find('figcaption').text()).toBe('Shown as written.')
    expect(page.html()).not.toContain('could not be drawn')
  })

  it('is content rather than a warning: no frame around it', async () => {
    /*
     * The frame is what says something is missing from the page. A sketch is the content,
     * whole, so it has none — which is also what tells the two states apart on screen and
     * not only in their wording.
     */
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    await settled(page)
    expect(page.find('pre.fallback').exists()).toBe(false)
  })

  it('leaves nothing outside the component', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    await settled(page)
    nothingEscaped()
  })

  it('shows its source as text, never as markup', async () => {
    /*
     * The source is a record written by a review session. This is the one branch that puts
     * it on the page verbatim, so it is the one that would turn a record into a script.
     */
    const page = await mountSuspended(MermaidFigure, {
      props: { source: 'a sketch\n<script>alert(1)</script>\n<img src=x onerror=alert(1)>' },
    })
    const html = await settled(page)
    expect(page.find('pre.written').exists()).toBe(true)
    expect(html).toContain('&lt;script&gt;')
    expect(page.find('pre').element.querySelector('script')).toBeNull()
    expect(page.find('pre').element.querySelector('img')).toBeNull()
    expect(page.find('pre').text()).toContain('<script>alert(1)</script>')
  })
})

describe('a source that was declared a diagram', () => {
  /*
   * A ```mermaid fence is a session saying "this is a diagram". `MarkdownView` passes
   * `declared` for exactly that, so a mistyped type keyword inside a fence reads as a
   * diagram that did not draw rather than as deliberate prose — which is what GitHub will
   * show for the same fence, and the draft preview exists to show what GitHub will show.
   */
  const MISTYPED = 'sequencediagram\n  A ->> B: hello'

  it('is never a sketch, even when mermaid does not recognise it', async () => {
    const page = await mountSuspended(MermaidFigure, {
      props: { source: MISTYPED, declared: true },
    })
    await settled(page)
    expect(page.find('pre.written').exists()).toBe(false)
    expect(page.find('pre.fallback').exists()).toBe(true)
    expect(page.find('figcaption').text()).toBe(
      'This diagram could not be drawn. Its source is above.',
    )
  })

  it('is a sketch when nobody declared it, which is the free field', async () => {
    /* The same source, from a brief's `diagram` field. Same text, different claim. */
    const page = await mountSuspended(MermaidFigure, { props: { source: MISTYPED } })
    await settled(page)
    expect(page.find('pre.written').exists()).toBe(true)
  })

  it('still leaves nothing outside the component', async () => {
    const page = await mountSuspended(MermaidFigure, {
      props: { source: MISTYPED, declared: true },
    })
    await settled(page)
    nothingEscaped()
  })

  it('is what a draft\'s mermaid fence is rendered with', async () => {
    const view = readFileSync(resolve('app/components/MarkdownView.vue'), 'utf8')
    expect(view).toContain('declared')
  })
})

describe('a diagram mermaid knows and cannot draw', () => {
  it('keeps the note that it could not be drawn, for a parse failure', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: BROKEN } })
    await settled(page)
    expect(page.find('pre.fallback').exists()).toBe(true)
    expect(page.find('figcaption').text()).toBe(
      'This diagram could not be drawn. Its source is above.',
    )
  })

  it('leaves nothing outside the component, for a parse failure', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: BROKEN } })
    await settled(page)
    nothingEscaped()
  })

  it('leaves nothing outside the component, for a draw failure', async () => {
    /* Parses, and jsdom has no `getBBox` — so this is mermaid's other failure path. */
    const page = await mountSuspended(MermaidFigure, { props: { source: VALID } })
    await settled(page)
    expect(page.find('pre.fallback').exists()).toBe(true)
    nothingEscaped()
  })
})

describe('more than one failure, and a page that goes away mid-render', () => {
  it('leaves nothing outside the component after three failing figures', async () => {
    /*
     * One app with three figures, rather than three mounts: mermaid clears a previous
     * render of the *same* id on its way in, so figures only ever collect their own
     * leavings. Three ids on one page is the case that needs saying.
     */
    const ThreeFigures = defineComponent({
      setup: () => () => h('div', [
        h(MermaidFigure, { source: SKETCH }),
        h(MermaidFigure, { source: BROKEN }),
        h(MermaidFigure, { source: VALID }),
      ]),
    })
    const page = await mountSuspended(ThreeFigures)
    await vi.waitFor(() => expect(page.findAll('figcaption')).toHaveLength(3), {
      timeout: 10_000,
      interval: 10,
    })
    nothingEscaped()
  })

  it('leaves nothing outside the component when it is unmounted mid-render', async () => {
    /*
     * **With a control, because this test can otherwise pass for the wrong reason.** It
     * unmounts and then waits, and if mermaid had not got as far as appending anything in
     * that window the assertion would hold while proving nothing. So a second figure is
     * mounted at the same moment and *not* unmounted: it settling is what says the window
     * was long enough for mermaid to run, and only then is the first one's absence a fact.
     */
    const gone = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    const control = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    /* Away before mermaid answers, which is a developer clicking off the page. */
    gone.unmount()

    await settled(control)
    expect(control.find('pre.written').exists()).toBe(true)
    nothingEscaped()
  })
})

describe('what the diagram is configured with', () => {
  /*
   * Read from the file, the way `controls.nuxt.spec.ts` reads `tokens.css`. mermaid writes
   * its `fontFamily` into a `<style>` inside the svg it draws, and jsdom cannot draw — so
   * the declaration is what there is to check here, and the browser run is what says it
   * looks right.
   */
  const component = readFileSync(resolve('app/components/MermaidFigure.vue'), 'utf8')
  const tokens = readFileSync(resolve('app/assets/css/tokens.css'), 'utf8')

  it('names the typeface the app uses, and not the one it left', () => {
    /*
     * Tied to the token rather than spelled out twice: the diagram said IBM Plex Sans for a
     * day after the app moved to Instrument Sans, and a test naming the font itself would
     * have let that happen again the next time the stack changes.
     */
    const stack = tokens.match(/--rd-font-sans:\s*([^;]+);/)?.[1]?.trim()
    expect(stack).toBeTruthy()
    /* Quotes stripped on both sides: the component escapes them for a JavaScript string. */
    const named = stack!.split(',').map(part => part.trim().replace(/'/g, ''))
    expect(named).toContain('Instrument Sans')
    for (const family of named) {
      expect(component.replace(/\\'/g, '')).toContain(family)
    }
    expect(component).not.toContain('IBM Plex Sans')
  })

  it('keeps mermaid on the strict security level', () => {
    expect(component).toContain('securityLevel: \'strict\'')
  })

  it('asks mermaid not to draw its own error graphic', () => {
    expect(component).toContain('suppressErrorRendering: true')
  })
})

/* --------------------------- What can and cannot be enlarged -------------------------- */

/*
 * **Only a drawing opens larger**, which is the half of DEV-928's second change that jsdom
 * can measure. The other half — the overlay itself, its zoom, its drag, Escape and the focus
 * that comes back — needs a renderer: `svg` is only ever set by a successful
 * `mermaid.render`, and jsdom has no `getBBox`, so a valid diagram always takes the
 * draw-failure path here. `e2e/review-desk.spec.ts` measures it in Chromium, and the
 * arithmetic behind it is `test/zoom.spec.ts`.
 *
 * So this file holds the states where there is *nothing* to enlarge. They are the ones a
 * screenshot would not show, because what they are about is a control that should not be
 * there: a sketch and a failed diagram show their source as text, and a button over a block
 * of text promising a larger view of it would be a lie the page tells.
 */
describe('what a source that is not a drawing offers', () => {
  it('offers no larger view of a sketch', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    await settled(page)
    expect(page.find('button.canvas').exists()).toBe(false)
    expect(page.html()).not.toContain('Opens larger')
  })

  it('offers no larger view of a diagram that could not be drawn', async () => {
    const page = await mountSuspended(MermaidFigure, {
      props: { source: BROKEN, declared: true },
    })
    await settled(page)
    expect(page.find('figcaption').text()).toContain('could not be drawn')
    expect(page.find('button.canvas').exists()).toBe(false)
  })

  /*
   * And nothing is left over the page by a figure that never had a drawing — the overlay is
   * teleported to the body, so a leak here would be a sheet nobody could close sitting on
   * every screen afterwards, which is DEV-880's own failure in a new costume.
   */
  it('leaves nothing over the page', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    await settled(page)
    expect(document.querySelectorAll('.over')).toHaveLength(0)
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(0)
  })
})

describe('the drawing is moved into the larger view, never copied', () => {
  /*
   * **The rule, read off the file.** A second `v-html` is the obvious way to build this
   * overlay and it is the wrong one: `test/controls.nuxt.spec.ts` counts every `v-html` in
   * the dashboard and names the reason for each, and "the same string again, over there" is
   * not a new reason. Read off the source because the branch that would break it cannot be
   * reached in jsdom.
   */
  const source = readFileSync(
    resolve(__dirname, '../app/components/MermaidFigure.vue'),
    'utf8',
  )

  it('hands the browser markup in exactly one place', () => {
    const uses = source.split('\n').filter(line => line.includes('v-html='))
    expect(uses).toHaveLength(1)
    expect(uses[0]).toContain('v-html="svg"')
  })

  /*
   * mermaid scopes its own stylesheet by the figure's id — `#{figureId} .node { … }` — so a
   * clone with its ids stripped renders unstyled and one that keeps them duplicates every id
   * in the document. The drawing is appended, which moves it.
   */
  it('moves the one element rather than cloning it', () => {
    expect(source).toContain('into.appendChild(svgEl)')
    expect(source).not.toContain('cloneNode')
  })

  /*
   * Vue removes a teleport's children with it, and the drawing is one of them — so the move
   * back has to happen before the overlay goes, on close and on unmount alike.
   */
  it('puts the drawing back before the overlay is taken away', () => {
    const closing = source.slice(source.indexOf('function closeLarge'))
    expect(closing.indexOf('place(inline.value)')).toBeLessThan(closing.indexOf('large.value = false'))
    expect(source).toContain('closeLarge(false)')
  })
})
