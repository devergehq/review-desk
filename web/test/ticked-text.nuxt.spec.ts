// @vitest-environment nuxt
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import TickedText from '../app/components/TickedText.vue'

/*
 * **One string a session wrote, with each pair of backticks drawn as inline code.**
 *
 * DEV-917's defect 3: on the review this was read against, a problem's detail printed its
 * backticks as characters — "all the back ticks… makes it a little bit harder to see". The
 * words were right and the record was right; the page did not know what the marks meant.
 *
 * What is really being tested here is the half that is not about backticks at all: that
 * **nothing in a session's prose can become markup**. A brief is written by an agent reading
 * a pull request, so its sentences are text somebody else can choose, served to the
 * developer. Every piece goes through Vue's own interpolation, there is no `v-html`, and
 * there is no markdown library — so these tests are the record of that rather than a hope.
 */

async function drawn(text: string | null) {
  return mountSuspended(TickedText, { props: { text } })
}

const COMMIT = 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678'

describe('a session’s sentence, with its code names drawn as code', () => {
  /**
   * The acceptance criterion, word for word: two backtick pairs, one unpaired backtick, and
   * `<script>` in one string.
   */
  it('draws two code elements, one literal backtick, and the angle brackets as text', async () => {
    const page = await drawn(
      'The `url.intended` slot and `redirect()` disagree ` about <script>alert(1)</script>',
    )

    const code = page.findAll('code')
    expect(code).toHaveLength(2)
    expect(code.map(one => one.text())).toEqual(['url.intended', 'redirect()'])

    /* The odd backtick is a character the session typed, so it is drawn. */
    expect(page.text()).toContain('disagree ` about')

    /*
     * **And the angle brackets are text.** `text()` would read a real script element's
     * content too, so the assertion is about the markup: there is no element here that the
     * string asked for.
     */
    expect(page.html()).toContain('&lt;script&gt;')
    expect(page.find('script').exists()).toBe(false)
    expect(page.text()).toContain('<script>alert(1)</script>')
  })

  it('keeps the spacing the session wrote, either side of a chip', async () => {
    /*
     * **No space is added and none is lost.** The pieces are drawn edge to edge with no
     * whitespace between the tags; a newline between two of them would be condensed to a
     * space and this sentence would read "ExportContext , built by".
     */
    const page = await drawn('`ExportContext`, built by `the schedule`.')
    expect(page.text()).toBe('ExportContext, built by the schedule.')
  })

  it('draws nothing at all for an empty or absent string', async () => {
    expect((await drawn(null)).text()).toBe('')
    expect((await drawn('')).text()).toBe('')
    expect((await drawn('')).findAll('code')).toHaveLength(0)
  })

  it('draws a pair of empty backticks as the two characters they are', async () => {
    /* `` is a session typing, not a code name with nothing in it. */
    const page = await drawn('a `` b')
    expect(page.findAll('code')).toHaveLength(0)
    expect(page.text()).toBe('a `` b')
  })

  it('renders no markdown: a star is a star and a hash is a hash', async () => {
    const page = await drawn('**not bold** # not a heading [not](a-link)')
    expect(page.text()).toBe('**not bold** # not a heading [not](a-link)')
    expect(page.find('strong').exists()).toBe(false)
    expect(page.find('a').exists()).toBe(false)
  })
})

describe('a commit id in a sentence', () => {
  it('draws the first seven characters, with the whole value one focus away', async () => {
    const page = await drawn(`Read with no history at ${COMMIT}, the base commit.`)

    const chip = page.find('code.code--commit')
    expect(chip.exists()).toBe(true)
    expect(chip.find('.short').text()).toBe('a1b2c3d')
    /* The whole forty, in the document, drawn when the chip has the focus. */
    expect(chip.find('.whole').text()).toBe(COMMIT)
    expect(chip.attributes('title')).toBe(COMMIT)
    expect(chip.attributes('tabindex')).toBe('0')
    expect(chip.attributes('aria-label')).toBe(`commit ${COMMIT}`)
    /* And the rest of the sentence is untouched. */
    expect(page.text()).toContain('Read with no history at')
    expect(page.text()).toContain(', the base commit.')
  })

  /*
   * **Inside a control, a chip is drawn and nothing else.**
   *
   * `OpenRow`'s accessible name is computed from everything inside its button, so a chip
   * that kept its own `aria-label` put all forty characters into a row whose visible line
   * shows seven — the "name nobody can listen past" the shortening exists to prevent — and
   * its `tabindex` made it a second tab stop inside a control. Found by review of DEV-928.
   */
  it('inside a control, is neither a tab stop nor a name of its own', async () => {
    const page = await mountSuspended(TickedText, {
      props: { text: `fixed in ${COMMIT}`, inControl: true },
    })

    const chip = page.find('code.code--commit')
    expect(chip.exists()).toBe(true)
    expect(chip.find('.short').text()).toBe('a1b2c3d')
    expect(chip.attributes('tabindex')).toBeUndefined()
    expect(chip.attributes('aria-label')).toBeUndefined()
    /* The whole value is still a pointer away, which is all a control can offer. */
    expect(chip.attributes('title')).toBe(COMMIT)
    /* And the forty characters are nowhere in what the row reads out. */
    expect(page.text()).not.toContain(COMMIT)
    expect(page.text()).toContain('fixed in a1b2c3d')
  })

  it('shortens one inside a backtick pair too', async () => {
    const page = await drawn(`the base commit \`${COMMIT}\``)
    const chip = page.find('code.code--commit')
    expect(chip.exists()).toBe(true)
    expect(chip.find('.short').text()).toBe('a1b2c3d')
  })

  it('leaves a run that is not forty hexadecimal characters alone', async () => {
    /* Thirty-nine, forty-one, and forty characters that are not all hex. */
    for (const value of [COMMIT.slice(1), `${COMMIT}9`, `z${COMMIT.slice(1)}`]) {
      const page = await drawn(`at ${value} it was`)
      expect(page.find('code.code--commit').exists(), value).toBe(false)
      expect(page.text()).toContain(value)
    }
  })

  it('draws every commit id in a string, not just the first', async () => {
    const second = COMMIT.replace('a1b2c3d', 'f0e9d8c')
    const page = await drawn(`from ${COMMIT} to ${second}`)
    const chips = page.findAll('code.code--commit')
    expect(chips).toHaveLength(2)
    expect(chips.map(one => one.find('.short').text())).toEqual(['a1b2c3d', 'f0e9d8c'])
  })
})

/*
 * **Which fields go through it, read off the files.**
 *
 * The issue names the fields: the brief's sentences and detail lists, a flow box's note, an
 * option's arguments, a part's summary, a finding's claim, evidence and title, and a
 * source's text. A list of pages is the only way to check that none of them was missed, and
 * it is also what stops the component reaching the one field it must not touch.
 */
describe('where the component is used, and the one place it is not', () => {
  const read = (path: string) => readFileSync(resolve(path), 'utf8')

  it.each([
    ['app/components/ReviewStepPage.vue', 'the headline and the line under it'],
    ['app/components/FlowDiagram.vue', 'a flow box’s note'],
    ['app/pages/reviews/[id]/understand.vue', 'the brief’s sentences and detail lists'],
    ['app/pages/reviews/[id]/alternatives.vue', 'an option’s arguments'],
    ['app/pages/reviews/[id]/solution.vue', 'a part’s summary'],
    ['app/pages/reviews/[id]/code-review.vue', 'a finding’s claim, evidence and title'],
  ])('%s draws %s through it', (path) => {
    expect(read(path)).toContain('<TickedText')
  })

  it('leaves the draft alone, because the draft really is markdown', async () => {
    /*
     * **The one field that is markdown is rendered as markdown, sanitised**, which is
     * `MarkdownView`'s job and was already right. Putting this component anywhere near it
     * would draw a review body's fenced code as a row of inline chips.
     */
    const draft = read('app/pages/reviews/[id]/draft.vue')
    expect(draft).toContain('MarkdownView')
    expect(draft).not.toContain('<TickedText')
  })
})
