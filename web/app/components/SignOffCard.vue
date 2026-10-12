<script setup lang="ts">
import type { DraftStatus, ReviewEvent } from '~/types/desk'

/**
 * The card a draft is signed off from, in the three states the Controls board draws.
 *
 * | `status` | The card says | What is true |
 * |---|---|---|
 * | `ready` | Ready for your sign-off | nothing has been posted; the split button is live |
 * | `approved` | Signed off | the status changed; the session has not picked it up yet |
 * | `posted` | Posted | the session posted it and recorded the GitHub review id |
 *
 * **The middle state is the one worth having.** Review Desk never posts to GitHub: the
 * developer signs off, the status changes, and the review session reads the approved draft
 * and posts it. There is therefore a real gap between "signed off" and "posted", and a
 * screen that collapsed the two would be claiming something had reached GitHub when
 * nothing had. The board draws a waiting bar for exactly that gap and so does this.
 *
 * It is built here and nothing on the queue or the rules page calls it. That is deliberate
 * and it is DEV-876's shape: the control set is the deliverable, the review page is a later
 * issue, and the page it belongs on is the one this issue does not touch.
 */
const props = withDefaults(
  defineProps<{
    status: DraftStatus
    event: ReviewEvent
    /** "2 findings and 2 comments will be posted." The caller counts; this draws. */
    summary?: string | null
    /**
     * How often the session looks, in seconds, when the record says.
     *
     * **Nothing passes this, and DEV-932 decided the other way.** The draft page now says
     * the number outright — `POSTED_WITHIN_SECONDS` in `app/utils/posting.ts`, two of
     * Locus's watcher intervals — labelled as the page's own setting rather than as
     * something the record reports. The ground this prop rests on is unchanged and still
     * true: nothing reports the schedule. What changed is that "not known here" is useless
     * to a developer deciding whether to go and look at a session. This card is mounted by
     * nothing; whoever wires it should take that decision with it, or take the card out.
     */
    pollSeconds?: number | null
    /** Where the posted review is, once there is one. Read-only: a link out. */
    postedUrl?: string | null
    /** How long after sign-off it was posted, when the record says. */
    postedAfter?: string | null
    working?: boolean
  }>(),
  {
    summary: null,
    pollSeconds: null,
    postedUrl: null,
    postedAfter: null,
    working: false,
  },
)

const emit = defineEmits<{
  'sign-off': []
  'send-back': []
  'update:event': [event: ReviewEvent]
}>()

/* The three words, from `utils/format`, so this card and the queue cannot disagree. */
const chosenEvent = computed(() => eventLabel(props.event))

/**
 * "As Approve, 40 seconds after you signed off."
 *
 * Built here rather than out of `<template v-if>` fragments, which is not fussiness: the
 * linter requires each fragment on its own line, and the renderer then puts a space before
 * the comma and before the full stop. A sentence is one string.
 */
const postedSentence = computed(() =>
  props.postedAfter == null
    ? `As ${chosenEvent.value}.`
    : `As ${chosenEvent.value}, ${props.postedAfter} after you signed off.`,
)
</script>

<template>
  <div class="card">
    <!-- Waiting on the developer. -->
    <template v-if="status === 'ready' || status === 'drafting' || status === 'sent_back'">
      <div>
        <div class="rd-display-card">
          Ready for your sign-off
        </div>
        <div
          v-if="summary"
          class="rd-note said"
        >
          {{ summary }} Nothing has been posted yet.
        </div>
        <div
          v-else
          class="rd-note said"
        >
          Nothing has been posted yet.
        </div>
      </div>

      <SignOffButton
        :event="event"
        :working="working"
        :disabled="status !== 'ready'"
        @sign-off="emit('sign-off')"
        @update:event="value => emit('update:event', value)"
      />

      <DeskButton
        class="back"
        :disabled="status !== 'ready'"
        @click="emit('send-back')"
      >
        Send back with a note
      </DeskButton>
    </template>

    <!-- Signed off, and waiting for the session that posts it. -->
    <template v-else-if="status === 'approved'">
      <div class="line">
        <span class="badge badge--quiet">
          <svg
            width="18"
            height="18"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M3 8.5l3.2 3.2L13 5" /></svg>
        </span>
        <div>
          <div class="rd-display-statement">
            Signed off
          </div>
          <div class="rd-note">
            It will be posted as {{ chosenEvent }}.
          </div>
        </div>
      </div>
      <div class="waiting">
        <!--
          The board draws this bar at 38% of the way along. **Nothing tells this screen how
          far along the session is**, so the track is drawn and left empty rather than
          filled to a number nobody measured — the board's element, in the state the brief
          asks for where there is no source.
        -->
        <div class="track" />
        <div class="rd-meta">
          Waiting for the review session to pick it up.
          <template v-if="pollSeconds != null">
            It checks every {{ pollSeconds }} seconds.
          </template>
          <template v-else>
            How soon is <NotMeasured
              because="nothing reports the review session's schedule to Review Desk"
              says="not known here"
            />.
          </template>
        </div>
      </div>
    </template>

    <!-- Posted by the session. -->
    <template v-else>
      <div class="line">
        <span class="badge">
          <svg
            width="18"
            height="18"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M3 8.5l3.2 3.2L13 5" /></svg>
        </span>
        <div>
          <div class="rd-display-statement">
            Posted
          </div>
          <div class="rd-note">
            {{ postedSentence }}
          </div>
        </div>
      </div>
      <a
        v-if="postedUrl"
        class="out"
        :href="postedUrl"
        target="_blank"
        rel="noreferrer noopener"
      >See it on GitHub<span class="rd-visually-hidden">, opens in a new tab</span></a>
    </template>
  </div>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 26px;
  border-radius: var(--rd-radius-card);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-raised);
}

.said {
  margin-top: 6px;
}

.back {
  align-self: flex-start;
}

.line {
  display: flex;
  align-items: center;
  gap: 12px;
}

.badge {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: var(--rd-accent);
  color: var(--rd-surface);
}

.badge--quiet {
  background: var(--rd-accent-wash);
  color: var(--rd-accent);
}

.waiting {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.track {
  height: 6px;
  border-radius: var(--rd-radius-bar);
  background: var(--rd-rail);
}

.out {
  font-weight: 500;
}
</style>
