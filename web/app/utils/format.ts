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
 * **All nine of them, which is the drift DEV-862 closed.** `replied` and `thread_resolved`
 * are what the harvester writes — a reply on a finding's comment thread, and that thread
 * being resolved — and this map had neither, so the One review screen printed the raw wire
 * string for the two kinds it is most likely to be showing. The map is keyed by
 * `OutcomeKind` rather than by `string`, so a kind added to the contract is a type error
 * here instead of a word nobody wrote.
 */
const VERBS: Record<OutcomeKind, string> = {
  cut: 'Cut',
  regraded: 'Regraded',
  rewritten: 'Rewrote',
  kept: 'Kept',
  rejected: 'Rejected',
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
 * A refused write, as the one sentence a screen shows about it.
 *
 * The contract requires `invalid_value` to name what it would have accepted, and a refusal
 * that showed the complaint without the remedy would waste that. Written once, because
 * four screens refuse things and a message assembled differently on each would make the
 * same refusal read four ways.
 */
export function refusal(error: DeskError): string {
  if (error.code === 'unreachable' || error.code === 'unreadable') {
    return `${error.message} Nothing was recorded.`
  }
  return error.expected ? `${error.message} Accepted: ${error.expected}.` : error.message
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
