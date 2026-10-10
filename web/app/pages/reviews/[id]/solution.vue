<script setup lang="ts">
import type { Brief, BriefPart, Finding, ReviewDetail } from '~/types/desk'

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
 * The severities of the ones that are going in the draft, plus a count of the ones that are
 * not — held back or suppressed. A set-aside finding is still a finding, and a card that
 * counted it with the rest would overstate what the author will read.
 */
function tallyOn(data: ReviewDetail, part: BriefPart) {
  const found = findingsOn(data, part)
  const posted = found.filter(finding => finding.disposition === 'in_draft')
  const aside = found.length - posted.length
  const order = ['blocker', 'should', 'question', 'nit'] as const
  return {
    any: found.length > 0,
    severities: order
      .map(severity => ({
        severity,
        n: posted.filter(finding => finding.severity === severity).length,
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
      <ReviewStepPage
        v-if="!data.brief || !parts(data.brief).length"
        :detail="data"
        step="solution_map"
        headline="The change has not been described in parts yet."
        sub="Step 4 is where a session says what was built, as the parts the change divides into. Nothing has written any."
      />

      <ReviewStepPage
        v-else
        :detail="data"
        step="solution_map"
        :headline="data.brief.approach_verdict ?? 'What was built'"
        :sub="`${count(allFiles(data.brief).length, 'file')}, ${linesAdded(data.brief)} lines added and ${linesRemoved(data.brief)} removed, in ${count(parts(data.brief).length, 'part')}.`"
      >
        <!-- ------------------------------ What was built ------------------------------ -->
        <section class="block">
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
                    The green numbers this part fixes. `fixes` is empty for a part that
                    fixes none — tests and generated types are part of a change and fix
                    nothing — and then there is no mark rather than a nought.
                  -->
                  <ProblemMark
                    v-for="number in part.fixes"
                    :key="number"
                    :number="number"
                    tone="fixed"
                    size="box"
                    :says="`this part fixes problem ${number}`"
                  />
                  <span class="part-title">{{ part.title }}</span>
                </span>

                <span class="rd-note part-summary">{{ part.summary }}</span>

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
                    <span
                      v-if="tallyOn(data, part).aside > 0"
                      class="rd-meta aside"
                    >{{ tallyOn(data, part).aside }} set aside</span>
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
                    :class="{ 'finding--aside': finding.disposition !== 'in_draft' }"
                  >
                    <span class="rd-mono key">{{ finding.key }}</span>
                    <SeverityTag :severity="finding.severity" />
                    <span class="claim">
                      {{ finding.claim }}
                      <span class="rd-mono where">
                        {{ finding.file }}<template v-if="finding.line">:{{ finding.line }}</template>
                      </span>
                    </span>
                    <DispositionTag
                      v-if="finding.disposition !== 'in_draft'"
                      :disposition="finding.disposition"
                    />
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
          Findings no part accounts for. Not an error, and not hidden: a brief whose parts
          do not between them name every file a finding sits in has described the change
          incompletely, and that is worth one section.
        -->
        <section
          v-if="findingsInNoPart(data, data.brief).length"
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
              :class="{ 'finding--aside': finding.disposition !== 'in_draft' }"
            >
              <span class="rd-mono key">{{ finding.key }}</span>
              <SeverityTag :severity="finding.severity" />
              <span class="claim">
                {{ finding.claim }}
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
              class="row"
            >
              <td
                class="td"
                data-label="The ticket left open"
              >
                {{ choice.left_open }}
              </td>
              <td
                class="td td--strong"
                data-label="Chosen"
              >
                {{ choice.chosen }}
              </td>
              <td
                class="td td--muted"
                data-label="Why"
              >
                {{ choice.why }}
              </td>
              <td
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
              class="row"
            >
              <td
                class="td td--strong"
                data-label="Finding"
              >
                <span class="rd-mono">{{ clash.finding_key }}</span>
              </td>
              <td
                class="td"
                data-label="The description says"
              >
                {{ clash.description_says }}
              </td>
              <td
                class="td"
                data-label="The code does"
              >
                {{ clash.code_does }}
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
.parts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(340px, 100%), 1fr));
  gap: 16px;
  align-items: start;
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

.part-head {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
  min-width: 0;
}

.part-title {
  font-weight: 600;
  font-size: 16px;
  min-width: 0;
  overflow-wrap: break-word;
}

.part-summary {
  overflow-wrap: break-word;
}

.part-figures {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  align-items: baseline;
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
