<script setup lang="ts">
import type { Brief, BriefPart, Finding, ReviewDetail, ReviewLayer } from '~/types/desk'
import type { Absence } from '~/utils/steps'

/**
 * Step 4 — **Solution**. Board A10, which Patrick approved on 11 October 2026.
 *
 * ```
 *  WHAT WAS BUILT, IN 7 PARTS
 *  The three problems, from Understand:  (1 The shared slot) (2 The builder's branch) …
 *        PART                       FIXES          SIZE      CODE REVIEW FOUND
 *  FIXES A PROBLEM
 *   1    The export context         (1 The sha…)   3 files   2 to post
 *        Every export is handed…                   +134 −70                      Open
 *  ──────────────────────────────────────────────────────────────────────────────────
 *  SUPPORTING
 *   6    Tests and generated types  None           5 files   Nothing          Open
 *  ──────────────────────────────────────────────────────────────────────────────────
 *  NOT CHANGED, BUT THE CHANGE RELIES ON IT
 *   7    The long-running queue…    None           1 file    Nothing          Open
 *        Nothing here changed…                     unchanged
 * ```
 *
 * ## What the section is for, stated once
 *
 * **Before the reader looks at code, show which piece of the build fixes which problem, and
 * where the review looked.** It had no stated job before, and so everything in it carried
 * equal weight — Patrick, reading a real review: "the left hand side shows all the files
 * that were edited, which I'm not really sure how useful that is… it's just a case of trying
 * to understand what I'm meant to be doing with this and how easy that is to read."
 *
 * Eight cards became eight rows for a measured reason. The cards carried the part's whole
 * summary, which on that review ran 33 to 52 words each, and reprinted the part's files on
 * one side and the review's findings on the other — so the page printed the file list twice
 * over and the findings twice over, and nothing on it said which part fixed what.
 *
 * ## Four rules this page is held to
 *
 * 1. **A row says where the review looked; it does not grade the part.** The counts are
 *    plain text, in the record's own three dispositions, with no colour that differs by
 *    severity or by count and no ordering by seriousness. "The pages give understanding,
 *    not verdicts" — so there is no severity tag anywhere in this section, inside a row or
 *    on it.
 * 2. **The one line never wraps.** `one_line` where the session wrote one, the summary cut
 *    to a line where it did not, and the cutting is the column's — see `oneLineOf`.
 * 3. **Nothing is printed twice.** The files are behind a fold inside the row and the
 *    findings are links, not copies: the code review page owns a finding's claim, its
 *    evidence and its code, and this page owns the join.
 * 4. **One row is open at a time**, by click and by keyboard, and it opens in place.
 *
 * ## The findings on a part are derived, not stored
 *
 * `contract/README.md` says so: "the review's findings whose `file` is one of
 * `brief.parts[].files[].path`". That is the join that makes the page worth having — it is
 * the only place a finding is shown beside the part of the change it is about — and it is
 * done here because the contract deliberately does not store it. A stored copy is a copy,
 * and a copy is a thing that can differ.
 *
 * Three consequences of the join being a join:
 *
 * 1. A finding added after the brief was written appears on its part without the brief
 *    being rewritten.
 * 2. A part whose files hold no findings says so. "Nothing" is a result.
 * 3. **A finding in no part is still shown**, under the table, rather than being dropped. On
 *    the review this was measured against seven findings of seventeen had no file or a file
 *    in no part, and they appeared nowhere on the page at all.
 *
 * **Every finding appears exactly once**, which is `findingsOn` and `findingsInNoPart`
 * reading one rule — `partOf`, the first part that claims the path. `test/solution.spec.ts`
 * counts them against the record.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review ? `Solution · #${review.pr} · Review Desk` : 'Solution · Review Desk'
  }),
})

/**
 * The page's one heading.
 *
 * The verdict on the approach when the brief has one; otherwise what step 4's own state
 * says. A brief whose change half has not been written has no verdict either, because both
 * arrive in the same write.
 *
 * **The absence is read off the step, not off the emptiness of the list.** DEV-892's defect
 * 6: this said "The change has not been described in parts yet" whatever the trail above it
 * said about step 4, and the notice underneath said the same sentence again — one claim,
 * drawn twice, and wrong whenever a session was writing the parts at that moment.
 */
function headline(brief: Brief, layers: ReviewLayer[]): string {
  if (brief.approach_verdict) return brief.approach_verdict
  return parts(brief).length ? 'What was built' : sentence(nothingYet(layers, 'solution_map'))
}

/**
 * What follows from step 4's state, as the one sentence under the heading.
 *
 * Never the heading again. "One sentence, once" is the issue's own wording, and the pair it
 * was written about was a headline and a notice carrying the same words three inches apart.
 *
 * Keyed off `absenceOf`, so the two cannot disagree about which absence this is.
 */
const WHY: Record<Absence, string> = {
  being_worked_on: 'A session is writing them now.',
  not_written_yet:
    'A session writes the problem first and what was built second, so this arrives after '
    + 'the Understand page is already readable. Nothing is wrong.',
  none_recorded: 'The step finished without describing the change in parts.',
}

function whyNoParts(layers: ReviewLayer[]): string {
  return WHY[absenceOf(layers, 'solution_map')]
}

/**
 * The line under the heading: the size of the change, when there is one to measure.
 *
 * **It counts every file the parts name, including the ones outside the diff**, and that
 * overstates the change by three on the invented example. It is left that way on purpose,
 * and the reasoning is worth writing down because the obvious fix is wrong.
 *
 * Counting only `in_diff` files makes this line say 24 — and the seven rows below it still
 * say 3, 4, 5, 4, 5, 5 and 1, which sum to 27. Only one of those rows is marked
 * "unchanged"; parts 4 and 5 each claim a file outside the diff behind a size cell that
 * says nothing about it. So the honest-looking number makes the page disagree with itself,
 * and a reader adding the column up is out by two with nothing to tell them why.
 *
 * Both halves — this line and the rows — count claimed files, so they agree. Making them
 * both count changed files means marking a partly-outside part on its row, which is a
 * change to the row that board A10 does not draw and DEV-919 did not ask for.
 *
 * **So the line says how many of them the pull request did not change**, which is the
 * option neither of the first two readings of this had: the count still sums the column,
 * and a reader comparing it with GitHub's own "24 files changed" is told the difference
 * rather than left to find it. Drawn only when there is one, because "(0 not changed)" is
 * a nought nobody asked for.
 *
 * This predates DEV-919. Found reviewing its diff, fixed by counting only the changed
 * files, that fix reverted when reviewing *it* — the rows still summed to 27 — and settled
 * here on the third reading.
 */
function sub(brief: Brief): string | null {
  if (!parts(brief).length) return null
  const outside = allFiles(brief).filter(file => !file.in_diff).length
  const named = `${count(allFiles(brief).length, 'file')}`
    + (outside ? ` (${outside} the pull request did not change)` : '')
  return `${named}, ${linesAdded(brief)} lines added `
    + `and ${linesRemoved(brief)} removed, in ${count(parts(brief).length, 'part')}.`
}

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

/* ------------------------------- Reading the brief ------------------------------- */

function parts(brief: Brief): BriefPart[] {
  return brief.parts ?? []
}

function files(part: BriefPart) {
  return part.files ?? []
}

/** Every file the change was described in, across every part. */
function allFiles(brief: Brief) {
  return parts(brief).flatMap(part => files(part))
}

function linesAdded(brief: Brief): number {
  return parts(brief).reduce((total, part) => total + part.lines_added, 0)
}

function linesRemoved(brief: Brief): number {
  return parts(brief).reduce((total, part) => total + part.lines_removed, 0)
}

/* ------------------------------ What the table draws ----------------------------- */

/**
 * The problems, as the tags the row's "Fixes" column carries.
 *
 * Drawn once in the legend as well, so a reader meets the three names before meeting them
 * scattered down a column — which is board A10's own arrangement and the reason the tags
 * carry names at all: a bare number read down a column reads as a count.
 */
function problems(brief: Brief) {
  return brief.problems ?? []
}

/** The findings on one part, and the ones no part accounts for. One rule, read twice. */
function onPart(data: ReviewDetail, brief: Brief, index: number): Finding[] {
  return findingsOn(data.findings, parts(brief), index)
}

function inNoPart(data: ReviewDetail, brief: Brief): Finding[] {
  return findingsInNoPart(data.findings, parts(brief))
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <!--
        **No brief at all, which is not one of the four absences below.** This is a statement
        about the record — step 2 has not written one — and it stays a statement about the
        record rather than being routed through step 4's state: a review with no brief cannot
        have a step 4 under way, and a heading that said "being worked on now" here would be
        reporting on a step that has nothing to work from.
      -->
      <ReviewStepPage
        v-if="!data.brief"
        :detail="data"
        step="solution_map"
        headline="Nothing has been written about this change yet."
        sub="Step 4 is where a session says what was built, as the parts the change divides into. There is no brief to hold it."
      />

      <!--
        **One page whose sections are each conditional, not two pages.**

        Solution drew an empty-state page whenever `parts` was empty — and `open_choices`
        and `disagreements` live only on this page, because the Understand page gives up
        its table to it and says so in a line and a link. So a brief with choices recorded
        and no parts had them drawn nowhere at all, while Understand promised "3 choices
        the ticket left open … they are on the Solution page". The three sections are three
        independent lists and are now gated independently.
      -->
      <ReviewStepPage
        v-else
        :detail="data"
        step="solution_map"
        :headline="headline(data.brief, data.layers)"
        :sub="sub(data.brief)"
      >
        <!-- ------------------------------ What was built ------------------------------ -->
        <section
          v-if="parts(data.brief).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            What was built, in {{ count(parts(data.brief).length, 'part') }}
          </h2>

          <!--
            **The three problems, met once before they are met scattered down a column.**
            Board A10's own legend. The tag carries the name because a bare number read down
            a column reads as a step count, which is `ProblemTag`'s whole reason for
            existing — and the link says where the numbering comes from.
          -->
          <p
            v-if="problems(data.brief).length"
            class="rd-note legend"
          >
            <span class="legend-words">The {{ count(problems(data.brief).length, 'problem') }},
              from <NuxtLink :to="`/reviews/${data.review.id}/understand`">the Understand
                page</NuxtLink>:</span>
            <ProblemTag
              v-for="problem in problems(data.brief)"
              :key="problem.number"
              :number="problem.number"
              :label="problem.label"
              tone="fixed"
            />
          </p>

          <div class="table">
            <!--
              The column captions. `aria-hidden`, because a row is a button and not a table
              cell — so the names a screen reader needs are inside each cell, where they
              belong to the value rather than to a position in a grid.
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
              v-for="grouping in partGroups(parts(data.brief))"
              :key="grouping.group"
            >
              <!--
                A group with no part in it is not in the list at all, so there is no heading
                to suppress here: `partGroups` drops it. A review whose every part fixes
                something draws one heading, not three with two empty.
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
                      **One line, and it never becomes two.** `.one-line` clips with an
                      ellipsis; the whole string is in the document for a screen reader, and
                      a summary standing in for a one line the session never wrote also
                      carries it as `title` so a pointer can reach the rest — which is the
                      code review page's answer for a finding with no title of its own.
                    -->
                    <span
                      class="one-line"
                      :class="{ 'one-line--cut': oneLineOf(row.part).cut }"
                      :title="oneLineOf(row.part).cut ? row.part.summary : undefined"
                    ><TickedText :text="oneLineOf(row.part).text" /></span>
                  </span>

                  <span class="cell cell--fixes">
                    <span class="rd-visually-hidden">Fixes:</span>
                    <!--
                      The named tags the flows already use, and no second version of them.
                      A part that fixes none says so in a word: tests and generated types
                      are part of a change and fix nothing, and an empty cell there is
                      indistinguishable from a page that failed to draw.
                    -->
                    <ProblemTag
                      v-for="number in row.part.fixes"
                      :key="number"
                      :number="number"
                      :label="labelOfProblem(data.brief, number)"
                      tone="fixed"
                    />
                    <span
                      v-if="!row.part.fixes.length"
                      class="rd-meta"
                    >None</span>
                  </span>

                  <span class="cell cell--size">
                    <span class="rd-visually-hidden">Size:</span>
                    <span>{{ sizeOf(row.part).files }}</span>
                    <!--
                      `unchanged` rather than `+0 −0`. A pair of noughts reads as a part
                      nobody wrote anything in; this is a part the pull request did not
                      touch, which is a different fact and the reason its group exists.
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
                    <span class="rd-visually-hidden">The code review found:</span>
                    <template v-if="foundOn(onPart(data, data.brief, row.index)).length">
                      <span
                        v-for="entry in foundOn(onPart(data, data.brief, row.index))"
                        :key="entry.label"
                        class="found"
                      ><strong>{{ entry.n }}</strong> {{ entry.label }}</span>
                    </template>
                    <!--
                      Said in a word, because an empty space here is indistinguishable from
                      a page that failed to join.
                    -->
                    <span
                      v-else
                      class="rd-meta"
                    >Nothing</span>
                  </span>
                </OpenRow>

                <!--
                  **The row's detail, in place, immediately under it.** A fold under the
                  whole table was the card grid's answer to a grid that reflowed when a
                  full-width row was inserted into it; rows do not reflow, so the detail
                  sits where the thing that opened it is.
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
                    v-if="onPart(data, data.brief, row.index).length"
                    class="findings-block"
                  >
                    <p class="rd-rubric">
                      What the code review found in this part · each opens on the code
                      review page
                    </p>
                    <ul class="findings">
                      <li
                        v-for="finding in onPart(data, data.brief, row.index)"
                        :key="finding.seq"
                      >
                        <!--
                          **The whole line is the link, and it reaches that finding** rather
                          than the page it is on. No claim, no evidence, no code: the code
                          review page owns those, and printing them here is what made this
                          page print everything twice.
                        -->
                        <NuxtLink
                          :to="findingLink(data.review.id, finding)"
                          class="finding"
                        >
                          <span class="severity">{{ finding.severity }}</span>
                          <span class="rd-mono key">{{ finding.key }}</span>
                          <span class="what"><TickedText :text="finding.title ?? finding.claim" /><!--
                            **What became of a carried finding this round.** The counts above group by
                            disposition, and a finding carried in as fixed keeps the disposition it was
                            carried with — so it sits in "to post" and will not be posted. This is the
                            thing on the line that says so, and dropping it was a regression: the cards
                            this replaces drew it, and DEV-892 is why.
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
                    **The files are reference, so they are behind a fold and it is closed.**
                    The size column gives the weight without the list, and up to seven paths
                    per part was half of what the old card printed on every one of them.
                  -->
                  <details
                    v-if="files(row.part).length"
                    class="files-fold"
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
            The findings on a part are the review's own, matched by file — so a finding
            written after this brief appears here without the brief changing, and a finding
            in a file no part names is listed below rather than lost.
          </p>
        </section>

        <!--
          The change in parts, when a session has not written it. Said here rather than by
          replacing the page, because the two lists below are recorded separately and may
          well be there when this is not.

          **The state is named once on this page.** It leads this notice only when the
          heading is the approach verdict and therefore did not say it; where the heading is
          the state, this carries the sentence that follows from it and nothing else. That
          pair — the same words as a heading and as a notice — is what DEV-892's defect 6
          named on this page.
        -->
        <DeskNotice
          v-else
          tone="plain"
        >
          <strong v-if="data.brief.approach_verdict">
            {{ sentence(nothingYet(data.layers, 'solution_map')) }}
          </strong>
          Step 4 is where a session says what was built, as the parts the change divides
          into. {{ whyNoParts(data.layers) }}
        </DeskNotice>

        <!--
          Findings no part accounts for. Not an error, and not hidden: on the review this
          was measured against seven of seventeen had no file or a file in no part, and they
          appeared nowhere on the page at all.
        -->
        <section
          v-if="parts(data.brief).length && inNoPart(data, data.brief).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            {{ noPartHeading(inNoPart(data, data.brief).length) }}
          </h2>
          <p class="rd-note legend">
            They are about the description, the ticket or the release, or they sit in a file
            none of the parts above names. A part can claim a file the pull request never
            changed, so this is a gap in the description rather than a limit of the record.
          </p>
          <ul class="findings findings--loose">
            <li
              v-for="finding in inNoPart(data, data.brief)"
              :key="finding.seq"
            >
              <NuxtLink
                :to="findingLink(data.review.id, finding)"
                class="finding"
              >
                <span class="severity">{{ finding.severity }}</span>
                <span class="rd-mono key">{{ finding.key }}</span>
                <span class="what"><TickedText :text="finding.title ?? finding.claim" /><!--
                  **What became of a carried finding this round.** The counts above group by
                  disposition, and a finding carried in as fixed keeps the disposition it was
                  carried with — so it sits in "to post" and will not be posted. This is the
                  thing on the line that says so, and dropping it was a regression: the cards
                  this replaces drew it, and DEV-892 is why.
                --><span
                  v-if="carriedAfterTitle(finding)"
                  class="carried"
                >{{ carriedAfterTitle(finding) }}</span></span>
                <span class="stands">{{ standsAt(finding) }} →</span>
              </NuxtLink>
            </li>
          </ul>
          <span class="end" />
        </section>

        <!--
          ------------------------- Where the ticket left a choice -------------------------
          **Drawn here and nowhere else.** DEV-885's comment: `open_choices` feeds board
          A5's "Already decided" fold and board A7's table, and they are one list. The table
          is Solution's; Understand carries one line and a link to it.
        -->
        <section
          v-if="(data.brief.open_choices ?? []).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            Where the ticket left a choice
          </h2>
          <DeskTable
            :columns="['The ticket left open', 'Chosen', 'Why', 'Against the ticket']"
            template="1.2fr 1fr 1.5fr 1fr"
            label="Where the ticket left a choice, and what the change chose"
          >
            <tr
              v-for="(choice, index) in data.brief.open_choices"
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
                  Two different things, and the distinction is the column's point. A
                  departure is a decision somebody should look at; filling a gap the ticket
                  left is the change doing its job.
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
                <!--
                  A choice may name a finding that raises it, and need not: an open choice
                  is a choice, not a fault.
                -->
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
        </section>

        <!-- ------------------ Where the description and the code disagree -------------- -->
        <section
          v-if="(data.brief.disagreements ?? []).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            Where the description and the code disagree
          </h2>
          <p class="rd-note legend">
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
              v-for="(clash, index) in data.brief.disagreements"
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
        </section>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.block {
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
 * horizontal scrollbar at 390px. The caption row and every data row share the list, because
 * a caption over a different track is a caption over nothing.
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
 * the size against the middle of the title and its one line — so nothing lined up across a
 * row of seven. Started, the first line of every cell is the first line of the row.
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
 * on `.cell`, and without it the ellipsis never fires — a flex and grid child's minimum is
 * its content, so the cell would widen instead and take the page with it. That is
 * `ProblemTag`'s own lesson, and it is the same lesson here.
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
 * **No colour that differs by severity or by count.** The count is the record's own figure
 * in the body ink and the words beside it are the body ink too: the row says where the
 * review looked, and a cell that painted the blocker red would be telling the reader which
 * part to be afraid of. `nowrap` so "held for a ticket" moves whole to the next line.
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
 * grade the part, which is the thing this section does not do.
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
 * A carried finding's round status, beside its title. Muted, and never wrapping inside
 * itself — it is a phrase, not a sentence — and in the body ink rather than a colour,
 * because like everything else in this section it reports rather than grades.
 *
 * **No `margin-left`.** The space that separates it from the title is the first character
 * of the string — see `carriedAfterTitle` — because a margin moves the box and leaves the
 * text nodes touching, which is what a screen reader and a copy both read.
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

.files-fold summary {
  cursor: pointer;
  font-size: 14px;
  color: var(--rd-accent);
}

.files-fold summary:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
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
 * `anywhere`, not `break-word`. A path has no spaces to break at, and
 * `database/migrations/2026_10_06_000000_add_allowed_columns_to_report_definitions.php` is
 * wider than a phone — so it breaks mid-token or it widens the page.
 */
.path {
  flex: 1 1 auto;
  min-width: 0;
  font-family: var(--rd-font-mono);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

/* ---------------------------------- Narrower than a laptop ------------------------- */

/*
 * At a phone's width the six columns become a stack, which is the code review page's own
 * answer to the same problem: a board has one width and an app has two. The cells keep the
 * order they are read in, and `OpenRow`'s control — auto-placed after them — lands on a
 * line of its own at the foot of the block.
 *
 * **Which is where it should be here, and this comment used to claim the opposite.** An
 * earlier version said the control stayed on the first line beside the number; it never
 * did, and `docs/screenshots/dev-919-1-table-390.png` showed it at the foot the whole
 * time. Left at the foot deliberately: stacked, the row reads number, title, one line,
 * tags, size, counts — and "Open" belongs after what it is opening, not above it. The rule
 * above each row is what separates one from the next.
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
