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
 *  YOUR CALL                                       The session put this in the review.
 *  ┌──────────────────────────────────────────────────────────────┐
 *  │ Post P │ Ticket T │ Set aside S │ Regrade G │ Edit wording E │
 *  └──────────────────────────────────────────────────────────────┘
 *  REGRADE TO
 *  [ Blocker ] [ Should ] [ Nit ] [ Question ]
 * ```
 *
 * ## The buttons are the three lists, and each key is its own first letter
 *
 * Patrick, 16 October 2026, on the keys this had: *he has no muscle memory for them.* So
 * **Post**, **Ticket** and **Set aside** are the three lists the code review page sorts
 * findings into, worded the same way, with P, T and S; Regrade keeps G; Rewrite became
 * **Edit wording**, which is what it does, and keeps E. A developer who can read the
 * button can type the key.
 *
 * ## Eight decisions behind eleven buttons, and one button over two decisions
 *
 * Which buttons are drawn is a fact about the finding, not a preference:
 *
 * | The finding | What it is offered |
 * |---|---|
 * | raised this round, or carried and still open | Post, Set aside, Regrade, Edit wording |
 * | ticketed or set aside this round | the same four; Post raises it again |
 * | carried and settled — fixed, accepted, withdrawn | Raise it again, Accept their reason, Withdraw |
 * | any of them | Ticket |
 *
 * **Post is `keep` on a finding this round still holds open, and `raise_again` on one it
 * has settled.** The store refuses `keep` outright when the round status is not open —
 * `decision.rs`: "moving its round status to still open on a click labelled Keep would put
 * a decision in the record the developer did not make" — and `raise_again` is the decision
 * that says it is still there. That refusal is right, and it is a fact about the record
 * rather than about the screen: a developer who has just moved a finding to a ticket and
 * wants it back is not learning which of two words means *post it* depending on what they
 * pressed a moment ago.
 *
 * So this is the mirror of what it replaces. The old version was **one decision under two
 * labels** — "Keep" in the draft, "Post it" on a held-back nit — which existed because
 * "Keep" beside "The session held this back for a ticket" reads as *leave it held back*.
 * With the button named for the list it moves the finding to, that ambiguity is gone, and
 * what is left is **one label over two decisions**, chosen by the record's own rule.
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

/**
 * Whether a decision can be made at all, and whether this one in particular can.
 *
 * `post` resolves first, because what it costs is what the decision behind it costs —
 * neither `keep` nor `raise_again` needs a block, so the answer is the same for both, and
 * resolving rather than special-casing is what keeps it the same if that ever changes.
 */
function available(decision: string): boolean {
  if (refusal.value != null) return false
  const chosen = decision === 'post' ? post.value : (decision as FindingDecision)
  return !(NEEDS_A_BLOCK.includes(chosen) && block.value == null)
}

/**
 * What `post` means on this finding: the decision the store will take for it.
 *
 * The one rule, in one place, read by the button and by the key alike. Argued at the top of
 * this file: `keep` is refused outright on a finding whose round status is not open, and
 * `raise_again` is what says it is still there.
 */
const post = computed<FindingDecision>(() =>
  isStillOpen(props.finding.round_status) ? 'keep' : 'raise_again',
)

/**
 * The buttons, in the order the board draws them, with their keys.
 *
 * Keys work from anywhere on the page and not while the developer is typing, which
 * `DecisionTrack` owns — and they are only attached while a finding is showing, because
 * this component is only mounted then.
 *
 * **Every decision has one**, which was step 1.2's own ask. Patrick, 15 October 2026, on
 * why he accepts and posts almost everything: "there's too much going on" — and the four
 * decisions that read as the default were the four with keys, while moving a finding to a
 * ticket or answering an author took a reach for the pointer. A decision that costs more
 * than its neighbour is not weighed against it; it is skipped.
 *
 * **And each key is the first letter of its own button**, which is the 16 October change:
 * a key nobody can derive from what they are reading is a key nobody presses.
 */
const DECISIONS: Array<{ value: string, label: string, key?: string }> = [
  { value: 'post', label: 'Post', key: 'P' },
  { value: 'cut', label: 'Set aside', key: 'S' },
  { value: 'regrade', label: 'Regrade', key: 'G' },
  { value: 'rewrite', label: 'Edit wording', key: 'E' },
]

/**
 * What the author's reply left to answer, on a finding an earlier round settled.
 *
 * **Offered on a carried finding and on nothing else.** These three are about what somebody
 * *else* did — the author fixed it, gave a reason, or the finding was withdrawn — and a
 * finding this round ticketed or set aside has had none of that happen to it. It has had a
 * decision taken on it by the developer, and the way back from a decision is the decision
 * that undoes it, which is Post.
 *
 * **R was the way back from a ticket until 16 October.** It no longer is: P is, and it
 * sends the same `raise_again` the store takes either way. R stays here for the finding
 * this panel was written for — the one carried in as fixed, where "raise it again" is a
 * claim about the author's work rather than about the developer's own filing.
 */
const REPLIES: Array<{ value: string, label: string, key?: string }> = [
  { value: 'raise_again', label: 'Raise it again', key: 'R' },
  { value: 'accept_reason', label: 'Accept their reason', key: 'A' },
  { value: 'withdraw', label: 'Withdraw', key: 'W' },
]

/**
 * Which set this finding is offered, and the ticket that is on both.
 *
 * **Carried and settled** is the test, where it used to be *settled* alone. A finding the
 * developer ticketed a moment ago is settled by that reading — `to_ticket` writes
 * `Ticketed` — and was then offered the author's three replies, which is a panel asking
 * what the author made of a decision the author has never seen. It is offered the four
 * decisions, with Post as the way back.
 */
const options = computed<TrackOption[]>(() => {
  const answering = isCarried(props.finding) && !open.value
  const all: Array<{ value: string, label: string, key?: string }> = [
    ...(answering ? REPLIES : DECISIONS),
    { value: 'to_ticket', label: 'Ticket', key: 'T' },
  ]
  return all
    .filter(one => available(one.value))
    .map(one => ({ value: one.value, label: one.label, key: one.key }))
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
  const chosen = decision === 'post' ? post.value : (decision as FindingDecision)
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
  /*
   * Trimmed on both sides of the comparison. `paragraph` is trimmed and the block's text is
   * not, so a block the session wrote with a trailing space differed from a textarea nobody
   * touched — and a rewrite of the title alone then sent a paragraph with it, set
   * `rewritten`, and had the outcome say the developer wrote words they never opened.
   */
  const was = (block.value?.text ?? '').trim()
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
          working-label="Editing"
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
