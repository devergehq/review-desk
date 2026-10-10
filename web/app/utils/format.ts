/*
 * Formatting and wording. Nothing here reads or writes a record.
 * Auto-imported, so a screen calls these by name.
 */

import type {
  DescriptionLint,
  DeskError,
  InboxReplyEvent,
  Rule,
  Instant,
  OutcomeKind,
  ReviewLayerName,
  ReviewLayerState,
  SeverityCounts,
} from '~/types/desk'

/** An ISO instant as "6 min ago". Never "in 0 min"; the desk does not predict. */
export function sinceLabel(at: Instant | null | undefined, now: Date = new Date()): string {
  if (!at) return 'at an unrecorded time'
  const seconds = Math.max(0, Math.floor((now.getTime() - new Date(at).getTime()) / 1000))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} ${hours === 1 ? 'hour' : 'hours'} ago`
  const days = Math.floor(hours / 24)
  return `${days} ${days === 1 ? 'day' : 'days'} ago`
}

/** "open 3 days", for a pull request nobody has picked up. */
export function openForLabel(at: Instant | null | undefined, now: Date = new Date()): string {
  if (!at) return 'open for an unrecorded time'
  const days = Math.floor((now.getTime() - new Date(at).getTime()) / 86_400_000)
  if (days < 1) return 'open today'
  return `open ${days} ${days === 1 ? 'day' : 'days'}`
}

/** `n` of something, named singular or plural. */
export function count(n: number, one: string, many: string = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`
}

/** "1 blocker, 2 should" — what a reader wants before opening a draft. */
export function severityTally(counts: SeverityCounts): string {
  const order: Array<[keyof SeverityCounts, string, string]> = [
    ['blocker', 'blocker', 'blockers'],
    ['should', 'should', 'should'],
    ['nit', 'nit', 'nits'],
    ['question', 'question', 'questions'],
  ]
  return order
    .filter(([key]) => counts[key] > 0)
    .map(([key, one, many]) => `${counts[key]} ${counts[key] === 1 ? one : many}`)
    .join(', ')
}

const LAYER_LABELS: Record<ReviewLayerName, string> = {
  lint: 'Lint',
  understand: 'Understand',
  alternatives: 'Alternatives',
  solution_map: 'Solution map',
  code_review: 'Code review',
  draft: 'Draft',
  harvest: 'Harvest',
}

/** A layer's name as the screen writes it. */
export function layerLabel(name: ReviewLayerName): string {
  return LAYER_LABELS[name] ?? name
}

/**
 * One of the store's layer details, as the screen words it.
 *
 * **Layer 6 is the developer's own act, and the store calls it `approved by the
 * developer`.** DEV-870 reserves "approve" for the GitHub review event, so the word is
 * changed here rather than in the store: `approve_draft`, the `approved` status and
 * `draft_approved` are the record's vocabulary and keep their names, and the screen's
 * vocabulary is this file's business.
 *
 * Deliberately one entry. A screen that reworded every detail would be a second source of
 * truth for them, so anything with no entry is shown exactly as the store wrote it.
 */
const LAYER_DETAILS: Record<string, string> = {
  'approved by the developer': 'signed off by the developer',
}

/** A layer's detail as the screen writes it, or as the store wrote it. */
export function layerDetail(detail: string | null | undefined): string {
  if (!detail) return ''
  return LAYER_DETAILS[detail] ?? detail
}

/**
 * What a step's outcome says on the trail, in a few words.
 *
 * **The store's own words wherever it has them**, which is `layer.detail` — "3 options",
 * "9 findings, 0 blockers", "waiting on you". A step whose detail is empty still has to say
 * something, because the trail's whole claim is that you can read the state of a review
 * off seven labels; so the fallback is the state, worded, rather than a blank cell.
 */
const LAYER_OUTCOMES: Record<ReviewLayerState, string> = {
  pending: 'not yet',
  running: 'running',
  done: 'done',
  waiting_on_you: 'waiting on you',
  failed: 'failed',
  skipped: 'skipped',
}

/** A step's outcome on the trail: what the store recorded, else its state. */
export function layerOutcome(layer: {
  state: ReviewLayerState
  detail?: string | null
}): string {
  return layerDetail(layer.detail) || LAYER_OUTCOMES[layer.state]
}

/** Which of the three ways a layer pill is drawn. */
export function layerTone(state: ReviewLayerState): 'done' | 'current' | 'pending' | 'failed' {
  if (state === 'done') return 'done'
  if (state === 'waiting_on_you' || state === 'running') return 'current'
  if (state === 'failed') return 'failed'
  return 'pending'
}

/**
 * Whether a session is still running. Allele owns this, so the honest answer is often
 * that nobody has said — and the screen says that rather than assuming it is alive.
 */
export function sessionLabel(alive: boolean | null | undefined): string {
  if (alive === true) return 'session alive'
  if (alive === false) return 'session ended'
  return 'session state unknown'
}

/** What the description lint found on one of the developer's own pull requests. */
export function descriptionLintLabel(lint: DescriptionLint | null | undefined): string {
  if (!lint) return 'description not checked'
  if (lint.passes) return 'description lint passes'
  if (lint.budget_ratio == null) return 'description lint fails'
  return `description is ${lint.budget_ratio}× its budget`
}

/**
 * What an outcome kind is called in a sentence about one finding.
 *
 * **All eleven of them, which is the drift DEV-862 closed.** `replied` and `thread_resolved`
 * are what the harvester writes — a reply on a finding's comment thread, and that thread
 * being resolved — and this map had neither, so the One review screen printed the raw wire
 * string for the two kinds it is most likely to be showing. The map is keyed by
 * `OutcomeKind` rather than by `string`, so a kind added to the contract is a type error
 * here instead of a word nobody wrote — which is how DEV-877's two arrived.
 */
const VERBS: Record<OutcomeKind, string> = {
  cut: 'Cut',
  regraded: 'Regraded',
  rewritten: 'Rewrote',
  kept: 'Kept',
  rejected: 'Rejected',
  ticketed: 'Moved to a ticket',
  withdrawn: 'Withdrew',
  raised_again: 'Raised again',
  replied: 'Replied to',
  thread_resolved: 'Resolved the thread on',
  code_changed_after: 'Code changed after',
}

/** What an outcome kind is called in a sentence about an edit. */
export function editVerb(kind: OutcomeKind | string): string {
  return VERBS[kind as OutcomeKind] ?? kind
}

/**
 * One thing an author or the harvester did, as the Inbox words it.
 *
 * Moved out of the template it used to be a nested ternary in. That expression handled
 * `rejected` and `code_changed_after` and fell through to the wire string with its
 * underscores swapped for spaces — which is how `thread_resolved` would have read as
 * "thread resolved" on the one group built from exactly that kind.
 */
export function replyLabel(event: InboxReplyEvent): string {
  const key = event.finding_key
  switch (event.kind) {
    case 'rejected':
      return `finding ${key} rejected with a reason`
    case 'code_changed_after':
      return `${key} fixed in a later commit`
    case 'replied':
      return `the author replied on ${key}`
    case 'thread_resolved':
      return `${key}'s thread was resolved`
    case 'raised_again':
      return `${key} was raised again`
    case 'kept':
      return `${key} was kept`
    case 'cut':
      return `${key} was cut`
    case 'regraded':
      return `${key} was regraded`
    case 'rewritten':
      return `${key} was rewritten`
    case 'ticketed':
      return `${key} was moved to a ticket`
    case 'withdrawn':
      return `${key} was withdrawn`
  }
}

/** A count the server has no measurement for is absent, not nought. */
export function figure(
  value: number | null | undefined,
  of: number | null | undefined = undefined,
): string | null {
  if (value == null) return null
  return of == null ? String(value) : `${value} of ${of}`
}

/**
 * One of the server's messages, as a sentence on a screen.
 *
 * **The server writes fragments, and deliberately**: "no review with id 1", "this draft is
 * approved and cannot be edited". They are written to be read after a code — a caller
 * branches on `code` and shows `message` — and they carry neither a capital nor a full
 * stop, which is right for a log line and wrong next to another sentence. Joined without
 * this, the screen read "no review with id 1 It may have been opened on another database."
 *
 * Capitalising and closing a sentence is a display decision, so it is made here, once,
 * rather than asked of every message the server might ever write.
 */
export function sentence(text: string): string {
  const trimmed = text.trim()
  if (trimmed.length === 0) return trimmed
  const opened = trimmed[0]!.toUpperCase() + trimmed.slice(1)
  return /[.!?:]$/.test(opened) ? opened : `${opened}.`
}

/**
 * A refused write, as the one sentence a screen shows about it.
 *
 * The contract requires `invalid_value` to name what it would have accepted, and a refusal
 * that showed the complaint without the remedy would waste that. Written once, because
 * four screens refuse things and a message assembled differently on each would make the
 * same refusal read four ways.
 */
export function refusal(error: DeskError): string {
  const said = sentence(error.message)
  if (error.code === 'unreachable') {
    /*
     * The command, because a notice has nowhere else to put it — unlike the screen-filling
     * state, which shows it beside the message as something to copy.
     */
    return `${said} Nothing was recorded. Start Review Desk with review-desk serve, then try again.`
  }
  if (error.code === 'unreadable') {
    return `${said} Nothing was recorded.`
  }
  return error.expected ? `${said} Accepted: ${error.expected}.` : said
}

/**
 * Which tone a refusal is shown in.
 *
 * `blocked` is something Review Desk needed and could not get — the machine rather than
 * the request — so it is boxed and names a remedy. Everything else is a request the desk
 * understood and refused, which the developer answers by changing what they typed.
 */
export function refusalTone(error: DeskError): 'problem' | 'blocked' {
  return error.code === 'github_unavailable'
    || error.code === 'store_unavailable'
    || error.code === 'unreachable'
    || error.code === 'unreadable'
    ? 'blocked'
    : 'problem'
}

/**
 * Why a rule is in the "Going stale" group, when nothing has written a note saying so.
 *
 * The contract's rule is that a rule is stale once no finding has cited it for sixty days
 * and it has not been deferred past today, and `Rule.note` is a free line the server may or
 * may not have filled in. This is what the screen says when it has not: the measurement the
 * grouping was made from, rather than the bare fact of the group.
 */
export function staleLabel(rule: Rule, now: Date = new Date()): string {
  if (rule.last_cited_at == null) {
    return `Nothing has ever cited it. Made ${sinceLabel(rule.created_at, now)}.`
  }
  return `No finding has cited it since ${sinceLabel(rule.last_cited_at, now)}.`
}

/**
 * What a write's answer becomes on screen.
 *
 * **One rule, one place, because it had already drifted.** Four screens turned a
 * `WriteResult` into a notice and three of them agreed; the One review screen wrote its own
 * shape — `{ text, ok }` — and never called [`refusalTone`], so a server that was not
 * answering was shown there in the same tone as a mistyped reason. A notice is the only
 * thing a developer sees when a write is refused, and the whole point of the tone is to say
 * whether the fix is their typing or their machine.
 */
export function noticeFor(
  result: { ok: true, message: string } | { ok: false, error: DeskError },
): { text: string, tone: 'plain' | 'problem' | 'blocked' } {
  return result.ok
    ? { text: result.message, tone: 'plain' }
    : { text: refusal(result.error), tone: refusalTone(result.error) }
}

/** What a notice holds. Declared once; three screens had written it out. */
export type DeskNotice = ReturnType<typeof noticeFor>
