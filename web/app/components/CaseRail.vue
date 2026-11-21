<script setup lang="ts">
/**
 * **On this tab** — the Understanding tab's own contents, down its left. S3, DEV-1327.
 *
 * One link per band, to the band's id, in the order the page draws them; the list is
 * `CASE_BANDS`, so the rail and the headings cannot name a band two ways. The band the
 * address points at is marked `aria-current="location"` — a link from Overview or an old
 * Alternatives address arrives with its band already marked.
 *
 * **Drawn only where there is room for it.** The case's own container decides, not the
 * window: with the sidebar and the conversation either side, the main column at a 1,280
 * window is narrower than at a 1,024 one without them. Under that width the rail is not
 * drawn at all — the bands are one column and each heading is still its own anchor.
 */
const { bands } = defineProps<{
  bands: ReadonlyArray<{ id: string, label: string }>
}>()

const route = useRoute()
</script>

<template>
  <nav
    class="rail"
    aria-label="On this tab"
  >
    <p class="rd-rubric rubric">
      On this tab
    </p>
    <ul class="links">
      <li
        v-for="band in bands"
        :key="band.id"
      >
        <a
          :href="`#${band.id}`"
          class="link"
          :aria-current="route.hash === `#${band.id}` ? 'location' : undefined"
        >{{ band.label }}</a>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.rail {
  position: sticky;
  top: 24px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.rubric {
  font-size: var(--rd-type-label);
  font-weight: 600;
  padding: 0 14px;
}

.links {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.link {
  display: block;
  padding: 7px 14px;
  border-left: 2px solid transparent;
  border-radius: 0 var(--rd-radius-xs) var(--rd-radius-xs) 0;
  font-size: var(--rd-type-body);
  line-height: 1.4;
  color: var(--rd-ink-dim);
  text-decoration: none;
}

.link:hover {
  background: var(--rd-ground);
  color: var(--rd-ink);
}

.link[aria-current='location'] {
  border-left-color: var(--rd-accent);
  background: var(--rd-ground);
  font-weight: 600;
  color: var(--rd-ink);
}

.link:focus-visible {
  outline: none;
  box-shadow: var(--rd-focus);
}
</style>
