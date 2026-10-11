<script setup lang="ts">
import type { DraftDetail, ReviewEvent } from '~/types/desk'
import type { WriteResult } from '~/composables/useReviewDesk'
import type { DeskNotice } from '~/utils/format'

/**
 * Step 6 — **the draft**: what GitHub will show, before anything is posted.
 *
 * Nothing on this screen posts. The developer signs off, and the review session posts it
 * under their name with no bot banner — a preview that could post would be a bot.
 *
 * ## It is inside the frame now, and that is what killed a dead end
 *
 * DEV-878: "The draft screen is drawn inside the frame with the trail, and the trail's step
 * 5 and step 6 lead to these pages." So this reads two things: the review, for the frame and
 * the trail, and the draft, for everything below them.
 *
 * **The draft read is allowed to fail.** `GET …/draft` refuses when no session has written
 * one, and that used to make step 6 a label rather than a link — a link would have landed on
 * a blocking not-found panel carrying no trail and no way back. Inside the frame there is no
 * such panel: the trail is drawn either way and this page says, in its own words, that there
 * is nothing to read yet. The guard in `StepTrail` went with the dead end it was for.
 *
 * So the review is the resource `DeskState` renders and the draft is read beside it.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const review = desk.review(reviewId)
const detail = desk.draft(reviewId)

useHead({
  title: computed(() =>
    detail.data.value ? `Draft · #${detail.data.value.review.pr} · Review Desk` : 'Draft · Review Desk',
  ),
})

/**
 * The page's one heading: where the draft stands, in a few words.
 *
 * Short, because the sentence under it says the rest — `statusLine` below is what this page
 * has always said and is unchanged. A heading that repeated it would be the same sentence
 * at two sizes.
 */
const heading = computed(() => {
  if (detail.error.value?.code === 'not_found') return 'No draft yet.'
  const status = detail.data.value?.draft.status
  if (status === 'approved') return 'Signed off.'
  if (status === 'posted') return 'Posted.'
  if (status === 'sent_back') return 'Sent back.'
  if (status === 'drafting') return 'Still being written.'
  return 'Read it, then sign it off.'
})

type View = 'rendered' | 'markdown' | 'edit'
const view = ref<View>('rendered')
const views: Array<{ key: View, label: string }> = [
  { key: 'rendered', label: 'Rendered' },
  { key: 'markdown', label: 'Markdown' },
  { key: 'edit', label: 'Edit' },
]

const editing = ref('')
const event = ref<ReviewEvent>('request_changes')
const note = ref('')
const sendingBack = ref(false)
const working = ref(false)
const notice = ref<DeskNotice | null>(null)

/**
 * The body the editor last loaded, which is what a save compares against.
 *
 * Kept apart from `editing`, because a refusal must not lose what the developer typed:
 * when a save is refused, `editing` keeps their text and this keeps what the record still
 * holds, so the next attempt is still an edit of the same thing.
 */
const loadedBody = ref('')

watch(
  () => detail.data.value,
  (value) => {
    if (!value) return
    /*
     * The editor is only re-seeded while it has nothing unsaved in it. A write refreshes
     * every open read, so a refused save re-runs this — and overwriting the textarea here
     * would discard the edit the refusal was about.
     *
     * One condition, not two. An `editing.value === ''` arm looked like it covered the
     * first load, which the comparison below already does — both start empty — and what it
     * really covered was a textarea the developer had deliberately cleared before pasting a
     * rewrite. Any write from elsewhere on the screen then refilled it with the old body.
     */
    if (editing.value === loadedBody.value) {
      editing.value = value.draft.body
    }
    loadedBody.value = value.draft.body
    if (value.draft.event) event.value = value.draft.event
  },
  { immediate: true },
)

const dirty = computed(() => editing.value !== loadedBody.value)

/**
 * The aside's two derived lists, computed rather than called from the template.
 *
 * **The editor is in the same slot.** `v-model="editing"` on the textarea is tracked by the
 * slot's render effect, so a call in the template re-ran both derivations on every keystroke
 * — and, because each handed back a fresh array, re-rendered the whole held-back panel with
 * them. Measured by review at 76µs a keystroke against 7µs from a stable identity: sub-
 * millisecond either way and nothing a reader could feel, which is why this is three lines
 * and not a component.
 *
 * They read the two resources rather than the slot's `data`, which is the same object: the
 * slot cannot be closed over, and it does not need to be.
 */
const editRows = computed(() => editLines(detail.data.value?.edits ?? []))

const heldRows = computed(() =>
  heldBackRows(detail.data.value?.held_back ?? [], review.data.value?.findings ?? []),
)

function decided(data: DraftDetail): boolean {
  return data.draft.status !== 'ready'
}

function said(result: WriteResult): boolean {
  notice.value = noticeFor(result)
  return result.ok
}

async function save() {
  const data = detail.data.value
  if (!data) return
  working.value = true
  const result = await desk.saveDraft(data.draft.review_id, loadedBody.value, editing.value)
  working.value = false
  if (said(result)) view.value = 'rendered'
}

async function approve() {
  const data = detail.data.value
  if (!data) return
  working.value = true
  said(await desk.approveDraft(data.draft.review_id, event.value))
  working.value = false
}

async function sendBack() {
  const data = detail.data.value
  if (!data) return
  working.value = true
  const result = await desk.sendBackDraft(data.draft.review_id, note.value)
  working.value = false
  if (said(result)) {
    sendingBack.value = false
    note.value = ''
  }
  /* A refused send-back keeps the note: the developer wrote it, and it is not wrong. */
}

async function fileTicket() {
  const data = detail.data.value
  if (!data) return
  working.value = true
  said(await desk.requestNitTicket(data.draft.review_id))
  working.value = false
}

/** Whether the developer is being asked why the rest is going to a ticket. */
const ticketingRest = ref(false)
const ticketNote = ref('')

/**
 * "Ticket what is left, and sign off" — one act, and a sign-off.
 *
 * The contract calls the operation `ticketRestAndApprove` and its summary says "approve",
 * which is the GitHub event. **The button does not**, for the reason the whole screen avoids
 * the word: the developer signs off, and the session posts what they signed off as the event
 * named in the control. A button reading "and approve" beside one reading "Sign off and
 * post" would say two different things about the same act.
 *
 * **It neither posts nor files anything.** Every finding the draft would have posted is held
 * back and recorded as ticketed this round, the one ticket is asked for, the body is
 * recomposed without them, and the draft is approved as `approve` — which is not a choice,
 * because a request for changes that ticketed everything it asked for would contradict
 * itself.
 *
 * The note is asked for rather than required. The API takes none, and the contract says why:
 * it is stored against every finding this moves, as the disposition's reason and each
 * outcome's detail, which is worth a sentence — but a developer who has read the draft and
 * decided none of it is for this round has said enough by pressing the button.
 */
async function ticketRest() {
  const data = detail.data.value
  if (!data) return
  working.value = true
  const result = await desk.ticketRestAndApprove(data.draft.review_id, ticketNote.value)
  working.value = false
  if (said(result)) {
    ticketingRest.value = false
    ticketNote.value = ''
  }
}

/**
 * Whether there is anything left for that button to file.
 *
 * **Counted with `willPost`, not off `finding_dispositions.in_draft`.** The server refuses
 * unless a finding is *posted* — the disposition **and** an open round status — so a review
 * whose carried findings are all `in_draft` and `fixed` has a non-zero `in_draft` count and
 * nothing to file. Drawing the button there is exactly the "a button that is always there
 * is a button that is sometimes a refusal" case the comment beside it says it avoids.
 */
const openFindings = computed(
  () => (review.data.value?.findings ?? []).some(willPost),
)

function statusLine(data: DraftDetail): string {
  const draft = data.draft
  if (draft.status === 'approved') {
    return `Signed off. Will be posted as "${eventLabel(draft.event)}" — waiting for the `
      + 'session to post it.'
  }
  if (draft.status === 'sent_back') return 'Sent back to the session with a note.'
  if (draft.status === 'posted') return 'Posted. Layer 7 reads the author’s replies.'
  if (draft.status === 'drafting') return 'The session is still writing this.'
  return 'Nothing has been posted. This is what GitHub will show.'
}

/** How the screen words each event. At module scope, as `VERBS` and `LAYER_LABELS` are. */
const EVENTS: Record<ReviewEvent, string> = {
  request_changes: 'Request changes',
  comment: 'Comment',
  approve: 'Approve',
}

function eventLabel(value: ReviewEvent | null | undefined): string {
  return value ? EVENTS[value] : 'not chosen'
}

/**
 * "118 of 150", or what is true when one of the two was not measured.
 *
 * `figure` does the formatting — it is the same rule the Rules screen's tiles use, and a
 * second implementation of "x of y" is a second place for the two to disagree about a null.
 * The contract's rule is that a count the server cannot answer is null and never zero, so a
 * figure built by interpolating two possibly-null numbers would print "118 of null".
 */
function ofFigure(value: number | null | undefined, of: number | null | undefined): string {
  return figure(value, of) ?? 'Not measured'
}
</script>

<template>
  <DeskState
    :resource="review"
    what="this review"
  >
    <template #default="{ data: owner }">
      <ReviewStepPage
        :detail="owner"
        step="draft"
        :headline="heading"
        :sub="detail.data.value ? statusLine(detail.data.value) : null"
      >
        <!--
          No draft, said inside the frame. This is the whole reason step 6 can be a link
          unconditionally: the trail is above this sentence, so a reader who came here from
          it has every other step one click away.
        -->
        <DeskNotice
          v-if="detail.error.value?.code === 'not_found'"
          tone="blocked"
        >
          No session has written a draft for this review. Step 6 is where one does; until
          then there is nothing to read and nothing to sign off, and the findings at step 5
          are what the review has so far.
        </DeskNotice>

        <!--
          Any other refusal is a refusal, and `DeskState` is what says so in the server's
          own words. Nested here rather than wrapping the page, because the frame above has
          already drawn and taking it away to report a failed second read would be the dead
          end this change removed.
        -->
        <DeskState
          v-else
          :resource="detail"
          what="this draft"
        >
          <template #default="{ data }">
            <div class="draft">
              <p
                v-if="data.draft.sent_back_note"
                class="rd-note sent-back"
              >
                <strong>Your note:</strong> {{ data.draft.sent_back_note }}
              </p>

              <div class="body">
                <DeskCard
                  as="section"
                  pad="tight"
                  class="preview"
                >
                  <div class="tabs">
                    <div class="rd-label tabs-label">
                      Preview
                    </div>
                    <div
                      role="tablist"
                      aria-label="How to view the draft"
                      class="tablist"
                    >
                      <button
                        v-for="tab in views"
                        :key="tab.key"
                        type="button"
                        role="tab"
                        class="rd-btn rd-btn--quiet"
                        :aria-selected="view === tab.key"
                        @click="view = tab.key"
                      >
                        {{ tab.label }}
                      </button>
                    </div>
                  </div>

                  <div class="pane">
                    <MarkdownView
                      v-if="view === 'rendered' && data.draft.body.length > 0"
                      :source="data.draft.body"
                    />

                    <!--
                A draft the session has opened and not written into. The body may be empty
                while the status is `drafting`, which the contract says in as many words, so
                the screen says what that is rather than drawing an empty page.
              -->
                    <p
                      v-else-if="view === 'rendered'"
                      class="rd-empty"
                    >
                      This draft has no body yet. Layer 6 is where a session writes the review, and
                      it has written nothing — there is nothing here to sign off.
                    </p>

                    <pre
                      v-else-if="view === 'markdown'"
                      class="source"
                    >{{ data.draft.body }}</pre>

                    <div
                      v-else
                      class="editor"
                    >
                      <label
                        class="rd-visually-hidden"
                        for="draft-body"
                      >The draft body in markdown</label>
                      <textarea
                        id="draft-body"
                        v-model="editing"
                        class="rd-field"
                        rows="22"
                        spellcheck="false"
                      />
                      <div class="editor-actions">
                        <button
                          type="button"
                          class="rd-btn rd-btn--primary"
                          :disabled="!dirty || working"
                          @click="save"
                        >
                          {{ working ? 'Saving…' : 'Save the draft' }}
                        </button>
                        <button
                          type="button"
                          class="rd-btn"
                          :disabled="!dirty || working"
                          @click="editing = loadedBody"
                        >
                          Discard changes
                        </button>
                        <p class="rd-meta editor-help">
                          An edit is recorded against the finding it changes. Three of the same kind
                          become a proposed rule.
                        </p>
                      </div>
                    </div>
                  </div>
                </DeskCard>

                <aside class="aside">
                  <DeskCard class="panel">
                    <div class="rd-label">
                      Checks
                    </div>

                    <!--
                `lint` is null until the review-craft lint has run over this body, and a
                body that was just saved has no result yet. Null is not a failing lint, so
                the screen says which it is rather than drawing "Fails".
              -->
                    <template v-if="data.lint">
                      <FactRow
                        label="Visible words"
                        :value="ofFigure(data.lint.visible_words, data.lint.word_budget)"
                        numeric
                        :failing="
                          data.lint.visible_words != null
                            && data.lint.word_budget != null
                            && data.lint.visible_words > data.lint.word_budget
                        "
                      />
                      <FactRow
                        label="Findings in the body"
                        :value="ofFigure(data.lint.findings_in_body, data.lint.findings_expected)"
                        numeric
                        :failing="
                          data.lint.findings_in_body != null
                            && data.lint.findings_expected != null
                            && data.lint.findings_in_body !== data.lint.findings_expected
                        "
                      />
                      <FactRow
                        label="Review lint"
                        :value="data.lint.passes ? 'Passes' : 'Fails'"
                        :failing="!data.lint.passes"
                      />
                      <FactRow
                        label="Raised before on this PR"
                        :value="
                          data.lint.raised_before_on_pr == null
                            ? 'Not measured'
                            : data.lint.raised_before_on_pr === 0
                              ? 'None'
                              : String(data.lint.raised_before_on_pr)
                        "
                        :failing="(data.lint.raised_before_on_pr ?? 0) > 0"
                      />
                      <p
                        v-for="line in data.lint.errors ?? []"
                        :key="line"
                        class="rd-note lint-error"
                      >
                        {{ line }}
                      </p>
                      <p class="rd-meta checked">
                        Checked {{ sinceLabel(data.lint.checked_at) }}
                      </p>
                    </template>
                    <p
                      v-else
                      class="rd-empty"
                    >
                      The review lint has not run over this body. That is not the same as it
                      failing: nothing has measured it yet.
                    </p>
                  </DeskCard>

                  <DeskCard class="panel">
                    <div class="rd-label">
                      Your edits to this draft
                    </div>
                    <p
                      v-if="data.edits.length === 0"
                      class="rd-meta help"
                    >
                      You have not changed this draft.
                    </p>
                    <!--
                      **One line a finding, not one a toggle.** Every edit was its own line
                      and the verb was printed twice — "Kept F1 · kept", four times over on
                      the review DEV-928 was raised from. The record keeps every edit and
                      still does; `editLines` is what changed, and it reports where each
                      finding ended up and how many times it moved to get there.
                    -->
                    <p
                      v-for="line in editRows"
                      :key="line.key"
                      class="edit"
                    >
                      <span class="edit-verb">{{ line.verb }}</span>
                      {{ line.key }}<!--
                        **The separators are in the strings, not in the stylesheet.** A
                        newline between two tags in a Vue template is condensed away, so a
                        margin would move the boxes apart and leave the text nodes touching —
                        which is what a screen reader reads and what a copy gives. The
                        Solution page learned this on a finding's round status.
                      --><span v-if="line.detail"> · {{ line.detail }}</span><span
                        v-if="line.changed"
                        class="rd-meta edit-times"
                      > · {{ line.changed }}</span>
                    </p>
                    <p class="rd-meta foot">
                      Saved as feedback when you sign off. Three edits of the same kind become a
                      proposed rule.
                    </p>
                  </DeskCard>

                  <DeskCard
                    v-if="data.held_back.length"
                    class="panel"
                  >
                    <!--
                      **The rows, and the join that makes them possible.** `HeldBackFinding`
                      carries a key, a claim and a severity; the title, the file and the line
                      a row needs come from the review's own findings, which this page is
                      already reading for the frame and the trail. Nothing about the record
                      or the contract changed to allow it.
                    -->
                    <HeldBackRows :rows="heldRows" />
                    <button
                      type="button"
                      class="rd-btn"
                      :disabled="data.nit_ticket.status !== 'none' || working"
                      @click="fileTicket"
                    >
                      {{
                        data.nit_ticket.status === 'none'
                          ? 'File as one ticket for an agent'
                          : data.nit_ticket.status === 'requested'
                            ? 'Queued as one ticket'
                            : `Filed as ${data.nit_ticket.ticket_id}`
                      }}
                    </button>
                  </DeskCard>

                  <DeskCard class="panel">
                    <label
                      class="rd-label"
                      for="event"
                    >Post as</label>
                    <select
                      id="event"
                      v-model="event"
                      class="rd-field"
                      :disabled="decided(data)"
                    >
                      <option value="request_changes">
                        Request changes
                      </option>
                      <option value="comment">
                        Comment
                      </option>
                      <option value="approve">
                        Approve
                      </option>
                    </select>
                    <button
                      type="button"
                      class="rd-btn rd-btn--primary"
                      :disabled="decided(data) || dirty || working"
                      @click="approve"
                    >
                      {{ working ? 'Working…' : 'Sign off and post' }}
                    </button>
                    <!--
                **"Ticket what is left, and sign off" — one act, and a sign-off.**

                Drawn only while there is something left to file. The API refuses a review
                with no open finding, saying to sign off instead and ask for a ticket for
                the nits on their own — so a button that is always there is a button that
                is sometimes a refusal, and the two things it would do are already the two
                buttons above it.
              -->
                    <template v-if="openFindings && !ticketingRest">
                      <button
                        type="button"
                        class="rd-btn"
                        :disabled="decided(data) || dirty || working"
                        @click="ticketingRest = true"
                      >
                        Ticket what is left, and sign off
                      </button>
                    </template>
                    <div
                      v-else-if="ticketingRest"
                      class="send-back"
                    >
                      <label
                        class="rd-meta"
                        for="ticket-rest-note"
                      >Why is the rest going to a ticket? Optional.</label>
                      <textarea
                        id="ticket-rest-note"
                        v-model="ticketNote"
                        class="rd-field"
                        rows="2"
                      />
                      <p class="rd-meta">
                        Every open finding is held back and recorded as ticketed this round,
                        and the draft is signed off. The event is not yours to choose here —
                        the control above is set to Approve, because a request for changes
                        that ticketed everything it asked for would contradict itself.
                        Review Desk files no ticket; an agent does, and the sign-off cannot
                        link to one that does not exist yet.
                      </p>
                      <div class="send-back-actions">
                        <button
                          type="button"
                          class="rd-btn rd-btn--primary"
                          :disabled="working"
                          @click="ticketRest"
                        >
                          Ticket it and sign off
                        </button>
                        <button
                          type="button"
                          class="rd-btn"
                          @click="ticketingRest = false"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>

                    <template v-if="!sendingBack">
                      <button
                        type="button"
                        class="rd-btn"
                        :disabled="decided(data) || working"
                        @click="sendingBack = true"
                      >
                        Send back with a note
                      </button>
                    </template>
                    <div
                      v-else
                      class="send-back"
                    >
                      <label
                        class="rd-meta"
                        for="send-back-note"
                      >What is the session to change?</label>
                      <textarea
                        id="send-back-note"
                        v-model="note"
                        class="rd-field"
                        rows="3"
                      />
                      <div class="send-back-actions">
                        <button
                          type="button"
                          class="rd-btn rd-btn--primary"
                          :disabled="working"
                          @click="sendBack"
                        >
                          Send back
                        </button>
                        <button
                          type="button"
                          class="rd-btn"
                          @click="sendingBack = false"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                    <DeskNotice
                      v-if="dirty"
                      tone="problem"
                    >
                      Save or discard your changes before signing off.
                    </DeskNotice>
                    <DeskNotice
                      v-if="notice"
                      :tone="notice.tone"
                    >
                      {{ notice.text }}
                    </DeskNotice>
                    <p class="rd-meta foot">
                      The review session posts it under your name, with no bot banner.
                    </p>
                  </DeskCard>
                </aside>
              </div>
            </div>
          </template>
        </DeskState>
      </ReviewStepPage>
    </template>
  </DeskState>
</template>

<style scoped>
.draft {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.back {
  font-size: 14px;
}

.title {
  margin: 6px 0 0;
}

.status {
  margin: 4px 0 0;
}

.status--decided {
  color: var(--rd-accent-dark);
  font-weight: 500;
}

.sent-back {
  margin: 8px 0 0;
  max-width: var(--rd-measure);
}

.sent-back strong {
  font-weight: 600;
}

.body {
  display: flex;
  flex-wrap: wrap;
  gap: 24px;
  align-items: flex-start;
}

.preview {
  flex: 999 1 560px;
  min-width: 0;
  padding: 0;
}

.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 8px 12px 8px 20px;
  border-bottom: 1px solid var(--rd-line);
}

.tabs-label {
  margin-right: auto;
}

.tablist {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.pane {
  padding: 24px 20px;
}

.source {
  margin: 0;
  padding: 16px;
  overflow-x: auto;
  background: var(--rd-ground);
  border: 1px solid var(--rd-line);
  border-radius: var(--rd-radius-sm);
  font-family: var(--rd-font-mono);
  font-size: 13px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

.editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.editor-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.editor-help {
  flex: 1 1 240px;
  margin: 0;
}

.aside {
  flex: 1 1 320px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.edit {
  margin: 0;
  font-size: 14px;
}

.edit-verb {
  font-weight: 500;
}

/*
 * How many times a finding moved, beside where it ended up. Quiet and never wrapping inside
 * itself — it is a phrase, not a sentence — and in the body ink, because the number of times
 * somebody changed their mind is not a fault.
 */
.edit-times {
  white-space: nowrap;
}

.help,
.foot,
.checked {
  margin: 0;
}

.foot,
.checked {
  padding-top: 8px;
  border-top: 1px solid var(--rd-rail);
}

.send-back {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.send-back-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.missing {
  padding: 20px;
}
</style>
