<script setup lang="ts">
/** A rubric, its count, and the list under it. Every group on the desk is one of these. */
defineProps<{
  heading: string
  /** Appended to the heading as "· 2". Left off when there is nothing to count. */
  count?: number | null
}>()
</script>

<template>
  <section class="group">
    <h2 class="rd-group-heading">
      {{ heading }}<template v-if="count != null">
        · {{ count }}
      </template>
    </h2>
    <slot />
  </section>
</template>

<style scoped>
.group {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
</style>
