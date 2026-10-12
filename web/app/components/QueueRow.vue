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
 * ## The two ways out, in the row's own repository line
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
 * **So the meta line is what left the anchor, and the links went into it.** DEV-957:
 * "On most rows it sits alone on a line under the pull request number, and on the #9568
 * row it floats top right above the action. It belongs inline with the repository and
 * ticket line under the title." A column of links at the end of the row was the only
 * place they could go while the whole row was one anchor; moving the *meta* out instead
 * keeps the links outside the anchor and puts them where they read.
 *
 * ```
 * ┌─ .queue-line ───────────────────────────────────────────────────────────┐
 * │ ┌─ .row, the link to the review ──────────────────────────────────────┐ │
 * │ │ #412 │ Retry failed webhook …  │ Draft ready │ 4 min │ Read and si… │ │
 * │ └─────────────────────────────────────────────────────────────────────┘ │
 * │ ┌─ .row-meta, outside the anchor ─────────────────────────────────────┐ │
 * │ │        acme/portal · round 1 · GitHub · ABC-231                     │ │
 * │ └─────────────────────────────────────────────────────────────────────┘ │
 * └─────────────────────────────────────────────────────────────────────────┘
 * ```
 *
 * The line under a row is on the wrapper rather than on `.row`, so it runs the width of
 * the row *and* its meta line instead of stopping short of them.
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

const props = withDefaults(
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
     * Where this row's pull request is, drawn in the row's repository line.
     *
     * Left off for a row whose own destination is already the pull request — "Your pull
     * requests" is that row, and a second link to where the row goes is noise.
     */
    prHref?: string | null
    /**
     * Where this row's ticket is, and the id to draw with it.
     *
     * **The row names the ticket once, here, and links it when the record has an
     * address.** It used to do neither without both: an id with no address was left to the
     * caller's `meta` string, and an id *with* one was drawn twice — once in that string
     * and once as the link. That was tolerable while the two sat in different places, and
     * DEV-957 put them on one line, where "acme/portal · round 1 · ABC-231 · GitHub ·
     * ABC-231" is plainly one id too many. So the callers stopped putting it in `meta` and
     * this draws it, which is the rule being true by construction rather than by three
     * pages agreeing.
     *
     * An address with no id is drawn not at all: a link whose text is a URL is a link a
     * reader has to parse.
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

/**
 * The ticket's address, refused here rather than by each caller.
 *
 * **The rule is that a stored address which is not `https` is never rendered as a link**,
 * and `contract/README.md`, `app/utils/links.ts` and migration 16's own comment all state
 * it. It was true of every call site — all four passed `ticketUrl(…)` — and not of this
 * component, which used what it was given. `ReviewStepPage` already guards internally, so
 * the two renderers disagreed about where the rule lives, and a fifth row call site passing
 * `review.ticket_url` straight through would have put a hand-written `javascript:` value in
 * an `href` with nothing failing. `ticketUrl` is idempotent on an address it has already
 * accepted, so guarding here costs nothing and makes the rule true by construction.
 */
const ticket = computed(() => ticketUrl(props.ticketHref))
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
      **The repository line, outside the row's own link**, with the two ways out inline in
      it. The whole line is a sibling of the anchor rather than a cell inside it, which is
      what lets the links be anchors at all — see the note at the top of this file.

      Drawn at all only when there is something to put in it: a row with no meta, no pull
      request link and no ticket draws no empty line.
    -->
    <span
      v-if="meta || prHref || ticketId"
      class="row-meta"
    >
      <span
        v-if="meta"
        class="rd-meta meta-text"
      >{{ meta }}</span>
      <!--
        The separators are `aria-hidden`: they are the line's punctuation, and a screen
        reader announcing "middle dot" between a repository and a link reads worse than the
        pause it already gives. The meta text's own dots are inside one string and are read
        as punctuation; these are elements, and an element gets announced.
      -->
      <span
        v-if="prHref || ticketId"
        class="row-out"
      >
        <template v-if="prHref">
          <span
            v-if="meta"
            class="sep"
            aria-hidden="true"
          >·</span>
          <a
            class="row-out-link"
            :href="prHref"
            target="_blank"
            rel="noopener noreferrer"
          >GitHub<span class="rd-visually-hidden">: pull request #{{ pr }}, opens in a new tab</span></a>
        </template>
        <template v-if="ticketId">
          <span
            v-if="meta || prHref"
            class="sep"
            aria-hidden="true"
          >·</span>
          <!--
            Linked when the record holds an address, and named as text when it does not.
            The contract's own words for the second case: "a review with an id and no
            address, which the row then names on its own line and does not link".
          -->
          <a
            v-if="ticket"
            class="row-out-link"
            :href="ticket"
            target="_blank"
            rel="noopener noreferrer"
          >{{ ticketId }}<span class="rd-visually-hidden">, on the tracker, opens in a new tab</span></a>
          <span
            v-else
            class="rd-meta"
          >{{ ticketId }}</span>
        </template>
      </span>
    </span>
  </div>
</template>

<style scoped>
/*
 * **The row and its repository line, with one rule under both.** The rule is here and not
 * on `.row` so that it runs under the meta line as well; before DEV-936 there was nothing
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
 * **The wrapper owns the row's padding now, and that is the change DEV-957 made.** It was
 * on `.row` and on `.row-out`, twice, so that a row and the links beside it began at the
 * same height. There is nothing beside the row any more — the meta line is *under* it — so
 * one padding on the wrapper is the whole of it, and a row whose meta line is absent keeps
 * the same height as one that has it.
 */
.queue-line {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 20px;
  align-items: flex-start;
  padding: 10px 0;
  border-bottom: 1px solid var(--rd-line);
}

/* The needs-you rows carry more air, which is the board's own difference between them. */
.queue-line--emphasis {
  padding: 14px 0;
}

/*
 * **The full width, so the meta line falls under it rather than beside it.** `1 1 auto`
 * let the links cell sit at the row's end, which is where DEV-957 found the GitHub link
 * "floating top right above the action".
 *
 * **`min-height` is the touch target, and it is here rather than implied by padding.**
 * `--rd-control-min` is this project's floor for anything you press — "Nothing you can
 * press is shorter than the first one", says the token's own comment — and
 * `e2e/review-desk.spec.ts` measures every row against 44 at 1440 and at 390. It
 * used to come out of the row's own 12 or 16 pixels of padding plus two lines of text; the
 * padding moved to the wrapper and the second line left the anchor, and a one-line row
 * measured 42.6 — found by that measurement, which is what it is for. Stated as the floor
 * it is, so the next change to this box cannot take it away by arithmetic.
 */
.row {
  display: flex;
  flex: 1 1 100%;
  flex-wrap: wrap;
  gap: 6px 20px;
  align-items: center;
  min-height: var(--rd-control-min);
  min-width: 0;
  color: var(--rd-ink);
  text-decoration: none;
}

.row:hover {
  color: var(--rd-ink);
}

/*
 * A row is a link, so it gets the page's focus ring — but the ring has to go round the
 * whole row rather than round the text, which is what the offset is doing here.
 *
 * **Outside the row rather than one pixel in**, since the padding moved to the wrapper: a
 * negative offset drew the ring over the first and last characters of a row that no longer
 * has any padding of its own to spare.
 */
.row:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
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
.row-meta {
  overflow-wrap: anywhere;
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
 * **The repository line, under the title and aligned with it.** The indent is the `#412`
 * cell's own width plus the row's column gap, so the line begins where the title begins
 * rather than under the pull request number — which is where DEV-957 found the links,
 * "alone on a line under the pull request number".
 *
 * It wraps, and it has to: it carries the repository, the round, what changed, the ticket
 * id and two links, and a 46-character repository name is wider than a phone on its own.
 * `overflow-wrap: anywhere` above is what makes a name with no hyphen in it shrink rather
 * than push the line past the row's edge — see the note on `.title` for why `anywhere` and
 * not `break-word`.
 */
.row-meta {
  display: flex;
  flex: 1 1 100%;
  flex-wrap: wrap;
  gap: 0 6px;
  align-items: baseline;
  min-width: 0;
  padding-left: 76px;
}

/*
 * **`row-out`, not `out`.** `ReviewStepPage` calls its header *anchors* `.out` and
 * `SignOffCard` calls an anchor `.out` too — so this component's *container* sharing the
 * name made one class mean two different things across three components, which is the rule
 * the `.line` rename above states: scoped CSS makes sharing safe for styling, and a test
 * selecting it unscoped then measures whichever one it reaches first.
 *
 * The two links sit inline in the line, in reading order after what the row is about.
 * Underlined rather than accent-coloured alone: at 13px beside a row of grey meta text,
 * colour is the cue a reader misses.
 */
.row-out {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 0 6px;
  align-items: baseline;
  min-width: 0;
  font-size: 13px;
}

.sep {
  color: var(--rd-ink-muted);
}

.row-out-link {
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
   * **No indent at a phone's width.** The `#412` cell is `flex: 0 0 auto` here rather than
   * a 56-pixel column, so there is nothing for 76 pixels to line up with — and 76 pixels
   * of a 358-pixel line is a fifth of the width this line needs for a repository name, a
   * round and two links.
   */
  .row-meta {
    padding-left: 0;
  }
}
</style>
