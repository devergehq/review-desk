<script setup lang="ts">
/**
 * A card that opens over the page, anchored to the control that opened it.
 *
 * **The mechanism DEV-917 is really about: one line in place, and the rest on a card that
 * floats.** A step of the review trail is drawn in a column about 150 pixels wide. The
 * sentences a session writes about it do not fit, and the two answers that do not work are
 * both already known: printing the whole thing pushed the page's headline half a screen
 * down, and opening it in place moved everything below it the moment a reader asked for it.
 * A card that overlays the page moves nothing at all.
 *
 * ```
 *  ┌──────────┐
 *  │ 3 Altern…│  the step: a name and one line, whatever the line's length
 *  │ 9 options│◀─ activating this opens the card
 *  └──────────┘
 *  ╔═══════════════════════════════╗
 *  ║ Alternatives          3 min   ║  the card, over whatever is under it
 *  ║ On the ticket    4            ║
 *  ║ From the pass    7            ║
 *  ╚═══════════════════════════════╝
 * ```
 *
 * **This component owns the floating and the keyboard, and nothing else.** What is on the
 * card is the caller's slot; where it is anchored is one prop. It is built as its own
 * component because it is the reusable part of this change — the trail is its one use
 * today, and reusing it anywhere else is a later decision, which is why it carries no
 * knowledge of a step, a layer or a review.
 *
 * ## What "moves nothing" means in CSS, exactly
 *
 * `position: absolute` inside the anchor the caller positions, so the card is out of flow
 * and no sibling is laid out around it. The caller gives the anchor `position: relative`;
 * this component asserts nothing about that, because a card that silently positioned
 * itself against the viewport when a caller forgot would be worse than one that is
 * obviously in the wrong place.
 *
 * **It cannot be `position: fixed`.** The trail scrolls with the page, and a fixed card
 * would come unstuck from the step it belongs to the moment the page moved.
 *
 * ## The keyboard, which is the half that is easy to leave out
 *
 * | Act | What happens |
 * |---|---|
 * | click, Enter, Space on the control | opens, because the control is a `<button>` |
 * | Escape | closes, and focus returns to the control |
 * | a click anywhere outside | closes |
 * | hover | opens, and leaving closes — unless a click pinned it |
 *
 * Hover alone is not enough and never was: it is unreachable by keyboard and meaningless
 * on a phone. It is here *as well*, because the board opens on hover and a reader
 * sweeping the trail with a mouse should not have to click seven times.
 *
 * **Focus returns to the control, not to the page.** Escape with the focus moved nowhere
 * would leave a keyboard reader at the top of the document, which is the trail's whole
 * length away from where they were.
 */
const props = withDefaults(
  defineProps<{
    /** Whether the card is open. The caller owns the state: one card at a time is its rule. */
    open: boolean
    /**
     * Which edge of the anchor the card hangs from.
     *
     * The last steps of a trail are at the right of the page, and a card hanging left from
     * one of those is a card half off the screen.
     */
    side?: 'left' | 'right'
    /** Named for a screen reader, since what is on it is the caller's. */
    label: string
  }>(),
  { side: 'left' },
)

const emit = defineEmits<{
  /** Asked to close: Escape, or a click outside. The caller sets `open` to false. */
  close: []
  /** Escape specifically, so the caller can return focus to the control it came from. */
  escape: []
}>()

const card = ref<HTMLElement | null>(null)

/**
 * A click outside closes it.
 *
 * **On `pointerdown`, not `click`.** A `click` listener on the document fires after the
 * control's own click has already toggled the state, so opening the card with the mouse
 * closed it again in the same gesture. `pointerdown` on the control happens before that
 * toggle, and the check below is what stops it closing a card the reader is reaching into.
 */
function onPointerDown(event: Event) {
  const target = event.target as Node | null
  if (!props.open || !target) return
  /*
   * Inside the card, or inside the anchor that owns it — which is the control. Closing on
   * the control's own press would fight the caller's toggle and leave the card flickering.
   */
  if (card.value?.contains(target)) return
  if (card.value?.parentElement?.contains(target)) return
  emit('close')
}

/*
 * **Every mounted card listens, and only the open one acts.** A trail has seven of these
 * and one can be open, so six of the seven return on the first line. There is no
 * `stopPropagation` here and it would buy nothing if there were: the listener is on the
 * window, which is where propagation ends, and stopping it there does not keep the other
 * six from being called.
 */
function onKey(event: KeyboardEvent) {
  if (!props.open || event.key !== 'Escape') return
  emit('escape')
  emit('close')
}

onMounted(() => {
  window.addEventListener('keydown', onKey)
  window.addEventListener('pointerdown', onPointerDown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  window.removeEventListener('pointerdown', onPointerDown)
})
</script>

<template>
  <!--
    `role="group"` and a name, rather than `dialog`. It is a card of facts beside the thing
    it is about, not something to be dismissed before the page can be used again: a dialog
    would promise a modal that traps focus, and trapping focus on a hover-openable card
    would be a trap a mouse could spring by accident.
  -->
  <div
    v-if="open"
    ref="card"
    class="floating"
    :class="`floating--${side}`"
    role="group"
    :aria-label="label"
  >
    <slot />
  </div>
</template>

<style scoped>
.floating {
  position: absolute;
  top: calc(100% + 6px);

  /*
   * Over everything near it, and the one layer this app uses for a thing that floats. The
   * page itself stacks nothing above 2.
   */
  z-index: 5;
  width: 330px;

  /*
   * **The width at a phone's width, which is what stops this fix causing a worse one.**
   * 330px inside a 390px viewport leaves 60px, and a card hanging from the right-hand
   * column would be off the edge — horizontal scroll on the device with the least room to
   * spare. `min()` keeps it inside the page; the two 16px gutters are the shell's own
   * padding.
   */
  max-width: min(330px, calc(100vw - 32px));
  padding: 14px 16px;
  border: 1px solid var(--rd-line-strong);
  border-radius: var(--rd-radius-control);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-raised);
  text-align: left;

  /* Inherited from the step it hangs off, which is 13px and grey. The card is neither. */
  color: var(--rd-ink);
  font-size: 14px;
}

.floating--left {
  left: 0;
}

.floating--right {
  right: 0;
}

/*
 * **Below 680px the card sits over its own anchor, and the arithmetic is why.**
 *
 * The trail is two columns under 860px, so a step in the right-hand column starts at about
 * half the page. A 330px card hanging left from there needs half the viewport plus 330 —
 * which is more than the viewport itself at anything under about 660px, and a page 529px
 * wide inside a 390px screen is the horizontal scroll this whole change is against. Hanging
 * it the other way is worse: the card would run off the left edge instead, where nothing
 * can scroll to reach it.
 *
 * So at that width it takes the anchor's own width. It is narrower than the board draws it
 * — the board was drawn at 1440 — and a narrow card that is wholly on screen beats a wide
 * one that is half off it. Callers whose contents need it say so in their own stylesheet:
 * the trail stacks each fact's label over its value here.
 */
@media (max-width: 680px) {
  .floating,
  .floating--left,
  .floating--right {
    left: 0;
    right: auto;
    width: 100%;
    max-width: 100%;
  }
}
</style>
