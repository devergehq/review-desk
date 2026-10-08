<script setup lang="ts">
import type { ReviewEvent } from '~/types/desk'

/**
 * The draft preview: what GitHub will show, before anything is posted.
 *
 * Nothing on this screen posts. The developer approves, and the review session posts it
 * under their name with no bot banner — a preview that could post would be a bot.
 */
const route = useRoute()
const desk = useReviewDesk()

const reviewId = computed(() => Number(route.params.id))
const detail = desk.draft(reviewId.value)

useHead({
  title: computed(() =>
    detail.value ? `#${detail.value.review.pr} draft · Review Desk` : 'Review Desk',
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
const notice = ref<{ text: string, ok: boolean } | null>(null)

watch(
  detail,
  (value) => {
    if (value && editing.value === '') editing.value = value.draft.body
    if (value?.draft.event) event.value = value.draft.event
  },
  { immediate: true },
)

const dirty = computed(() => detail.value != null && editing.value !== detail.value.draft.body)

const decided = computed(
  () => detail.value != null && detail.value.draft.status !== 'ready',
)

function save() {
  if (!detail.value) return
  const edits = desk.saveDraft(detail.value.draft.review_id, editing.value)
  notice.value = {
    text:
      edits.length > 0
        ? `Saved. ${count(edits.length, 'edit')} recorded for the harvester.`
        : 'Saved. Nothing changed that the harvester records.',
    ok: true,
  }
  view.value = 'rendered'
}

function approve() {
  if (!detail.value) return
  desk.approveDraft(detail.value.draft.review_id, event.value)
  notice.value = {
    text: 'Approved. The review session posts it; Review Desk does not.',
    ok: true,
  }
}

function sendBack() {
  if (!detail.value) return
  const result = desk.sendBackDraft(detail.value.draft.review_id, note.value)
  notice.value = { text: result.message, ok: result.ok }
  if (result.ok) {
    sendingBack.value = false
    note.value = ''
  }
}

function fileTicket() {
  if (!detail.value) return
  notice.value = { text: desk.requestNitTicket(detail.value.draft.review_id), ok: true }
}

const statusLine = computed(() => {
  const draft = detail.value?.draft
  if (!draft) return ''
  if (draft.status === 'approved') {
    return `Approved as "${eventLabel(draft.event)}". Waiting for the session to post it.`
  }
  if (draft.status === 'sent_back') return 'Sent back to the session with a note.'
  if (draft.status === 'posted') return 'Posted. Layer 7 reads the author’s replies.'
  if (draft.status === 'drafting') return 'The session is still writing this.'
  return 'Nothing has been posted. This is what GitHub will show.'
})

function eventLabel(value: ReviewEvent | null | undefined): string {
  const labels: Record<ReviewEvent, string> = {
    request_changes: 'Request changes',
    comment: 'Comment',
    approve: 'Approve',
  }
  return value ? labels[value] : 'not chosen'
}
</script>

<template>
  <div
    v-if="detail"
    class="draft"
  >
    <div>
      <NuxtLink
        :to="`/reviews/${detail.draft.review_id}`"
        class="back"
      >
        Back to the review
      </NuxtLink>
      <h1 class="title">
        <PrRef :pr="detail.review.pr" /> Draft review
      </h1>
      <p
        class="rd-meta status"
        :class="{ 'status--decided': decided }"
      >
        {{ statusLine }}
      </p>
      <p
        v-if="detail.draft.sent_back_note"
        class="rd-note sent-back"
      >
        <strong>Your note:</strong> {{ detail.draft.sent_back_note }}
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
            v-if="view === 'rendered'"
            :source="detail.draft.body"
          />

          <pre
            v-else-if="view === 'markdown'"
            class="source"
          >{{ detail.draft.body }}</pre>

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
                :disabled="!dirty"
                @click="save"
              >
                Save the draft
              </button>
              <button
                type="button"
                class="rd-btn"
                :disabled="!dirty"
                @click="editing = detail.draft.body"
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
          <FactRow
            v-if="detail.lint.visible_words != null"
            label="Visible words"
            :value="`${detail.lint.visible_words} of ${detail.lint.word_budget}`"
            numeric
            :failing="
              detail.lint.word_budget != null
                && detail.lint.visible_words > detail.lint.word_budget
            "
          />
          <FactRow
            v-if="detail.lint.findings_in_body != null"
            label="Findings in the body"
            :value="`${detail.lint.findings_in_body} of ${detail.lint.findings_expected}`"
            numeric
            :failing="detail.lint.findings_in_body !== detail.lint.findings_expected"
          />
          <FactRow
            label="Review lint"
            :value="detail.lint.passes ? 'Passes' : 'Fails'"
            :failing="!detail.lint.passes"
          />
          <FactRow
            label="Raised before on this PR"
            :value="
              detail.lint.raised_before_on_pr == null
                ? 'Not measured'
                : detail.lint.raised_before_on_pr === 0
                  ? 'None'
                  : String(detail.lint.raised_before_on_pr)
            "
            :failing="(detail.lint.raised_before_on_pr ?? 0) > 0"
          />
          <p class="rd-meta checked">
            Checked {{ sinceLabel(detail.lint.checked_at) }}
          </p>
        </DeskCard>

        <DeskCard class="panel">
          <div class="rd-label">
            Your edits to this draft
          </div>
          <p
            v-if="detail.edits.length === 0"
            class="rd-meta help"
          >
            You have not changed this draft.
          </p>
          <p
            v-for="edit in detail.edits"
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
          v-if="detail.held_back.length"
          class="panel"
        >
          <div class="rd-label">
            Held back ·
            {{
              detail.held_back.every(item => item.severity === 'nit')
                ? count(detail.held_back.length, 'nit')
                : count(detail.held_back.length, 'finding')
            }}
          </div>
          <p
            v-for="item in detail.held_back"
            :key="item.finding_key"
            class="rd-note held"
          >
            {{ item.finding_key }} · {{ item.claim }}
          </p>
          <button
            type="button"
            class="rd-btn"
            :disabled="detail.nit_ticket.status !== 'none'"
            @click="fileTicket"
          >
            {{
              detail.nit_ticket.status === 'none'
                ? 'File as one ticket for an agent'
                : detail.nit_ticket.status === 'requested'
                  ? 'Queued as one ticket'
                  : `Filed as ${detail.nit_ticket.ticket_id}`
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
            :disabled="decided"
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
            :disabled="decided || dirty"
            @click="approve"
          >
            Approve and post
          </button>
          <template v-if="!sendingBack">
            <button
              type="button"
              class="rd-btn"
              :disabled="decided"
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
            :tone="notice.ok ? 'plain' : 'problem'"
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

  <DeskCard
    v-else
    class="missing"
  >
    <h1>No draft yet</h1>
    <p class="rd-note">
      This review has not reached layer 6.
      <NuxtLink to="/">Back to the inbox</NuxtLink>.
    </p>
  </DeskCard>
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
