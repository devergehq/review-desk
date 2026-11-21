<script setup lang="ts">
import type { ReviewMessage } from '~/types/desk'
import type { CiteSource } from '~/utils/cites'

/**
 * **One reply from the review's session, in the answer contract's shape.** DEV-1333, to S6
 * and S7.
 *
 * The server parsed it before it kept it (DEV-1332) and `answer` is that parse, so this
 * draws `answer` as it came and never checks or mends it:
 *
 * | `kind` | Drawn as |
 * | -- | -- |
 * | `answer` | the headline; each point after its basis; the cites; *not sure* as a caution; the suggestions, inert |
 * | `working` | the headline, and up to six activity steps, the last one in progress |
 * | `cant` | *Can't answer yet*, the headline saying why, and the points saying what would let it |
 * | not parsed | folded: the opening, *Show the rest (n words)*, the line saying why nothing is offered, and one button |
 *
 * **`asked` is the session's own account of the question**, drawn as the session's under
 * *Your question, as your session recorded it* — never as a developer's message.
 *
 * Every word is interpolated, never `v-html`: a reply is data.
 */
const props = defineProps<{
  message: ReviewMessage
  source: CiteSource
  /** Whether a question can be sent from here, which the one button needs. */
  asks: boolean
}>()

const emit = defineEmits<{ 'ask-again': [message: ReviewMessage] }>()

const answer = computed(() => props.message.answer ?? null)

/*
 * **Turn into a finding** — DEV-1334, S6's fourth panel. The same one call Accept on a
 * `new_finding` makes (`addFinding`), so a finding the developer adds has one way in. Offered on
 * an `answer` that cites somewhere to put it; once taken, the line says which finding it is.
 */
const desk = useReviewDesk()
const turned = computed(() => turnedInto(props.message, props.source.findings))
const toFinding = computed(() =>
  answer.value ? turnIntoFinding(answer.value, props.message) : null)
const turning = ref(false)
const turnProblem = ref<string | null>(null)
async function turn(): Promise<void> {
  if (!toFinding.value || turning.value) return
  turning.value = true
  turnProblem.value = null
  const result = await desk.addFinding(props.source.review.id, toFinding.value)
  turning.value = false
  if (!result.ok) turnProblem.value = refusal(result.error)
}
const folded = computed(() => (answer.value ? null : foldedReply(props.message.body)))
const activity = computed(() => answer.value?.activity ?? [])
</script>

<template>
  <li
    class="reply"
    data-author="session"
    :data-kind="answer?.kind ?? 'not_parsed'"
  >
    <template v-if="answer?.asked">
      <p class="who rd-meta">
        Your question, as your session recorded it
      </p>
      <p class="asked rd-recorded">
        {{ answer.asked }}
      </p>
    </template>

    <p class="who rd-meta">
      <span
        class="dot"
        aria-hidden="true"
      />
      <strong>Session</strong>
      <span v-if="answer?.kind === 'cant'"> · can’t answer yet</span>
      · {{ agoLabel(message.created_at) ?? 'at an unrecorded time' }}
    </p>

    <!-- In the contract's shape: the server parsed it. -->
    <template v-if="answer">
      <p
        class="headline"
        :class="{ 'headline--quiet': answer.kind === 'working' }"
      >
        {{ answer.headline }}
      </p>

      <ol
        v-if="answer.kind === 'working' && activity.length"
        class="activity"
        aria-label="What it is doing now"
      >
        <li
          v-for="(step, index) in activity"
          :key="index"
          :data-done="index < activity.length - 1 || undefined"
        >
          <span
            class="mark"
            aria-hidden="true"
          >{{ index < activity.length - 1 ? '✓' : '…' }}</span>
          {{ step }}
        </li>
      </ol>
      <p
        v-if="answer.kind === 'working'"
        class="rd-meta foot"
      >
        You can keep reading; the answer lands here.
      </p>

      <ul
        v-if="answer.points?.length"
        class="points"
      >
        <li
          v-for="(point, index) in answer.points"
          :key="index"
        >
          <span
            class="basis"
            :data-basis="point.basis"
          >{{ BASIS_LABELS[point.basis] }}</span>
          <span class="point">{{ point.text }}</span>
        </li>
      </ul>

      <p
        v-if="answer.cites?.length"
        class="cites"
      >
        <CiteChip
          v-for="(cite, index) in answer.cites"
          :key="index"
          :cite="cite"
          :source="source"
        />
      </p>

      <p
        v-if="answer.not_sure"
        class="not-sure"
      >
        <span aria-hidden="true">!</span>
        <span><span class="rd-visually-hidden">Not sure: </span>{{ answer.not_sure }}</span>
      </p>

      <p
        v-if="turned || toFinding"
        class="turn"
        data-testid="turn-into-a-finding"
      >
        <template v-if="turned">
          <span>✓ Added as {{ turned.key }}</span>
          <NuxtLink :to="findingPath(source.review.id, turned.key)">
            Open {{ turned.key }}
          </NuxtLink>
        </template>
        <DeskButton
          v-else
          kind="quiet"
          :working="turning"
          working-label="Adding"
          @click="turn"
        >
          Turn into a finding
        </DeskButton>
        <span
          v-if="turnProblem"
          class="turn-problem"
          role="alert"
        >{{ turnProblem }}</span>
      </p>

      <section
        v-if="answer.suggests?.length"
        class="suggests"
        aria-label="Suggests"
      >
        <p class="rd-label">
          Suggests
        </p>
        <ul>
          <SuggestionCard
            v-for="(suggestion, index) in answer.suggests"
            :key="index"
            :suggestion="suggestion"
          >
            <template #actions>
              <SuggestionActions
                :suggestion="suggestion"
                :place="index + 1"
                :message="message"
                :source="source"
              />
            </template>
          </SuggestionCard>
        </ul>
      </section>
    </template>

    <!-- Out of shape: kept as the raw message, folded and inert. -->
    <div
      v-else-if="folded"
      class="unparsed"
    >
      <p class="unparsed-head">
        This answer didn’t follow the format
      </p>
      <p class="rd-recorded opening">
        “{{ folded.opening }}{{ folded.rest ? '…' : '' }}”
      </p>
      <details
        v-if="folded.rest"
        class="rd-fold"
      >
        <summary>Show the rest ({{ folded.rest.toLocaleString('en-AU') }} {{ folded.rest === 1 ? 'word' : 'words' }})</summary>
        <p class="rd-recorded">
          {{ folded.restText }}
        </p>
      </details>
      <p class="rd-meta">
        No cites and no basis, so no actions are offered and it isn’t kept with the review.
      </p>
      <DeskButton
        kind="quiet"
        :disabled="!asks"
        @click="emit('ask-again', message)"
      >
        Ask it again, in the format
      </DeskButton>
    </div>
  </li>
</template>

<style scoped>
.reply {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.who {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  margin: 0;
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: var(--rd-radius-dot);
  background: var(--rd-accent);
}

.reply[data-kind='cant'] .dot,
.reply[data-kind='not_parsed'] .dot {
  background: var(--rd-warn-ink);
}

.asked {
  margin: 0 0 4px;
  padding: 10px 12px;
  border-left: 3px solid var(--rd-line-strong);
  color: var(--rd-ink-dim);
}

.headline {
  margin: 0;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.headline--quiet {
  font-weight: 400;
}

.activity {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 10px 12px;
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-control);
  list-style: none;
}

.activity li {
  display: flex;
  gap: 8px;
  overflow-wrap: anywhere;
}

.mark {
  flex: none;
  width: 14px;
  color: var(--rd-accent);
}

.foot {
  margin: 0;
}

.points {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.points li {
  display: flex;
  gap: 8px;
  align-items: baseline;
}

.basis {
  flex: none;
  padding: 1px 8px;
  border: 1px solid var(--rd-line-strong);
  border-radius: var(--rd-radius-pill);
  font-size: var(--rd-type-meta);
}

.basis[data-basis='ran'] {
  border-color: var(--rd-accent);
  color: var(--rd-accent-dark);
}

.point {
  overflow-wrap: anywhere;
}

.cites {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
}

.not-sure {
  display: flex;
  gap: 8px;
  margin: 0;
  color: var(--rd-warn-ink);
  overflow-wrap: anywhere;
}

.turn {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: center;
  margin: 0;
  color: var(--rd-accent-dark);
  font-size: var(--rd-type-ui);
}

.turn a {
  color: var(--rd-accent-dark);
}

.turn-problem {
  flex-basis: 100%;
  color: var(--rd-warn-ink);
}

.suggests {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--rd-line);
}

.suggests > p {
  margin: 0;
}

.suggests ul {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.unparsed {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--rd-warn-line);
  border-radius: var(--rd-radius-card);
}

.unparsed p {
  margin: 0;
}

.unparsed-head {
  font-weight: 600;
  color: var(--rd-warn-ink);
}

.opening {
  overflow-wrap: anywhere;
}
</style>
