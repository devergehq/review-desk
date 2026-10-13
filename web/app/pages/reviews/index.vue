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
 * on tc-portal #9585, after posting review 2: "the actual review itself has disappeared… I
 * can't even get to the review page if I wanted to go back." This is the page that answers
 * that, and the queue's new "Waiting on the author" group is the shortcut to it.
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

        <DeskSection
          v-else
          heading="Every review"
          :count="data.reviews.length"
        >
          <QueueRow
            v-for="row in data.reviews"
            :key="row.review.id"
            :pr="row.review.pr"
            :title="row.review.title ?? 'no title recorded'"
            :meta="about(row)"
            :to="`/reviews/${row.review.id}/understand`"
            action="Read the review"
            action-tone="quiet"
            :age="row.last_activity_at ? `last written to ${sinceLabel(row.last_activity_at)}` : null"
            age-unknown-because="nothing has written to this review since Review Desk started keeping when one last changed"
            :pr-href="prUrl(row.review.repo, row.review.pr)"
            :ticket-href="ticketUrl(row.review.ticket_url)"
            :ticket-id="row.review.ticket_id"
          >
            <span class="step">{{ stepLabel(row) }}</span>
            <!--
              "step 6 of 7", and nothing at all for a review that has not started one.
              `step 0 of 7` would be a nought where the record says plainly that no step
              has begun, and the line above already says "Not started" in words.
            -->
            <span
              v-if="row.step_number != null"
              class="rd-meta step-of"
            >step {{ row.step_number }} of {{ row.steps_total }}</span>
            <!--
              **When and why, on the row**, which is what DEV-945 asks the dismissed view
              for. On its own line because the reason is the developer's sentence and has
              no length limit — the step line above it is two or three words.

              Drawn on every view rather than only on the dismissed one: under
              **Everything** a dismissed review sits beside reviews in play, and a row
              that did not say so would be the one row on the page that reads as something
              it is not.
            -->
            <span
              v-if="row.left_the_desk === 'dismissed' && row.review.dismissed_at"
              class="rd-meta left"
            >Dismissed {{ sinceLabel(row.review.dismissed_at) }}<template
              v-if="row.review.dismissed_reason"
            > — {{ row.review.dismissed_reason }}</template></span>
            <!--
              **A finish, in the words of the thing that happened.** Not "dismissed", which
              would put a decision in the developer's mouth about a merge nobody made — the
              two are told apart on every row, which is DEV-957's criterion. The date is
              drawn only when the record holds one: GitHub does not always give a time, and
              a row must then say the pull request merged without naming the day.
            -->
            <span
              v-else-if="row.left_the_desk === 'finished'"
              class="rd-meta left"
            >{{ finishedLine(row) }}</span>
          </QueueRow>
        </DeskSection>
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

.step-of {
  display: block;
  flex: 1 1 100%;
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
