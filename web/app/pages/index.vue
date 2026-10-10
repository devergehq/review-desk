<script setup lang="ts">
import type {
  Inbox,
  InboxBriefItem,
  InboxDraftItem,
  InboxInProgressItem,
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
 * | Waiting on the author | **None.** No `/inbox` group lists a posted review awaiting a reply |
 * | The age of a draft, a reply or a running session | **None.** Only a brief and a pull request carry a time |
 * | "Session not seen for 20 min" | `session_alive` is a tri-state boolean; there is no last-seen time |
 *
 * Each is drawn in the state it is in rather than filled with a number: the group is
 * present and says it is not measured, and a row with no time says so where the time goes.
 * Inventing either would be the one thing the brief forbids — "a figure with no data source
 * is shown as unknown, never as a number".
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
}

/** Every review that is waiting on the developer, merged into one row each. */
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

  return [...byReview.values()]
}

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
      at: null,
      atUnknown: 'a draft waiting to be signed off carries no time in the inbox',
    }
  }
  const first = row.reply!.events[0]!
  return {
    what: `The author replied to ${first.finding_key}`,
    detail: first.detail ?? replyLabel(first),
    action: 'Read the reply',
    to: stepFor(row),
    at: null,
    atUnknown: 'an author reply carries no time in the inbox',
  }
}

/** What else is waiting behind the next action, so one row can say it needs two things. */
function alsoWaiting(row: Waiting): string[] {
  const also: string[] = []
  if (row.brief && row.draft) also.push('a draft is ready behind it')
  if (row.brief && row.reply) also.push('the author has replied too')
  else if (row.draft && row.reply) also.push('the author has replied too')
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
  for (const row of needingYou(data)) bump(row.repo)
  for (const item of data.in_progress) bump(item.pull_request.repo)
  for (const item of data.own_prs_without_reviewer) bump(item.pull_request.repo)

  return [
    { value: ALL_REPOSITORIES, label: 'All repositories' },
    ...[...tally.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, rows]) => ({ value: name, label: name, count: rows })),
  ]
}

const showing = (name: string) => repo.value === ALL_REPOSITORIES || repo.value === name

/* ------------------------------ The groups ------------------------------ */

/**
 * Oldest first, with the rows whose time has no source after the ones that have it.
 *
 * The board says "Oldest first" and three of the four groups carry no time at all, so the
 * caption says what this really did rather than implying a sort the data cannot support.
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
  if (!data) return { needsYou: [], withAgent: [], ownPrs: [] }

  const needsYou = oldestFirst(
    needingYou(data).filter(row => showing(row.repo)).map(queueRow),
    row => row.at,
  )
  const withAgent: InboxInProgressItem[] = data.in_progress.filter(item =>
    showing(item.pull_request.repo),
  )
  const ownPrs = oldestFirst(
    data.own_prs_without_reviewer.filter(item => showing(item.pull_request.repo)),
    item => item.opened_at ?? null,
  )
  return { needsYou, withAgent, ownPrs }
})

/** Whether any row on the page has no recorded time, which the caption then says. */
const someTimesMissing = computed(
  () =>
    rows.value.needsYou.some(row => row.at == null)
    || rows.value.withAgent.length > 0
    || rows.value.ownPrs.some(item => item.opened_at == null),
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
 * database with one review — and the author group is left out because nothing can count it.
 */
const besides = computed(() => {
  const said: string[] = []
  if (rows.value.withAgent.length) said.push(`${rows.value.withAgent.length} with the agent`)
  if (rows.value.ownPrs.length) said.push(`${rows.value.ownPrs.length} of your own`)
  return said.join(' · ')
})

/** What the right-hand cell says about a session Allele owns and may not have reported. */
function sessionAction(alive: boolean | null | undefined): string {
  if (alive === true) return 'Session running'
  if (alive === false) return 'Session ended'
  return 'Session state unknown'
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
 */
function isEmpty(data: Inbox): boolean {
  return data.totals.items === 0 && data.in_progress.length === 0
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
          note="nothing for you to do"
        >
          <QueueRow
            v-for="item in rows.withAgent"
            :key="item.review_id"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="`· ${item.pull_request.repo}`"
            :to="`/reviews/${item.review_id}/understand`"
            :action="sessionAction(item.session_alive)"
            :action-tone="item.session_alive === false ? 'warn' : 'quiet'"
            age-unknown-because="the inbox carries no time for a running session"
          >
            <SegmentBar
              :segments="layerSegments(item)"
              :label="`Layer ${item.layer_number} of ${item.layers_total}, ${layerLabel(item.layer_name).toLowerCase()}`"
            />
            <span class="rd-meta">{{ layerLabel(item.layer_name) }}</span>
          </QueueRow>
        </DeskSection>

        <!--
          Waiting on the author. **The board draws this group and the API has no source for
          it**: no `/inbox` group lists a review that was posted and is waiting for a reply.
          Drawn rather than hidden, saying what is missing — hiding it would read as "no
          review is waiting on an author", which is a claim nothing here can make.
        -->
        <DeskSection
          heading="Waiting on the author"
          note="not measured yet"
        >
          <p class="rd-empty">
            Reviews that have been posted and are waiting for the author are
            <NotMeasured because="GET /inbox serves no group for a posted review awaiting a reply, and DEV-876 adds no API field" />.
            Each one is still on its own review page.
          </p>
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
