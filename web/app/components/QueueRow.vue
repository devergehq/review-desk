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
 *
 * ## The two ways out, beside the row rather than inside it
 *
 * DEV-936 puts a review's pull request and its ticket on the row, so that either can be
 * opened without opening the review first. They are a **sibling** of the row's own link,
 * not a cell inside it, and the criterion is the reason: pressing one of these must not
 * open the review.
 *
 * **A nested anchor would not save you here.** `<a>` inside `<a>` is invalid HTML, and an
 * HTML *parser* closes the first when it meets the second — but this dashboard renders on
 * the client, so Vue would build the nesting through the DOM API, where nothing refuses
 * it. The browser would draw it and a click on the inner link would bubble to the outer
 * one and open the review as well, which is precisely the thing forbidden. A sibling makes
 * it true by construction; the alternative is a handler that stops the event, which is a
 * thing that can be got wrong and a thing a keyboard gets round.
 *
 * ```
 * ┌─ .queue-line ───────────────────────────────────────────────────────────┐
 * │ ┌─ .row, the link to the review ───────────────────────┐ ┌─ .out ─────┐ │
 * │ │ #412 │ Retry failed … │ Draft ready │ 4 min │ Read … │ │ GitHub     │ │
 * │ └──────────────────────────────────────────────────────┘ │ ABC-231    │ │
 * └─────────────────────────────────────────────────────────────────────────┘
 * ```
 *
 * The line under a row is on the wrapper now rather than on `.row`, so it runs the width
 * of the row *and* its links instead of stopping short of them.
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
    /**
     * Where this row's pull request is, drawn beside the row.
     *
     * Left off for a row whose own destination is already the pull request — "Your pull
     * requests" is that row, and a second link to where the row goes is noise.
     */
    prHref?: string | null
    /**
     * Where this row's ticket is, and the id to draw as the link's text.
     *
     * **Both or neither.** An id with no address is drawn by the header as text and is
     * drawn here not at all: this is a row, and the meta line beside it already names the
     * ticket. An address with no id has nothing to call itself but the address, and a link
     * whose text is a URL is a link a reader has to parse.
     */
    ticketHref?: string | null
    ticketId?: string | null
  }>(),
  {
    meta: null,
    to: null,
    href: null,
    actionTone: 'quiet',
    age: null,
    ageUnknownBecause: 'the inbox carries no time for this kind of item',
    emphasis: false,
    prHref: null,
    ticketHref: null,
    ticketId: null,
  },
)
</script>

<template>
  <div
    class="queue-line"
    :class="{ 'queue-line--emphasis': emphasis }"
  >
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

    <!--
      The two ways out, outside the row's own link. Each says where it goes and that it
      opens elsewhere, because neither is something a reader can see coming.
    -->
    <span
      v-if="prHref || (ticketHref && ticketId)"
      class="out"
    >
      <a
        v-if="prHref"
        class="out-link"
        :href="prHref"
        target="_blank"
        rel="noopener noreferrer"
      >GitHub<span class="rd-visually-hidden">: pull request #{{ pr }}, opens in a new tab</span></a>
      <a
        v-if="ticketHref && ticketId"
        class="out-link"
        :href="ticketHref"
        target="_blank"
        rel="noopener noreferrer"
      >{{ ticketId }}<span class="rd-visually-hidden">, on the tracker, opens in a new tab</span></a>
    </span>
  </div>
</template>

<style scoped>
/*
 * **The row and its two links, on one line with one rule under it.** The rule is here and
 * not on `.row` so that it runs under the links as well; before DEV-936 there was nothing
 * beside the row for it to stop short of.
 *
 * **`queue-line` rather than `line`**, which is what it was called for one commit — and the
 * reason generalises past this component: **a class name shared between components is a
 * silent measurement bug the moment a test selects it unscoped.** Scoped CSS makes sharing
 * a name safe for *styling*, which is what makes the trap easy to walk into; a test has no
 * scope. `ReasonPrompt` and `SignOffCard` each have a `.line`, and so does every line of a
 * code hunk — while the two tests that measure this wrapper hand `.line` to a page-wide
 * overflow check and map every `.line` on the page to its `.row`. So the first queue row to
 * share a screen with a code hunk would have had the overflow assertion measuring code
 * lines, and **passing**, which is the whole danger. Nothing else here catches that: not
 * the type checker, not the linter, not the end-to-end suite, because the suite would have
 * been green about the wrong element.
 *
 * **`flex-start`, not `center`.** The row's cells and the links both carry the same 12 or
 * 16 pixels of padding so that the two columns begin at the same height; centred, a short
 * links cell floats against the middle of a row whose title has wrapped, and the padding
 * was doing nothing.
 */
.queue-line {
  display: flex;
  flex-wrap: wrap;
  gap: 0 20px;
  align-items: flex-start;
  border-bottom: 1px solid var(--rd-line);
}

.row {
  display: flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  gap: 6px 20px;
  align-items: center;
  padding: 12px 0;
  min-width: 0;
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

/*
 * **A repository name with nothing in it to break on, broken anyway.**
 *
 * DEV-936's measurement found this: `devergehq/reviewdeskdashboardandcontractfixtures` is
 * 48 characters with no hyphen or space in the name part, so there is no break opportunity
 * in it — and the row's meta line then set a min-content width wider than a 390-pixel
 * screen: the rows then overflowed their own section by 12 pixels.
 * `e2e/review-desk.spec.ts` has asserted that nothing scrolls sideways at a phone since
 * DEV-876, and it was true of every name the seed had — all of them hyphenated, all of
 * them breakable.
 *
 * **`anywhere` rather than `break-word`, and the difference is the one that matters here.**
 * Both break a word that cannot fit a line; only `anywhere` counts the break when the box's
 * **min-content** width is computed, which is the number a flex item is shrunk against. With
 * `break-word` the span stays as wide as its longest token and pushes the line out; with
 * `anywhere` it shrinks and the token wraps. Measured in `e2e/header-links.spec.ts`'s own
 * terms: with `break-word`, `section.group` holds 370 pixels of content in 358; with
 * `anywhere` it holds 358.
 *
 * It predates this change — nothing about the two links causes it, and at 390 they are on a
 * line of their own — and is fixed here because the measurement that found it is here.
 */
.title,
.about .rd-meta {
  overflow-wrap: anywhere;
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
 * The two links, stacked at the end of the line and the same width down the page so the
 * column reads as a column. Underlined rather than accent-coloured alone: at 13px beside a
 * row of grey meta text, colour is the cue a reader misses.
 */
.out {
  display: flex;
  flex: 0 0 96px;
  flex-direction: column;
  gap: 2px;
  align-items: flex-start;
  font-size: 13px;
  padding: 12px 0;
}

/* The same air the needs-you row gives itself, so the two cells line up vertically. */
.queue-line--emphasis .out {
  padding: 16px 0;
}

.out-link {
  max-width: 100%;
  text-decoration: underline;
  text-underline-offset: 2px;
  overflow-wrap: anywhere;
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

  /*
   * At a phone's width the links are the last line of the row rather than a column at its
   * end, and they sit side by side there: two stacked lines of one word each is three
   * lines of furniture under one row.
   */
  /*
   * **`wrap`, so the row gap can apply.** Without it the two links could not fall onto a
   * second line — only shrink and break mid-word through `overflow-wrap: anywhere` — so a
   * long ticket id beside "GitHub" read as two broken stacks rather than the two words
   * this is meant to be.
   */
  .out {
    flex: 1 1 100%;
    flex-wrap: wrap;
    flex-direction: row;
    gap: 4px 12px;
    padding: 0 0 12px;
  }

  .queue-line--emphasis .out {
    padding: 0 0 16px;
  }
}
</style>
