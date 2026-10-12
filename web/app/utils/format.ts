/*
 * Formatting and wording. Nothing here reads or writes a record.
 * Auto-imported, so a screen calls these by name.
 */

import type {
  DescriptionLint,
  DeskError,
  InboxPostedItem,
  InboxReplyEvent,
  Rule,
  Instant,
  OutcomeKind,
  PrState,
  ReviewEvent,
  ReviewLayerName,
  ReviewLayerState,
  ReviewListItem,
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

/**
 * How each severity is counted, as one and as many. **`should` has no plural.**
 *
 * One table, read by this tally and by the draft page's held-back groups. It was written
 * out twice — found by review — and the two copies are the kind that drift quietly: a
 * heading reading "3 shoulds" is wrong in a way nothing fails on.
 *
 * Keyed off the record's four severities rather than listed as an array, so a fifth added
 * to the contract is a type error here rather than a heading with no plural rule.
 */
export const SEVERITY_WORDS: Record<keyof SeverityCounts, [string, string]> = {
  blocker: ['blocker', 'blockers'],
  should: ['should', 'should'],
  nit: ['nit', 'nits'],
  question: ['question', 'questions'],
}

/**
 * "1 blocker, 2 should" — what a reader wants before opening a draft.
 *
 * **In the contract's own enum order**, which is not the order the code review page sorts
 * findings in: there, a question reads above a nit, because a question is something the
 * author can answer and a nit is not. This is a tally rather than a ranking, so it keeps
 * the order the counts are declared in and `SEVERITY_ORDER` keeps the other.
 */
export function severityTally(counts: SeverityCounts): string {
  return (Object.keys(SEVERITY_WORDS) as Array<keyof SeverityCounts>)
    .filter(key => counts[key] > 0)
    .map(key => `${counts[key]} ${counts[key] === 1 ? SEVERITY_WORDS[key][0] : SEVERITY_WORDS[key][1]}`)
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

/**
 * A duration in whole seconds as "17 min".
 *
 * **Null in, null out, and the caller renders the absence.** The API serves a duration the
 * record cannot answer as null — which is every finished step of a review from before
 * DEV-890 — and a formatter that turned that into "0 sec" would be the one thing the whole
 * change is against. Returning null makes the screen decide what to say instead, which is
 * where `NotMeasured` lives.
 *
 * Minutes are the unit a review is read in: under a minute is seconds, under a day is
 * minutes and hours, and anything longer is days. One unit and at most one below it — "1
 * hr 4 min", never "1 hr 4 min 12 sec" — because the figure is read at a glance and the
 * seconds of an hour-long step are noise.
 */
export function durationLabel(seconds: number | null | undefined): string | null {
  if (seconds == null) return null
  if (seconds < 60) return `${seconds} sec`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min`
  const hours = Math.floor(minutes / 60)
  const over = minutes % 60
  if (hours < 24) return over ? `${hours} hr ${over} min` : `${hours} hr`
  const days = Math.floor(hours / 24)
  const spare = hours % 24
  return spare ? `${days} ${days === 1 ? 'day' : 'days'} ${spare} hr` : `${days} ${days === 1 ? 'day' : 'days'}`
}

/**
 * How long a step took, for the line under its name on the trail.
 *
 * **Only for a step that has finished**, which is what DEV-890 asks the trail to show. A
 * step still going has a total too — the passes it has already closed — and showing it was
 * worse than showing nothing: step 2 waiting on the developer read "7 sec", which is how
 * long the agent took before the wait and reads like how long you have been keeping it.
 * The queue is where a step still going is timed, and it times the right thing: how long
 * the review has been in the state it is in.
 *
 * A step that finished with no duration on the record says so rather than drawing a nought
 * — that is every finished step of a review from before DEV-890, and a nought would say it
 * was instant.
 */
export function stepDurationLabel(layer: {
  state: ReviewLayerState
  seconds?: number | null
}): string | null {
  const finished = layer.state === 'done' || layer.state === 'failed' || layer.state === 'skipped'
  if (!finished) return null
  return durationLabel(layer.seconds) ?? 'time not recorded'
}

/**
 * How long the thing has been where it is now — "20 min", from when it got there.
 *
 * `sinceLabel` says "20 min ago", which is the same measurement worded as a point in the
 * past. This is for the places that are about the span: "Code review · 20 min".
 */
export function forLabel(at: Instant | null | undefined, now: Date = new Date()): string | null {
  if (!at) return null
  const seconds = Math.max(0, Math.floor((now.getTime() - new Date(at).getTime()) / 1000))
  return durationLabel(seconds)
}

/**
 * What the queue says about a session, strongest fact first.
 *
 * **"Session state unknown" was all this could say until DEV-890.** Allele owns whether a
 * session is running and tells Review Desk nothing, so `session_alive` is null in every
 * response this server serves — and the row said so, which is honest and useless. What the
 * record does hold is when anything last wrote to the review, and that is what the row
 * shows instead. The two `session_alive` branches are kept for the day Allele reports it:
 * an answer from the system that owns the fact outranks an inference from the record.
 */
export function activityLabel(
  alive: boolean | null | undefined,
  lastActivityAt: Instant | null | undefined,
  now: Date = new Date(),
): string {
  if (alive === true) return 'Session running'
  if (alive === false) return 'Session ended'
  if (!lastActivityAt) return 'No activity recorded'
  return `Last activity ${sinceLabel(lastActivityAt, now)}`
}

/*
 * **There is no `stepNoteRunsLong` here any more, and no character threshold behind it.**
 *
 * It answered one question — is this note too long for the two lines the trail clamped it
 * to — and the trail does not clamp anything now. DEV-917 draws a step's `detail` on one
 * line, ellipsised by the browser, and puts what the note was trying to say on a card of
 * facts: so the length that mattered is the column's, the browser is the only thing that
 * knows it, and a constant here guessing at it from the shell's width was always an
 * estimate of a measurement somebody else was making.
 */

/** A step's outcome on the trail: what the store recorded, else its state. */
export function layerOutcome(layer: {
  state: ReviewLayerState
  detail?: string | null
}): string {
  return layerDetail(layer.detail) || LAYER_OUTCOMES[layer.state]
}

/**
 * When a harvest last read this review — "read 3 min ago" — or null.
 *
 * **DEV-933, and a fact of its own rather than part of the line above.** The step's words
 * and the moment they were written answer different questions: what came back, and
 * whether anything is still looking. The developer who asked what the harvest step was
 * for was asking the second.
 *
 * **It is not on the step's line, and that was measured rather than argued.** A step's
 * column is 154 pixels of text at 1440 and 175 at 390; the detail alone fits exactly,
 * and every way of putting both on one line ran 186 to 239 pixels — so whichever went
 * first, the other was cut. The detail is what the store's four-word rule protects, so
 * the line keeps it and this goes where the trail already puts what will not fit: the
 * card beside the step, and the line's `title`.
 *
 * **Worded here and never stored.** A detail is a frozen string: "read 3 min ago" written
 * into one is a lie fifteen minutes later, and the harvest runs every fifteen minutes.
 * The record keeps the instant — the layer row's `updated_at` — and this says it in words
 * when the page renders. Not on a clock: like `sinceLabel` everywhere else on the desk it
 * is as fresh as the page, so a tab left open all afternoon still says what it said when
 * it loaded.
 *
 * **Only while the step is `running`.** That means the pull request is open and still
 * being watched, so "when was it last looked at" is live. Once it is `done` the pull
 * request has merged or closed and nothing will read it again; "read 2 months ago" beside
 * "merged, 3 replies back" would be a fossil.
 */
export function harvestReadLabel(
  layer: {
    name?: ReviewLayerName
    state: ReviewLayerState
    updated_at?: Instant | null
  },
  now: Date = new Date(),
): string | null {
  if (layer.name !== 'harvest' || layer.state !== 'running' || !layer.updated_at) return null
  return `read ${sinceLabel(layer.updated_at, now)}`
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
 * A GitHub review event, as the developer chose it: "Request changes", not
 * `request_changes`.
 *
 * **One map, two readers.** `SignOffCard` carried this and the posted rows on the queue
 * needed the same three words; a second copy is how two screens come to call one event
 * different things.
 */
export function eventLabel(event: ReviewEvent): string {
  switch (event) {
    case 'request_changes': return 'Request changes'
    case 'comment': return 'Comment'
    case 'approve': return 'Approve'
    default: return event
  }
}

/**
 * What has come back on a posted review, in the record's own terms.
 *
 * **Three states, three sentences, and only one of them has figures in it.** The counts
 * are null when nothing could have found one, and which absence it is matters: a review
 * nothing has read back from GitHub is a different thing from one a harvest ran over and
 * could read nothing of, because the posted review raised its findings in the body alone
 * and there is no comment thread for a reply to attach to.
 *
 * The figures carry their measurement time, because the record only supports a statement
 * about *then*: "0 replies" over a figure taken two hours ago reads as a statement about
 * now, and the author may have answered since.
 */
export function authorResponseLabel(item: InboxPostedItem): string {
  /*
   * **Each figure speaks for itself, and an absent one never silences a measured one.**
   * The three are nulled on their own sources, so a review can be harvested with nothing
   * for a reply to attach to and still have a real count of the code that changed —
   * dropping that because the other two are unknown would lose a measurement to an
   * absence, which is the same fault as the nought this whole rule exists to prevent.
   */
  const said: string[] = []

  /*
   * **Both thread-derived figures absent means nothing could read a thread**, and the
   * row says which absence it is. They are nulled together because they come off the
   * same read: the harvester reaches a thread through a finding's comment id and gets
   * both from it, or neither.
   */
  if (item.replies == null && item.threads_resolved == null) {
    said.push(
      item.harvested_at == null
        ? 'Not harvested yet — nothing has read this review back from GitHub'
        : item.findings === 0
          ? 'No findings were posted, so there is nothing for the author to reply to'
          : `No finding is linked to a comment on GitHub, so no reply could be read (${
            count(item.findings, 'finding')})`,
    )
  }

  /*
   * **Each figure drawn only where there is one**, and never through a `?? 0`. A null
   * coalesced to nought is the exact thing this function exists to prevent, and it is
   * reachable on its own: an outcome recorded by hand gives a review a reply count with
   * no harvest behind it, leaving the resolution count beside it still unmeasured.
   */
  if (item.replies != null) said.push(count(item.replies, 'reply', 'replies'))
  if (item.threads_resolved != null) {
    said.push(`${count(item.threads_resolved, 'thread')} resolved`)
  }

  /*
   * **"findings whose code changed", not "commits since".** DEV-930 asks for the second
   * and the record holds neither: the harvester writes one of these per finding whose
   * file changed, so one commit touching three findings' files writes three.
   */
  if (item.code_changed_after != null) {
    said.push(`${count(item.code_changed_after, 'finding')} with code changed since`)
  }

  /*
   * **The measurement's own time, and only where there is a measurement to date.** "as at
   * 2 hr ago" over a figure taken then is what stops a nought reading as a statement about
   * now; over "not harvested yet" it would be dating an absence.
   *
   * And where a figure exists with no harvest behind it, the row says so — for *any* of
   * the three, not only `replies`. They are nulled on their own sources, so the one that
   * survives without a harvest is not always the first: `review-desk outcome add` can
   * record a resolution without a reply, and that figure needs its basis as much.
   */
  const measured
    = item.replies != null || item.threads_resolved != null || item.code_changed_after != null
  /*
   * **Not where the row has already said it.** When both thread-derived figures are
   * absent the first sentence is "Not harvested yet", and adding "recorded without a
   * harvest" after a figure between them states the same absence twice in one line —
   * reachable through `outcome add --kind code_changed_after` on a review nothing has
   * harvested.
   */
  const saidAlready = item.replies == null && item.threads_resolved == null
  if (item.harvested_at != null) said.push(`as at ${sinceLabel(item.harvested_at)}`)
  else if (measured && !saidAlready) said.push('recorded without a harvest')

  return said.join(' · ')
}

/**
 * What the record knows of a posted pull request's state.
 *
 * `null` is not "open": it is the harvester never having looked, which is every review
 * until `review-desk harvest` runs over it. The group's heading says an author is being
 * waited on, and this is the row saying how much of that the record can actually support.
 */
export function prStateLabel(state: PrState | null | undefined): string {
  if (state == null) return 'pull request state not established'
  return `pull request ${state} at the last harvest`
}

/**
 * Where a review has got to, as the step **and what the step says** — never the step alone.
 *
 * A bare step name is the step a review is *waiting at* when the state is `waiting_on_you`
 * and the step it has *left* when the state is `done`. A posted review's furthest started
 * step is the draft, so a cell reading "Draft" tells a reader it still needs signing off
 * while the queue says it is with the author. "Draft · signed off by the developer" says
 * the same record and misleads nobody.
 *
 * **The outcome comes from `layerOutcome`, which the trail already uses.** Writing the
 * detail out raw here looked like the same thing and was not: `layerDetail` rewrites the
 * one phrase the store and the screen disagree about — the store writes layer 6's detail
 * as "approved by the developer", and DEV-870 reserves "approve" for the GitHub review
 * event, so the screen says "signed off". A second path to the same cell skipped that and
 * would have drawn a word this app spent an issue deciding not to use. It also handles a
 * detail that is present and empty, which `??` does not.
 *
 * A review nothing has reported on is "Not started" and not a step number: the record
 * holds seven pending steps, which is a thing it knows rather than a thing it has not
 * measured.
 *
 * **A review harvested after it was posted reports step 7, and the row says when it was
 * last read.** Until DEV-933 nothing in the harvest path wrote a step, so a posted review
 * reported step 6 and this said so. Now the harvest writes `running` while the pull
 * request is open and `done` once it has settled, and the state word never reaches the
 * row: `layerOutcome` prefers the step's own note, and a harvest always writes one —
 * "nothing back yet", "2 replies back", "merged, 2 replies back".
 *
 * **A `harvested_at` alone does not make it step 7.** A review read before DEV-933 holds
 * the stamp and no step, and the sweep reads reviews still being drafted, where the step
 * is not written at all. Either way the row draws the step the record holds and infers
 * nothing from the stamp — and says no read time, because this says it only for a step 7
 * that is running.
 *
 * **The read time comes from `review.harvested_at`**, the harvester's own stamp, rather
 * than from the step's `updated_at` as the trail's does — this row holds the review and
 * not the layer, and `harvested_at` is the more exact answer to the question anyway. The
 * two are written microseconds apart by the same harvest. A row has the width for it,
 * which the trail's 154-pixel column does not; that argument is in `harvestReadLabel`.
 *
 * A *session* may write step 7 too, and is reported like any other step. What there must
 * not be is a step derived from a harvest time.
 */
export function stepLabel(row: ReviewListItem, now: Date = new Date()): string {
  if (row.step_number == null || row.step_name == null || row.step_state == null) {
    return 'Not started'
  }
  const said = `${layerLabel(row.step_name)} · ${layerOutcome({
    state: row.step_state,
    detail: row.step_detail,
  })}`
  const read = harvestReadLabel({
    name: row.step_name,
    state: row.step_state,
    updated_at: row.review.harvested_at,
  }, now)
  return read ? `${said} · ${read}` : said
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
 * five screens refuse things and a message assembled differently on each would make the
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
 * **One rule, one place, because it had already drifted.** Five screens turn a
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
