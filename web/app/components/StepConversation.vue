<script setup lang="ts">
import type { ReviewLayerName } from '~/types/desk'

/**
 * The conversation with the review's session, on one step of the review. DEV-1141.
 *
 * **Kept in the record and delivered by Locus** — option A of DEV-1140. The developer asks
 * here; Review Desk stores it; the review worker's watcher hands it to the session and
 * marks it read; the session answers with `review-desk message reply`, and the answer
 * appears here. Review Desk never writes into a session.
 *
 * **A fold, under the strip**, because the step's own page is what this screen is for and a
 * conversation is the exception rather than the rule. The fold's line says what is behind
 * it — how many messages, and how many are not read yet — so nobody opens it to find out
 * whether anything is there.
 *
 * **Nothing here signs anything off.** Sending the draft back is still the draft page's own
 * button; its note appears here too, because the server keeps it in the conversation.
 */
const props = defineProps<{ reviewId: number, step: ReviewLayerName }>()

const desk = useReviewDesk()
const conversation = desk.messages(props.reviewId)
const session = desk.session(props.reviewId)

const here = computed(() =>
  (conversation.data.value?.messages ?? []).filter(message => message.step === props.step),
)
const waiting = computed(() =>
  here.value.filter(message => message.author === 'developer' && message.read_at == null).length,
)

const summary = computed(() => {
  if (here.value.length === 0) return `Ask the agent about ${layerLabel(props.step)}`
  const parts = [`${count(here.value.length, 'message')} with the agent on this step`]
  if (waiting.value > 0) parts.push(`${waiting.value} not read yet`)
  return parts.join(' · ')
})

const text = ref('')
const working = ref(false)
const refused = ref<string | null>(null)
const said = ref<string | null>(null)

async function ask(): Promise<void> {
  if (working.value || text.value.trim().length === 0) return
  working.value = true
  refused.value = null
  said.value = null
  const result = await desk.askSession(props.reviewId, props.step, text.value)
  working.value = false
  if (result.ok) {
    text.value = ''
    said.value = result.message
  }
  else {
    refused.value = refusal(result.error)
  }
}

let timer: ReturnType<typeof setInterval> | null = null
function stop(): void {
  if (timer != null) clearInterval(timer)
  timer = null
}
function start(): void {
  if (timer != null || document.visibilityState === 'hidden') return
  timer = setInterval(() => void conversation.refresh(), SESSION_READ_EVERY_SECONDS * 1000)
}
function onVisibilityChange(): void {
  if (document.visibilityState === 'hidden') return stop()
  void conversation.refresh()
  start()
}
onMounted(() => {
  document.addEventListener('visibilitychange', onVisibilityChange)
  start()
})
onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibilityChange)
  stop()
})

const fieldId = useId()
</script>

<template>
  <details
    class="rd-fold conversation"
    data-testid="step-conversation"
  >
    <summary>{{ summary }}</summary>

    <ol
      v-if="here.length"
      class="messages"
    >
      <li
        v-for="message in here"
        :key="message.seq"
        class="message"
        :data-author="message.author"
      >
        <p class="who rd-meta">
          <strong>{{ message.author === 'developer' ? 'You' : 'The session' }}</strong>
          <span v-if="message.kind === 'send_back'"> · sent the draft back</span>
          <span v-if="message.in_reply_to"> · answering {{ message.in_reply_to }}</span>
          · {{ agoLabel(message.created_at) ?? 'at an unrecorded time' }}
        </p>
        <p class="body rd-recorded">
          {{ message.body }}
        </p>
        <p
          v-if="deliveryLine(message, session.data.value?.reading)"
          class="delivery rd-meta"
          :data-undelivered="deliveryLine(message, session.data.value?.reading)?.startsWith('Not delivered') || undefined"
        >
          {{ deliveryLine(message, session.data.value?.reading) }}
        </p>
      </li>
    </ol>

    <form
      class="compose"
      @submit.prevent="ask()"
    >
      <label
        :for="fieldId"
        class="rd-meta"
      >Ask the review’s session about {{ layerLabel(step) }}</label>
      <textarea
        :id="fieldId"
        v-model="text"
        rows="3"
        placeholder="Ask why, or ask for rework"
      />
      <div class="actions">
        <p class="rd-meta note">
          The session’s watcher picks it up within about 20 seconds. A message can ask for an
          explanation or for rework; only you sign off.
        </p>
        <DeskButton
          kind="primary"
          :working="working"
          working-label="Asking"
          :disabled="text.trim().length === 0"
          @click="ask()"
        >
          Ask
        </DeskButton>
      </div>
      <p
        v-if="refused"
        class="refused"
        role="alert"
      >
        {{ refused }}
      </p>
      <p
        v-else-if="said"
        class="rd-meta"
        role="status"
      >
        {{ said }}
      </p>
    </form>
  </details>
</template>

<style scoped>
.conversation {
  margin: 0;
}

.messages {
  display: flex;
  flex-direction: column;
  gap: 14px;
  margin: 8px 0 16px;
  padding: 0;
  list-style: none;
}

.message {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-width: var(--rd-measure-recorded);
}

.who,
.delivery {
  margin: 0;
}

.body {
  margin: 0;
  padding: 10px 12px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ground);
  color: var(--rd-ink);
}

.message[data-author='developer'] .body {
  background: var(--rd-accent-wash);
}

.delivery[data-undelivered] {
  color: var(--rd-warn-ink);
}

.compose {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: var(--rd-measure-recorded);
}

.compose textarea {
  font: inherit;
  padding: 10px 12px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-control);
  resize: vertical;
}

.compose textarea:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
}

.note {
  margin: 0;
  flex: 1 1 280px;
}

.refused {
  margin: 0;
  color: var(--rd-warn-ink);
}
</style>
