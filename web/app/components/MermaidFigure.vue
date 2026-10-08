<script setup lang="ts">
/**
 * One mermaid diagram. A browser was chosen as the renderer for Review Desk so that a
 * brief's diagram and a draft's diagram are drawn rather than described, so this is the
 * component that earns that choice.
 *
 * mermaid is loaded on mount and never during the build: the app is generated static and
 * the diagram source is a record, not a template. If mermaid cannot draw it, the source
 * is shown instead — a diagram that fails is still readable.
 */
const props = defineProps<{ source: string, label?: string }>()

const svg = ref('')
const failed = ref(false)
const host = useId()

onMounted(async () => {
  try {
    const mermaid = (await import('mermaid')).default
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'base',
      fontFamily: '\'IBM Plex Sans\', system-ui, sans-serif',
      themeVariables: {
        primaryColor: '#ffffff',
        primaryBorderColor: '#d9dce2',
        primaryTextColor: '#16181d',
        lineColor: '#565b66',
        fontSize: '14px',
      },
    })
    const rendered = await mermaid.render(`mermaid-${host}`, props.source)
    svg.value = rendered.svg
  }
  catch {
    failed.value = true
  }
})
</script>

<template>
  <figure class="figure">
    <!--
      mermaid's own output, from the diagram source in the record. mermaid runs with
      `securityLevel: 'strict'`, which is what makes inserting its SVG safe.
    -->
    <!-- eslint-disable vue/no-v-html -->
    <div
      v-if="svg"
      class="canvas"
      :aria-label="label"
      v-html="svg"
    />
    <!-- eslint-enable vue/no-v-html -->
    <pre
      v-else
      class="fallback"
    ><code>{{ source }}</code></pre>
    <figcaption
      v-if="failed"
      class="rd-meta"
    >
      This diagram could not be drawn. Its source is above.
    </figcaption>
  </figure>
</template>

<style scoped>
.figure {
  margin: 0;
}

/* A wide diagram scrolls inside its own frame; the page never scrolls sideways. */
.canvas {
  overflow-x: auto;
  max-width: 100%;
}

.canvas :deep(svg) {
  max-width: 100%;
  height: auto;
}

.fallback {
  margin: 0;
  padding: 12px;
  overflow-x: auto;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  font-family: var(--rd-font-mono);
  font-size: 13px;
}
</style>
