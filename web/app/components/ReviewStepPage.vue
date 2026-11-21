<script setup lang="ts">
import type { ReviewLayerName, ReviewDetail } from '~/types/desk'
import type { ReviewTab } from '~/utils/reviewTabs'

/**
 * The frame every tab of a review sits in.
 *
 * ```
 *  ┌── ReviewHeader: crumb, #number and title, state word, facts, action, five tabs ──┐
 *  └───────────────────────────────────────────────────────────────────────────────────┘
 *  the round switcher, timings, the session, the conversation
 *  The tab's one heading, in the sans at the scale's title
 *  the one line under it, when the tab has one
 *  …the tab…
 * ```
 *
 * **Which review you are in, which tab you are on, and where the others are** — the three
 * things every one of these pages has to say, so they are here once. Since DEV-1324 the
 * header says them: it replaced the top line that named the review and the step trail that
 * linked the steps. The trail's *other* job, saying where each step stands, is Overview's
 * *Where it stands* since DEV-1326, with the trail folded under it.
 *
 * **The two ways out — the pull request and the ticket — are in the header**, for the
 * reason DEV-936 put them at the top: Patrick, 12 October 2026, "at a minimum, the link at
 * the very top or somewhere prominent that's always going to be shown on every page… And
 * then if there is a ticket, like MOB-999, a link to Linear." `app/utils/links.ts` holds the
 * difference between them: the pull request's address is built, the ticket's is stored.
 */
const props = defineProps<{
  detail: ReviewDetail
  /** Which step this is. The trail marks it and the document title says it. */
  step: ReviewLayerName
  /** Which of the five tabs this page is. The header's tab strip marks it. */
  tab: ReviewTab
  /**
   * The steps whose conversation this tab holds, when it holds more than `step`'s.
   *
   * **Understanding is three steps' worth of conversation.** Alternatives and Solution each
   * had an address, and a message asked on either was kept under its own step; with the
   * three on one tab, a conversation fold that read `step` alone would hide those. The fold
   * reads all of them and asks under `step`.
   */
  conversationSteps?: ReviewLayerName[]
  /**
   * The page's one heading, or `null` for a tab that opens on something else.
   *
   * **Overview has none** (DEV-1326): it opens on *The bottom line*, which is its own box
   * with its own label, and an h2 over it would say the same sentence twice.
   */
  headline: string | null
  /** The line under the heading, when the page has one. */
  sub?: string | null
}>()

/**
 * The earlier round that wrote the brief this round has none of, or null.
 *
 * Null on steps 5 to 7 whatever the record holds: a round's findings, its draft and its
 * harvest are its own, written against its own head commit, and saying one of them was
 * "written in round 1" would be false about the one thing rounds exist to separate.
 * `briefWrittenEarlier` holds that rule, so every page gets the same answer.
 */
const writtenEarlier = computed(() => briefWrittenEarlier(props.detail, props.step))

/** The step's name as the notice above writes it, from the one place that words them. */
const stepName = computed(() => layerLabel(props.step))
</script>

<template>
  <div class="page">
    <header class="top">
      <!--
        **Which review, where it stands, what to do next, and the five tabs** — DEV-1324's
        header, which replaced the top line naming the review and the step trail. The two
        ways out the top line held since DEV-936, the pull request and the ticket, are in it.
      -->
      <ReviewHeader
        :detail="detail"
        :tab="tab"
      />

      <!--
        **Which round of the pull request this is, and the same step of every other one.**
        Under the header now, because the header has a height it may not pass and this draws
        nothing at all on a review whose pull request has one round.
      -->
      <RoundSwitcher
        :detail="detail"
        :step="step"
      />

      <!--
        How long the review has taken, under the trail it is about: the total with the
        agent and the total waiting on the developer. Here rather than on one page, because
        it is a fact about the review and every one of these pages is the review.
      -->
      <ReviewTimings :timings="detail.timings" />

      <!--
        **What the review's session is doing, from Allele**, under how long it has taken.
        Draws nothing when there is no Allele or no session, so a review page on a machine
        without Allele is as it was. Keyed by the review so a page reused for another one
        cannot draw the last review's session for a moment.
      -->
      <SessionStrip
        :key="detail.review.id"
        :review-id="detail.review.id"
      />

      <!--
        **The conversation with that session, on this step**, folded under the strip that
        says whether anything can read it. DEV-1141.
      -->
      <StepConversation
        :key="`conversation-${detail.review.id}-${step}`"
        :review-id="detail.review.id"
        :step="step"
        :steps="conversationSteps"
      />

      <!--
        The headline and the line under it are a session's prose, so the backtick pairs in
        them are drawn as code — `TickedText` owns that, and every field on these four
        pages that a session wrote goes through it.
      -->
      <!--
        **An h2 now**: the review's title in the header is the page's one h1, and this is
        the tab's own heading under it.
      -->
      <h2
        v-if="headline != null"
        class="rd-display headline"
      >
        <TickedText :text="headline" />
      </h2>
      <!--
        **Between the headline and the line under it**, which is the one place a reader's eye
        is already going. The case page puts its derived answer line here — the facts the
        record already holds, under the sentence the session wrote and above the verdict on
        the approach, so the three read as one opening rather than as three sections.

        A slot rather than a second string prop: what goes here is a row of values, and a
        page that had to flatten them into a sentence to pass them in would be writing the
        summary this page exists not to write.
      -->
      <slot name="beneath" />

      <p
        v-if="sub"
        class="sub"
      >
        <TickedText :text="sub" />
      </p>
    </header>

    <!--
      **A dismissed review says so on every one of its pages**, and says it under the trail
      rather than in the card at the foot that undoes it. A developer arriving from the
      Reviews list's dismissed view, or from a link, is reading a review that is in nobody's
      queue and that no session will be started for; finding that out by scrolling to the
      bottom is finding it out too late.

      It links to the card rather than carrying the button. One place undoes a dismissal,
      which is the same card that makes one, so the two cannot come to say different things
      about the same review — and this notice is then on all five step pages without any of
      them gaining a write.
    -->
    <DeskNotice
      v-if="detail.review.dismissed_at"
      tone="problem"
    >
      <strong>This review is dismissed.</strong>
      It is out of the queue, no session will be started for it, and it is not in the
      Reviews list's default view. Everything on it has been kept.
      <NuxtLink :to="`/reviews/${detail.review.id}/understanding#remove-review`">
        Bring it back
      </NuxtLink>
      on Understanding.
    </DeskNotice>

    <!--
      **Where this round's understanding was written, when it was not written here.**
      Nothing carries a brief between rounds: `briefs` is keyed by the review id, as is
      every record behind steps 1 to 4, and opening a round copies none of it. So a later
      round whose session wrote no brief draws nothing at Understand, Alternatives and
      Solution — and a reader meeting an empty page cannot tell "nobody has written this"
      from "it was written in round 1 and nobody has rewritten it".

      **Both halves of it are measured.** `brief_written` on each round says which rounds
      wrote one; `briefWrittenEarlier` answers the latest earlier round that did, which is
      also what makes "no round since" true — any round in between that had written one would
      have been named instead. Nothing here compares two briefs: a diff between rounds is a
      thing in its own right and DEV-976 leaves it alone.

      **The link alone, because the heading above has already said it.** The three pages this
      can appear on take their `headline` and `sub` from the same answer, so the state is
      stated once. DEV-892's sixth defect was the Solution page saying one sentence twice,
      as its headline and again as a notice under it.

      **A destination, not a claim about what is there.** "Open round 2's Alternatives" says
      where it goes; "read it as round 2 wrote it" said round 2 wrote one, which is more than
      `brief_written` measures — round 2 may hold a brief with no options at all, and the
      fixtures have exactly that. A second review of this branch caught it.
    -->
    <DeskNotice v-if="writtenEarlier">
      <!--
        **Closed hard against the full stop**, for the reason `RoundSwitcher`'s own comment
        gives: Vue's `whitespace: 'condense'` condenses the whitespace around an
        interpolation rather than deleting it, so a closing tag on its own line leaves a
        trailing space inside the anchor — and the browser's default underline runs over it.
        Neither the component nor the browser assertions see it, because both match on the
        accessible name and that normalises whitespace.
      -->
      <NuxtLink :to="writtenEarlier.to">Open round {{ writtenEarlier.round }}’s {{ stepName }}</NuxtLink>.
    </DeskNotice>

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

/*
 * **`header-out`, not `out`.** `SignOffCard` already called an anchor `.out`, and `QueueRow`
 * called its links container the same — one name for three things, which `QueueRow`'s own
 * comment explains is a silent measurement bug the moment a test selects it unscoped.
 * `step-pages.nuxt.spec.ts` asserts these are "the only `.out` anchors on the page", which
 * was true only because `SignOffCard` is drawn by no page yet; the draft page is a
 * `ReviewStepPage`, so wiring it in would have made that comment false and the test wrong.
 *
 * **Underlined, not accent-coloured alone.** This line is 13px, uppercase and letterspaced,
 * and at that size a colour change is the one cue a reader can miss — so the link carries
 * the one cue that survives being small, being grey-on-grey and being printed.
 *
 * **No wrapping rule of its own**, and that is measured rather than assumed. `.where`
 * above already carries `overflow-wrap: break-word`, which the anchor inherits, and in an
 * ordinary paragraph that is enough to break a 38-character repository name at 390 pixels:
 * `e2e/header-links.spec.ts` passes with nothing here. `QueueRow` needs `anywhere` for the
 * same name because its meta line is a **flex item**, whose min-content width `break-word`
 * does not reduce — a difference that does not arise here.
 */
.header-out {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.headline {
  /* The page's h1, at the scale's title: the S header's own size for a review's title. */
  margin: 14px 0 0;
  font-size: var(--rd-type-title);
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
    font-size: var(--rd-type-lead);
    margin-top: 6px;
  }

  .sub {
    font-size: 15px;
  }
}
</style>
