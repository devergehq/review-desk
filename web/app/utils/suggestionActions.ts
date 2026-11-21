/*
 * **What Accept and Dismiss do on one suggestion**, worked out with no reference to the page.
 * DEV-1334, to S8 and S9.
 *
 * The session proposes; the developer decides. **Every Accept is a control the page already
 * has**, reached in one click, and goes through the same store call that control makes:
 *
 * | Suggestion | Accept | The page's own control |
 * | -- | -- | -- |
 * | `new_finding` | adds the finding to *To post* | `addFinding`, which *Turn into a finding* uses too |
 * | `regrade` | `decideFinding` `regrade` | `G` on Findings |
 * | `add_to_draft` | `decideFinding` `rewrite`, the sentence added to the finding's paragraph | `E` on Findings |
 * | `set_aside` | `decideFinding` `cut`, with the session's why as the note | `S` on Findings |
 * | `draft_reply` | fills the reply on Replies; the developer sends it | `R` `A` `W` on Replies |
 * | `follow_up` | fills the ask box; nothing is written | the composer |
 *
 * An `add_to_draft` that names no finding, or names one with no words to add, has no one-click
 * write on the page behind it, so its Accept opens the place where the developer makes it.
 *
 * Each write carries `prompted_by`: the reply's number and the suggestion's place. **Never a
 * session's name** (Patrick, 21 November 2026): the decision is the developer's.
 */

import type {
  AddFindingRequest,
  Answer,
  DecideFindingRequest,
  Finding,
  PromptedBy,
  ReviewMessage,
  Suggestion,
  SuggestionAnswer,
} from '~/types/desk'

/** The three calls a suggested reply can make, as Replies words its buttons. */
export type ReplyCall = Extract<Suggestion, { type: 'draft_reply' }>['call']

/**
 * A reply a `draft_reply` suggestion filled in on Replies, waiting for the developer to send
 * it. Held for the review in `useState`, so it survives the navigation to Replies.
 */
export interface ReplyFill {
  /** The finding answered. */
  seq: number
  call: ReplyCall
  words: string
  prompted_by: PromptedBy
}

/** What pressing Accept does. */
export type Acting
  = | { kind: 'add', request: AddFindingRequest }
    | { kind: 'decide', seq: number, request: DecideFindingRequest }
    | { kind: 'fill-ask', question: string }
    | { kind: 'fill-reply', fill: ReplyFill }
    | { kind: 'open', to: 'findings' | 'draft', finding: string | null }

/** `F9` as 9. */
function seqOf(key: string): number {
  return Number(key.slice(1))
}

/**
 * **What Accept does for this suggestion.**
 *
 * `paragraph` is the text of the finding's block in the draft, for an `add_to_draft`: the
 * sentence is added to it, which is what *Edit wording* with that sentence typed at the end
 * would send. Null when the draft holds no block for it.
 */
export function accepting(
  suggestion: Suggestion,
  prompted: PromptedBy,
  paragraph: string | null = null,
): Acting {
  switch (suggestion.type) {
    case 'new_finding':
      return {
        kind: 'add',
        request: {
          severity: suggestion.severity,
          title: suggestion.title,
          where: suggestion.where,
          basis: suggestion.basis,
          prompted_by: prompted,
        },
      }
    case 'regrade':
      return {
        kind: 'decide',
        seq: seqOf(suggestion.finding),
        request: { decision: 'regrade', severity: suggestion.to, prompted_by: prompted },
      }
    case 'add_to_draft': {
      const finding = suggestion.finding ?? null
      const sentence = suggestion.text?.trim() ?? ''
      if (finding == null) return { kind: 'open', to: 'draft', finding: null }
      if (sentence.length === 0 || paragraph == null) return { kind: 'open', to: 'findings', finding }
      return {
        kind: 'decide',
        seq: seqOf(finding),
        request: {
          decision: 'rewrite',
          text: paragraph.trim().length > 0 ? `${paragraph.trimEnd()} ${sentence}` : sentence,
          prompted_by: prompted,
        },
      }
    }
    case 'set_aside':
      return {
        kind: 'decide',
        seq: seqOf(suggestion.finding),
        request: { decision: 'cut', note: suggestion.why, prompted_by: prompted },
      }
    case 'draft_reply':
      return {
        kind: 'fill-reply',
        fill: {
          seq: seqOf(suggestion.finding),
          call: suggestion.call,
          words: suggestion.words,
          prompted_by: prompted,
        },
      }
    case 'follow_up':
      return { kind: 'fill-ask', question: suggestion.question }
  }
}

/** The Accept button's words: the act, as the page's own control words it. */
export function acceptLabel(suggestion: Suggestion): string {
  switch (suggestion.type) {
    case 'new_finding':
      return 'Add to findings'
    case 'regrade':
      return 'Regrade'
    case 'add_to_draft':
      if (!suggestion.finding) return 'Open the draft'
      return suggestion.text ? 'Add to the wording' : `Edit ${suggestion.finding}’s wording`
    case 'set_aside':
      return 'Set aside'
    case 'draft_reply':
      return 'Fill the reply'
    case 'follow_up':
      return 'Put in the ask box'
  }
}

/**
 * The Dismiss button's words. A regrade is dismissed by keeping what it is, which is how S8
 * words it: *Keep as should*.
 */
export function dismissLabel(suggestion: Suggestion): string {
  if (suggestion.type === 'regrade') return `Keep as ${suggestion.from}`
  return 'Dismiss'
}

/** The developer's answer to the suggestion at this place (from 1), or to the whole answer (null). */
export function answerAt(
  message: Pick<ReviewMessage, 'suggestion_answers'>,
  place: number | null,
): SuggestionAnswer | null {
  return message.suggestion_answers.find(one => (one.suggestion ?? null) === place) ?? null
}

/**
 * **The line an answered suggestion is drawn as**, in place of its buttons: S8's *Added as
 * F16 · should · ran*, and the same shape for the others.
 */
export function answeredLine(suggestion: Suggestion, answer: SuggestionAnswer): string {
  if (answer.verdict === 'dismissed') {
    return answer.reason ? `Dismissed · “${answer.reason}”` : 'Dismissed'
  }
  switch (suggestion.type) {
    case 'new_finding':
      return `Added as ${answer.finding_key ?? 'a finding'} · ${suggestion.severity} · ${suggestion.basis}`
    case 'regrade':
      return `Regraded ${suggestion.finding} to ${suggestion.to}`
    case 'add_to_draft':
      return `Added to ${suggestion.finding ?? 'the draft'}’s wording`
    case 'set_aside':
      return `Set ${suggestion.finding} aside`
    case 'draft_reply':
      return `Answered ${suggestion.finding} on Replies`
    case 'follow_up':
      return 'Asked'
  }
}

/* ----------------------------- Turn into a finding ----------------------------- */

/** The longest a finding's title may be: the contract's `title` limit. */
export const TITLE_MAX = 100

const PATH = /^(\S+):(\d+)$|^\S+\.[A-Za-z0-9]+$/
const FINDING = /^F[1-9]\d*$/

/**
 * **What *Turn into a finding* sends for an answer**, by the one call Accept on a
 * `new_finding` sends. One click, so nothing is asked first: the headline is the title,
 * clipped to a line at a word; the first cite that is a path is where it is, else the first
 * cite; the first point's basis is how it is known, else *read*; and it is a *should*, which
 * the developer regrades with `G` like any other. S6's card: "It goes into To post; you decide
 * it like any other."
 *
 * Null when the answer is not an `answer`, or cites nothing that can say where it is.
 */
export function turnIntoFinding(
  answer: Answer,
  message: Pick<ReviewMessage, 'seq'>,
): AddFindingRequest | null {
  if (answer.kind !== 'answer') return null
  const cites = (answer.cites ?? []).map(cite => cite.trim()).filter(Boolean)
  const where = cites.find(cite => PATH.test(cite)) ?? cites.find(cite => !FINDING.test(cite)) ?? null
  if (where == null) return null
  return {
    severity: 'should',
    title: clip(answer.headline, TITLE_MAX),
    where: clip(where, 200),
    basis: answer.points?.[0]?.basis ?? 'read',
    prompted_by: { message: message.seq, suggestion: null },
  }
}

/** At most `max` characters, cut at a word and marked with an ellipsis when cut. */
export function clip(text: string, max: number): string {
  const said = text.trim()
  if ([...said].length <= max) return said
  const room = [...said].slice(0, max - 1).join('')
  const atWord = room.slice(0, Math.max(room.lastIndexOf(' '), Math.floor(max / 2)))
  return `${atWord.trimEnd()}…`
}

/** The finding an answer was turned into, if the record holds it. */
export function turnedInto(
  message: Pick<ReviewMessage, 'suggestion_answers'>,
  findings: readonly Pick<Finding, 'seq' | 'key'>[],
): Pick<Finding, 'seq' | 'key'> | null {
  const seq = answerAt(message, null)?.finding_seq ?? null
  return seq == null ? null : findings.find(one => one.seq === seq) ?? { seq, key: `F${seq}` }
}
