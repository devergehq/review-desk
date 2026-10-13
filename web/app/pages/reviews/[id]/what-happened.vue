<script setup lang="ts">
/**
 * Step 7 — **What happened**. DEV-958.
 *
 * ```
 *  The pull request is still open.
 *  Last read 4 days ago. 14 findings were posted. 7 of them had something come
 *  back: 2 replies, 7 whose code changed.
 *
 *  WHAT CAME BACK · 7 of 14 findings
 *  ──────────────────────────────────────────────────────────────────────────────
 *  F1   blocker   What F1 says is wrong                     The finding · On GitHub
 *       app/Reporting/ColumnPolicy.php:47 · Carried from round 1 · Still open
 *         A reply from dana-okoye. 1 day ago
 *         “I pushed a fix for this one but I want to flag that it is not the fix
 *          you suggested…”
 *         The thread was resolved by dana-okoye. seen 13 hours ago
 *         The code at line 47 changed in 65eff61. 5 hours ago
 *  ──────────────────────────────────────────────────────────────────────────────
 *
 *  NOTHING CAME BACK · the other 7 of 14 findings            [ Open the 7 … ]
 *
 *  THE ROUNDS OF THIS PULL REQUEST
 *  Round 1   11 findings · 3 replies · 2 threads resolved
 *  Round 2   14 findings · 2 replies · 1 thread resolved   4 carried in, 1 still open
 * ```
 *
 * The figures are the fixture's — `test/what-happened.nuxt.spec.ts` and
 * `e2e/what-happened.spec.ts` assert every one of them — because a sketch that disagrees
 * with the thing it is a sketch of is worse than none.
 *
 * **Before this page there was nowhere to read what a posted review came to.** The harvest
 * ran every quarter of an hour and wrote its findings into outcomes nothing drew: the round
 * and draft pages said nothing new, the code review page drew a finding's own replies
 * behind a fold on the finding, and the whole of it on the queue was "pull request merged ·
 * replies 2 · code changed since 7 findings" — which Patrick read and said "I don't really
 * know what that means." Step 7 was the one step of seven with no page.
 *
 * ## It describes; it does not grade
 *
 * The issue's own words, and three rules come out of it. `whatHappened` carries the
 * argument; what is here is the drawing:
 *
 * 1. **No good or bad colour anywhere on the page.** A severity is the record's own word in
 *    the body ink — no `SeverityTag`, no wash. `HeldBackRows` made the same call for the
 *    same reason: a colour tells a reader which of these to be afraid of, and that is a
 *    judgement about what came back rather than a report of it.
 * 2. **A finding nothing came back on says so.** It is a row in a folded group with a
 *    count on the heading, never a blank cell — "0 replies" and "nobody has looked" are
 *    different facts and this page has to be able to say which.
 * 3. **The author's words are quoted whole and interpolated.** Not clamped, not summarised,
 *    not `v-html`. The one reply in the fixtures runs to 108 words because that is the
 *    length an author really answers a blocker at, and the page is measured at 390 with it.
 *
 * ## Two reads? No — one
 *
 * `GET /reviews/{id}` is the whole of it: the findings, the outcomes, the pull request's
 * state and the rounds. The draft is not read, because nothing here is decided, and
 * **nothing new is read from GitHub** — both out of DEV-958's scope. That is why this page
 * has no second resource to fail and no notice about one.
 */
import type { FindingRow } from '~/utils/whatHappened'

const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.review(reviewId)

/**
 * The whole page, worked out once.
 *
 * **Once, and that is not a performance note.** `whatHappened` words every outcome and
 * `outcomeWhen` defaults to the clock, so calling it from each of a dozen places in the
 * markup would be a dozen readings of the clock in one render — the measurement is in
 * `StepTrail`, where a title and a card built from two calls to it could say "just now"
 * and "1 min ago" about the same step. The counts would be free to disagree with the rows
 * they are counting, too, which is this issue's own defect one level up.
 *
 * Null while the read is in flight; `DeskState` draws its own state then and the slot
 * below is not rendered. The computeds under it carry the empty answer so that nothing in
 * the template has to assert a non-null.
 */
const page = computed(() => {
  const data = detail.data.value
  return data == null ? null : whatHappened(data)
})

const headline = computed(() => page.value?.headline ?? '')
const sub = computed(() => page.value?.sub ?? '')
const harvested = computed(() => page.value?.harvested ?? false)
const answered = computed(() => page.value?.answered ?? [])
const silent = computed(() => page.value?.silent ?? [])
const rounds = computed(() => page.value?.rounds ?? [])
const anyObserved = computed(() => page.value?.anyObserved ?? false)
const anyWithoutComment = computed(() => page.value?.anyWithoutComment ?? false)
const anyUnaddressable = computed(() => page.value?.anyUnaddressable ?? false)
const noPullRequestAddress = computed(() => page.value?.noPullRequestAddress ?? false)
const elsewhere = computed(() => page.value?.elsewhere ?? 0)

/** How many findings this review posted: the denominator both section headings count out of. */
const posted = computed(() => answered.value.length + silent.value.length)

useHead({
  title: computed(() => {
    const review = detail.data.value?.review
    return review
      ? `What happened · #${review.pr} · Review Desk`
      : 'What happened · Review Desk'
  }),
})

/**
 * Which finding's folded row is open, by its sequence number.
 *
 * One at a time, which is the rule every fold on these pages follows. The sequence number
 * rather than the position, because `aria-controls` keys on it and the groups do not draw
 * the rows in the record's order.
 */
const open = ref<number | null>(null)

/** Whether the "nothing came back" group is unfolded. Closed on arrival: it is the quiet half. */
const showSilent = ref(false)

function toggle(seq: number) {
  open.value = open.value === seq ? null : seq
}

/**
 * Fold the quiet half, and let go of whichever row was open inside it.
 *
 * Without the second half, folding and unfolding brought back a row already open — the
 * reader closed the group and found it reopened at the row they had left, which reads as
 * the control having done nothing.
 */
function toggleSilent() {
  showSilent.value = !showSilent.value
  if (!showSilent.value) open.value = null
}

/**
 * Another review on the same page starts folded, with nothing open.
 *
 * **The component is reused across a parameter change**, which is the router's own
 * behaviour and not a thing to work around — but the two pieces of state here are about
 * *this* review's rows, and `open` is a sequence number, so a row left open on review 50
 * came back open on review 51 over whichever finding happened to share the number.
 */
watch(reviewId, () => {
  open.value = null
  showSilent.value = false
})

/**
 * The row's name for a screen reader.
 *
 * **`spoken`, not the raw line**, for the reason `HeldBackRows` carries: `OpenRow`'s
 * accessible name is a plain string and the line is recorded prose, so every rule
 * `TickedText` applies to the drawing has to reach the name as well — or a commit id is
 * shown as seven characters and announced as forty.
 */
function said(row: FindingRow): string {
  return `${row.key}, ${row.severity}, ${spoken(row.line)}`
}

function detailId(seq: number): string {
  return `nothing-back-${seq}`
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this review"
  >
    <template #default="{ data }">
      <ReviewStepPage
        :detail="data"
        step="harvest"
        :headline="headline"
        :sub="sub"
      >
        <!--
          **A review nothing has read says so once, and in the tone of an absence.**
          `prEndLabel` has already said it in the headline; this says what follows from it,
          which is that every figure below would be a figure about nothing. `plain`, not
          `problem`: nobody has done anything wrong, and the harvest is a sweep rather than
          something the developer runs.
        -->
        <DeskNotice
          v-if="!harvested"
          tone="plain"
        >
          <strong>No harvest has read this review back from GitHub.</strong>
          Whatever the author has said or pushed since it was posted is on the pull request
          and not yet on this record, so the rows below are what Review Desk holds rather
          than what is there — and anything in them was recorded by hand.
        </DeskNotice>

        <!-- ------------------------------ What came back ------------------------------ -->
        <DeskSection
          heading="What came back"
          :note="answered.length === 0 ? 'nothing yet' : `${answered.length} of ${posted} findings`"
        >
          <!--
            Nothing yet, which is most posted reviews most of the time. It is a sentence and
            not an empty table: a table with a head and no rows reads as a load that failed.
          -->
          <p
            v-if="answered.length === 0"
            class="rd-note none"
          >
            Nothing has come back on any of the findings this review posted.
            <template v-if="harvested">
              The harvest has read the pull request and found no reply, no resolved thread
              and no change to the code the findings point at.
            </template>
          </p>

          <ul
            v-else
            class="answered"
          >
            <li
              v-for="row in answered"
              :key="row.seq"
              class="answered-row"
            >
              <div class="head">
                <span class="rd-mono rd-meta key">{{ row.key }}</span>
                <!--
                  **The severity as a word, in the body ink.** Rule 1 at the top of this
                  file. The record's own word, lower case as the record holds it.
                -->
                <!--
                  **`{{ ' ' }}`, outside the clipped box — and it is the third form tried.**

                  `rd-visually-hidden` is text in the document, so a label and its value
                  with nothing between them read as one word: `Severity:blocker`. A newline
                  between them is no better, because Vue's `whitespace: 'condense'` deletes
                  a whitespace-only text node between two elements. `&#32;` looked like the
                  answer and is one only where nothing follows it on the line: compiled
                  with the repo's own `@vue/compiler-dom`, `</span>&#32;` followed by a
                  newline and an element — or a *comment* — emits no text node at all, and
                  four boundaries in this app were written exactly that way. An
                  interpolated space is a text node wherever it is written, which is why it
                  is this one.

                  Outside the clipped box rather than inside it, because
                  `.rd-visually-hidden` is a 1×1 `overflow: hidden` box and a trailing
                  space at the end of one is the kind of character a renderer may drop.
                  Visually it costs nothing: the box is `position: absolute`, so the space
                  is the first in-flow content of the line and is collapsed away.
                -->
                <span class="rd-meta severity">
                  <span class="rd-visually-hidden">Severity:</span>{{ ' ' }}{{ row.severity }}
                </span>
                <span class="line">
                  <TickedText :text="row.line" />{{ ' ' }}
                  <!--
                    A line the page cut says so, with the whole claim within a pointer's
                    reach. `HeldBackRows`' rule: a reader has to know the line they are
                    reading was cut by the page and is not what the review wrote.

                    The space before it is the rule this file argues for, inside a cell
                    rather than between two: without it the row's text read
                    `…a session really…shortened`.
                  -->
                  <span
                    v-if="row.shortened"
                    class="rd-meta shortened"
                  >shortened</span>
                </span>
                <!--
                  **The two links the issue asks a row for.** The finding on the code review
                  page, which always exists; and the comment on GitHub, which is drawn only
                  where an address can be built — see `commentUrl`, and the footnote at the
                  foot of this page for what the absence means.
                -->
                <span class="out">
                  <NuxtLink
                    class="to-finding"
                    :to="row.finding"
                  >The finding<span class="rd-visually-hidden"> {{ row.key }} on the code review page</span></NuxtLink>
                  <a
                    v-if="row.comment"
                    class="to-finding"
                    :href="row.comment"
                    target="_blank"
                    rel="noopener noreferrer"
                  >On GitHub<span class="rd-visually-hidden">: {{ row.key }}’s comment, opens in a new tab</span></a>
                </span>
              </div>

              <!--
                **Mono only where it is a path.** `whereOf` answers "The pull request
                itself" for a finding that names no file, and a sentence set in JetBrains
                Mono reads as a path that happens to have spaces in it.
              -->
              <p class="rd-meta where">
                <span :class="{ 'rd-mono': row.inCode }">{{ row.where }}</span>
                <template v-if="row.carried">
                  · {{ row.carried }}
                </template>
              </p>

              <!--
                What came back, oldest first. One block each: the sentence, when it happened,
                and the author's words where the outcome carries them.
              -->
              <ol class="back">
                <li
                  v-for="came in row.cameBack"
                  :key="came.id"
                  class="came"
                >
                  <p class="came-said">
                    {{ came.sentence }}
                    <span class="rd-meta when">{{ came.when }}</span>
                    <span
                      v-if="came.file"
                      class="rd-mono rd-meta came-file"
                    >{{ came.file }}</span>
                  </p>
                  <!--
                    **The author's words, whole and interpolated.** `rd-recorded` keeps the
                    paragraph breaks the author typed and holds the line to a readable
                    measure; nothing is handed to the browser as markup, so a reply carrying
                    a backtick, a tag or GitHub's own markdown arrives as what was typed.
                  -->
                  <blockquote
                    v-if="came.quote"
                    class="rd-recorded quote"
                  >
                    {{ came.quote }}
                  </blockquote>
                </li>
              </ol>
            </li>
          </ul>
        </DeskSection>

        <!-- -------------------------- Nothing came back on ---------------------------- -->
        <!--
          **Counted and folded**, which is the issue's own criterion. Closed on arrival: on
          a real review this is most of the findings, and a reader who opened the page to
          find out what came back should not have to scroll past what did not.
        -->
        <DeskSection
          v-if="silent.length > 0"
          heading="Nothing came back"
          :note="`the other ${silent.length} of ${posted} findings`"
        >
          <p class="rd-note none">
            No reply, no resolved thread and no change to the code these findings point at.
            <template v-if="!harvested">
              Nothing has looked, so this is an absence of a reading rather than a reading.
            </template>
          </p>

          <button
            type="button"
            class="rd-btn fold"
            :aria-expanded="showSilent"
            aria-controls="nothing-back"
            @click="toggleSilent()"
          >
            {{ showSilent ? 'Fold' : 'Open' }} the
            {{ silent.length }} findings nothing came back on
          </button>

          <div
            v-if="showSilent"
            id="nothing-back"
            class="rows"
          >
            <template
              v-for="row in silent"
              :key="row.seq"
            >
              <OpenRow
                :open="open === row.seq"
                :controls="detailId(row.seq)"
                opens="Open"
                :label="said(row)"
                class="silent-row"
                @toggle="toggle(row.seq)"
              >
                <!--
                  **A space between the cells as well as inside them.** They are separated
                  by newlines, which `whitespace: 'condense'` deletes, so the row's text —
                  which is its accessible name — read `F4Severity: shouldWhat F4 says…`.
                  Measured by reading a row's `textContent` out of the real binary, after
                  fixing the label's own boundary and finding the cells still closed up.
                -->
                <span class="cell cell--key rd-mono rd-meta">{{ row.key }}</span>{{ ' ' }}
                <span class="cell cell--severity rd-meta">
                  <span class="rd-visually-hidden">Severity:</span>{{ ' ' }}{{ row.severity }}
                </span>{{ ' ' }}
                <span class="cell cell--line">
                  <TickedText
                    :text="row.line"
                    in-control
                  />{{ ' ' }}
                  <!--
                    **A line the page cut says so here too.** It is the same rule as the
                    row above, and it was only on the open half until the browser showed
                    F4 — the one finding with no title — reading as a sentence that
                    trailed off for no reason a reader could see.
                  -->
                  <span
                    v-if="row.shortened"
                    class="rd-meta shortened"
                  >shortened</span>
                </span>
              </OpenRow>

              <div
                v-if="open === row.seq"
                :id="detailId(row.seq)"
                class="silent-opened"
              >
                <p class="rd-meta where">
                  <span :class="{ 'rd-mono': row.inCode }">{{ row.where }}</span>
                  <template v-if="row.carried">
                    · {{ row.carried }}
                  </template>
                </p>
                <!--
                  **Three sentences, because there are three reasons nothing came back**,
                  and the row said the third about the second as well — which is false of
                  it and tells the reader to stop looking. A finding raised in the body
                  alone has no thread at all; a finding whose comment is recorded under an
                  id no address can be built from has one, and it is reachable on the pull
                  request.
                -->
                <p class="rd-note">
                  Nothing has come back on {{ row.key }}.
                  <template v-if="row.comment">
                    It was posted as a comment, so a reply would be read.
                  </template>
                  <template v-else-if="row.postedAsComment && row.addressable">
                    It was posted as a comment, so a reply would be read — but the id
                    recorded for that comment is not one this page can build an address
                    from, so there is no link to it here.
                  </template>
                  <!--
                    **And the repository name can be the thing that failed**, which is
                    neither the finding's fault nor its comment id's: a released Review
                    Desk could store a `repo` that `prUrl` refuses, and every comment
                    address on this page is built on top of the pull request's.
                  -->
                  <template v-else-if="row.postedAsComment">
                    It was posted as a comment, so a reply would be read. There is no link
                    to it here because this review's repository name is not one a GitHub
                    address can be built from.
                  </template>
                  <template v-else>
                    No comment on GitHub is recorded against it, so there is no thread for a
                    reply to attach to and none could be read.
                  </template>
                </p>
                <p class="out out--opened">
                  <NuxtLink
                    class="to-finding"
                    :to="row.finding"
                  >The finding<span class="rd-visually-hidden"> {{ row.key }} on the code review page</span></NuxtLink>
                  <a
                    v-if="row.comment"
                    class="to-finding"
                    :href="row.comment"
                    target="_blank"
                    rel="noopener noreferrer"
                  >On GitHub<span class="rd-visually-hidden">: {{ row.key }}’s comment, opens in a new tab</span></a>
                </p>
              </div>
            </template>
            <span class="end" />
          </div>
        </DeskSection>

        <!--
          **What the record holds about findings this page draws no row for.**
          The harvester writes against any finding of the review — `harvest_code_changes`
          iterates all of them and asks only for a file — so a held-back nit or a finding
          carried in as `fixed` can carry an outcome, and this page is about what came back
          on what was *posted*. Counting those into the figures above would put numbers on
          screen about rows that are not there; saying nothing would lose a measurement. So
          they are counted here, and sent to the page that owns the findings.
        -->
        <DeskSection
          v-if="elsewhere > 0"
          heading="Outcomes on findings this page does not list"
          :note="count(elsewhere, 'outcome')"
        >
          <!--
            **The number is on the heading and not again here.** `HeldBackRows` carries the
            measurement: its own heading said "4 nits" over a group heading that said "4
            nits" too, three lines apart, and a reader met the same three words twice.
          -->
          <p class="rd-note none">
            Against findings this review did not post — a nit held back for a ticket, one
            set aside, or one carried into this round already fixed. They are not counted
            in the figures above, which are about the findings that were posted.
            <NuxtLink :to="`/reviews/${reviewId}/code-review`">
              The code review page
            </NuxtLink>
            lists every finding this round holds.
          </p>
        </DeskSection>

        <!-- ------------------------------- The rounds -------------------------------- -->
        <!--
          **Drawn only where the pull request has more than one round.** A timeline of one
          round restates the page's own heading, and every review is a round.
        -->
        <DeskSection
          v-if="rounds.length > 0"
          heading="The rounds of this pull request"
          :note="`${rounds.length} rounds`"
        >
          <DeskTable
            :columns="['Round', 'What it held', 'Carried in']"
            template="minmax(0, 1fr) minmax(0, 3fr) minmax(0, 2fr)"
            label="Every round of this pull request, and what each carried from the one before"
          >
            <tr
              v-for="line in rounds"
              :key="line.round"
              role="row"
              class="row"
            >
              <td
                role="cell"
                class="td td--strong"
                data-label="Round"
              >
                Round {{ line.round }}
                <span
                  v-if="line.here"
                  class="rd-meta"
                >· this one</span>
              </td>
              <td
                role="cell"
                class="td"
                data-label="What it held"
              >
                {{ line.said }}
              </td>
              <td
                role="cell"
                class="td"
                data-label="Carried in"
              >
                <template v-if="line.carried">
                  {{ line.carried }}
                </template>
                <span
                  v-else
                  class="rd-meta"
                >nothing — every finding was raised in it</span>
              </td>
            </tr>
          </DeskTable>
        </DeskSection>

        <!-- ------------------------------- The footnotes ------------------------------ -->
        <!--
          **Said once, at the foot, rather than on every row.** Two things a reader needs to
          know to read the rows above correctly, and both are properties of what GitHub
          gives rather than of any one finding.
        -->
        <footer
          v-if="anyObserved || anyWithoutComment || anyUnaddressable || noPullRequestAddress"
          class="notes"
        >
          <p
            v-if="anyObserved"
            class="rd-meta"
          >
            <strong>“Seen” rather than a time.</strong>
            GitHub does not record when a review thread was resolved, so the time against a
            resolution above is when the harvest looked. It may have happened well before.
          </p>
          <p
            v-if="anyWithoutComment"
            class="rd-meta"
          >
            <strong>A finding raised in the body alone.</strong>
            A finding this review did not post as its own comment has no thread, so no
            reply could be read on it and nothing here is a link to one.
            <!--
              **And only where there is one to point at.** The note below says when the
              repository name is one no GitHub address can be built from, and promising a
              link at the top of the page that is drawn as plain text there would be the
              page contradicting itself two paragraphs apart.
            -->
            <template v-if="!noPullRequestAddress">
              The pull request itself is linked at the top of this page.
            </template>
          </p>
          <p
            v-if="noPullRequestAddress"
            class="rd-meta"
          >
            <strong>No link to GitHub.</strong>
            This review's repository is recorded under a name no GitHub address can be
            built from, and every address on this page that leads to GitHub is built on top
            of the pull request's.
            <!--
              **And only where there is a row to say it of.** This note is drawn for a
              review with no posted finding at all — that is what reading it off the
              review rather than off the rows is for — and promising links on rows that
              are not on the screen is the same shape as the sentence above it.
            -->
            <template v-if="posted > 0">
              The links to each finding's own row on the code review page are unaffected.
            </template>
            Nothing is missing from the record; what is missing is somewhere on GitHub to
            point at.
          </p>
          <p
            v-if="anyUnaddressable"
            class="rd-meta"
          >
            <strong>A comment with no address.</strong>
            A comment is linked where the record holds GitHub's own numeric comment id for
            it. One of these findings was posted as a comment under an id of another kind,
            which names no place on the pull request — so the comment exists and this page
            does not link it rather than linking somewhere else.
          </p>
        </footer>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
/* ----------------------------- What came back ------------------------------ */

.answered {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/*
 * One rule between rows and none above the first, so the section's own ruled heading is the
 * top line. The board's arrangement everywhere else on these pages.
 */
.answered-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 16px 0 18px;
  border-bottom: 1px solid var(--rd-line);
  min-width: 0;
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 12px;
  min-width: 0;
}

.key {
  flex: none;
}

/*
 * **The severity, as a word.** No wash, no border, no colour that differs by severity —
 * rule 1 at the top of this file. `min-width` so the column of them lines up down the page
 * at a width where they do not wrap.
 */
.severity {
  flex: none;
  min-width: 62px;
}

.line {
  flex: 1 1 22ch;
  min-width: 0;
  font-size: 17px;
  overflow-wrap: break-word;
}

.shortened {
  margin-left: 8px;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.out {
  display: flex;
  flex: none;
  flex-wrap: wrap;
  gap: 4px 16px;
  margin-left: auto;
}

.out--opened {
  margin: 8px 0 0;
}

/*
 * **Underlined, not accent-coloured alone.** `ReviewStepPage` carries the same rule for the
 * two links in the header and the same measurement behind it: at this size a colour change
 * is the one cue a reader can miss, so the link carries the cue that survives being small
 * and being printed.
 */
.to-finding {
  color: var(--rd-accent);
  font-size: 13px;
  text-decoration: underline;
  text-underline-offset: 2px;
  white-space: nowrap;
}

.where {
  margin: 0;
  overflow-wrap: anywhere;
}

/* ------------------------- What came back, per outcome --------------------- */

.back {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
  min-width: 0;
}

/*
 * A quiet rule to the left of each outcome, so several under one finding read as a list of
 * things that happened rather than as more of the finding. The ink line and not a severity
 * colour, which is rule 1 again.
 */
.came {
  padding-left: 14px;
  border-left: 2px solid var(--rd-line-strong);
  min-width: 0;
}

.came-said {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 10px;
  margin: 0;
  font-size: 15px;
  min-width: 0;
}

.when {
  flex: none;
  font-variant-numeric: tabular-nums;
}

.came-file {
  overflow-wrap: anywhere;
  min-width: 0;
}

/*
 * The author's words. Washed and inset so the quotation is visibly a quotation, with no
 * colour that says anything about what was said — the code review page draws a reply the
 * same way, and this is that block at the length a real reply runs to.
 */
.quote {
  margin: 8px 0 0;
  padding: 10px 14px;
  border-left: 3px solid var(--rd-line-strong);
  border-radius: 0 var(--rd-radius-control) var(--rd-radius-control) 0;
  background: var(--rd-ground);
  font-size: 15px;
}

/* --------------------------- Nothing came back ----------------------------- */

.none {
  margin: 0 0 14px;
  max-width: var(--rd-measure);
}

/*
 * The fold's own control. `rd-btn` carries the 44px floor every pressable thing on this
 * desk clears; `test/contrast.spec.ts` holds the token it reads that from.
 */
.fold {
  align-self: flex-start;
}

.rows {
  display: flex;
  flex-direction: column;
  margin-top: 16px;
  min-width: 0;
}

/*
 * `OpenRow` draws the grid of a row; the columns are the caller's, as they are on the
 * draft page's panel. Key, severity, line — and the control's own "Open" at the end, which
 * `OpenRow` appends.
 */
.silent-row {
  grid-template-columns: 4ch 7ch minmax(0, 1fr) auto;
}

.silent-opened {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 16px 16px;
  border-top: 1px solid var(--rd-line);
  background: var(--rd-ground);
  min-width: 0;
}

.cell {
  min-width: 0;
}

.cell--line {
  overflow-wrap: break-word;
}

/* The rule under the last row, which a row cannot draw without doubling every other. */
.end {
  border-top: 1px solid var(--rd-line);
}

/* -------------------------------- The rounds ------------------------------- */

.td {
  overflow-wrap: break-word;
}

.td--strong {
  font-weight: 500;
}

/* ------------------------------- The footnotes ----------------------------- */

.notes {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 18px;
  border-top: 1px solid var(--rd-line);
  max-width: var(--rd-measure);
}

/* ---------------------------------- Narrow --------------------------------- */

/*
 * At a phone's width the two links leave the row's first line — `margin-left: auto` has
 * nothing to push against once the line has wrapped — and the severity's reserved column
 * is 62px of nothing. Measured at 390, which is where the one 108-word reply in the
 * fixtures is read.
 */
@media (max-width: 560px) {
  .out {
    margin-left: 0;
  }

  .severity {
    min-width: 0;
  }

  .line {
    flex-basis: 100%;
    font-size: 16px;
  }

  .silent-row {
    grid-template-columns: 4ch minmax(0, 1fr) auto;
  }

  /*
   * The severity leaves the narrow row rather than squeezing the line into nothing. It is
   * in the row's accessible name either way — `said()` puts it there — and it is on the
   * detail the row opens.
   */
  .silent-row .cell--severity {
    display: none;
  }

  .quote {
    padding: 10px 12px;
  }
}
</style>
