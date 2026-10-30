<script setup lang="ts">
/**
 * What the review's Allele session is doing, in one line under the trail.
 *
 * **The answer to "why hasn't it moved?"**, which before this the page could only guess at
 * from silence on the record. Allele knows whether the session is working, blocked on a
 * permission prompt or gone, and `GET /reviews/{id}/session` asks it. Decided on 30
 * October 2026 in DEV-1140; the boards it was drawn from are the status strip mock-ups.
 *
 * | Allele says | The strip reads |
 * |---|---|
 * | running | Working, for how long |
 * | awaiting_input | Waiting on a permission prompt, for how long — answered in Allele |
 * | response_ready, idle | Finished its turn |
 * | suspended, done, no such session | Session gone, or not running when suspended |
 * | anything else | Unknown, with what Allele said |
 * | no answer within a second | Can't reach Allele |
 * | no Allele, or no session recorded | **Nothing at all** |
 *
 * **It only reads.** Nothing here answers a prompt, starts a session, approves or posts.
 * It asks again every {@link SESSION_READ_EVERY_SECONDS} seconds while the tab is visible,
 * and once the moment it is looked at again.
 */
const props = defineProps<{ reviewId: number }>()

const desk = useReviewDesk()
const session = desk.session(props.reviewId)

const line = computed(() => sessionLine(session.data.value?.reading))
const sessionId = computed(() => session.data.value?.session_id ?? null)

/*
 * ---- A fresh session, when this one is gone (DEV-1142) ----
 *
 * **Offered only when Allele says the session is gone and nothing is already asked.** The
 * server asks Allele again before recording it, and the dispatcher asks a third time before
 * starting anything, so a reading this page drew a while ago cannot start a session over a
 * live one. Nothing here approves or posts: a fresh session reads the record and carries on.
 */
const fresh = computed(() => freshSessionLine(session.data.value?.fresh_session))
const canAsk = computed(() =>
  line.value?.tone === 'gone'
  && (session.data.value?.fresh_session == null
    || session.data.value.fresh_session.declined_at != null))
const confirming = ref(false)
const asking = ref(false)
const refused = ref<string | null>(null)

async function startFresh(): Promise<void> {
  if (asking.value) return
  asking.value = true
  refused.value = null
  const result = await desk.requestFreshSession(props.reviewId)
  asking.value = false
  if (result.ok) confirming.value = false
  else refused.value = refusal(result.error)
}

let timer: ReturnType<typeof setInterval> | null = null

function stop(): void {
  if (timer == null) return
  clearInterval(timer)
  timer = null
}

function start(): void {
  if (timer != null || document.visibilityState === 'hidden') return
  timer = setInterval(() => void session.refresh(), SESSION_READ_EVERY_SECONDS * 1000)
}

function onVisibilityChange(): void {
  if (document.visibilityState === 'hidden') {
    stop()
    return
  }
  void session.refresh()
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
</script>

<template>
  <div
    v-if="line"
    class="session"
  >
    <p
      class="strip"
      :data-tone="line.tone"
      data-testid="session-strip"
    >
      <svg
        class="mark"
        width="12"
        height="12"
        viewBox="0 0 12 12"
        aria-hidden="true"
      >
        <circle
          v-if="line.tone === 'working'"
          cx="6"
          cy="6"
          r="5"
        />
        <template v-else-if="line.tone === 'waiting'">
          <rect
            x="2"
            y="1"
            width="3"
            height="10"
          />
          <rect
            x="7"
            y="1"
            width="3"
            height="10"
          />
        </template>
        <path
          v-else-if="line.tone === 'finished'"
          class="stroke"
          d="M1.5 6.5l3 3 6-7"
        />
        <path
          v-else-if="line.tone === 'gone'"
          class="stroke"
          d="M2 2l8 8M10 2l-8 8"
        />
        <path
          v-else-if="line.tone === 'unreachable'"
          class="stroke"
          d="M1 6h3M8 6h3M5 3l2 6"
        />
        <circle
          v-else
          class="stroke"
          cx="6"
          cy="6"
          r="5"
          stroke-dasharray="2 2"
        />
      </svg>
      <span class="said">
        <strong class="label">{{ line.label }}</strong>
        <template v-if="line.facts">
          · {{ line.facts }}
        </template>
      </span>
      <code
        v-if="sessionId"
        class="who"
      >{{ sessionId }}</code>
      <span class="source">from Allele</span>
      <DeskButton
        v-if="canAsk && !confirming"
        kind="secondary"
        class="fresh-button"
        @click="confirming = true"
      >
        Start a fresh session
      </DeskButton>
    </p>

    <p
      v-if="fresh"
      class="fresh rd-meta"
      :data-tone="fresh.tone"
      data-testid="fresh-session"
    >
      {{ fresh.text }}
    </p>

    <section
      v-if="confirming"
      class="confirm"
      aria-labelledby="fresh-session-heading"
    >
      <h2
        id="fresh-session-heading"
        class="confirm-title"
      >
        Start a fresh session on this review?
      </h2>
      <ul class="facts">
        <li>The dispatcher asks Allele that <code>{{ sessionId }}</code> is gone before starting anything. If it is still there, nothing starts and this page says why.</li>
        <li>The new session reads the record and carries on from where the review is. Steps already settled are not redone.</li>
        <li>Your decisions and your messages stay with the review, and the new session reads them.</li>
        <li>Nothing is posted. You still sign off.</li>
      </ul>
      <div class="confirm-actions">
        <DeskButton
          kind="primary"
          :working="asking"
          working-label="Asking"
          @click="startFresh()"
        >
          Start a fresh session
        </DeskButton>
        <DeskButton
          kind="quiet"
          @click="confirming = false"
        >
          Not now
        </DeskButton>
      </div>
      <p
        v-if="refused"
        class="refused"
        role="alert"
      >
        {{ refused }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.session {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.fresh-button {
  margin-left: 8px;
}

.fresh {
  margin: 0;
  padding: 0 16px;
}

.fresh[data-tone='late'],
.fresh[data-tone='declined'] {
  color: var(--rd-warn-ink);
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
  padding: 20px 24px;
  border-radius: var(--rd-radius-card);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-raised);
}

.confirm-title {
  margin: 0;
  font-family: var(--rd-font-display);
  font-size: 24px;
  font-weight: 400;
}

.facts {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding-left: 20px;
  color: var(--rd-ink-dim);
}

.confirm-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.refused {
  margin: 0;
  color: var(--rd-warn-ink);
}

/*
 * One line, the height of a row: the strip is a state line, not a notice, so it is drawn
 * like the queue's status cell — a state in bold and one run of facts after it — and the
 * colour reinforces a shape that already says which state it is.
 */
.strip {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  margin: 0;
  padding: 10px 16px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ground);
  color: var(--rd-ink-dim);
  font-size: 14px;
  line-height: 1.4;
}

.label {
  font-weight: 600;
  color: var(--rd-ink);
}

.mark {
  flex: none;
  fill: currentcolor;
}

.mark .stroke {
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

.who {
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
}

.source {
  margin-left: auto;
  font-size: 12px;
  color: var(--rd-ink-muted);
}

.strip[data-tone='working'] {
  background: var(--rd-accent-wash);
}

.strip[data-tone='working'] .mark,
.strip[data-tone='finished'] .mark {
  color: var(--rd-accent);
}

.strip[data-tone='waiting'] {
  background: var(--rd-warn-wash);
  color: var(--rd-warn-ink);
}

.strip[data-tone='waiting'] .label,
.strip[data-tone='waiting'] .source {
  color: var(--rd-warn-ink);
}

.strip[data-tone='waiting'] .mark {
  color: var(--rd-warn-line);
}

.strip[data-tone='gone'] {
  background: var(--rd-wrong-wash);
  color: var(--rd-wrong);
}

.strip[data-tone='gone'] .label {
  color: var(--rd-wrong);
}

.strip[data-tone='unknown'] .mark,
.strip[data-tone='unreachable'] .mark {
  color: var(--rd-ink-muted);
}

.strip[data-tone='unreachable'] {
  background: var(--rd-page);
  border: 1px dashed var(--rd-control-line);
}
</style>
