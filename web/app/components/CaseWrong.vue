<script setup lang="ts">
import type { Brief, ReviewDetail } from '~/types/desk'

/**
 * Band 1 of the case page — **what was wrong**.
 *
 * ```
 *  WHAT WAS WRONG · 4
 *        WAS WRONG              AND NOW                           FIXED BY
 *   (1)  Helpers only           The shared browser setup tags…    parts 2, 4   By name
 *        two sign-in helpers…
 *   (4)  Blind four-minute wait Partly. A refusal stops the…      part 3       By name
 *  ─────────────────────────────────────────────────────────────────────────────────────
 *  ▸ The two flows, before and after
 *  ▸ How it works today · the idea behind the change · what the review read · …
 * ```
 *
 * **The band is the shape of its own answer**: four problems are four rows, and the two
 * columns are the two halves of the record — `was_wrong` and `now_fixed` — so the before
 * and after is read across a row rather than across two pictures. What the old Understand
 * page drew above the rows is still here, in the two folds under them.
 *
 * ## Two things this band does that nothing did before
 *
 * **"Fixed by" is a join, and it is the reason the three steps are worth merging.** A
 * numbered problem lived on step 2 and the part that fixes it on step 4, and the only thing
 * connecting them was the reader's memory of a number. `partsFixing` answers it from
 * `parts[].fixes`, and the number it draws is the number band 3 draws on the part's row.
 *
 * **A problem nothing claims to fix says so.** That is the cell's empty answer, in words,
 * and it is a fact about the change rather than a gap in the record.
 *
 * ## Why the flows are folded
 *
 * They are the picture Patrick called "perfect", and folding something he praised needs its
 * reason written down. The rows above *are* the before and after, in the session's own two
 * lines per problem; the flows are the same answer drawn, and on review #9639 they are
 * roughly two thirds of the band's height. The fold's summary names them, it is one click,
 * and the band opens with the answer rather than with the illustration of it. If that turns
 * out to be the wrong way round in use, the fold is one attribute away from being open by
 * default.
 */
const props = defineProps<{
  detail: ReviewDetail
  brief: Brief
}>()

/**
 * Which problem a reader has opened, or `null`.
 *
 * One at a time, and it drives three places at once: the row's own detail and the marks on
 * both flows. That is why these rows are `OpenRow` and not `<details>` — a `<summary>` can
 * only open the thing inside it.
 */
const openProblem = ref<number | null>(null)

function toggleProblem(number: number) {
  openProblem.value = openProblem.value === number ? null : number
}

const problems = computed(() => problemsOf(props.brief))
const flowBefore = computed(() => props.brief.flow_before ?? [])
const flowAfter = computed(() => props.brief.flow_after ?? [])
const sources = computed(() => props.detail.sources ?? [])

/** Whether there is a picture to fold at all. */
const hasFlows = computed(() => flowBefore.value.length > 0 || flowAfter.value.length > 0)

/**
 * What the second fold holds, named piece by piece on its own summary line.
 *
 * **Named rather than counted.** "4 more things" tells a reader nothing about whether to
 * press it; "how it works today · the idea behind the change" tells them exactly. Each
 * clause is conditional on its own field, so the line cannot promise a section the brief
 * does not carry — which is this app's rule for every figure it draws.
 */
const reference = computed(() => {
  const said: string[] = []
  if (props.brief.how_it_works_today) said.push('how it works today')
  if (props.brief.support_table) said.push('the idea behind the change')
  if (sources.value.length) said.push('what the review read')
  if (props.brief.problem) said.push('the problem as recorded')
  if (props.brief.provenance) said.push('where it came from')
  return said
})
</script>

<template>
  <section
    id="what-was-wrong"
    class="band"
  >
    <h2 class="rd-display-lead">
      What was wrong<template v-if="problems.length">
        · {{ problems.length }}
      </template>
    </h2>

    <!--
      **No numbered problems, and the band says which nothing this is — from step 2's own
      state.** A brief written the old way carries none and is drawn by the page's own
      branch; this is the rarer one, a structured brief whose problem half records a picture
      or a table and no numbers.
    -->
    <DeskNotice
      v-if="!problems.length"
      tone="plain"
    >
      <strong>{{ sentence(nothingYet(detail.layers, 'understand')) }}</strong>
      Step 2 is where a session numbers what is wrong, so that an option can be judged
      against each one and a part of the change can say which it fixes.
    </DeskNotice>

    <!--
      A brief caught between a session's two writes carries the problems and no `now_fixed`,
      and that absence is a state with words rather than an empty cell.
    -->
    <div
      v-else
      class="rows"
    >
      <div class="rows-head">
        <span />
        <span class="rd-rubric">Was wrong</span>
        <span class="rd-rubric">And now</span>
        <span class="rd-rubric">Fixed by</span>
        <span />
      </div>

      <template
        v-for="problem in problems"
        :key="problem.number"
      >
        <OpenRow
          class="problem"
          :open="openProblem === problem.number"
          :controls="`problem-${problem.number}`"
          opens="By name"
          :label="`problem ${problem.number}`"
          @toggle="toggleProblem(problem.number)"
        >
          <ProblemMark
            :number="problem.number"
            tone="wrong"
            size="row"
            says="the problem"
          />
          <span
            class="cell cell--wrong"
            data-label="Was wrong"
          >
            <!--
              The label and the sentence, one above the other. The label is two or three
              words and is what the reader's eye takes the row by; the sentence is under it
              rather than instead of it, because a brief recorded before `label` existed has
              only the sentence and must still read as a row.
            -->
            <span
              v-if="problem.label"
              class="label"
            >{{ problem.label }}</span>
            <span class="said"><TickedText :text="problem.was_wrong" /></span>
          </span>
          <span
            class="cell"
            data-label="And now"
          >
            <!--
              `now_fixed` is nullable by design: the problem half can be written before the
              change is described, which is what lets a blind pass be handed a problem
              without the answer. So the absence is a state with words.
            -->
            <template v-if="problem.now_fixed">
              <TickedText :text="problem.now_fixed" />
            </template>
            <span
              v-else
              class="rd-meta"
            >not written yet</span>
          </span>
          <span
            class="cell cell--fixed-by"
            data-label="Fixed by"
          >
            <!--
              **The join, and the one thing on this page that nothing drew before.** A
              problem no part claims is said in words: an empty cell there is
              indistinguishable from a page that failed to join, and "nothing yet" is a fact
              about the change.
            -->
            <template v-if="fixedByLabel(brief, problem.number)">{{
              fixedByLabel(brief, problem.number)
            }}</template>
            <span
              v-else
              class="rd-meta"
            >nothing yet</span>
          </span>
        </OpenRow>

        <div
          v-if="openProblem === problem.number"
          :id="`problem-${problem.number}`"
          class="detail"
        >
          <p class="rd-rubric detail-head">
            {{ problem.number }}
            <template v-if="problem.detail_title">
              · {{ problem.detail_title }}
            </template>
          </p>
          <div class="detail-pair">
            <div class="detail-side">
              <h3 class="detail-label detail-label--wrong">
                Before
              </h3>
              <ul
                v-if="problem.detail_before?.length"
                class="lines"
              >
                <!--
                  **The line's text is one box.** The `li` is a flex row so the en-dash sits
                  beside the words, and `TickedText` adds no box of its own — so without this
                  span every word run and every code chip in the line would be a flex item of
                  the `li` and the line would draw as a row of narrow columns.
                -->
                <li
                  v-for="(line, index) in problem.detail_before"
                  :key="index"
                >
                  <span class="line-text"><TickedText :text="line" /></span>
                </li>
              </ul>
              <p
                v-else
                class="rd-meta"
              >
                The brief records no detail for this side.
              </p>
            </div>
            <div class="detail-side">
              <h3 class="detail-label detail-label--fixed">
                After
              </h3>
              <ul
                v-if="problem.detail_after?.length"
                class="lines"
              >
                <!-- One box for the line's text, as in the Before list above. -->
                <li
                  v-for="(line, index) in problem.detail_after"
                  :key="index"
                >
                  <span class="line-text"><TickedText :text="line" /></span>
                </li>
              </ul>
              <p
                v-else
                class="rd-meta"
              >
                The brief records no detail for this side.
              </p>
            </div>
          </div>
        </div>
      </template>
      <span class="end" />
    </div>

    <!-- ------------------------------- The two flows ------------------------------- -->
    <details
      v-if="hasFlows"
      class="rd-fold"
    >
      <summary>
        <template v-if="flowBefore.length && flowAfter.length">
          The two flows, before and
          after
        </template>
        <template v-else-if="flowBefore.length">
          The flow as it works today, drawn
        </template>
        <template v-else>
          The flow after the change, drawn
        </template>
      </summary>

      <div class="flows">
        <!--
          **The problems, listed once, above both flows.** Each mark in a box is a tag
          carrying the problem's name, and the names have to be readable in one place before
          a reader meets them scattered down two columns. In number order, which is the
          record's own order and the only order that makes "the same 2" easy to find.
        -->
        <div
          v-if="problems.length"
          class="key"
        >
          <span class="rd-note">
            {{ count(problems.length, 'problem') }}:
          </span>
          <ProblemTag
            v-for="problem in problems"
            :key="problem.number"
            :number="problem.number"
            :label="problem.label"
            tone="wrong"
          />
        </div>

        <div class="pair">
          <DeskCard
            v-if="flowBefore.length"
            tone="well"
            pad="section"
            class="side"
          >
            <div class="side-head">
              <h3 class="rd-display-card">
                Before
              </h3>
              <p
                v-if="brief.flow_before_caption"
                class="caption"
              >
                {{ brief.flow_before_caption }}
              </p>
            </div>
            <FlowDiagram
              :rows="flowBefore"
              tone="wrong"
              :focus="openProblem"
              :problems="problems"
            />
          </DeskCard>

          <DeskCard
            v-if="flowAfter.length"
            tone="well"
            pad="section"
            class="side"
          >
            <div class="side-head">
              <h3 class="rd-display-card">
                After
              </h3>
              <p
                v-if="brief.flow_after_caption"
                class="caption"
              >
                {{ brief.flow_after_caption }}
              </p>
            </div>
            <FlowDiagram
              :rows="flowAfter"
              tone="fixed"
              :focus="openProblem"
              :problems="problems"
            />
          </DeskCard>

          <!--
            The after side, during the minutes between the session's two writes. **A column
            that says it is not written yet, not a column that is missing**: the picture is a
            before and an after, and an absent right-hand side reads as a page that failed to
            load rather than as a record that is twenty minutes old.
          -->
          <DeskCard
            v-else-if="flowBefore.length"
            tone="pending"
            pad="section"
            class="side"
          >
            <div class="side-head">
              <h3 class="rd-display-card">
                After
              </h3>
            </div>
            <p class="rd-note pending-note">
              Not written yet. A session writes the problem first and hands it to an
              independent pass before it writes what was done about it, so this side arrives
              second. Nothing is wrong.
            </p>
          </DeskCard>
        </div>

        <!--
          The legend describes two flows, so it is drawn when there are two. Printed over a
          brief with no picture at all it explained marks that were not there.
        -->
        <p
          v-if="flowBefore.length && flowAfter.length"
          class="rd-note legend"
        >
          Each tag is one of the problems above. Red on the left marks what was wrong; green
          on the right, with the same number, marks what fixes it. Open a problem's row above
          to pick its boxes out of both flows.
        </p>
        <p
          v-else-if="flowBefore.length"
          class="rd-note legend"
        >
          Each red tag is one of the problems above. The green marks that say what fixes each
          one arrive with the after flow.
        </p>
      </div>
    </details>

    <!--
      ----------------------------- The reference material ---------------------------
      **One fold, and its summary names every piece.** These are the four things the old
      Understand page drew open under the picture — how the system works today, the one
      supporting table, what the review read the problem from, and the problem in the words
      the session wrote it in. Each is reference: wanted sometimes, and never first. Patrick
      on the three pages this one replaces: "you've got to dig into them". So the digging is
      behind one control that says what is down there.
    -->
    <details
      v-if="reference.length"
      class="rd-fold"
    >
      <summary>{{ reference.join(' · ') }}</summary>

      <div class="reference">
        <div
          v-if="brief.how_it_works_today"
          class="piece"
        >
          <h3 class="rd-rubric">
            How it works today
          </h3>
          <p class="rd-recorded recorded">
            <TickedText :text="brief.how_it_works_today" />
          </p>
        </div>

        <div
          v-if="brief.support_table"
          class="piece"
        >
          <h3 class="rd-rubric">
            {{ brief.support_table.title ?? 'The idea behind the change' }}
          </h3>
          <DeskTable
            :columns="brief.support_table.columns"
            :template="`repeat(${brief.support_table.columns.length}, minmax(0, 1fr))`"
            :label="brief.support_table.title ?? 'The idea behind the change'"
          >
            <tr
              v-for="(row, index) in brief.support_table.rows"
              :key="index"
              role="row"
              class="row"
            >
              <td
                v-for="(cell, column) in row"
                :key="column"
                role="cell"
                class="td"
                :class="{ 'td--strong': column === 0 }"
                :data-label="brief.support_table.columns[column]"
              >
                <TickedText :text="cell" />
              </td>
            </tr>
          </DeskTable>
          <span class="end" />
        </div>

        <!--
          What the review read the problem from. A link is an extra way to read a source
          rather than the only way: DEV-885 forbids a page whose only route to something is
          GitHub.
        -->
        <div
          v-if="sources.length"
          class="piece"
        >
          <h3 class="rd-rubric">
            What the review read the problem from
          </h3>
          <DeskTable
            :columns="['Kind', 'Reference', 'State', 'What it tells us', '']"
            template="110px minmax(0, 1.1fr) 130px minmax(0, 1.6fr) 60px"
            label="What this review read the problem from"
          >
            <tr
              v-for="source in sources"
              :key="source.position"
              role="row"
              class="row"
            >
              <td
                role="cell"
                class="td td--muted"
                data-label="Kind"
              >
                {{ source.kind }}
              </td>
              <td
                role="cell"
                class="td td--strong"
                data-label="Reference"
              >
                <TickedText :text="source.label" />
              </td>
              <td
                role="cell"
                class="td"
                data-label="State"
              >
                <!--
                  The ticket's state as the session read it. A source that reported none
                  draws an empty cell rather than a word Review Desk made up — it read no
                  ticket.
                -->
                <DeskPill
                  v-if="source.status"
                  tone="quiet"
                >
                  {{ source.status }}
                </DeskPill>
              </td>
              <td
                role="cell"
                class="td"
                data-label="What it tells us"
              >
                <TickedText
                  v-if="source.note"
                  :text="source.note"
                />
              </td>
              <td
                role="cell"
                class="td"
                data-label=""
              >
                <a
                  v-if="source.link"
                  :href="source.link"
                  target="_blank"
                  rel="noreferrer noopener"
                >open<span class="rd-visually-hidden">
                  {{ source.label }}, in a new tab</span></a>
              </td>
            </tr>
          </DeskTable>
          <span class="end" />
        </div>

        <div
          v-if="brief.problem"
          class="piece"
        >
          <h3 class="rd-rubric">
            The problem, as recorded
          </h3>
          <p class="rd-recorded recorded">
            <TickedText :text="brief.problem" />
          </p>
        </div>

        <div
          v-if="brief.provenance"
          class="piece"
        >
          <h3 class="rd-rubric">
            Where this came from, as recorded
          </h3>
          <p class="rd-recorded recorded">
            <TickedText :text="brief.provenance" />
          </p>
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

/* -------------------------------- The problem rows --------------------------------- */

.rows {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

/*
 * **Five tracks, and `minmax(0, …)` on both flexible ones.** A grid track's default minimum
 * is its content, so a long sentence widens the row rather than wrapping inside it — the one
 * declaration between this band and a horizontal scrollbar. "And now" takes more than "Was
 * wrong" because the first column carries a label over a short line and the second carries
 * the sentence a reader is here for.
 */
.rows-head,
.problem {
  grid-template-columns: 44px minmax(0, 0.95fr) minmax(0, 1.4fr) 104px 88px;
}

.rows-head {
  display: grid;
  gap: 8px 20px;
  padding: 0 16px 10px;
  align-items: baseline;
}

.end {
  border-top: 1px solid var(--rd-line);
}

.cell {
  min-width: 0;
  overflow-wrap: break-word;
}

.cell--wrong {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.label {
  font-weight: 600;
}

.said {
  font-size: 14px;
  color: var(--rd-ink-muted);
}

/* A brief with no label has only the sentence, and it is then the row's own words. */
.cell--wrong:not(:has(.label)) .said {
  font-size: inherit;
  color: inherit;
}

.cell--fixed-by {
  font-size: 14px;
  color: var(--rd-ink-muted);
}

.detail {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 22px 24px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
}

.detail-head {
  margin: 0;
}

.detail-pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 28px;
  min-width: 0;
}

.detail-side {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.detail-label {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.detail-label--wrong {
  color: var(--rd-wrong);
}

.detail-label--fixed {
  color: var(--rd-accent-dark);
}

.lines {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/*
 * The board's en-dash bullet, drawn rather than read out as a list marker. **A flex row here
 * has exactly two items: the dash and `.line-text`** — `TickedText` is `display: contents`,
 * so without that box a line holding five code names comes out as eight narrow columns with
 * the names broken mid-word.
 */
.lines li {
  display: flex;
  gap: 10px;
  overflow-wrap: break-word;
}

.lines li::before {
  content: '–';
  flex: none;
  color: var(--rd-ink-faint);
}

/*
 * `flex: 1` so the box is the column less the dash for *every* line, not just a long one:
 * shrink-to-fit only reaches the far edge while its content overflows. `min-width: 0` is the
 * guard against a token some later rule declines to break.
 */
.line-text {
  flex: 1;
  min-width: 0;
}

/* ------------------------------------ The flows ------------------------------------ */

.flows {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-top: 14px;
  min-width: 0;
}

/*
 * The row of tags over both flows. It wraps, and each tag moves whole: three tags of two or
 * three words each do not fit a phone's width on one line, and a tag split across two is two
 * halves of a shape.
 */
.key {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 14px;
  min-width: 0;
}

/*
 * The two flows side by side, and **one above the other the moment they do not both fit**.
 * 460px is the narrowest a flow box reads at, and `min(…, 100%)` is what stops the track
 * being wider than the page at 390px — the difference between wrapping and scrolling.
 */
.pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(460px, 100%), 1fr));
  gap: 28px;
  align-items: start;
  min-width: 0;
}

.side {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.side-head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
}

.caption {
  margin: 0;
  color: var(--rd-ink-muted);
}

.legend,
.pending-note {
  margin: 0;
  max-width: var(--rd-measure);
}

/* -------------------------------- The two folds ------------------------------------ */

/*
 * **The control itself is `.rd-fold` in `tokens.css`**, which the three bands share: a
 * summary in the accent, at the height everything you press is held to, with a disclosure
 * triangle a `display: flex` summary does not draw for itself. What is here is what the
 * fold holds.
 */
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

.recorded {
  margin: 0;
  color: var(--rd-ink-dim);
}

@media (max-width: 860px) {
  /*
   * The five-column row becomes a card, the way every table on these pages does. The column
   * names move out of sight rather than away, so a screen reader still has them, and each
   * cell says which column it is.
   */
  .rows-head {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .problem {
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 16px;
    align-items: start;
  }

  .cell::before {
    content: attr(data-label);
    display: block;
    margin-bottom: 2px;
    font-size: 13px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--rd-ink-muted);
  }

  /* `::before` needs a block above it, which a column of two lines already is. */
  .cell--wrong::before {
    margin-bottom: 4px;
  }
}
</style>
