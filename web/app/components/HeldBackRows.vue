<script setup lang="ts">
import type { HeldBackRow } from '~/utils/draft'

/**
 * The findings held back, as rows you open rather than paragraphs you wade through.
 *
 * ```
 *  HELD BACK · 8 FINDINGS
 *  6 nits
 *   F2   nit        The superseded accessor is still public          Open
 *   F16  nit        `allowed_columns` is read in two places          Open
 *  ───────────────────────────────────────────────────────────────────────
 *  2 questions
 *   F19  question   Whether the audit entry still fits…        Open
 *                   ╷ the claim, whole, and where it is in the code
 * ```
 *
 * ## What it is for, stated once
 *
 * **These are the findings the developer is not posting, and the panel's job is to let them
 * check that list before they sign off.** It printed each finding's key and its whole claim
 * as one paragraph — 41 to 63 words each, eight of them, in a column about 290 pixels wide.
 * Patrick, reading a real review: "a real mess". Backticks printed literally and a long code
 * name ran out of the card.
 *
 * So it is the Solution page's answer to the same problem, which DEV-919 built and DEV-928
 * asks for by name: a row that is readable closed, and what is behind it arriving under it.
 * `OpenRow` is that row and is reused whole — the control geometry, the rule above, the open
 * wash, `aria-expanded` and `aria-controls` all come from it.
 *
 * ## Three rules this panel is held to
 *
 * 1. **It describes; it does not grade.** The severity is the record's own word in the body
 *    ink, with no tag, no colour that differs by severity and no ordering by anything but
 *    the record's own order within a group. `SeverityTag` is right on the code review page,
 *    where the developer is deciding what to do about each finding; here it would be telling
 *    a reader which of the things they have already decided not to post to be afraid of.
 * 2. **Nothing is printed twice.** A row whose line already is the whole claim shows no
 *    claim when it opens — see `heldBackRows` — and the code is named, not quoted: the code
 *    review page owns a finding's evidence and its hunk.
 * 3. **One row is open at a time**, by click and by keyboard, and it opens in place.
 *
 * ## Why this is a component and the edits list is not
 *
 * `draft.vue` is edited by DEV-929 and DEV-932 after this, so the panel that grew a grid, a
 * fold, two media queries and a piece of state is a file of its own and the page gains three
 * lines. The edits list is a derived list of `<p>`s with no state at all, and wrapping that
 * in a component would be a file to open to find out that it is a paragraph.
 */
const props = defineProps<{ rows: HeldBackRow[] }>()

const groups = computed(() => heldBackGroups(props.rows))

/**
 * Which row is open, by the finding's sequence number. One at a time, as on both boards.
 *
 * The sequence number rather than the position, because the groups draw the rows out of the
 * record's order — a question may be drawn above a nit that was raised before it — and
 * `aria-controls` keys on it.
 */
const open = ref<number | null>(null)

function toggle(seq: number) {
  open.value = open.value === seq ? null : seq
}

/**
 * The id a row's detail carries, for `aria-controls`.
 *
 * A fixed prefix. It took a prop for a while, so that two of these panels on one page could
 * not collide — and nothing ever passed one, which review measured: the draft page's aside
 * holds exactly one list of held-back findings, because the record holds one. A second
 * panel can have the prop back on the day it exists.
 */
function detailId(seq: number): string {
  return `held-back-${seq}`
}

/**
 * The row's name for a screen reader: the line as `spoken` says it, after the key.
 *
 * **`OpenRow`'s accessible name is a plain string, and the line is recorded prose** — so
 * every rule `TickedText` applies to the drawing has to be applied to the name as well, or
 * the two disagree about the same words. This stripped the backticks by hand for a while
 * and that was half an answer: it left a commit id shown as seven characters and announced
 * as forty, which review found. `spoken` derives both from one split, so a third rule
 * cannot leave the name behind the way the second one did.
 */
function said(row: HeldBackRow): string {
  return `${row.key}, ${spoken(row.line)}`
}

/**
 * How the panel's own heading counts what is in it: findings, always.
 *
 * **It said "4 nits" when every one of them was a nit**, which is what this panel said
 * before the rows existed and was right then. It is not right now: the group heading
 * immediately under it says "4 nits" too, so a reader of review 15's draft met the same
 * three words twice, three lines apart. Found by review, measured against the fixtures.
 *
 * The severities are the groups' to say. This counts the list.
 */
const heading = computed(() => count(props.rows.length, 'finding'))
</script>

<template>
  <div class="held">
    <div class="rd-label">
      Held back · {{ heading }}
    </div>

    <div class="rows">
      <template
        v-for="group in groups"
        :key="group.severity ?? 'unsaid'"
      >
        <!--
          A group with no row in it is not in the list at all — `heldBackGroups` drops it —
          so there is no heading to suppress here. A review whose every held-back finding is
          a nit draws one heading, not four with three empty.
        -->
        <h3 class="held-group">
          {{ group.heading }}
        </h3>

        <template
          v-for="row in group.rows"
          :key="row.seq"
        >
          <OpenRow
            :open="open === row.seq"
            :controls="detailId(row.seq)"
            opens="Open"
            :label="said(row)"
            class="held-row"
            @toggle="toggle(row.seq)"
          >
            <!--
              **A space between the cells as well as inside them**, for the reason
              `what-happened.vue` measures: they are separated by newlines, which
              `whitespace: 'condense'` deletes, so the row's text — its accessible name —
              read `F4Severity: nitWhat F4 says…`.
            -->
            <span class="cell cell--key rd-mono rd-meta">{{ row.key }}</span>{{ ' ' }}

            <!--
              **The severity as a word, with no tag and no colour.** Drawn only where the
              record names one: `HeldBackFinding.severity` is optional in the contract, and a
              cell reading "undefined" would be worse than one that says nothing. The group's
              own heading says it as well, which is deliberate — a row read on its own, or
              read out by a screen reader, carries its own severity.
            -->
            <span class="cell cell--severity rd-meta">
              <!--
                `{{ ' ' }}`, for the reason `what-happened.vue` measures: the newline that
                was here is a whitespace-only node between two elements, and Vue's
                `whitespace: 'condense'` deletes it — so the text read `Severity:nit`.
              -->
              <span class="rd-visually-hidden">Severity:</span>{{ ' ' }}<span
                v-if="row.severity"
              >{{ row.severity }}</span>
              <span
                v-else
                class="rd-meta"
              >not recorded</span>
            </span>{{ ' ' }}

            <span class="cell cell--line">
              <!--
                `TickedText`, so a backtick pair in a title draws as code rather than
                printing its backticks — which is what the paragraphs this replaces did.
              -->
              <!--
                `in-control`, because this is inside `OpenRow`'s button. A commit chip that
                was focusable here would be a second tab stop inside the row, and its own
                accessible name would put all forty characters into the row's — where the
                eye sees seven. Review of the commit before this found it.
              -->
              <TickedText
                :text="row.line"
                in-control
              />{{ ' ' }}
              <!--
                **A line the page cut says so.** The claim's first twelve words standing in
                for a title the session never wrote is not what the review said, and a reader
                has to know that before they decide this finding can wait. The ellipsis is in
                the string; this is the word for it, and the `title` attribute puts the whole
                claim within reach of a pointer without opening the row.
              -->
              <span
                v-if="row.shortened"
                class="shortened"
                :title="row.claim ?? undefined"
              >shortened</span>
            </span>
          </OpenRow>

          <!--
            **The row's detail, in place, immediately under it.** The Solution page's own
            arrangement: the thing that opened it is the thing it appears under.
          -->
          <div
            v-if="open === row.seq"
            :id="detailId(row.seq)"
            class="held-opened"
          >
            <p
              v-if="row.shortened"
              class="rd-meta cut-note"
            >
              No title was written for this finding, so the row shows the first
              {{ TITLE_WORDS }} words of its claim. The claim is below, whole.
            </p>
            <p
              v-if="row.claim"
              class="claim"
            >
              <TickedText :text="row.claim" />
            </p>
            <!--
              **Where it is, named and not quoted.** The code review page owns a finding's
              hunk, its evidence and the comment as it will post; this panel owns the list of
              what is not being posted. A path breaks mid-token because a path has no spaces
              to break at — the Solution page's own lesson about its file list.
            -->
            <p
              v-if="row.where"
              class="where"
            >
              <span class="rd-rubric">In the code</span>
              <span class="rd-mono path">{{ row.where }}</span>
            </p>
            <p
              v-else
              class="rd-meta"
            >
              The record names no file for this finding.
            </p>
          </div>
        </template>
      </template>
      <span class="end" />
    </div>
  </div>
</template>

<style scoped>
.held {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

/*
 * **The rows reach the card's own edges**, which is what makes the rule above each one read
 * as a divider rather than as a box inside a box. `DeskCard`'s `pad-normal` is 16px, and
 * these negative margins are that number — stated here because the card cannot know it has
 * a table in it, and a row inset by 16px on both sides in a 290px column loses an eighth of
 * the width the line it carries has to live in.
 */
.rows {
  display: flex;
  flex-direction: column;
  margin: 0 -16px;
  min-width: 0;
}

.end {
  border-top: 1px solid var(--rd-line);
}

/*
 * A group's heading. The serif face the boards use, at the size that reads as a divider
 * inside a list rather than as a second panel heading — board A10's own treatment of the
 * same thing on the Solution page.
 */
.held-group {
  margin: 14px 0 2px;
  padding: 0 16px;
  font-family: var(--rd-font-display);
  font-weight: 400;
  font-size: 17px;
}

.held-group:first-of-type {
  margin-top: 0;
}

/*
 * **Three lines, always, because the column is 290 pixels wide.**
 *
 * The Solution page's rows are six columns at a laptop's width and stack at 860px; this
 * panel lives in the aside, which is about 290 pixels inside a 1440-pixel page and about
 * 358 at a phone's width. There is no width at which a horizontal row fits, so there is one
 * layout and no media query for it: the key and the severity share a line, the line the row
 * carries takes the whole width under them, and `OpenRow`'s control sits at the foot — after
 * what it opens, which is where the Solution page's own comment argues it belongs.
 *
 * **`minmax(0, 1fr)` is what keeps a code name inside the card.** A grid track's default
 * minimum is its content, so a title holding `report_definitions.allowed_columns` would
 * widen the row rather than wrap inside it — which is the defect the issue names, in the
 * one declaration that fixes it.
 */
.held-row {
  grid-template-columns: 34px minmax(0, 1fr);
  grid-template-areas:
    "key severity"
    "line line"
    "opens opens";
  gap: 2px 10px;
  align-items: start;
  padding: 10px 16px;
}

/*
 * `OpenRow` owns the control and this owns the grid, which is its own documented bargain —
 * "the grid is the caller's". So the area it lands in is named here.
 *
 * **Every class in this component is prefixed `held-`**, and that is not decoration: the
 * step trail above this panel draws its own `.row` and `.group`, and a test asking the
 * rendered page for `.group` got two of the trail's before either of these. `OpenRow`'s own
 * element is `.row`, so an unprefixed name here would also be two classes deep in the same
 * word.
 */
.held-row :deep(.opens) {
  grid-area: opens;
}

.cell {
  min-width: 0;
}

/*
 * **`.rd-meta` carries the size and the ink**, applied rather than restated: it is the one
 * definition of the desk's quiet text, and a cell that declared `13px` and
 * `--rd-ink-muted` for itself would keep the old pair the day that definition changed, with
 * nothing failing. Found by review, which pointed out that two of the panel's three cells
 * had copied it out.
 */
.cell--key {
  grid-area: key;
}

/*
 * The severity, as quiet as the key beside it — `.rd-meta` carries the ink, applied in the
 * template. **Capitalised by CSS and not by a second copy of the word**: the record holds
 * `nit`, the page shows `Nit`, and a mapping in script would be a place for the two to
 * disagree.
 */
.cell--severity {
  grid-area: severity;
  text-transform: capitalize;
}

.cell--line {
  grid-area: line;
  font-weight: 500;
  overflow-wrap: break-word;
}

/*
 * **The word that says the line is not the review's own.** Quiet, because it is a note about
 * the page rather than about the finding, and in the body ink: it is not a warning and the
 * finding is not worse for having no title.
 */
.shortened {
  margin-left: 6px;
  font-weight: 400;
  font-size: 12px;
  color: var(--rd-ink-muted);
  font-variant: small-caps;
  letter-spacing: 0.04em;
  white-space: nowrap;
}

/* ---------------------------------- An opened row ---------------------------------- */

.held-opened {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 2px 16px 14px 50px;
  background: var(--rd-ground);
  min-width: 0;
}

.held-opened p {
  margin: 0;
}

.claim {
  max-width: var(--rd-measure-recorded);
  overflow-wrap: break-word;
}

.cut-note {
  max-width: var(--rd-measure-recorded);
}

.where {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

/*
 * `anywhere`, not `break-word`. A path has no spaces to break at, and
 * `app/Console/Commands/DispatchScheduledExports.php:61` is wider than this column — so it
 * breaks mid-token or it widens the card.
 */
.path {
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

/*
 * **At a phone's width only the gutters change.** The rows are already stacked at every
 * width — see the grid above — so there is no layout here to rearrange; the shell's own
 * padding is 12px rather than 16px below 560px, and these are the negative margins that
 * match it.
 */
@media (max-width: 560px) {
  .rows {
    margin: 0 -12px;
  }

  .held-row {
    padding: 10px 12px;
  }

  .held-group {
    padding: 0 12px;
  }

  .held-opened {
    padding: 2px 12px 14px 46px;
  }
}
</style>
