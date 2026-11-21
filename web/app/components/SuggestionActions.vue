<script setup lang="ts">
import type { ReviewMessage, Suggestion } from '~/types/desk'
import type { CiteSource } from '~/utils/cites'
import type { ReplyFill } from '~/utils/suggestionActions'

/**
 * **Accept and Dismiss on one suggestion**, in `SuggestionCard`'s `actions` slot. DEV-1334,
 * to S8 and S9.
 *
 * ```
 *  Regrade F6 from should to blocker               [ Regrade ] [ Keep as should ]
 *  With F16 found, a hidden column is selected…
 *  ─ answered ─
 *  ✓ Added as F16 · should · ran                   Open F16
 *  Dismissed                                       Say why
 * ```
 *
 * **Accept is a control the page already has**, and goes through the store call that
 * control makes — `utils/suggestionActions.ts` has the table. Nothing here signs off,
 * approves or posts, and nothing here writes to a session: Review Desk stores the
 * developer's decision and a watching session reads it like anything else.
 *
 * **Dismiss is one click, and the reason comes after it**, so the click that says *no* is
 * not held up by a field. *Say why* sends the dismissal again with the words.
 *
 * **No Undo.** The page's own decisions have none (the way back from Set aside is Post, a
 * decision of its own), and a finding is never deleted. An added finding is set aside on
 * Findings like any other. See the pull request's *Departs from the design*.
 */
const props = defineProps<{
  suggestion: Suggestion
  /** Its place in the reply's `suggests`, from 1. */
  place: number
  message: Pick<ReviewMessage, 'seq' | 'suggestion_answers'>
  source: CiteSource
}>()

const desk = useReviewDesk()
const router = useRouter()
const reviewId = computed(() => props.source.review.id)
const prompted = computed(() => ({ message: props.message.seq, suggestion: props.place }))

const answered = computed(() => answerAt(props.message, props.place))
const line = computed(() => (answered.value ? answeredLine(props.suggestion, answered.value) : null))
const added = computed(() => answered.value?.finding_key ?? null)

const working = ref(false)
const problem = ref<string | null>(null)
/** Accepted without a write: the ask box or the reply was filled. Not kept by the record. */
const filled = ref<string | null>(null)

/*
 * The paragraph an `add_to_draft` adds its sentence to: read only for the one suggestion
 * that needs it, since every other card would be reading a draft for nothing.
 */
const needsTheDraft = props.suggestion.type === 'add_to_draft'
  && props.suggestion.finding != null && (props.suggestion.text ?? '').length > 0
const draft = needsTheDraft ? desk.draft(reviewId) : null
const paragraph = computed<string | null>(() => {
  if (props.suggestion.type !== 'add_to_draft' || !props.suggestion.finding) return null
  const key = props.suggestion.finding
  const block = draft?.data.value?.sections.find(
    section => section.kind === 'finding' && section.finding_key === key,
  )
  return block?.text ?? null
})

const replyFill = useState<ReplyFill | null>(`reply-fill:${reviewId.value}`, () => null)
const { fill } = useAskBox(reviewId.value)

async function accept(): Promise<void> {
  if (working.value) return
  problem.value = null
  const acting = accepting(props.suggestion, prompted.value, paragraph.value)
  switch (acting.kind) {
    case 'add':
    case 'decide': {
      working.value = true
      const result = acting.kind === 'add'
        ? await desk.addFinding(reviewId.value, acting.request)
        : await desk.decideFinding(reviewId.value, acting.seq, acting.request)
      working.value = false
      if (!result.ok) problem.value = refusal(result.error)
      return
    }
    case 'fill-ask':
      // The composer's own gate decides whether it can be sent: nothing is sent from here.
      fill(acting.question)
      filled.value = 'In the ask box. Nothing is sent until you ask.'
      return
    case 'fill-reply':
      replyFill.value = acting.fill
      filled.value = 'Taken to Replies to fill. Nothing is sent until you send it there.'
      await router.push(replyPath(reviewId.value, `F${acting.fill.seq}`))
      return
    case 'open':
      await router.push(
        acting.to === 'draft'
          ? tabPath(reviewId.value, 'draft')
          : findingPath(reviewId.value, acting.finding!),
      )
  }
}

async function dismiss(reason: string | null = null): Promise<void> {
  if (working.value) return
  problem.value = null
  working.value = true
  const result = await desk.dismissSuggestion(reviewId.value, props.message.seq, props.place, reason)
  working.value = false
  if (!result.ok) problem.value = refusal(result.error)
  else asking.value = false
}

/* ---- Why, after the click ---- */

const asking = ref(false)
const why = ref('')
const whyId = useId()
function sayWhy(): void {
  asking.value = true
  why.value = ''
  void nextTick(() => document.getElementById(whyId)?.focus())
}
function sendWhy(): void {
  const said = why.value.trim()
  if (said.length > 0) void dismiss(said)
}
</script>

<template>
  <div
    class="actions"
    data-testid="suggestion-actions"
    :data-answered="answered?.verdict ?? (filled ? 'filled' : 'open')"
  >
    <template v-if="answered">
      <p
        class="answered"
        :data-verdict="answered.verdict"
      >
        <span
          v-if="answered.verdict === 'accepted'"
          aria-hidden="true"
        >✓ </span>{{ line }}
      </p>
      <NuxtLink
        v-if="added"
        class="link"
        :to="findingPath(reviewId, added)"
      >
        Open {{ added }}
      </NuxtLink>
      <button
        v-else-if="answered.verdict === 'dismissed' && !answered.reason && !asking"
        type="button"
        class="link say-why"
        @click="sayWhy"
      >
        Say why
      </button>
      <form
        v-if="asking"
        class="reason"
        @submit.prevent="sendWhy"
      >
        <label
          class="rd-visually-hidden"
          :for="whyId"
        >Why you dismissed it</label>
        <input
          :id="whyId"
          v-model="why"
          class="field"
          maxlength="200"
          placeholder="Why not, in a line"
          @keydown.esc="asking = false"
        >
        <DeskButton
          kind="quiet"
          :working="working"
          :disabled="why.trim().length === 0"
          @click="sendWhy"
        >
          Keep the reason
        </DeskButton>
      </form>
    </template>

    <p
      v-else-if="filled"
      class="answered rd-meta"
    >
      {{ filled }}
    </p>

    <div
      v-else
      class="buttons"
    >
      <DeskButton
        kind="primary"
        :working="working"
        working-label="Accepting"
        :data-testid="`accept-${suggestion.type}`"
        @click="accept"
      >
        {{ acceptLabel(suggestion) }}
      </DeskButton>
      <DeskButton
        kind="secondary"
        :disabled="working"
        :data-testid="`dismiss-${suggestion.type}`"
        @click="dismiss()"
      >
        {{ dismissLabel(suggestion) }}
      </DeskButton>
    </div>

    <p
      v-if="problem"
      class="problem"
      role="alert"
    >
      {{ problem }}
    </p>
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  align-items: center;
}

.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.answered {
  margin: 0;
  font-size: var(--rd-type-ui);
}

.answered[data-verdict='accepted'] {
  color: var(--rd-accent-dark);
}

.link {
  color: var(--rd-accent-dark);
  font-size: var(--rd-type-ui);
}

.say-why {
  min-height: var(--rd-control-min);
  padding: 0;
  font: inherit;
  background: none;
  border: 0;
  cursor: pointer;
}

.reason {
  display: flex;
  flex-basis: 100%;
  gap: 6px;
}

.field {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  font: inherit;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-control);
}

.problem {
  flex-basis: 100%;
  margin: 0;
  color: var(--rd-warn-ink);
  font-size: var(--rd-type-meta);
}
</style>
