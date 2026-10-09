<script setup lang="ts">
/**
 * A choice between a few things, as one track with the chosen one lifted out of it.
 *
 * The Controls board draws this four times — deciding on a finding, regrading it, answering
 * the author, and filtering by repository — and the Rules board draws it a fifth, for the
 * two answers to a stale rule. It is the same control each time: a `#E8EAE9` track, 4px of
 * padding, and the chosen item on a white card with a shadow while the rest stay flat. So
 * it is one component, with the parts each use needs: an icon, a keyboard key, a count.
 *
 * ## Two modes, because two of those five are not a choice
 *
 * `choose` (the default) is a **`radiogroup`**: one option is the state, picking another
 * replaces it, `v-model` carries it, and the arrow keys come free.
 *
 * `act` is a **group of buttons**: "Retire it" and "Keep 60 more days" are two different
 * things to do, and neither is a state the control is left in afterwards. They look
 * identical on the board and they must not be the same element — a radio that fires an
 * action and then unchecks itself announces the opposite of what happened.
 *
 * ## The keys work, and they do not fire while you are typing
 *
 * DEV-876: "Keys shown on a control must actually work, and must not fire while the
 * developer is typing in a field." A key is drawn next to an option only when the option
 * carries one, and then it picks that option from anywhere on the page — because the thing
 * it decides is what the page is about, and reaching for the pointer to press `Y` is the
 * waste the key exists to remove.
 *
 * Four guards, and each is a real way this goes wrong:
 *
 * 1. **The event's target is a field.** An `input`, `textarea`, `select` or anything
 *    `contenteditable`: `G` is a letter in "regrade" and in a reason somebody is typing.
 * 2. **A modifier is held.** `Cmd-E` is the browser's, not ours.
 * 3. **The key is composing.** An IME candidate window is open and these are not keys yet.
 * 4. **The track is disabled.** A write is in flight; the pointer cannot press it either.
 *
 * The listener is added on mount and removed on unmount, so a component that has gone does
 * not keep deciding things.
 */
export interface TrackOption {
  /** What comes back from `v-model` when this one is chosen. */
  value: string
  label: string
  /** An SVG path drawn at 16×16, as the board draws it. */
  icon?: string
  /**
   * The key that picks it, as the developer sees it on the control — `Y`. Matched
   * case-insensitively against `event.key`, so Shift is not required.
   */
  key?: string
  /** A number after the label, as the repository filter has. */
  count?: number | null
  /** In `act` mode, draw this one as the lifted item. Ignored in `choose` mode. */
  emphasis?: boolean
}

const props = withDefaults(
  defineProps<{
    options: TrackOption[]
    /** What the group is for, for a screen reader: "How to decide on F1". */
    label: string
    disabled?: boolean
    mode?: 'choose' | 'act'
  }>(),
  { disabled: false, mode: 'choose' },
)

/** In `act` mode, which option was pressed. In `choose` mode, use `v-model`. */
const emit = defineEmits<{ act: [value: string] }>()

/*
 * Not `required`, because `act` mode has no state to bind. A caller in `choose` mode
 * passes `v-model` and gets the default only if it forgets to.
 */
const picked = defineModel<string>({ default: '' })

/*
 * `radiogroup`, not a list of buttons. One of these is chosen and choosing another
 * unchooses it, which is what a radio group is — and it buys the arrow keys for nothing.
 */
function choose(value: string) {
  if (props.disabled) return
  if (props.mode === 'act') {
    emit('act', value)
    return
  }
  picked.value = value
}

/** Whether a keystroke was meant for this control, or for something being typed into. */
function isForUs(event: KeyboardEvent): boolean {
  if (props.disabled) return false
  if (event.metaKey || event.ctrlKey || event.altKey) return false
  if (event.isComposing) return false

  const target = event.target as HTMLElement | null
  if (!target) return true
  if (target.isContentEditable) return false
  const tag = target.tagName
  return tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT'
}

function onKey(event: KeyboardEvent) {
  if (!isForUs(event)) return
  const hit = props.options.find(
    option => option.key != null && option.key.toLowerCase() === event.key.toLowerCase(),
  )
  if (!hit) return
  /*
   * Only once it is ours. Claiming the keystroke before knowing that would eat a key some
   * other part of the page, or the browser, has a use for.
   */
  event.preventDefault()
  choose(hit.value)
}

onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div
    class="track"
    :role="mode === 'choose' ? 'radiogroup' : 'group'"
    :aria-label="label"
  >
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      :role="mode === 'choose' ? 'radio' : undefined"
      class="item"
      :class="{
        'item--picked': mode === 'choose' ? option.value === picked : option.emphasis,
      }"
      :aria-checked="mode === 'choose' ? option.value === picked : undefined"
      :disabled="disabled"
      @click="choose(option.value)"
    >
      <svg
        v-if="option.icon"
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.6"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      ><path :d="option.icon" /></svg>
      <span>{{ option.label }}</span>
      <span
        v-if="option.count != null"
        class="count"
      >{{ option.count }}</span>
      <!--
        The key, drawn as a key. `aria-hidden`, because a screen reader reads the option's
        label and announcing "Y" after it would be a shortcut nobody using one would press
        — and the accessible name is what the label slot already says.
      -->
      <span
        v-if="option.key"
        class="key rd-mono"
        aria-hidden="true"
      >{{ option.key }}</span>
    </button>
  </div>
</template>

<style scoped>
.track {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-self: flex-start;
  max-width: 100%;
  padding: 4px;
  border-radius: var(--rd-radius);
  background: var(--rd-rail);
}

.item {
  display: flex;
  align-items: center;
  gap: 9px;
  min-height: var(--rd-control-item);
  padding: 0 16px;
  border: 0;
  border-radius: var(--rd-radius-sm);
  background: transparent;
  color: var(--rd-ink-dim);
  font: inherit;
  font-weight: 500;
  cursor: pointer;
  transition:
    background-color 0.14s ease,
    box-shadow 0.14s ease,
    transform 0.08s ease,
    color 0.14s ease;
}

/* The board's icon sits closer to the left edge than the label does to the right. */
.item:has(svg) {
  padding-left: 13px;
}

.item:active:not(:disabled) {
  transform: translateY(1px);
}

.item:hover:not(:disabled):not(.item--picked) {
  color: var(--rd-ink);
}

.item--picked {
  background: var(--rd-surface);
  color: var(--rd-ink);
  font-weight: 600;
  box-shadow: var(--rd-lift-chosen);
}

.item:disabled {
  cursor: not-allowed;
  color: var(--rd-ink-muted);
}

.count {
  color: var(--rd-ink-muted);
  font-weight: 400;
}

.item--picked .count {
  color: var(--rd-ink-muted);
}

.key {
  padding: 0 5px;
  border: 1px solid var(--rd-key-line);
  border-radius: var(--rd-radius-key);
  background: var(--rd-surface);
  color: var(--rd-ink-muted);
  font-size: 11px;
}

@media (prefers-reduced-motion: reduce) {
  .item {
    transition: none;
  }
}
</style>
