<script setup lang="ts">
/**
 * A surface something sits on, in the four the boards draw.
 *
 * | Tone | Drawn | Where |
 * |---|---|---|
 * | `plain` | white, a 1px shadow | a line of facts, a callout |
 * | `well` | `#F6F7F6`, 20px radius, no shadow | a section of a board |
 * | `raised` | white, 18px radius, a long soft shadow | a card that is the point of its column |
 * | `chosen` | white, a 1.5px accent ring and a green shadow | a rule proposal waiting on the developer |
 * | `pending` | a dashed edge | something not settled |
 *
 * **The boards tell a card from the page by shadow, not by a border**, because the page is
 * white and so is the card. That is the one structural change from the first dashboard,
 * where the page was grey and a 1px line was enough.
 */
withDefaults(
  defineProps<{
    tone?: 'plain' | 'well' | 'raised' | 'chosen' | 'pending'
    /** The element to render. A list item inside a list, a section on its own. */
    as?: string
    pad?: 'normal' | 'tight' | 'section'
  }>(),
  { tone: 'plain', as: 'div', pad: 'normal' },
)
</script>

<template>
  <component
    :is="as"
    class="card"
    :class="[`card--${tone}`, `card--pad-${pad}`]"
  >
    <slot />
  </component>
</template>

<style scoped>
.card {
  border-radius: var(--rd-radius);
  background: var(--rd-surface);
  box-shadow: var(--rd-lift-card);
}

.card--pad-normal {
  padding: 16px;
}

.card--pad-tight {
  padding: 14px 16px;
}

/* A section of a board: 32px all round, which is what the Controls board's wells have. */
.card--pad-section {
  padding: 32px;
}

.card--well {
  border-radius: var(--rd-radius-section);
  background: var(--rd-ground);
  box-shadow: none;
}

.card--raised {
  border-radius: var(--rd-radius-card);
  box-shadow: var(--rd-lift-raised);
}

.card--chosen {
  border-radius: var(--rd-radius-section);
  box-shadow: var(--rd-ring-proposal);
}

.card--pending {
  border: 1px dashed var(--rd-line-strong);
  box-shadow: none;
}
</style>
