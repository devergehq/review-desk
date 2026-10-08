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
const problem = ref('')

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
  problem.value = ''
  open.value = true
}

function save() {
  if (target.value !== 'in_draft' && reason.value.trim().length === 0) {
    problem.value = 'Say why it is not going in the draft.'
    return
  }
  emit('change', {
    seq: props.finding.seq,
    disposition: target.value,
    reason: target.value === 'in_draft' ? '' : reason.value,
  })
  open.value = false
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
      <DeskNotice
        v-if="problem"
        tone="problem"
      >
        {{ problem }}
      </DeskNotice>
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
  min-height: 32px;
  padding: 0 10px;
  border: 1px solid var(--rd-control-line);
  border-radius: var(--rd-radius-xs);
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
