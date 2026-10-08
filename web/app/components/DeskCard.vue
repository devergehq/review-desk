<script setup lang="ts">
/** The white surface everything on the desk sits on. One place for the border. */
withDefaults(
  defineProps<{
    /** `chosen` rings the card in the accent; `pending` dashes its border. */
    tone?: 'plain' | 'chosen' | 'pending'
    /** The element to render. A list item inside a list, a section on its own. */
    as?: string
    pad?: 'normal' | 'tight'
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
  background: var(--rd-surface);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius);
}

.card--pad-normal {
  padding: 16px;
}

.card--pad-tight {
  padding: 14px 16px;
}

.card--chosen {
  border: 2px solid var(--rd-accent);
  padding: 15px;
}

.card--pending {
  border-style: dashed;
  border-color: var(--rd-line-strong);
}
</style>
