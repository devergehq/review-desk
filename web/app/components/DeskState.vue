<script setup lang="ts" generic="T">
/**
 * The three states a read has, in one place for all five screens.
 *
 * A read over HTTP is in flight, or it failed, or it answered — and before the dashboard
 * called an API none of that existed, so each screen rendered a value and nothing else.
 * This is what put loading, "the server is not answering", and a retry on all four from one
 * component rather than from four near-identical blocks that would have drifted.
 *
 * **It keeps the last good answer under an error.** A server stopped while a page is open
 * leaves the developer reading what they were already reading, with a message over it and
 * a retry — not a blank page, which DEV-862 asks for by name and which is also the only
 * honest rendering: the record has not gone anywhere, the connection has.
 *
 * ## What it says, and why each one is different
 *
 * | State | What it says |
 * |---|---|
 * | First read in flight | Nothing but a reserved space, so the page does not jump when it lands |
 * | Nothing answered at all | Review Desk is not running, and the command that starts it |
 * | Nothing answered in time | Something is in front of it, or it is stuck — a different fix |
 * | The answer was not this contract's | Something else is answering on this address |
 * | `store_unavailable` | Review Desk is broken, loudly — the brief's "broken is loud" |
 * | `not_found` | This review, draft or rule is not on the desk, in the server's own words |
 * | `conflict` | A statement about the record: retyping will not change it |
 * | `bad_request`, `invalid_value` | What was sent, and what would have been accepted |
 * | Answered, nothing in it | The screen's own empty slot, which says what is true |
 */
import type { DeskError } from '~/types/desk'
import type { DeskResource } from '~/composables/useReviewDesk'

const desk = useReviewDesk()

const props = defineProps<{
  /** The read to render. */
  resource: DeskResource<T>
  /**
   * Whether the answer counts as empty. Each screen decides: an inbox with no items, a
   * rules page with no rules. Left off, any answer is enough.
   */
  isEmpty?: (data: T) => boolean
  /** What this screen is, for a sentence a person can act on — "the inbox", "this review". */
  what?: string
}>()

defineSlots<{
  /** The screen, once there is something to draw. */
  default: (props: { data: T }) => unknown
  /** What to say when the answer holds nothing. */
  empty?: () => unknown
}>()

const what = computed(() => props.what ?? 'this screen')

/** The first read: nothing has ever answered, so there is nothing to show under a message. */
const firstRead = computed(() => props.resource.pending.value && !props.resource.loaded.value)

/** An error with nothing behind it fills the screen; one with records sits over them. */
const blocking = computed(
  () => props.resource.error.value != null && props.resource.data.value == null,
)

const empty = computed(() => {
  const data = props.resource.data.value
  if (data == null) return false
  return props.isEmpty ? props.isEmpty(data) : false
})

/**
 * A heading and a sentence for each way a read can fail.
 *
 * **The tone is not decided here.** Each branch used to carry its own `'blocked'` or
 * `'problem'`, which is `refusalTone` in `utils/format.ts` written a second time — two
 * lists that have to agree, with nothing tying them, on the surface where the distinction
 * matters most: `blocked` is what boxes the message and shows a command to run. Settled
 * once, below, from the error itself.
 */
const problem = computed(() => {
  const error: DeskError | null = props.resource.error.value
  if (!error) return null

  const tone = refusalTone(error)

  if (error.code === 'unreachable') {
    return {
      heading: 'Review Desk is not answering',
      /*
       * The data layer's own sentence, not one written here. There are two ways to not
       * answer — nothing is listening, or something is and is taking too long — and they
       * send the developer to different places. Writing the sentence here threw that
       * distinction away, which is exactly what the distinction was added to avoid.
       */
      body: `${sentence(error.message)} The record is where it was.`,
      command: 'review-desk serve',
      tone,
    }
  }

  if (error.code === 'unreadable') {
    return {
      heading: 'Something else is answering',
      body: sentence(error.message),
      command: null,
      tone,
    }
  }

  if (error.code === 'store_unavailable') {
    return {
      heading: 'Review Desk could not read its database',
      body:
        `${sentence(error.message)} This is Review Desk being unable to run, `
        + 'not a refused request.',
      command: 'review-desk db version',
      tone,
    }
  }

  if (error.code === 'github_unavailable') {
    return {
      heading: 'A read of GitHub could not be made',
      body: `${sentence(error.message)} Review Desk reads GitHub through your own gh login.`,
      command: 'gh auth login',
      tone,
    }
  }

  if (error.code === 'not_found') {
    return {
      /*
       * A heading that reads the same whatever `what` is. "The desk holds no this review"
       * was what interpolating it produced, and the server's own message already names the
       * thing — "review 1 has no draft" — so the heading does not have to.
       */
      heading: 'Not on the desk',
      body: `${sentence(error.message)} It may have been opened on another database.`,
      command: null,
      tone,
    }
  }

  if (error.code === 'conflict') {
    return {
      /*
       * **Not the same sentence as a refused request, and the contract draws the line.** A
       * `conflict` is a statement about the record — this draft is approved, this review
       * holds back no nits — and nothing the developer retypes will change it. A
       * `bad_request` or an `invalid_value` is about what they sent, and retyping is
       * exactly the answer. One heading for both would have told them to try again at the
       * moment trying again is pointless.
       */
      heading: 'The record is not in that state',
      body: refusal(error),
      command: null,
      tone,
    }
  }

  /* `bad_request` and `invalid_value`: what was sent, which the developer can change. */
  return {
    heading: 'That was refused',
    body: refusal(error),
    command: null,
    tone,
  }
})

function retry() {
  void props.resource.refresh()
}
</script>

<template>
  <!--
    A reserved space rather than a spinner. The height is the one thing a loading state owes
    the page: without it the content lands and everything below it moves, which DEV-862
    calls out as a layout jump.
  -->
  <div
    v-if="firstRead"
    class="loading"
    role="status"
    aria-live="polite"
  >
    <span class="rd-meta">Reading {{ what }}…</span>
  </div>

  <template v-else>
    <!--
      Over the records when there are some, instead of them when there are none. Either way
      the retry is the same control, so a developer who has restarted the server has one
      thing to click.
    -->
    <DeskNotice
      v-if="problem"
      :tone="problem.tone"
      class="problem"
    >
      <strong class="problem-heading">{{ problem.heading }}</strong>
      {{ problem.body }}
      <code
        v-if="problem.command"
        class="rd-mono command"
      >{{ problem.command }}</code>
      <!--
        Only while there are invented records to switch to, which is never in a built
        dashboard. A developer who has not started the binary can still look at a screen.
      -->
      <button
        v-if="problem.tone === 'blocked' && desk.sampleAvailable.value && !desk.isSampleData.value"
        type="button"
        class="rd-btn sample"
        @click="desk.useSampleRecords()"
      >
        Use sample records
      </button>
      <button
        type="button"
        class="rd-btn retry"
        :disabled="resource.pending.value"
        @click="retry"
      >
        {{ resource.pending.value ? 'Retrying…' : 'Retry' }}
      </button>
    </DeskNotice>

    <template v-if="!blocking && resource.data.value != null">
      <slot
        v-if="!empty"
        :data="resource.data.value"
      />
      <slot
        v-else
        name="empty"
      />
    </template>
  </template>
</template>

<style scoped>
.loading {
  /*
   * Tall enough that the first screenful does not shift when the answer arrives. Not a
   * guess at the content's height — nothing can be — but enough that the page's own chrome
   * stays put.
   */
  min-height: 180px;
  display: flex;
  align-items: flex-start;
  padding-top: 8px;
}

.problem {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.problem-heading {
  font-weight: 600;
}

.command {
  padding: 2px 6px;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-xs);
  font-size: 13px;
}

.sample {
  margin-left: auto;
}

.retry {
  /*
   * `auto` on whichever of the two comes first, so the pair sits at the right-hand end
   * whether or not the sample control is there.
   */
  margin-left: auto;
}

.sample + .retry {
  margin-left: 0;
}
</style>
