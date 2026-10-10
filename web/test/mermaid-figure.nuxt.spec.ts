// @vitest-environment nuxt
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
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
 * Let the component's `onMounted` finish.
 *
 * It imports mermaid, parses and may render, none of which `mountSuspended` awaits. Polled
 * rather than slept on, so the file does not pay for a fixed delay it rarely needs.
 */
async function settled(page: { vm: unknown, html: () => string }): Promise<string> {
  for (let tick = 0; tick < 200; tick += 1) {
    const html = page.html()
    if (html.includes('figcaption') || html.includes('<svg')) return html
    await new Promise(resolve => setTimeout(resolve, 10))
  }
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
    for (let tick = 0; tick < 200; tick += 1) {
      if (page.findAll('figcaption').length === 3) break
      await new Promise(resolve => setTimeout(resolve, 10))
    }
    expect(page.findAll('figcaption')).toHaveLength(3)
    nothingEscaped()
  })

  it('leaves nothing outside the component when it is unmounted mid-render', async () => {
    const page = await mountSuspended(MermaidFigure, { props: { source: SKETCH } })
    /* Gone before mermaid has answered, which is a developer clicking away from the page. */
    page.unmount()
    await new Promise(resolve => setTimeout(resolve, 600))
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
