/*
 * **What the conversation beside a review draws, decided once.** DEV-1333, to S6 and S7.
 *
 * The panel (`ConversationPanel`) draws what `GET /reviews/{id}/messages` served, and every
 * choice about it is here so `test/conversation.spec.ts` tests the rules rather than a
 * template:
 *
 * - **The server's answer is drawn as it came.** A reply's `answer` is the server's parse
 *   of it against `docs/answer-contract.md`; nothing here checks it again or mends it.
 * - **A reply something later replaced is not drawn.** The server works out `superseded_by`:
 *   a `working` reply is replaced by the next parsed reply to the same question, a `cant` by
 *   the next parsed `answer`. So the thread draws the newest word on each question.
 * - **Asked so far counts the developer's questions**, by the tab each was asked about. In
 *   one way there are none to count — the developer asks in their own session — so the line
 *   is *Written so far*, counting what the session wrote here (DEV-1335, S13).
 * - **A reference is what one way hands the developer's session** instead of a question:
 *   the review, the command that writes here, and where the contract is (DEV-1335).
 * - **A suggested question is offered only when the record can say why it is worth
 *   asking** — `design.md`: a fact the record cannot derive is left out.
 *
 * Nothing here reads or writes a record.
 */

import type { MessageAbout, ReviewDetail, ReviewLayerName, ReviewMessage, Suggestion } from '~/types/desk'

import { whereOf } from './findings'
import { REVIEW_TABS } from './reviewTabs'
import type { ReviewTab } from './reviewTabs'
import { blindPick, builtOption, optionsOf } from './theCase'

/* ---------------------------------- The thread ----------------------------------- */

/** The messages the thread draws, oldest first: every one nothing later replaced. */
export function threadOf(messages: readonly ReviewMessage[]): ReviewMessage[] {
  return messages.filter(message => message.superseded_by == null)
}

/** One tab's count on the *Asked so far* line. */
export interface AskedOnTab {
  tab: ReviewTab
  label: string
  count: number
}

/**
 * **How many questions the developer asked about each tab**, in the strip's order.
 *
 * A developer's `ask`, by its `about.tab`; a send-back note is the Draft's own act and not a
 * question, and a session's reply is not asked. A tab nobody asked about is left off the
 * line rather than drawn as 0, so five tabs fit the column; an empty conversation draws no
 * line at all.
 */
export function askedSoFar(messages: readonly ReviewMessage[]): AskedOnTab[] {
  return REVIEW_TABS
    .map(({ tab, label }) => ({
      tab,
      label,
      count: messages.filter(message =>
        message.author === 'developer' && message.kind === 'ask' && message.about.tab === tab).length,
    }))
    .filter(one => one.count > 0)
}

/**
 * **What the session wrote about each tab**, for one way's *Written so far* line (S13).
 *
 * A session's reply by its `about.tab`, the newest word on each question only — a `working`
 * a later reply replaced is not counted twice. Laid out as `askedSoFar` is: the strip's order,
 * a tab with nothing left off, no line for an empty conversation.
 */
export function writtenSoFar(messages: readonly ReviewMessage[]): AskedOnTab[] {
  return REVIEW_TABS
    .map(({ tab, label }) => ({
      tab,
      label,
      count: messages.filter(message =>
        message.author === 'session' && message.superseded_by == null && message.about.tab === tab).length,
    }))
    .filter(one => one.count > 0)
}

/* ------------------------------------ About ------------------------------------- */

/** The finding a number names, as its key: 9 is `F9`. */
export function findingKey(number: number): string {
  return `F${number}`
}

/** The number in a finding's key, `F9` is 9, or null for anything that is not one. */
export function findingNumber(key: string | null | undefined): number | null {
  const matched = /^F([1-9]\d*)$/.exec(key ?? '')
  return matched ? Number(matched[1]) : null
}

/**
 * **What a question asked now is about**: the tab the developer is on, and the finding
 * selected there. A finding goes with it only on Findings and Replies, the two tabs the
 * server keeps one on, and only when the review has it.
 */
export function aboutHere(
  tab: ReviewTab,
  selected: string | null,
  findings: ReviewDetail['findings'],
): MessageAbout {
  const number = findingNumber(selected)
  const onAFindingTab = tab === 'findings' || tab === 'replies'
  const known = number != null && findings.some(finding => finding.key === selected)
  return { tab, finding: onAFindingTab && known ? number : null }
}

/** The tab's name as the strip writes it. */
function tabLabel(tab: ReviewTab): string {
  return REVIEW_TABS.find(one => one.tab === tab)!.label
}

/**
 * **The About chip's words**, S6's: *the whole review* on Overview, the tab's name
 * elsewhere, and a finding as its key and where it is — *F9 · ColumnPolicy.php:52* — or, on
 * Replies, *the reply on F3*.
 */
export function aboutLabel(about: MessageAbout, findings: ReviewDetail['findings']): string {
  if (about.finding != null) {
    const key = findingKey(about.finding)
    if (about.tab === 'replies') return `the reply on ${key}`
    const finding = findings.find(one => one.key === key)
    return finding && finding.file != null ? `${key} · ${whereOf(finding)}` : key
  }
  return about.tab === 'overview' ? 'the whole review' : tabLabel(about.tab)
}

/**
 * The line over a developer's question: *on F9*, *on Understanding*. Nothing on Overview,
 * which is the whole review and S6 draws as *You* alone.
 */
export function askedOnLabel(about: MessageAbout): string | null {
  if (about.finding != null) {
    return about.tab === 'replies'
      ? `on the reply on ${findingKey(about.finding)}`
      : `on ${findingKey(about.finding)}`
  }
  return about.tab === 'overview' ? null : `on ${tabLabel(about.tab)}`
}

/** The placeholder in the ask box: *Ask about F9…*, else the pull request. */
export function askPlaceholder(about: MessageAbout, pr: number): string {
  return about.finding != null && about.tab === 'findings'
    ? `Ask about ${findingKey(about.finding)}…`
    : `Ask about #${pr}…`
}

/* ------------------------------ Suggested questions ------------------------------ */

const SMALL = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine']

/** A small count in words, as S6 writes *the other seven*. */
function inWords(n: number): string {
  return SMALL[n] ?? String(n)
}

/** One question the empty conversation offers, and what in the record it comes from. */
export interface SuggestedQuestion {
  asks: string
  from: 'approach' | 'disagreement' | 'not_checked' | 'risk'
}

/**
 * **The questions an empty conversation starts with**, S6's four, each only when the record
 * holds what it is about:
 *
 * | Question | Offered when |
 * | -- | -- |
 * | What would you expect to go wrong after this merges? | always: it asks, and asserts nothing |
 * | Why option C over the other seven? | the brief built one option of two or more |
 * | Where did the second reader disagree? | the blind pass picked a different option |
 * | Does what wasn't checked matter here? | the review recorded what it did not check |
 */
export function suggestedQuestions(
  detail: Pick<ReviewDetail, 'brief' | 'not_checked'>,
): SuggestedQuestion[] {
  const out: SuggestedQuestion[] = [
    { asks: 'What would you expect to go wrong after this merges?', from: 'risk' },
  ]
  const brief = detail.brief ?? null
  const built = brief ? builtOption(brief) : null
  const others = brief ? optionsOf(brief).length - 1 : 0
  if (built && others > 0) {
    out.push({ asks: `Why option ${built.key} over the other ${inWords(others)}?`, from: 'approach' })
  }
  const blind = brief ? blindPick(brief) : null
  if (blind && built && blind.key !== built.key) {
    out.push({ asks: 'Where did the second reader disagree?', from: 'disagreement' })
  }
  if (detail.not_checked?.trim()) {
    out.push({ asks: 'Does what wasn’t checked matter here?', from: 'not_checked' })
  }
  return out
}

/** The line under the suggestions, naming what in this review they were written from. */
export function suggestedFrom(questions: readonly SuggestedQuestion[]): string | null {
  const parts = [
    questions.some(one => one.from === 'approach') ? 'its approach' : null,
    questions.some(one => one.from === 'disagreement') ? 'its disagreements' : null,
    questions.some(one => one.from === 'not_checked') ? 'what it didn’t check' : null,
  ].filter((part): part is string => part != null)
  if (parts.length === 0) return null
  const list = parts.length === 1
    ? parts[0]
    : `${parts.slice(0, -1).join(', ')}${parts.length > 2 ? ',' : ''} and ${parts.at(-1)}`
  return `Written from this review: ${list}.`
}

/* ------------------------------ A reply out of shape ------------------------------ */

/** How much of a reply that did not parse is drawn before its fold, as S7 draws it. */
export const FOLDED_OPENING = 200

/** A reply that did not parse: its opening, and how many words the fold holds. */
export interface FoldedReply {
  opening: string
  /** The words after the opening, or 0 when the opening is all of it. */
  rest: number
  /** Everything after the opening, which the fold shows. */
  restText: string
}

/**
 * **The first ~200 characters, cut at a word**, and the rest behind *Show the rest (n
 * words)*. Nothing is dropped: the fold holds every word the opening does not.
 */
export function foldedReply(body: string): FoldedReply {
  const text = body.trim()
  if (text.length <= FOLDED_OPENING) return { opening: text, rest: 0, restText: '' }
  const space = text.lastIndexOf(' ', FOLDED_OPENING)
  const cut = space > FOLDED_OPENING / 2 ? space : FOLDED_OPENING
  const restText = text.slice(cut).trim()
  return {
    opening: text.slice(0, cut).trimEnd(),
    rest: restText.split(/\s+/).filter(Boolean).length,
    restText,
  }
}

/** The re-ask the folded reply's one button sends, naming the reply it is about. */
export function askAgainInTheFormat(reply: Pick<ReviewMessage, 'seq'>): string {
  return `Please send reply ${reply.seq} again in the answer format (docs/answer-contract.md): `
    + 'a headline, points with a basis, and cites.'
}

/* --------------------------------- Suggestions ---------------------------------- */

/** Basis words on a chip, as S7 draws them. */
export const BASIS_LABELS = { ran: 'Ran', read: 'Read', told: 'Told' } as const

const REPLY_CALLS = {
  raise_again: 'Raise it again',
  accept_reason: 'Accept their reason',
  withdraw: 'Withdraw',
} as const

/**
 * **One suggestion in a line**, and the words behind it when it carries any, from the closed
 * list of six (`docs/answer-contract.md`). Drawn inert here; DEV-1334 gives the card its
 * Accept and Dismiss.
 */
export function suggestionLine(suggestion: Suggestion): { line: string, why: string | null } {
  switch (suggestion.type) {
    case 'new_finding':
      return { line: `New finding: ${suggestion.title}`, why: suggestion.where }
    case 'regrade':
      return {
        line: `Regrade ${suggestion.finding} from ${suggestion.from} to ${suggestion.to}`,
        why: suggestion.why,
      }
    case 'add_to_draft':
      return {
        line: suggestion.finding ? `Add to ${suggestion.finding} in the draft` : 'Add to the draft',
        why: suggestion.text ?? null,
      }
    case 'set_aside':
      return { line: `Set ${suggestion.finding} aside`, why: suggestion.why }
    case 'draft_reply':
      return { line: `${REPLY_CALLS[suggestion.call]} on ${suggestion.finding}`, why: suggestion.words }
    case 'follow_up':
      return { line: `Ask next: ${suggestion.question}`, why: null }
  }
}

/* ---------------------------------- One way ------------------------------------ */

/** Where the reply command and its contract are written up, for a session reading a reference. */
export const CONTRACT_POINTER
  = 'README.md, “Talking to the review’s session”, and docs/answer-contract.md'

/**
 * **What *Copy a reference* puts on the clipboard**, for the developer to paste into their own
 * session. DEV-1335, to Patrick's Q4 on DEV-1331: the review's id, the `review-desk message
 * reply` command, and a pointer to the README's contract section.
 *
 * It names what the question is about, as the chip does, so the session asked about F9 knows
 * it. Given an unparsed reply, it asks for that reply again in the format, which is what *Ask
 * it again, in the format* sends in two way. The command carries no `--to`: in one way there
 * is no question in the record to answer, and the session records what it was asked in
 * `asked` instead, which the contract refuses beside `--to`.
 *
 * Text only. Review Desk hands it to the developer; it sends it nowhere.
 */
export function referenceFor(
  review: number,
  step: ReviewLayerName,
  about: MessageAbout,
  findings: ReviewDetail['findings'],
  again: Pick<ReviewMessage, 'seq'> | null = null,
): string {
  return [
    `Review Desk review ${review}, about ${aboutLabel(about, findings)}.`,
    ...(again ? [`Reply ${again.seq} didn’t follow the answer format; send it again in the format.`] : []),
    `Read the conversation: review-desk message list --review ${review}`,
    `Answer with: review-desk message reply --review ${review} --step ${step} --answer -`,
    '(one JSON object; put the question you were asked in "asked")',
    `The contract: ${CONTRACT_POINTER}.`,
  ].join('\n')
}

/**
 * **A fresh session, where Review Desk cannot ask Allele for one**: the command a session
 * the developer starts runs to pick the review up (S11's *Shown as a command you run
 * yourself*). Review Desk starts nothing; with Allele, DEV-1142's request row on the strip is
 * the way.
 */
export function freshSessionCommand(review: number): string {
  return `review-desk message list --review ${review}`
}
