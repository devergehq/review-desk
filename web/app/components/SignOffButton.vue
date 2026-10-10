<script setup lang="ts">
import type { ReviewEvent } from '~/types/desk'

/**
 * The split control that signs a draft off: what it would be posted as, and the act.
 *
 * ```
 * ┌──────────────────┬─────────────────────────────┐
 * │ POST AS          │                             │
 * │ Request changes ⌄│  Sign off and post       →  │
 * └──────────────────┴─────────────────────────────┘
 * ```
 *
 * **"Approve" is GitHub's word and it appears only inside the left-hand menu.** That is
 * Patrick's decision of 9 October 2026 (DEV-870): the three options keep GitHub's names
 * because they *are* GitHub's review events, and what the developer does is sign off. The
 * API, the store and the command line still say `approveDraft` and `approved`; the screen's
 * vocabulary is the screen's business.
 *
 * **Nothing here posts to GitHub.** Signing off changes the draft's status; the review
 * session reads the approved draft and posts it. The label says "and post" because that is
 * what the developer is setting in motion, not what this button does.
 *
 * The two halves are two buttons, which is what makes this reachable by keyboard: Tab to
 * the event, Enter to open the menu, Tab to the act. A single button with a click target in
 * one corner would be neither.
 */
const props = withDefaults(
  defineProps<{
    /** Which event the draft would be posted as. */
    event: ReviewEvent
    /** A sign-off is in flight. */
    working?: boolean
    disabled?: boolean
  }>(),
  { working: false, disabled: false },
)

const emit = defineEmits<{ 'sign-off': [], 'update:event': [event: ReviewEvent] }>()

/** GitHub's own names, and what each one does to a pull request. */
const EVENTS: Array<{ value: ReviewEvent, label: string, hint: string }> = [
  { value: 'request_changes', label: 'Request changes', hint: 'Blocks the merge until answered' },
  { value: 'comment', label: 'Comment', hint: 'Says it without blocking' },
  { value: 'approve', label: 'Approve', hint: 'Good to merge' },
]

const open = ref(false)
const label = computed(
  () => EVENTS.find(one => one.value === props.event)?.label ?? props.event,
)

function pick(event: ReviewEvent) {
  emit('update:event', event)
  open.value = false
}
</script>

<template>
  <div class="wrap">
    <div class="split">
      <button
        type="button"
        class="event"
        :aria-expanded="open"
        aria-haspopup="true"
        :disabled="disabled || working"
        @click="open = !open"
      >
        <span class="event-text">
          <span class="rubric">Post as</span>
          <span class="event-label">{{ label }}</span>
        </span>
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        ><path d="M3.5 6l4.5 4.5L12.5 6" /></svg>
      </button>
      <button
        type="button"
        class="act"
        :disabled="disabled || working"
        :aria-busy="working ? 'true' : undefined"
        @click="emit('sign-off')"
      >
        <span
          v-if="working"
          class="spinner"
          aria-hidden="true"
        />
        <span>{{ working ? 'Signing off…' : 'Sign off and post' }}</span>
        <svg
          v-if="!working"
          width="18"
          height="14"
          viewBox="0 0 18 14"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        ><path d="M1 7h15M11 1.5L16.5 7 11 12.5" /></svg>
      </button>
    </div>

    <!--
      The three events, as a radio group: one of them is what this will be posted as, and
      choosing another unchooses the first. Shown below the control rather than over it, so
      nothing is covered and no layer has to be managed.
    -->
    <div
      v-if="open"
      class="menu"
      role="radiogroup"
      aria-label="How to post this review"
    >
      <button
        v-for="one in EVENTS"
        :key="one.value"
        type="button"
        role="radio"
        class="option"
        :class="{ 'option--picked': one.value === event }"
        :aria-checked="one.value === event"
        @click="pick(one.value)"
      >
        <span>{{ one.label }}</span>
        <span class="hint">{{ one.hint }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.split {
  display: flex;
  flex-wrap: wrap;
  border-radius: var(--rd-radius);
  overflow: hidden;
  box-shadow: var(--rd-lift-split);
}

.event,
.act {
  min-height: var(--rd-control-min);
  border: 0;
  font: inherit;
  color: var(--rd-surface);
  cursor: pointer;
  transition:
    background-color 0.14s ease,
    transform 0.08s ease;
}

.event:active:not(:disabled),
.act:active:not(:disabled) {
  transform: translateY(1px);
}

.event {
  display: flex;
  flex: 1 1 170px;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  /* 56px: this is the one action on the card it sits in. */
  min-height: var(--rd-action-tall);
  padding: 0 16px;
  background: var(--rd-accent-pressed);
  text-align: left;
}

.event:hover:not(:disabled) {
  background: var(--rd-accent-dark);
}

.event-text {
  display: flex;
  flex-direction: column;
  line-height: 1.2;
}

.rubric {
  font-size: 11px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--rd-accent-pale);
}

.event-label {
  font-weight: 500;
}

.act {
  display: flex;
  flex: 2 1 220px;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: var(--rd-action-tall);
  padding: 0 20px;
  border-left: 1px solid var(--rd-accent-dark);
  background: var(--rd-accent);
  font-size: 16px;
  font-weight: 600;
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 18%);
}

.act:hover:not(:disabled) {
  background: var(--rd-accent-hover);
}

.event:disabled,
.act:disabled {
  cursor: not-allowed;
  background: var(--rd-rail);
  color: var(--rd-ink-muted);
  box-shadow: none;
}

.menu {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 6px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
}

.option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-height: var(--rd-control-item);
  padding: 0 14px;
  border: 0;
  border-radius: var(--rd-radius-sm);
  background: transparent;
  color: var(--rd-ink);
  font: inherit;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
}

.option--picked {
  background: var(--rd-surface);
  font-weight: 600;
  box-shadow: var(--rd-lift-chosen-quiet);
}

.hint {
  font-size: 13px;
  font-weight: 400;
  color: var(--rd-ink-muted);
}

.spinner {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid rgb(255 255 255 / 40%);
  border-top-color: #ffffff;
  box-sizing: border-box;
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .event,
  .act {
    transition: none;
  }

  .spinner {
    animation: none;
  }
}
</style>
