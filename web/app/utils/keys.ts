/*
 * One rule about keystrokes, for every control on a page that has keys.
 *
 * `DecisionTrack` wrote this first and `code-review.vue` needed exactly the same answer for
 * J and K — and a second copy of a rule about *when a key is not a key* is the kind that
 * drifts quietly: the day one of them learns about a new kind of field, a key fires inside
 * it somewhere else.
 */

/**
 * Whether a keystroke was the developer pressing a key at the page, or something else.
 *
 * DEV-876: "Keys shown on a control must actually work, and must not fire while the
 * developer is typing in a field." Three ways that goes wrong, and each has happened to
 * somebody's shortcut:
 *
 * 1. **The target is a field.** An `input`, `textarea`, `select` or anything
 *    `contenteditable` — `G` is a letter in "regrade" and in a reason somebody is typing,
 *    and `J` is a letter in most sentences.
 * 2. **A modifier is held.** `Cmd-E` is the browser's, not ours.
 * 3. **The key is composing.** An IME candidate window is open and these are not keys yet.
 *
 * A keystroke with no target at all is ours: that is the document itself, which is where a
 * key pressed at nothing in particular arrives.
 */
export function keyIsForThePage(event: KeyboardEvent): boolean {
  if (event.metaKey || event.ctrlKey || event.altKey) return false
  if (event.isComposing) return false

  const target = event.target as HTMLElement | null
  if (!target) return true
  if (target.isContentEditable) return false
  const tag = target.tagName
  return tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT'
}
