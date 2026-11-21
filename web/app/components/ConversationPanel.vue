<script setup lang="ts">
import type { ReviewDetail, ReviewLayerName, ReviewMessage } from '~/types/desk'
import type { ReviewTab } from '~/utils/reviewTabs'

/**
 * **Ask this review**: the conversation with the review's session, beside every tab.
 * DEV-1333, to S2, S4 and S6; the answers in S7's shape.
 *
 * It replaced `StepConversation`, the fold under the session strip. The conversation is the
 * same one — kept in the record, delivered to the session by Locus, answered with
 * `review-desk message reply` (DEV-1141) — drawn in the shell's right-hand column:
 *
 * ```
 *  Ask this review
 *  ● who is answering · their state           ← answering(), from Allele's reading
 *  Asked so far: Understanding 3  Findings 3  ← askedSoFar(), by each question's tab
 *  …the thread, or the questions to start with…
 *  About: F9 · ColumnPolicy.php:52  ×         ← the tab, and the finding selected on it
 *  [ Ask about F9…                         ↑ ]
 *  Answers stay with this review, in its own session.
 * ```
 *
 * **Every question goes through `send`**, the composer's and *Ask it again, in the format*
 * alike, and `send` asks `asksFromHere` first: DEV-1335 gates it there. Review Desk only
 * stores the question; it never starts, resumes or drives a session.
 *
 * **Its width is the column's, whatever that is.** Nothing here is sized to 380px, so the
 * phone's bottom sheet (DEV-1336) hosts this same component.
 */
const props = defineProps<{
  detail: ReviewDetail
  /** The step a question asked here is kept under, as the server still requires one. */
  step: ReviewLayerName
  /** The tab the developer is on, which a question is about unless a finding is. */
  tab: ReviewTab
}>()

const reviewId = computed(() => props.detail.review.id)
const desk = useReviewDesk()
const conversation = desk.messages(reviewId.value)
const session = desk.session(reviewId.value)
const route = useRoute()

const header = computed(() => answering(session.data.value?.reading))
const asks = computed(() => asksFromHere(header.value.mode))

const messages = computed<ReviewMessage[]>(() => conversation.data.value?.messages ?? [])
const thread = computed(() => threadOf(messages.value))
const asked = computed(() => askedSoFar(messages.value))
const suggestions = computed(() => suggestedQuestions(props.detail))
const suggestionsFrom = computed(() => suggestedFrom(suggestions.value))

/* ---- What a question asked now is about ---- */

/** The finding selected on this tab, as its key, from `?finding=` (or `#F9`). */
const selected = computed<string | null>(() => {
  const seq = findingAsked(route.query, route.hash)
  return props.detail.findings.find(finding => finding.seq === seq)?.key ?? null
})
/** The developer took the finding off this question with the chip's ×. */
const unpinned = ref(false)
watch(selected, () => {
  unpinned.value = false
})
const about = computed(() =>
  aboutHere(props.tab, unpinned.value ? null : selected.value, props.detail.findings))

/* ---- Asking ---- */

const { text } = useAskBox(reviewId.value)
const working = ref(false)
const refused = ref<string | null>(null)
const said = ref<string | null>(null)
const box = ref<HTMLTextAreaElement | null>(null)

/** The one way a question leaves this panel. */
async function send(body: string): Promise<boolean> {
  if (!asks.value || working.value || body.trim().length === 0) return false
  working.value = true
  refused.value = null
  said.value = null
  const result = await desk.askSession(reviewId.value, props.step, body, about.value)
  working.value = false
  if (!result.ok) {
    refused.value = refusal(result.error)
    return false
  }
  said.value = result.message
  void conversation.refresh()
  return true
}

async function ask(): Promise<void> {
  if (await send(text.value)) text.value = ''
}

function askAgain(reply: ReviewMessage): void {
  void send(askAgainInTheFormat(reply))
}

/** Enter asks and Shift+Enter is a new line, as S6's hint says; not while an IME composes. */
function onKey(event: KeyboardEvent): void {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return
  event.preventDefault()
  void ask()
}

/** The line over one of the developer's messages: *You · on F9*, *You · sent the draft back*. */
function youLine(message: ReviewMessage): string {
  if (message.kind === 'send_back') return 'You · sent the draft back'
  const on = askedOnLabel(message.about)
  return on ? `You · ${on}` : 'You'
}

function start(question: string): void {
  text.value = question
  box.value?.focus()
}

/* ---- Reading it again while it is on screen, as the fold did ---- */

let timer: ReturnType<typeof setInterval> | null = null
function stop(): void {
  if (timer != null) clearInterval(timer)
  timer = null
}
function begin(): void {
  if (timer != null || document.visibilityState === 'hidden') return
  timer = setInterval(() => void conversation.refresh(), SESSION_READ_EVERY_SECONDS * 1000)
}
function onVisibilityChange(): void {
  if (document.visibilityState === 'hidden') return stop()
  void conversation.refresh()
  begin()
}
onMounted(() => {
  document.addEventListener('visibilitychange', onVisibilityChange)
  begin()
})
onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibilityChange)
  stop()
})

const fieldId = useId()
const headingId = useId()
</script>

<template>
  <section
    class="panel"
    data-testid="conversation-panel"
    :data-mode="header.mode"
    :aria-labelledby="headingId"
  >
    <header class="head">
      <h2
        :id="headingId"
        class="title"
      >
        Ask this review
      </h2>
      <p
        class="answering rd-meta"
        data-testid="answering"
      >
        <span
          class="dot"
          :data-tone="header.tone"
          aria-hidden="true"
        />
        {{ header.who }} · {{ header.state }}
      </p>
    </header>

    <div class="body">
      <p
        v-if="asked.length"
        class="asked-so-far rd-meta"
        data-testid="asked-so-far"
      >
        Asked so far:
        <span
          v-for="one in asked"
          :key="one.tab"
          class="tally"
        >{{ one.label }} <strong>{{ one.count }}</strong></span>
      </p>

      <ol
        v-if="thread.length"
        class="thread"
      >
        <template
          v-for="message in thread"
          :key="message.seq"
        >
          <li
            v-if="message.author === 'developer'"
            class="mine"
            data-author="developer"
          >
            <p class="who rd-meta">
              {{ youLine(message) }}
            </p>
            <p class="bubble rd-recorded">
              {{ message.body }}
            </p>
            <p
              v-if="deliveryLine(message, session.data.value?.reading)"
              class="delivery rd-meta"
            >
              {{ deliveryLine(message, session.data.value?.reading) }}
            </p>
          </li>
          <ConversationReply
            v-else
            :message="message"
            :source="detail"
            :asks="asks"
            @ask-again="askAgain"
          />
        </template>
      </ol>

      <section
        v-else-if="asks"
        class="start"
        aria-label="Start with one of these"
      >
        <p class="rd-label">
          Start with one of these
        </p>
        <ul>
          <li
            v-for="question in suggestions"
            :key="question.asks"
          >
            <button
              type="button"
              class="suggested"
              @click="start(question.asks)"
            >
              {{ question.asks }}
              <span aria-hidden="true">↗</span>
            </button>
          </li>
        </ul>
        <p
          v-if="suggestionsFrom"
          class="rd-meta"
        >
          {{ suggestionsFrom }}
        </p>
      </section>

      <p
        v-else
        class="rd-meta nothing"
      >
        Nothing has been asked about this review.
      </p>
    </div>

    <form
      class="compose"
      @submit.prevent="ask()"
    >
      <p
        v-if="asks"
        class="about"
        data-testid="about"
      >
        <span>About: {{ aboutLabel(about, detail.findings) }}</span>
        <button
          v-if="about.finding != null"
          type="button"
          class="unpin"
          :aria-label="`Not about ${aboutLabel(about, detail.findings)}`"
          @click="unpinned = true"
        >
          ×
        </button>
      </p>
      <label
        :for="fieldId"
        class="rd-visually-hidden"
      >Ask the review’s session</label>
      <div class="field">
        <textarea
          :id="fieldId"
          ref="box"
          v-model="text"
          rows="2"
          :disabled="!asks"
          :placeholder="asks
            ? askPlaceholder(about, detail.review.pr)
            : 'Without Allele, Review Desk can’t reach your session. Ask in your session; what it writes lands here.'"
          @keydown="onKey"
        />
        <div class="field-foot">
          <span
            v-if="asks"
            class="rd-meta"
          >↵ ask · ⇧↵ new line</span>
          <DeskButton
            kind="primary"
            :working="working"
            working-label="Asking"
            :disabled="!asks || text.trim().length === 0"
            @click="ask()"
          >
            Ask
          </DeskButton>
        </div>
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
        class="rd-meta said"
        role="status"
      >
        {{ said }}
      </p>
      <p class="rd-meta note">
        {{ asks
          ? 'Answers stay with this review, in its own session.'
          : 'Everything asked so far is kept here and still reads.' }}
      </p>
    </form>
  </section>
</template>

<style scoped>
/*
 * A column of three: the header, the thread that scrolls, the composer at the foot. Its
 * height is whatever its host gives it — the shell's column is the window's height, and a
 * host with no height lets the thread grow instead — and nothing here names a width.
 */
.panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  min-width: 0;
}

.head {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 20px 20px 14px;
  border-bottom: 1px solid var(--rd-line);
}

.title {
  margin: 0;
  font-size: var(--rd-type-lead);
  font-weight: 600;
}

.answering {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
}

.dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: var(--rd-radius-dot);
  background: var(--rd-accent);
}

.dot[data-tone='busy'] {
  background: var(--rd-warn-ink);
}

.dot[data-tone='stopped'] {
  background: var(--rd-wrong);
}

.body {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  padding: 14px 20px;
  overflow-y: auto;
}

.asked-so-far {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin: 0;
}

.tally {
  padding: 1px 8px;
  border-radius: var(--rd-radius-xs);
  background: var(--rd-ground);
  color: var(--rd-ink);
}

.thread {
  display: flex;
  flex-direction: column;
  gap: 18px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.mine {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-left: 32px;
}

.who,
.delivery {
  margin: 0;
}

.bubble {
  margin: 0;
  padding: 10px 12px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ground);
  color: var(--rd-ink);
}

.start {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.start > p {
  margin: 0;
}

.start ul {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.suggested {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  min-height: var(--rd-control-min);
  padding: 8px 12px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-control);
  background: var(--rd-surface);
  color: var(--rd-ink);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.suggested:hover {
  border-color: var(--rd-accent);
}

.suggested:focus-visible,
.unpin:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.nothing {
  margin: 0;
}

.compose {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 20px 18px;
  border-top: 1px solid var(--rd-line);
}

.about {
  display: inline-flex;
  align-self: flex-start;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  margin: 0;
  padding: 2px 8px;
  border-radius: var(--rd-radius-xs);
  background: var(--rd-accent-wash);
  color: var(--rd-accent-dark);
  font-size: var(--rd-type-meta);
  overflow-wrap: anywhere;
}

.unpin {
  padding: 0 2px;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-control);
  background: var(--rd-surface);
}

.field:focus-within {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.field textarea {
  padding: 0;
  border: 0;
  background: none;
  font: inherit;
  resize: vertical;
}

.field textarea:focus {
  outline: none;
}

.field textarea:disabled {
  color: var(--rd-ink-muted);
}

.field-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.refused,
.said,
.note {
  margin: 0;
}

.refused {
  color: var(--rd-warn-ink);
}
</style>
