<script setup lang="ts">
import type { Brief, BriefPart, Finding, ReviewDetail } from '~/types/desk'
import type { Absence } from '~/utils/steps'

/**
 * Band 3 of the case page — **what was built**.
 *
 * ```
 *  WHAT WAS BUILT · 5 PARTS · 9 FILES · +107 −31
 *        PART                        FIXES        SIZE      CODE REVIEW FOUND
 *  FIXES A PROBLEM
 *   1    The credential picker       (2 Token…)   1 file    2 to post · 1 ticket    Open
 *        Looks up a host's gate…                 +45 −0
 *  ─────────────────────────────────────────────────────────────────────────────────────
 *  ▸ 8 findings about no one part · 4 choices the ticket left open · 3 places the
 *    description and the code disagree
 * ```
 *
 * Board A10, which Patrick approved on 11 October 2026, unchanged in everything but where
 * it sits and what is folded under it. Its four rules still hold:
 *
 * 1. **A row says where the review looked; it does not grade the part.** The counts are
 *    plain text, in the record's own three dispositions, with no colour that differs by
 *    severity or by count. "The pages give understanding, not verdicts."
 * 2. **The one line never wraps.** `one_line` where the session wrote one, the summary cut
 *    to a line where it did not, and the cutting is the column's — see `oneLineOf`.
 * 3. **Nothing is printed twice.** The files are behind a fold inside the row and the
 *    findings are links, not copies.
 * 4. **One row is open at a time**, by click and by keyboard, and it opens in place.
 *
 * ## The findings on a part are derived, not stored
 *
 * `contract/README.md` says so: "the review's findings whose `file` is one of
 * `brief.parts[].files[].path`". It is the only place a finding is shown beside the part of
 * the change it is about, and **every finding appears exactly once** — `findingsOn` and
 * `findingsInNoPart` read one rule, `partOf`, the first part that claims the path.
 *
 * ## What the fold holds, and why all three are in one
 *
 * The findings no part accounts for, the choices the ticket left open, and the places the
 * description and the code disagree. Three lists, each a table on the old Solution page,
 * each read after the rows above rather than before them — so one control, whose line
 * counts each of them. **The counts are counted rather than asserted**, so the summary
 * cannot promise a section that is empty.
 */
const props = defineProps<{
  detail: ReviewDetail
  brief: Brief
}>()

/**
 * Which part is open, by its index in the record. One at a time, as on the board.
 *
 * The index rather than the number, because `aria-controls` and the detail's `id` key on it
 * and the groups draw the parts out of order — row 7 may be drawn above row 6.
 */
const openPart = ref<number | null>(null)

function togglePart(index: number) {
  openPart.value = openPart.value === index ? null : index
}

const parts = computed(() => partsOf(props.brief))
const problems = computed(() => problemsOf(props.brief))
const choices = computed(() => props.brief.open_choices ?? [])
const clashes = computed(() => props.brief.disagreements ?? [])
const loose = computed(() => findingsInNoPart(props.detail.findings, parts.value))

/**
 * What follows from step 4's state, as the sentence after the phrase.
 *
 * Keyed off `absenceOf`, so the two cannot disagree about which absence this is.
 */
const WHY: Record<Absence, string> = {
  being_worked_on: 'A session is writing them now.',
  not_written_yet:
    'A session writes the problem first and what was built second, so this arrives after '
    + 'the band above is already readable. Nothing is wrong.',
  none_recorded: 'The step finished without describing the change in parts.',
}

function files(part: BriefPart) {
  return part.files ?? []
}

/** The findings on one part. */
function onPart(index: number): Finding[] {
  return findingsOn(props.detail.findings, parts.value, index)
}

/** What the fold's line says, and nothing it cannot. */
const fold = computed(() => foldSummary([
  {
    count: loose.value.length,
    says: loose.value.length === 1
      ? 'finding about no one part'
      : 'findings about no one part',
  },
  {
    count: choices.value.length,
    says: choices.value.length === 1
      ? 'choice the ticket left open'
      : 'choices the ticket left open',
  },
  {
    count: clashes.value.length,
    says: clashes.value.length === 1
      ? 'place the description and the code disagree'
      : 'places the description and the code disagree',
  },
]))
</script>

<template>
  <section
    id="what-was-built"
    class="band"
  >
    <h2 class="rd-display-lead">
      What was built<template v-if="parts.length">
        · {{ builtHeading(brief) }}
      </template>
    </h2>

    <!--
      **The state is named from step 4's own state.** DEV-892's defect 6: the page said "The
      change has not been described in parts yet" whatever the trail above it said about step
      4. A session writes the problem first and what was built second, so this band is
      empty for minutes on every review and that is not a fault.
    -->
    <DeskNotice
      v-if="!parts.length"
      tone="plain"
    >
      <strong>{{ sentence(nothingYet(detail.layers, 'solution_map')) }}</strong>
      Step 4 is where a session says what was built, as the parts the change divides into.
      {{ WHY[absenceOf(detail.layers, 'solution_map')] }}
    </DeskNotice>

    <template v-else>
      <!--
        **The problems, met once before they are met scattered down a column.** Board A10's
        own legend. The tag carries the name because a bare number read down a column reads
        as a step count — and the numbering is band 1's, directly above, so there is no link
        to follow any more.
      -->
      <p
        v-if="problems.length"
        class="rd-note legend"
      >
        <span class="legend-words">The {{ count(problems.length, 'problem') }} above:</span>
        <ProblemTag
          v-for="problem in problems"
          :key="problem.number"
          :number="problem.number"
          :label="problem.label"
          tone="fixed"
        />
      </p>

      <div class="table">
        <!--
          The column captions. `aria-hidden`, because a row is a button and not a table cell
          — so the names a screen reader needs are inside each cell, where they belong to the
          value rather than to a position in a grid.
        -->
        <div
          class="caption-row"
          aria-hidden="true"
        >
          <span />
          <span class="rd-rubric">Part</span>
          <span class="rd-rubric">Fixes</span>
          <span class="rd-rubric">Size</span>
          <span class="rd-rubric">Code review found</span>
          <span />
        </div>

        <template
          v-for="grouping in partGroups(parts)"
          :key="grouping.group"
        >
          <!--
            A group with no part in it is not in the list at all, so there is no heading to
            suppress here: `partGroups` drops it.
          -->
          <h3 class="part-group">
            {{ grouping.heading }}
          </h3>

          <template
            v-for="row in grouping.rows"
            :key="row.index"
          >
            <OpenRow
              :open="openPart === row.index"
              :controls="`part-${row.index}`"
              opens="Open"
              :label="row.part.title"
              class="part"
              @toggle="togglePart(row.index)"
            >
              <span class="cell cell--number">{{ row.number }}</span>

              <span class="cell cell--part">
                <span class="part-title">{{ row.part.title }}</span>
                <!--
                  **One line, and it never becomes two.** `.one-line` clips with an ellipsis;
                  the whole string is in the document for a screen reader, and a summary
                  standing in for a one line the session never wrote also carries it as
                  `title` so a pointer can reach the rest.
                -->
                <span
                  class="one-line"
                  :class="{ 'one-line--cut': oneLineOf(row.part).cut }"
                  :title="oneLineOf(row.part).cut ? row.part.summary : undefined"
                ><TickedText :text="oneLineOf(row.part).text" /></span>
              </span>

              <span class="cell cell--fixes">
                <!--
                  `{{ ' ' }}` after the label: neither a newline nor `&#32;` at the end of
                  one survives Vue's `whitespace: 'condense'`, so this read `Fixes:None`.
                -->
                <span class="rd-visually-hidden">Fixes:</span>{{ ' ' }}
                <ProblemTag
                  v-for="number in row.part.fixes"
                  :key="number"
                  :number="number"
                  :label="labelOfProblem(brief, number)"
                  tone="fixed"
                />
                <span
                  v-if="!row.part.fixes.length"
                  class="rd-meta"
                >None</span>
              </span>

              <span class="cell cell--size">
                <span class="rd-visually-hidden">Size:</span>{{ ' ' }}<span>{{
                  sizeOf(row.part).files }}</span>
                <!--
                  `unchanged` rather than `+0 −0`. A pair of noughts reads as a part nobody
                  wrote anything in; this is a part the pull request did not touch, which is
                  a different fact and the reason its group exists.
                -->
                <span
                  v-if="sizeOf(row.part).lines"
                  class="lines"
                >{{ sizeOf(row.part).lines }}</span>
                <span
                  v-else
                  class="lines lines--unchanged"
                >unchanged</span>
              </span>

              <span class="cell cell--found">
                <span class="rd-visually-hidden">The code review found:</span>{{ ' ' }}
                <template v-if="foundOn(onPart(row.index)).length">
                  <span
                    v-for="entry in foundOn(onPart(row.index))"
                    :key="entry.label"
                    class="found"
                  ><strong>{{ entry.n }}</strong> {{ entry.label }}</span>
                </template>
                <!--
                  Said in a word, because an empty space here is indistinguishable from a page
                  that failed to join.
                -->
                <span
                  v-else
                  class="rd-meta"
                >Nothing</span>
              </span>
            </OpenRow>

            <!--
              **The row's detail, in place, immediately under it.** A fold under the whole
              table was the card grid's answer to a grid that reflowed when a full-width row
              was inserted into it; rows do not reflow, so the detail sits where the thing
              that opened it is.
            -->
            <div
              v-if="openPart === row.index"
              :id="`part-${row.index}`"
              class="opened"
            >
              <p class="summary">
                <TickedText :text="row.part.summary" />
              </p>

              <div
                v-if="onPart(row.index).length"
                class="findings-block"
              >
                <p class="rd-rubric">
                  What the code review found in this part · each opens on the code review
                  page
                </p>
                <ul class="findings">
                  <li
                    v-for="finding in onPart(row.index)"
                    :key="finding.seq"
                  >
                    <!--
                      **The whole line is the link, and it reaches that finding** rather than
                      the page it is on. No claim, no evidence, no code: the code review page
                      owns those.
                    -->
                    <NuxtLink
                      :to="findingLink(detail.review.id, finding)"
                      class="finding"
                    >
                      <span class="severity">{{ finding.severity }}</span>
                      <span class="rd-mono key">{{ finding.key }}</span>
                      <span class="what"><TickedText :text="finding.title ?? finding.claim" /><!--
                        **What became of a carried finding this round.** The counts above
                        group by disposition, and a finding carried in as fixed keeps the
                        disposition it was carried with — so it sits in "to post" and will not
                        be posted. This is the thing on the line that says so.
                      --><span
                        v-if="carriedAfterTitle(finding)"
                        class="carried"
                      >{{ carriedAfterTitle(finding) }}</span></span>
                      <span class="stands">{{ standsAt(finding) }} →</span>
                    </NuxtLink>
                  </li>
                </ul>
              </div>
              <p
                v-else
                class="rd-empty"
              >
                The code review found nothing in this part.
              </p>

              <!--
                **The files are reference, so they are behind a fold and it is closed.** The
                size column gives the weight without the list, and up to seven paths per part
                was half of what the old card printed on every one of them.
              -->
              <details
                v-if="files(row.part).length"
                class="rd-fold files-fold"
              >
                <summary>{{ count(files(row.part).length, 'file') }} in this part</summary>
                <ul class="files">
                  <li
                    v-for="file in files(row.part)"
                    :key="file.path"
                    class="file"
                  >
                    <span class="path">{{ file.path }}</span>
                    <span
                      v-if="file.in_diff"
                      class="lines"
                    >+{{ file.lines_added }} −{{ file.lines_removed }}</span>
                    <span
                      v-else
                      class="lines lines--unchanged"
                    >not in the change</span>
                  </li>
                </ul>
              </details>
              <p
                v-else
                class="rd-meta"
              >
                No file is recorded against this part.
              </p>
            </div>
          </template>
        </template>
        <span class="end" />
      </div>

      <p class="rd-note legend">
        <span class="legend-words">The findings on a part are the review's own, matched by
          file — so a finding written after this brief appears here without the brief
          changing, and a finding in a file no part names is under the fold below rather
          than lost.</span>
      </p>
    </template>

    <!--
      ----------------------- The three lists, behind one control --------------------
      Drawn here and nowhere else. `open_choices` fed board A5's "Already decided" fold and
      board A7's table, and they are one list — so the band that says what was built is
      where both the choices and the clashes sit, under the rows they are about.
    -->
    <details
      v-if="fold"
      class="rd-fold"
    >
      <summary>{{ fold }}</summary>

      <div class="reference">
        <!--
          Findings no part accounts for. Not an error, and not hidden: on the review this was
          measured against seven of seventeen had no file or a file in no part, and they
          appeared nowhere on the page at all.
        -->
        <div
          v-if="loose.length"
          class="piece"
        >
          <h3 class="rd-rubric">
            {{ noPartHeading(loose.length) }}
          </h3>
          <p class="rd-note">
            They are about the description, the ticket or the release, or they sit in a file
            none of the parts above names. A part can claim a file the pull request never
            changed, so this is a gap in the description rather than a limit of the record.
          </p>
          <ul class="findings findings--loose">
            <li
              v-for="finding in loose"
              :key="finding.seq"
            >
              <NuxtLink
                :to="findingLink(detail.review.id, finding)"
                class="finding"
              >
                <span class="severity">{{ finding.severity }}</span>
                <span class="rd-mono key">{{ finding.key }}</span>
                <span class="what"><TickedText :text="finding.title ?? finding.claim" /><span
                  v-if="carriedAfterTitle(finding)"
                  class="carried"
                >{{ carriedAfterTitle(finding) }}</span></span>
                <span class="stands">{{ standsAt(finding) }} →</span>
              </NuxtLink>
            </li>
          </ul>
          <span class="end" />
        </div>

        <div
          v-if="choices.length"
          class="piece"
        >
          <h3 class="rd-rubric">
            Where the ticket left a choice
          </h3>
          <DeskTable
            :columns="['The ticket left open', 'Chosen', 'Why', 'Against the ticket']"
            template="1.2fr 1fr 1.5fr 1fr"
            label="Where the ticket left a choice, and what the change chose"
          >
            <tr
              v-for="(choice, index) in choices"
              :key="index"
              role="row"
              class="row"
            >
              <td
                role="cell"
                class="td"
                data-label="The ticket left open"
              >
                <TickedText :text="choice.left_open" />
              </td>
              <td
                role="cell"
                class="td td--strong"
                data-label="Chosen"
              >
                <TickedText :text="choice.chosen" />
              </td>
              <td
                role="cell"
                class="td td--muted"
                data-label="Why"
              >
                <TickedText :text="choice.why" />
              </td>
              <td
                role="cell"
                class="td td--pills"
                data-label="Against the ticket"
              >
                <!--
                  Two different things, and the distinction is the column's point. A departure
                  is a decision somebody should look at; filling a gap the ticket left is the
                  change doing its job.
                -->
                <DeskPill
                  v-if="choice.departs_from_ticket"
                  tone="info-strong"
                >
                  Departs from the ticket
                </DeskPill>
                <span
                  v-else
                  class="rd-meta"
                >Left open by the ticket</span>
                <DeskPill
                  v-if="choice.finding_key"
                  tone="quiet"
                >
                  {{ choice.finding_key }} raises it
                </DeskPill>
              </td>
            </tr>
          </DeskTable>
          <span class="end" />
        </div>

        <div
          v-if="clashes.length"
          class="piece"
        >
          <h3 class="rd-rubric">
            Where the description and the code disagree
          </h3>
          <p class="rd-note">
            Each one names the finding that raises it. A brief that said the description
            claimed one thing and the code did another, and raised nothing about it, would
            post a review that never mentioned it — so the contract requires the finding.
          </p>
          <DeskTable
            :columns="['Finding', 'The description says', 'The code does']"
            template="110px 1fr 1fr"
            label="Where the description and the code disagree"
          >
            <tr
              v-for="(clash, index) in clashes"
              :key="index"
              role="row"
              class="row"
            >
              <td
                role="cell"
                class="td td--strong"
                data-label="Finding"
              >
                <span class="rd-mono">{{ clash.finding_key }}</span>
              </td>
              <td
                role="cell"
                class="td"
                data-label="The description says"
              >
                <TickedText :text="clash.description_says" />
              </td>
              <td
                role="cell"
                class="td"
                data-label="The code does"
              >
                <TickedText :text="clash.code_does" />
              </td>
            </tr>
          </DeskTable>
          <span class="end" />
        </div>
      </div>
    </details>
  </section>
</template>

<style scoped>
.band {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.end {
  border-top: 1px solid var(--rd-line);
}

/*
 * The legend is a row of whole tags with a sentence in front of it. `flex-wrap` on the
 * parent is the only thing that can know there is no room, which is why the tag itself
 * carries `flex: none` and `nowrap` and nothing here repeats them.
 */
.legend {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  margin: 0;
  max-width: var(--rd-measure);
}

.legend-words {
  flex: 0 1 auto;
}

/* ------------------------------------- The table ----------------------------------- */

.table {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/*
 * **The six tracks, and `minmax(0, …)` on every flexible one.** A grid track's default
 * minimum is its content, so a long title or a long path widens the row rather than
 * shrinking inside it — which is the one declaration standing between this page and a
 * horizontal scrollbar at 390px.
 */
.caption-row,
.part {
  display: grid;
  grid-template-columns:
    34px minmax(0, 2.4fr) minmax(0, 1.25fr) 112px minmax(0, 1.15fr) auto;
  gap: 8px 18px;
  min-width: 0;
}

.caption-row {
  padding: 0 16px 8px;
}

/*
 * A group's heading. The serif face the boards use for a heading, at the size that reads as
 * a divider inside a table rather than as a second page heading.
 */
.part-group {
  margin: 22px 0 6px;
  padding: 0 16px;
  font-family: var(--rd-font-display);
  font-weight: 400;
  font-size: 19px;
}

.part-group:first-of-type {
  margin-top: 0;
}

/*
 * `align-items: start`, against `OpenRow`'s own `center`. A row is two lines tall in its
 * second column and one line tall in the others, and centring put the number, the tags and
 * the size against the middle of the title and its one line.
 */
.part {
  align-items: start;
}

.cell {
  min-width: 0;
}

.cell--number {
  color: var(--rd-ink-muted);
  font-variant-numeric: tabular-nums;
}

.cell--part {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.part-title {
  font-weight: 600;
  overflow-wrap: break-word;
}

/*
 * **The one line, which is one line.** Three declarations and each is load-bearing:
 * `nowrap` so it cannot become two, `overflow: hidden` so what does not fit is not drawn,
 * and `text-overflow: ellipsis` so the cut is visible rather than silent. `min-width: 0` is
 * on `.cell`, and without it the ellipsis never fires.
 */
.one-line {
  font-size: 14px;
  color: var(--rd-ink-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* A wrapping row of whole tags: the one rule a tag row has to carry. */
.cell--fixes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.cell--size,
.cell--found {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 12px;
  font-size: 14px;
}

.cell--size {
  flex-direction: column;
  gap: 2px;
}

.lines {
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
  color: var(--rd-ink-muted);
  white-space: nowrap;
}

.lines--unchanged {
  font-family: var(--rd-font-sans);
  font-size: 13px;
}

/*
 * **No colour that differs by severity or by count.** The row says where the review looked,
 * and a cell that painted the blocker red would be telling the reader which part to be
 * afraid of. `nowrap` so "held for a ticket" moves whole to the next line.
 */
.found {
  white-space: nowrap;
}

/* ---------------------------------- An opened row ---------------------------------- */

.opened {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 4px 16px 20px 68px;
  background: var(--rd-ground);
  min-width: 0;
}

.summary {
  margin: 0;
  max-width: var(--rd-measure-recorded);
  overflow-wrap: break-word;
}

.findings-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.findings-block p {
  margin: 0;
}

.findings {
  display: flex;
  flex-direction: column;
  gap: 0;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/*
 * One finding, as a link. Four tracks: how serious it is, its number, its one line, and
 * where it stands — and the whole row is the anchor, so there is no small target inside a
 * line a reader has already decided to follow.
 */
.finding {
  display: grid;
  grid-template-columns: 74px 38px minmax(0, 1fr) auto;
  gap: 4px 12px;
  align-items: baseline;
  padding: 8px 0;
  border-top: 1px solid var(--rd-line);
  color: inherit;
  text-decoration: none;
  font-size: 14px;
  min-width: 0;
}

.finding:hover {
  background: var(--rd-rail-pressed);
}

.finding:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

/*
 * **The severity as a word, with no tag and no colour.** `SeverityTag` is right on the code
 * review page, where the developer is deciding what to do about each one; here it would
 * grade the part, which is the thing this band does not do.
 */
.severity {
  text-transform: capitalize;
  color: var(--rd-ink-muted);
}

.key {
  color: var(--rd-ink-muted);
}

.what {
  min-width: 0;
  overflow-wrap: break-word;
}

/*
 * A carried finding's round status, beside its title. **No `margin-left`** — the space that
 * separates it from the title is the first character of the string, because a margin moves
 * the box and leaves the text nodes touching, which is what a screen reader and a copy both
 * read.
 */
.carried {
  color: var(--rd-ink-muted);
  white-space: nowrap;
}

.stands {
  color: var(--rd-accent);
  white-space: nowrap;
  text-align: right;
}

.finding:hover .stands {
  color: var(--rd-accent-dark);
  text-decoration: underline;
}

.findings--loose .finding {
  padding: 10px 16px;
}

/* ------------------------------------ The files ------------------------------------ */

/*
 * **The fold inside an opened row, which is `.rd-fold` at a smaller size.** It was the one
 * control on these pages with a pointer and no height of its own — a 14px line, which is
 * under the 44px floor `test/contrast.spec.ts` holds every pressable thing to. It escaped
 * that test while it lived on a page rather than in a component; moving the band here is
 * what found it.
 */
.files-fold > summary {
  font-size: 14px;
}

.files {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
  max-width: var(--rd-measure);
  min-width: 0;
}

.file {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 2px 12px;
  align-items: baseline;
  min-width: 0;
}

/*
 * `anywhere`, not `break-word`. A path has no spaces to break at and is wider than a phone,
 * so it breaks mid-token or it widens the page.
 */
.path {
  flex: 1 1 auto;
  min-width: 0;
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

/* --------------------------------- The three lists --------------------------------- */

/* The control itself is `.rd-fold` in `tokens.css`; what is here is what it holds. */
.reference {
  display: flex;
  flex-direction: column;
  gap: 32px;
  padding-top: 14px;
  min-width: 0;
}

.piece {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.piece p {
  margin: 0;
  max-width: var(--rd-measure);
}

/* ---------------------------------- Narrower than a laptop ------------------------- */

/*
 * At a phone's width the six columns become a stack, which is the code review page's own
 * answer to the same problem: a board has one width and an app has two. The cells keep the
 * order they are read in, and `OpenRow`'s control — auto-placed after them — lands on a line
 * of its own at the foot of the block, which is where it belongs: "Open" belongs after what
 * it is opening, not above it.
 */
@media (max-width: 860px) {
  .caption-row {
    display: none;
  }

  .part {
    grid-template-columns: 26px minmax(0, 1fr) auto;
  }

  .cell--fixes,
  .cell--size,
  .cell--found {
    grid-column: 2 / -1;
  }

  .cell--size {
    flex-direction: row;
    align-items: baseline;
    gap: 2px 12px;
  }

  .opened {
    padding: 4px 16px 20px 42px;
  }

  .finding {
    grid-template-columns: 74px minmax(0, 1fr);
  }

  .what,
  .stands {
    grid-column: 1 / -1;
  }

  .stands {
    text-align: left;
  }
}

@media (max-width: 560px) {
  .opened {
    padding: 4px 12px 18px 16px;
  }

  .findings--loose .finding {
    padding: 10px 12px;
  }
}
</style>
