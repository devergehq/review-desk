<script setup lang="ts">
import type {
  Inbox,
  InboxBriefItem,
  InboxDraftItem,
  InboxInProgressItem,
  InboxPostedItem,
  InboxReplyItem,
} from '~/types/desk'
import type { DeskNotice } from '~/utils/format'

/**
 * The queue: every review, grouped by who has to act, **one row each**.
 *
 * ## One row per review
 *
 * `GET /inbox` serves five arrays, and the screen this replaces drew one group per array.
 * A review can be in three of them at once — the session wrote a draft before the developer
 * confirmed the brief, and the author has replied to the round before — so the seeded
 * database showed one pull request three times. DEV-876: "A review that needs two things
 * from the developer is one row showing its next action."
 *
 * So the three needs-you arrays are merged by `review_id` and the row shows the **earliest
 * unfinished layer**: the brief at layer 2, then the draft at layer 6, then the author's
 * reply after it. What else is waiting is said on the row's second line rather than given a
 * row of its own.
 *
 * ## What the API cannot answer, and what the screen does about it
 *
 * | The board draws | Source |
 * |---|---|
 * | Waiting on the author | `posted_awaiting_author` — **DEV-930** |
 * | The age of a draft, a reply or a running session | `draft_ready_at`, `replied_at`, `in_state_since` — **DEV-890** |
 * | "Session not seen for 20 min" | `last_activity_at`, which is when anything last wrote to the review |
 *
 * **All three were "None" once**, and the rows said so where the figure goes. What changed
 * each time is the record, not the screen's rule: each step keeps every state it entered,
 * so there is a time to show for a draft waiting, a reply and a session's current step; and
 * a posted review now has a group of its own, so the one group the board drew that nothing
 * could fill is filled. `session_alive` is still Allele's to answer and still null, and the
 * rows still say so rather than assuming a session is alive.
 *
 * A row whose figure is still absent says so where the figure goes. Inventing one would be
 * the one thing the brief forbids — "a figure with no data source is shown as unknown,
 * never as a number" — and the posted group is where that rule earns its keep twice over:
 * a review nothing has harvested and a review a harvest could read nothing of are two
 * absences, and the row says which.
 *
 * ## One row per review, across groups as well as within them
 *
 * A posted review whose author has answered is **not** drawn under "Waiting on the author".
 * It is already under "Needs you" — the author has replied and that is the developer's turn
 * — and drawing it twice is the defect DEV-876 exists to prevent. The group says how many
 * it left out and where they went, so it cannot read as "no review is waiting on an
 * author", and **the figures about the author's response are drawn on the row that is about
 * the author's response**: the needs-you row, which says "2 replies · 1 thread resolved"
 * where it used to say only which finding was replied to.
 *
 * Two facts the boards do not draw and the API does serve are kept as one line each at the
 * foot: what was handled without the developer, and a rule proposal waiting on them.
 */
const desk = useReviewDesk()
const inbox = desk.inbox()

useHead({ title: 'Queue · Review Desk' })

const link = ref('')
const starting = ref(false)

/*
 * What the desk said back. `blocked` is for something Review Desk needed and could not
 * get — the developer's `gh` is missing or logged out, or its own database is unreadable:
 * nothing was created, and the fix is a command rather than a different link.
 */
const notice = ref<DeskNotice | null>(null)

async function start() {
  starting.value = true
  const result = await desk.startReview(link.value)
  starting.value = false

  notice.value = noticeFor(result)
  /* The link stays in the field when it is refused: retyping what was right is waste. */
  if (result.ok) link.value = ''
}

/** Which repository the queue is showing. `ALL_REPOSITORIES` for every one. */
const repo = ref(ALL_REPOSITORIES)

/* ------------------------------- The rows ------------------------------- */

/** What one review is waiting for, in the order the layers run. */
interface Waiting {
  review_id: number
  repo: string
  pr: number
  title: string
  round: number | null
  ticket: string | null
  is_delta: boolean
  brief: InboxBriefItem | null
  draft: InboxDraftItem | null
  reply: InboxReplyItem | null
  /**
   * The in-progress item for this review, when a session is also working on it.
   *
   * **Joined in for the reason `posted` below is**: the row is drawn here and not under
   * "With the agent", so what that group would have said about the session has to be said
   * on this row or not at all.
   */
  agent: InboxInProgressItem | null
  /**
   * The posted-review item for this review, when there is one.
   *
   * **Joined in so that the author's figures are drawn on the row they describe.** A
   * review whose author has replied is here rather than under "Waiting on the author",
   * and the counts of what came back belong on the row saying what came back — not in a
   * group this review is deliberately not in.
   */
  posted: InboxPostedItem | null
}

/**
 * Every review that is waiting on the developer, merged into one row each.
 *
 * Called through `waiting` below rather than directly, because every reader of it wants
 * the same answer about the same inbox.
 */
function needingYou(data: Inbox): Waiting[] {
  const byReview = new Map<number, Waiting>()

  const slot = (
    id: number,
    pull: { repo: string, pr: number, title?: string | null },
  ): Waiting => {
    const held = byReview.get(id)
    if (held) return held
    const made: Waiting = {
      review_id: id,
      repo: pull.repo,
      pr: pull.pr,
      title: pull.title ?? 'no title recorded',
      round: null,
      ticket: null,
      is_delta: false,
      brief: null,
      draft: null,
      reply: null,
      agent: null,
      posted: null,
    }
    byReview.set(id, made)
    return made
  }

  for (const item of data.briefs_to_confirm) {
    const row = slot(item.review_id, item.pull_request)
    row.brief = item
    row.ticket = item.ticket_id ?? null
  }
  for (const item of data.drafts_to_approve) {
    const row = slot(item.review_id, item.pull_request)
    row.draft = item
    row.round = item.round
    row.is_delta = item.is_delta
  }
  for (const item of data.author_replies) {
    const row = slot(item.review_id, item.pull_request)
    row.reply = item
  }
  /*
   * **Joined, never slotted.** Neither of these makes a row: a posted review and a review
   * with a session running are only here if one of the three groups above put them here.
   * What they add is what the group they are *not* drawn in would have said.
   */
  for (const item of data.posted_awaiting_author) {
    const row = byReview.get(item.review_id)
    if (row) row.posted = item
  }
  for (const item of data.in_progress) {
    const row = byReview.get(item.review_id)
    if (row) row.agent = item
  }

  return [...byReview.values()]
}

/**
 * The merge, done once per answer.
 *
 * Four things read it — the rows, the posted group's subtraction, the note beside that
 * group's heading, and the repository chips — and it was a plain function call at each,
 * so it rebuilt its `Map` and walked three arrays five times per render, including on
 * every press of a repository chip. A `computed` over the one thing it depends on is both
 * smaller than the calls it replaces and the shape the rest of this file already uses.
 */
const waiting = computed(() => (inbox.data.value ? needingYou(inbox.data.value) : []))

/**
 * Where a row goes: **Understand, whatever the row is waiting for.**
 *
 * Patrick, on tc-portal #9584: "when we click a review, we want to go to the review, not to
 * the draft." In a page-per-step review the review *starts* at Understand — his order is
 * understand, alternatives, solution, then the code, then the draft — so every row lands
 * there and the reading happens in the order it was designed to happen in.
 *
 * **The trail is what says which step is waiting**, and it is a better place for that than
 * the destination of a click: it says so about all seven steps at once, where a
 * destination can only say it about one. From Understand the draft is one click along the
 * trail, which is also the click that walks you past the alternatives and the solution on
 * the way — the reading that precedes a sign-off.
 */
function stepFor(row: Waiting): string {
  return `/reviews/${row.review_id}/understand`
}

/** The next thing the developer does about this review, and what to say about it. */
function nextAction(row: Waiting) {
  if (row.brief) {
    return {
      what: 'Problem brief ready',
      detail: row.brief.blocks_later_layers
        ? 'nothing else runs until you confirm'
        : 'nothing posts until you confirm',
      action: 'Read the brief',
      to: stepFor(row),
      at: row.brief.brief_ready_at ?? null,
      atUnknown: 'the brief carries no ready time',
    }
  }
  if (row.draft) {
    const tally = severityTally(row.draft.severities)
    const held
      = row.draft.held_back > 0 ? `${count(row.draft.held_back, 'nit')} to a ticket` : ''
    return {
      what: 'Draft ready',
      detail: [tally, held].filter(Boolean).join(' · ') || 'nothing in the draft yet',
      action: 'Read and sign off',
      to: stepFor(row),
      at: row.draft.draft_ready_at ?? null,
      atUnknown: 'this review was recorded before Review Desk kept when a draft became ready',
    }
  }
  const first = row.reply!.events[0]!
  return {
    what: `The author replied to ${first.finding_key}`,
    /*
     * **What came back, in figures, where the record has them.** These are the counts the
     * posted-review group carries, and this is the row they are about — the review is
     * here rather than in that group precisely because something has come back on it, so
     * drawing them there would have meant drawing them nowhere.
     */
    detail: row.posted
      ? `${first.detail ?? replyLabel(first)} · ${authorResponseLabel(row.posted)}`
      : (first.detail ?? replyLabel(first)),
    action: 'Read the reply',
    to: stepFor(row),
    at: row.reply!.replied_at ?? null,
    atUnknown: 'no outcome on this review carries a time',
  }
}

/** What else is waiting behind the next action, so one row can say it needs two things. */
function alsoWaiting(row: Waiting): string[] {
  const also: string[] = []
  if (row.brief && row.draft) also.push('a draft is ready behind it')
  if (row.brief && row.reply) also.push('the author has replied too')
  else if (row.draft && row.reply) also.push('the author has replied too')
  /*
   * **And the session, which is why this row is not under "With the agent".** That group
   * draws the step, a bar and how long the step has been running; this review is drawn
   * here instead, so without this line those facts are not on the page at all — the row
   * would be the quieter for having been deduplicated, which is the opposite of the
   * point.
   */
  if (row.agent) {
    const running = layerLabel(row.agent.layer_name).toLowerCase()
    const forHow = row.agent.in_state_since ? ` for ${forLabel(row.agent.in_state_since)}` : ''
    also.push(
      `a session is on step ${row.agent.layer_number} of ${row.agent.layers_total}, `
      + `${running}${forHow}`,
    )
  }
  return also
}

/** The second line under a review's title: where it is, which round, what changed. */
function about(row: Waiting): string {
  const parts = [row.repo]
  if (row.round != null) parts.push(`round ${row.round}`)
  if (row.is_delta) parts.push('changes since your last review')
  if (row.ticket) parts.push(row.ticket)
  return parts.join(' · ')
}

/* --------------------------- The repository filter --------------------------- */

/**
 * The repositories with something in the queue, each with the number of rows it has.
 *
 * Counted from the rows themselves rather than from `GET /repos`, so the number beside a
 * repository is exactly the number of rows pressing it shows. `/repos` counts open reviews,
 * which is a different and larger number, and a filter whose count does not match what it
 * filters to is worse than no count.
 */
function repoTabs(data: Inbox) {
  const tally = new Map<string, number>()
  const bump = (name: string) => tally.set(name, (tally.get(name) ?? 0) + 1)
  for (const row of waiting.value) bump(row.repo)
  for (const item of withAgent.value) bump(item.pull_request.repo)
  for (const item of data.own_prs_without_reviewer) bump(item.pull_request.repo)
  /*
   * The posted rows the page actually draws, which is the group minus the reviews already
   * counted under "Needs you". A chip saying 4 over five rows is the failure the comment
   * above names: "a filter whose count does not match what it filters to is worse than no
   * count."
   */
  for (const item of withAuthor.value) bump(item.pull_request.repo)

  return [
    { value: ALL_REPOSITORIES, label: 'All repositories' },
    ...[...tally.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, rows]) => ({ value: name, label: name, count: rows })),
  ]
}

/**
 * Every review this page draws above the posted group — needs-you **and** with-the-agent.
 *
 * One row per review is DEV-876's rule and it holds across groups, not only within them,
 * so a posted review has to be subtracted from this whole set rather than from the
 * needs-you merge alone. Both halves are reachable:
 *
 * - its author has answered, which puts it in `author_replies` — and makes it the
 *   developer's turn rather than the author's
 * - a session is running a layer on it, which puts it in `in_progress`. The server's
 *   query has no draft-status guard — any layer `running` qualifies — so a posted review
 *   whose harvest step is running is in both arrays at once.
 */
const drawnAbove = computed(() => {
  const drawn = new Set(waiting.value.map(row => row.review_id))
  for (const item of withAgent.value) drawn.add(item.review_id)
  return drawn
})

/**
 * The reviews a session is working on, minus the ones already waiting on the developer.
 *
 * **The same rule, one group earlier.** `reviews_in_progress` asks only whether a layer is
 * `running`, and nothing stops a review being there *and* in one of the needs-you arrays:
 * the harvester records an author's reply while a session has the harvest step running,
 * and the review is in `author_replies` and `in_progress` at once. Drawn from both, one
 * review becomes two rows — the defect DEV-876 exists to prevent, one group further up
 * than the posted group where it was first noticed.
 *
 * It stays where the developer has something to do about it. "Needs you" is a to-do list;
 * "With the agent" is captioned "nothing for you to do", which stops being true the
 * moment the author has answered.
 */
const withAgent = computed(() => {
  const needing = new Set(waiting.value.map(row => row.review_id))
  return (inbox.data.value?.in_progress ?? []).filter(
    item => !needing.has(item.review_id),
  )
})

/** How many reviews a session is working on are drawn above, for the note beside them. */
const alsoWithAgent = computed(() => {
  const needing = new Set(waiting.value.map(row => row.review_id))
  return (inbox.data.value?.in_progress ?? []).filter(
    item => showing(item.pull_request.repo) && needing.has(item.review_id),
  ).length
})

/**
 * The posted reviews this page draws under "Waiting on the author", before the picker.
 *
 * **The group, minus whatever is already drawn above it.** A review that is waiting on the
 * developer, or that a session is working on, is not waiting on its author — it belongs to
 * the earlier group, with the figures about the author's response on it. How many were left
 * out is said beside the heading, because a count that silently disagrees with the record
 * is worse than no count.
 *
 * A computed over one source rather than a function taking the inbox, because half of it
 * — `waiting` — is already one: a function reading the slot's `data` for one half and the
 * resource for the other would be two answers to the same question, equal only because the
 * slot happens to be handed the resource's own value.
 */
const withAuthor = computed(() =>
  (inbox.data.value?.posted_awaiting_author ?? [])
    .filter(item => !drawnAbove.value.has(item.review_id)))

/**
 * How many posted reviews the group left to the groups above it, for the note beside it.
 *
 * **Counted over the repository being shown**, which is the whole point of the note: the
 * rows are filtered by the picker, so a count taken over the whole record would point at
 * a row that is not on the page — "1 more is posted and is under Needs you" over a review
 * on a repository the reader has filtered out.
 */
const alsoPosted = computed(() =>
  (inbox.data.value?.posted_awaiting_author ?? []).filter(
    item => showing(item.pull_request.repo) && drawnAbove.value.has(item.review_id),
  ).length)

const showing = (name: string) => repo.value === ALL_REPOSITORIES || repo.value === name

/* ------------------------------ The groups ------------------------------ */

/**
 * Oldest first, with the rows whose time has no source after the ones that have it.
 *
 * The board says "Oldest first" and said it over groups that carried no time at all, so the
 * caption says what this really did rather than implying a sort the data cannot support.
 * Every group has a time to sort by now — the posted rows by when they were posted, which
 * is the last of the four to get one — and a row whose own time is still absent sorts last
 * and says so where the figure goes.
 */
function oldestFirst<T>(rows: T[], at: (row: T) => string | null): T[] {
  return [...rows].sort((a, b) => {
    const left = at(a)
    const right = at(b)
    if (left == null && right == null) return 0
    if (left == null) return 1
    if (right == null) return -1
    return new Date(left).getTime() - new Date(right).getTime()
  })
}

/** One needs-you row, display-ready: the merge, its next action and what is behind it. */
function queueRow(row: Waiting) {
  const next = nextAction(row)
  return {
    ...next,
    review_id: row.review_id,
    pr: row.pr,
    title: row.title,
    about: about(row),
    detail: [next.detail, ...alsoWaiting(row)].join(' · '),
  }
}

const rows = computed(() => {
  const data = inbox.data.value
  if (!data) {
    return {
      needsYou: [],
      withAgent: [],
      ownPrs: [],
      withAuthor: [],
      alsoPosted: 0,
      alsoWithAgent: 0,
    }
  }

  const needsYou = oldestFirst(
    waiting.value.filter(row => showing(row.repo)).map(queueRow),
    row => row.at,
  )
  // Sorted like every other group, now that it has a time to sort by: the caption over
  // the page says "oldest first", and until DEV-890 this group was the one it could not
  // be true of. The store serves these newest first, which is the opposite.
  const agentRows: InboxInProgressItem[] = oldestFirst(
    withAgent.value.filter(item => showing(item.pull_request.repo)),
    item => item.in_state_since ?? null,
  )
  const ownPrs = oldestFirst(
    data.own_prs_without_reviewer.filter(item => showing(item.pull_request.repo)),
    item => item.opened_at ?? null,
  )
  // Oldest posting first, like every other group, so the caption over the page stays
  // true: the API serves these newest first, which is the opposite.
  const shown = oldestFirst(
    withAuthor.value.filter(item => showing(item.pull_request.repo)),
    item => item.posted_at ?? null,
  )
  return {
    needsYou,
    withAgent: agentRows,
    ownPrs,
    withAuthor: shown,
    alsoPosted: alsoPosted.value,
    alsoWithAgent: alsoWithAgent.value,
  }
})

/** Whether any row on the page has no recorded time, which the caption then says. */
const someTimesMissing = computed(
  () =>
    rows.value.needsYou.some(row => row.at == null)
    || rows.value.withAgent.some(item => item.in_state_since == null)
    || rows.value.ownPrs.some(item => item.opened_at == null)
    || rows.value.withAuthor.some(item => item.posted_at == null),
)

/**
 * Where a pull request is on GitHub.
 *
 * **Written as a function because `//` cannot go in a component's interpolated attribute.**
 * `vue-tsc` emits a component's props as one line, so the `//` in `https://` starts a
 * JavaScript line comment and everything after it in the template is commented out: every
 * later `item` and `data` became `TS2304: Cannot find name`, seventeen of them. Measured
 * rather than guessed — the same attribute with one slash reports 0 errors and with two
 * reports 17. It works inline on a *native* element, which is why the screen this replaces
 * could write the same URL in an `<a :href>`. The URL built here is identical.
 */
function prUrl(repo: string, pr: number): string {
  return `https://github.com/${repo}/pull/${pr}`
}

/**
 * The line under the headline: what is on the desk that is not waiting on the developer.
 *
 * The board reads "2 with the agent · 2 waiting on the author · 2 of your own". Only the
 * parts with something in them are said — "0 with the agent · 0 of your own" is noise on a
 * database with one review — and since DEV-930 the author's part is one of them, where
 * before it had to be left out because nothing could count it.
 *
 * **It is also the signpost for a group that is below the fold at 390 pixels.** The posted
 * group sits under the needs-you rows, and on a phone that is off the bottom of the first
 * screen; this line is above it and says the number.
 */
const besides = computed(() => {
  const said: string[] = []
  if (rows.value.withAgent.length) said.push(`${rows.value.withAgent.length} with the agent`)
  if (rows.value.withAuthor.length) {
    said.push(`${rows.value.withAuthor.length} waiting on the author`)
  }
  if (rows.value.ownPrs.length) said.push(`${rows.value.ownPrs.length} of your own`)
  return said.join(' · ')
})

/**
 * What the right-hand cell says about a review a session is working on.
 *
 * **"Session state unknown" was all it could say until DEV-890.** Allele owns whether a
 * session is running and tells Review Desk nothing, so `session_alive` is null on every
 * row this server serves — which was honest and useless. What the record does hold is when
 * anything last wrote to the review, so the row says that instead, and says when nothing
 * has. `activityLabel` holds the order: an answer from the system that owns the fact
 * outranks an inference from the record.
 */
function sessionAction(item: InboxInProgressItem): string {
  return activityLabel(item.session_alive, item.last_activity_at)
}

/**
 * The note beside the group's count: how many posted reviews are drawn elsewhere.
 *
 * **Said, because the count would otherwise be quietly short.** A posted review whose
 * author has answered is under "Needs you" instead, and a reader scanning headings would
 * take this number for the number of reviews out with authors. Empty when none was left
 * out, so the usual case carries no footnote.
 */
/**
 * What a group that held a row back says beside its count.
 *
 * **Every group that subtracts says so.** One row per review means a row drawn in one
 * group is missing from another, and a count that is quietly short is worse than no
 * count. The wording gives no reason, deliberately: the subtraction covers two unlike
 * cases — the developer has something to do, or a session is working on it — and the one
 * reason that fitted both turned out to be true of neither.
 */
function heldBackNote(held: number, what: string): string {
  if (held === 0) return 'nothing for you to do'
  return held === 1
    ? `1 more is ${what} and is already drawn above`
    : `${held} more are ${what} and are already drawn above`
}

const postedNote = computed(() => heldBackNote(rows.value.alsoPosted, 'posted'))

/**
 * The same, for the reviews a session is working on that are drawn under "Needs you".
 *
 * Its count was silently short until this said so, which is the thing the posted group's
 * own note exists to prevent — and the standard this file holds everything else to.
 */
const agentNote = computed(() =>
  heldBackNote(rows.value.alsoWithAgent, 'with the agent'))

/** The first line of a posted row: what was posted, as what, and which round. */
function postedTitle(item: InboxPostedItem): string {
  const round
    = item.rounds_on_pr > 1 ? `Round ${item.round} of ${item.rounds_on_pr} posted` : 'Posted'
  const event = item.posted_as == null ? null : eventLabel(item.posted_as)
  return event == null ? round : `${round} as ${event}`
}

/** The seven layers, as the bar's segments: done, the one running, then the rest. */
function layerSegments(item: InboxInProgressItem): Array<'done' | 'now' | 'todo'> {
  return Array.from({ length: item.layers_total }, (_, at) => {
    const number = at + 1
    if (number < item.layer_number) return 'done'
    if (number === item.layer_number) return 'now'
    return 'todo'
  })
}

/**
 * Whether the inbox has anything in it at all.
 *
 * The waiting groups *and* the two panels. A database with one running session is not an
 * empty desk, even though nothing is waiting on the developer — "nothing is waiting on you"
 * and "there is nothing here yet" are different sentences and a new database deserves the
 * second one.
 *
 * **The posted group counts here, and leaving it out was the reported bug surviving its own
 * fix.** `totals.items` is the five waiting groups and deliberately excludes this one, so a
 * desk whose only review was posted had `items === 0` and no session running — and this
 * function sent it to the fresh-database card, which says "This database holds no reviews"
 * over a record holding exactly the review the developer came to find. The group built to
 * show it is inside the other branch and would never have drawn.
 */
function isEmpty(data: Inbox): boolean {
  return (
    data.totals.items === 0
    && data.in_progress.length === 0
    && data.posted_awaiting_author.length === 0
  )
}

/**
 * What was dealt with without the developer, as one sentence.
 *
 * The boards do not draw this and the API serves it, so it is kept rather than dropped —
 * one quiet line at the foot. Assembled as a string rather than from `<template v-if>`
 * fragments, which the linter puts on their own lines and the renderer then separates with
 * spaces: "Handled without you  this week :" is what that produced.
 */
function handledLine(data: Inbox): string {
  const h = data.handled_without_you
  const when
    = h.window_days === 7 ? 'this week' : `in ${count(h.window_days, 'day')}`
  const said: string[] = []
  if (h.description_problems_caught != null) {
    said.push(
      `${count(h.description_problems_caught, 'description problem')} caught by the CI check`,
    )
  }
  if (h.nits_filed != null && h.nit_tickets != null) {
    said.push(
      `${count(h.nits_filed, 'nit')} filed as ${count(h.nit_tickets, 'ticket')} for an agent`,
    )
  }
  return `Handled without you ${when}: ${
    said.length ? said.join(' · ') : 'nothing has reported in yet'
  }`
}
</script>

<template>
  <div class="queue">
    <DeskState
      :resource="inbox"
      :is-empty="isEmpty"
      what="the queue"
    >
      <template #default="{ data }">
        <div class="top">
          <div class="headline">
            <h1 class="rd-display">
              {{ rows.needsYou.length === 1 ? '1 review needs you' : `${rows.needsYou.length} reviews need you` }}
            </h1>
            <p
              v-if="besides"
              class="rd-meta aside-line"
            >
              {{ besides }}
            </p>
          </div>
          <LinkField
            id="start"
            v-model="link"
            class="start"
            label="Start a review"
            :working="starting"
            working-label="Reading…"
            :refusal="notice && notice.tone !== 'plain' ? notice.text : null"
            :hint="notice && notice.tone === 'plain' ? notice.text : null"
            @submit="start"
          />
        </div>

        <div class="filter">
          <DecisionTrack
            v-model="repo"
            label="Which repository to show"
            :options="repoTabs(data)"
          />
          <span class="rd-meta order">
            Oldest first<template v-if="someTimesMissing">, and the rows with no recorded
              time last</template>
          </span>
        </div>

        <p
          v-if="data.totals.items === 0"
          class="rd-empty"
        >
          Nothing is waiting on you. Anything a session produces turns up here.
        </p>

        <!-- Needs you: the three waiting groups, merged into one row per review. -->
        <DeskSection
          v-if="rows.needsYou.length"
          heading="Needs you"
          :count="rows.needsYou.length"
        >
          <QueueRow
            v-for="row in rows.needsYou"
            :key="row.review_id"
            emphasis
            :pr="row.pr"
            :title="row.title"
            :meta="row.about"
            :to="row.to"
            :action="row.action"
            action-tone="go"
            :age="row.at ? sinceLabel(row.at) : null"
            :age-unknown-because="row.atUnknown"
          >
            <span class="what-title">{{ row.what }}</span>
            <span class="rd-meta what-detail">{{ row.detail }}</span>
          </QueueRow>
        </DeskSection>

        <!-- With the agent: nothing for the developer to do, and which layer is running. -->
        <DeskSection
          v-if="rows.withAgent.length"
          heading="With the agent"
          :count="rows.withAgent.length"
          :note="agentNote"
        >
          <QueueRow
            v-for="item in rows.withAgent"
            :key="item.review_id"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="`· ${item.pull_request.repo}`"
            :to="`/reviews/${item.review_id}/understand`"
            :action="sessionAction(item)"
            :action-tone="item.session_alive === false ? 'warn' : 'quiet'"
            :age="item.in_state_since ? sinceLabel(item.in_state_since) : null"
            age-unknown-because="this review was recorded before Review Desk kept when a step started"
          >
            <SegmentBar
              :segments="layerSegments(item)"
              :label="`Layer ${item.layer_number} of ${item.layers_total}, ${layerLabel(item.layer_name).toLowerCase()}`"
            />
            <span class="rd-meta">
              {{ layerLabel(item.layer_name) }}
              <!--
                How long the review has been in its present state, beside the step it is
                in: "Code review · 20 min". The age cell says when that began; this says
                how long it has lasted, which is the figure Patrick asked for.
              -->
              <template v-if="item.in_state_since">
                · {{ forLabel(item.in_state_since) }}
              </template>
            </span>
          </QueueRow>
        </DeskSection>

        <!--
          Waiting on the author: posted, and the pull request not seen merged or closed.
          **Hidden when empty, like the three groups above**, and that is a change of
          position rather than of rule. It was drawn unconditionally while nothing could
          fill it, because hiding it would have read as "no review is waiting on an
          author" — a claim the API could not make. It can now: an empty
          `posted_awaiting_author` says the record holds no posted review with an open
          pull request, so an empty group is a true sentence and "Waiting on the author ·
          0" on a fresh database is noise.
        -->
        <DeskSection
          v-if="rows.withAuthor.length"
          heading="Waiting on the author"
          :count="rows.withAuthor.length"
          :note="postedNote"
        >
          <QueueRow
            v-for="item in rows.withAuthor"
            :key="item.review_id"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="`· ${item.pull_request.repo}`"
            :to="`/reviews/${item.review_id}/understand`"
            action="Read the review"
            action-tone="quiet"
            :age="item.posted_at ? sinceLabel(item.posted_at) : null"
            age-unknown-because="this review's GitHub id was recorded without a draft, so nothing holds when it was posted"
          >
            <span class="what-title">{{ postedTitle(item) }}</span>
            <!--
              What came back, and where the record has no answer, which absence it is.
              Never a nought over an unread thread: `authorResponseLabel` holds that rule
              and says the measurement's own time beside the figures it has.
            -->
            <span class="rd-meta what-detail">
              {{ authorResponseLabel(item) }} · {{ prStateLabel(item.pr_state) }}
            </span>
          </QueueRow>
        </DeskSection>

        <!-- The developer's own pull requests. GitHub's state, which Review Desk only reads. -->
        <DeskSection
          v-if="rows.ownPrs.length"
          heading="Your pull requests"
          :count="rows.ownPrs.length"
        >
          <QueueRow
            v-for="item in rows.ownPrs"
            :key="`${item.pull_request.repo}-${item.pull_request.pr}`"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="`· ${item.pull_request.repo}`"
            :href="prUrl(item.pull_request.repo, item.pull_request.pr)"
            action="Open on GitHub"
            action-tone="go"
            :age="item.opened_at ? openForLabel(item.opened_at).replace('open ', '') : null"
            age-unknown-because="nothing has told Review Desk when this pull request was opened"
          >
            <span class="own">
              <span class="own-warn">No reviewer requested</span>
              <span class="rd-meta">· {{ descriptionLintLabel(item.description_lint) }}</span>
            </span>
          </QueueRow>
        </DeskSection>

        <!--
          Two facts the boards do not draw, kept rather than dropped: the API serves both,
          and a figure that reaches the screen and is thrown away is a figure nobody can
          act on. One quiet line each, at the foot, out of the groups' way.
        -->
        <div class="foot">
          <p class="rd-meta foot-line">
            {{ handledLine(data) }}
          </p>
          <p
            v-if="data.proposed_rules.length"
            class="rd-meta foot-line"
          >
            {{ count(data.proposed_rules.length, 'rule proposal') }} waiting on you ·
            <NuxtLink to="/rules">{{
              data.proposed_rules.length === 1 ? 'decide on it' : 'decide on them'
            }}</NuxtLink>
          </p>
        </div>
      </template>

      <!--
        A database with nothing in it. It reads as new rather than as broken, and it says
        the one thing that fills it: a session recording a review.
      -->
      <template #empty>
        <DeskCard
          tone="raised"
          class="fresh"
        >
          <h1 class="rd-display">
            Nothing here yet
          </h1>
          <p class="rd-note rd-measure">
            This database holds no reviews. A review turns up here once an agent session has
            recorded one — it writes the brief and the findings through
            <code class="rd-mono">review-desk</code>, and what is waiting on you appears in
            this list.
          </p>
          <p class="rd-note rd-measure">
            To start one yourself, paste a pull request link below. Review Desk reads the
            pull request through your own <code class="rd-mono">gh</code> login, records the
            review at the problem brief, and writes nothing to GitHub.
          </p>
          <LinkField
            id="start-empty"
            v-model="link"
            class="fresh-start"
            label="Start a review"
            :working="starting"
            working-label="Reading…"
            :refusal="notice && notice.tone !== 'plain' ? notice.text : null"
            :hint="notice && notice.tone === 'plain' ? notice.text : null"
            @submit="start"
          />
        </DeskCard>
      </template>
    </DeskState>
  </div>
</template>

<style scoped>
.queue {
  display: flex;
  flex-direction: column;
  gap: 40px;
  line-height: 1.45;
}

.top {
  display: flex;
  flex-wrap: wrap;
  gap: 20px 40px;
  align-items: flex-end;
}

.headline {
  flex: 1 1 360px;
  min-width: 0;
}

.aside-line {
  margin: 8px 0 0;
}

.start {
  flex: 1 1 360px;
  min-width: 0;
}

.filter {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  align-items: center;
  font-size: 14px;
}

.order {
  margin-left: auto;
}

.what-title {
  display: block;
  flex: 1 1 100%;
  font-weight: 600;
}

.what-detail {
  display: block;
  flex: 1 1 100%;
}

.own {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 6px;
  font-size: 13px;
}

.own-warn {
  color: var(--rd-warn-ink);
  font-weight: 600;
}

.foot {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.foot-line {
  margin: 0;
}

.fresh {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 32px;
}

.fresh-start {
  max-width: 28rem;
  padding-top: 8px;
}

@media (max-width: 560px) {
  .order {
    margin-left: 0;
  }
}
</style>
