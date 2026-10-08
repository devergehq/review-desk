<script setup lang="ts">
import type { Disposition, Finding } from '~/types/desk'

/**
 * One finding, and the control that moves it between the draft, held back and
 * suppressed. A finding is never deleted: moving it records a reason, which is what lets
 * a later rule proposal cite it.
 */
const props = defineProps<{ finding: Finding, editable?: boolean }>()
const emit = defineEmits<{
  change: [payload: { seq: number, disposition: Disposition, reason: string }]
}>()

const open = ref(false)
const target = ref<Disposition>(props.finding.disposition)
const reason = ref(props.finding.disposition_reason ?? '')

/**
 * Whether a save has been asked for and not yet come back.
 *
 * The panel closes when the finding it is about changes, which is what a successful write
 * produces: the write re-reads the review, so this component is handed a new finding. A
 * *refused* write re-reads nothing, so the panel stays open with the reason still typed in
 * it — which is what DEV-862 asks for, and it needs no second copy of the rule that was
 * broken.
 */
const saving = ref(false)

watch(
  () => props.finding,
  () => {
    if (saving.value) {
      saving.value = false
      open.value = false
    }
  },
)

const options: Array<{ value: Disposition, label: string }> = [
  { value: 'in_draft', label: 'In draft' },
  { value: 'held_back', label: 'Held back for a ticket' },
  { value: 'suppressed', label: 'Suppressed' },
]

const where = computed(() => {
  const f = props.finding
  if (!f.file) return null
  return f.line != null ? `${f.file}:${f.line}` : f.file
})

const rest = computed(() => {
  const f = props.finding
  return [
    `lens: ${f.lens}`,
    f.evidence ? `evidence: ${f.evidence}` : null,
    f.rule_key ? `cites ${f.rule_key}` : null,
  ]
    .filter(Boolean)
    .join(' · ')
})

function start() {
  target.value = props.finding.disposition
  reason.value = props.finding.disposition_reason ?? ''
  open.value = true
}

/**
 * Ask for the move, and let the record refuse it.
 *
 * **There is no copy of the "say why" rule here**, and there was. The server requires a
 * reason for `held_back` and `suppressed` — a finding removed from the draft without one
 * teaches the harvester nothing — and this component used to check the same thing first,
 * so the real refusal was never reached from the screen and the two wordings could drift.
 * One rule, in the record, with the screen showing what it said.
 */
function save() {
  saving.value = true
  emit('change', {
    seq: props.finding.seq,
    disposition: target.value,
    reason: target.value === 'in_draft' ? '' : reason.value,
  })
}
</script>

<template>
  <li
    class="finding"
    :class="{ 'finding--quiet': finding.disposition === 'suppressed' }"
  >
    <div class="line">
      <SeverityTag :severity="finding.severity" />
      <div class="body">
        <div class="claim">
          {{ finding.key }} · {{ finding.claim }}
        </div>
        <div class="rd-meta where">
          <span
            v-if="where"
            class="rd-mono"
          >{{ where }}</span>
          <span v-if="where"> · </span>{{ rest }}
        </div>
        <p
          v-if="finding.disposition_reason"
          class="rd-meta reason"
        >
          {{ finding.disposition_reason }}
        </p>
      </div>
      <div class="state">
        <DispositionTag :disposition="finding.disposition" />
        <button
          v-if="editable && !open"
          type="button"
          class="change"
          @click="start"
        >
          Change<span class="rd-visually-hidden"> the disposition of {{ finding.key }}</span>
        </button>
      </div>
    </div>

    <div
      v-if="open"
      class="editor"
    >
      <div class="field">
        <label
          class="rd-meta"
          :for="`disposition-${finding.seq}`"
        >Where {{ finding.key }} goes</label>
        <select
          :id="`disposition-${finding.seq}`"
          v-model="target"
          class="rd-field"
        >
          <option
            v-for="option in options"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
      </div>
      <div
        v-if="target !== 'in_draft'"
        class="field field--wide"
      >
        <label
          class="rd-meta"
          :for="`reason-${finding.seq}`"
        >Why</label>
        <input
          :id="`reason-${finding.seq}`"
          v-model="reason"
          class="rd-field"
          type="text"
          @keyup.enter="save"
        >
      </div>
      <div class="editor-actions">
        <button
          type="button"
          class="rd-btn rd-btn--primary"
          @click="save"
        >
          Save
        </button>
        <button
          type="button"
          class="rd-btn"
          @click="open = false"
        >
          Cancel
        </button>
      </div>
    </div>
  </li>
</template>

<style scoped>
.finding {
  padding: 12px 0;
  border-top: 1px solid var(--rd-rail);
}

.line {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: flex-start;
}

.body {
  flex: 1 1 380px;
  min-width: 0;
}

.claim {
  font-weight: 500;
}

.where {
  margin-top: 2px;
}

.reason {
  margin: 2px 0 0;
}

.finding--quiet .claim {
  color: var(--rd-ink-muted);
}

.state {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.change {
  /* A secondary control, but still a control: the same 44px as every other one. */
  min-height: var(--rd-control-min);
  padding: 0 12px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-sm);
  background: var(--rd-surface);
  color: var(--rd-ink-dim);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.change:hover {
  background: var(--rd-rail);
  color: var(--rd-ink);
}

.editor {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: flex-end;
  margin-top: 10px;
  padding: 12px;
  background: var(--rd-ground);
  border-radius: var(--rd-radius-sm);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1 1 200px;
}

.field--wide {
  flex: 2 1 280px;
}

.editor-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
