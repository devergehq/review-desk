import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/*
 * DEV-855 asks that body text meet 4.5:1. That is a number, so it is computed rather than
 * eyeballed: the tokens are read out of the stylesheet, so changing one breaks this.
 *
 * WCAG 2.2 also wants 3:1 for the boundary of something you can interact with, which is
 * why `--rd-control-line` exists: the mockups' border grey is 2.1:1 on white, fine for a
 * rule drawn between two cards and not fine for the edge of a text field.
 */

const css = readFileSync(resolve('app/assets/css/tokens.css'), 'utf8')

function token(name: string): string {
  const match = css.match(new RegExp(`--rd-${name}:\\s*(#[0-9a-fA-F]{6})`))
  if (!match) throw new Error(`token --rd-${name} is not defined`)
  return match[1]!
}

function channel(value: number): number {
  const srgb = value / 255
  return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map(at => channel(parseInt(hex.slice(at, at + 2), 16)))
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!
}

function ratio(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (high! + 0.05) / (low! + 0.05)
}

/** Every foreground-on-background pair the stylesheet and the components actually use. */
const TEXT_PAIRS: Array<[string, string, string]> = [
  ['ink on a card', 'ink', 'surface'],
  ['ink on the ground', 'ink', 'ground'],
  ['ink on the rail', 'ink', 'rail'],
  ['muted on a card', 'ink-muted', 'surface'],
  ['muted on the ground', 'ink-muted', 'ground'],
  ['muted on the rail', 'ink-muted', 'rail'],
  ['dim on a card', 'ink-dim', 'surface'],
  ['dim on the ground', 'ink-dim', 'ground'],
  ['dim on the rail', 'ink-dim', 'rail'],
  ['a link on a card', 'accent', 'surface'],
  ['a link on the ground', 'accent', 'ground'],
  ['the accent wash tag', 'accent-dark', 'accent-wash'],
  ['the warning tag', 'warn-ink', 'warn-wash'],
  /*
   * The two flows on the Understand page. A mark's numeral is reversed out of its fill, and
   * the box it sits in carries the wash — so the wash has to hold body text as well.
   */
  ['a wrong mark on a card', 'wrong', 'surface'],
  ['a wrong mark on the ground', 'wrong', 'ground'],
  ['a wrong mark on its own wash', 'wrong', 'wrong-wash'],
  ['a box on the wrong wash', 'ink-dim', 'wrong-wash'],
  ['a note on the wrong wash', 'ink-muted', 'wrong-wash'],
  ['a box on the fixed wash', 'ink-dim', 'fixed-wash'],
  ['a note on the fixed wash', 'ink-muted', 'fixed-wash'],
  ['a fixed mark on its own wash', 'accent', 'fixed-wash'],
]

const ON_FILL_PAIRS: Array<[string, string, string]> = [
  ['a wrong mark\u2019s numeral', 'surface', 'wrong'],
  ['a fixed mark\u2019s numeral', 'surface', 'accent'],
  ['a primary button', 'surface', 'accent'],
  ['a primary button, hovered', 'surface', 'accent-dark'],
  ['an ink button', 'surface', 'ink'],
]

describe('body text clears 4.5:1', () => {
  it.each(TEXT_PAIRS)('%s', (_name, fore, back) => {
    expect(ratio(token(fore), token(back))).toBeGreaterThanOrEqual(4.5)
  })

  it.each(ON_FILL_PAIRS)('%s', (_name, fore, back) => {
    expect(ratio(token(fore), token(back))).toBeGreaterThanOrEqual(4.5)
  })
})

describe('the edge of a control clears 3:1', () => {
  it.each([
    ['on a card', 'surface'],
    ['on the ground', 'ground'],
  ])('a field border %s', (_name, back) => {
    expect(ratio(token('control-line'), token(back))).toBeGreaterThanOrEqual(3)
  })

  it('the focus ring clears 3:1 against both surfaces', () => {
    expect(ratio(token('accent'), token('surface'))).toBeGreaterThanOrEqual(3)
    expect(ratio(token('accent'), token('ground'))).toBeGreaterThanOrEqual(3)
  })

  it('the mockups’ drawing line is only used for drawing', () => {
    /* Kept, and deliberately below 3:1: it draws dashed edges, never a control. */
    expect(ratio(token('line-strong'), token('surface'))).toBeLessThan(3)
    expect(css).not.toMatch(/\.rd-(btn|field)[^{]*\{[^}]*--rd-line-strong/)
  })
})

/**
 * The tokens a control may set its own height from, and the floor each one has to clear.
 *
 * The approved boards draw controls at four heights — a 44px input inside a field shell, a
 * 46px item in a segmented track, a 50px action, a 56px action that is the point of its
 * card — so "44px" is no longer the only allowed value and a check that insisted on the
 * literal `--rd-control-min` would have pushed a component into declaring a height it does
 * not use. What has to stay true is the floor, which is read out of the stylesheet here
 * rather than asserted of one name.
 */
const HEIGHT_TOKENS = ['control-min', 'control-item', 'action-min', 'action-tall']

describe('nothing you can press is smaller than a finger', () => {
  it.each(HEIGHT_TOKENS)('--rd-%s is at least 44px', (name) => {
    const match = css.match(new RegExp(`--rd-${name}:\\s*(\\d+)px`))
    expect(match, `token --rd-${name} is not defined`).not.toBeNull()
    expect(Number(match![1])).toBeGreaterThanOrEqual(44)
  })

  it('controls are at least 44px tall', () => {
    expect(css).toMatch(/--rd-control-min:\s*44px/)
    expect(css).toMatch(/\.rd-btn\s*\{[^}]*min-height:\s*var\(--rd-control-min\)/)
    expect(css).toMatch(/\.rd-field\s*\{[^}]*min-height:\s*var\(--rd-control-min\)/)
  })

  /*
   * One exception, stated rather than silent: a `<summary>` inside a rendered draft is
   * content, laid out the compact way GitHub lays it out, and clears WCAG 2.2's 24px
   * target minimum on its own.
   */
  it('no component sets a shorter height on something you press', () => {
    const components = readdirSync(resolve('app/components'))
      .filter(name => name.endsWith('.vue'))
      .map(name => ({ name, text: readFileSync(resolve('app/components', name), 'utf8') }))
    expect(components.length).toBeGreaterThan(10)
    const allowed = new RegExp(
      `min-height:\\s*var\\(--rd-(${HEIGHT_TOKENS.map(name => name.replace('-', '\\-')).join('|')})\\)`,
    )
    const offenders = components
      .filter(file => /cursor:\s*pointer/.test(file.text))
      .filter(file => !allowed.test(file.text))
      .filter(file => file.name !== 'MarkdownView.vue')
      .map(file => file.name)
    expect(offenders).toEqual([])
  })

  it('a rendered fold\u2019s summary still clears the 24px target minimum', () => {
    const markdown = readFileSync(resolve('app/components/MarkdownView.vue'), 'utf8')
    const height = markdown.match(/summary\)\s*\{[^}]*min-height:\s*(\d+)px/)
    expect(height).not.toBeNull()
    expect(Number(height![1])).toBeGreaterThanOrEqual(24)
  })
})
