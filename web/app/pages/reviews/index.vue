<script setup lang="ts">
import type { ReviewListItem, ReviewsShown } from '~/types/desk'

/**
 * The Reviews list: **every review the record holds, whatever became of it.**
 *
 * ## Why a list at all
 *
 * Every other read is about a review that needs something. `GET /inbox` is what is waiting
 * and `GET /timings` is what has finished, so a review that was posted and has not been
 * replied to was in neither — and the navigation's "Reviews" pointed at whichever review
 * happened to be waiting, which is not a place a particular review can be found. Patrick,
 * on a review, after posting review 2: "the actual review itself has disappeared… I
 * can't even get to the review page if I wanted to go back." This is the page that answers
 * that, and the queue's new "Waiting on the author" group is the shortcut to it.
 *
 * ## One row per pull request, with its earlier rounds folded into it
 *
 * The design pass of 15 October 2026, over the live desk:
 *
 * > **One pull request, many rows.** #9541 appears five times on Reviews, once per round,
 * > with near-identical titles and status. The Queue already folds rounds together;
 * > Reviews doesn't.
 *
 * It does now, and the rule is the queue's: **one pull request is one thing to deal with.**
 * The row is the newest round in the answer; every earlier round the answer holds opens
 * from a fold on it, with its own step, its own sentence about how it left the desk, and
 * its own link. Nothing is dropped — this page's promise is "every review the record holds,
 * whatever became of it", and a round behind a fold is still on the page.
 *
 * **The title is the newest round that records one**, which is a different rule from the
 * queue's and deliberately so. A queue row is a *round*, so a round with no title says it
 * has none; this row is a *pull request*, and #9541's round 5 records no title while its
 * rounds 1 to 4 all record "DUC-1998: team hierarchy schema, closure engine and self-row
 * backfill". Folding on the newest round alone would have put four real titles behind the
 * words "no title recorded".
 *
 * **The fold is a sibling of the row's link, not a cell in it.** `QueueRow` argues that at
 * length about anchors; a `<summary>` inside an `<a>` is the same mistake, and pressing it
 * would open the review instead of opening the fold.
 *
 * **And the summary says what is inside it.** A dismissal's reason and a supersession's
 * explanation are the two sentences this page exists to carry, and either can be on a
 * round that is not the newest — so the fold's own line counts the rounds and names the
 * states behind it. A reader never has to open a fold to find out whether it matters.
 *
 * ## It describes; it does not grade
 *
 * There is no score here, no ranking, and no figure about how well a review went. That
 * rules out four things the page would otherwise have grown, each for a stated reason:
 *
 * | Not drawn | Why |
 * |---|---|
 * | the seven-step bar the queue uses | `SegmentBar` paints every step below the current one as done, so a `skipped` step 1 and a `failed` step 3 both read as having run — a false claim, not merely a graded one. And a column of bars re-creates for the eye the percentage its `role="img"` exists to keep out of a screen reader. |
 * | a findings count, or a severity tally | on a list of every review, findings-per-review is a league table of the authors named two columns to its left |
 * | a bare round number | the same: beside the repository and the pull request it reads as a count of rework. It is "round 1 of 3", a position in a sequence. |
 * | how long a review took | `GET /timings` answers that, deliberately separately |
 *
 * **And the step is never drawn as a name alone.** `stepLabel` holds that rule: a bare
 * step name is the step a review is waiting at when the state is `waiting_on_you` and the
 * step it has *left* when the state is `done`, so a posted review would read "Draft" and
 * tell a reader it still needs signing off — while the queue says it is with the author.
 *
 * ## The reviews that have left the desk are here, and not in the default view
 *
 * **Two ways of leaving, and they are told apart on every row.** A review the developer
 * dismissed is out of the queue and out of this page's default view — DEV-945 — and a
 * review whose pull request merged or closed is **finished**, which is DEV-957 and is not
 * a dismissal: nobody decided it, it happened, and no round of that pull request can be
 * posted afterwards. This is the one page that can still find either.
 *
 * The second picker is what asks for them, and it has four positions so that the page's
 * own promise still holds: "every review the record holds, whatever became of it" is
 * **Everything**, and the default is the narrower claim it makes in the line under the
 * heading.
 *
 * Each row that has left says so under the title, because a reader of either view is
 * asking exactly that. A dismissal says when and why; the reason is the developer's own
 * words, so it is drawn as text — not through `TickedText`, which is for prose a session
 * wrote. A finish says how the pull request ended and, when the record holds a date, when:
 * the two sentences are different because the facts are, and a row that said "dismissed"
 * over a merge would be putting words in the developer's mouth.
 *
 * ## And a third row that is nobody's turn, which stays in play
 *
 * A **superseded** round — an unposted round the author has pushed past — is not off the
 * desk in the filter's sense: its pull request is in play and so is its own record, so it is
 * under **In play** and there is no position for it. What it is not is anybody's turn, and
 * the step column would otherwise read "Draft · waiting on you" over a round the store
 * refuses every act on. So it reads in the past tense like the other two and says why.
 *
 * ## Where the record has no answer
 *
 * A review nothing has reported on is "Not started", in words: the record holds seven
 * pending steps, which is something it knows rather than something nobody measured, so
 * `NotMeasured` would be the wrong component and a step number of 1 would be a claim about
 * the lint. A review nothing has written to since the record began keeping that says so
 * where its time goes.
 */
const desk = useReviewDesk()

useHead({ title: 'Reviews · Review Desk' })

/**
 * What the last screen left to say, which is only ever a deleted review's receipt.
 *
 * Read once, on mount: the review's own page cannot say what became of it, because by
 * then it is not there to draw. A reload says nothing, which is right — this is an
 * acknowledgement, not a state the page holds.
 */
const handedOver = ref(takeHandOver())

/** Which repository to show. `ALL_REPOSITORIES` for every one, as the queue words it. */
const repo = ref(ALL_REPOSITORIES)

/**
 * Which reviews to show. The contract's four positions, as they are sent: the page holds
 * the API's own word for the filter rather than a word of its own, so there is nothing to
 * translate and nothing to disagree about.
 */
const show = ref<ReviewsShown>('in_play')

const showTabs: Array<{ value: ReviewsShown, label: string }> = [
  { value: 'in_play', label: 'In play' },
  { value: 'dismissed', label: 'Dismissed' },
  { value: 'finished', label: 'Finished' },
  { value: 'everything', label: 'Everything' },
]

/**
 * The read follows both pickers, server-side, as the repository picker already did: a
 * filtered view computed in the page would be the page deciding what a dismissal or a
 * finish is, and the record is where that is decided. **Finished especially**: it is
 * derived from whichever round of the pull request a harvest last reached, which a page
 * holding one row has not got.
 */
const reviews = desk.reviews(repo, show)

/**
 * The one-off tidy's proposal, and whether it is open.
 *
 * **Read whether or not it is open**, which is the composable's own shape — a resource
 * follows the repository picker and refreshes with every write, and gating the *read* on a
 * button would mean the first open shows a spinner over a question the page could already
 * have answered. What the button gates is the drawing.
 */
const duplicates = desk.duplicateRounds(repo)
const tidyOpen = ref(false)

/**
 * The repositories to pick from, read the way the Rules screen reads them.
 *
 * **A second read, and the same exception that screen already takes.** The contract's rule
 * is one GET per screen, and a picker that filters *server-side* cannot be built from the
 * filtered answer: once a repository is chosen the response holds only that one, and tabs
 * built from it would leave the reader no way back. `GET /repos` is the unfiltered list,
 * and Rules has read it beside `GET /rules?repo=` since that screen was built.
 *
 * **And no count beside each name.** Rules puts `active_rules` there because that is what
 * its page filters to. The nearest thing here is `open_reviews`, which counts reviews with
 * no posted GitHub review — a smaller number than this page shows, and the queue's own
 * comment says it: a filter whose count does not match what it filters to is worse than no
 * count.
 */
const repos = desk.repos()

const repoTabs = computed(() => [
  { value: ALL_REPOSITORIES, label: 'All repositories' },
  ...(repos.data.value ?? []).map(item => ({ value: item.repo, label: item.repo })),
])

/**
 * The second line under a review's title: where it is, which round of how many, and
 * whether it was posted.
 *
 * **Posted is said here because the step column cannot say it.** The seven steps are the
 * review process, and a review whose GitHub id was recorded by `review link` had that
 * process run outside Review Desk — so no step was ever reported and the step column
 * reads "Not started", truthfully, beside a review the queue is calling posted. Said on
 * this line the two stop contradicting each other, and it costs no new field: a review
 * has carried `github_review_id` since the first slice.
 *
 * It is a state, not a mark: this page says where a review is and never how it went.
 *
 * **The ticket is not here, and the reasoning it used to carry has been answered.** This
 * said the id twice for a review with an address — once here and once as the row's link —
 * on the ground that a review with an id and *no* address would otherwise have no ticket on
 * its row at all. DEV-957 put the link inline in this very line, which made the repetition
 * read as a mistake, and `QueueRow` now draws the id either way: a link when the record
 * holds an address, plain text when it does not. The line is still what the page is read
 * by; the ticket on it is written by the component rather than by this.
 */
function about(row: ReviewListItem): string {
  const parts = [row.review.repo]
  parts.push(
    row.rounds_on_pr > 1
      ? `round ${row.review.round} of ${row.rounds_on_pr}`
      : `round ${row.review.round}`,
  )
  if (row.review.github_review_id) parts.push('posted')
  return parts.join(' · ')
}

/** What this view holds, said in the line under the heading. */
const VIEW_LINES: Record<ReviewsShown, string> = {
  in_play: 'Every review in play. The ones that have left the desk are under Dismissed '
    + 'and Finished.',
  dismissed: 'The reviews you have dismissed, each with when and why.',
  finished: 'The reviews whose pull request merged or closed. Nobody dismissed these — '
    + 'no round of them can be posted now.',
  everything: 'Every review the record holds, whatever became of it — the dismissed and '
    + 'the finished ones among them.',
}

/**
 * What this view holds, read off **the answer** rather than off the picker.
 *
 * `data.dismissed` is in the response for this reason — the contract's own words are that
 * the page's filter reads what it was given rather than trusting its own state. Read off
 * the ref, the line changes the instant the picker moves while the rows underneath are
 * still the previous view's for as long as the read is in flight, which is one sentence
 * describing another view's rows.
 */
function viewLine(answer: { show: ReviewsShown }): string {
  return VIEW_LINES[answer.show]
}

/* --------------------- One row per pull request --------------------- */

/** One pull request on the list: the round it is drawn as, and the rounds behind it. */
interface PullRequestRow {
  /** `repo#pr`, which is the key this page is grouped on and the row's own `:key`. */
  key: string
  /** The newest round of this pull request **in the answer**: the row's step, age and link. */
  face: ReviewListItem
  /**
   * The pull request's title: the newest round that records one.
   *
   * **A fact about the pull request rather than about a round**, which is what makes this
   * rule different from the queue's. A queue row is one round and says so when that round
   * records no title; this row is the pull request, and `trilogy-care/tc-portal#9541` holds
   * five rounds of which only the newest records none. Reading the face round alone drew
   * "no title recorded" over four rounds that all carry the same real title.
   */
  title: string
  /** Every other round the answer holds, newest first. Empty for most pull requests. */
  earlier: ReviewListItem[]
}

/**
 * The answer's reviews, grouped into one row per pull request.
 *
 * **Insertion order is kept**, so the page's stated sort — newest first, by when each was
 * opened — still decides where a pull request sits: it goes where its first-mentioned round
 * was. The server serves newest first, so that is its newest round.
 *
 * **Grouped in the page rather than by the server, deliberately.** `GET /reviews` answers
 * about reviews because a review is what the record holds and what the four-position filter
 * selects; which of them a reader wants drawn as one line is a question about this page.
 * The queue groups its own five arrays for the same reason and in the same shape.
 */
function byPullRequest(rows: ReviewListItem[]): PullRequestRow[] {
  const held = new Map<string, ReviewListItem[]>()
  for (const row of rows) {
    const key = `${row.review.repo}#${row.review.pr}`
    const list = held.get(key)
    if (list) list.push(row)
    else held.set(key, [row])
  }

  return [...held.entries()].map(([key, list]) => {
    /*
     * **Sorted on the round, not taken as served.** The answer's order is by when a review
     * was opened, which is the newest round for every pull request whose rounds were opened
     * in order and is not a promise the contract makes. The round number is the fact.
     */
    const byRound = [...list].sort((left, right) => right.review.round - left.review.round)
    const face = byRound[0]!
    return {
      key,
      face,
      title: byRound.find(row => row.review.title)?.review.title ?? 'no title recorded',
      earlier: byRound.slice(1),
    }
  })
}

/**
 * What the fold's own line says: how many rounds are behind it, which, and what they are.
 *
 * **The states are named on the summary because two of them are the reason this page
 * exists.** A dismissal carries the developer's own words and a supersession explains where
 * a round went; both can be on a round that is not the newest, and a fold that said only
 * "4 earlier rounds" would make a reader open every one to find out whether any of them
 * mattered. Counting them on the line answers that without opening anything.
 *
 * A round still in play contributes no word, which is the ordinary case: "4 earlier rounds ·
 * rounds 1 to 4" is a complete sentence about four rounds that simply came before.
 */
const LEFT_WORDS: Record<string, string> = {
  dismissed: 'dismissed',
  finished: 'finished',
  out_of_play: 'out of play',
  superseded: 'superseded',
}

function earlierSummary(row: PullRequestRow): string {
  const rounds = row.earlier.map(item => item.review.round)
  const low = Math.min(...rounds)
  const high = Math.max(...rounds)
  const said = [
    count(row.earlier.length, 'earlier round'),
    low === high ? `round ${low}` : `rounds ${low} to ${high}`,
  ]

  const tally = new Map<string, number>()
  for (const item of row.earlier) {
    const word = item.left_the_desk == null ? null : LEFT_WORDS[item.left_the_desk]
    if (word) tally.set(word, (tally.get(word) ?? 0) + 1)
  }
  for (const [word, many] of tally) said.push(`${many} ${word}`)

  return said.join(' · ')
}

/**
 * What a row that has left the desk says under its step — **one function, two places.**
 *
 * It was four `v-else-if` branches in the markup, which was fine while a row was a review.
 * A row is a pull request now and the same sentence has to be written for the round on its
 * face and for every round in its fold, and a condition chain written twice is two chains
 * that drift.
 *
 * `null` for a round that has not left, which is most of them.
 */
function leftLine(row: ReviewListItem): string | null {
  if (row.left_the_desk === 'dismissed' && row.review.dismissed_at) {
    const why = row.review.dismissed_reason ? ` — ${row.review.dismissed_reason}` : ''
    return `Dismissed ${sinceLabel(row.review.dismissed_at)}${why}`
  }
  if (row.left_the_desk === 'finished') return finishedLine(row)
  if (row.left_the_desk === 'out_of_play') return outOfPlayLine(row)
  if (row.left_the_desk === 'superseded') {
    return `Superseded by round ${row.rounds_on_pr}. The author pushed past the commit this `
      + 'round was written against, so it can never be the review that posts.'
  }
  return null
}

/**
 * What a finished row says under its title.
 *
 * **The pull request's own word, and the day only when the record holds one.**
 * `finished_how` is `merged` or `closed` on a finished row; `finished_at` is null when
 * GitHub gave no time, and the sentence then stops after the word rather than inventing a
 * date. `row.review.pr_state` is deliberately **not** read here — it is the column of this
 * one round, null for three of the four rounds of a merged pull request, and the server
 * serves `finished_how` precisely so a row does not have to know that.
 */
function finishedLine(row: ReviewListItem): string {
  const day = dayLabel(row.finished_at)
  const when = day ? ` on ${day}` : ''
  /*
   * **No default word.** `?? 'merged'` was here, and on the one page whose whole point is
   * telling the two ways off the desk apart it would have stated a fact the record did not:
   * a finished row served without `finished_how` would read "merged" whether its pull
   * request merged or closed. `finished_at` already handles its own absence by leaving the
   * day out; this leaves the word out the same way. Found by a review of this branch.
   */
  const how = row.finished_how
  if (how == null) {
    return `Finished${when}. No round of this pull request can be posted.`
  }
  return `Finished — the pull request ${how}${when}. No round of it can be posted.`
}

/**
 * What a row whose pull request is out of play says under its title.
 *
 * **The reason, and that it is coming back.** Out of play is the one of the four ways off
 * the queue that undoes itself — the author converted the pull request to a draft, or the
 * review request was withdrawn — so the sentence has to say *paused* rather than *over*, or
 * a reader will read it as a finish and stop expecting the review.
 */
function outOfPlayLine(row: ReviewListItem): string {
  const why = row.review.out_of_play
  const since = row.review.out_of_play_at ? ` ${sinceLabel(row.review.out_of_play_at)}` : ''
  if (why === 'withdrawn') {
    return `Out of play${since} — the review request was withdrawn. It comes back when `
      + 'somebody asks for a review again.'
  }
  if (why === 'draft') {
    return `Out of play${since} — the pull request is a draft. It comes back when the `
      + 'author marks it ready.'
  }
  /*
   * **No default reason.** A row served as out of play without one would otherwise read
   * "the pull request is a draft" about a withdrawn request — the mistake `finishedLine`
   * above avoids by leaving the word out rather than guessing it.
   */
  return `Out of play${since}. It comes back when the pull request does.`
}

/**
 * The rows the page draws: one per pull request, in the answer's own order.
 *
 * A computed over the resource rather than a call in the markup, which is the shape the
 * queue uses and for its reason: four bindings per row need the answer, and a function
 * called from the template is re-evaluated on every render — on every press of either
 * picker.
 */
const pullRequests = computed(() => byPullRequest(reviews.data.value?.reviews ?? []))

/** Whether any row has no activity time, which the caption then says. */
const someTimesMissing = computed(
  () => (reviews.data.value?.reviews ?? []).some(row => row.last_activity_at == null),
)

/**
 * Nothing at all, which is a new database rather than a broken one.
 *
 * **Only when no repository is being asked about.** The read follows the picker, so a
 * repository with no reviews answers with an empty list too — and `GET /repos` lists any
 * repository holding a review *or* a repository-scoped rule, so one with only a rule is
 * reachable from the picker. Treating that as empty said "This database holds no reviews"
 * about a database holding several, and took the picker away with it: the control lives in
 * the default slot, so the only way back to "All repositories" was to reload the page. The
 * filter is component state and never reaches the URL, so there was not even an address to
 * edit.
 */
function isEmpty(answer: {
  repo: string | null
  show: ReviewsShown
  reviews: ReviewListItem[]
}): boolean {
  return answer.repo == null
    && answer.show === 'in_play'
    && answer.reviews.length === 0
}

/**
 * What an empty answer means, when it is not an empty database.
 *
 * **Three ways to ask for nothing, and three different things to say.** A repository with
 * no reviews is not a database with none — that is the fault the comment above records —
 * and "no review has been dismissed" is not either of them. One sentence for each, said
 * inside the default slot so the pickers stay on the page to get back out with.
 */
function nothingHere(answer: { repo: string | null, show: ReviewsShown }): string {
  const where = answer.repo == null ? '' : ` on ${answer.repo}`
  const lines: Record<ReviewsShown, string> = {
    dismissed: `No review${where} has been dismissed. A review you dismiss leaves the `
      + 'queue and turns up here.',
    finished: `No review${where} has finished. A review whose pull request merges or `
      + 'closes leaves the queue and turns up here.',
    everything: `No review${where} yet.`,
    in_play: `No review${where} is in play. Any that have left the desk are under `
      + 'Dismissed and Finished.',
  }
  return lines[answer.show]
}
</script>

<template>
  <div class="reviews">
    <!--
      **Above the state, not inside it.** The sentence a delete hands over was drawn in
      the default slot, and `DeskState` draws the *empty* slot instead of that one — so
      deleting the last review in play, which is precisely the database DEV-945 was
      raised from, dropped the only acknowledgement the one irreversible act gets. Here
      it is on the page whichever slot is drawn.
    -->
    <DeskNotice
      v-if="handedOver"
      tone="plain"
      class="handed-over"
    >
      {{ handedOver }}
    </DeskNotice>

    <DeskState
      :resource="reviews"
      :is-empty="isEmpty"
      what="the reviews"
    >
      <template #default="{ data }">
        <div class="top">
          <h1 class="rd-display">
            {{ count(data.reviews.length, 'review') }}
          </h1>

          <!--
            **The line says what this view is, not what the page is.** "Every review the
            record holds" was true of a page with no filter and is the claim
            **Everything** makes; the default view leaves the dismissed ones out, and a
            heading that went on claiming all of them would be the page contradicting its
            own picker.
          -->
          <p class="rd-meta aside-line">
            {{ viewLine(data) }}
            Rounds of one pull request are folded into its row.
            How long one took is on its own page.
          </p>
        </div>

        <div class="filter">
          <DecisionTrack
            v-model="repo"
            label="Which repository to show"
            :options="repoTabs"
            :disabled="repos.pending.value && !repos.loaded.value"
          />
          <DecisionTrack
            v-model="show"
            label="Which reviews to show"
            :options="showTabs"
          />
          <!--
            **The sort, said out loud**, the way the queue says "Oldest first". Newest
            first here means the order the reviews were opened in — deliberately not when
            they last changed, which is null for every review from before Review Desk kept
            it and would put those in no particular place.
          -->
          <span class="rd-meta order">
            Newest first<template v-if="someTimesMissing">, by when each was opened — some
              have no record of when they last changed</template>
          </span>
        </div>

        <!--
          A repository with nothing in it. Said here rather than through the empty card,
          which claims something about the whole database — and said inside the default
          slot so the picker stays on the page to get back out with.
        -->
        <p
          v-if="data.reviews.length === 0"
          class="rd-empty"
        >
          {{ nothingHere(data) }}
          <template v-if="data.repo != null">
            Every other repository's are under <strong>All repositories</strong>.
          </template>
        </p>

        <!--
          **"Every pull request", and the count counts the rows it draws.** It read "Every
          review" over one row per review; a row is a pull request now, so the heading is.
          The headline above still counts reviews, which is the record's own number and
          this page's promise — the line between the two says rounds are folded, so the
          13 and the 8 cannot read as a contradiction.
        -->
        <DeskSection
          v-else
          heading="Every pull request"
          :count="pullRequests.length"
        >
          <QueueRow
            v-for="row in pullRequests"
            :key="row.key"
            :pr="row.face.review.pr"
            :title="row.title"
            :meta="about(row.face)"
            :to="`/reviews/${row.face.review.id}/understand`"
            action="Read the review"
            action-tone="quiet"
            :age="row.face.last_activity_at
              ? `last written to ${sinceLabel(row.face.last_activity_at)}` : null"
            age-unknown-because="nothing has written to this review since Review Desk started keeping when one last changed"
            :pr-href="prUrl(row.face.review.repo, row.face.review.pr)"
            :ticket-href="ticketUrl(row.face.review.ticket_url)"
            :ticket-id="row.face.review.ticket_id"
          >
            <!--
              **The step and its number on one line, what it says on the next.** Three
              lines before: the step and its outcome wrapped, and "step 6 of 7" took a
              third. `stepCell` puts the number where it belongs — beside the step it is a
              number of — and the outcome drops to the row's second line, which is the
              shape the queue's rows already have.

              The contract requires a screen to draw `step_state` or `step_detail` and
              never the name alone. Both lines are in this one cell, which is what that
              rule is about; `stepCell`'s own note carries the argument.
            -->
            <span class="step">{{ stepCell(row.face).said }}<span
              class="rd-visually-hidden"
            >, {{ stepCell(row.face).of }}</span></span>
            <span
              v-if="stepCell(row.face).detail"
              class="rd-meta step-of"
            >{{ stepCell(row.face).detail }}</span>
            <!--
              **When and why, on the row**, which is what DEV-945 asks the dismissed view
              for, and the same line for a finish, a pull request out of play and a
              superseded round. `leftLine` holds all four so that the fold below can write
              the same sentence about a round that is not on the face.

              Drawn on every view rather than only on the dismissed one: under
              **Everything** a dismissed review sits beside reviews in play, and a row
              that did not say so would be the one row on the page that reads as something
              it is not.
            -->
            <span
              v-if="leftLine(row.face)"
              class="rd-meta left"
            >{{ leftLine(row.face) }}</span>

            <!--
              **The rounds behind this one.** Outside the row's own link — `QueueRow` draws
              this slot as a sibling of the anchor — because a `<summary>` inside a link is
              a control inside a control.
            -->
            <template
              v-if="row.earlier.length"
              #rounds
            >
              <details class="rd-fold earlier">
                <summary>{{ earlierSummary(row) }}</summary>
                <ol class="earlier-list">
                  <li
                    v-for="item in row.earlier"
                    :key="item.review.id"
                    class="earlier-round"
                  >
                    <NuxtLink
                      class="earlier-link"
                      :to="`/reviews/${item.review.id}/understand`"
                    >Round {{ item.review.round }} of {{ item.rounds_on_pr }}</NuxtLink>
                    <span class="earlier-step">{{ stepCell(item).said }}<span
                      class="rd-visually-hidden"
                    >, {{ stepCell(item).of }}</span></span>
                    <span
                      v-if="stepCell(item).detail"
                      class="rd-meta earlier-detail"
                    >{{ stepCell(item).detail }}</span>
                    <span
                      v-if="leftLine(item)"
                      class="rd-meta left earlier-left"
                    >{{ leftLine(item) }}</span>
                  </li>
                </ol>
              </details>
            </template>
          </QueueRow>
        </DeskSection>

        <!--
          **The one-off tidy, opened rather than drawn.** It answers about the record as the
          rule before DEV-962 left it, and on a desk with no duplicates there is nothing to
          say — so the page asks for the proposal when the developer opens it, and says so
          either way. Here because the Reviews list is the one place every round is visible.
        -->
        <div class="tidy-opener">
          <DeskButton
            v-if="!tidyOpen"
            kind="quiet"
            @click="tidyOpen = true"
          >
            Tidy the duplicate rounds
          </DeskButton>
          <template v-else>
            <DeskState
              :resource="duplicates"
              what="the duplicate rounds"
            >
              <template #default="{ data: tidy }">
                <TidyRounds
                  v-if="tidy.pull_requests.length > 0"
                  :page="tidy"
                />
                <DeskNotice
                  v-else
                  tone="plain"
                >
                  No pull request has more than one unposted round. A new commit on an
                  unposted round moves that round rather than opening another, so none can
                  turn up from here on.
                </DeskNotice>
              </template>
            </DeskState>
            <DeskButton
              kind="quiet"
              @click="tidyOpen = false"
            >
              Close the tidy
            </DeskButton>
          </template>
        </div>
      </template>

      <!--
        Nothing in the default view. It reads as new rather than broken, and it points at
        the queue, which is where a review is started.

        **And it carries the one control that is otherwise not on the page.** `DeskState`
        draws this slot *instead of* the default one, pickers and all, so a database whose
        only review is a dismissed one said "no reviews" with no way to reach the view
        that holds it — on the one page that is supposed to be able to find a review
        whatever became of it. The card cannot say for certain which of the two states it
        is in, because the answer it is drawn from is the one that leaves dismissed
        reviews out; what it can do is offer the look, which is what this button is.
      -->
      <template #empty>
        <DeskCard
          tone="raised"
          class="fresh"
        >
          <h1 class="rd-display">
            No reviews in play
          </h1>
          <p class="rd-note rd-measure">
            No review is waiting, running or finished here. One turns up as soon as an
            agent session records it, and stays afterwards — including once it has been
            posted, which is what this page is for.
          </p>
          <p class="rd-note rd-measure">
            To start one, paste a pull request link on the
            <NuxtLink to="/">queue</NuxtLink>.
          </p>
          <DeskButton
            kind="secondary"
            @click="show = 'everything'"
          >
            Look at the reviews that have left the desk
          </DeskButton>
        </DeskCard>
      </template>
    </DeskState>
  </div>
</template>

<style scoped>
.reviews {
  display: flex;
  flex-direction: column;
  gap: 40px;
  line-height: 1.45;
}

.top {
  max-width: 44rem;
}

.handed-over {
  max-width: 44rem;
}

.aside-line {
  margin: 8px 0 0;
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

.step {
  display: block;
  flex: 1 1 100%;
  font-weight: 600;
}

/*
 * What the step says, under the step. `text-wrap: pretty` for the queue's reason: this is a
 * line of clauses, and a greedy break leaves the last word of one alone on a line.
 */
.step-of {
  display: block;
  flex: 1 1 100%;
  text-wrap: pretty;
}

/*
 * **The line a row that has left the desk carries**, either way. It takes the row's whole
 * width and wraps, because a dismissal's reason is a sentence somebody typed rather than a
 * label. Its own colour, so the row reads as a review that has left the desk without the
 * words having to say it twice.
 *
 * **`left`, and it was `dismissed`.** One class for both lines rather than two identical
 * ones: what it styles is "this row has left the desk", and the sentence says which way.
 */
.left {
  display: block;
  flex: 1 1 100%;
  overflow-wrap: break-word;
  color: var(--rd-warn-ink);
}

/*
 * **The fold of earlier rounds.** `.rd-fold` is the app's fold control — its summary, its
 * 44-pixel target and its drawn triangle are all there — and this adds only what is about
 * a list of rounds: the type size of a line that is not the row's own, and the rows inside.
 */
.earlier {
  font-size: 13px;
}

/*
 * **The summary is indented to the title column, and the rounds under it are not.** It is a
 * line *about* the row — "4 earlier rounds · rounds 1 to 4 · 1 superseded" — so it reads
 * where the row's other such lines read, under the title; the rounds themselves are rows and
 * sit on the row's own columns. 76 pixels is the number cell's width plus the row's column
 * gap, which is what `QueueRow`'s repository line uses.
 */
.earlier > summary {
  padding-left: 76px;
}

@media (max-width: 560px) {
  .earlier > summary {
    padding-left: 0;
  }
}

.earlier-list {
  display: grid;
  gap: 10px;
  margin: 8px 0 4px;
  padding: 0;
  list-style: none;
}

/*
 * **One round, on the row above's own columns.** The tracks are `QueueRow`'s — the number's
 * width, the two text columns, the age, the action — because a round in this fold *is* a row
 * of this list, and a reader scanning the status column down the page should not find the
 * fold's steps sitting in the middle of the title column.
 *
 * The round's number goes where a title goes and its step where a status goes, which leaves
 * the first, fourth and fifth tracks empty: the pull request's number is on the row above
 * and saying it four more times is the repetition this change is removing, and a round's own
 * age and action are what pressing it is for.
 */
.earlier-round {
  display: grid;
  grid-template-columns:
    56px minmax(0, 1fr) minmax(0, 1fr) 90px 150px;
  gap: 2px 20px;
  align-items: baseline;
}

.earlier-link {
  grid-column: 2;
}

.earlier-step {
  grid-column: 3;
}

.earlier-link {
  font-weight: 600;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.earlier-step {
  font-weight: 600;
}

/*
 * The two sentences under a round sit under its step rather than under its number — the
 * number is the thing being pressed, and a sentence beginning in its column would read as
 * part of the link's own label.
 *
 * **In the status column and not spanning past it**, although a finish's sentence would read
 * more easily on a wider line. The face row's own sentence is inside `QueueRow`'s status
 * cell and cannot span; a fold row that did would put the same sentence at two widths on one
 * row, three lines apart, which is the kind of difference a reader looks for a meaning in.
 */
.earlier-detail,
.earlier-left {
  grid-column: 3;
}

@media (max-width: 560px) {
  /*
   * **One column at a phone's width**, which is what the row above does too: 390 pixels does
   * not hold five tracks, and a round number in a 56-pixel column beside a step cell of
   * about 90 would put "What happened 7/7" on three lines.
   */
  .earlier-round {
    grid-template-columns: minmax(0, 1fr);
  }

  .earlier-link,
  .earlier-step,
  .earlier-detail,
  .earlier-left {
    grid-column: 1;
  }
}

.fresh {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  padding: 32px;
}

@media (max-width: 560px) {
  .order {
    margin-left: 0;
  }
}
</style>
