<script setup lang="ts">
import type { ReviewLayerName, ReviewDetail } from '~/types/desk'

/**
 * The shell every step page of a review sits in.
 *
 * ```
 *  ACME/PORTAL · #514 · GIVE A SCHEDULED EXPORT ITS OWN CONTEXT
 *  ┌── the step trail, seven steps in two groups ──────────────┐
 *  └───────────────────────────────────────────────────────────┘
 *  The one headline sentence, in Newsreader at 40px
 *  the one line under it, when the page has one
 *  …the page…
 * ```
 *
 * **Which review you are in, which step you are on, and where the other steps are** — the
 * three things that have to be on every one of these pages, so they are here once instead
 * of three times. Patrick's reason for the whole design is that someone outside the domain
 * can pick a review up cold and move back to these pages freely while reading the
 * findings; the trail is what "freely" means.
 *
 * **Every page of a review is inside this, including the draft.** DEV-878 brought the last
 * two in: the code review page is step 5, the draft screen is step 6, and the one-page
 * review at `/reviews/{id}` is a redirect to Understand. That is what lets the trail link
 * all five steps that have a page unconditionally — a step can no longer lead somewhere
 * that carries no way back, because there is nowhere left like that.
 */
defineProps<{
  detail: ReviewDetail
  /** Which step this is. The trail marks it and the document title says it. */
  step: ReviewLayerName
  /** The page's one heading. */
  headline: string
  /** The line under the heading, when the page has one. */
  sub?: string | null
}>()
</script>

<template>
  <div class="page">
    <header class="top">
      <p class="rd-rubric where">
        {{ detail.review.repo }} · #{{ detail.review.pr }}
        <template v-if="detail.review.title">
          · {{ detail.review.title }}
        </template>
      </p>

      <StepTrail
        :layers="detail.layers"
        :review-id="detail.review.id"
        :active="step"
        :round="detail.review.round"
      />

      <!--
        How long the review has taken, under the trail it is about: the total with the
        agent and the total waiting on the developer. Here rather than on one page, because
        it is a fact about the review and every one of these pages is the review.
      -->
      <ReviewTimings :timings="detail.timings" />

      <h1 class="rd-display headline">
        {{ headline }}
      </h1>
      <p
        v-if="sub"
        class="sub"
      >
        {{ sub }}
      </p>
    </header>

    <slot />
  </div>
</template>

<style scoped>
/*
 * 52px between sections, which is what the boards set. Wide enough that the eye reads each
 * section as a thing rather than as more of the last one, which is the whole mechanism of
 * "your cognitive load is much lower".
 */
.page {
  display: flex;
  flex-direction: column;
  gap: 52px;
  min-width: 0;
}

.top {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-width: 0;
}

.where {
  margin: 0;
  overflow-wrap: break-word;
}

.headline {
  /* The board's 40px rather than `.rd-display`'s 48px: this sentence runs long. */
  margin: 14px 0 0;
  font-size: 40px;
  line-height: 1.12;
  text-wrap: balance;
  max-width: 940px;
}

.sub {
  margin: 0;
  font-size: 17px;
  max-width: 800px;
}

@media (max-width: 560px) {
  .page {
    gap: 36px;
  }

  .headline {
    font-size: 28px;
    margin-top: 6px;
  }

  .sub {
    font-size: 15px;
  }
}
</style>
