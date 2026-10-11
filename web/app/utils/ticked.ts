/*
 * One string a session wrote, split into the pieces it is drawn in — and the same string as
 * it is **spoken**.
 *
 * **This was `TickedText.vue`'s own script until DEV-928.** It moved because a second caller
 * appeared and got one of the two rules right. The draft page's held-back rows pass a
 * finding's title to `OpenRow` as an accessible name, which is a plain string rather than
 * markup, and the first version of that stripped the backticks by hand — so a title holding
 * `` `report_definitions.allowed_columns` `` was no longer announced with its marks, and a
 * title holding a commit id was still *shown* as seven characters and *announced* as forty.
 * Review found the second half. One rule, two homes, and the newer home knew about one of
 * them: the repository's own lesson, recorded on master as "Copy the linter's rule instead
 * of paraphrasing it a fourth time".
 *
 * So the split has one home and both forms are derived from it. A third rule added to
 * [`pieces`] reaches the spoken form without anybody remembering to go and look.
 */

/** One piece of the string, and how it is drawn. */
export interface Piece {
  kind: 'text' | 'code' | 'commit'
  /** What to draw. For a commit, the first seven characters. */
  text: string
  /** The whole 40 characters, for a commit. */
  whole?: string
}

/**
 * A run of exactly 40 hexadecimal characters, bounded by something that is not one.
 *
 * `\b` would not do: it treats `_` as a word character, so `commit_<40 hex>` would not
 * match at all, and it counts a letter beyond `f` as a boundary, so 41 hex characters would
 * match the first 40 of them. The look-arounds say what is meant — nothing alphanumeric
 * either side — which also leaves a 41-character run alone, as a thing this rule has no
 * opinion about rather than one it half-shortens.
 */
const COMMIT = /(?<![0-9a-z])[0-9a-f]{40}(?![0-9a-z])/gi

/** Everything between a pair of backticks. Non-empty, so `` prints as two backticks. */
const TICKED = /`([^`]+)`/g

/** Split one run of plain text into its text and its commit ids. */
function commits(text: string, inCode: boolean): Piece[] {
  const found: Piece[] = []
  let at = 0
  for (const match of text.matchAll(COMMIT)) {
    const start = match.index
    if (start > at) found.push({ kind: inCode ? 'code' : 'text', text: text.slice(at, start) })
    found.push({ kind: 'commit', text: match[0].slice(0, 7), whole: match[0] })
    at = start + match[0].length
  }
  if (at < text.length) found.push({ kind: inCode ? 'code' : 'text', text: text.slice(at) })
  return found
}

/**
 * The string, as the pieces it is drawn in.
 *
 * **An unpaired backtick comes out as a backtick.** The pairs are matched left to right and
 * whatever is left over is plain text, so a sentence with three backticks in it draws one
 * code chip and one literal tick — which is a session's typing error shown rather than a
 * page swallowing a character.
 */
export function pieces(text: string | null | undefined): Piece[] {
  const said = text ?? ''
  const out: Piece[] = []
  let at = 0
  for (const found of said.matchAll(TICKED)) {
    const start = found.index
    if (start > at) out.push(...commits(said.slice(at, start), false))
    out.push(...commits(found[1]!, true))
    at = start + found[0].length
  }
  if (at < said.length) out.push(...commits(said.slice(at), false))
  return out
}

/**
 * The same string, as a screen reader should hear it.
 *
 * For somewhere that takes a plain string rather than markup — `OpenRow`'s accessible name
 * is the one caller today. **Every piece is spoken as the drawing speaks it**, which is the
 * whole point of deriving it from the same split:
 *
 * | Piece | Drawn | Spoken |
 * |---|---|---|
 * | plain text | as written | as written |
 * | a backtick pair | a code chip, no marks | the words, no marks |
 * | a commit id | seven characters, the whole on focus | **seven**, as the eye sees it |
 *
 * The commit is the row the hand-written version got wrong, and it is worth saying which
 * way round the answer goes. `TickedText` announces the *whole* forty characters, because
 * there the short form is on screen beside it and a reader may need to copy the value. Here
 * there is no chip and no focus to put on it: this is a row's name in a list, and a name
 * forty characters of hex long is a name nobody can listen past. Seven is what the eye sees,
 * so seven is what the ear gets.
 */
export function spoken(text: string | null | undefined): string {
  return pieces(text).map(piece => piece.text).join('')
}
