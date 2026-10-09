<script setup lang="ts">
/**
 * A button, in the four kinds and the five states the Controls board draws.
 *
 * | Kind | The board calls it | Where it goes |
 * |---|---|---|
 * | `primary` | "The one action" | One per card: sign off, make it a rule |
 * | `secondary` | "Everything else" | Send back, retire, dismiss. The default. |
 * | `ink` | the control inside a field | Start |
 * | `quiet` | — | A tab, or an item in a track that draws its own chosen state |
 *
 * The class names are the stylesheet's and were not renamed to the boards' words: the two
 * screens this issue does not rebuild are already written against `rd-btn--primary`, and a
 * rename would have been a change to them for no gain.
 *
 * **Resting, the pointer over it, and pressed are CSS** — they are facts about the pointer
 * and belong in `tokens.css` with the rest of the vocabulary. **Working and not available
 * are props**, because they are facts about the record: a write is in flight, or the record
 * is in a state where this cannot be done. The board draws all five and so does this.
 *
 * `working` implies unavailable. A button that is already doing the thing must not be
 * pressable again — that is a second `POST`, and on this app a second sign-off.
 */
withDefaults(
  defineProps<{
    kind?: 'primary' | 'secondary' | 'ink' | 'quiet'
    /** A write is in flight. Shows the spinner and the working label, and refuses a press. */
    working?: boolean
    /** What it says while it is working. The board's own example is "Signing off". */
    workingLabel?: string
    /** The record is not in a state where this can be done. */
    disabled?: boolean
    /** The arrow the board puts on an action that sends something somewhere. */
    arrow?: boolean
    /** A link rather than a button, for an action that is a navigation. */
    to?: string | null
    /** Rendered as `<a href>`, for GitHub — which Review Desk only ever reads. */
    href?: string | null
  }>(),
  {
    kind: 'secondary',
    working: false,
    workingLabel: '',
    disabled: false,
    arrow: false,
    to: null,
    href: null,
  },
)
</script>

<template>
  <!--
    A link when it navigates, a button when it acts. Never a div with a click handler: Tab
    skips those, which is the whole of DEV-876's "controls are real buttons, links and
    labelled inputs, reachable by keyboard".
  -->
  <NuxtLink
    v-if="to"
    :to="to"
    class="rd-btn"
    :class="kind === 'secondary' ? undefined : `rd-btn--${kind}`"
  >
    <slot />
    <svg
      v-if="arrow"
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
  </NuxtLink>

  <a
    v-else-if="href"
    :href="href"
    class="rd-btn"
    :class="kind === 'secondary' ? undefined : `rd-btn--${kind}`"
    target="_blank"
    rel="noreferrer noopener"
  >
    <slot />
  </a>

  <button
    v-else
    type="button"
    class="rd-btn"
    :class="[kind === 'secondary' ? '' : `rd-btn--${kind}`, { 'is-working': working }]"
    :disabled="disabled || working"
    :aria-busy="working ? 'true' : undefined"
  >
    <span
      v-if="working"
      class="spinner"
      aria-hidden="true"
    />
    <template v-if="working && workingLabel">
      {{ workingLabel }}
    </template>
    <slot v-else />
    <svg
      v-if="arrow && !working"
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
</template>

<style scoped>
/*
 * The one state with no resting equivalent in the stylesheet: the board draws a ring with
 * one quarter of it in the foreground colour, turning.
 */
.spinner {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid var(--rd-line-strong);
  border-top-color: var(--rd-ink);
  box-sizing: border-box;
  animation: spin 0.7s linear infinite;
}

.rd-btn--primary .spinner,
.rd-btn--ink .spinner {
  border-color: rgb(255 255 255 / 40%);
  border-top-color: #ffffff;
}

/* A working button keeps its resting fill rather than dimming: it is doing the thing. */
.rd-btn.is-working {
  cursor: progress;
  opacity: 1;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* A turning ring is motion. Shown still, for a reader who has asked for no animation. */
@media (prefers-reduced-motion: reduce) {
  .spinner {
    animation: none;
  }
}
</style>
