<script setup lang="ts">
import type { Brief, ReviewLayerName } from '~/types/desk'

/**
 * **The case for the change** — steps 2, 3 and 4 on one page, in three bands.
 *
 * ```
 *  ┌── the step trail, with 2, 3 and 4 all marked as this page ───────────────┐
 *  Every request the browser sends to the portal now carries the gate's credentials…
 *  4 problems · option B built · fixes 3 of 4 · blind pass would pick D
 *  Right shape; the fail-fast sits one call site too low, and nothing has tested it…
 *
 *  WHAT WAS WRONG · 4         four rows, and which part fixes each
 *  WHAT WAS CHOSEN · 8        eight rows, a dot per problem
 *  WHAT WAS BUILT · 5 PARTS   five rows, with their sizes and what the review found
 * ```
 *
 * ## Why one page
 *
 * Understand, Alternatives and Solution were a page each, and Patrick — who reads about
 * seventy-two reviews a month — said of all three on 16 October 2026 that the content is
 * good and that they are "quite broad, quite large, you've got to dig into them". Measured
 * on review #9639: 3,653 + 3,132 + 2,768 pixels, and three navigations to read one case.
 *
 * Three options were put to him on an artifact. He picked this one: **the three steps become
 * one page with a band each, and every band is the shape of its own answer rather than a
 * summary of it** — four problems as four rows, eight options as eight dotted rows, five
 * parts as five rows with their sizes. Nothing on the page is written by anything; every
 * word is a field, and the counts are read rather than asserted.
 *
 * Option 3 — one screen, no scrolling, each row opening a pane — is where this probably ends
 * up once it is clear which rows he opens. It is deliberately not built: "after some
 * testing, as an experiment".
 *
 * ## Three addresses, one page
 *
 * `/understand`, `/alternatives` and `/solution` all draw this. **Nothing is redirected**,
 * which is deliberate: the queue links to `/understand`, the round switcher builds the same
 * three suffixes for every other round, and every link anybody holds is one of the three. So
 * the routes stay and the bodies merged, and the trail says what has happened — it washes all
 * three steps as the page you are on, while `aria-current` names the one the address is for.
 * Steps 3 and 4 carry a fragment, so pressing one lands at its own band.
 *
 * ## What it costs, stated rather than discovered
 *
 * **The two flows are folded**, and they are the picture Patrick called "perfect". The rows
 * above them are the same before-and-after in the session's own words, and the fold's summary
 * names the picture; `CaseWrong` carries the full argument.
 *
 * **The option prose is one click away**, on the row and again in the band's fold. That was
 * named as this option's own risk when it was put to him — "the cell prose in the options
 * matrix has nowhere to go but a fold, and that prose is the part you said you enjoy".
 *
 * ## The four states a brief has
 *
 * No brief at all; a brief written before any of this; a brief caught between a session's
 * two writes; and the structured brief. The first two are their own pages, because there is
 * nothing to band. The third is the ordinary page with half its fields absent, and each
 * absence says which absence it is rather than drawing nothing — `CaseChosen` and `CaseBuilt`
 * each read their own step's state for that sentence.
 */
defineProps<{
  /**
   * Which of the three addresses this is.
   *
   * It picks the document title's step, what `aria-current` names, and which step the
   * "written in an earlier round" notice is about. It changes nothing on the page itself:
   * all three draw the whole case, which is the point.
   */
  step: ReviewLayerName
}>()

const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `The case · #${review.pr} · Review Desk` : 'The case · Review Desk'
  }),
})

/**
 * Whether this brief was written before the structured one existed.
 *
 * **It carries none of the six things the bands draw**, which is a stronger test than "the
 * headline is missing": a brief whose problem half is being written has a headline and no
 * parts, and that is a brief in progress rather than an old one. A brief with nothing at all
 * is the one the fallback is for.
 */
function writtenTheOldWay(brief: Brief): boolean {
  return brief.headline == null
    && brief.how_it_works_today == null
    && problemsOf(brief).length === 0
    && (brief.flow_before ?? []).length === 0
    && (brief.flow_after ?? []).length === 0
    && brief.support_table == null
}

/**
 * Whether the session has written the problem half and not yet the rest.
 *
 * **A brief arrives in two writes.** The session writes the problem half first — `problem`,
 * `how_it_works_today`, the before flow and its caption, and each problem's number and
 * `was_wrong` — hands that to an independent options pass, and writes everything else when
 * the pass reports. For the minutes in between, a brief has a before flow and numbered
 * problems and no headline, no after flow, no `now_fixed`, no options and no parts.
 *
 * That is a state the page has to name. It is not an old-way brief, it is not an error and
 * it is not an empty page. Saying so is the difference between a reader thinking the record
 * is broken and knowing it is twenty minutes old.
 */
function stillBeingWritten(brief: Brief): boolean {
  return (brief.flow_before ?? []).length > 0 && (brief.flow_after ?? []).length === 0
}

/** The headline, or what to say while the sentence that states the answer is unwritten. */
function headline(brief: Brief): string {
  if (brief.headline) return brief.headline
  return stillBeingWritten(brief)
    ? 'The problem is written. What was done about it is not, yet.'
    : 'This brief records no headline sentence.'
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <!--
        No brief at all. Step 2 has not run, which is a different thing from a brief written
        the old way and a different thing again from one waiting to be confirmed.

        **And where an earlier round wrote it, the heading says so instead.** Nothing carries
        a brief between rounds, so a later round whose session wrote none draws nothing here
        — and "nothing has been written about this problem yet", over a pull request that has
        a brief one round back, is an ambiguity rather than a smaller version of it.
      -->
      <ReviewStepPage
        v-if="!data.brief"
        :detail="data"
        :step="step"
        :here="CASE_STEPS"
        :headline="briefWrittenEarlier(data, step)?.headline
          ?? 'Nothing has been written about this problem yet.'"
        :sub="briefWrittenEarlier(data, step)?.sub
          ?? 'Steps 2 to 4 are where a session writes what the change is solving, what else it could have done, and what it built. It has written none of it, so there is nothing to confirm and nothing after it can run.'"
      >
        <!--
          **Said on this page whatever the brief holds, because a moved head is true of the
          review rather than of the brief.** A new commit on an unposted round moves that
          round's head, so what is on this page may describe a commit the pull request has
          moved past — and that is at its most misleading where there is least to read.
        -->
        <MovedHead :detail="data" />

        <!--
          **The one page a never-started review has**, and so the one place it can be taken
          off the desk.
        -->
        <RemoveReview :detail="data" />
      </ReviewStepPage>

      <!--
        A brief written before any of this. **It still shows**, which is an acceptance
        criterion: the recorded text, with one line saying what it is. Never a blank page and
        never an error.

        The heading is the pull request's own title rather than the recorded problem. There is
        no `headline` to draw — that is what makes this brief an old one — and setting the
        heading to the problem drew the same paragraph twice on one page, once at 40px and
        once as the record.
      -->
      <ReviewStepPage
        v-else-if="writtenTheOldWay(data.brief)"
        :detail="data"
        :step="step"
        :here="CASE_STEPS"
        :headline="data.review.title ?? `Pull request #${data.review.pr}`"
      >
        <BriefConfirmation
          :brief="data.brief"
          :review-id="data.review.id"
          :not-for-signing="data.not_for_signing"
        />

        <MovedHead :detail="data" />

        <DeskNotice tone="plain">
          <strong>Written before the structured brief.</strong>
          This brief records the problem as one piece of text, which is what a brief held
          before 10 October 2026. There is no before-and-after picture, no numbered problems
          and no supporting table to draw — so what the session wrote is below, as it wrote
          it.
        </DeskNotice>

        <DeskSection heading="The problem, as recorded">
          <p class="rd-recorded recorded">
            <TickedText :text="data.brief.problem" />
          </p>
          <!--
            The one `MermaidFigure` on this page, and only for a brief old enough to have a
            `diagram`. `MermaidFigure` owns what happens when the field holds something that
            is not a diagram.
          -->
          <MermaidFigure
            v-if="data.brief.diagram"
            :source="data.brief.diagram"
            label="How the problem happens today, as the session drew it"
          />
        </DeskSection>

        <DeskSection
          v-if="data.brief.provenance"
          heading="Where this came from"
        >
          <p class="rd-recorded recorded">
            <TickedText :text="data.brief.provenance" />
          </p>
        </DeskSection>

        <!--
          **The options and the parts, which an old brief can still carry.** A brief written
          the old way held options as a paragraph each way, and nothing numbered — so the
          matrix has no columns and the band draws the rows and their reasoning. Left in
          rather than gated out: the one acceptance criterion this state has is that what it
          recorded still shows.
        -->
        <CaseChosen
          v-if="optionsOf(data.brief).length"
          :brief="data.brief"
          :layers="data.layers"
        />
        <CaseBuilt
          v-if="partsOf(data.brief).length"
          :detail="data"
          :brief="data.brief"
        />

        <RemoveReview :detail="data" />
      </ReviewStepPage>

      <!-- The structured brief: the case, in three bands. -->
      <ReviewStepPage
        v-else
        :detail="data"
        :step="step"
        :here="CASE_STEPS"
        :headline="headline(data.brief)"
        :sub="data.brief.approach_verdict ?? null"
      >
        <!--
          **The derived answer line, between the headline and the verdict on the approach.**
          Four facts the record already holds, each of them drawn again in the band under it.
          `CaseAnswer` carries the argument for why it is derived rather than written.
        -->
        <template #beneath>
          <CaseAnswer :brief="data.brief" />
        </template>

        <!--
          **Layer 2's one act.** Confirming a brief is one of only two things in the whole
          system that record the developer's sign-off, and this is the page it is done on.
        -->
        <BriefConfirmation
          :brief="data.brief"
          :review-id="data.review.id"
          :not-for-signing="data.not_for_signing"
        />

        <MovedHead :detail="data" />

        <CaseWrong
          :detail="data"
          :brief="data.brief"
        />
        <CaseChosen
          :brief="data.brief"
          :layers="data.layers"
        />
        <CaseBuilt
          :detail="data"
          :brief="data.brief"
        />

        <!--
          **Last on the page, and that is the whole of its placement argument.** Dismissing a
          review is not a step of reading one, so it sits below everything that is.
        -->
        <RemoveReview :detail="data" />
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.recorded {
  margin: 0;
  color: var(--rd-ink-dim);
}
</style>
