/**
 * **What is typed in a review's ask box**, held for the review rather than for the page.
 * DEV-1333.
 *
 * The conversation sits beside every tab, and each tab is its own page, so the panel is
 * drawn afresh on each. A question half typed on Findings is still there on Draft because
 * the text is held here, by review, and not in the panel.
 *
 * **`fill` is how something other than the developer's typing sets it** — DEV-1334's
 * `follow_up` suggestion, which fills the box rather than asking. It replaces what is there
 * and asks nothing: only the developer sends.
 */
export function useAskBox(reviewId: number) {
  const text = useState<string>(`ask-box:${reviewId}`, () => '')

  function fill(question: string): void {
    text.value = question
  }

  return { text, fill }
}
