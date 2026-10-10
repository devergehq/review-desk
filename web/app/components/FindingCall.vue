<script setup lang="ts">
import type {
  DecideFindingRequest,
  DraftSection,
  DraftStatus,
  Finding,
  FindingDecision,
  Severity,
} from '~/types/desk'

import type { TrackOption } from './DecisionTrack.vue'

/**
 * "Your call" — every decision the record has, on one finding.
 *
 * ```
 *  YOUR CALL
 *  ┌──────────────────────────────────────────────┐
 *  │ Keep  │  Cut  │  Regrade  │  Rewrite        │   The session put this in the review.
 *  └──────────────────────────────────────────────┘
 *  REGRADE TO
 *  [ Blocker ] [ Should ] [ Nit ] [ Question ]
 * ```
 *
 * **Eight decisions behind eleven buttons**, which is the contract's own collapsing: "Keep"
 * on a finding in the draft and "Post it" on a held-back nit are both `keep`, because both
 * mean *this is posted*. Which buttons are drawn is a fact about the finding, not a
 * preference:
 *
 * | The finding | What it is offered |
 * |---|---|
 * | raised this round, or carried and still open | Keep, Cut, Regrade, Rewrite |
 * | carried and settled — fixed, accepted, withdrawn, ticketed | Raise it again, Accept their reason, Withdraw |
 * | either | To a ticket |
 *
 * **A settled carried finding is not offered Keep**, and that is not tidiness. The store
 * refuses it: "Keep" means post this as it stands, and a finding carried as fixed is not
 * posted as an open finding — so moving its round status on a click labelled Keep would put
 * a decision in the record the developer did not make. Raising it again is the decision that
 * says it is still there, and it is one of the three buttons drawn instead.
 *
 * ## Three decisions need words, and the words are asked for before the write
 *
 * A regrade needs a severity, a rewrite needs a title or a paragraph, and accepting the
 * author's reason needs the reason. The server refuses each by name — and a refusal the
 * screen could have avoided is a round trip the developer paid for, so the control opens the
 * field instead of sending and reporting back.
 *
 * ## What is not offered, and why it says so
 *
 * An approved or posted draft is what the review session posts under the developer's name,
 * so nothing can be decided about it. A draft written as one block of markdown holds no
 * block for any finding, so the three decisions that change words have nothing to change.
 * Both are drawn as a sentence rather than as buttons that do not work.
 */
const props = defineProps<{
  finding: Finding
  /** The draft's blocks, or null when there is no draft at all. */
  sections: DraftSection[] | null
  draftStatus: DraftStatus | null
  /** A write is in flight; nothing here may be pressed twice. */
  working: boolean
}>()

const emit = defineEmits<{ decide: [request: DecideFindingRequest] }>()

/** The block this draft holds for this finding, if any. */
const block = computed(() =>
  (props.sections ?? []).find(
    section => section.kind === 'finding' && section.finding_seq === props.finding.seq,
  ) ?? null,
)

/** Whether this finding still stands as an open finding of this round. */
const open = computed(() => isStillOpen(props.finding.round_status))

/**
 * Why nothing can be decided, or null when something can.
 *
 * Ordered by what a developer can do about it. An approved draft is a state of the review
 * and the only way back is to send it back; a one-block draft is a property of how the
 * session wrote it and nothing on this page changes it; no draft at all is a session that
 * has not got there yet.
 */
const refusal = computed<string | null>(() => {
  if (props.draftStatus === 'approved') {
    return (
      'You have signed this draft off, so nothing more can be decided about it — it is '
      + 'what the review session posts under your name. Send it back if it needs changing.'
    )
  }
  if (props.draftStatus === 'posted') {
    return 'This review is posted. What the author makes of it is step 7.'
  }
  if (props.sections == null) {
    return (
      'There is no draft yet, so a decision would have nothing to change. Step 6 is where '
      + 'a session writes the review.'
    )
  }
  return null
})

/**
 * Which decisions change words that have to exist first, from `decision.rs`'s own table.
 *
 * `keep` and `raise_again` are not here: they write a block for a finding the draft has
 * none of, which is the designs' "Post it" on a held-back nit.
 */
const NEEDS_A_BLOCK: FindingDecision[] = ['cut', 'regrade', 'rewrite']

/** Whether a decision can be made at all, and whether this one in particular can. */
function available(decision: FindingDecision): boolean {
  if (refusal.value != null) return false
  return !(NEEDS_A_BLOCK.includes(decision) && block.value == null)
}

/**
 * The buttons, in the order the Controls board draws them, with its keys.
 *
 * The keys are the board's: Y, X, G, E. They work from anywhere on the page and not while
 * the developer is typing, which `DecisionTrack` owns — and they are only attached while a
 * finding is open, because this component is only mounted then.
 */
const DECISIONS: Array<{ value: FindingDecision, label: string, key?: string }> = [
  { value: 'keep', label: 'Keep', key: 'Y' },
  { value: 'cut', label: 'Cut', key: 'X' },
  { value: 'regrade', label: 'Regrade', key: 'G' },
  { value: 'rewrite', label: 'Rewrite', key: 'E' },
]

/**
 * `keep` on a finding the draft holds no block for says **"Post it"**, not "Keep".
 *
 * The contract's own table is `Keep · Post it`: one decision, two labels, because the same
 * act reads differently depending on where the finding stands. On a held-back nit the panel
 * says "The session held this back for a ticket" and a button beside it reading "Keep" is
 * read as *leave it held back* — while pressing it writes the finding a block and posts it.
 * That is the opposite of what the developer is told they are doing.
 */
function labelFor(decision: FindingDecision, label: string): string {
  if (decision !== 'keep' || block.value != null) return label
  return 'Post it'
}

const REPLIES: Array<{ value: FindingDecision, label: string, key?: string }> = [
  { value: 'raise_again', label: 'Raise it again' },
  { value: 'accept_reason', label: 'Accept their reason' },
  { value: 'withdraw', label: 'Withdraw' },
]

const options = computed<TrackOption[]>(() => {
  const offered = open.value ? DECISIONS : REPLIES
  const all: Array<{ value: FindingDecision, label: string, key?: string }> = [
    ...offered,
    { value: 'to_ticket', label: 'To a ticket' },
  ]
  return all
    .filter(one => available(one.value))
    .map(one => ({
      value: one.value as string,
      label: labelFor(one.value, one.label),
      key: one.key,
    }))
})

/* ------------------------- The three that need words ------------------------- */

type Asking = 'regrade' | 'rewrite' | 'accept_reason' | null

const asking = ref<Asking>(null)
const severity = ref<Severity>(props.finding.severity)
const title = ref('')
const text = ref('')
const note = ref('')
const problem = ref('')

/*
 * Asked again from scratch for each finding. One row is open at a time, so this component is
 * remounted — but a developer who opens F9, starts a rewrite, folds it and opens F9 again
 * should not find half a sentence from before.
 */
watch(
  () => props.finding.seq,
  () => {
    asking.value = null
    severity.value = props.finding.severity
    title.value = ''
    text.value = ''
    note.value = ''
    problem.value = ''
  },
)

const SEVERITIES: Severity[] = ['blocker', 'should', 'nit', 'question']

function pick(decision: string) {
  problem.value = ''
  const chosen = decision as FindingDecision
  if (chosen === 'regrade' || chosen === 'rewrite' || chosen === 'accept_reason') {
    asking.value = chosen
    /* A rewrite opens on the words that are there, which is what rewriting means. */
    if (chosen === 'rewrite') {
      title.value = block.value?.title ?? props.finding.title ?? props.finding.claim
      text.value = block.value?.text ?? ''
    }
    return
  }
  emit('decide', { decision: chosen })
}

function regrade() {
  if (severity.value === props.finding.severity) {
    problem.value = `${props.finding.key} is already a ${severity.value}.`
    return
  }
  emit('decide', { decision: 'regrade', severity: severity.value })
  asking.value = null
}

function rewrite() {
  const heading = title.value.trim()
  const paragraph = text.value.trim()
  if (heading.length === 0) {
    problem.value = 'The heading cannot be blank: it is what gets posted.'
    return
  }
  /*
   * The server refuses a paragraph that begins or ends with a newline, because blocks are
   * joined by one blank line and a block bringing its own would compose a body nobody
   * wrote. Trimmed here rather than refused: a trailing newline in a textarea is a thing
   * the developer did not mean to type.
   *
   * **`null` means "leave the paragraph alone", and an emptied box does not mean that.**
   * `rewrite_finding_block` is `COALESCE(?4, text)`, so sending null for an empty textarea
   * kept the session's paragraph while the notice said the finding would be posted in the
   * developer's words and the outcome's detail said only the heading changed. Sent whenever
   * it differs from what the block holds — including when what it differs *to* is nothing,
   * which is a heading on its own and a shape the contract allows.
   */
  const was = block.value?.text ?? ''
  emit('decide', {
    decision: 'rewrite',
    title: heading,
    text: paragraph === was ? null : paragraph,
  })
  asking.value = null
}

function acceptReason() {
  if (note.value.trim().length === 0) {
    problem.value = 'Say what the author’s reason was. It is what the record keeps.'
    return
  }
  emit('decide', { decision: 'accept_reason', note: note.value.trim() })
  asking.value = null
}

/**
 * What the session had decided, which stays visible beside the developer's.
 *
 * DEV-878's criterion: "The session's own choice stays visible beside the developer's." The
 * record keeps one disposition, not two, so what this reads is the record as it stands — and
 * the moment the developer decides, the outcome recording who decided is what carries the
 * other half. The page shows both: this sentence, and the notice the write answers with.
 */
const sessionSaid = computed(() => {
  if (props.finding.disposition === 'in_draft') return 'The session put this in the review.'
  if (props.finding.disposition === 'held_back') {
    return 'The session held this back for a ticket.'
  }
  return 'The session set this aside.'
})
</script>

<template>
  <div class="call">
    <div class="head">
      <span class="rd-rubric">Your call</span>
      <span class="rd-note said">{{ sessionSaid }}</span>
    </div>

    <DeskNotice
      v-if="refusal"
      tone="blocked"
    >
      {{ refusal }}
    </DeskNotice>

    <!--
      A draft written as one block of markdown. Said once, here, rather than on each of the
      three buttons it stops: a sentence per button would be the same sentence three times.
    -->
    <DeskNotice
      v-else-if="block == null && (sections ?? []).every(section => section.kind !== 'finding')"
      tone="blocked"
    >
      This draft was written as one block of markdown rather than in parts, so it holds no
      block for {{ finding.key }} — cutting, regrading and rewriting have nothing to change.
      Keeping it writes it a block, and moving it to a ticket takes it out of the review.
    </DeskNotice>

    <DecisionTrack
      v-if="options.length > 0"
      mode="act"
      :options="options"
      :label="`How to decide on ${finding.key}`"
      :disabled="working"
      @act="pick"
    />

    <div
      v-if="asking === 'regrade'"
      class="asking"
    >
      <span
        id="regrade-to"
        class="rd-label"
      >Regrade to</span>
      <div
        class="severities"
        role="radiogroup"
        aria-labelledby="regrade-to"
      >
        <button
          v-for="one in SEVERITIES"
          :key="one"
          type="button"
          role="radio"
          class="severity"
          :class="{ 'severity--picked': severity === one }"
          :aria-checked="severity === one"
          @click="severity = one"
        >
          <SeverityTag :severity="one" />
        </button>
      </div>
      <div class="actions">
        <DeskButton
          kind="primary"
          :working="working"
          working-label="Regrading"
          @click="regrade"
        >
          Regrade {{ finding.key }}
        </DeskButton>
        <DeskButton @click="asking = null">
          Cancel
        </DeskButton>
      </div>
    </div>

    <div
      v-else-if="asking === 'rewrite'"
      class="asking"
    >
      <label
        class="rd-label"
        for="rewrite-title"
      >The heading, as it will post</label>
      <input
        id="rewrite-title"
        v-model="title"
        class="rd-field"
        type="text"
      >
      <label
        class="rd-label"
        for="rewrite-text"
      >The paragraph under it</label>
      <textarea
        id="rewrite-text"
        v-model="text"
        class="rd-field"
        rows="4"
      />
      <p class="rd-note aside">
        The session’s own wording is kept beside yours, and the record says the words posted
        were yours.
      </p>
      <div class="actions">
        <DeskButton
          kind="primary"
          :working="working"
          working-label="Rewriting"
          @click="rewrite"
        >
          Post it in my words
        </DeskButton>
        <DeskButton @click="asking = null">
          Cancel
        </DeskButton>
      </div>
    </div>

    <div
      v-else-if="asking === 'accept_reason'"
      class="asking"
    >
      <label
        class="rd-label"
        for="accept-note"
      >The author’s reason, in their words</label>
      <textarea
        id="accept-note"
        v-model="note"
        class="rd-field"
        rows="3"
      />
      <p class="rd-note aside">
        Recorded as a rejection with a reason you accepted, which is what
        “author disagreed once, with a reason you accepted” counts.
      </p>
      <div class="actions">
        <DeskButton
          kind="primary"
          :working="working"
          working-label="Recording"
          @click="acceptReason"
        >
          Accept it
        </DeskButton>
        <DeskButton @click="asking = null">
          Cancel
        </DeskButton>
      </div>
    </div>

    <DeskNotice
      v-if="problem"
      tone="problem"
    >
      {{ problem }}
    </DeskNotice>
  </div>
</template>

<style scoped>
.call {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-top: 16px;
  border-top: 1px solid var(--rd-line);
  min-width: 0;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 16px;
  align-items: baseline;
}

.said {
  margin: 0;
}

.asking {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 16px;
  border-radius: var(--rd-radius-control);
  background: var(--rd-ground);
  max-width: var(--rd-measure);
}

.severities {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

/*
 * The tag is the label. The button around it draws the chosen state, so the four severities
 * are told apart by the same shapes they are told apart by everywhere else on this page.
 */
.severity {
  display: flex;
  min-height: var(--rd-control-min);
  align-items: center;
  padding: 0 10px;
  border: 1px solid var(--rd-key-line);
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
  font: inherit;
  cursor: pointer;
}

.severity--picked {
  border-color: var(--rd-ink);
  box-shadow: var(--rd-lift-chosen-quiet);
}

.severity:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 4px;
}

.aside {
  margin: 0;
  max-width: var(--rd-measure-recorded);
}
</style>
