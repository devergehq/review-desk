<script setup lang="ts">
/**
 * One review in the queue: **one row, whatever it is waiting for.**
 *
 * That is DEV-876's first rule and the defect it is fixing. The Inbox served five arrays
 * and the screen drew one group per array, so a review whose session had written a draft
 * before the developer confirmed its brief appeared twice — and the one in the seeded
 * database appeared three times, because the author had replied to it as well. A developer
 * reading that cannot tell three things to do from one thing to do three times.
 *
 * Five cells, at the board's own widths:
 *
 * ```
 * #412 │ Retry failed webhook …  │ Draft ready            │ 4 min │ Read and sign off
 *      │ acme/portal · round 1   │ 1 blocker, 1 should    │       │
 * ```
 *
 * The third is what the review is waiting for, and it is a slot: the needs-you groups put
 * the next action there, "With the agent" puts the layer bar, and the quieter two put one
 * sentence. The widths stay the same down the page so the columns line up across groups.
 *
 * **The whole row is the link**, not a link inside it, so the thing you click is the thing
 * you read. At 390px the cells wrap onto their own lines in the order they are read.
 */
/*
 * **`NuxtLink` resolved to the component, not named as a string.** Measured in the built
 * binary: `:is="to ? 'NuxtLink' : …"` resolved the string to nothing in the static build,
 * the row came out as an unknown element, and the whole row stopped being a link — which
 * the end-to-end run found by waiting sixty seconds for a link that was not there. A
 * string is fine for `a` and `div`, which are real tags; a component has to be the
 * component.
 */
const NuxtLink = resolveComponent('NuxtLink')

withDefaults(
  defineProps<{
    pr: number
    title: string
    /** The line under the title: the repository, the round, what changed. */
    meta?: string | null
    /** Where the row goes. A review on the desk is internal; a pull request is not. */
    to?: string | null
    href?: string | null
    /** The right-hand label. The board writes it as a verb: "Read and sign off". */
    action: string
    /** `go` is the accent, `quiet` is grey, `warn` is the brown that wants looking at. */
    actionTone?: 'go' | 'quiet' | 'warn'
    /** The age, already worded. `null` draws "not recorded": the API has no time for it. */
    age?: string | null
    /** Why there is no age, for the unknown state. */
    ageUnknownBecause?: string
    /** Needs-you rows set the title in display type and give the row more air. */
    emphasis?: boolean
  }>(),
  {
    meta: null,
    to: null,
    href: null,
    actionTone: 'quiet',
    age: null,
    ageUnknownBecause: 'the inbox carries no time for this kind of item',
    emphasis: false,
  },
)
</script>

<template>
  <component
    :is="to ? NuxtLink : href ? 'a' : 'div'"
    :to="to ?? undefined"
    :href="href ?? undefined"
    :target="href ? '_blank' : undefined"
    :rel="href ? 'noreferrer noopener' : undefined"
    class="row"
    :class="{ 'row--emphasis': emphasis }"
  >
    <span class="pr rd-mono">#{{ pr }}</span>

    <span class="about">
      <span :class="emphasis ? 'rd-display-row title' : 'title'">{{ title }}</span>
      <span
        v-if="meta"
        class="rd-meta"
      >{{ meta }}</span>
    </span>

    <span class="what">
      <slot />
    </span>

    <span class="age rd-meta">
      <template v-if="age">{{ age }}</template>
      <NotMeasured
        v-else
        :because="ageUnknownBecause"
        says="no time recorded"
      />
    </span>

    <span
      class="action"
      :class="`action--${actionTone}`"
    >
      {{ action }}
      <span
        v-if="href"
        class="rd-visually-hidden"
      >, opens in a new tab</span>
    </span>
  </component>
</template>

<style scoped>
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 20px;
  align-items: center;
  padding: 12px 0;
  border-bottom: 1px solid var(--rd-line);
  color: var(--rd-ink);
  text-decoration: none;
}

/* The needs-you rows carry more air, which is the board's own difference between them. */
.row--emphasis {
  padding: 16px 0;
}

.row:hover {
  color: var(--rd-ink);
}

/*
 * A row is a link, so it gets the page's focus ring — but the ring has to go round the
 * whole row rather than round the text, which is what the offset is doing here.
 */
.row:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: -1px;
  border-radius: var(--rd-radius-xs);
}

.pr {
  flex: 0 0 56px;
  color: var(--rd-ink-muted);
}

.about {
  display: block;
  flex: 3 1 300px;
  min-width: 0;
}

.title {
  display: block;
}

/* A quieter row's title is the body face, and its meta sits under it just the same. */
.row:not(.row--emphasis) .about .rd-meta {
  display: inline;
}

.row--emphasis .about .rd-meta {
  display: block;
}

.what {
  display: flex;
  flex: 2 1 240px;
  flex-wrap: wrap;
  gap: 2px 10px;
  align-items: center;
  min-width: 0;
}

.age {
  flex: 0 0 90px;
}

.action {
  flex: 0 0 150px;
  text-align: right;
  font-size: 13px;
}

.action--go {
  color: var(--rd-accent);
  font-size: 15px;
  font-weight: 600;
}

.row:hover .action--go {
  color: var(--rd-accent-dark);
  text-decoration: underline;
}

.action--quiet {
  color: var(--rd-ink-muted);
}

.action--warn {
  color: var(--rd-warn-ink);
  font-weight: 600;
}

/*
 * At a phone's width the five cells become five lines, and the right-aligned action would
 * read as belonging to the row below. Left-aligned, with the age beside it.
 */
@media (max-width: 560px) {
  .row {
    gap: 4px 12px;
  }

  .pr,
  .age,
  .action {
    flex: 0 0 auto;
  }

  .action {
    text-align: left;
  }
}
</style>
