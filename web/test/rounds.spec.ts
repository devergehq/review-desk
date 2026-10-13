/*
 * **A review's rounds, as rules rather than as a render — the two questions `utils/rounds.ts`
 * answers, tested in the order that file states them.**
 *
 * The sentences a page says about *this* round's head, and the one decision in them (what to
 * say when the record cannot count the commits); then where the pull request's *other* rounds
 * are, which is DEV-976. One file because one module, and because a reader who finds this one
 * should not have to know there is a second named almost the same thing.
 */
import { describe, expect, it } from 'vitest'

import {
  baseOnlyLine,
  commitLabel,
  foldedLine,
  movesSince,
  updatedLine,
  withdrawnLine,
  briefWrittenEarlier,
  roundDismissed,
  roundLinks,
  roundsLabel,
} from '~/utils/rounds'
import { loadSource } from '../app/data/sample'

import type {
  Draft,
  HeadUpdate,
  HeadUpdates,
  Review,
  ReviewDetail,
  RoundSummary,
} from '~/types/desk'

function updates(over: Partial<HeadUpdates> = {}): HeadUpdates {
  return {
    since_sha: null,
    moved: 0,
    base_only: 0,
    folded: 0,
    commits: null,
    last_at: null,
    updates: [],

/* ================== Where the pull request's other rounds are ================= */

/** A review out of the fixtures, with the fields a case wants replaced. */
function detail(id: number, over: Partial<ReviewDetail> = {}): ReviewDetail {
  const found = loadSource().reviews.find(one => one.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return { ...found, ...over }
}

/** One round of a pull request, with only what a case cares about spelled out. */
function round(over: Partial<RoundSummary> & { review_id: number, round: number }): RoundSummary {
  return {
    head_sha: 'aaaaaaa',
    created_at: '2026-10-08T10:31:00Z',
    posted_at: null,
    event: null,
    findings: 0,
    commits: 0,
    replies: 0,
    threads_resolved: 0,
    statuses: { new: 0, fixed: 0, still_open: 0, accepted: 0, withdrawn: 0, ticketed: 0 },
    brief_written: false,