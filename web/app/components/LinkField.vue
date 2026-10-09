<script setup lang="ts">
/**
 * A labelled field with its action inside it, in the board's three states.
 *
 * | State | The board draws | What it means here |
 * |---|---|---|
 * | Resting | a `--rd-control-line` edge, the action in ink | nothing typed, or nothing said about it |
 * | In use | an accent edge, a `#D9EFE9` halo, the action in accent | the field has focus, or something valid is in it |
 * | Refused | a `#8A4B00` edge, a `#FFE9BF` halo, the action greyed | the desk refused what was typed |
 *
 * **The states are what the record says, not what the pointer is doing.** `refused` comes
 * from the desk's own answer — a link that is not a pull request, a machine where `gh` is
 * not logged in — and the field keeps what the developer typed when it is refused, because
 * retyping something that was right is waste.
 *
 * The label is a real `<label for>`, the field is an `<input>`, and the action is a
 * `<button>`: three elements a keyboard can reach, which a board does not have to care
 * about and an app does.
 */
const props = withDefaults(
  defineProps<{
    /** Needed for `<label for>`, and so two of these on one page do not collide. */
    id: string
    label: string
    placeholder?: string
    action?: string
    /** What the desk said back. Present means refused; the field draws itself that way. */
    refusal?: string | null
    /** A line under the field when there is nothing wrong — what the link resolved to. */
    hint?: string | null
    /** A write is in flight. */
    working?: boolean
    workingLabel?: string
  }>(),
  {
    placeholder: 'Paste a pull request link',
    action: 'Start',
    refusal: null,
    hint: null,
    working: false,
    workingLabel: 'Reading…',
  },
)

const emit = defineEmits<{ submit: [] }>()

const value = defineModel<string>({ required: true })
const focused = ref(false)

/**
 * Whether the refusal is about what is in the field *now*.
 *
 * A refused field greys its action, which is the board's drawing and is right while the
 * refused text is still there. It would be wrong a keystroke later: the developer has
 * started fixing the link and the control that would send the fix is dead. So the refusal
 * goes stale the moment the text changes, and comes back when the desk refuses again.
 */
const stale = ref(false)
watch(value, () => {
  if (props.refusal) stale.value = true
})
watch(() => props.refusal, () => {
  stale.value = false
})

const refused = computed(() => props.refusal != null && !stale.value)

/** Which of the three the field is in. Refused outranks in use: it is the newer fact. */
const state = computed(() => {
  if (refused.value) return 'refused'
  if (focused.value || value.value.trim().length > 0) return 'in-use'
  return 'resting'
})

/** Nothing is sent while the thing in the field is the thing that was just refused. */
function submit() {
  if (props.working || refused.value) return
  emit('submit')
}

/** The id of whatever is explaining the field, for `aria-describedby`. */
const described = computed(() => {
  if (refused.value) return `${props.id}-refusal`
  if (props.hint) return `${props.id}-hint`
  return undefined
})
</script>

<template>
  <div class="wrap">
    <label
      class="rd-label"
      :for="id"
    >{{ label }}</label>
    <div
      class="shell"
      :class="`shell--${state}`"
    >
      <svg
        class="chain"
        width="18"
        height="18"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      ><path
        d="M6.5 9.5l3-3M7 5l1.3-1.3a2.6 2.6 0 013.7 3.7L10.7 8.7M9 11l-1.3 1.3a2.6 2.6 0 01-3.7-3.7L5.3 7.3"
      /></svg>
      <input
        :id="id"
        v-model="value"
        class="field"
        type="text"
        inputmode="url"
        :placeholder="placeholder"
        :aria-describedby="described"
        :aria-invalid="refused ? 'true' : undefined"
        @focus="focused = true"
        @blur="focused = false"
        @keyup.enter="submit"
      >
      <button
        type="button"
        class="go"
        :disabled="working || refused"
        :aria-busy="working ? 'true' : undefined"
        @click="submit"
      >
        <span
          v-if="working"
          class="spinner"
          aria-hidden="true"
        />
        {{ working ? workingLabel : action }}
      </button>
    </div>
    <p
      v-if="refused"
      :id="`${id}-refusal`"
      class="refusal"
      role="status"
    >
      {{ refusal }}
    </p>
    <p
      v-else-if="hint"
      :id="`${id}-hint`"
      class="rd-meta hint"
    >
      {{ hint }}
    </p>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.shell {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  padding: 5px 5px 5px 16px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-quiet);
}

.shell--in-use {
  border: 1.5px solid var(--rd-accent);
  box-shadow: var(--rd-halo-in-use);
}

.shell--refused {
  border: 1.5px solid var(--rd-warn-line);
  box-shadow: var(--rd-halo-refused);
}

.chain {
  flex: 0 0 auto;
  color: var(--rd-ink-muted);
}

.shell--in-use .chain {
  color: var(--rd-accent);
}

.shell--refused .chain {
  color: var(--rd-warn-line);
}

.field {
  flex: 1 1 auto;
  min-width: 0;
  min-height: var(--rd-control-min);
  border: 0;
  outline: 0;
  background: transparent;
  font: inherit;
  color: var(--rd-ink);
}

/*
 * The shell draws the focus, which is the one place this app does not use the global ring:
 * an outline on the input would be drawn inside the shell's own edge, two rings deep. The
 * shell's accent edge and halo are the focus indicator, and they clear 3:1.
 */
.field:focus-visible {
  outline: 0;
}

.go {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  flex: 0 0 auto;
  min-height: var(--rd-control-min);
  padding: 0 18px;
  border: 0;
  border-radius: var(--rd-radius-sm);
  background: var(--rd-ink);
  color: var(--rd-surface);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
  transition:
    background-color 0.14s ease,
    box-shadow 0.14s ease,
    transform 0.08s ease;
}

.go:active:not(:disabled) {
  transform: translateY(1px);
}

.shell--in-use .go {
  background: var(--rd-accent);
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 18%);
}

.shell--in-use .go:hover:not(:disabled) {
  background: var(--rd-accent-hover);
}

.go:disabled {
  cursor: not-allowed;
  background: var(--rd-rail);
  color: var(--rd-ink-muted);
  box-shadow: none;
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

.refusal {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
  color: var(--rd-warn-ink);
}

.hint {
  margin: 0;
}

@media (prefers-reduced-motion: reduce) {
  .go,
  .spinner {
    transition: none;
    animation: none;
  }
}
</style>
