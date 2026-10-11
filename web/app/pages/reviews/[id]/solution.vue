<script setup lang="ts">
import type { Brief, BriefPart, Finding, ReviewDetail, ReviewLayer } from '~/types/desk'
import type { Absence } from '~/utils/steps'

/**
 * Step 4 — **Solution**. Board A7, which Patrick liked in principle on 10 October 2026 and
 * wanted to see on a live review.
 *
 * The page is the change, in the parts the session chose to describe it in. A part card is
 * readable on its own — what it is, what it fixes, how many files and lines, and what the
 * code review found in it — and opens to the file list and the findings themselves.
 *
 * **The findings on a part are derived, not stored.** `contract/README.md` says so: "the
 * review's findings whose `file` is one of `brief.parts[].files[].path`". That is the join
 * that makes the page worth having — it is the only place a finding is shown next to the
 * part of the change it is about — and it is done here because the contract deliberately
 * does not store it. A stored copy is a copy, and a copy is a thing that can differ.
 *
 * Three consequences of the join being a join:
 *
 * 1. A finding added after the brief was written appears on its part without the brief
 *    being rewritten.
 * 2. A part whose files hold no findings says so. "Nothing found here" is a result.
 * 3. **A finding in no part is still shown**, at the foot, rather than being dropped. A
 *    page that silently lost a finding because no part claimed its file would be worse than
 *    one that had no parts at all — which is the whole reason `in_diff: false` exists.
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

/** The line under the heading: the size of the change, when there is one to measure. */
function sub(brief: Brief): string | null {
  if (!parts(brief).length) return null
  return `${count(allFiles(brief).length, 'file')}, ${linesAdded(brief)} lines added `
    + `and ${linesRemoved(brief)} removed, in ${count(parts(brief).length, 'part')}.`
}

/** Which part is open, by its position in the list. One at a time, as on the board. */
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

/* -------------------------- The findings on a part, derived ---------------------- */

/**
 * The findings whose file one of this part's files names.
 *
 * In the review's own finding order, not the part's file order: a reader works down the
 * findings of a part the way they work down the findings of a review, and F3 before F9
 * everywhere is worth more than grouping by file.
 */
function findingsOn(data: ReviewDetail, part: BriefPart): Finding[] {
  const paths = new Set(files(part).map(file => file.path))
  return data.findings.filter(finding => finding.file != null && paths.has(finding.file))
}

/**
 * Findings in a file no part claims.
 *
 * Shown rather than dropped, and the count is the thing worth knowing: a brief whose parts
 * do not between them account for every finding has described the change incompletely, and
 * that is a fact about the brief rather than about the page.
 */
function findingsInNoPart(data: ReviewDetail, brief: Brief): Finding[] {
  const claimed = new Set(allFiles(brief).map(file => file.path))
  return data.findings.filter(
    finding => finding.file == null || !claimed.has(finding.file),
  )
}

/**
 * What a part's findings say in one line on its card.
 *
 * The severities of the ones the author is going to read, plus a count of the ones they are
 * not. A finding somebody held back is still a finding, and a card that counted it with the
 * rest would overstate what the review says.
 *
 * **`willPost`, not the disposition.** This counted `disposition === 'in_draft'` and called
 * the result `posted`, under a docstring about what the author will read — which is the
 * mistake PR #32 fixed on the code review page's headline and DEV-892's comment of 11
 * October found still standing here. A finding carried into this round as `fixed` keeps
 * whatever disposition it was carried with, so it can sit at `in_draft` and still not be
 * composed into the draft: `DraftSection.posted` requires the disposition **and** an open
 * round status. A card announcing "2 Shoulds" about a finding that was fixed last round is
 * wrong about the only thing it says out loud.
 *
 * The three lists on the code review page stay keyed on the disposition, because that is
 * what the developer decided. This is a claim about what the author receives, which is a
 * different question with a different answer.
 */
function tallyOn(data: ReviewDetail, part: BriefPart) {
  const found = findingsOn(data, part)
  const posting = found.filter(willPost)
  const aside = found.length - posting.length
  const order = ['blocker', 'should', 'question', 'nit'] as const
  return {
    any: found.length > 0,
    severities: order
      .map(severity => ({
        severity,
        n: posting.filter(finding => finding.severity === severity).length,
      }))
      .filter(entry => entry.n > 0),
    aside,
  }
}

/** The words for a severity and a count — "2 Shoulds", "1 Blocker". */
function severityWord(severity: string, n: number): string {
  const LABELS: Record<string, string> = {
    blocker: 'Blocker',
    should: 'Should',
    question: 'Question',
    nit: 'Nit',
  }
  const one = LABELS[severity] ?? severity
  return `${n} ${n === 1 ? one : `${one}s`}`
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

          <div class="parts">
            <template
              v-for="(part, index) in parts(data.brief)"
              :key="index"
            >
              <button
                type="button"
                class="part"
                :class="{ 'part--open': openPart === index }"
                :aria-expanded="openPart === index"
                :aria-controls="`part-${index}`"
                @click="togglePart(index)"
              >
                <span class="part-head">
                  <!--
                    The green numbers this part fixes, in a lane of their own so the title
                    is always beside them. `fixes` is empty for a part that fixes none —
                    tests and generated types are part of a change and fix nothing — and
                    then the lane is not drawn at all rather than a nought being drawn in
                    it.
                  -->
                  <span
                    v-if="part.fixes.length"
                    class="part-marks"
                  >
                    <ProblemMark
                      v-for="number in part.fixes"
                      :key="number"
                      :number="number"
                      tone="fixed"
                      size="box"
                      :says="`this part fixes problem ${number}`"
                    />
                  </span>
                  <span class="part-title">{{ part.title }}</span>
                </span>

                <span class="rd-note part-summary"><TickedText :text="part.summary" /></span>

                <span class="part-figures">
                  <span>{{ count(files(part).length, 'file') }}</span>
                  <span class="added">+{{ part.lines_added }}</span>
                  <span class="removed">−{{ part.lines_removed }}</span>
                </span>

                <span class="part-tally">
                  <template v-if="tallyOn(data, part).any">
                    <DeskPill
                      v-for="entry in tallyOn(data, part).severities"
                      :key="entry.severity"
                      :tone="entry.severity === 'blocker' ? 'ink' : 'quiet'"
                    >{{ severityWord(entry.severity, entry.n) }}</DeskPill>
                    <!--
                      **"Not posting", not "set aside".** The count is everything the author
                      will not read, which since DEV-892 includes a finding still at
                      `in_draft` that this round closed — and nobody set that one aside.
                    -->
                    <span
                      v-if="tallyOn(data, part).aside > 0"
                      class="rd-meta aside"
                    >{{ tallyOn(data, part).aside }} not posting</span>
                  </template>
                  <!--
                    A part the review found nothing in. Said in words, because an empty
                    space here is indistinguishable from a page that failed to join.
                  -->
                  <span
                    v-else
                    class="rd-meta aside"
                  >Nothing found here</span>
                </span>

                <span class="part-opens">
                  {{ openPart === index ? 'Fold' : 'Files and findings' }}
                </span>
              </button>
            </template>
          </div>

          <!--
            **The fold sits under the whole grid, not inside it.**

            Interleaved — the detail immediately after the card that opens it — the card
            grid reflows every time one is opened: a full-width row inserted after the
            first of three cards pushes the other two onto the next line, so the thing you
            just clicked jumps and so does everything beside it. The board puts the cards in
            a grid and the details beneath, and that is why.

            It is still in place: the same page, directly below, and `aria-controls` ties
            the two together for a reader who is not looking at where it landed.
          -->
          <template
            v-for="(part, index) in parts(data.brief)"
            :key="`detail-${index}`"
          >
            <div
              v-if="openPart === index"
              :id="`part-${index}`"
              class="part-detail"
            >
              <div class="detail-side">
                <p class="rd-rubric">
                  {{ part.title }} · the files
                </p>
                <ul class="files">
                  <li
                    v-for="file in files(part)"
                    :key="file.path"
                    class="file"
                  >
                    <span class="path">{{ file.path }}</span>
                    <!--
                        **A file the pull request did not change.** `in_diff: false` is how
                        a part claims one, and it has no lines added or removed because
                        that is what being outside the diff means — so the two figures are
                        replaced by the fact, rather than drawn as a pair of noughts.
                      -->
                    <template v-if="file.in_diff">
                      <span class="added">+{{ file.lines_added }}</span>
                      <span class="removed">−{{ file.lines_removed }}</span>
                    </template>
                    <span
                      v-else
                      class="outside"
                    >not changed by this pull request</span>
                  </li>
                </ul>
              </div>

              <div class="detail-side">
                <p class="rd-rubric">
                  What the code review found here
                </p>
                <ul
                  v-if="findingsOn(data, part).length"
                  class="findings"
                >
                  <li
                    v-for="finding in findingsOn(data, part)"
                    :key="finding.seq"
                    class="finding"
                    :class="{ 'finding--aside': !willPost(finding) }"
                  >
                    <span class="rd-mono key">{{ finding.key }}</span>
                    <SeverityTag :severity="finding.severity" />
                    <span class="claim">
                      <TickedText :text="finding.claim" />
                      <span class="rd-mono where">
                        {{ finding.file }}<template v-if="finding.line">:{{ finding.line }}</template>
                      </span>
                    </span>
                    <DispositionTag
                      v-if="finding.disposition !== 'in_draft'"
                      :disposition="finding.disposition"
                    />
                    <!--
                      A carried finding's round status, drawn the way the code review page
                      draws it. It is what explains the card's count above: a finding at
                      `in_draft` that this round closed is counted as not posting, and this
                      is the only thing on the row that says why.
                    -->
                    <DeskPill
                      v-if="roundStatusLabel(finding)"
                      :tone="isStillOpen(finding.round_status) ? 'info' : 'quiet'"
                    >
                      {{ roundStatusLabel(finding) }}
                    </DeskPill>
                  </li>
                </ul>
                <p
                  v-else
                  class="rd-empty"
                >
                  The review found nothing in this part.
                </p>
              </div>
            </div>
          </template>

          <p class="rd-note legend">
            A green number says which of the problems on <NuxtLink
              :to="`/reviews/${data.review.id}/understand`"
            >the Understand page</NuxtLink> that part fixes. The findings on a part are the
            review's own, matched by file —
            so a finding written after this brief appears here without the brief changing.
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
          Findings no part accounts for. Not an error, and not hidden: a brief whose parts
          do not between them name every file a finding sits in has described the change
          incompletely, and that is worth one section.
        -->
        <section
          v-if="parts(data.brief).length && findingsInNoPart(data, data.brief).length"
          class="block"
        >
          <h2 class="rd-display-lead">
            {{ count(findingsInNoPart(data, data.brief).length, 'finding') }}
            in no part of this description
          </h2>
          <p class="rd-note legend">
            The review found these in files none of the parts above names, or in no file at
            all. A part can claim a file the pull request never changed, so this is a gap in
            the description rather than a limit of the record.
          </p>
          <ul class="findings findings--loose">
            <li
              v-for="finding in findingsInNoPart(data, data.brief)"
              :key="finding.seq"
              class="finding"
              :class="{ 'finding--aside': !willPost(finding) }"
            >
              <span class="rd-mono key">{{ finding.key }}</span>
              <SeverityTag :severity="finding.severity" />
              <span class="claim">
                <TickedText :text="finding.claim" />
                <span
                  v-if="finding.file"
                  class="rd-mono where"
                >
                  {{ finding.file }}<template v-if="finding.line">:{{ finding.line }}</template>
                </span>
                <span
                  v-else
                  class="rd-meta where"
                >in no file</span>
              </span>
              <!--
                **The same two marks as a row inside a part**, and for the same reason. This
                list dimmed by the disposition while the one above it dimmed by `willPost`,
                so a finding carried in as `fixed` and still at `in_draft` was drawn here
                full-strength — as something the author will read — three inches under a
                card counting it as not posting. One page, one vocabulary.
              -->
              <DispositionTag
                v-if="finding.disposition !== 'in_draft'"
                :disposition="finding.disposition"
              />
              <DeskPill
                v-if="roundStatusLabel(finding)"
                :tone="isStillOpen(finding.round_status) ? 'info' : 'quiet'"
              >
                {{ roundStatusLabel(finding) }}
              </DeskPill>
            </li>
          </ul>
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

.legend {
  margin: 0;
  max-width: var(--rd-measure);
}

.end {
  border-top: 1px solid var(--rd-line);
}

/* ---------------------------------- The part cards --------------------------------- */

/*
 * `minmax(min(340px, 100%), 1fr)` is the board's: three cards across at 1200px, two at a
 * tablet, one at a phone — and the `min(…, 100%)` is what keeps the track from being wider
 * than the page at 390px, which is the difference between wrapping and scrolling.
 */
/*
 * **No `align-items: start`, which is the second half of defect 5.** Each card took its own
 * height, so a row of three stood at three heights and left holes in the grid — and the
 * figures, the tally and the control sat at a different place in every card, so there was
 * no line to read across. Stretched, every card in a row is the height of the tallest, and
 * `margin-top: auto` on the figures puts the bottom three lines of all of them level.
 */
.parts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(340px, 100%), 1fr));
  gap: 16px;
  min-width: 0;
}

.part {
  display: flex;
  flex-direction: column;
  gap: 10px;
  align-items: stretch;
  min-width: 0;
  min-height: var(--rd-action-tall);
  padding: 18px 20px;
  border: 1px solid var(--rd-line-strong);
  border-radius: var(--rd-radius);
  background: var(--rd-surface);
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background-color 0.14s ease,
    box-shadow 0.14s ease;
}

@media (prefers-reduced-motion: reduce) {
  .part {
    transition: none;
  }
}

.part:hover {
  background: var(--rd-ground);
  box-shadow: var(--rd-lift-quiet-hover);
}

.part--open {
  background: var(--rd-ground);
}

.part:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

/*
 * **One layout for every card's head, which is the first half of defect 5.** This wrapped:
 * a title that did not fit beside its marks went under them, so on one grid some cards read
 * "2 · Give the export its own context" across and others read the number on one line and
 * the title on the next. The marks are a lane that does not wrap, the title takes the rest
 * and wraps inside itself, and there is one arrangement whatever either of them is.
 *
 * `baseline`, not `center`: a mark is a 26px circle and the title is a line of text that may
 * run to three, and what should line up is the numeral and the title's first line.
 */
.part-head {
  display: flex;
  flex-wrap: nowrap;
  gap: 10px;
  align-items: baseline;
  min-width: 0;
}

/*
 * The marks keep their own lane and wrap as whole circles if a part fixes several.
 *
 * **It does not shrink, and it is capped at half the head.** The two have to be said
 * separately, because each alone is wrong.
 *
 * `flex: none` alone fixes the lane at max-content, so its own `flex-wrap` can never fire
 * and nothing caps how many problems a part may fix: nine marks measure 282px of a 340px
 * card and leave the title six pixels, which is defect 5 again wearing a different hat.
 *
 * Letting it shrink instead is worse in the common case. A shrinkable lane gives up width
 * to a title whose own basis is far larger, collapses to its min-content — one circle — and
 * stacks two marks vertically where they had sat side by side. That was on the Solution
 * picture for one commit.
 *
 * So: no shrink, and a `max-width` that clamps before any shrinking would be needed. Two
 * marks are 58px and stay on one line; nine are capped at half the head and wrap into rows
 * of their own, and the title always has the other half.
 */
.part-marks {
  flex: 0 0 auto;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-width: 50%;
}

.part-title {
  flex: 1 1 auto;
  font-weight: 600;
  font-size: 16px;
  min-width: 0;
  overflow-wrap: break-word;
}

.part-summary {
  overflow-wrap: break-word;
}

/*
 * `margin-top: auto`: the figures, the tally and the control are the bottom block of the
 * card, so they line up across a row however long the title and the summary above them run.
 */
.part-figures {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  align-items: baseline;
  margin-top: auto;
  font-family: var(--rd-font-mono);
  font-size: 13px;
  color: var(--rd-ink-muted);
}

.added {
  color: var(--rd-accent-dark);
}

.removed {
  color: var(--rd-wrong);
}

/* A wrapping row of whole pills: the one rule a pill row has to carry. */
.part-tally {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.aside {
  white-space: nowrap;
}

.part-opens {
  font-size: 14px;
  font-weight: 500;
  color: var(--rd-accent);
  white-space: nowrap;
}

.part:hover .part-opens {
  color: var(--rd-accent-dark);
  text-decoration: underline;
}

/*
 * The fold spans the whole grid rather than sitting inside its card, which is what makes
 * "in place" mean in place: the file list and the findings have room to be read without the
 * card they came from being three columns wide.
 */
.part-detail {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(420px, 100%), 1fr));
  gap: 32px;
  padding: 22px 24px;
  border-radius: var(--rd-radius);
  background: var(--rd-ground);
  min-width: 0;
}

.detail-side {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.detail-side p {
  margin: 0;
}

/* ------------------------------------ The files ------------------------------------ */

.files,
.findings {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

.file {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  align-items: baseline;
  font-family: var(--rd-font-mono);
  font-size: 13px;
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
  overflow-wrap: anywhere;
}

.outside {
  font-family: var(--rd-font-sans);
  font-size: 13px;
  color: var(--rd-ink-muted);
}

/* ---------------------------------- The findings ----------------------------------- */

.finding {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  align-items: baseline;
  min-width: 0;
}

.findings--loose .finding {
  padding: 10px 0;
  border-top: 1px solid var(--rd-line);
}

/*
 * A set-aside finding is quieter, **by colour rather than by opacity**. The board fades it
 * to 0.62, which on `--rd-ink-muted` lands under 4.5:1 — and a finding somebody decided to
 * hold back is still a finding somebody has to be able to read.
 */
.finding--aside .claim {
  color: var(--rd-ink-muted);
}

.key {
  flex: none;
  font-weight: 600;
}

.claim {
  flex: 1 1 240px;
  min-width: 0;
  overflow-wrap: break-word;
}

.where {
  color: var(--rd-ink-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

@media (max-width: 560px) {
  .part-detail {
    padding: 16px;
    gap: 20px;
  }
}
</style>
