<script setup lang="ts">
import type { ReviewLayerName, ReviewDetail } from '~/types/desk'

/**
 * The shell every step page of a review sits in.
 *
 * ```
 *  ACME/PORTAL · #514 · ABC-402 · GIVE A SCHEDULED EXPORT ITS OWN CONTEXT
 *  └ to GitHub ┘   └ to the tracker ┘
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
 *
 * ## The two links, and why they are here rather than on one page
 *
 * **The pull request and the ticket are the two places a reader leaves a review for**, and
 * until DEV-936 the top line named both and linked neither: the repository and the number
 * were plain text, the ticket was nowhere near the top, and the only way out to GitHub was
 * a row in a table at the foot of the Understand page. Patrick, 12 October 2026: "at a
 * minimum, the link at the very top or somewhere prominent that's always going to be shown
 * on every page… And then if there is a ticket, like MOB-999, a link to Linear." This is
 * the one line every page of a review already has, which is the whole reason it is the
 * place to put them.
 *
 * They arrive by opposite routes, and `app/utils/links.ts` holds that difference: the pull
 * request's address is **built** from `repo` and `pr`, and the ticket's is **stored**,
 * because nothing can turn an opaque `ABC-402` into an address. A review with an id and no
 * address draws the id as text — which is every review opened before the field existed,
 * and is not a broken link.
 */
const props = defineProps<{
  detail: ReviewDetail
  /** Which step this is. The trail marks it and the document title says it. */
  step: ReviewLayerName
  /** The page's one heading. */
  headline: string
  /** The line under the heading, when the page has one. */
  sub?: string | null
}>()

/**
 * Where this review's pull request is. Built from the record, never read from it.
 *
 * `null` for a `repo` that is not a repository name, which `prUrl` refuses rather than
 * building a link to somewhere else out of — the header then draws the repository and the
 * number as text, which is what it does for a ticket it cannot link.
 */
const pullRequest = computed(() => prUrl(props.detail.review.repo, props.detail.review.pr))

/**
 * Where this review's ticket is, or `null` for a review that has no address to draw.
 *
 * Null for two different reviews, and the header treats them the same because there is
 * nothing to tell a reader apart: one was never given an address, and one holds a value
 * that is not `https` and so is refused at render as well as at the write.
 */
const ticket = computed(() => ticketUrl(props.detail.review.ticket_url))

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
        **Which review you are in, and the two ways out of it.** The repository and the
        number are the link to the pull request; the ticket's id is the link to the
        tracker, and is drawn as text when the record holds no address for it. Both say
        where they go and that they open elsewhere, because "opens in a new tab" is not
        something a reader can see coming.
      -->
      <p class="rd-rubric where">
        <a
          v-if="pullRequest"
          class="header-out"
          :href="pullRequest"
          target="_blank"
          rel="noopener noreferrer"
        >{{ detail.review.repo }} · #{{ detail.review.pr }}<span class="rd-visually-hidden">, on GitHub, opens in a new tab</span></a>
        <template v-else>
          {{ detail.review.repo }} · #{{ detail.review.pr }}
        </template>

        <template v-if="detail.review.ticket_id">
          ·
          <a
            v-if="ticket"
            class="header-out"
            :href="ticket"
            target="_blank"
            rel="noopener noreferrer"
          >{{ detail.review.ticket_id }}<span class="rd-visually-hidden">, on the tracker, opens in a new tab</span></a>
          <template v-else>
            {{ detail.review.ticket_id }}
          </template>
        </template>

        <template v-if="detail.review.title">
          · {{ detail.review.title }}
        </template>
      </p>

      <!--
        **Which round of the pull request this is, and the same step of every other one.**
        Here because the three lines of this header descend in scope: which pull request,
        which round of it, which step of that round. Drawn once for all six step pages, and
        drawing nothing at all on a review whose pull request has one round.

        Its own component and its own block, because DEV-962 is changing this header too.
      -->
      <RoundSwitcher
        :detail="detail"
        :step="step"
      />

      <StepTrail
        :layers="detail.layers"
        :review-id="detail.review.id"
        :active="step"
        :round="detail.review.round"
        :off-the-desk="detail.not_for_signing != null"
      />

      <!--
        How long the review has taken, under the trail it is about: the total with the
        agent and the total waiting on the developer. Here rather than on one page, because
        it is a fact about the review and every one of these pages is the review.
      -->
      <ReviewTimings :timings="detail.timings" />

      <!--
        The headline and the line under it are a session's prose, so the backtick pairs in
        them are drawn as code — `TickedText` owns that, and every field on these four
        pages that a session wrote goes through it.
      -->
      <h1 class="rd-display headline">
        <TickedText :text="headline" />
      </h1>
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
      <NuxtLink :to="`/reviews/${detail.review.id}/understand#remove-review`">
        Bring it back
      </NuxtLink>
      on Understand.
    </DeskNotice>

    <!--
      **Where this round's understanding was written, when it was not written here.**
      Nothing carries a brief between rounds: `briefs` is keyed by the review id, as is
      every record behind steps 1 to 4, and opening a round copies none of it. So a later
      round whose session wrote no brief draws nothing at Understand, Alternatives and
      Solution — and a reader meeting an empty page cannot tell "nobody has written this"
      from "it was written in round 1 and nobody has rewritten it".

      **Both halves of the sentence are measured.** `brief_written` on each round says which
      rounds wrote one; `briefWrittenEarlier` answers the latest earlier round that did,
      which is also what makes "no round since" true — any round in between that had written
      one would have been named instead. Nothing here compares two briefs: a diff between
      rounds is a thing in its own right and DEV-976 leaves it alone.
    -->
    <DeskNotice v-if="writtenEarlier">
      <strong>This round has written no {{ stepName }} of its own.</strong>
      Round {{ writtenEarlier.round }} wrote the last one and no round since has written
      another; nothing carries a brief from one round to the next.
      <NuxtLink :to="writtenEarlier.to">
        Read {{ stepName }} in round {{ writtenEarlier.round }}
      </NuxtLink>.
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
