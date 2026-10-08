/*
 * Formatting and wording. Nothing here reads or writes a record.
 * Auto-imported, so a screen calls these by name.
 */

import type {
  DescriptionLint,
  Instant,
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

/** What an outcome kind is called in a sentence about an edit. */
export function editVerb(kind: string): string {
  const verbs: Record<string, string> = {
    cut: 'Cut',
    regraded: 'Regraded',
    rewritten: 'Rewrote',
    kept: 'Kept',
    rejected: 'Rejected',
    raised_again: 'Raised again',
    code_changed_after: 'Code changed after',
  }
  return verbs[kind] ?? kind
}

/** A count the server has no measurement for is absent, not nought. */
export function figure(
  value: number | null | undefined,
  of: number | null | undefined = undefined,
): string | null {
  if (value == null) return null
  return of == null ? String(value) : `${value} of ${of}`
}
