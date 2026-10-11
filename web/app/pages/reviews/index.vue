<script setup lang="ts">
import type { ReviewListItem } from '~/types/desk'

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

/** Which repository to show. `ALL_REPOSITORIES` for every one, as the queue words it. */
const repo = ref(ALL_REPOSITORIES)
const reviews = desk.reviews(repo)

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
 */
function about(row: ReviewListItem): string {
  const parts = [row.review.repo]
  parts.push(
    row.rounds_on_pr > 1
      ? `round ${row.review.round} of ${row.rounds_on_pr}`
      : `round ${row.review.round}`,
  )
  if (row.review.github_review_id) parts.push('posted')
  if (row.review.ticket_id) parts.push(row.review.ticket_id)
  return parts.join(' · ')
}

/** Whether any row has no activity time, which the caption then says. */
const someTimesMissing = computed(
  () => (reviews.data.value?.reviews ?? []).some(row => row.last_activity_at == null),
)

/** Nothing at all, which is a new database rather than a broken one. */
function isEmpty(answer: { reviews: ReviewListItem[] }): boolean {
  return answer.reviews.length === 0
}
</script>

<template>
  <div class="reviews">
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
          <p class="rd-meta aside-line">
            Every review the record holds, whatever became of it. How long one took is on
            its own page.
          </p>
        </div>

        <div class="filter">
          <DecisionTrack
            v-model="repo"
            label="Which repository to show"
            :options="repoTabs"
            :disabled="repos.pending.value && !repos.loaded.value"
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

        <DeskSection
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
          </QueueRow>
        </DeskSection>
      </template>

      <!--
        A database with nothing in it. It reads as new rather than broken, and it points at
        the queue, which is where a review is started.
      -->
      <template #empty>
        <DeskCard
          tone="raised"
          class="fresh"
        >
          <h1 class="rd-display">
            No reviews yet
          </h1>
          <p class="rd-note rd-measure">
            This database holds no reviews. One turns up here as soon as an agent session
            records it, and stays here afterwards — including once it has been posted,
            which is what this page is for.
          </p>
          <p class="rd-note rd-measure">
            To start one, paste a pull request link on the
            <NuxtLink to="/">queue</NuxtLink>.
          </p>
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

.fresh {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 32px;
}

@media (max-width: 560px) {
  .order {
    margin-left: 0;
  }
}
</style>
