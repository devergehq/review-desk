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
  <p
    v-if="line"
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
  </p>
</template>

<style scoped>
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
