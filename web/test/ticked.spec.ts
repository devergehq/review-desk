import { describe, expect, it } from 'vitest'

import { pieces, spoken } from '../app/utils/ticked'

/*
 * **The split one string is drawn in, and the same string as it is heard.**
 *
 * `test/ticked-text.nuxt.spec.ts` mounts the component and asserts what reaches the page.
 * This is the rule underneath it, which until DEV-928 had no test of its own because it had
 * no name of its own — it was a `computed` inside the component.
 *
 * It got one because a second caller appeared and got half of it right: a held-back row's
 * accessible name is a plain string, and the hand-written version of it stripped backticks
 * and left commit ids alone, so a title was shown as seven characters and announced as
 * forty. `spoken` is derived from the same split now, and the second half of this file is
 * what holds the two together.
 */

/** Forty hexadecimal characters, which is what a commit id is. */
const SHA = 'a3f91c2e8b4d7f60a1c5e9b2d8f4a7c3e6b9d2f5'

describe('the pieces a string is drawn in', () => {
  it('is one piece of plain text when there is nothing to mark', () => {
    expect(pieces('a plain sentence')).toEqual([{ kind: 'text', text: 'a plain sentence' }])
  })

  it('draws a backtick pair as code, and drops the marks', () => {
    expect(pieces('the `allowed_columns` list')).toEqual([
      { kind: 'text', text: 'the ' },
      { kind: 'code', text: 'allowed_columns' },
      { kind: 'text', text: ' list' },
    ])
  })

  /*
   * A session's typing error is shown rather than swallowed: the pairs are matched left to
   * right and whatever is left over is text.
   */
  it('leaves an unpaired backtick as a backtick', () => {
    expect(pieces('`a` and `b').map(piece => piece.text)).toEqual(['a', ' and `b'])
  })

  it('shortens a commit id to seven, and keeps the whole', () => {
    expect(pieces(`fixed in ${SHA}`)).toEqual([
      { kind: 'text', text: 'fixed in ' },
      { kind: 'commit', text: 'a3f91c2', whole: SHA },
    ])
  })

  /* The length rule is exact: 41 characters is a run this rule has no opinion about. */
  it('leaves a run of 41 hexadecimal characters alone', () => {
    expect(pieces(`${SHA}a`)).toEqual([{ kind: 'text', text: `${SHA}a` }])
  })

  it('shortens a commit id inside a backtick pair too', () => {
    expect(pieces(`\`${SHA}\``)).toEqual([
      { kind: 'commit', text: 'a3f91c2', whole: SHA },
    ])
  })

  it('is nothing at all for an empty string, and for nothing', () => {
    expect(pieces('')).toEqual([])
    expect(pieces(null)).toEqual([])
    expect(pieces(undefined)).toEqual([])
  })
})

describe('the same string, spoken', () => {
  it('says a sentence as it is written', () => {
    expect(spoken('a plain sentence')).toBe('a plain sentence')
  })

  /*
   * **The marks are not read out.** A title holding a code name was announced as "backtick
   * report underscore definitions dot allowed underscore columns backtick" before this.
   */
  it('says the words inside a backtick pair, without the marks', () => {
    expect(spoken('`report_definitions.allowed_columns` is read twice'))
      .toBe('report_definitions.allowed_columns is read twice')
  })

  /*
   * **Seven, as the eye sees it** — and this is the half the hand-written version got
   * wrong. `TickedText` announces the whole forty, because the short form is on screen
   * beside it and a reader may need to copy the value; a row's *name* has no chip and no
   * focus to put on it, and a name forty characters of hex long is one nobody can listen
   * past.
   */
  it('says a commit id as the seven characters the eye sees', () => {
    expect(spoken(`fixed in ${SHA}`)).toBe('fixed in a3f91c2')
  })

  it('applies both rules to one string', () => {
    expect(spoken(`\`ColumnPolicy\` changed in ${SHA}`))
      .toBe('ColumnPolicy changed in a3f91c2')
  })

  it('says nothing for nothing', () => {
    expect(spoken(null)).toBe('')
    expect(spoken(undefined)).toBe('')
    expect(spoken('')).toBe('')
  })

  /*
   * **The property that made this a module rather than two functions.** Whatever the pieces
   * are drawn as, the spoken form is their drawn text joined — so a third rule added to
   * `pieces` reaches the name without anybody remembering to come back here. This is the
   * test that would have caught the commit id.
   */
  it('is exactly what the pieces draw, for every rule there is', () => {
    for (const text of [
      'plain',
      '`code` and more',
      `a commit ${SHA} here`,
      `\`${SHA}\``,
      '`a` and `b` and `c`',
      'a stray ` tick',
    ]) {
      expect(spoken(text), text).toBe(pieces(text).map(piece => piece.text).join(''))
    }
  })
})
