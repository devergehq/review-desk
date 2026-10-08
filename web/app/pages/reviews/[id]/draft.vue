<script setup lang="ts">
import type { DraftDetail, ReviewEvent } from '~/types/desk'

/**
 * The draft preview: what GitHub will show, before anything is posted.
 *
 * Nothing on this screen posts. The developer approves, and the review session posts it
 * under their name with no bot banner — a preview that could post would be a bot.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.draft(reviewId)

useHead({
  title: computed(() =>
    detail.data.value ? `#${detail.data.value.review.pr} draft · Review Desk` : 'Review Desk',
  ),
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
const notice = ref<{ text: string, tone: 'plain' | 'problem' | 'blocked' } | null>(null)

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
     */
    if (editing.value === '' || editing.value === loadedBody.value) {
      editing.value = value.draft.body
    }
    loadedBody.value = value.draft.body
    if (value.draft.event) event.value = value.draft.event
  },
  { immediate: true },
)

const dirty = computed(() => editing.value !== loadedBody.value)

function decided(data: DraftDetail): boolean {
  return data.draft.status !== 'ready'
}

function said(result: { ok: true, message: string } | { ok: false, error: DeskError }) {
  notice.value = result.ok
    ? { text: result.message, tone: 'plain' }
    : { text: refusal(result.error), tone: refusalTone(result.error) }
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

function statusLine(data: DraftDetail): string {
  const draft = data.draft
  if (draft.status === 'approved') {
    return `Approved as "${eventLabel(draft.event)}". Waiting for the session to post it.`
  }
  if (draft.status === 'sent_back') return 'Sent back to the session with a note.'
  if (draft.status === 'posted') return 'Posted. Layer 7 reads the author’s replies.'
  if (draft.status === 'drafting') return 'The session is still writing this.'
  return 'Nothing has been posted. This is what GitHub will show.'
}

function eventLabel(value: ReviewEvent | null | undefined): string {
  const labels: Record<ReviewEvent, string> = {
    request_changes: 'Request changes',
    comment: 'Comment',
    approve: 'Approve',
  }
  return value ? labels[value] : 'not chosen'
}

/**
 * "118 of 150", or what is true when one of the two was not measured.
 *
 * The contract's rule is that a count the server cannot answer is null and never zero, so
 * a figure built by interpolating two possibly-null numbers would print "118 of null". The
 * lint object itself is nullable for the same reason, which is the fourth drift DEV-862
 * found: `DraftLint` requires `passes` and `checked_at`, so there is no shape of it that
 * means "not measured" — and this screen used to assume it was always there.
 */
function ofFigure(value: number | null | undefined, of: number | null | undefined): string {
  if (value == null) return 'Not measured'
  return of == null ? String(value) : `${value} of ${of}`
}
</script>

<template>
  <DeskState
    :resource="detail"
    what="this draft"
  >
    <template #default="{ data }">
      <div class="draft">
        <div>
          <NuxtLink
            :to="`/reviews/${data.draft.review_id}`"
            class="back"
          >
            Back to the review
          </NuxtLink>
          <h1 class="title">
            <PrRef :pr="data.review.pr" /> Draft review
          </h1>
          <p
            class="rd-meta status"
            :class="{ 'status--decided': decided(data) }"
          >
            {{ statusLine(data) }}
          </p>
          <p
            v-if="data.draft.sent_back_note"
            class="rd-note sent-back"
          >
            <strong>Your note:</strong> {{ data.draft.sent_back_note }}
          </p>
        </div>

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
                it has written nothing — there is nothing here to approve.
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
              <p
                v-for="edit in data.edits"
                :key="`${edit.finding_key}-${edit.kind}`"
                class="edit"
              >
                <span class="edit-verb">{{ editVerb(edit.kind) }}</span>
                {{ edit.finding_key }} · {{ edit.summary }}
              </p>
              <p class="rd-meta foot">
                Saved as feedback when you approve. Three edits of the same kind become a
                proposed rule.
              </p>
            </DeskCard>

            <DeskCard
              v-if="data.held_back.length"
              class="panel"
            >
              <div class="rd-label">
                Held back ·
                {{
                  data.held_back.every(item => item.severity === 'nit')
                    ? count(data.held_back.length, 'nit')
                    : count(data.held_back.length, 'finding')
                }}
              </div>
              <p
                v-for="item in data.held_back"
                :key="item.finding_key"
                class="rd-note held"
              >
                {{ item.finding_key }} · {{ item.claim }}
              </p>
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
                {{ working ? 'Working…' : 'Approve and post' }}
              </button>
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
                Save or discard your changes before approving.
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

.held {
  margin: 0;
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
