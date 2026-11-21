/*
 * **The sidebar's pull requests, grouped, from two reads the app already makes.**
 *
 * `docs/design.md`, *The application shell*: the sidebar holds "pull requests grouped as
 * being reviewed, waiting on the author, done this week", and the S screens draw a fourth
 * above them, open, *Waiting on you*. Every group is read off the record:
 *
 * | Group | From |
 * | -- | -- |
 * | Waiting on you | `GET /inbox`: `briefs_to_confirm`, `drafts_to_approve`, `author_replies` |
 * | Being reviewed | `GET /inbox`: `in_progress`, `awaiting_session` |
 * | Waiting on the author | `GET /inbox`: `posted_awaiting_author` |
 * | Done this week | `GET /reviews?show=finished`: rows whose `finished_at` is in the last 7 days |
 *
 * **One row per pull request**, as on the queue: an array is asked a question about a
 * review, and one pull request can answer two of them. The first group to claim a pull
 * request keeps it, in the order above — the group that asks something of the developer
 * first.
 *
 * **A finished review GitHub gave no time for is not in *Done this week***, because the
 * record cannot say which week it was. It is left out rather than guessed into one; the
 * Reviews list's *Finished* view still has it.
 *
 * **The developer's own pull requests are not here.** `own_prs_without_reviewer` is a
 * pull request the developer *wrote* that has no reviewer, not a review on the desk; the
 * list page holds it (S1's *Your own*), and the sidebar lists the reviews.
 *
 * Nothing here reads or writes a record. `test/sidebar.spec.ts` is a test of the rules.
 */

import type {
  Inbox,
  InboxPr,
  Instant,
  ReviewList,
  ReviewListItem,
  SessionReading,
} from '~/types/desk'

import type { ReviewState } from './stateWord'

/** One pull request in the sidebar. */
export interface SidebarRow {
  /** `repo#pr`, which is what one-row-per-pull-request is deduplicated on. */
  key: string
  /** The round the row opens, or null for a request no round exists of yet. */
  reviewId: number | null
  pr: number
  /** The recorded title, or null when no round records one. */
  title: string | null
  state: ReviewState
}

export type SidebarGroupId = 'waiting_on_you' | 'being_reviewed' | 'waiting_on_author' | 'done'

export interface SidebarGroup {
  id: SidebarGroupId
  label: string
  rows: SidebarRow[]
}

/** How far back *Done this week* reaches. A week is seven days, not since Monday. */
export const DONE_WITHIN_DAYS = 7

const prKey = (pr: { repo: string, pr: number }) => `${pr.repo}#${pr.pr}`

/** A row from an inbox item, which is always a pull request and usually a round of one. */
function fromInbox(
  pullRequest: InboxPr,
  reviewId: number | null,
  title: string | null | undefined,
  state: ReviewState,
): SidebarRow {
  return {
    key: prKey(pullRequest),
    reviewId,
    pr: pullRequest.pr,
    title: title ?? pullRequest.title ?? null,
    state,
  }
}

/** A row from the Reviews list's *Finished* view. */
function fromFinished(item: ReviewListItem): SidebarRow {
  return {
    key: prKey(item.review),
    reviewId: item.review.id,
    pr: item.review.pr,
    title: item.review.title ?? null,
    state: item.finished_how === 'closed' ? 'closed' : 'merged',
  }
}

/** Whether a finished review finished within the window, which needs a recorded time. */
function finishedWithin(item: ReviewListItem, now: Date): boolean {
  if (item.left_the_desk !== 'finished' || !item.finished_at) return false
  const age = now.getTime() - new Date(item.finished_at).getTime()
  return age >= 0 && age <= DONE_WITHIN_DAYS * 24 * 60 * 60 * 1000
}

/**
 * The four groups, always four and always in this order.
 *
 * **A group with nothing in it is still returned**, empty: the sidebar draws its heading
 * and a "none" line, so a reader can tell an empty group from a missing one. `finished` is
 * null while that read has not answered, which draws *Done this week* empty rather than
 * holding the other three back.
 */
export function sidebarGroups(
  inbox: Inbox,
  finished: ReviewList | null,
  now: Date = new Date(),
): SidebarGroup[] {
  const claimed = new Set<string>()
  const keep = (rows: SidebarRow[]) =>
    rows.filter((row) => {
      if (claimed.has(row.key)) return false
      claimed.add(row.key)
      return true
    })

  const waiting = keep([
    ...inbox.briefs_to_confirm.map(item =>
      fromInbox(item.pull_request, item.review_id, null, 'brief_ready')),
    ...inbox.drafts_to_approve.map(item =>
      fromInbox(item.pull_request, item.review_id, null, 'draft_ready')),
    ...inbox.author_replies.map(item =>
      fromInbox(item.pull_request, item.newest_round.review_id, item.newest_round.title, 'replies_in')),
  ])

  const reviewing = keep([
    ...inbox.in_progress.map(item =>
      fromInbox(item.pull_request, item.newest_round.review_id, item.newest_round.title, 'being_written')),
    /*
     * **Waiting for a session, not "not started"**: the contract's own rule for this array.
     * A round in it may hold an approved draft, a sent-back one or a brief that needed no
     * confirmation — work has been done, and what is true is that no session is working on
     * it now. Only a request with no round at all has not been started.
     */
    ...inbox.awaiting_session.map(item =>
      fromInbox(
        item.pull_request,
        item.review_id,
        null,
        item.review_id == null ? 'not_started' : 'waiting_for_session',
      )),
  ])

  const onAuthor = keep(
    inbox.posted_awaiting_author.map(item =>
      fromInbox(item.pull_request, item.review_id, null, 'posted')),
  )

  const done = keep(
    (finished?.reviews ?? []).filter(item => finishedWithin(item, now)).map(fromFinished),
  )

  return [
    { id: 'waiting_on_you', label: 'Waiting on you', rows: waiting },
    { id: 'being_reviewed', label: 'Being reviewed', rows: reviewing },
    { id: 'waiting_on_author', label: 'Waiting on the author', rows: onAuthor },
    { id: 'done', label: 'Done this week', rows: done },
  ]
}

/**
 * When GitHub was last read, as the record holds it, or null when it holds no such time.
 *
 * **The newest harvest the two reads carry.** Review Desk reads GitHub in the harvest, and
 * a harvest stamps `harvested_at` on the round it read; nothing records a read of GitHub
 * as a whole. So this is the latest of those stamps — which is when GitHub was last read
 * *for a review on the desk* — and null, drawing no line at all, when none is recorded.
 * It is never the time this page loaded, which would be a claim about GitHub that nothing
 * measured.
 */
export function githubReadAt(inbox: Inbox | null, finished: ReviewList | null): Instant | null {
  const stamps = [
    ...(inbox?.posted_awaiting_author ?? []).map(item => item.harvested_at),
    ...(finished?.reviews ?? []).map(item => item.review.harvested_at),
  ].filter((at): at is Instant => typeof at === 'string')

  if (stamps.length === 0) return null
  return stamps.reduce((newest, at) =>
    new Date(at).getTime() > new Date(newest).getTime() ? at : newest)
}

/** The sidebar's agent line: whether Allele answered, in a few words. */
export interface AgentLine {
  /** `on` is Allele answered; `off` is it could not be reached. */
  tone: 'on' | 'off'
  text: string
}

/**
 * The agent line for one review's session reading, or null for no line.
 *
 * **Read from the reading DEV-1140 serves** (`GET /reviews/{id}/session`), the only place
 * the record says anything about Allele, and that reading is per review — so the line is
 * drawn on a review's pages and nowhere else. Off them nothing has asked Allele anything,
 * and a line there would be a claim nobody measured.
 *
 * Absent is silent, as it is on the review's own strip: no socket, or no session recorded,
 * serves `reading: null` and draws nothing. Any other answer is Allele answering, which is
 * what *Allele running* says — whatever the session itself is doing is the strip's to say.
 * Unreachable is the one reading where Allele did not answer, and it says so.
 */
export function agentLine(reading: SessionReading | null | undefined): AgentLine | null {
  if (reading == null) return null
  if (reading.status === 'unreachable') return { tone: 'off', text: 'Can\'t reach Allele' }
  return { tone: 'on', text: 'Allele running' }
}
